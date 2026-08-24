use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission, Condition, Direction,
    ExecutionCoordinator, ExecutionGateContext, ExecutionStatus, OrderProposal, OrderType,
    PreExecutionAuthority, ProductionConditions, RejectionCode,
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
    fill_calls: usize,
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
            fill_calls: 0,
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
        self.fill_calls += 1;
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
        open_order_setup_ids: vec![],
        position_setup_ids: vec![],
    }
}

fn all_true_conditions() -> ProductionConditions {
    ProductionConditions {
        time_valid: Condition::True,
        data_valid: Condition::True,
        premarket_plan_complete: Condition::True,
        environment_valid: Condition::True,
        direction_valid: Condition::True,
        qualified_swing_exists: Condition::True,
        fib_zone_valid: Condition::True,
        fib_zone_outside_value: Condition::True,
        location_reached: Condition::True,
        fib_886_valid: Condition::True,
        participation_valid: Condition::True,
        countertrend_aggression: Condition::True,
        aggression_at_extreme: Condition::True,
        effort_failed: Condition::True,
        absorption_present: Condition::True,
        first_dominance_shift: Condition::True,
        second_attempt_present: Condition::True,
        second_attempt_has_countertrend_aggression: Condition::True,
        second_failure_structure_valid: Condition::True,
        second_failure_valid: Condition::True,
        final_reconfirmation: Condition::True,
        structural_target_valid: Condition::True,
        per_trade_risk_valid: Condition::True,
        portfolio_risk_valid: Condition::True,
        correlation_risk_valid: Condition::True,
        setup_not_duplicated: Condition::True,
        no_conflicting_order: Condition::True,
        news_blackout_clear: Condition::True,
        broker_safe: Condition::Unknown,
        execution_engine_safe: Condition::Unknown,
    }
}

fn authority() -> PreExecutionAuthority {
    PreExecutionAuthority {
        llm_setup_pass: Condition::True,
        strategy_validator_pass: Condition::True,
        risk_engine_pass: Condition::True,
        portfolio_coordinator_pass: Condition::True,
    }
}

fn proposal(setup_id: &str, instrument: &str) -> OrderProposal {
    OrderProposal {
        setup_id: setup_id.to_owned(),
        instrument: instrument.to_owned(),
        side: Direction::Long,
        order_type: OrderType::StopLimit,
        entry_trigger: 100.25,
        entry_limit: 100.50,
        stop: 98.50,
        target: 103.00,
        size: 2,
        risk_dollars: 250.0,
        risk_percent: 0.0025,
        open_portfolio_risk_before: 100.0,
        open_portfolio_risk_after: 250.0,
        cluster_id: "NASDAQ_CLUSTER".to_owned(),
        cluster_risk_after: 200.0,
        strategy_pass: Condition::True,
        risk_pass: Condition::True,
        portfolio_pass: Condition::True,
        execution_pass: Condition::Unknown,
    }
}

fn context<'a>(owner: &'a str) -> ExecutionGateContext<'a> {
    ExecutionGateContext {
        submitting_agent_id: owner,
        production_conditions: all_true_conditions(),
        authority: authority(),
    }
}

fn registered() -> ExecutionCoordinator<MockBroker> {
    let mut coordinator = ExecutionCoordinator::new(MockBroker::default());
    coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();
    coordinator
}

#[test]
fn section_16_every_unknown_broker_reconciliation_field_fails_closed() {
    for field in 0..4 {
        let mut reconciliation = clear_reconciliation();
        match field {
            0 => reconciliation.broker_connected = Condition::Unknown,
            1 => reconciliation.broker_state_known = Condition::Unknown,
            2 => reconciliation.position_state_known = Condition::Unknown,
            3 => reconciliation.open_order_state_known = Condition::Unknown,
            _ => unreachable!(),
        }

        let mut broker = MockBroker::default();
        broker.reconciliation = Ok(reconciliation);
        let mut coordinator = ExecutionCoordinator::new(broker);
        coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();
        assert_eq!(
            coordinator.gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT")),
            Err(RejectionCode::BrokerUnsafe)
        );
    }
}

