use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission, Condition,
    Direction, EtTime, ExecutionCoordinator, ExecutionGateContext, LocationSetup, MarketState,
    OrderProposal, OrderType, PreExecutionAuthority, ProductionConditions, RejectionCode,
    StructureKey, SwingImpulse,
};

fn long_impulse(start_index: usize, end_index: usize) -> SwingImpulse {
    SwingImpulse {
        direction: Direction::Long,
        start_index,
        end_index,
        swing_low: 80.0,
        swing_high: 100.0,
    }
}

#[test]
fn sections_37_117_new_structure_can_start_during_entry_window_but_never_reuse_known_structure() {
    let known = StructureKey::from(long_impulse(2, 7));
    let candidate = long_impulse(8, 13);

    let setup = LocationSetup::from_new_structure(
        EtTime::from_hms(10, 30, 0).unwrap(),
        MarketState::ValueUp,
        candidate,
        90.0,
        0.25,
        &[known],
    )
    .unwrap();
    assert_eq!(setup.structure(), StructureKey::from(candidate));

    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(10, 30, 0).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[known, StructureKey::from(candidate)],
        )
        .is_none()
    );
}

#[test]
fn sections_3_37_117_new_structure_creation_obeys_exact_entry_cutoff_and_direction() {
    let candidate = long_impulse(8, 13);
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(10, 59, 59).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_some()
    );
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(11, 0, 0).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_none()
    );
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(9, 29, 59).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_none()
    );
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(10, 30, 0).unwrap(),
            MarketState::ValueDown,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_none()
    );
}

#[derive(Debug, Default)]
struct GateBroker;

impl BrokerAdapter for GateBroker {
    type Error = ();

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
            open_order_setup_ids: vec![],
            position_setup_ids: vec![],
        })
    }

    fn submit_protected(
        &mut self,
        _proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error> {
        unreachable!("gate-only hardening test must never submit")
    }

    fn entry_filled(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        unreachable!("gate-only hardening test must never reconcile fill")
    }

    fn hard_stop_confirmed(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        unreachable!("gate-only hardening test must never inspect protection")
    }

    fn flatten_instrument(&mut self, _instrument: &str) -> Result<(), Self::Error> {
        unreachable!("gate-only hardening test must never flatten")
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

fn proposal() -> OrderProposal {
    OrderProposal {
        setup_id: "SETUP-HARDEN".to_owned(),
        instrument: "MNQ".to_owned(),
        side: Direction::Long,
        order_type: OrderType::StopLimit,
        entry_trigger: 100.25,
        entry_limit: 100.50,
        stop: 98.50,
        target: 103.00,
        size: 1,
        risk_dollars: 100.0,
        risk_percent: 0.001,
        open_portfolio_risk_before: 0.0,
        open_portfolio_risk_after: 100.0,
        cluster_id: "NASDAQ_CLUSTER".to_owned(),
        cluster_risk_after: 100.0,
        strategy_pass: Condition::True,
        risk_pass: Condition::True,
        portfolio_pass: Condition::True,
        execution_pass: Condition::Unknown,
    }
}

fn ready_coordinator() -> ExecutionCoordinator<GateBroker> {
    let mut coordinator = ExecutionCoordinator::new(GateBroker);
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator
        .register_setup("SETUP-HARDEN", "MNQ_AGENT")
        .unwrap();
    coordinator
}

fn context(conditions: ProductionConditions) -> ExecutionGateContext<'static> {
    ExecutionGateContext {
        submitting_agent_id: "MNQ_AGENT",
        production_conditions: conditions,
        authority: PreExecutionAuthority {
            llm_setup_pass: Condition::True,
            strategy_validator_pass: Condition::True,
            risk_engine_pass: Condition::True,
            portfolio_coordinator_pass: Condition::True,
        },
    }
}

#[test]
fn sections_9_56_109_execution_gate_cannot_overwrite_upstream_duplicate_failure() {
    for condition in [Condition::False, Condition::Unknown] {
        let mut coordinator = ready_coordinator();
        let mut conditions = all_true_conditions();
        conditions.setup_not_duplicated = condition;
        assert_eq!(
            coordinator.gate(&proposal(), context(conditions)),
            Err(RejectionCode::ProcessError)
        );
    }
}

#[test]
fn section_56_execution_gate_cannot_overwrite_upstream_conflicting_order_failure() {
    for condition in [Condition::False, Condition::Unknown] {
        let mut coordinator = ready_coordinator();
        let mut conditions = all_true_conditions();
        conditions.no_conflicting_order = condition;
        assert_eq!(
            coordinator.gate(&proposal(), context(conditions)),
            Err(RejectionCode::ProcessError)
        );
    }
}
