mod common;

use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission, Condition, EtTime,
    ExecutionCoordinator, ExecutionGateContext, ExecutionStatus, OrderProposal, RejectionCode,
    SetupState,
};

#[derive(Debug, Clone)]
struct TimeBroker {
    reconciliation: BrokerReconciliation,
    submission: BrokerSubmission,
    submit_calls: usize,
    reconcile_calls: usize,
}

impl Default for TimeBroker {
    fn default() -> Self {
        Self {
            reconciliation: clear_reconciliation(),
            submission: BrokerSubmission {
                broker_order_id: "TIME-BROKER-1".to_owned(),
                entry_filled: false,
            },
            submit_calls: 0,
            reconcile_calls: 0,
        }
    }
}

impl BrokerAdapter for TimeBroker {
    type Error = ();

    fn capabilities(&self) -> BrokerCapabilities {
        BrokerCapabilities {
            protected_stop_limit_bracket: Condition::True,
        }
    }

    fn reconcile(&mut self) -> Result<BrokerReconciliation, Self::Error> {
        self.reconcile_calls += 1;
        Ok(self.reconciliation.clone())
    }

    fn submit_protected(
        &mut self,
        _proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error> {
        self.submit_calls += 1;
        Ok(self.submission.clone())
    }

    fn entry_filled(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        Ok(Condition::False)
    }

    fn hard_stop_confirmed(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        Ok(Condition::True)
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

fn time(hour: u8, minute: u8, second: u8) -> EtTime {
    EtTime::from_hms(hour, minute, second).unwrap()
}

fn context(current_time_et: EtTime) -> ExecutionGateContext<'static> {
    ExecutionGateContext {
        submitting_agent_id: common::OWNER,
        llm_setup_pass: Condition::True,
        current_time_et,
    }
}

fn ready(setup_id: &str, broker: TimeBroker) -> ExecutionCoordinator<TimeBroker> {
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
fn sections_3_109_117_122_live_gate_obeys_exact_entry_window_boundaries() {
    for (setup_id, current_time, allowed) in [
        ("TIME-092959", time(9, 29, 59), false),
        ("TIME-093000", time(9, 30, 0), true),
        ("TIME-105959", time(10, 59, 59), true),
        ("TIME-110000", time(11, 0, 0), false),
    ] {
        let mut coordinator = ready(setup_id, TimeBroker::default());
        let result = coordinator.gate(&proposal(setup_id), context(current_time));
        if allowed {
            assert!(result.is_ok());
            assert_eq!(coordinator.adapter().reconcile_calls, 1);
            let status = coordinator.setup_status(setup_id).unwrap();
            assert_eq!(status.state, Some(SetupState::EntryAuthorized));
            assert!(status.order_active_or_reserved);
        } else {
            assert_eq!(result, Err(RejectionCode::TimeCutoff));
            assert_eq!(coordinator.adapter().reconcile_calls, 0);
            let status = coordinator.setup_status(setup_id).unwrap();
            assert_eq!(status.state, None);
            assert!(!status.order_active_or_reserved);
        }
        assert_eq!(coordinator.adapter().submit_calls, 0);
    }
}

#[test]
fn sections_3_4_109_117_permit_before_cutoff_cannot_transmit_at_cutoff() {
    let setup_id = "TIME-PERMIT-CROSS";
    let mut coordinator = ready(setup_id, TimeBroker::default());
    let permit = coordinator
        .gate(&proposal(setup_id), context(time(10, 59, 59)))
        .unwrap();

    let authorized = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(authorized.state, Some(SetupState::EntryAuthorized));
    assert!(authorized.order_active_or_reserved);

    assert_eq!(
        coordinator.submit(permit, time(11, 0, 0)),
        Err(RejectionCode::TimeCutoff)
    );
    assert_eq!(coordinator.adapter().submit_calls, 0);

    let cleared = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(cleared.state, Some(SetupState::EntryAuthorized));
    assert!(!cleared.order_active_or_reserved);
    assert!(!cleared.consumed);

    assert_eq!(
        coordinator.gate(&proposal(setup_id), context(time(11, 0, 0))),
        Err(RejectionCode::TimeCutoff)
    );
    assert_eq!(coordinator.adapter().submit_calls, 0);
}

#[test]
fn sections_3_117_submit_at_105959_remains_permitted() {
    let setup_id = "TIME-SUBMIT-LAST-SECOND";
    let mut coordinator = ready(setup_id, TimeBroker::default());
    let permit = coordinator
        .gate(&proposal(setup_id), context(time(10, 59, 59)))
        .unwrap();

    assert_eq!(
        coordinator.submit(permit, time(10, 59, 59)).unwrap(),
        ExecutionStatus::Pending {
            broker_order_id: "TIME-BROKER-1".to_owned()
        }
    );
    assert_eq!(coordinator.adapter().submit_calls, 1);
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::OrderPending)
    );
}

#[test]
fn section_4_position_management_safety_remains_available_after_1100() {
    let setup_id = "TIME-MANAGE-AFTER-CUTOFF";
    let broker = TimeBroker {
        submission: BrokerSubmission {
            broker_order_id: "TIME-FILLED".to_owned(),
            entry_filled: true,
        },
        ..Default::default()
    };
    let mut coordinator = ready(setup_id, broker);
    let permit = coordinator
        .gate(&proposal(setup_id), context(time(10, 59, 59)))
        .unwrap();
    assert!(matches!(
        coordinator.submit(permit, time(10, 59, 59)).unwrap(),
        ExecutionStatus::FilledProtected { .. }
    ));

    coordinator
        .adapter_mut()
        .reconciliation
        .position_setup_ids
        .push(setup_id.to_owned());

    // The open-position safety API intentionally has no entry-window parameter.
    assert_eq!(coordinator.verify_open_position_safety(setup_id), Ok(()));
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::PositionManagement)
    );
}