#[test]
fn sections_16_122_false_broker_state_and_invalid_data_fail_closed() {
    let mut broker = MockBroker::default();
    let mut reconciliation = clear_reconciliation();
    reconciliation.broker_connected = Condition::False;
    broker.reconciliation = Ok(reconciliation);
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();
    assert_eq!(
        coordinator.gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT")),
        Err(RejectionCode::BrokerUnsafe)
    );

    let mut coordinator = registered();
    let mut gate_context = context("MNQ_AGENT");
    gate_context.production_conditions.data_valid = Condition::Unknown;
    assert_eq!(
        coordinator.gate(&proposal("SETUP-14", "MNQ"), gate_context),
        Err(RejectionCode::DataInvalid)
    );
}

#[test]
fn sections_8_9_103_missing_setup_wrong_owner_and_duplicate_reservation_reject() {
    let mut missing = ExecutionCoordinator::new(MockBroker::default());
    assert_eq!(
        missing.gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT")),
        Err(RejectionCode::ProcessError)
    );

    let mut wrong_owner = registered();
    assert_eq!(
        wrong_owner.gate(&proposal("SETUP-14", "MNQ"), context("OTHER_AGENT")),
        Err(RejectionCode::DuplicateSetup)
    );

    let mut coordinator = registered();
    let raw = proposal("SETUP-14", "MNQ");
    let _permit = coordinator.gate(&raw, context("MNQ_AGENT")).unwrap();
    assert_eq!(
        coordinator.gate(&raw, context("MNQ_AGENT")),
        Err(RejectionCode::DuplicateSetup)
    );
    let status = coordinator.setup_status("SETUP-14").unwrap();
    assert!(status.order_active_or_reserved);
    assert!(!status.consumed);
}

#[test]
fn section_9_reconciled_broker_order_or_position_for_same_setup_rejects_duplicate() {
    for position in [false, true] {
        let mut reconciliation = clear_reconciliation();
        if position {
            reconciliation.position_setup_ids.push("SETUP-14".to_owned());
        } else {
            reconciliation.open_order_setup_ids.push("SETUP-14".to_owned());
        }
        let mut broker = MockBroker::default();
        broker.reconciliation = Ok(reconciliation);
        let mut coordinator = ExecutionCoordinator::new(broker);
        coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();
        assert_eq!(
            coordinator.gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT")),
            Err(RejectionCode::DuplicateSetup)
        );
    }
}

#[test]
fn section_64_protected_bracket_capability_must_be_true_no_fallback_is_invented() {
    for capability in [Condition::False, Condition::Unknown] {
        let mut broker = MockBroker::default();
        broker.capabilities.protected_stop_limit_bracket = capability;
        let mut coordinator = ExecutionCoordinator::new(broker);
        coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();
        assert_eq!(
            coordinator.gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT")),
            Err(RejectionCode::BrokerUnsafe)
        );
        assert_eq!(coordinator.adapter().submit_calls, 0);
    }
}

#[test]
fn sections_109_110_122_gate_recalculates_final_authority_and_promotes_only_opaque_copy() {
    let mut coordinator = registered();
    let raw = proposal("SETUP-14", "MNQ");
    assert_eq!(raw.execution_pass, Condition::Unknown);

    let permit = coordinator.gate(&raw, context("MNQ_AGENT")).unwrap();
    assert_eq!(raw.execution_pass, Condition::Unknown);
    assert_eq!(permit.proposal().execution_pass, Condition::True);

    let mut manually_promoted = proposal("SETUP-MANUAL", "MNQ");
    manually_promoted.execution_pass = Condition::True;
    let mut other = ExecutionCoordinator::new(MockBroker::default());
    other.register_setup("SETUP-MANUAL", "MNQ_AGENT").unwrap();
    assert_eq!(
        other.gate(&manually_promoted, context("MNQ_AGENT")),
        Err(RejectionCode::ProcessError)
    );
}

#[test]
fn section_109_llm_strategy_risk_and_portfolio_authority_each_fail_closed() {
    for field in 0..4 {
        let mut coordinator = registered();
        let mut gate_context = context("MNQ_AGENT");
        match field {
            0 => gate_context.authority.llm_setup_pass = Condition::Unknown,
            1 => gate_context.authority.strategy_validator_pass = Condition::False,
            2 => gate_context.authority.risk_engine_pass = Condition::Unknown,
            3 => gate_context.authority.portfolio_coordinator_pass = Condition::False,
            _ => unreachable!(),
        }
        assert!(
            coordinator
                .gate(&proposal("SETUP-14", "MNQ"), gate_context)
                .is_err()
        );
    }
}

