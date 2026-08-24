mod common;

use auction_core::{
    Condition, FootprintCandle5m, FootprintLevel, SetupState, ShortOrderflowSequence,
    buyer_aggression, first_seller_dominance_shift, genuine_second_buyer_attempt,
    potential_buyer_absorption, second_failure_lower, second_test_has_real_buying,
    seller_reconfirmation,
};

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

fn levels(
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

fn prior_deltas(value: i64) -> [i64; 20] {
    [value; 20]
}

#[test]
fn sections_85_88_mirror_4x_upper_half_buyer_aggression_and_delta_median() {
    let below_data = levels(20_000, 100, 35, Some(3.99), None);
    let below = candle(&below_data, 7.5, 4.0, 20_000, 100, 9.0);
    assert_eq!(
        buyer_aggression(below, &prior_deltas(100), Condition::True),
        Condition::False
    );

    let boundary_data = levels(20_000, 100, 35, Some(4.0), None);
    let boundary = candle(&boundary_data, 7.5, 4.0, 20_000, 100, 9.0);
    assert_eq!(
        buyer_aggression(boundary, &prior_deltas(100), Condition::True),
        Condition::True
    );
    assert_eq!(
        buyer_aggression(boundary, &prior_deltas(101), Condition::True),
        Condition::False
    );
    assert_eq!(
        buyer_aggression(boundary, &prior_deltas(100)[..19], Condition::True),
        Condition::Unknown
    );
}

#[test]
fn section_85_mirrors_extreme_participation_to_top_quarter() {
    let pct_35_data = levels(20_000, 100, 35, Some(4.0), None);
    let pct_35 = candle(&pct_35_data, 7.5, 4.0, 20_000, 100, 3.0);
    assert_eq!(pct_35.aggression_at_short_extreme(), Condition::True);

    let pct_34_data = levels(20_000, 100, 34, Some(4.0), None);
    let pct_34 = candle(&pct_34_data, 7.5, 4.0, 20_000, 100, 3.0);
    assert_eq!(pct_34.aggression_at_short_extreme(), Condition::False);

    let poc_data = levels(20_000, 100, 10, Some(4.0), None);
    let poc_boundary = candle(&poc_data, 7.5, 4.0, 20_000, 100, 7.5);
    assert_eq!(poc_boundary.aggression_at_short_extreme(), Condition::True);
}

#[test]
fn sections_85_89_upper_wick_25pct_is_inclusive_and_unknown_failure_cannot_advance() {
    let data = levels(20_000, 100, 35, Some(4.0), None);
    let aggression = candle(&data, 7.5, 4.0, 20_000, 100, 9.0);
    assert_eq!(
        potential_buyer_absorption(aggression, Condition::True),
        Condition::True
    );

    let location = common::short_location();
    let mut sequence = ShortOrderflowSequence::from_location_reached(&location).unwrap();
    assert_eq!(
        sequence.record_aggression(
            aggression,
            &prior_deltas(100),
            common::mnq_participation(),
            Condition::Unknown
        ),
        Condition::Unknown
    );
    assert_eq!(sequence.state(), SetupState::LocationReached);
}

#[test]
fn section_90_seller_dominance_requires_strict_midpoint_break() {
    let sell_data = levels(20_000, -100, 10, None, Some(4.0));
    let valid = candle(&sell_data, 6.0, 4.0, 20_000, -100, 3.0);
    assert_eq!(
        first_seller_dominance_shift(valid, 5.0, Condition::True),
        Condition::True
    );

    let midpoint = candle(&sell_data, 6.0, 5.0, 20_000, -100, 3.0);
    assert_eq!(
        first_seller_dominance_shift(midpoint, 5.0, Condition::True),
        Condition::False
    );
}

#[test]
fn section_91_requires_genuine_real_buying_and_one_tick_lower_failure() {
    let shallow_levels = [
        level(0.0, 5_000, 0, None, None),
        level(2.0, 5_000, 0, None, None),
        level(4.0, 5_000, 0, None, None),
        level(5.0, 5_000, 100, Some(4.0), None),
    ];
    let shallow = FootprintCandle5m {
        open: 4.0,
        high: 5.0,
        low: 0.0,
        close: 4.5,
        total_volume: 20_000,
        candle_delta: 100,
        volume_poc: 4.0,
        levels: &shallow_levels,
        completed: true,
    };
    assert_eq!(genuine_second_buyer_attempt(shallow, 5.0), Condition::False);

    let test_data = levels(20_000, 100, 35, Some(4.0), None);
    let test = candle(&test_data, 6.0, 7.0, 20_000, 100, 9.0);
    assert_eq!(genuine_second_buyer_attempt(test, 5.0), Condition::True);
    assert_eq!(
        second_test_has_real_buying(test, Condition::True, 8.0),
        Condition::True
    );

    let no_buy_data = levels(20_000, 100, 35, None, None);
    let no_buy = candle(&no_buy_data, 6.0, 7.0, 20_000, 100, 9.0);
    assert_eq!(
        second_test_has_real_buying(no_buy, Condition::True, 8.0),
        Condition::False
    );

    assert_eq!(second_failure_lower(7.75, 8.0, 0.25), Condition::True);
    assert_eq!(second_failure_lower(7.751, 8.0, 0.25), Condition::False);
}

#[test]
fn section_92_seller_reconfirmation_requires_strict_midpoint_break() {
    let data = levels(20_000, -100, 10, None, Some(4.0));
    let valid = candle(&data, 6.0, 4.0, 20_000, -100, 3.0);
    assert_eq!(
        seller_reconfirmation(valid, 5.0, Condition::True),
        Condition::True
    );

    let midpoint = candle(&data, 6.0, 5.0, 20_000, -100, 3.0);
    assert_eq!(
        seller_reconfirmation(midpoint, 5.0, Condition::True),
        Condition::False
    );
}

#[test]
fn sections_85_92_and_102_advance_mirrored_states_without_short_authorization() {
    let aggression_data = levels(20_000, 100, 35, Some(4.0), None);
    let aggression = candle(&aggression_data, 7.5, 4.0, 20_000, 100, 9.0);
    let dominance_data = levels(20_000, -100, 10, None, Some(4.0));
    let dominance = candle(&dominance_data, 6.0, 4.0, 20_000, -100, 3.0);
    let second_test_levels = [
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
        levels: &second_test_levels,
        completed: true,
    };
    let reconfirmation_data = levels(20_000, -100, 10, None, Some(4.0));
    let reconfirmation = candle(&reconfirmation_data, 5.0, 3.0, 20_000, -100, 3.0);

    let location = common::short_location();
    let mut sequence = ShortOrderflowSequence::from_location_reached(&location).unwrap();
    assert_eq!(
        sequence.record_aggression(
            aggression,
            &prior_deltas(100),
            common::mnq_participation(),
            Condition::True
        ),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::AggressionPresent);

    assert_eq!(sequence.record_absorption(8.0), Condition::True);
    assert_eq!(sequence.state(), SetupState::PotentialAbsorption);

    assert_eq!(
        sequence.record_first_dominance_shift(dominance, common::mnq_participation()),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::FirstDominanceShift);

    assert_eq!(sequence.begin_second_test(), Condition::True);
    assert_eq!(sequence.state(), SetupState::WaitingSecondTest);

    assert_eq!(
        sequence.record_second_test(second_test, common::mnq_participation()),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::SecondTest);

    assert_eq!(sequence.confirm_second_failure(0.25), Condition::True);
    assert_eq!(sequence.state(), SetupState::SecondFailure);

    assert_eq!(
        sequence.record_reconfirmation(reconfirmation, common::mnq_participation()),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::FinalReconfirmation);
    assert_ne!(sequence.state(), SetupState::EntryAuthorized);
}
