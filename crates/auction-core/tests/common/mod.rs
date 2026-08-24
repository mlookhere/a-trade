#![allow(dead_code)]

use auction_core::{
    Condition, DataCycleReadiness, DataHealth, Direction, EnvironmentInput, EtTime,
    FootprintCandle5m, FootprintLevel, FrozenPremarketScenario, HourlyStructure, LocationEvent,
    LocationSetup, LongOrderflowSequence, MarketState, ParticipationContext, ParticipationRule,
    PremarketPlanComponents, PremarketReferences, RequiredMarketDataStatus,
    RequiredVolumeProfileStatus, SessionValue, ShortOrderflowSequence, StrategyValidationInputs,
    StrategyValidationProof, SwingImpulse, validate_strategy,
};

pub const OWNER: &str = "MNQ_AGENT";
pub const INSTRUMENT: &str = "MNQ";

pub fn true_data() -> DataCycleReadiness {
    DataCycleReadiness {
        health: DataHealth {
            quote_fresh: Condition::True,
            footprint_fresh: Condition::True,
            volume_profile_available: Condition::True,
            broker_state_known: Condition::True,
            position_state_known: Condition::True,
            open_order_state_known: Condition::True,
            timestamps_synchronized: Condition::True,
        },
        market_data: RequiredMarketDataStatus {
            current_bid: Condition::True,
            current_ask: Condition::True,
            last_trade: Condition::True,
            ohlcv_1m: Condition::True,
            ohlcv_5m: Condition::True,
            ohlcv_15m: Condition::True,
            ohlcv_1h: Condition::True,
            ohlcv_4h: Condition::True,
        },
        volume_profile: RequiredVolumeProfileStatus {
            vah: Condition::True,
            val: Condition::True,
            poc: Condition::True,
            volume_at_price: Condition::True,
            low_volume_nodes: Condition::True,
            high_volume_nodes: Condition::True,
            same_methodology_every_session: Condition::True,
            methodology_unchanged_intraday: Condition::True,
        },
    }
}

pub fn references() -> PremarketReferences {
    PremarketReferences {
        reference_vah: 6.0,
        reference_val: 4.0,
        reference_poc: 5.0,
        prior_rth_high: 10.0,
        prior_rth_low: 0.0,
        overnight_high: 9.0,
        overnight_low: 1.0,
        call_wall: None,
        put_wall: None,
        gamma_flip: None,
    }
}

pub fn frozen_scenario(owner: &str, instrument: &str) -> FrozenPremarketScenario {
    FrozenPremarketScenario::freeze(
        owner,
        instrument,
        "SCENARIO-SEALED",
        EtTime::from_hms(9, 0, 0).unwrap(),
        references(),
        PremarketPlanComponents {
            market_environment_established: Condition::True,
            direction_permission_established: Condition::True,
            relevant_swing_structure_established: Condition::True,
            fib_location_established: Condition::True,
            gamma_regime_established: Condition::True,
            structural_targets_established: Condition::True,
            invalidations_established: Condition::True,
        },
    )
    .unwrap()
}

pub fn long_environment() -> EnvironmentInput {
    EnvironmentInput {
        d1: SessionValue {
            vah: 8.0,
            val: 6.0,
            poc: 7.0,
        },
        d2: SessionValue {
            vah: 7.0,
            val: 5.0,
            poc: 6.0,
        },
        d3: SessionValue {
            vah: 6.0,
            val: 4.0,
            poc: 5.0,
        },
        hourly: HourlyStructure {
            latest_swing_high: 10.0,
            previous_swing_high: 9.0,
            latest_swing_low: 5.0,
            previous_swing_low: 4.0,
        },
        prior_value_areas_substantially_overlap: Condition::False,
    }
}

pub fn short_environment() -> EnvironmentInput {
    EnvironmentInput {
        d1: SessionValue {
            vah: 4.0,
            val: 2.0,
            poc: 3.0,
        },
        d2: SessionValue {
            vah: 5.0,
            val: 3.0,
            poc: 4.0,
        },
        d3: SessionValue {
            vah: 6.0,
            val: 4.0,
            poc: 5.0,
        },
        hourly: HourlyStructure {
            latest_swing_high: 8.0,
            previous_swing_high: 9.0,
            latest_swing_low: 2.0,
            previous_swing_low: 3.0,
        },
        prior_value_areas_substantially_overlap: Condition::False,
    }
}

