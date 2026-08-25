mod common;

use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerOrderCancellation, BrokerReconciliation,
    BrokerSubmission, Condition, EtTime, ExecutionCoordinator, ExecutionGateContext,
    ExecutionStatus, OrderProposal, RejectionCode, SetupState, TerminalState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockError {
    Failure,
}

#[derive(Debug, Clone)]
struct MockBroker {
    fill_state: Result<Condition, MockError>,
    stop_state: Result<Condition, MockError>,
    cancel_state: Result<Condition, MockError>,
    cancel_calls: usize,
}

impl Default for MockBroker {
    fn default() -> Self {
        Self {
            fill_state: Ok(Condition::False),
            stop_state: Ok(Condition::True),
            cancel_state: Ok(Condition::True),
            cancel_calls: 0,
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
        Ok(BrokerReconciliation {
            broker_connected: Condition::True,
            broker_state_known: Condition::True,
            position_state_known: Condition::True,
            open_order_state_known: Condition::True,
            no_conflicting_order: Condition::True,
            open_order_setup_ids: vec![],
            position_setup_ids: vec![],
        })
    }

    fn submit_protected(
        &mut self,
        _proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error> {
        Ok(BrokerSubmission {
            broker_order_id: "BROKER-PENDING".to_owned(),
            entry_filled: false,
        })
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

impl BrokerOrderCancellation for MockBroker {
    fn cancel_entry_order(
        &mut self,
        _setup_id: &str,
        _broker_order_id: &str,
    ) -> Result<Condition, Self::Error> {
        self.cancel_calls += 1;
        self.cancel_state
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

fn pending_long(
    setup_id: &str,
    broker: MockBroker,
) -> (ExecutionCoordinator<MockBroker>, OrderProposal) {
    let proposal = common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT);
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    let permit = coordinator.gate(&proposal, context()).unwrap();
    assert!(matches!(
        coordinator.submit(permit, in_window()).unwrap(),
        ExecutionStatus::Pending { .. }
    ));
    (coordinator, proposal)
}

fn pending_short(
    setup_id: &str,
    broker: MockBroker,
) -> (ExecutionCoordinator<MockBroker>, OrderProposal) {
    let proof = common::valid_short_proof(setup_id, common::OWNER, common::INSTRUMENT);
    let proposal = auction_core::build_order_proposal(auction_core::OrderProposalInputs {
        strategy: &proof,
        entry_limit: -0.5,
        target: -15.0,
        execution_config: auction_core::InstrumentExecutionConfig {
            tick_size: 0.25,
            max_entry_slippage_ticks: 4,
            stop_buffer_ticks: 2,
        },
        account_equity: 100_000.0,
        risk_percent: 0.0025,
        tick_value: 1.25,
        commissions_per_contract: 1.0,
        slippage_reserve_per_contract: 1.0,
        current_open_portfolio_risk: 100.0,
        current_cluster_risk: 50.0,
        portfolio_limits: auction_core::PortfolioRiskLimits {
            max_portfolio_open_risk_percent: 0.02,
            max_cluster_open_risk_percent: 0.01,
        },
        cluster_id: "NASDAQ_CLUSTER",
        existing_cluster_exposure: &[],
    })
    .unwrap();
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    let permit = coordinator.gate(&proposal, context()).unwrap();
    assert!(matches!(
        coordinator.submit(permit, in_window()).unwrap(),
        ExecutionStatus::Pending { .. }
    ));
    (coordinator, proposal)
}

#[test]
fn section_60_price_at_long_slippage_boundary_remains_pending() {
    let setup_id = "SETUP-LONG-BOUNDARY";
    let (mut coordinator, proposal) = pending_long(setup_id, MockBroker::default());
    let boundary = proposal.entry_trigger() + 1.0;

    assert!(matches!(
        coordinator
            .reconcile_pending_entry(setup_id, boundary, 1)
            .unwrap(),
        ExecutionStatus::Pending { .. }
    ));
    assert_eq!(coordinator.adapter().cancel_calls, 0);
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::OrderPending)
    );
}

#[test]
fn section_60_long_price_beyond_slippage_cancels_and_marks_missed() {
    let setup_id = "SETUP-LONG-MISSED";
    let (mut coordinator, proposal) = pending_long(setup_id, MockBroker::default());
    let beyond = proposal.entry_trigger() + 1.25;

    assert!(matches!(
        coordinator
            .reconcile_pending_entry(setup_id, beyond, 0)
            .unwrap(),
        ExecutionStatus::Missed { .. }
    ));
    assert_eq!(coordinator.adapter().cancel_calls, 1);
    let status = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(
        status.state,
        Some(SetupState::Terminal(TerminalState::Missed))
    );
    assert!(!status.consumed);
    assert!(!status.order_active_or_reserved);
    assert_eq!(coordinator.execution_engine_safe(), Condition::True);
}

#[test]
fn section_60_short_price_beyond_slippage_uses_sealed_short_direction() {
    let setup_id = "SETUP-SHORT-MISSED";
    let (mut coordinator, proposal) = pending_short(setup_id, MockBroker::default());
    let beyond = proposal.entry_trigger() - 1.25;

    assert!(matches!(
        coordinator
            .reconcile_pending_entry(setup_id, beyond, 0)
            .unwrap(),
        ExecutionStatus::Missed { .. }
    ));
    assert_eq!(coordinator.adapter().cancel_calls, 1);
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::Terminal(TerminalState::Missed))
    );
}

#[test]
fn section_61_second_completed_candle_expires_even_when_price_is_inside_allowance() {
    let setup_id = "SETUP-STALE";
    let (mut coordinator, proposal) = pending_long(setup_id, MockBroker::default());

    assert!(matches!(
        coordinator
            .reconcile_pending_entry(setup_id, proposal.entry_trigger(), 2)
            .unwrap(),
        ExecutionStatus::Missed { .. }
    ));
    assert_eq!(coordinator.adapter().cancel_calls, 1);
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::Terminal(TerminalState::Missed))
    );
}

