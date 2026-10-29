mod common;

use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission, Condition, EtTime,
    ExecutionCoordinator, ExecutionGateContext, ExecutionStatus, OrderProposal, RejectionCode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockError {
    Failure,
}

#[derive(Debug, Clone)]
struct MockBroker {
    reconciliation: Result<BrokerReconciliation, MockError>,
    submission: Result<BrokerSubmission, MockError>,
    stop_state: Result<Condition, MockError>,
    reconcile_calls: usize,
    stop_calls: usize,
    flatten_calls: usize,
}

impl Default for MockBroker {
    fn default() -> Self {
        Self {
            reconciliation: Ok(clear_reconciliation()),
            submission: Ok(BrokerSubmission {
                broker_order_id: "BROKER-OPEN".to_owned(),
                entry_filled: true,
            }),
            stop_state: Ok(Condition::True),
            reconcile_calls: 0,
            stop_calls: 0,
            flatten_calls: 0,
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
        self.reconcile_calls += 1;
        self.reconciliation.clone()
    }

    fn submit_protected(
        &mut self,
        _proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error> {
        self.submission.clone()
    }

    fn entry_filled(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        Ok(Condition::True)
    }

    fn hard_stop_confirmed(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        self.stop_calls += 1;
        self.stop_state
    }

    fn flatten_instrument(&mut self, _instrument: &str) -> Result<(), Self::Error> {
        self.flatten_calls += 1;
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

fn in_window() -> EtTime {
    EtTime::from_hms(10, 0, 0).unwrap()
}

fn context() -> ExecutionGateContext<'static> {
    ExecutionGateContext {
        submitting_agent_id: common::OWNER,
        llm_setup_pass: Condition::True,
        current_time_et: in_window(),
    }
}

fn filled_position(setup_id: &str) -> ExecutionCoordinator<MockBroker> {
    let mut coordinator = ExecutionCoordinator::new(MockBroker::default());
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    let proposal = common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT);
    let permit = coordinator.gate(&proposal, context()).unwrap();
    assert!(matches!(
        coordinator.submit(permit, in_window()).unwrap(),
        ExecutionStatus::FilledProtected { .. }
    ));
    let mut reconciliation = clear_reconciliation();
    reconciliation.position_setup_ids.push(setup_id.to_owned());
    coordinator.adapter_mut().reconciliation = Ok(reconciliation);
    coordinator
}

#[test]
fn section_118_reconciled_position_with_confirmed_stop_remains_manageable() {
    let setup_id = "SETUP-OPEN-SAFE";
    let mut coordinator = filled_position(setup_id);
    assert_eq!(coordinator.verify_open_position_safety(setup_id), Ok(()));
    assert_eq!(coordinator.adapter().flatten_calls, 0);
    assert_eq!(coordinator.adapter().stop_calls, 2);
    assert_eq!(coordinator.execution_engine_safe(), Condition::True);
}

#[test]
fn sections_16_98_118_unknown_or_missing_position_state_fails_closed() {
    let setup_id = "SETUP-OPEN-MISSING";
    let mut coordinator = filled_position(setup_id);
    coordinator.adapter_mut().reconciliation = Ok(clear_reconciliation());
    assert_eq!(
        coordinator.verify_open_position_safety(setup_id),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.adapter().flatten_calls, 0);
    assert_eq!(coordinator.execution_engine_safe(), Condition::True);

    let setup_id = "SETUP-OPEN-UNKNOWN";
    let mut coordinator = filled_position(setup_id);
    let mut reconciliation = clear_reconciliation();
    reconciliation.position_setup_ids.push(setup_id.to_owned());
    reconciliation.position_state_known = Condition::Unknown;
    coordinator.adapter_mut().reconciliation = Ok(reconciliation);
    assert_eq!(
        coordinator.verify_open_position_safety(setup_id),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.adapter().flatten_calls, 0);
    assert_eq!(coordinator.execution_engine_safe(), Condition::True);
}

#[test]
fn sections_64_100_118_unconfirmed_stop_flattens_once_and_latches_execution_unsafe() {
    for stop_state in [
        Ok(Condition::False),
        Ok(Condition::Unknown),
        Err(MockError::Failure),
    ] {
        let setup_id = match stop_state {
            Ok(Condition::False) => "SETUP-OPEN-STOP-FALSE",
            Ok(Condition::Unknown) => "SETUP-OPEN-STOP-UNKNOWN",
            Err(MockError::Failure) => "SETUP-OPEN-STOP-ERROR",
            Ok(Condition::True) => unreachable!(),
        };
        let mut coordinator = filled_position(setup_id);
        coordinator.adapter_mut().stop_state = stop_state;
        assert_eq!(
            coordinator.verify_open_position_safety(setup_id),
            Err(RejectionCode::BrokerUnsafe)
        );
        assert_eq!(coordinator.adapter().flatten_calls, 1);
        assert_eq!(coordinator.execution_engine_safe(), Condition::False);
        assert_eq!(
            coordinator.verify_open_position_safety(setup_id),
            Err(RejectionCode::BrokerUnsafe)
        );
        assert_eq!(coordinator.adapter().flatten_calls, 1);
    }
}

/// A shared broker/Execution Engine failure must not suppress safety checks
/// on other already-open positions (§§73, 100-101, 118).
fn latch_other_setup_failure(
    coordinator: &mut ExecutionCoordinator<MockBroker>,
    failed_setup_id: &str,
) {
    coordinator
        .register_setup(failed_setup_id, common::OWNER)
        .unwrap();
    coordinator.adapter_mut().submission = Err(MockError::Failure);
    let proposal =
        common::valid_long_proposal(failed_setup_id, common::OWNER, common::INSTRUMENT);
    let permit = coordinator.gate(&proposal, context()).unwrap();
    assert_eq!(
        coordinator.submit(permit, in_window()),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
}

#[test]
fn sections_73_101_118_protective_checks_continue_after_shared_execution_fault() {
    let open_setup = "SETUP-OPEN-AFTER-SHARED-FAULT";
    let mut coordinator = filled_position(open_setup);
    latch_other_setup_failure(&mut coordinator, "SETUP-OTHER-SUBMISSION-FAILURE");

    let reconciliations_before = coordinator.adapter().reconcile_calls;
    let stop_checks_before = coordinator.adapter().stop_calls;
    assert_eq!(coordinator.verify_open_position_safety(open_setup), Ok(()));
    assert_eq!(
        coordinator.adapter().reconcile_calls,
        reconciliations_before + 1
    );
    assert_eq!(coordinator.adapter().stop_calls, stop_checks_before + 1);
    assert_eq!(coordinator.adapter().flatten_calls, 0);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
}

#[test]
fn sections_16_64_100_118_shared_fault_never_skips_stop_fail_close() {
    let open_setup = "SETUP-STOP-AFTER-SHARED-FAULT";
    let mut coordinator = filled_position(open_setup);
    let prior_reconciliation = coordinator.adapter().reconciliation.clone();
    latch_other_setup_failure(&mut coordinator, "SETUP-OTHER-BROKER-FAULT");

    // An unqueryable broker is not interpreted as a safe position or missing stop.
    coordinator.adapter_mut().reconciliation = Err(MockError::Failure);
    let checks_before = coordinator.adapter().stop_calls;
    assert_eq!(
        coordinator.verify_open_position_safety(open_setup),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.adapter().stop_calls, checks_before);
    assert_eq!(coordinator.adapter().flatten_calls, 0);

    coordinator.adapter_mut().reconciliation = prior_reconciliation;
    coordinator.adapter_mut().stop_state = Ok(Condition::Unknown);
    assert_eq!(
        coordinator.verify_open_position_safety(open_setup),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.adapter().flatten_calls, 1);
    assert_eq!(
        coordinator.verify_open_position_safety(open_setup),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.adapter().flatten_calls, 1);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
}

#[test]
fn section_118_pending_or_unknown_setup_is_not_promoted_to_open_position() {
    let setup_id = "SETUP-OPEN-PENDING";
    let broker = MockBroker {
        submission: Ok(BrokerSubmission {
            broker_order_id: "BROKER-PENDING".to_owned(),
            entry_filled: false,
        }),
        ..Default::default()
    };
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    let proposal = common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT);
    let permit = coordinator.gate(&proposal, context()).unwrap();
    assert!(matches!(
        coordinator.submit(permit, in_window()).unwrap(),
        ExecutionStatus::Pending { .. }
    ));
    assert_eq!(
        coordinator.verify_open_position_safety(setup_id),
        Err(RejectionCode::ProcessError)
    );

    let mut coordinator = ExecutionCoordinator::new(MockBroker::default());
    assert_eq!(
        coordinator.verify_open_position_safety("UNKNOWN-SETUP"),
        Err(RejectionCode::ProcessError)
    );
    assert_eq!(
        coordinator.verify_open_position_safety("   "),
        Err(RejectionCode::ProcessError)
    );
}
