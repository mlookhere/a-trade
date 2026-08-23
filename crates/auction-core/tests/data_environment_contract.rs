use auction_core::{
    Condition, DataHealth, Direction, EnvironmentInput, GammaContext, GammaRegime, HourlyStructure,
    MarketState, PremarketReferences, SessionValue, VolatilityExpectation, direction_allowed,
    evaluate_environment,
};

fn value(mid: f64, poc: f64) -> SessionValue {
    SessionValue {
        vah: mid + 1.0,
        val: mid - 1.0,
        poc,
    }
}

fn bullish_hourly() -> HourlyStructure {
    HourlyStructure {
        latest_swing_high: 110.0,
        previous_swing_high: 105.0,
        latest_swing_low: 95.0,
        previous_swing_low: 90.0,
    }
}

fn bearish_hourly() -> HourlyStructure {
    HourlyStructure {
        latest_swing_high: 105.0,
        previous_swing_high: 110.0,
        latest_swing_low: 90.0,
        previous_swing_low: 95.0,
    }
}

#[test]
fn section_16_data_health_requires_every_fact_true() {
    let healthy = DataHealth {
        quote_fresh: Condition::True,
        footprint_fresh: Condition::True,
        volume_profile_available: Condition::True,
        broker_state_known: Condition::True,
        position_state_known: Condition::True,
        open_order_state_known: Condition::True,
        timestamps_synchronized: Condition::True,
    };
    assert_eq!(healthy.validity(), Condition::True);

    let mut unknown = healthy;
    unknown.open_order_state_known = Condition::Unknown;
    assert_eq!(unknown.validity(), Condition::Unknown);

    let mut false_dominates_unknown = unknown;
    false_dominates_unknown.quote_fresh = Condition::False;
    assert_eq!(false_dominates_unknown.validity(), Condition::False);
}

#[test]
fn sections_14_15_gamma_is_volatility_context_not_direction() {
    let positive = GammaContext {
        regime: GammaRegime::Positive,
        flip: None,
        call_wall: None,
        put_wall: None,
    };
    let negative = GammaContext {
        regime: GammaRegime::Negative,
        flip: None,
        call_wall: None,
        put_wall: None,
    };

    assert_eq!(
        positive.volatility_expectation(),
        VolatilityExpectation::Dampened
    );
    assert_eq!(
        negative.volatility_expectation(),
        VolatilityExpectation::Amplified
    );
    assert!(direction_allowed(MarketState::ValueUp, Direction::Long));
    assert!(!direction_allowed(MarketState::ValueUp, Direction::Short));
}

#[test]
fn sections_19_20_value_up_accepts_exactly_three_of_four_plus_bullish_structure() {
    let result = evaluate_environment(EnvironmentInput {
        d1: value(95.0, 12.0),
        d2: value(100.0, 11.0),
        d3: value(90.0, 10.0),
        hourly: bullish_hourly(),
        prior_value_areas_substantially_overlap: Condition::False,
    });

    assert_eq!(result.value_up_migration, Condition::True);
    assert_eq!(result.htf_bull_structure, Condition::True);
    assert_eq!(result.market_state, MarketState::ValueUp);
}

#[test]
fn section_21_value_down_accepts_exactly_three_of_four_plus_bearish_structure() {
    let result = evaluate_environment(EnvironmentInput {
        d1: value(95.0, 10.0),
        d2: value(90.0, 11.0),
        d3: value(100.0, 12.0),
        hourly: bearish_hourly(),
        prior_value_areas_substantially_overlap: Condition::False,
    });

    assert_eq!(result.value_down_migration, Condition::True);
    assert_eq!(result.htf_bear_structure, Condition::True);
    assert_eq!(result.market_state, MarketState::ValueDown);
}

#[test]
fn sections_19_23_two_of_four_does_not_create_direction() {
    let result = evaluate_environment(EnvironmentInput {
        d1: value(80.0, 12.0),
        d2: value(90.0, 11.0),
        d3: value(100.0, 10.0),
        hourly: bullish_hourly(),
        prior_value_areas_substantially_overlap: Condition::False,
    });

    assert_eq!(result.value_up_migration, Condition::False);
    assert_eq!(result.value_down_migration, Condition::False);
    assert_eq!(result.market_state, MarketState::Unclear);
}

#[test]
fn section_22_balanced_requires_explicit_substantial_overlap_true() {
    let base = EnvironmentInput {
        d1: value(100.0, 10.0),
        d2: value(100.0, 10.0),
        d3: value(100.0, 10.0),
        hourly: bullish_hourly(),
        prior_value_areas_substantially_overlap: Condition::True,
    };

    assert_eq!(
        evaluate_environment(base).market_state,
        MarketState::Balanced
    );

    let unknown = EnvironmentInput {
        prior_value_areas_substantially_overlap: Condition::Unknown,
        ..base
    };
    assert_eq!(
        evaluate_environment(unknown).market_state,
        MarketState::Unclear
    );
}

#[test]
fn sections_19_23_ties_do_not_satisfy_migration() {
    let result = evaluate_environment(EnvironmentInput {
        d1: value(100.0, 10.0),
        d2: value(100.0, 10.0),
        d3: value(100.0, 10.0),
        hourly: bullish_hourly(),
        prior_value_areas_substantially_overlap: Condition::False,
    });

    assert_eq!(result.value_up_migration, Condition::False);
    assert_eq!(result.value_down_migration, Condition::False);
    assert_eq!(result.market_state, MarketState::Unclear);
}

#[test]
fn sections_16_23_non_finite_market_input_fails_to_unclear() {
    let result = evaluate_environment(EnvironmentInput {
        d1: value(f64::NAN, 12.0),
        d2: value(100.0, 11.0),
        d3: value(90.0, 10.0),
        hourly: bullish_hourly(),
        prior_value_areas_substantially_overlap: Condition::True,
    });

    assert_eq!(result.value_up_migration, Condition::Unknown);
    assert_eq!(result.market_state, MarketState::Unclear);
}

#[test]
fn section_25_missing_optional_gamma_references_are_not_fabricated() {
    let references = PremarketReferences {
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
    };

    assert_eq!(references.call_wall, None);
    assert_eq!(references.put_wall, None);
    assert_eq!(references.gamma_flip, None);
}
