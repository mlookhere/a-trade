use std::collections::HashMap;

use crate::{
    AuthorizationInputs, Condition, OrderProposal, ProductionConditions, RejectionCode,
    trade_authorized,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerCapabilities {
    /// §64 protected parent STOP_LIMIT + hard stop + target as one broker-supported structure.
    pub protected_stop_limit_bracket: Condition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerReconciliation {
    pub broker_connected: Condition,
    pub broker_state_known: Condition,
    pub position_state_known: Condition,
    pub open_order_state_known: Condition,
    /// Reconciled broker-side SETUP_ID tags for currently open orders.
    pub open_order_setup_ids: Vec<String>,
    /// Reconciled broker-side SETUP_ID tags for current positions.
    pub position_setup_ids: Vec<String>,
}

impl BrokerReconciliation {
    #[must_use]
    pub fn broker_safe(&self) -> Condition {
        all_conditions(&[
            self.broker_connected,
            self.broker_state_known,
            self.position_state_known,
            self.open_order_state_known,
        ])
    }

    #[must_use]
    pub fn setup_clear(&self, setup_id: &str) -> Condition {
        if self.position_state_known != Condition::True
            || self.open_order_state_known != Condition::True
        {
            return Condition::Unknown;
        }
        if setup_id.trim().is_empty() {
            return Condition::Unknown;
        }

        Condition::from(
            !self
                .open_order_setup_ids
                .iter()
                .chain(self.position_setup_ids.iter())
                .any(|candidate| candidate == setup_id),
        )
    }
}

/// Provider-neutral adapter only. Broker-specific retry, partial-fill, replace, and routing
/// semantics are intentionally absent because canonical knowledge does not define them.
pub trait BrokerAdapter {
    type Error;

    fn capabilities(&self) -> BrokerCapabilities;

    fn reconcile(&mut self) -> Result<BrokerReconciliation, Self::Error>;

    /// Submit the complete §64 protected structure exactly as represented by the approved
    /// proposal. Implementations must not modify strategy prices, side, size, or order type.
    fn submit_protected(
        &mut self,
        proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error>;

    /// TRUE means the entry is confirmed filled; FALSE means accepted/unfilled; UNKNOWN is unsafe.
    fn entry_filled(&mut self, setup_id: &str) -> Result<Condition, Self::Error>;

    /// After any confirmed fill, canonical §64 requires the hard stop to be confirmed.
    fn hard_stop_confirmed(&mut self, setup_id: &str) -> Result<Condition, Self::Error>;

    /// Emergency §64 flatten path. No retry policy is implied by this single attempt contract.
    fn flatten_instrument(&mut self, instrument: &str) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerSubmission {
    pub broker_order_id: String,
    /// TRUE only when the broker response already confirms the entry filled.
    pub entry_filled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending { broker_order_id: String },
    FilledProtected { broker_order_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRegistryStatus {
    pub owner_agent_id: String,
    pub consumed: bool,
    pub order_active_or_reserved: bool,
    pub broker_order_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetupRecord {
    owner_agent_id: String,
    consumed: bool,
    order_active_or_reserved: bool,
    instrument: Option<String>,
    broker_order_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct SetupRegistry {
    setups: HashMap<String, SetupRecord>,
}

impl SetupRegistry {
    /// §103 registration is expected to be called by the Setup Coordinator when SETUP_ID is
    /// created. Duplicate registration is rejected instead of silently changing ownership.
    pub fn register_setup(
        &mut self,
        setup_id: &str,
        owner_agent_id: &str,
    ) -> Result<(), RejectionCode> {
        if setup_id.trim().is_empty() || owner_agent_id.trim().is_empty() {
            return Err(RejectionCode::ProcessError);
        }
        if self.setups.contains_key(setup_id) {
            return Err(RejectionCode::DuplicateSetup);
        }

        self.setups.insert(
            setup_id.to_owned(),
            SetupRecord {
                owner_agent_id: owner_agent_id.to_owned(),
                consumed: false,
                order_active_or_reserved: false,
                instrument: None,
                broker_order_id: None,
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn status(&self, setup_id: &str) -> Option<SetupRegistryStatus> {
        self.setups.get(setup_id).map(|record| SetupRegistryStatus {
            owner_agent_id: record.owner_agent_id.clone(),
            consumed: record.consumed,
            order_active_or_reserved: record.order_active_or_reserved,
            broker_order_id: record.broker_order_id.clone(),
        })
    }

    fn reserve_for_submission(
        &mut self,
        setup_id: &str,
        owner_agent_id: &str,
        instrument: &str,
    ) -> Result<(), RejectionCode> {
        let Some(record) = self.setups.get_mut(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.owner_agent_id != owner_agent_id {
            return Err(RejectionCode::DuplicateSetup);
        }
        if record.consumed || record.order_active_or_reserved {
            return Err(RejectionCode::DuplicateSetup);
        }
        if instrument.trim().is_empty() {
            return Err(RejectionCode::ProcessError);
        }

        record.order_active_or_reserved = true;
        record.instrument = Some(instrument.to_owned());
        Ok(())
    }

    fn clear_reservation(&mut self, setup_id: &str) {
        if let Some(record) = self.setups.get_mut(setup_id) {
            if !record.consumed {
                record.order_active_or_reserved = false;
                record.instrument = None;
                record.broker_order_id = None;
            }
        }
    }

    fn record_pending_order(
        &mut self,
        setup_id: &str,
        broker_order_id: &str,
    ) -> Result<(), RejectionCode> {
        let Some(record) = self.setups.get_mut(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if !record.order_active_or_reserved || record.consumed || broker_order_id.trim().is_empty()
        {
            return Err(RejectionCode::ProcessError);
        }
        record.broker_order_id = Some(broker_order_id.to_owned());
        Ok(())
    }

    /// §8: consumption happens when and only when the entry is confirmed filled.
    fn mark_filled_consumed(&mut self, setup_id: &str) -> Result<(), RejectionCode> {
        let Some(record) = self.setups.get_mut(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.consumed {
            return Err(RejectionCode::DuplicateSetup);
        }
        if !record.order_active_or_reserved {
            return Err(RejectionCode::ProcessError);
        }

        record.consumed = true;
        record.order_active_or_reserved = false;
        Ok(())
    }

    fn execution_record(&self, setup_id: &str) -> Option<&SetupRecord> {
        self.setups.get(setup_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreExecutionAuthority {
    pub llm_setup_pass: Condition,
    pub strategy_validator_pass: Condition,
    pub risk_engine_pass: Condition,
    pub portfolio_coordinator_pass: Condition,
}

#[derive(Debug, Clone, Copy)]
pub struct ExecutionGateContext<'a> {
    pub submitting_agent_id: &'a str,
    /// Full §122 strategy conditions from deterministic upstream components. Critical execution
    /// fields are independently overwritten by this gate before final authorization.
    pub production_conditions: ProductionConditions,
    pub authority: PreExecutionAuthority,
}

/// Opaque approval token. Fields are private and there is no public constructor, so the
/// Execution Engine can only receive one from the deterministic gate in this module.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionPermit {
    proposal: OrderProposal,
}

impl ExecutionPermit {
    #[must_use]
    pub fn proposal(&self) -> &OrderProposal {
        &self.proposal
    }
}

pub struct ExecutionCoordinator<A: BrokerAdapter> {
    adapter: A,
    registry: SetupRegistry,
    engine_safe: bool,
}

impl<A: BrokerAdapter> ExecutionCoordinator<A> {
    #[must_use]
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            registry: SetupRegistry::default(),
            engine_safe: true,
        }
    }

    pub fn register_setup(
        &mut self,
        setup_id: &str,
        owner_agent_id: &str,
    ) -> Result<(), RejectionCode> {
        self.registry.register_setup(setup_id, owner_agent_id)
    }

    #[must_use]
    pub const fn execution_engine_safe(&self) -> Condition {
        if self.engine_safe {
            Condition::True
        } else {
            Condition::False
        }
    }

    #[must_use]
    pub const fn adapter(&self) -> &A {
        &self.adapter
    }

    #[must_use]
    pub fn adapter_mut(&mut self) -> &mut A {
        &mut self.adapter
    }

    #[must_use]
    pub fn setup_status(&self, setup_id: &str) -> Option<SetupRegistryStatus> {
        self.registry.status(setup_id)
    }

    /// §§9,16,64,103,109,110,122 deterministic Execution Gate. It independently reconciles
    /// broker state, ownership, duplicates, protected-bracket capability, and final authority.
    pub fn gate(
        &mut self,
        proposal: &OrderProposal,
        context: ExecutionGateContext<'_>,
    ) -> Result<ExecutionPermit, RejectionCode> {
        if !self.engine_safe {
            return Err(RejectionCode::BrokerUnsafe);
        }
        if context.submitting_agent_id.trim().is_empty() {
            return Err(RejectionCode::ProcessError);
        }
        if proposal.execution_pass != Condition::Unknown {
            return Err(RejectionCode::ProcessError);
        }
        if proposal.strategy_pass != Condition::True {
            return Err(RejectionCode::ProcessError);
        }
        if proposal.risk_pass != Condition::True {
            return Err(RejectionCode::RiskRejected);
        }
        if proposal.portfolio_pass != Condition::True {
            return Err(RejectionCode::PortfolioRiskRejected);
        }
        if context.production_conditions.data_valid != Condition::True {
            return Err(RejectionCode::DataInvalid);
        }
        if context.authority.risk_engine_pass != Condition::True {
            return Err(RejectionCode::RiskRejected);
        }
        if context.authority.portfolio_coordinator_pass != Condition::True {
            return Err(RejectionCode::PortfolioRiskRejected);
        }
        if context.authority.llm_setup_pass != Condition::True
            || context.authority.strategy_validator_pass != Condition::True
        {
            return Err(RejectionCode::ProcessError);
        }

        let capabilities = self.adapter.capabilities();
        if capabilities.protected_stop_limit_bracket != Condition::True {
            return Err(RejectionCode::BrokerUnsafe);
        }

        let reconciliation = match self.adapter.reconcile() {
            Ok(value) => value,
            Err(_) => return Err(RejectionCode::BrokerUnsafe),
        };
        if reconciliation.broker_safe() != Condition::True {
            return Err(RejectionCode::BrokerUnsafe);
        }
        if reconciliation.setup_clear(&proposal.setup_id) != Condition::True {
            return Err(RejectionCode::DuplicateSetup);
        }

        let Some(record) = self.registry.execution_record(&proposal.setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.owner_agent_id != context.submitting_agent_id
            || record.consumed
            || record.order_active_or_reserved
        {
            return Err(RejectionCode::DuplicateSetup);
        }

        let mut conditions = context.production_conditions;
        conditions.setup_not_duplicated = Condition::True;
        conditions.no_conflicting_order = Condition::True;
        conditions.broker_safe = Condition::True;
        conditions.execution_engine_safe = Condition::True;
        let authority = AuthorizationInputs {
            llm_setup_pass: context.authority.llm_setup_pass,
            strategy_validator_pass: context.authority.strategy_validator_pass,
            risk_engine_pass: context.authority.risk_engine_pass,
            portfolio_coordinator_pass: context.authority.portfolio_coordinator_pass,
            execution_gate_pass: Condition::True,
        };
        if !trade_authorized(conditions, authority) {
            return Err(RejectionCode::ProcessError);
        }

        self.registry.reserve_for_submission(
            &proposal.setup_id,
            context.submitting_agent_id,
            &proposal.instrument,
        )?;

        let mut approved = proposal.clone();
        approved.execution_pass = Condition::True;
        Ok(ExecutionPermit { proposal: approved })
    }

    /// Submit only an opaque gate-approved token. A fresh broker reconciliation is performed
    /// immediately before transmission to close the gate/submit race as far as provider-neutral
    /// semantics allow.
    pub fn submit(&mut self, permit: ExecutionPermit) -> Result<ExecutionStatus, RejectionCode> {
        if !self.engine_safe {
            return Err(RejectionCode::BrokerUnsafe);
        }

        let setup_id = permit.proposal.setup_id.clone();
        if !self.reservation_valid(&permit.proposal) {
            return Err(RejectionCode::ProcessError);
        }

        let reconciliation = match self.adapter.reconcile() {
            Ok(value) => value,
            Err(_) => {
                self.registry.clear_reservation(&setup_id);
                return Err(RejectionCode::BrokerUnsafe);
            }
        };
        if reconciliation.broker_safe() != Condition::True {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::BrokerUnsafe);
        }
        if reconciliation.setup_clear(&setup_id) != Condition::True {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::DuplicateSetup);
        }
        if self.adapter.capabilities().protected_stop_limit_bracket != Condition::True {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::BrokerUnsafe);
        }

        let submission = match self.adapter.submit_protected(&permit.proposal) {
            Ok(value) => value,
            Err(_) => {
                self.engine_safe = false;
                return Err(RejectionCode::BrokerUnsafe);
            }
        };
        if submission.broker_order_id.trim().is_empty() {
            self.engine_safe = false;
            return Err(RejectionCode::BrokerUnsafe);
        }

        self.registry
            .record_pending_order(&setup_id, &submission.broker_order_id)?;

        if !submission.entry_filled {
            return Ok(ExecutionStatus::Pending {
                broker_order_id: submission.broker_order_id,
            });
        }

        self.registry.mark_filled_consumed(&setup_id)?;
        self.verify_protection_after_fill(&setup_id, &submission.broker_order_id)
    }

    pub fn reconcile_fill(&mut self, setup_id: &str) -> Result<ExecutionStatus, RejectionCode> {
        if !self.engine_safe {
            return Err(RejectionCode::BrokerUnsafe);
        }
        let Some(record) = self.registry.execution_record(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.consumed {
            return Err(RejectionCode::DuplicateSetup);
        }
        if !record.order_active_or_reserved {
            return Err(RejectionCode::ProcessError);
        }
        let Some(broker_order_id) = record.broker_order_id.clone() else {
            return Err(RejectionCode::ProcessError);
        };

        let filled = match self.adapter.entry_filled(setup_id) {
            Ok(value) => value,
            Err(_) => {
                self.engine_safe = false;
                return Err(RejectionCode::BrokerUnsafe);
            }
        };
        match filled {
            Condition::False => Ok(ExecutionStatus::Pending { broker_order_id }),
            Condition::Unknown => {
                self.engine_safe = false;
                Err(RejectionCode::BrokerUnsafe)
            }
            Condition::True => {
                self.registry.mark_filled_consumed(setup_id)?;
                self.verify_protection_after_fill(setup_id, &broker_order_id)
            }
        }
    }

    fn reservation_valid(&self, proposal: &OrderProposal) -> bool {
        self.registry
            .execution_record(&proposal.setup_id)
            .is_some_and(|record| {
                record.order_active_or_reserved
                    && !record.consumed
                    && record.broker_order_id.is_none()
                    && record.instrument.as_deref() == Some(proposal.instrument.as_str())
            })
    }

    fn verify_protection_after_fill(
        &mut self,
        setup_id: &str,
        broker_order_id: &str,
    ) -> Result<ExecutionStatus, RejectionCode> {
        let stop_confirmed = self.adapter.hard_stop_confirmed(setup_id);
        if matches!(stop_confirmed, Ok(Condition::True)) {
            return Ok(ExecutionStatus::FilledProtected {
                broker_order_id: broker_order_id.to_owned(),
            });
        }

        self.engine_safe = false;
        let instrument = self
            .registry
            .execution_record(setup_id)
            .and_then(|record| record.instrument.clone())
            .ok_or(RejectionCode::ProcessError)?;
        let _flatten_result = self.adapter.flatten_instrument(&instrument);
        Err(RejectionCode::BrokerUnsafe)
    }
}

fn all_conditions(conditions: &[Condition]) -> Condition {
    if conditions.contains(&Condition::False) {
        Condition::False
    } else if conditions.contains(&Condition::Unknown) {
        Condition::Unknown
    } else {
        Condition::True
    }
}