#[test]
fn section_122_other_mandatory_strategy_condition_cannot_be_bypassed_by_execution_gate() {
    let mut coordinator = registered();
    let mut gate_context = context("MNQ_AGENT");
    gate_context.production_conditions.news_blackout_clear = Condition::False;
    assert_eq!(
        coordinator.gate(&proposal("SETUP-14", "MNQ"), gate_context),
        Err(RejectionCode::ProcessError)
    );
}

#[test]
fn sections_8_64_pending_submission_is_exactly_once_and_does_not_consume_setup() {
    let mut coordinator = registered();
    let raw = proposal("SETUP-14", "MNQ");
    let permit = coordinator.gate(&raw, context("MNQ_AGENT")).unwrap();
    let status = coordinator.submit(permit).unwrap();

    assert_eq!(
        status,
        ExecutionStatus::Pending {
            broker_order_id: "BROKER-1".to_owned()
        }
    );
    assert_eq!(coordinator.adapter().submit_calls, 1);
    let submitted = coordinator.adapter().last_submission.as_ref().unwrap();
    assert_eq!(submitted.setup_id, raw.setup_id);
    assert_eq!(submitted.instrument, raw.instrument);
    assert_eq!(submitted.side, raw.side);
    assert_eq!(submitted.order_type, raw.order_type);
    assert_eq!(submitted.entry_trigger, raw.entry_trigger);
    assert_eq!(submitted.entry_limit, raw.entry_limit);
    assert_eq!(submitted.stop, raw.stop);
    assert_eq!(submitted.target, raw.target);
    assert_eq!(submitted.size, raw.size);
    assert_eq!(submitted.execution_pass, Condition::True);

    let registry = coordinator.setup_status("SETUP-14").unwrap();
    assert!(!registry.consumed);
    assert!(registry.order_active_or_reserved);
    assert_eq!(registry.broker_order_id.as_deref(), Some("BROKER-1"));
}

#[test]
fn section_8_immediate_fill_consumes_setup_and_section_64_confirms_stop() {
    let mut broker = MockBroker::default();
    broker.submission = Ok(BrokerSubmission {
        broker_order_id: "FILLED-1".to_owned(),
        entry_filled: true,
    });
    broker.stop_state = Ok(Condition::True);
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();

    let permit = coordinator
        .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
        .unwrap();
    assert_eq!(
        coordinator.submit(permit).unwrap(),
        ExecutionStatus::FilledProtected {
            broker_order_id: "FILLED-1".to_owned()
        }
    );
    let registry = coordinator.setup_status("SETUP-14").unwrap();
    assert!(registry.consumed);
    assert!(!registry.order_active_or_reserved);
    assert_eq!(coordinator.adapter().stop_calls, 1);
    assert_eq!(coordinator.adapter().flatten_calls, 0);
    assert_eq!(coordinator.execution_engine_safe(), Condition::True);

    assert_eq!(
        coordinator.gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT")),
        Err(RejectionCode::DuplicateSetup)
    );
}

#[test]
fn sections_8_64_later_fill_consumes_only_when_fill_becomes_true() {
    let mut coordinator = registered();
    let permit = coordinator
        .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
        .unwrap();
    coordinator.submit(permit).unwrap();

    coordinator.adapter_mut().fill_state = Ok(Condition::False);
    assert!(matches!(
        coordinator.reconcile_fill("SETUP-14").unwrap(),
        ExecutionStatus::Pending { .. }
    ));
    assert!(!coordinator.setup_status("SETUP-14").unwrap().consumed);

    coordinator.adapter_mut().fill_state = Ok(Condition::True);
    coordinator.adapter_mut().stop_state = Ok(Condition::True);
    assert!(matches!(
        coordinator.reconcile_fill("SETUP-14").unwrap(),
        ExecutionStatus::FilledProtected { .. }
    ));
    assert!(coordinator.setup_status("SETUP-14").unwrap().consumed);
}

