use auction_core::{
    Condition, Direction, FootprintCandle5m, FootprintLevel, ManagementDirective, SetupState,
    SetupStateError, defensive_stop_candidate_allowed, effort_without_value_reclaim,
    favorable_effort_failure, favorable_effort_successful, final_target_directive, one_r_reached,
    reference_management_directive, structural_trail, value_reclaimed,
};

#[derive(Clone, Copy)]
struct CandleSpec {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    delta: i64,
    buy_imbalance_ratio: Option<f64>,
    sell_imbalance_ratio: Option<f64>,
    completed: bool,
}

fn candle(spec: CandleSpec) -> FootprintCandle5m<'static> {
    let price = (spec.high + spec.low) / 2.0;
    let (bid_volume, ask_volume) = if spec.delta >= 0 {
        (40_u64, 60_u64)
    } else {
        (60_u64, 40_u64)
    };
    let levels = Box::leak(Box::new([FootprintLevel {
        price,
        bid_volume,
        ask_volume,
        delta: spec.delta,
        buy_imbalance_ratio: spec.buy_imbalance_ratio,
        sell_imbalance_ratio: spec.sell_imbalance_ratio,
    }]));
    FootprintCandle5m {
        open: spec.open,
        high: spec.high,
        low: spec.low,
        close: spec.close,
        total_volume: 100,
        candle_delta: spec.delta,
        volume_poc: price,
        levels,
        completed: spec.completed,
    }
}

fn long_candle(
    close: f64,
    delta: i64,
    buy_imbalance_ratio: Option<f64>,
) -> FootprintCandle5m<'static> {
    candle(CandleSpec {
        open: 99.5,
        high: 100.5,
        low: 99.0,
        close,
        delta,
        buy_imbalance_ratio,
        sell_imbalance_ratio: None,
        completed: true,
    })
}

fn short_candle(
    close: f64,
    delta: i64,
    sell_imbalance_ratio: Option<f64>,
) -> FootprintCandle5m<'static> {
    candle(CandleSpec {
        open: 105.5,
        high: 106.0,
        low: 104.5,
        close,
        delta,
        buy_imbalance_ratio: None,
        sell_imbalance_ratio,
        completed: true,
    })
}

#[test]
fn sections_74_77_long_value_reclaim_includes_exact_val_boundary() {
    let below = long_candle(99.99, 10, None);
    let equal = long_candle(100.0, 10, None);
    assert_eq!(
        value_reclaimed(Direction::Long, below, 100.0),
        Condition::False
    );
    assert_eq!(
        value_reclaimed(Direction::Long, equal, 100.0),
        Condition::True
    );
}

#[test]
fn section_94_short_value_reclaim_mirrors_at_exact_vah_boundary() {
    let above = short_candle(105.01, -10, None);
    let equal = short_candle(105.0, -10, None);
    assert_eq!(
        value_reclaimed(Direction::Short, above, 105.0),
        Condition::False
    );
    assert_eq!(
        value_reclaimed(Direction::Short, equal, 105.0),
        Condition::True
    );
}

#[test]
fn section_75_preserves_not_above_wording_independently_from_reclaim_equality() {
    let first = long_candle(99.8, 20, Some(4.0));
    let second = long_candle(100.0, 15, None);
    assert_eq!(
        effort_without_value_reclaim(Direction::Long, first, second, 100.0),
        Condition::True
    );
    assert_eq!(
        value_reclaimed(Direction::Long, second, 100.0),
        Condition::True
    );

    let above = long_candle(100.01, 15, None);
    assert_eq!(
        effort_without_value_reclaim(Direction::Long, first, above, 100.0),
        Condition::False
    );
}

#[test]
fn sections_75_41_exact_buy_imbalance_boundary_is_four_point_zero() {
    let first = long_candle(99.8, 20, Some(4.0));
    let second = long_candle(99.9, 10, None);
    assert_eq!(
        effort_without_value_reclaim(Direction::Long, first, second, 100.0),
        Condition::True
    );

    let below_threshold = long_candle(99.8, 20, Some(3.999));
    assert_eq!(
        effort_without_value_reclaim(Direction::Long, below_threshold, second, 100.0),
        Condition::False
    );
}

#[test]
fn section_94_mirrors_two_candle_seller_effort_without_reclaim() {
    let first = short_candle(105.2, -20, Some(4.0));
    let second = short_candle(105.0, -15, None);
    assert_eq!(
        effort_without_value_reclaim(Direction::Short, first, second, 105.0),
        Condition::True
    );
    assert_eq!(
        value_reclaimed(Direction::Short, second, 105.0),
        Condition::True
    );

    let below = short_candle(104.99, -15, None);
    assert_eq!(
        effort_without_value_reclaim(Direction::Short, first, below, 105.0),
        Condition::False
    );
}

