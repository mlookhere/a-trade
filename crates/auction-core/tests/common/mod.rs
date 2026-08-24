#![allow(dead_code)]

use auction_core::{
    Condition, DataCycleReadiness, DataHealth, Direction, EnvironmentInput, EtTime,
    FootprintCandle5m, FootprintLevel, FrozenPremarketScenario, HourlyStructure, LocationEvent,
    LocationSetup, LongOrderflowSequence, MarketState, OrderProposal, OrderProposalInputs,
    ParticipationContext, ParticipationRule, PortfolioRiskLimits, PremarketPlanComponents,
    PremarketReferences, RequiredMarketDataStatus, RequiredVolumeProfileStatus, SessionValue,
    ShortOrderflowSequence, StrategyValidationInputs, StrategyValidationProof, SwingImpulse,
    build_order_proposal, validate_strategy,
};

pub const OWNER: &str = "MNQ_AGENT";
pub const INSTRUMENT: &str = "MNQ";

pub fn execution_config() -> auction_core::InstrumentExecutionConfig {
    auction_core::InstrumentExecutionConfig {
        tick_size: 0.25,
        max_entry_slippage_ticks: 4,
        stop_buffer_ticks: 2,
    }
}

pub fn risk_limits() -> PortfolioRiskLimits {
    PortfolioRiskLimits {
        max_portfolio_open_risk_percent: 0.02,
        max_cluster_open_risk_percent: 0.01,
    }
}

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

fn long_levels(
    total_volume: u64,
    delta: i64,
    bottom_percent: u64,
    sell_ratio: Option<f64>,
    buy_ratio: Option<f64>,
) -> [FootprintLevel; 4] {
    let bottom = total_volume * bottom_percent / 100;
    let remaining = total_volume - bottom;
    let second = remaining / 3;
    let third = remaining / 3;
    let fourth = remaining - second - third;
    [
        level(0.0, bottom, delta, None, sell_ratio),
        level(3.0, second, 0, None, None),
        level(7.0, third, 0, buy_ratio, None),
        level(10.0, fourth, 0, None, None),
    ]
}

fn short_levels(
    total_volume: u64,
    delta: i64,
    top_percent: u64,
    buy_ratio: Option<f64>,
    sell_ratio: Option<f64>,
) -> [FootprintLevel; 4] {
    let top = total_volume * top_percent / 100;
    let remaining = total_volume - top;
    let first = remaining / 3;
    let second = remaining / 3;
    let third = remaining - first - second;
    let low_delta = if delta < 0 { delta } else { 0 };
    let high_delta = if delta > 0 { delta } else { 0 };
    [
        level(0.0, first, low_delta, None, sell_ratio),
        level(3.0, second, 0, None, None),
        level(7.0, third, 0, None, None),
        level(10.0, top, high_delta, buy_ratio, None),
    ]
}

fn candle<'a>(
    levels: &'a [FootprintLevel],
    open: f64,
    close: f64,
    total_volume: u64,
    delta: i64,
    poc: f64,
) -> FootprintCandle5m<'a> {
    FootprintCandle5m {
        open,
        high: 10.0,
        low: 0.0,
        close,
        total_volume,
        candle_delta: delta,
        volume_poc: poc,
        levels,
        completed: true,
    }
}

pub fn valid_long_proof(setup_id: &str, owner: &str, instrument: &str) -> StrategyValidationProof {
    let location = long_location();
    let aggression_levels = long_levels(20_000, -100, 35, Some(4.0), None);
    let aggression = candle(&aggression_levels, 2.5, 6.0, 20_000, -100, 1.0);
    let dominance_levels = long_levels(20_000, 100, 10, None, Some(4.0));
    let dominance = candle(&dominance_levels, 4.0, 6.0, 20_000, 100, 7.0);
    let second_levels = [
        level(2.25, 5_000, -100, None, Some(4.0)),
        level(3.0, 5_000, 0, None, None),
        level(7.0, 5_000, 0, None, None),
        level(10.0, 5_000, 0, None, None),
    ];
    let second_test = FootprintCandle5m {
        open: 4.0,
        high: 10.0,
        low: 2.25,
        close: 3.0,
        total_volume: 20_000,
        candle_delta: -100,
        volume_poc: 7.0,
        levels: &second_levels,
        completed: true,
    };
    let reconfirm_levels = long_levels(20_000, 100, 10, None, Some(4.0));
    let reconfirmation = candle(&reconfirm_levels, 4.0, 7.0, 20_000, 100, 7.0);

    let mut sequence = LongOrderflowSequence::from_location_reached(&location).unwrap();
    assert_eq!(
        sequence.record_aggression(
            aggression,
            &[100; 20],
            mnq_participation(),
            Condition::True,
        ),
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

pub fn valid_short_proof(
    setup_id: &str,
    owner: &str,
    instrument: &str,
) -> StrategyValidationProof {
    let location = short_location();
    let aggression_levels = short_levels(20_000, 100, 35, Some(4.0), None);
    let aggression = candle(&aggression_levels, 7.5, 4.0, 20_000, 100, 9.0);
    let dominance_levels = short_levels(20_000, -100, 10, None, Some(4.0));
    let dominance = candle(&dominance_levels, 6.0, 4.0, 20_000, -100, 3.0);
    let second_levels = [
        level(0.0, 5_000, 0, None, None),
        level(3.0, 5_000, 0, None, None),
        level(6.0, 5_000, 0, None, None),
        level(7.75, 5_000, 100, Some(4.0), None),
    ];
    let second_test = FootprintCandle5m {
        open: 6.0,
        high: 7.75,
        low: 0.0,
        close: 7.0,
        total_volume: 20_000,
        candle_delta: 100,
        volume_poc: 7.0,
        levels: &second_levels,
        completed: true,
    };
    let reconfirm_levels = short_levels(20_000, -100, 10, None, Some(4.0));
    let reconfirmation = candle(&reconfirm_levels, 5.0, 3.0, 20_000, -100, 3.0);

    let mut sequence = ShortOrderflowSequence::from_location_reached(&location).unwrap();
    assert_eq!(
        sequence.record_aggression(
            aggression,
            &[100; 20],
            mnq_participation(),
            Condition::True,
        ),
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

pub fn valid_long_proposal(setup_id: &str, owner: &str, instrument: &str) -> OrderProposal {
    let proof = valid_long_proof(setup_id, owner, instrument);
    build_order_proposal(OrderProposalInputs {
        strategy: &proof,
        entry_limit: 10.5,
        target: 25.0,
        execution_config: execution_config(),
        account_equity: 100_000.0,
        risk_percent: 0.0025,
        tick_value: 1.25,
        commissions_per_contract: 1.0,
        slippage_reserve_per_contract: 1.0,
        current_open_portfolio_risk: 100.0,
        current_cluster_risk: 50.0,
        portfolio_limits: risk_limits(),
        cluster_id: "NASDAQ_CLUSTER",
        existing_cluster_exposure: &[],
    })
    .unwrap()
}
