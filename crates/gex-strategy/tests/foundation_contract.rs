// AI-native GEX spec §§2, 6-10, 34, 39, 42; current project promotion control §116.
use gex_strategy::{
    DataQuality, DecisionVerdict, DirectionFromSpot, GammaRegime, GexInteractionState,
    GexLevelRole, GexNormalizationError, GexScope, GexSign, MarketSnapshot,
    MarketSnapshotInput, OperatingMode, Playbook, RawGexLevel, ResearchGammaRegimeConfig,
    ResearchRegimeError, RuleProvenance, Session, SnapshotError,
    classify_research_gamma_regime, normalize_gex_levels,
};

fn raw(strike: f64, gex_value: f64, scope: GexScope) -> RawGexLevel {
    RawGexLevel {
        strike,
        gex_value,
        scope,
    }
}

fn assert_close(actual: f64, expected: f64) {
    let tolerance = expected.abs().max(1.0) * 1e-12;
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn spec_8_normalization_preserves_sign_distance_direction_and_unassigned_role() {
    let normalized = normalize_gex_levels(
        500.0,
        &[
            raw(510.0, 12.0, GexScope::Aggregate),
            raw(490.0, -8.0, GexScope::Aggregate),
            raw(500.0, 0.0, GexScope::Aggregate),
        ],
    )
    .unwrap();

    assert_eq!(normalized[0].sign, GexSign::Positive);
    assert_close(normalized[0].magnitude, 12.0);
    assert_close(normalized[0].distance_from_spot, 10.0);
    assert_eq!(normalized[0].direction_from_spot, DirectionFromSpot::Above);
    assert_eq!(normalized[0].rank_within_scope, 1);
    assert_eq!(normalized[0].role, GexLevelRole::Unassigned);

    assert_eq!(normalized[1].sign, GexSign::Negative);
    assert_eq!(normalized[1].direction_from_spot, DirectionFromSpot::Below);
    assert_eq!(normalized[1].rank_within_scope, 2);

    assert_eq!(normalized[2].sign, GexSign::Zero);
    assert_eq!(normalized[2].direction_from_spot, DirectionFromSpot::At);
    assert_eq!(normalized[2].rank_within_scope, 3);
}

#[test]
fn specs_7_8_ranking_is_strictly_within_scope() {
    let expiry = GexScope::Expiry("2026-09-18".to_owned());
    let normalized = normalize_gex_levels(
        500.0,
        &[
            raw(510.0, 10.0, GexScope::Aggregate),
            raw(520.0, 5.0, GexScope::Aggregate),
            raw(510.0, -1_000.0, expiry.clone()),
            raw(520.0, 100.0, expiry.clone()),
        ],
    )
    .unwrap();

    assert_eq!(normalized[0].rank_within_scope, 1);
    assert_eq!(normalized[1].rank_within_scope, 2);
    assert_eq!(normalized[2].rank_within_scope, 1);
    assert_eq!(normalized[3].rank_within_scope, 2);
}

#[test]
fn spec_8_invalid_numeric_or_scope_input_fails_closed() {
    assert_eq!(
        normalize_gex_levels(0.0, &[]),
        Err(GexNormalizationError::InvalidSpot)
    );
    assert_eq!(
        normalize_gex_levels(
            500.0,
            &[raw(f64::NAN, 1.0, GexScope::Aggregate)]
        ),
        Err(GexNormalizationError::InvalidStrike)
    );
    assert_eq!(
        normalize_gex_levels(
            500.0,
            &[raw(500.0, f64::INFINITY, GexScope::Aggregate)]
        ),
        Err(GexNormalizationError::InvalidGexValue)
    );
    assert_eq!(
        normalize_gex_levels(
            500.0,
            &[raw(500.0, 1.0, GexScope::Expiry(" ".to_owned()))]
        ),
        Err(GexNormalizationError::InvalidScope)
    );
}

fn snapshot_input() -> MarketSnapshotInput {
    MarketSnapshotInput {
        snapshot_id: "snapshot-1".to_owned(),
        symbol: "SPY".to_owned(),
        source_timestamp_millis: 1_000,
        received_timestamp_millis: 1_025,
        spot_price: 500.0,
        session: Session::Regular,
        data_quality: DataQuality::Structured,
    }
}

#[test]
fn specs_6_28_snapshot_validates_identity_time_and_data_age() {
    let snapshot = MarketSnapshot::build(
        snapshot_input(),
        &[raw(510.0, 10.0, GexScope::Aggregate)],
    )
    .unwrap();

    assert_eq!(snapshot.snapshot_id(), "snapshot-1");
    assert_eq!(snapshot.symbol(), "SPY");
    assert_eq!(snapshot.source_timestamp_millis(), 1_000);
    assert_eq!(snapshot.received_timestamp_millis(), 1_025);
    assert_eq!(snapshot.data_age_millis(), 25);
    assert_close(snapshot.spot_price(), 500.0);
    assert_eq!(snapshot.session(), Session::Regular);
    assert_eq!(snapshot.data_quality(), DataQuality::Structured);
    assert_eq!(snapshot.gex_levels().len(), 1);
}

#[test]
fn specs_6_33_missing_or_invalid_snapshot_identity_fails_closed() {
    let mut input = snapshot_input();
    input.snapshot_id = " ".to_owned();
    assert_eq!(
        MarketSnapshot::build(input, &[]),
        Err(SnapshotError::MissingSnapshotId)
    );

    let mut input = snapshot_input();
    input.symbol = String::new();
    assert_eq!(
        MarketSnapshot::build(input, &[]),
        Err(SnapshotError::MissingSymbol)
    );

    let mut input = snapshot_input();
    input.source_timestamp_millis = -1;
    assert_eq!(
        MarketSnapshot::build(input, &[]),
        Err(SnapshotError::InvalidSourceTimestamp)
    );

    let mut input = snapshot_input();
    input.received_timestamp_millis = 999;
    assert_eq!(
        MarketSnapshot::build(input, &[]),
        Err(SnapshotError::SourceAfterReceived)
    );

    let mut input = snapshot_input();
    input.spot_price = f64::NAN;
    assert_eq!(
        MarketSnapshot::build(input, &[]),
        Err(SnapshotError::InvalidSpot)
    );
}

fn classify(
    levels: &[RawGexLevel],
    scope: GexScope,
    mixed_share_threshold: f64,
) -> GammaRegime {
    let normalized = normalize_gex_levels(500.0, levels).unwrap();
    classify_research_gamma_regime(
        &normalized,
        &ResearchGammaRegimeConfig {
            window_points: 20.0,
            mixed_share_threshold,
            scope,
        },
    )
    .unwrap()
    .regime
}

#[test]
fn spec_10_research_classifier_covers_positive_negative_mixed_and_unknown() {
    assert_eq!(
        classify(
            &[
                raw(505.0, 10.0, GexScope::Aggregate),
                raw(495.0, -1.0, GexScope::Aggregate),
            ],
            GexScope::Aggregate,
            0.20,
        ),
        GammaRegime::Positive
    );

    assert_eq!(
        classify(
            &[
                raw(505.0, 1.0, GexScope::Aggregate),
                raw(495.0, -10.0, GexScope::Aggregate),
            ],
            GexScope::Aggregate,
            0.20,
        ),
        GammaRegime::Negative
    );

    assert_eq!(
        classify(
            &[
                raw(505.0, 6.0, GexScope::Aggregate),
                raw(495.0, -4.0, GexScope::Aggregate),
            ],
            GexScope::Aggregate,
            0.20,
        ),
        GammaRegime::Mixed
    );

    assert_eq!(
        classify(
            &[raw(
                505.0,
                10.0,
                GexScope::Expiry("2026-09-18".to_owned()),
            )],
            GexScope::Aggregate,
            0.20,
        ),
        GammaRegime::Unknown
    );
}

#[test]
fn specs_7_10_research_classifier_does_not_cross_contaminate_scopes() {
    let expiry = GexScope::Expiry("2026-09-18".to_owned());
    let raw_levels = [
        raw(505.0, 10.0, GexScope::Aggregate),
        raw(495.0, -1_000.0, expiry.clone()),
    ];

    assert_eq!(
        classify(&raw_levels, GexScope::Aggregate, 0.20),
        GammaRegime::Positive
    );
    assert_eq!(
        classify(&raw_levels, expiry, 0.20),
        GammaRegime::Negative
    );
}

#[test]
fn specs_10_34_classifier_requires_explicit_valid_configuration() {
    let normalized = normalize_gex_levels(
        500.0,
        &[raw(505.0, 10.0, GexScope::Aggregate)],
    )
    .unwrap();

    for (config, expected) in [
        (
            ResearchGammaRegimeConfig {
                window_points: 0.0,
                mixed_share_threshold: 0.20,
                scope: GexScope::Aggregate,
            },
            ResearchRegimeError::InvalidWindow,
        ),
        (
            ResearchGammaRegimeConfig {
                window_points: 20.0,
                mixed_share_threshold: 0.0,
                scope: GexScope::Aggregate,
            },
            ResearchRegimeError::InvalidMixedShareThreshold,
        ),
        (
            ResearchGammaRegimeConfig {
                window_points: 20.0,
                mixed_share_threshold: 0.51,
                scope: GexScope::Aggregate,
            },
            ResearchRegimeError::InvalidMixedShareThreshold,
        ),
        (
            ResearchGammaRegimeConfig {
                window_points: 20.0,
                mixed_share_threshold: 0.20,
                scope: GexScope::Expiry(String::new()),
            },
            ResearchRegimeError::InvalidScope,
        ),
    ] {
        assert_eq!(
            classify_research_gamma_regime(&normalized, &config),
            Err(expected)
        );
    }
}

#[test]
fn specs_0_10_36_research_classifier_is_explicitly_validation_required() {
    let normalized = normalize_gex_levels(
        500.0,
        &[raw(505.0, 10.0, GexScope::Aggregate)],
    )
    .unwrap();
    let result = classify_research_gamma_regime(
        &normalized,
        &ResearchGammaRegimeConfig {
            window_points: 20.0,
            mixed_share_threshold: 0.20,
            scope: GexScope::Aggregate,
        },
    )
    .unwrap();

    assert_eq!(
        result.provenance,
        RuleProvenance::ValidationRequiredAssumption
    );
}

#[test]
fn specs_2_11_19_30_foundational_contract_enums_remain_explicit() {
    assert_ne!(OperatingMode::StrictSourceBaseline, OperatingMode::AiNative);
    assert_ne!(Playbook::PositiveGammaMagnet, Playbook::NegativeGammaExpansion);
    assert_ne!(GexInteractionState::Testing, GexInteractionState::Breaking);
    assert_ne!(DecisionVerdict::Wait, DecisionVerdict::Allow);
}