#[test]
fn position_management_rejects_incomplete_or_invalid_candles_as_unknown() {
    let incomplete = candle(CandleSpec {
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 100.0,
        delta: 20,
        buy_imbalance_ratio: Some(4.0),
        sell_imbalance_ratio: None,
        completed: false,
    });
    let valid = long_candle(99.8, 10, None);

    assert_eq!(
        value_reclaimed(Direction::Long, incomplete, 100.0),
        Condition::Unknown
    );
    assert_eq!(
        effort_without_value_reclaim(Direction::Long, incomplete, valid, 100.0),
        Condition::Unknown
    );
    assert_eq!(
        favorable_effort_successful(Direction::Long, incomplete, valid),
        Condition::Unknown
    );
    assert_eq!(
        favorable_effort_failure(Direction::Long, incomplete, valid),
        Condition::Unknown
    );
    assert_eq!(
        final_target_directive(Direction::Long, incomplete, 101.0),
        ManagementDirective::Unknown
    );
}

#[test]
fn sections_79_80_long_healthy_and_failure_candles_are_exact_and_exclusive() {
    let previous = candle(CandleSpec {
        open: 100.0,
        high: 101.0,
        low: 99.5,
        close: 100.7,
        delta: 5,
        buy_imbalance_ratio: None,
        sell_imbalance_ratio: None,
        completed: true,
    });
    let healthy = candle(CandleSpec {
        open: 100.6,
        high: 101.2,
        low: 100.0,
        close: 100.8,
        delta: 20,
        buy_imbalance_ratio: Some(4.0),
        sell_imbalance_ratio: None,
        completed: true,
    });
    let failure = candle(CandleSpec {
        open: 100.6,
        high: 101.0,
        low: 99.8,
        close: 100.6,
        delta: 20,
        buy_imbalance_ratio: Some(4.0),
        sell_imbalance_ratio: None,
        completed: true,
    });

    assert_eq!(
        favorable_effort_successful(Direction::Long, healthy, previous),
        Condition::True
    );
    assert_eq!(
        favorable_effort_failure(Direction::Long, healthy, previous),
        Condition::False
    );
    assert_eq!(
        favorable_effort_successful(Direction::Long, failure, previous),
        Condition::False
    );
    assert_eq!(
        favorable_effort_failure(Direction::Long, failure, previous),
        Condition::True
    );
}

#[test]
fn section_94_mirrors_healthy_seller_and_seller_failure() {
    let previous = candle(CandleSpec {
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 99.5,
        delta: -5,
        buy_imbalance_ratio: None,
        sell_imbalance_ratio: None,
        completed: true,
    });
    let healthy = candle(CandleSpec {
        open: 99.5,
        high: 100.0,
        low: 98.8,
        close: 99.3,
        delta: -20,
        buy_imbalance_ratio: None,
        sell_imbalance_ratio: Some(4.0),
        completed: true,
    });
    let failure = candle(CandleSpec {
        open: 99.5,
        high: 100.5,
        low: 99.0,
        close: 99.6,
        delta: -20,
        buy_imbalance_ratio: None,
        sell_imbalance_ratio: Some(4.0),
        completed: true,
    });

    assert_eq!(
        favorable_effort_successful(Direction::Short, healthy, previous),
        Condition::True
    );
    assert_eq!(
        favorable_effort_failure(Direction::Short, healthy, previous),
        Condition::False
    );
    assert_eq!(
        favorable_effort_successful(Direction::Short, failure, previous),
        Condition::False
    );
    assert_eq!(
        favorable_effort_failure(Direction::Short, failure, previous),
        Condition::True
    );
}

#[test]
fn section_81_exact_one_r_boundary_is_inclusive_for_long_and_short() {
    assert_eq!(
        one_r_reached(Direction::Long, 100.0, 98.0, 101.999),
        Condition::False
    );
    assert_eq!(
        one_r_reached(Direction::Long, 100.0, 98.0, 102.0),
        Condition::True
    );
    assert_eq!(
        one_r_reached(Direction::Short, 100.0, 102.0, 98.001),
        Condition::False
    );
    assert_eq!(
        one_r_reached(Direction::Short, 100.0, 102.0, 98.0),
        Condition::True
    );
    assert_eq!(
        one_r_reached(Direction::Long, 100.0, 101.0, 102.0),
        Condition::Unknown
    );
}

