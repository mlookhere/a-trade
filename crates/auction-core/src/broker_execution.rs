use std::collections::HashMap;

use crate::{
    Condition, Direction, EtTime, InstrumentExecutionConfig, OperationalSafetyController,
    OrderProposal, RejectionCode, SessionPermissions, SetupState, TerminalState, slippage_guard,
    trigger_expired,
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
    /// Canonical §56 provider-neutral conflict fact. Adapter-specific conflict semantics remain
    /// outside the strategy core; FALSE or UNKNOWN fails closed at the live Execution Gate.
    pub no_conflicting_order: Condition,
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
            || setup_id.trim().is_empty()
        {
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

/// Provider-neutral §6 execution capability used only for canonical §§60-61 pending-entry
/// cancellation. Adapter-specific cancel transport, retries, and replace behavior stay outside
/// the strategy core.
pub trait BrokerOrderCancellation: BrokerAdapter {
    fn cancel_entry_order(
        &mut self,
        setup_id: &str,
        broker_order_id: &str,
    ) -> Result<Condition, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerSubmission {
    pub broker_order_id: String,
    pub entry_filled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending { broker_order_id: String },
    Missed { broker_order_id: String },
    FilledProtected { broker_order_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRegistryStatus {
    pub owner_agent_id: String,
    pub consumed: bool,
    pub order_active_or_reserved: bool,
    pub broker_order_id: Option<String>,
    pub state: Option<SetupState>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingEntryGuard {
    direction: Direction,
    trigger: f64,
    execution_config: InstrumentExecutionConfig,
}

#[derive(Debug, Clone, PartialEq)]
struct SetupRecord {
    owner_agent_id: String,
    consumed: bool,
    order_active_or_reserved: bool,
    instrument: Option<String>,
    broker_order_id: Option<String>,
    state: Option<SetupState>,
    pending_entry_guard: Option<PendingEntryGuard>,
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
                state: None,
                pending_entry_guard: None,
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
            state: record.state,
        })
    }

    /// Canonical §§102-103: a sealed strategy-validated setup reaches the live execution tail
    /// from FINAL_RECONFIRMATION. A successful gate binds the owner at ENTRY_AUTHORIZED. If a
    /// pre-transmission safety recheck clears the reservation, ENTRY_AUTHORIZED is retained rather
    /// than regressing state; the same owner may reserve it again only while no order is active.
    fn reserve_for_submission(
        &mut self,
        setup_id: &str,
        owner_agent_id: &str,
        proposal: &OrderProposal,
    ) -> Result<(), RejectionCode> {
        let Some(record) = self.setups.get_mut(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.owner_agent_id != owner_agent_id
            || record.consumed
            || record.order_active_or_reserved
        {
            return Err(RejectionCode::DuplicateSetup);
        }
        if proposal.instrument().trim().is_empty() || proposal.setup_id() != setup_id {
            return Err(RejectionCode::ProcessError);
        }

        match record.state {
            None => {
                record.state = Some(
                    SetupState::FinalReconfirmation
                        .advance(SetupState::EntryAuthorized)
                        .map_err(|_| RejectionCode::ProcessError)?,
                );
            }
            Some(SetupState::EntryAuthorized) => {}
            Some(_) => return Err(RejectionCode::DuplicateSetup),
        }

        record.order_active_or_reserved = true;
        record.instrument = Some(proposal.instrument().to_owned());
        record.pending_entry_guard = Some(PendingEntryGuard {
            direction: proposal.side(),
            trigger: proposal.entry_trigger(),
            execution_config: proposal.execution_config(),
        });
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
        record.pending_entry_guard = None;
    }

    fn record_pending_order(
        &mut self,
        setup_id: &str,
        broker_order_id: &str,
    ) -> Result<(), RejectionCode> {
        let Some(record) = self.setups.get_mut(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if !record.order_active_or_reserved
            || record.consumed
            || broker_order_id.trim().is_empty()
            || record.state != Some(SetupState::EntryAuthorized)
            || record.pending_entry_guard.is_none()
        {
            return Err(RejectionCode::ProcessError);
        }
        record.state = Some(
            SetupState::EntryAuthorized
                .advance(SetupState::OrderPending)
                .map_err(|_| RejectionCode::ProcessError)?,
        );
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
        if !record.order_active_or_reserved || record.state != Some(SetupState::OrderPending) {
            return Err(RejectionCode::ProcessError);
        }
        record.state = Some(
            SetupState::OrderPending
                .advance(SetupState::Filled)
                .map_err(|_| RejectionCode::ProcessError)?,
        );
        record.consumed = true;
        record.order_active_or_reserved = false;
        record.pending_entry_guard = None;
        Ok(())
    }

    fn mark_missed(&mut self, setup_id: &str) -> Result<(), RejectionCode> {
        let Some(record) = self.setups.get_mut(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.consumed
            || !record.order_active_or_reserved
            || record.broker_order_id.is_none()
            || record.state != Some(SetupState::OrderPending)
        {
            return Err(RejectionCode::ProcessError);
        }
        record.order_active_or_reserved = false;
        record.pending_entry_guard = None;
        record.state = Some(SetupState::OrderPending.terminate(TerminalState::Missed));
        Ok(())
    }

    fn mark_position_management(&mut self, setup_id: &str) -> Result<(), RejectionCode> {
        let Some(record) = self.setups.get_mut(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if !record.consumed
            || record.order_active_or_reserved
            || record.broker_order_id.is_none()
            || record.state != Some(SetupState::Filled)
        {
            return Err(RejectionCode::ProcessError);
        }
        record.state = Some(
            SetupState::Filled
                .advance(SetupState::PositionManagement)
                .map_err(|_| RejectionCode::ProcessError)?,
        );
        Ok(())
    }

    fn execution_record(&self, setup_id: &str) -> Option<&SetupRecord> {
        self.setups.get(setup_id)
    }
}

/// Live §§3-4/109/122 execution context. Current ET time is mandatory and is recalculated at the
/// live gate rather than trusting the timestamp embedded in an earlier strategy proof.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionGateContext<'a> {
    pub submitting_agent_id: &'a str,
    pub llm_setup_pass: Condition,
    pub current_time_et: EtTime,
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

    /// Canonical §§3-4,6,56,64,102-103,109,117,122 live Execution Gate.
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
        if !SessionPermissions::at(context.current_time_et).allow_new_entries {
            return Err(RejectionCode::TimeCutoff);
        }
        if context.llm_setup_pass != Condition::True {
            return Err(RejectionCode::ProcessError);
        }
        if self
            .operational_safety
            .agent_automation_allowed(context.submitting_agent_id)
            != Condition::True
        {
            return Err(RejectionCode::ProcessError);
        }
        if proposal.execution_pass() != Condition::Unknown
            || proposal.strategy_pass() != Condition::True
        {
            return Err(RejectionCode::ProcessError);
        }
        if proposal.risk_pass() != Condition::True {
            return Err(RejectionCode::RiskRejected);
        }
        if proposal.portfolio_pass() != Condition::True {
            return Err(RejectionCode::PortfolioRiskRejected);
        }
        if proposal.owner_agent_id() != context.submitting_agent_id {
            return Err(RejectionCode::DuplicateSetup);
        }
        if self.adapter.capabilities().protected_stop_limit_bracket != Condition::True {
            return Err(RejectionCode::BrokerUnsafe);
        }

        let reconciliation = self
            .adapter
            .reconcile()
            .map_err(|_| RejectionCode::BrokerUnsafe)?;
        if reconciliation.broker_safe() != Condition::True {
            return Err(RejectionCode::BrokerUnsafe);
        }
        if reconciliation.no_conflicting_order != Condition::True {
            return Err(RejectionCode::ProcessError);
        }
        if reconciliation.setup_clear(proposal.setup_id()) != Condition::True {
            return Err(RejectionCode::DuplicateSetup);
        }

        let Some(record) = self.registry.execution_record(proposal.setup_id()) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.owner_agent_id != context.submitting_agent_id
            || record.owner_agent_id != proposal.owner_agent_id()
            || record.consumed
            || record.order_active_or_reserved
        {
            return Err(RejectionCode::DuplicateSetup);
        }

        self.registry.reserve_for_submission(
            proposal.setup_id(),
            context.submitting_agent_id,
            proposal,
        )?;
        let mut approved = proposal.clone();
        approved.mark_execution_pass();
        Ok(ExecutionPermit {
            proposal: approved,
            owner_agent_id: context.submitting_agent_id.to_owned(),
        })
    }

    /// Canonical §§3-4/109/117 second time-authority check at the broker transmission boundary.
    /// A permit created before the cutoff cannot be transmitted after the cutoff. When time fails,
    /// only the untransmitted reservation is cleared; ENTRY_AUTHORIZED is retained.
    pub fn submit(
        &mut self,
        permit: ExecutionPermit,
        current_time_et: EtTime,
    ) -> Result<ExecutionStatus, RejectionCode> {
        if !self.engine_safe {
            return Err(RejectionCode::BrokerUnsafe);
        }
        let setup_id = permit.proposal.setup_id().to_owned();
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
        if reconciliation.no_conflicting_order != Condition::True {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::ProcessError);
        }
        if reconciliation.setup_clear(&setup_id) != Condition::True {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::DuplicateSetup);
        }
        if self.adapter.capabilities().protected_stop_limit_bracket != Condition::True {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::BrokerUnsafe);
        }

        // Keep this check immediately adjacent to the only broker-transmission call in the core.
        if !SessionPermissions::at(current_time_et).allow_new_entries {
            self.registry.clear_reservation(&setup_id);
            return Err(RejectionCode::TimeCutoff);
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

    fn reconcile_fill_only(&mut self, setup_id: &str) -> Result<ExecutionStatus, RejectionCode> {
        if !self.engine_safe {
            return Err(RejectionCode::BrokerUnsafe);
        }
        let Some(record) = self.registry.execution_record(setup_id) else {
            return Err(RejectionCode::ProcessError);
        };
        if record.consumed {
            return Err(RejectionCode::DuplicateSetup);
        }
        if !record.order_active_or_reserved || record.state != Some(SetupState::OrderPending) {
            return Err(RejectionCode::ProcessError);
        }
        let Some(broker_order_id) = record.broker_order_id.clone() else {
            return Err(RejectionCode::ProcessError);
        };

        match self.adapter.entry_filled(setup_id) {
            Ok(Condition::False) => Ok(ExecutionStatus::Pending { broker_order_id }),
            Ok(Condition::Unknown) | Err(_) => {
                self.engine_safe = false;
                Err(RejectionCode::BrokerUnsafe)
            }
            Ok(Condition::True) => {
                self.registry.mark_filled_consumed(setup_id)?;
                self.verify_protection_after_fill(setup_id, &broker_order_id)
            }
        }
    }

    /// Canonical §§16,64,73,98,100,102,118 provider-neutral open-position safety cycle.
    /// This remains available after 11:00 because §4 preserves existing-position management.
    pub fn verify_open_position_safety(&mut self, setup_id: &str) -> Result<(), RejectionCode> {
        if !self.engine_safe {
            return Err(RejectionCode::BrokerUnsafe);
        }
        if setup_id.trim().is_empty() {
            return Err(RejectionCode::ProcessError);
        }

        let instrument = {
            let Some(record) = self.registry.execution_record(setup_id) else {
                return Err(RejectionCode::ProcessError);
            };
            if !record.consumed
                || record.order_active_or_reserved
                || record.broker_order_id.is_none()
                || record.state != Some(SetupState::PositionManagement)
            {
                return Err(RejectionCode::ProcessError);
            }
            record
                .instrument
                .clone()
                .filter(|value| !value.trim().is_empty())
                .ok_or(RejectionCode::ProcessError)?
        };

        let reconciliation = self
            .adapter
            .reconcile()
            .map_err(|_| RejectionCode::BrokerUnsafe)?;
        if reconciliation.broker_safe() != Condition::True
            || !reconciliation
                .position_setup_ids
                .iter()
                .any(|candidate| candidate == setup_id)
        {
            return Err(RejectionCode::BrokerUnsafe);
        }

        if matches!(
            self.adapter.hard_stop_confirmed(setup_id),
            Ok(Condition::True)
        ) {
            return Ok(());
        }

        self.engine_safe = false;
        let _ = self.adapter.flatten_instrument(&instrument);
        Err(RejectionCode::BrokerUnsafe)
    }

    fn reservation_valid(&self, proposal: &OrderProposal) -> bool {
        let expected_guard = PendingEntryGuard {
            direction: proposal.side(),
            trigger: proposal.entry_trigger(),
            execution_config: proposal.execution_config(),
        };
        self.registry
            .execution_record(proposal.setup_id())
            .is_some_and(|record| {
                record.order_active_or_reserved
                    && !record.consumed
                    && record.broker_order_id.is_none()
                    && record.instrument.as_deref() == Some(proposal.instrument())
                    && record.state == Some(SetupState::EntryAuthorized)
                    && record.pending_entry_guard == Some(expected_guard)
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
            self.registry.mark_position_management(setup_id)?;
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

impl<A: BrokerOrderCancellation> ExecutionCoordinator<A> {
    /// Canonical §§60-61/102/117 pending-entry lifecycle. Orders already accepted before 11:00
    /// remain governed only by the existing slippage/expiry contracts; no new cutoff-cancel rule
    /// is inferred here.
    pub fn reconcile_pending_entry(
        &mut self,
        setup_id: &str,
        current_price: f64,
        completed_5m_candles_since_reconfirmation: u32,
    ) -> Result<ExecutionStatus, RejectionCode> {
        let (broker_order_id, guard) = {
            let Some(record) = self.registry.execution_record(setup_id) else {
                return Err(RejectionCode::ProcessError);
            };
            if record.consumed
                || !record.order_active_or_reserved
                || record.state != Some(SetupState::OrderPending)
            {
                return Err(RejectionCode::ProcessError);
            }
            let Some(broker_order_id) = record.broker_order_id.clone() else {
                return Err(RejectionCode::ProcessError);
            };
            let Some(guard) = record.pending_entry_guard else {
                return Err(RejectionCode::ProcessError);
            };
            (broker_order_id, guard)
        };

        match self.reconcile_fill_only(setup_id)? {
            ExecutionStatus::FilledProtected { broker_order_id } => {
                return Ok(ExecutionStatus::FilledProtected { broker_order_id });
            }
            ExecutionStatus::Missed { .. } => return Err(RejectionCode::ProcessError),
            ExecutionStatus::Pending { .. } => {}
        }

        let slippage_clear = slippage_guard(
            guard.direction,
            guard.trigger,
            current_price,
            guard.execution_config,
        );
        if slippage_clear == Condition::Unknown {
            self.engine_safe = false;
            return Err(RejectionCode::BrokerUnsafe);
        }
        if slippage_clear == Condition::True
            && !trigger_expired(completed_5m_candles_since_reconfirmation)
        {
            return Ok(ExecutionStatus::Pending { broker_order_id });
        }

        match self.adapter.cancel_entry_order(setup_id, &broker_order_id) {
            Ok(Condition::True) => {
                self.registry.mark_missed(setup_id)?;
                Ok(ExecutionStatus::Missed { broker_order_id })
            }
            Ok(Condition::False | Condition::Unknown) | Err(_) => {
                self.engine_safe = false;
                Err(RejectionCode::BrokerUnsafe)
            }
        }
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
