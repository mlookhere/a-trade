mod common;

use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission, Condition, EtTime,
    ExecutionCoordinator, ExecutionGateContext, ExecutionStatus, OrderProposal,
    ProcessViolationScope, RejectionCode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockError {
    Failure,
}

#[derive(Debug, Clone)]
struct MockBroker {
    capabilities: BrokerCapabilities,
    reconciliation: Result<BrokerReconciliation, MockError>,
    submission: Result<BrokerSubmission, MockError>,
    fill_state: Result<Condition, MockError>,
    stop_state: Result<Condition, MockError>,
    flatten_result: Result<(), MockError>,
    reconcile_calls: usize,
    submit_calls: usize,
    stop_calls: usize,
    flatten_calls: usize,
    last_submission: Option<OrderProposal>,
    last_flatten_instrument: Option<String>,
}

impl Default for MockBroker {
    fn default() -> Self {
        Self {
            capabilities: BrokerCapabilities {
                protected_stop_limit_bracket: Condition::True,
            },
            reconciliation: Ok(clear_reconciliation()),
            submission: Ok(BrokerSubmission {
                broker_order_id: "BROKER-1".to_owned(),
                entry_filled: false,
            }),
            fill_state: Ok(Condition::False),
            stop_state: Ok(Condition::True),
            flatten_result: Ok(()),
            reconcile_calls: 0,
            submit_calls: 0,
            stop_calls: 0,
            flatten_calls: 0,
            last_submission: None,
            last_flatten_instrument: None,
        }
    }
}

impl BrokerAdapter for MockBroker {
    type Error = MockError;

    fn capabilities(&self) -> BrokerCapabilities {
        self.capabilities
    }

    fn reconcile(&mut self) -> Result<BrokerReconciliation, Self::Error> {
        self.reconcile_calls += 1;
        self.reconciliation.clone()
    }

