mod common;

use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission, Condition,
    ExecutionCoordinator, ExecutionGateContext, ExecutionStatus, OrderProposal, RejectionCode,
    SetupState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockError {
    Failure,
}

#[derive(Debug, Clone)]
struct MockBroker {
    reconciliation: Result<BrokerReconciliation, MockError>,
    submission: Result<BrokerSubmission, MockError>,
    fill_state: Result<Condition, MockError>,
    stop_state: Result<Condition, MockError>,
}

impl Default for MockBroker {
    fn default() -> Self {
        Self {
            reconciliation: Ok(clear_reconciliation()),
            submission: Ok(BrokerSubmission {
                broker_order_id: "BROKER-PENDING".to_owned(),
                entry_filled: false,
            }),
            fill_state: Ok(Condition::False),
            stop_state: Ok(Condition::True),
        }
    }
}

impl BrokerAdapter for MockBroker {
    type Error = MockError;

    fn capabilities(&self) -> BrokerCapabilities {
        BrokerCapabilities {
            protected_stop_limit_bracket: Condition::True,
        }
    }

    fn reconcile(&mut self) -> Result<BrokerReconciliation, Self::Error> {
        self.reconciliation.clone()
    }

    fn submit_protected(
        &mut self,
        _proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error> {
        self.submission.clone()
    }

    fn entry_filled(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        self.fill_state
    }

    fn hard_stop_confirmed(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        self.stop_state
    }

    fn flatten_instrument(&mut self, _instrument: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn clear_reconciliation() -> BrokerReconciliation {
    BrokerReconciliation {
        broker_connected: Condition::True,
        broker_state_known: Condition::True,
        position_state_known: Condition::True,
        open_order_state_known: Condition::True,
        no_conflicting_order: Condition::True,
        open_order_setup_ids: vec![],
        position_setup_ids: vec![],
    }
}

fn context() -> ExecutionGateContext<'static> {
    ExecutionGateContext {
        submitting_agent_id: common::OWNER,
        llm_setup_pass: Condition::True,
    }
}

fn ready_coordinator(setup_id: &str, broker: MockBroker) -> ExecutionCoordinator<MockBroker> {
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    coordinator
}

fn proposal(setup_id: &str) -> OrderProposal {
    common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT)
}

#[test]
fn sections_102_103_gate_and_broker_acceptance_advance_exact_execution_tail() {
    let setup_id = "SETUP-STATE-PENDING";
    let mut coordinator = ready_coordinator(setup_id, MockBroker::default());
    assert_eq!(coordinator.setup_status(setup_id).unwrap().state, None);

    let permit = coordinator.gate(&proposal(setup_id), context()).unwrap();
    let authorized = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(authorized.state, Some(SetupState::EntryAuthorized));
    assert!(authorized.order_active_or_reserved);
    assert!(!authorized.consumed);

    assert_eq!(
        coordinator.submit(permit).unwrap(),
        ExecutionStatus::Pending {
            broker_order_id: "BROKER-PENDING".to_owned()
        }
    );
    let pending = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(pending.state, Some(SetupState::OrderPending));
    assert!(pending.order_active_or_reserved);
    assert!(!pending.consumed);
    assert_eq!(pending.broker_order_id.as_deref(), Some("BROKER-PENDING"));
}

#[test]
fn sections_64_102_117_confirmed_fill_and_stop_reach_position_management_without_skip() {
    let setup_id = "SETUP-STATE-IMMEDIATE-FILL";
    let broker = MockBroker {
        submission: Ok(BrokerSubmission {
            broker_order_id: "BROKER-FILLED".to_owned(),
            entry_filled: true,
        }),
        ..Default::default()
    };
    let mut coordinator = ready_coordinator(setup_id, broker);
    let permit = coordinator.gate(&proposal(setup_id), context()).unwrap();

    assert_eq!(
        coordinator.submit(permit).unwrap(),
        ExecutionStatus::FilledProtected {
            broker_order_id: "BROKER-FILLED".to_owned()
        }
    );
    let managed = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(managed.state, Some(SetupState::PositionManagement));
    assert!(managed.consumed);
    assert!(!managed.order_active_or_reserved);
}

#[test]
fn sections_102_117_pending_fill_reconciliation_advances_to_position_management() {
    let setup_id = "SETUP-STATE-RECONCILE";
    let mut coordinator = ready_coordinator(setup_id, MockBroker::default());
    let permit = coordinator.gate(&proposal(setup_id), context()).unwrap();
    assert!(matches!(
        coordinator.submit(permit).unwrap(),
        ExecutionStatus::Pending { .. }
    ));
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::OrderPending)
    );

    coordinator.adapter_mut().fill_state = Ok(Condition::True);
    assert!(matches!(
        coordinator.reconcile_fill(setup_id).unwrap(),
        ExecutionStatus::FilledProtected { .. }
    ));
    let managed = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(managed.state, Some(SetupState::PositionManagement));
    assert!(managed.consumed);
}

#[test]
fn sections_64_102_unprotected_fill_stays_filled_without_inventing_terminal_state() {
    let setup_id = "SETUP-STATE-UNPROTECTED";
    let broker = MockBroker {
        submission: Ok(BrokerSubmission {
            broker_order_id: "BROKER-UNPROTECTED".to_owned(),
            entry_filled: true,
        }),
        stop_state: Ok(Condition::False),
        ..Default::default()
    };
    let mut coordinator = ready_coordinator(setup_id, broker);
    let permit = coordinator.gate(&proposal(setup_id), context()).unwrap();

    assert_eq!(
        coordinator.submit(permit),
        Err(RejectionCode::BrokerUnsafe)
    );
    let filled = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(filled.state, Some(SetupState::Filled));
    assert!(filled.consumed);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
}

#[test]
fn section_102_pretransmission_fail_close_keeps_entry_authorized_without_regression() {
    let setup_id = "SETUP-STATE-PREFLIGHT";
    let mut coordinator = ready_coordinator(setup_id, MockBroker::default());
    let permit = coordinator.gate(&proposal(setup_id), context()).unwrap();
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::EntryAuthorized)
    );

    let mut unavailable = clear_reconciliation();
    unavailable.broker_state_known = Condition::Unknown;
    coordinator.adapter_mut().reconciliation = Ok(unavailable);
    assert_eq!(coordinator.submit(permit), Err(RejectionCode::BrokerUnsafe));

    let cleared = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(cleared.state, Some(SetupState::EntryAuthorized));
    assert!(!cleared.order_active_or_reserved);
    assert!(!cleared.consumed);

    coordinator.adapter_mut().reconciliation = Ok(clear_reconciliation());
    let _new_permit = coordinator.gate(&proposal(setup_id), context()).unwrap();
    let reauthorized = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(reauthorized.state, Some(SetupState::EntryAuthorized));
    assert!(reauthorized.order_active_or_reserved);
}
