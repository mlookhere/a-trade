use std::collections::HashMap;

use crate::{
    AuthorizationInputs, Condition, OperationalSafetyController, OrderProposal,
    ProductionConditions, RejectionCode, trade_authorized,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerCapabilities {
    pub protected_stop_limit_bracket: Condition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerReconciliation {
    pub broker_connected: Condition,
    pub broker_state_known: Condition,
    pub position_state_known: Condition,
    pub open_order_state_known: Condition,
    pub open_order_setup_ids: Vec<String>,
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

pub trait BrokerAdapter {
    type Error;

    fn capabilities(&self) -> BrokerCapabilities;
    fn reconcile(&mut self) -> Result<BrokerReconciliation, Self::Error>;
    fn submit_protected(
        &mut self,
        proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error>;
    fn entry_filled(&mut self, setup_id: &str) -> Result<Condition, Self::Error>;
    fn hard_stop_confirmed(&mut self, setup_id: &str) -> Result<Condition, Self::Error>;
    fn flatten_instrument(&mut self, instrument: &str) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerSubmission {
    pub broker_order_id: String,
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
        let Some(record) = self.setups.get_mut(setup_id) else {
            return;
        };
        if record.consumed {
            return;
        }
        record.order_active_or_reserved = false;
        record.instrument = None;
        record.broker_order_id = None;
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
    pub production_conditions: ProductionConditions,
    pub authority: PreExecutionAuthority,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionPermit {
    proposal: OrderProposal,
    owner_agent_id: String,
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
    operational_safety: OperationalSafetyController,
}

impl<A: BrokerAdapter> ExecutionCoordinator<A> {
    #[must_use]
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            registry: SetupRegistry::default(),
            engine_safe: true,
            operational_safety: OperationalSafetyController::new(),
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
    pub const fn operational_safety(&self) -> &OperationalSafetyController {
        &self.operational_safety
    }

    #[must_use]
    pub fn operational_safety_mut(&mut self) -> &mut OperationalSafetyController {
        &mut self.operational_safety
    }

    #[must_use]
    pub fn setup_status(&self, setup_id: &str) -> Option<SetupRegistryStatus> {
        self.registry.status(setup_id)
    }

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
        if self
            .operational_safety
            .agent_automation_allowed(context.submitting_agent_id)
            != Condition::True
        {
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
        if self.adapter.capabilities().protected_stop_limit_bracket != Condition::True {
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

        // Broker reconciliation proves the exact SETUP_ID is clear, but it does not prove that
        // upstream strategy/portfolio checks found no effective duplicate or conflicting order.
        // Preserve those independent fail-closed facts rather than overwriting them.
        let mut conditions = context.production_conditions;
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
        Ok(ExecutionPermit {
            proposal: approved,
            owner_agent_id: context.submitting_agent_id.to_owned(),
        })
    }

    pub fn submit(&mut self, permit: ExecutionPermit) -> Result<ExecutionStatus, RejectionCode> {
        if !self.engine_safe {
            return Err(RejectionCode::BrokerUnsafe);
        }
        let setup_id = permit.proposal.setup_id.clone();
        if self
            .operational_safety
            .agent_automation_allowed(&permit.owner_agent_id)
            != Condition::True
        {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::ProcessError);
        }
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
        if matches!(
            self.adapter.hard_stop_confirmed(setup_id),
            Ok(Condition::True)
        ) {
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
        let _ = self.adapter.flatten_instrument(&instrument);
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
