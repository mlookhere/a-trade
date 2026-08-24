use auction_core::{
    Condition, FootprintCandle5m, FootprintLevel, LongOrderflowSequence, ParticipationRule,
    SetupState, buyer_reconfirmation, first_buyer_dominance_shift, genuine_second_seller_attempt,
    participation_valid, potential_absorption, second_failure_higher, second_test_has_real_selling,
    seller_aggression,
};

fn levels(
    total_volume: u64,
    delta: i64,
    bottom_percent: u64,
    lower_sell_ratio: Option<f64>,
    upper_sell_ratio: Option<f64>,
    buy_ratio: Option<f64>,
) -> [FootprintLevel; 4] {
    let bottom = total_volume * bottom_percent / 100;
    let remaining = total_volume - bottom;
    let second = remaining / 3;
    let third = remaining / 3;
    let fourth = remaining - second - third;

    [
        level(0.0, bottom, delta, None, lower_sell_ratio),
        level(3.0, second, 0, None, None),
        level(7.0, third, 0, buy_ratio, upper_sell_ratio),
        level(10.0, fourth, 0, None, None),
    ]
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
fn sections_12_40_require_completed_consistent_five_minute_footprint() {
    let data = levels(20_000, -100, 35, Some(4.0), None, None);
    let mut execution = candle(&data, 4.0, 6.0, 20_000, -100, 1.0);
    assert!(execution.valid());

    execution.completed = false;
    assert!(!execution.valid());
    assert_eq!(
        participation_valid(execution, ParticipationRule::MnqSourceThreshold, None),
        Condition::Unknown
    );

    let mut conflicting = candle(&data, 4.0, 6.0, 19_999, -100, 1.0);
    assert!(!conflicting.valid());
    conflicting.total_volume = 20_000;
    conflicting.candle_delta = -99;
    assert!(!conflicting.valid());
}

#[test]
fn sections_38_39_keep_mnq_threshold_isolated_from_non_mnq_rvol() {
    let mnq_low_data = levels(19_999, -100, 35, Some(4.0), None, None);
    let mnq_low = candle(&mnq_low_data, 4.0, 6.0, 19_999, -100, 1.0);
    assert_eq!(
        participation_valid(mnq_low, ParticipationRule::MnqSourceThreshold, None),
        Condition::False
    );

    let mnq_pass_data = levels(20_000, -100, 35, Some(4.0), None, None);
    let mnq_pass = candle(&mnq_pass_data, 4.0, 6.0, 20_000, -100, 1.0);
    assert_eq!(
        participation_valid(mnq_pass, ParticipationRule::MnqSourceThreshold, None),
        Condition::True
    );

    let history = [25_000_u64; 20];
    assert_eq!(
        participation_valid(
            mnq_pass,
            ParticipationRule::NonMnqTimeNormalizedRvol,
            Some(&history)
        ),
        Condition::False
    );

    let history = [20_000_u64; 20];
    assert_eq!(
        participation_valid(
            mnq_pass,
            ParticipationRule::NonMnqTimeNormalizedRvol,
            Some(&history)
        ),
        Condition::True
    );
    assert_eq!(
        participation_valid(
            mnq_pass,
            ParticipationRule::NonMnqTimeNormalizedRvol,
            Some(&history[..19])
        ),
        Condition::Unknown
    );
}

#[test]
fn sections_41_43_use_4x_imbalance_and_delta_median_equality() {
    let below_data = levels(20_000, -100, 35, Some(3.99), None, None);
    let below = candle(&below_data, 4.0, 6.0, 20_000, -100, 1.0);
    assert_eq!(
        seller_aggression(below, &prior_deltas(100), Condition::True),
        Condition::False
    );

    let boundary_data = levels(20_000, -100, 35, Some(4.0), None, None);
    let boundary = candle(&boundary_data, 4.0, 6.0, 20_000, -100, 1.0);
    assert_eq!(
        seller_aggression(boundary, &prior_deltas(100), Condition::True),
        Condition::True
    );
    assert_eq!(
        seller_aggression(boundary, &prior_deltas(101), Condition::True),
        Condition::False
    );
    assert_eq!(
        seller_aggression(boundary, &prior_deltas(100)[..19], Condition::True),
        Condition::Unknown
    );
}

#[test]
fn section_44_extreme_quarter_and_35_percent_boundaries_are_deterministic() {
    let pct_35_data = levels(20_000, -100, 35, Some(4.0), None, None);
    let pct_35 = candle(&pct_35_data, 4.0, 6.0, 20_000, -100, 7.0);
    assert_eq!(pct_35.aggression_at_long_extreme(), Condition::True);

    let pct_34_data = levels(20_000, -100, 34, Some(4.0), None, None);
    let pct_34 = candle(&pct_34_data, 4.0, 6.0, 20_000, -100, 7.0);
    assert_eq!(pct_34.aggression_at_long_extreme(), Condition::False);

    let poc_data = levels(20_000, -100, 10, Some(4.0), None, None);
    let poc_boundary = candle(&poc_data, 4.0, 6.0, 20_000, -100, 2.5);
    assert_eq!(poc_boundary.aggression_at_long_extreme(), Condition::True);
}

#[test]
fn sections_45_48_unknown_effort_result_cannot_advance_and_wick_25pct_is_inclusive() {
    let data = levels(20_000, -100, 35, Some(4.0), None, None);
    let aggression = candle(&data, 2.5, 6.0, 20_000, -100, 1.0);
    assert_eq!(
        potential_absorption(aggression, Condition::True),
        Condition::True
    );

    let mut sequence = LongOrderflowSequence::from_location_reached(SetupState::LocationReached)
        .unwrap();
    assert_eq!(
        sequence.record_aggression(
            aggression,
            &prior_deltas(100),
            Condition::True,
            Condition::Unknown
        ),
        Condition::Unknown
    );
    assert_eq!(sequence.state(), SetupState::LocationReached);

    assert_eq!(
        sequence.record_aggression(
            aggression,
            &prior_deltas(100),
            Condition::True,
            Condition::False
        ),
        Condition::False
    );
    assert_eq!(sequence.state(), SetupState::LocationReached);
}

#[test]
fn section_49_first_dominance_shift_uses_strict_midpoint_and_buy_imbalance() {
    let data = levels(20_000, 100, 10, None, None, Some(4.0));
    let valid = candle(&data, 4.0, 6.0, 20_000, 100, 7.0);
    assert_eq!(
        first_buyer_dominance_shift(valid, 5.0, Condition::True),
        Condition::True
    );

    let midpoint = candle(&data, 4.0, 5.0, 20_000, 100, 7.0);
    assert_eq!(
        first_buyer_dominance_shift(midpoint, 5.0, Condition::True),
        Condition::False
    );
}

#[test]
fn sections_51_54_enforce_genuine_second_attempt_real_selling_and_one_tick_higher_low() {
    let sell_data = levels(20_000, -100, 10, Some(4.0), None, None);
    let shallow = FootprintCandle5m {
        low: 5.0,
        high: 10.0,
        open: 7.0,
        close: 6.0,
        total_volume: 20_000,
        candle_delta: -100,
        volume_poc: 7.0,
        levels: &[
            level(5.0, 5_000, -100, None, Some(4.0)),
            level(6.0, 5_000, 0, None, None),
            level(8.0, 5_000, 0, None, None),
            level(10.0, 5_000, 0, None, None),
        ],
        completed: true,
    };
    assert_eq!(
        genuine_second_seller_attempt(shallow, 5.0),
        Condition::False
    );

    let test = candle(&sell_data, 4.0, 3.0, 20_000, -100, 7.0);
    assert_eq!(
        genuine_second_seller_attempt(test, 5.0),
        Condition::True
    );
    assert_eq!(
        second_test_has_real_selling(test, Condition::True, 2.0),
        Condition::True
    );

    let no_sell_data = levels(20_000, -100, 10, None, None, None);
    let no_sell = candle(&no_sell_data, 4.0, 3.0, 20_000, -100, 7.0);
    assert_eq!(
        second_test_has_real_selling(no_sell, Condition::True, 2.0),
        Condition::False
    );

    assert_eq!(second_failure_higher(2.25, 2.0, 0.25), Condition::True);
    assert_eq!(
        second_failure_higher(2.249, 2.0, 0.25),
        Condition::False
    );
}

#[test]
fn section_55_reconfirmation_requires_strict_midpoint_retake_and_buy_imbalance() {
    let data = levels(20_000, 100, 10, None, None, Some(4.0));
    let valid = candle(&data, 4.0, 6.0, 20_000, 100, 7.0);
    assert_eq!(
        buyer_reconfirmation(valid, 5.0, Condition::True),
        Condition::True
    );

    let midpoint = candle(&data, 4.0, 5.0, 20_000, 100, 7.0);
    assert_eq!(
        buyer_reconfirmation(midpoint, 5.0, Condition::True),
        Condition::False
    );
}

#[test]
fn sections_42_55_and_102_advance_every_state_without_entry_authorization() {
    let aggression_data = levels(20_000, -100, 35, Some(4.0), None, None);
    let aggression = candle(&aggression_data, 2.5, 6.0, 20_000, -100, 1.0);
    let dominance_data = levels(20_000, 100, 10, None, None, Some(4.0));
    let dominance = candle(&dominance_data, 4.0, 6.0, 20_000, 100, 7.0);
    let test_data = levels(20_000, -100, 10, Some(4.0), None, None);
    let test = candle(&test_data, 4.0, 3.0, 20_000, -100, 7.0);
    let reconfirmation_data = levels(20_000, 100, 10, None, None, Some(4.0));
    let reconfirmation = candle(&reconfirmation_data, 4.0, 6.0, 20_000, 100, 7.0);

    let mut sequence = LongOrderflowSequence::from_location_reached(SetupState::LocationReached)
        .unwrap();
    assert_eq!(
        sequence.record_aggression(
            aggression,
            &prior_deltas(100),
            Condition::True,
            Condition::True
        ),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::AggressionPresent);

    assert_eq!(sequence.record_absorption(2.0), Condition::True);
    assert_eq!(sequence.state(), SetupState::PotentialAbsorption);

    assert_eq!(
        sequence.record_first_dominance_shift(dominance, Condition::True),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::FirstDominanceShift);

    assert_eq!(sequence.begin_second_test(), Condition::True);
    assert_eq!(sequence.state(), SetupState::WaitingSecondTest);

    assert_eq!(
        sequence.record_second_test(test, Condition::True),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::SecondTest);

    assert_eq!(sequence.confirm_second_failure(0.25), Condition::True);
    assert_eq!(sequence.state(), SetupState::SecondFailure);

    assert_eq!(
        sequence.record_reconfirmation(reconfirmation, Condition::True),
        Condition::True
    );
    assert_eq!(sequence.state(), SetupState::FinalReconfirmation);
    assert_ne!(sequence.state(), SetupState::EntryAuthorized);
}