#[test]
fn sections_76_81_long_structural_trail_requires_all_gates_and_strict_tightening() {
    let trail = structural_trail(
        Direction::Long,
        Condition::True,
        Condition::True,
        Condition::True,
        101.0,
        0.25,
        99.0,
    );
    assert_eq!(trail.candidate_stop, Some(100.75));
    assert_eq!(trail.apply, Condition::True);

    let equal_stop = structural_trail(
        Direction::Long,
        Condition::True,
        Condition::True,
        Condition::True,
        99.25,
        0.25,
        99.0,
    );
    assert_eq!(equal_stop.candidate_stop, Some(99.0));
    assert_eq!(equal_stop.apply, Condition::False);

    let unknown_structure = structural_trail(
        Direction::Long,
        Condition::True,
        Condition::True,
        Condition::Unknown,
        101.0,
        0.25,
        99.0,
    );
    assert_eq!(unknown_structure.apply, Condition::Unknown);
}

#[test]
fn section_94_short_structural_trail_mirrors_and_never_widens() {
    let trail = structural_trail(
        Direction::Short,
        Condition::True,
        Condition::True,
        Condition::True,
        99.0,
        0.25,
        101.0,
    );
    assert_eq!(trail.candidate_stop, Some(99.25));
    assert_eq!(trail.apply, Condition::True);

    let widening = structural_trail(
        Direction::Short,
        Condition::True,
        Condition::True,
        Condition::True,
        101.0,
        0.25,
        101.0,
    );
    assert_eq!(widening.candidate_stop, Some(101.25));
    assert_eq!(widening.apply, Condition::False);
}

#[test]
fn section_76_defensive_stop_candidate_requires_explicit_structure_permission_and_tightening() {
    assert_eq!(
        defensive_stop_candidate_allowed(Direction::Long, 99.0, 100.0, Condition::True),
        Condition::True
    );
    assert_eq!(
        defensive_stop_candidate_allowed(Direction::Long, 99.0, 98.0, Condition::True),
        Condition::False
    );
    assert_eq!(
        defensive_stop_candidate_allowed(Direction::Short, 101.0, 100.0, Condition::True),
        Condition::True
    );
    assert_eq!(
        defensive_stop_candidate_allowed(Direction::Short, 101.0, 102.0, Condition::True),
        Condition::False
    );
    assert_eq!(
        defensive_stop_candidate_allowed(Direction::Long, 99.0, 100.0, Condition::Unknown),
        Condition::Unknown
    );
}

#[test]
fn sections_82_83_reference_management_never_auto_exits_or_reverses() {
    assert_eq!(
        reference_management_directive(Condition::True, Condition::False),
        ManagementDirective::MaintainOrTrail
    );
    assert_eq!(
        reference_management_directive(Condition::False, Condition::True),
        ManagementDirective::Tighten
    );
    assert_eq!(
        reference_management_directive(Condition::False, Condition::False),
        ManagementDirective::NoDirective
    );
    assert_eq!(
        reference_management_directive(Condition::Unknown, Condition::False),
        ManagementDirective::Unknown
    );
}

#[test]
fn sections_84_94_final_swing_target_touch_or_overshoot_exits_remaining() {
    let long_touch = candle(CandleSpec {
        open: 100.0,
        high: 102.0,
        low: 99.5,
        close: 101.0,
        delta: 10,
        buy_imbalance_ratio: None,
        sell_imbalance_ratio: None,
        completed: true,
    });
    let long_below = candle(CandleSpec {
        high: 101.99,
        ..CandleSpec {
            open: 100.0,
            high: 102.0,
            low: 99.5,
            close: 101.0,
            delta: 10,
            buy_imbalance_ratio: None,
            sell_imbalance_ratio: None,
            completed: true,
        }
    });
    assert_eq!(
        final_target_directive(Direction::Long, long_touch, 102.0),
        ManagementDirective::ExitRemaining
    );
    assert_eq!(
        final_target_directive(Direction::Long, long_touch, 101.5),
        ManagementDirective::ExitRemaining
    );
    assert_eq!(
        final_target_directive(Direction::Long, long_below, 102.0),
        ManagementDirective::NoDirective
    );

    let short_touch = candle(CandleSpec {
        open: 100.0,
        high: 100.5,
        low: 98.0,
        close: 99.0,
        delta: -10,
        buy_imbalance_ratio: None,
        sell_imbalance_ratio: None,
        completed: true,
    });
    assert_eq!(
        final_target_directive(Direction::Short, short_touch, 98.0),
        ManagementDirective::ExitRemaining
    );
    assert_eq!(
        final_target_directive(Direction::Short, short_touch, 98.5),
        ManagementDirective::ExitRemaining
    );
}

#[test]
fn sections_73_102_filled_position_management_closed_states_cannot_be_skipped() {
    assert_eq!(
        SetupState::Filled.advance(SetupState::PositionManagement),
        Ok(SetupState::PositionManagement)
    );
    assert_eq!(
        SetupState::PositionManagement.advance(SetupState::Closed),
        Ok(SetupState::Closed)
    );
    assert_eq!(
        SetupState::Filled.advance(SetupState::Closed),
        Err(SetupStateError::StateSkip)
    );
}