#[test]
fn section_117_fill_is_reconciled_before_any_miss_cancellation() {
    let setup_id = "SETUP-FILL-RACE";
    let broker = MockBroker {
        fill_state: Ok(Condition::True),
        ..Default::default()
    };
    let (mut coordinator, proposal) = pending_long(setup_id, broker);
    let beyond = proposal.entry_trigger() + 10.0;

    assert!(matches!(
        coordinator
            .reconcile_pending_entry(setup_id, beyond, 2)
            .unwrap(),
        ExecutionStatus::FilledProtected { .. }
    ));
    assert_eq!(coordinator.adapter().cancel_calls, 0);
    let status = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(status.state, Some(SetupState::PositionManagement));
    assert!(status.consumed);
}

#[test]
fn sections_98_100_unconfirmed_cancellation_fails_closed_without_false_missed_state() {
    for cancel_state in [
        Ok(Condition::False),
        Ok(Condition::Unknown),
        Err(MockError::Failure),
    ] {
        let setup_id = match cancel_state {
            Ok(Condition::False) => "SETUP-CANCEL-FALSE",
            Ok(Condition::Unknown) => "SETUP-CANCEL-UNKNOWN",
            Err(_) => "SETUP-CANCEL-ERROR",
            Ok(Condition::True) => unreachable!(),
        };
        let broker = MockBroker {
            cancel_state,
            ..Default::default()
        };
        let (mut coordinator, proposal) = pending_long(setup_id, broker);
        let beyond = proposal.entry_trigger() + 1.25;

        assert_eq!(
            coordinator.reconcile_pending_entry(setup_id, beyond, 0),
            Err(RejectionCode::BrokerUnsafe)
        );
        assert_eq!(coordinator.adapter().cancel_calls, 1);
        assert_eq!(coordinator.execution_engine_safe(), Condition::False);
        let status = coordinator.setup_status(setup_id).unwrap();
        assert_eq!(status.state, Some(SetupState::OrderPending));
        assert!(status.order_active_or_reserved);
        assert!(!status.consumed);
    }
}

#[test]
fn section_10_invalid_price_fails_closed_without_fabricating_cancellation_result() {
    let setup_id = "SETUP-PRICE-UNKNOWN";
    let (mut coordinator, _) = pending_long(setup_id, MockBroker::default());

    assert_eq!(
        coordinator.reconcile_pending_entry(setup_id, f64::NAN, 0),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.adapter().cancel_calls, 0);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
    let status = coordinator.setup_status(setup_id).unwrap();
    assert_eq!(status.state, Some(SetupState::OrderPending));
    assert!(status.order_active_or_reserved);
}

#[test]
fn sections_61_102_missed_setup_cannot_be_reauthorized_or_reused() {
    let setup_id = "SETUP-MISSED-REUSE";
    let (mut coordinator, proposal) = pending_long(setup_id, MockBroker::default());
    assert!(matches!(
        coordinator
            .reconcile_pending_entry(setup_id, proposal.entry_trigger(), 2)
            .unwrap(),
        ExecutionStatus::Missed { .. }
    ));

    assert_eq!(
        coordinator.gate(&proposal, context()),
        Err(RejectionCode::DuplicateSetup)
    );
    assert_eq!(
        coordinator.setup_status(setup_id).unwrap().state,
        Some(SetupState::Terminal(TerminalState::Missed))
    );
}