pub fn long_location() -> LocationSetup {
    let mut location = LocationSetup::at_open(
        EtTime::from_hms(9, 30, 0).unwrap(),
        MarketState::ValueUp,
        SwingImpulse {
            direction: Direction::Long,
            start_index: 1,
            end_index: 6,
            swing_low: 0.0,
            swing_high: 10.0,
        },
        references().reference_val,
        0.25,
    )
    .unwrap();
    assert_eq!(location.observe_price(2.5), LocationEvent::Reached);
    location
}

pub fn short_location() -> LocationSetup {
    let mut location = LocationSetup::at_open(
        EtTime::from_hms(9, 30, 0).unwrap(),
        MarketState::ValueDown,
        SwingImpulse {
            direction: Direction::Short,
            start_index: 1,
            end_index: 6,
            swing_low: 0.0,
            swing_high: 10.0,
        },
        references().reference_vah,
        0.25,
    )
    .unwrap();
    assert_eq!(location.observe_price(8.0), LocationEvent::Reached);
    location
}

pub fn mnq_participation() -> ParticipationContext<'static> {
    ParticipationContext {
        rule: ParticipationRule::MnqSourceThreshold,
        prior_same_bucket_volumes: None,
    }
}

fn level(
    price: f64,
    volume: u64,
    delta: i64,
    buy_imbalance_ratio: Option<f64>,
    sell_imbalance_ratio: Option<f64>,
) -> FootprintLevel {
    FootprintLevel {
        price,
        bid_volume: volume / 2,
        ask_volume: volume - volume / 2,
        delta,
        buy_imbalance_ratio,
        sell_imbalance_ratio,
    }
}

fn candle<'a>(
    levels: &'a [FootprintLevel],
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    delta: i64,
    poc: f64,
) -> FootprintCandle5m<'a> {
    let total_volume = levels
        .iter()
        .map(|level| level.bid_volume + level.ask_volume)
        .sum();
    FootprintCandle5m {
        open,
        high,
        low,
        close,
        total_volume,
        candle_delta: delta,
        volume_poc: poc,
        levels,
        completed: true,
    }
}

pub fn valid_long_sequence(location: &LocationSetup) -> LongOrderflowSequence {
    let aggression_levels = [
        level(0.0, 7_000, -100, None, Some(4.0)),
        level(3.0, 4_333, 0, None, None),
        level(7.0, 4_333, 0, None, None),
        level(10.0, 4_334, 0, None, None),
    ];
    let dominance_levels = [
        level(0.0, 5_000, 0, None, None),
        level(3.0, 5_000, 0, None, None),
        level(7.0, 5_000, 100, Some(4.0), None),
        level(10.0, 5_000, 0, None, None),
    ];
    let second_levels = [
        level(2.25, 5_000, -100, None, Some(4.0)),
        level(3.0, 5_000, 0, None, None),
        level(7.0, 5_000, 0, None, None),
        level(10.0, 5_000, 0, None, None),
    ];
    let reconfirm_levels = dominance_levels;
    let aggression = candle(&aggression_levels, 2.5, 10.0, 0.0, 6.0, -100, 1.0);
    let dominance = candle(&dominance_levels, 4.0, 10.0, 0.0, 6.0, 100, 7.0);
    let second_test = candle(&second_levels, 4.0, 10.0, 2.25, 3.0, -100, 7.0);
    let reconfirmation = candle(&reconfirm_levels, 4.0, 10.0, 0.0, 7.0, 100, 7.0);

    let mut sequence = LongOrderflowSequence::from_location_reached(location).unwrap();
    assert_eq!(
        sequence.record_aggression(aggression, &[100; 20], mnq_participation(), Condition::True),
        Condition::True
    );
    assert_eq!(sequence.record_absorption(2.0), Condition::True);
    assert_eq!(
        sequence.record_first_dominance_shift(dominance, mnq_participation()),
        Condition::True
    );
    assert_eq!(sequence.begin_second_test(), Condition::True);
    assert_eq!(
        sequence.record_second_test(second_test, mnq_participation()),
        Condition::True
    );
    assert_eq!(sequence.confirm_second_failure(0.25), Condition::True);
    assert_eq!(
        sequence.record_reconfirmation(reconfirmation, mnq_participation()),
        Condition::True
    );
    sequence
}