    fn submit_protected(
        &mut self,
        proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error> {
        self.submit_calls += 1;
        self.last_submission = Some(proposal.clone());
        self.submission.clone()
    }

    fn entry_filled(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        self.fill_state
    }

    fn hard_stop_confirmed(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        self.stop_calls += 1;
        self.stop_state
    }

    fn flatten_instrument(&mut self, instrument: &str) -> Result<(), Self::Error> {
        self.flatten_calls += 1;
        self.last_flatten_instrument = Some(instrument.to_owned());
        self.flatten_result
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

fn context(owner: &str, llm_setup_pass: Condition) -> ExecutionGateContext<'_> {
    ExecutionGateContext {
        submitting_agent_id: owner,
        llm_setup_pass,
        current_time_et: in_window(),
    }
}

fn operationally_ready(broker: MockBroker) -> ExecutionCoordinator<MockBroker> {
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator
}

fn registered(setup_id: &str) -> ExecutionCoordinator<MockBroker> {
    let mut coordinator = operationally_ready(MockBroker::default());
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    coordinator
}

fn proposal(setup_id: &str) -> OrderProposal {
    common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT)
}

#[test]
fn section_16_unknown_or_false_broker_health_fails_closed() {
    for field in 0..5 {
        let setup_id = format!("SETUP-BROKER-{field}");
        let mut reconciliation = clear_reconciliation();
        match field {
            0 => reconciliation.broker_connected = Condition::Unknown,
            1 => reconciliation.broker_state_known = Condition::Unknown,
            2 => reconciliation.position_state_known = Condition::Unknown,
            3 => reconciliation.open_order_state_known = Condition::Unknown,
            4 => reconciliation.broker_connected = Condition::False,
            _ => unreachable!(),
        }
        let broker = MockBroker {
            reconciliation: Ok(reconciliation),
            ..Default::default()
        };
        let mut coordinator = operationally_ready(broker);
        coordinator
            .register_setup(&setup_id, common::OWNER)
            .unwrap();
        assert_eq!(
            coordinator.gate(
                &proposal(&setup_id),
                context(common::OWNER, Condition::True)
            ),
            Err(RejectionCode::BrokerUnsafe)
        );
    }
}

#[test]
fn section_56_conflicting_order_false_or_unknown_fails_closed() {
    for conflict_clear in [Condition::False, Condition::Unknown] {
        let setup_id = if conflict_clear == Condition::False {
            "SETUP-CONFLICT-FALSE"
        } else {
            "SETUP-CONFLICT-UNKNOWN"
        };
        let mut reconciliation = clear_reconciliation();
        reconciliation.no_conflicting_order = conflict_clear;
        let broker = MockBroker {
            reconciliation: Ok(reconciliation),
            ..Default::default()
        };
        let mut coordinator = operationally_ready(broker);
        coordinator.register_setup(setup_id, common::OWNER).unwrap();
        assert_eq!(
            coordinator.gate(&proposal(setup_id), context(common::OWNER, Condition::True)),
            Err(RejectionCode::ProcessError)
        );
        assert_eq!(coordinator.adapter().submit_calls, 0);
    }
}

#[test]
fn sections_8_9_103_ownership_and_duplicate_state_fail_closed() {
    let setup_id = "SETUP-OWNERSHIP";
    let raw = proposal(setup_id);

    let mut missing = operationally_ready(MockBroker::default());
    assert_eq!(
        missing.gate(&raw, context(common::OWNER, Condition::True)),
        Err(RejectionCode::ProcessError)
    );

    let mut wrong_owner = registered(setup_id);
    assert_eq!(
        wrong_owner.gate(&raw, context("OTHER_AGENT", Condition::True)),
        Err(RejectionCode::DuplicateSetup)
    );

    let mut broker_duplicate = registered(setup_id);
    let mut duplicate = clear_reconciliation();
    duplicate.open_order_setup_ids.push(setup_id.to_owned());
    broker_duplicate.adapter_mut().reconciliation = Ok(duplicate);
    assert_eq!(
        broker_duplicate.gate(&raw, context(common::OWNER, Condition::True)),
        Err(RejectionCode::DuplicateSetup)
    );

    let mut reserved = registered(setup_id);
    let _permit = reserved
        .gate(&raw, context(common::OWNER, Condition::True))
        .unwrap();
    assert_eq!(
        reserved.gate(&raw, context(common::OWNER, Condition::True)),
        Err(RejectionCode::DuplicateSetup)
    );
}

#[test]
fn section_64_protected_bracket_capability_must_be_true() {
    for capability in [Condition::False, Condition::Unknown] {
        let setup_id = if capability == Condition::False {
            "SETUP-CAP-FALSE"
        } else {
            "SETUP-CAP-UNKNOWN"
        };
        let broker = MockBroker {
            capabilities: BrokerCapabilities {
                protected_stop_limit_bracket: capability,
            },
            ..Default::default()
        };
        let mut coordinator = operationally_ready(broker);
        coordinator.register_setup(setup_id, common::OWNER).unwrap();
        assert_eq!(
            coordinator.gate(&proposal(setup_id), context(common::OWNER, Condition::True)),
            Err(RejectionCode::BrokerUnsafe)
        );
    }
}

#[test]
fn sections_109_110_122_only_gate_can_promote_execution_and_llm_must_be_true() {
    let setup_id = "SETUP-PROMOTE";
    let raw = proposal(setup_id);
    assert_eq!(raw.execution_pass(), Condition::Unknown);

    for pass in [Condition::False, Condition::Unknown] {
        let mut coordinator = registered(setup_id);
        assert_eq!(
            coordinator.gate(&raw, context(common::OWNER, pass)),
            Err(RejectionCode::ProcessError)
        );
    }

    let mut coordinator = registered(setup_id);
    let permit = coordinator
        .gate(&raw, context(common::OWNER, Condition::True))
        .unwrap();
    assert_eq!(raw.execution_pass(), Condition::Unknown);
    assert_eq!(permit.proposal().execution_pass(), Condition::True);
}

#[test]
fn sections_8_64_pending_submission_is_exactly_once_and_not_consumed() {
    let setup_id = "SETUP-PENDING";
    let mut coordinator = registered(setup_id);
    let raw = proposal(setup_id);
    let permit = coordinator
        .gate(&raw, context(common::OWNER, Condition::True))
        .unwrap();
    assert_eq!(
        coordinator.submit(permit, in_window()).unwrap(),
        ExecutionStatus::Pending {
            broker_order_id: "BROKER-1".to_owned()
        }
    );
    assert_eq!(coordinator.adapter().submit_calls, 1);
    let submitted = coordinator.adapter().last_submission.as_ref().unwrap();
    assert_eq!(submitted.setup_id(), raw.setup_id());
    assert_eq!(submitted.instrument(), raw.instrument());
    assert_eq!(submitted.entry_trigger(), raw.entry_trigger());
    assert_eq!(submitted.stop(), raw.stop());
    assert_eq!(submitted.execution_pass(), Condition::True);
    let registry = coordinator.setup_status(setup_id).unwrap();
    assert!(!registry.consumed);
    assert!(registry.order_active_or_reserved);
}

#[test]
fn section_64_immediate_fill_requires_confirmed_stop_or_flattens_and_disables_engine() {
    for stop_state in [Condition::True, Condition::False, Condition::Unknown] {
        let setup_id = match stop_state {
            Condition::True => "SETUP-STOP-TRUE",
            Condition::False => "SETUP-STOP-FALSE",
            Condition::Unknown => "SETUP-STOP-UNKNOWN",
        };
        let broker = MockBroker {
            submission: Ok(BrokerSubmission {
                broker_order_id: "FILLED".to_owned(),
                entry_filled: true,
            }),
            stop_state: Ok(stop_state),
            ..Default::default()
        };
        let mut coordinator = operationally_ready(broker);
        coordinator.register_setup(setup_id, common::OWNER).unwrap();
        let permit = coordinator
            .gate(&proposal(setup_id), context(common::OWNER, Condition::True))
            .unwrap();

        if stop_state == Condition::True {
            assert!(matches!(
                coordinator.submit(permit, in_window()).unwrap(),
                ExecutionStatus::FilledProtected { .. }
            ));
            assert_eq!(coordinator.adapter().flatten_calls, 0);
            assert_eq!(coordinator.execution_engine_safe(), Condition::True);
        } else {
            assert_eq!(
                coordinator.submit(permit, in_window()),
                Err(RejectionCode::BrokerUnsafe)
            );
            assert_eq!(coordinator.adapter().flatten_calls, 1);
            assert_eq!(
                coordinator.adapter().last_flatten_instrument.as_deref(),
                Some(common::INSTRUMENT)
            );
            assert_eq!(coordinator.execution_engine_safe(), Condition::False);
        }
        assert!(coordinator.setup_status(setup_id).unwrap().consumed);
    }
}

#[test]
fn section_64_stop_adapter_error_and_flatten_failure_remain_broker_unsafe() {
    for flatten_result in [Ok(()), Err(MockError::Failure)] {
        let setup_id = if flatten_result.is_ok() {
            "SETUP-STOP-ERROR"
        } else {
            "SETUP-FLATTEN-ERROR"
        };
        let broker = MockBroker {
            submission: Ok(BrokerSubmission {
                broker_order_id: "FILLED-ERROR".to_owned(),
                entry_filled: true,
            }),
            stop_state: Err(MockError::Failure),
            flatten_result,
            ..Default::default()
        };
        let mut coordinator = operationally_ready(broker);
        coordinator.register_setup(setup_id, common::OWNER).unwrap();
        let permit = coordinator
            .gate(&proposal(setup_id), context(common::OWNER, Condition::True))
            .unwrap();
        assert_eq!(
            coordinator.submit(permit, in_window()),
            Err(RejectionCode::BrokerUnsafe)
        );
        assert_eq!(coordinator.adapter().flatten_calls, 1);
        assert_eq!(coordinator.execution_engine_safe(), Condition::False);
    }
}

#[test]
fn submission_error_does_not_retry_and_disables_shared_execution_engine() {
    let setup_id = "SETUP-SUBMIT-ERROR";
    let broker = MockBroker {
        submission: Err(MockError::Failure),
        ..Default::default()
    };
    let mut coordinator = operationally_ready(broker);
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    let permit = coordinator
        .gate(&proposal(setup_id), context(common::OWNER, Condition::True))
        .unwrap();
    assert_eq!(
        coordinator.submit(permit, in_window()),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.adapter().submit_calls, 1);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
}

#[test]
fn pre_submit_reconciliation_rechecks_duplicate_and_conflict_without_transmitting() {
    for conflict in [false, true] {
        let setup_id = if conflict {
            "SETUP-RECHECK-CONFLICT"
        } else {
            "SETUP-RECHECK-DUPLICATE"
        };
        let mut coordinator = registered(setup_id);
        let permit = coordinator
            .gate(&proposal(setup_id), context(common::OWNER, Condition::True))
            .unwrap();
        let mut changed = clear_reconciliation();
        if conflict {
            changed.no_conflicting_order = Condition::False;
        } else {
            changed.open_order_setup_ids.push(setup_id.to_owned());
        }
        coordinator.adapter_mut().reconciliation = Ok(changed);
        let expected = if conflict {
            RejectionCode::ProcessError
        } else {
            RejectionCode::DuplicateSetup
        };
        assert_eq!(coordinator.submit(permit, in_window()), Err(expected));
        assert_eq!(coordinator.adapter().submit_calls, 0);
        assert!(
            !coordinator
                .setup_status(setup_id)
                .unwrap()
                .order_active_or_reserved
        );
    }
}

#[test]
fn sections_98_100_operational_safety_and_post_gate_kill_switch_cannot_be_bypassed() {
    let setup_id = "SETUP-OPS";
    let raw = proposal(setup_id);
    let mut unknown = ExecutionCoordinator::new(MockBroker::default());
    unknown.register_setup(setup_id, common::OWNER).unwrap();
    assert_eq!(
        unknown.gate(&raw, context(common::OWNER, Condition::True)),
        Err(RejectionCode::ProcessError)
    );
    assert_eq!(unknown.adapter().reconcile_calls, 0);

    let kill_id = "SETUP-KILL";
    let mut coordinator = registered(kill_id);
    let permit = coordinator
        .gate(&proposal(kill_id), context(common::OWNER, Condition::True))
        .unwrap();
    coordinator
        .operational_safety_mut()
        .record_process_violation(ProcessViolationScope::SharedInfrastructure)
        .unwrap();
    assert_eq!(
        coordinator.submit(permit, in_window()),
        Err(RejectionCode::ProcessError)
    );
    assert_eq!(coordinator.adapter().submit_calls, 0);
}
