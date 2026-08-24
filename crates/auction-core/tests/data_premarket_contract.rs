use auction_core::{
    AuthorizationInputs, Condition, DataCycleReadiness, DataHealth, EtTime, FrozenPremarketScenario,
    PremarketPlanComponents, PremarketReferences, PremarketScenarioError, ProductionConditions,
    RequiredMarketDataStatus, RequiredVolumeProfileStatus, trade_authorized,
};

fn true_health() -> DataHealth {
    DataHealth {
        quote_fresh: Condition::True,
        footprint_fresh: Condition::True,
        volume_profile_available: Condition::True,
        broker_state_known: Condition::True,
        position_state_known: Condition::True,
        open_order_state_known: Condition::True,
        timestamps_synchronized: Condition::True,
    }
}

fn true_market_data() -> RequiredMarketDataStatus {
    RequiredMarketDataStatus {
        current_bid: Condition::True,
        current_ask: Condition::True,
        last_trade: Condition::True,
        ohlcv_1m: Condition::True,
        ohlcv_5m: Condition::True,
        ohlcv_15m: Condition::True,
        ohlcv_1h: Condition::True,
        ohlcv_4h: Condition::True,
    }
}

fn true_profile() -> RequiredVolumeProfileStatus {
    RequiredVolumeProfileStatus {
        vah: Condition::True,
        val: Condition::True,
        poc: Condition::True,
        volume_at_price: Condition::True,
        low_volume_nodes: Condition::True,
        high_volume_nodes: Condition::True,
        same_methodology_every_session: Condition::True,
        methodology_unchanged_intraday: Condition::True,
    }
}

fn true_plan() -> PremarketPlanComponents {
    PremarketPlanComponents {
        market_environment_established: Condition::True,
        direction_permission_established: Condition::True,
        relevant_swing_structure_established: Condition::True,
        fib_location_established: Condition::True,
        gamma_regime_established: Condition::True,
        structural_targets_established: Condition::True,
        invalidations_established: Condition::True,
    }
}

fn references() -> PremarketReferences {
    PremarketReferences {
        reference_vah: 101.0,
        reference_val: 99.0,
        reference_poc: 100.0,
        prior_rth_high: 103.0,
        prior_rth_low: 97.0,
        overnight_high: 102.0,
        overnight_low: 98.0,
        call_wall: None,
        put_wall: None,
        gamma_flip: None,
    }
}

fn all_true_production(data_valid: Condition, plan_complete: Condition) -> ProductionConditions {
    ProductionConditions {
        time_valid: Condition::True,
        data_valid,
        premarket_plan_complete: plan_complete,
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
        broker_safe: Condition::True,
        execution_engine_safe: Condition::True,
    }
}

fn all_true_authority() -> AuthorizationInputs {
    AuthorizationInputs {
        llm_setup_pass: Condition::True,
        strategy_validator_pass: Condition::True,
        risk_engine_pass: Condition::True,
        portfolio_coordinator_pass: Condition::True,
        execution_gate_pass: Condition::True,
    }
}

#[test]
fn section_11_required_market_data_defaults_unknown_and_requires_every_family() {
    assert_eq!(
        RequiredMarketDataStatus::default().readiness(),
        Condition::Unknown
    );

    let ready = true_market_data();
    assert_eq!(ready.readiness(), Condition::True);

    let mut unknown = ready;
    unknown.ohlcv_4h = Condition::Unknown;
    assert_eq!(unknown.readiness(), Condition::Unknown);

    let mut false_dominates = unknown;
    false_dominates.current_bid = Condition::False;
    assert_eq!(false_dominates.readiness(), Condition::False);
}

#[test]
fn section_13_profile_data_requires_all_fields_and_stable_methodology() {
    assert_eq!(
        RequiredVolumeProfileStatus::default().readiness(),
        Condition::Unknown
    );

    let ready = true_profile();
    assert_eq!(ready.readiness(), Condition::True);

    let mut intraday_change = ready;
    intraday_change.methodology_unchanged_intraday = Condition::False;
    assert_eq!(intraday_change.readiness(), Condition::False);

    let mut cross_session_unknown = ready;
    cross_session_unknown.same_methodology_every_session = Condition::Unknown;
    assert_eq!(cross_session_unknown.readiness(), Condition::Unknown);
}

#[test]
fn sections_11_13_16_combined_data_readiness_fails_closed() {
    let ready = DataCycleReadiness {
        health: true_health(),
        market_data: true_market_data(),
        volume_profile: true_profile(),
    };
    assert_eq!(ready.validity(), Condition::True);

    let mut stale_or_unknown = ready;
    stale_or_unknown.health.open_order_state_known = Condition::Unknown;
    assert_eq!(stale_or_unknown.validity(), Condition::Unknown);

    let mut missing_required_stream = ready;
    missing_required_stream.market_data.ohlcv_15m = Condition::False;
    assert_eq!(missing_required_stream.validity(), Condition::False);
}

