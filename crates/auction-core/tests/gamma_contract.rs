use auction_core::{
    Direction, GammaContext, GammaRegime, GammaSnapshotError, MarketState, RelativeExpectation,
    ReliableGammaSnapshot, VolatilityExpectation, direction_allowed,
};

fn context(regime: GammaRegime) -> GammaContext {
    GammaContext {
        regime,
        flip: Some(100.0),
        call_wall: Some(110.0),
        put_wall: Some(90.0),
    }
}

#[test]
fn section_15_positive_gamma_interpretation_is_exact_and_non_directional() {
    let interpretation = context(GammaRegime::Positive).interpretation();
    assert_eq!(interpretation.volatility, VolatilityExpectation::Dampened);
    assert_eq!(
        interpretation.mean_reversion,
        Some(RelativeExpectation::Higher)
    );
    assert_eq!(
        interpretation.breakout_persistence,
        Some(RelativeExpectation::Lower)
    );
    assert_eq!(interpretation.move_acceleration, None);

    assert!(direction_allowed(MarketState::ValueUp, Direction::Long));
    assert!(!direction_allowed(MarketState::ValueUp, Direction::Short));
}

#[test]
fn section_15_negative_gamma_interpretation_is_exact_and_non_directional() {
    let interpretation = context(GammaRegime::Negative).interpretation();
    assert_eq!(interpretation.volatility, VolatilityExpectation::Amplified);
    assert_eq!(interpretation.mean_reversion, None);
    assert_eq!(interpretation.breakout_persistence, None);
    assert_eq!(
        interpretation.move_acceleration,
        Some(RelativeExpectation::Higher)
    );

    assert!(direction_allowed(MarketState::ValueDown, Direction::Short));
    assert!(!direction_allowed(MarketState::ValueDown, Direction::Long));
}

#[test]
fn sections_14_15_unavailable_gamma_preserves_unknown_context_without_inference() {
    let unavailable = GammaContext {
        regime: GammaRegime::Unavailable,
        flip: None,
        call_wall: None,
        put_wall: None,
    };
    let interpretation = unavailable.interpretation();
    assert_eq!(interpretation.volatility, VolatilityExpectation::Unknown);
    assert_eq!(interpretation.mean_reversion, None);
    assert_eq!(interpretation.breakout_persistence, None);
    assert_eq!(interpretation.move_acceleration, None);
    assert_eq!(
        unavailable.volatility_expectation(),
        VolatilityExpectation::Unknown
    );
}

#[test]
fn sections_14_25_reliable_snapshot_keeps_required_metadata_and_optional_levels() {
    let snapshot = ReliableGammaSnapshot::new(
        "MNQ",
        "2026-08-24T09:00:00-04:00",
        GammaContext {
            regime: GammaRegime::Positive,
            flip: None,
            call_wall: None,
            put_wall: None,
        },
    )
    .unwrap();

    assert_eq!(snapshot.underlying(), "MNQ");
    assert_eq!(snapshot.timestamp(), "2026-08-24T09:00:00-04:00");
    assert_eq!(snapshot.context().flip, None);
    assert_eq!(
        snapshot.interpretation().volatility,
        VolatilityExpectation::Dampened
    );
}

#[test]
fn section_14_reliable_snapshot_rejects_missing_metadata_or_unavailable_regime() {
    assert_eq!(
        ReliableGammaSnapshot::new("", "timestamp", context(GammaRegime::Positive)),
        Err(GammaSnapshotError::MissingUnderlying)
    );
    assert_eq!(
        ReliableGammaSnapshot::new("MNQ", " ", context(GammaRegime::Positive)),
        Err(GammaSnapshotError::MissingTimestamp)
    );
    assert_eq!(
        ReliableGammaSnapshot::new(
            "MNQ",
            "timestamp",
            GammaContext {
                regime: GammaRegime::Unavailable,
                flip: None,
                call_wall: None,
                put_wall: None,
            },
        ),
        Err(GammaSnapshotError::UnavailableRegime)
    );
}

#[test]
fn sections_14_25_reliable_snapshot_rejects_nonfinite_supplied_levels() {
    for invalid in [
        GammaContext {
            regime: GammaRegime::Positive,
            flip: Some(f64::NAN),
            call_wall: None,
            put_wall: None,
        },
        GammaContext {
            regime: GammaRegime::Positive,
            flip: None,
            call_wall: Some(f64::INFINITY),
            put_wall: None,
        },
        GammaContext {
            regime: GammaRegime::Negative,
            flip: None,
            call_wall: None,
            put_wall: Some(f64::NEG_INFINITY),
        },
    ] {
        assert_eq!(
            ReliableGammaSnapshot::new("MNQ", "timestamp", invalid),
            Err(GammaSnapshotError::InvalidLevel)
        );
    }
}
