mod common;

use auction_core::{
    Condition, DataCycleReadiness, EnvironmentInput, EtTime, FrozenPremarketScenario,
    LocationSetup, LongOrderflowSequence, NewsBlackoutWindow, OrderflowSequenceEvidence,
    RejectionCode, StrategyValidationInputs, validate_strategy,
};

fn inputs<'a>(
    scenario: &'a FrozenPremarketScenario,
    location: &'a LocationSetup,
    sequence: &'a LongOrderflowSequence,
    now_et: EtTime,
    data_readiness: DataCycleReadiness,
    environment: EnvironmentInput,
    owner: &'a str,
    news_schedule_valid: Condition,
    news_windows: &'a [NewsBlackoutWindow],
) -> StrategyValidationInputs<'a> {
    StrategyValidationInputs {
        setup_id: "SETUP-VALIDATOR",
        owner_agent_id: owner,
        instrument: common::INSTRUMENT,
        now_et,
        data_readiness,
        premarket_scenario: scenario,
        environment,
        location,
        orderflow: OrderflowSequenceEvidence::Long(sequence),
        news_schedule_valid,
        news_windows,
    }
}

#[test]
fn sections_109_117_valid_long_and_short_sequences_produce_opaque_strategy_proofs() {
    let long = common::valid_long_proof("SETUP-L", common::OWNER, common::INSTRUMENT);
    assert_eq!(long.setup_id(), "SETUP-L");
    assert_eq!(long.owner_agent_id(), common::OWNER);
    assert_eq!(long.instrument(), common::INSTRUMENT);
    assert_eq!(long.direction(), auction_core::Direction::Long);

    let short = common::valid_short_proof("SETUP-S", common::OWNER, common::INSTRUMENT);
    assert_eq!(short.setup_id(), "SETUP-S");
    assert_eq!(short.direction(), auction_core::Direction::Short);
}

#[test]
fn sections_3_4_109_time_cutoff_is_recalculated_and_fails_closed() {
    let location = common::long_location();
    let sequence = common::valid_long_sequence(&location);
    let scenario = common::frozen_scenario(common::OWNER, common::INSTRUMENT);
    assert_eq!(
        validate_strategy(inputs(
            &scenario,
            &location,
            &sequence,
            EtTime::from_hms(11, 0, 0).unwrap(),
            common::true_data(),
            common::long_environment(),
            common::OWNER,
            Condition::True,
            &[],
        )),
        Err(RejectionCode::TimeCutoff)
    );
}

#[test]
fn sections_10_11_109_unknown_data_cannot_be_promoted_to_strategy_pass() {
    let location = common::long_location();
    let sequence = common::valid_long_sequence(&location);
    let scenario = common::frozen_scenario(common::OWNER, common::INSTRUMENT);
    let mut data = common::true_data();
    data.health.quote_fresh = Condition::Unknown;
    assert_eq!(
        validate_strategy(inputs(
            &scenario,
            &location,
            &sequence,
            EtTime::from_hms(10, 0, 0).unwrap(),
            data,
            common::long_environment(),
            common::OWNER,
            Condition::True,
            &[],
        )),
        Err(RejectionCode::DataInvalid)
    );
}

#[test]
fn sections_18_24_109_wrong_environment_direction_is_rejected() {
    let location = common::long_location();
    let sequence = common::valid_long_sequence(&location);
    let scenario = common::frozen_scenario(common::OWNER, common::INSTRUMENT);
    assert_eq!(
        validate_strategy(inputs(
            &scenario,
            &location,
            &sequence,
            EtTime::from_hms(10, 0, 0).unwrap(),
            common::true_data(),
            common::short_environment(),
            common::OWNER,
            Condition::True,
            &[],
        )),
        Err(RejectionCode::WrongDirection)
    );
}

#[test]
fn sections_95_96_109_active_news_blackout_rejects_new_authorization() {
    let location = common::long_location();
    let sequence = common::valid_long_sequence(&location);
    let scenario = common::frozen_scenario(common::OWNER, common::INSTRUMENT);
    let windows = [NewsBlackoutWindow {
        start_inclusive: EtTime::from_hms(9, 55, 0).unwrap(),
        end_exclusive: EtTime::from_hms(10, 5, 0).unwrap(),
    }];
    assert_eq!(
        validate_strategy(inputs(
            &scenario,
            &location,
            &sequence,
            EtTime::from_hms(10, 0, 0).unwrap(),
            common::true_data(),
            common::long_environment(),
            common::OWNER,
            Condition::True,
            &windows,
        )),
        Err(RejectionCode::NewsBlackout)
    );
}

#[test]
fn sections_17_102_109_identity_or_incomplete_sequence_cannot_forge_pass() {
    let location = common::long_location();
    let complete = common::valid_long_sequence(&location);
    let scenario = common::frozen_scenario(common::OWNER, common::INSTRUMENT);
    assert_eq!(
        validate_strategy(inputs(
            &scenario,
            &location,
            &complete,
            EtTime::from_hms(10, 0, 0).unwrap(),
            common::true_data(),
            common::long_environment(),
            "OTHER_AGENT",
            Condition::True,
            &[],
        )),
        Err(RejectionCode::ProcessError)
    );

    let incomplete = LongOrderflowSequence::from_location_reached(&location).unwrap();
    assert_eq!(
        validate_strategy(inputs(
            &scenario,
            &location,
            &incomplete,
            EtTime::from_hms(10, 0, 0).unwrap(),
            common::true_data(),
            common::long_environment(),
            common::OWNER,
            Condition::True,
            &[],
        )),
        Err(RejectionCode::NoReconfirmation)
    );
}