#[test]
fn section_25_required_references_are_valid_while_optional_gamma_may_be_absent() {
    let valid = references();
    assert_eq!(valid.validity(), Condition::True);
    assert_eq!(valid.call_wall, None);
    assert_eq!(valid.put_wall, None);
    assert_eq!(valid.gamma_flip, None);

    let mut invalid_range = valid;
    invalid_range.reference_vah = 98.0;
    invalid_range.reference_val = 99.0;
    assert_eq!(invalid_range.validity(), Condition::False);

    let mut invalid_optional = valid;
    invalid_optional.gamma_flip = Some(f64::NAN);
    assert_eq!(invalid_optional.validity(), Condition::False);
}

#[test]
fn section_17_plan_components_use_true_false_unknown_without_guessing() {
    assert_eq!(PremarketPlanComponents::default().readiness(), Condition::Unknown);
    assert_eq!(true_plan().readiness(), Condition::True);

    let mut unknown = true_plan();
    unknown.invalidations_established = Condition::Unknown;
    assert_eq!(unknown.readiness(), Condition::Unknown);

    let mut false_dominates = unknown;
    false_dominates.fib_location_established = Condition::False;
    assert_eq!(false_dominates.readiness(), Condition::False);
}

#[test]
fn sections_17_25_33_scenario_freezes_at_092959_but_not_093000() {
    let frozen = FrozenPremarketScenario::freeze(
        "AGENT-MNQ",
        "MNQ",
        "SCENARIO-2026-08-24-MNQ",
        EtTime::from_hms(9, 29, 59).unwrap(),
        references(),
        true_plan(),
    )
    .unwrap();

    assert_eq!(frozen.agent_id(), "AGENT-MNQ");
    assert_eq!(frozen.instrument(), "MNQ");
    assert_eq!(frozen.scenario_id(), "SCENARIO-2026-08-24-MNQ");
    assert_eq!(frozen.premarket_plan_complete(), Condition::True);
    assert_eq!(frozen.references(), references());

    assert_eq!(
        FrozenPremarketScenario::freeze(
            "AGENT-MNQ",
            "MNQ",
            "SCENARIO-LATE",
            EtTime::from_hms(9, 30, 0).unwrap(),
            references(),
            true_plan(),
        ),
        Err(PremarketScenarioError::FreezeAtOrAfterOpen)
    );
}

#[test]
fn sections_17_33_incomplete_unknown_or_unidentified_scenario_cannot_freeze() {
    let before_open = EtTime::from_hms(9, 0, 0).unwrap();
    assert_eq!(
        FrozenPremarketScenario::freeze(
            "",
            "MNQ",
            "SCENARIO",
            before_open,
            references(),
            true_plan(),
        ),
        Err(PremarketScenarioError::MissingAgentId)
    );
    assert_eq!(
        FrozenPremarketScenario::freeze(
            "AGENT",
            "",
            "SCENARIO",
            before_open,
            references(),
            true_plan(),
        ),
        Err(PremarketScenarioError::MissingInstrument)
    );
    assert_eq!(
        FrozenPremarketScenario::freeze(
            "AGENT",
            "MNQ",
            "",
            before_open,
            references(),
            true_plan(),
        ),
        Err(PremarketScenarioError::MissingScenarioId)
    );

    let mut incomplete = true_plan();
    incomplete.structural_targets_established = Condition::False;
    assert_eq!(
        FrozenPremarketScenario::freeze(
            "AGENT",
            "MNQ",
            "SCENARIO",
            before_open,
            references(),
            incomplete,
        ),
        Err(PremarketScenarioError::IncompletePlan)
    );

    let mut unknown = true_plan();
    unknown.gamma_regime_established = Condition::Unknown;
    assert_eq!(
        FrozenPremarketScenario::freeze(
            "AGENT",
            "MNQ",
            "SCENARIO",
            before_open,
            references(),
            unknown,
        ),
        Err(PremarketScenarioError::UnknownPlan)
    );
}

#[test]
fn sections_17_25_invalid_locked_references_cannot_produce_complete_plan() {
    let mut invalid = references();
    invalid.overnight_high = 97.0;
    invalid.overnight_low = 98.0;

    assert_eq!(
        FrozenPremarketScenario::freeze(
            "AGENT",
            "MNQ",
            "SCENARIO",
            EtTime::from_hms(9, 0, 0).unwrap(),
            invalid,
            true_plan(),
        ),
        Err(PremarketScenarioError::InvalidReferences)
    );
}

#[test]
fn sections_11_13_16_17_109_122_data_and_frozen_plan_feed_existing_authorization() {
    let data = DataCycleReadiness {
        health: true_health(),
        market_data: true_market_data(),
        volume_profile: true_profile(),
    };
    let scenario = FrozenPremarketScenario::freeze(
        "AGENT-MNQ",
        "MNQ",
        "SCENARIO",
        EtTime::from_hms(9, 29, 59).unwrap(),
        references(),
        true_plan(),
    )
    .unwrap();

    assert!(trade_authorized(
        all_true_production(data.validity(), scenario.premarket_plan_complete()),
        all_true_authority(),
    ));

    assert!(!trade_authorized(
        all_true_production(Condition::Unknown, scenario.premarket_plan_complete()),
        all_true_authority(),
    ));
    assert!(!trade_authorized(
        all_true_production(data.validity(), Condition::Unknown),
        all_true_authority(),
    ));
}