pub fn valid_short_sequence(location: &LocationSetup) -> ShortOrderflowSequence {
    let aggression_levels = [
        level(0.0, 4_333, 0, None, None),
        level(3.0, 4_333, 0, None, None),
        level(7.0, 4_334, 0, None, None),
        level(10.0, 7_000, 100, Some(4.0), None),
    ];
    let dominance_levels = [
        level(0.0, 5_000, -100, None, Some(4.0)),
        level(3.0, 5_000, 0, None, None),
        level(7.0, 5_000, 0, None, None),
        level(10.0, 5_000, 0, None, None),
    ];
    let second_levels = [
        level(0.0, 5_000, 0, None, None),
        level(3.0, 5_000, 0, None, None),
        level(6.0, 5_000, 0, None, None),
        level(7.75, 5_000, 100, Some(4.0), None),
    ];
    let reconfirm_levels = dominance_levels;
    let aggression = candle(&aggression_levels, 7.5, 10.0, 0.0, 4.0, 100, 9.0);
    let dominance = candle(&dominance_levels, 6.0, 10.0, 0.0, 4.0, -100, 3.0);
    let second_test = candle(&second_levels, 6.0, 7.75, 0.0, 7.0, 100, 7.0);
    let reconfirmation = candle(&reconfirm_levels, 5.0, 10.0, 0.0, 3.0, -100, 3.0);

    let mut sequence = ShortOrderflowSequence::from_location_reached(location).unwrap();
    assert_eq!(
        sequence.record_aggression(aggression, &[100; 20], mnq_participation(), Condition::True),
        Condition::True
    );
    assert_eq!(sequence.record_absorption(8.0), Condition::True);
    assert_eq!(
        sequence.record_first_dominance_shift(dominance, mnq_participation()),
        Condition::True
    );
    assert_eq!(sequence.begin_second_test(), Condition::True);
    assert_eq!(
        sequence.record_second_test(second_test, mnq_participation()),
        Condition::True
    );
    assert_eq!(sequence.confirm_second_failure(0.25), Condition::True);
    assert_eq!(
        sequence.record_reconfirmation(reconfirmation, mnq_participation()),
        Condition::True
    );
    sequence
}

pub fn valid_long_proof(setup_id: &str, owner: &str, instrument: &str) -> StrategyValidationProof {
    let location = long_location();
    let sequence = valid_long_sequence(&location);
    let scenario = frozen_scenario(owner, instrument);
    validate_strategy(StrategyValidationInputs {
        setup_id,
        owner_agent_id: owner,
        instrument,
        now_et: EtTime::from_hms(10, 0, 0).unwrap(),
        data_readiness: true_data(),
        premarket_scenario: &scenario,
        environment: long_environment(),
        location: &location,
        orderflow: auction_core::OrderflowSequenceEvidence::Long(&sequence),
        news_schedule_valid: Condition::True,
        news_windows: &[],
    })
    .unwrap()
}

pub fn valid_short_proof(setup_id: &str, owner: &str, instrument: &str) -> StrategyValidationProof {
    let location = short_location();
    let sequence = valid_short_sequence(&location);
    let scenario = frozen_scenario(owner, instrument);
    validate_strategy(StrategyValidationInputs {
        setup_id,
        owner_agent_id: owner,
        instrument,
        now_et: EtTime::from_hms(10, 0, 0).unwrap(),
        data_readiness: true_data(),
        premarket_scenario: &scenario,
        environment: short_environment(),
        location: &location,
        orderflow: auction_core::OrderflowSequenceEvidence::Short(&sequence),
        news_schedule_valid: Condition::True,
        news_windows: &[],
    })
    .unwrap()
}