#[test]
fn section_64_false_or_unknown_stop_confirmation_flattens_and_disables_shared_engine() {
    for stop_state in [Condition::False, Condition::Unknown] {
        let mut broker = MockBroker::default();
        broker.submission = Ok(BrokerSubmission {
            broker_order_id: "FILLED-FAIL".to_owned(),
            entry_filled: true,
        });
        broker.stop_state = Ok(stop_state);
        let mut coordinator = ExecutionCoordinator::new(broker);
        coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();
        coordinator.register_setup("UNRELATED", "ES_AGENT").unwrap();

        let permit = coordinator
            .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
            .unwrap();
        assert_eq!(coordinator.submit(permit), Err(RejectionCode::BrokerUnsafe));
        assert!(coordinator.setup_status("SETUP-14").unwrap().consumed);
        assert_eq!(coordinator.adapter().flatten_calls, 1);
        assert_eq!(
            coordinator.adapter().last_flatten_instrument.as_deref(),
            Some("MNQ")
        );
        assert_eq!(coordinator.execution_engine_safe(), Condition::False);

        assert_eq!(
            coordinator.gate(&proposal("UNRELATED", "ES"), context("ES_AGENT")),
            Err(RejectionCode::BrokerUnsafe)
        );
    }
}

#[test]
fn section_64_stop_confirmation_adapter_error_also_flattens() {
    let mut broker = MockBroker::default();
    broker.submission = Ok(BrokerSubmission {
        broker_order_id: "FILLED-ERROR".to_owned(),
        entry_filled: true,
    });
    broker.stop_state = Err(MockError::Failure);
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();

    let permit = coordinator
        .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
        .unwrap();
    assert_eq!(coordinator.submit(permit), Err(RejectionCode::BrokerUnsafe));
    assert_eq!(coordinator.adapter().flatten_calls, 1);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
}

#[test]
fn section_64_flatten_failure_remains_broker_unsafe_and_consumed() {
    let mut broker = MockBroker::default();
    broker.submission = Ok(BrokerSubmission {
        broker_order_id: "FILLED-FLATTEN-FAIL".to_owned(),
        entry_filled: true,
    });
    broker.stop_state = Ok(Condition::False);
    broker.flatten_result = Err(MockError::Failure);
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();

    let permit = coordinator
        .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
        .unwrap();
    assert_eq!(coordinator.submit(permit), Err(RejectionCode::BrokerUnsafe));
    assert!(coordinator.setup_status("SETUP-14").unwrap().consumed);
    assert_eq!(coordinator.adapter().flatten_calls, 1);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
}

#[test]
fn submission_error_does_not_retry_and_disables_shared_execution_engine() {
    let mut broker = MockBroker::default();
    broker.submission = Err(MockError::Failure);
    let mut coordinator = ExecutionCoordinator::new(broker);
    coordinator.register_setup("SETUP-14", "MNQ_AGENT").unwrap();

    let permit = coordinator
        .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
        .unwrap();
    assert_eq!(coordinator.submit(permit), Err(RejectionCode::BrokerUnsafe));
    assert_eq!(coordinator.adapter().submit_calls, 1);
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
    let registry = coordinator.setup_status("SETUP-14").unwrap();
    assert!(!registry.consumed);
    assert!(registry.order_active_or_reserved);
}

#[test]
fn unknown_later_fill_state_disables_engine_without_inventing_flatten_before_confirmed_fill() {
    let mut coordinator = registered();
    let permit = coordinator
        .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
        .unwrap();
    coordinator.submit(permit).unwrap();
    coordinator.adapter_mut().fill_state = Ok(Condition::Unknown);

    assert_eq!(
        coordinator.reconcile_fill("SETUP-14"),
        Err(RejectionCode::BrokerUnsafe)
    );
    assert_eq!(coordinator.execution_engine_safe(), Condition::False);
    assert_eq!(coordinator.adapter().flatten_calls, 0);
    assert!(!coordinator.setup_status("SETUP-14").unwrap().consumed);
}

#[test]
fn pre_submit_reconciliation_change_rejects_without_transmitting() {
    let mut coordinator = registered();
    let permit = coordinator
        .gate(&proposal("SETUP-14", "MNQ"), context("MNQ_AGENT"))
        .unwrap();

    let mut changed = clear_reconciliation();
    changed.open_order_setup_ids.push("SETUP-14".to_owned());
    coordinator.adapter_mut().reconciliation = Ok(changed);
    assert_eq!(coordinator.submit(permit), Err(RejectionCode::DuplicateSetup));
    assert_eq!(coordinator.adapter().submit_calls, 0);
    assert_eq!(coordinator.execution_engine_safe(), Condition::True);
    assert!(!coordinator.setup_status("SETUP-14").unwrap().order_active_or_reserved);
}
