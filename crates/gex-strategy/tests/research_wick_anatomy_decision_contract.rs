use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvAgeLimits, RequiredOhlcvCoverage,
    ResearchWickAnatomyDecisionError, ResearchWickAnatomyError, RuleProvenance, Session,
    assess_required_ohlcv_coverage, calculate_research_wick_anatomy_at_decision,
};

fn snapshot(id: &str, symbol: &str) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: symbol.to_owned(),
            source_timestamp_millis: 950,
            received_timestamp_millis: 1_000,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn series(
    snap: &MarketSnapshot,
    source_id: &str,
    quality: DataQuality,
    bars: &[(OhlcvTimeframe, i64, f64, f64, f64, f64)],
) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snap,
        OhlcvObservationInput {
            snapshot_id: snap.snapshot_id().to_owned(),
            symbol: snap.symbol().to_owned(),
            source_id: source_id.to_owned(),
            quality,
            bars: bars
                .iter()
                .map(|(timeframe, stamp, open, high, low, close)| OhlcvBarInput {
                    timeframe: *timeframe,
                    source_timestamp_millis: *stamp,
                    received_timestamp_millis: *stamp + 1,
                    open: *open,
                    high: *high,
                    low: *low,
                    close: *close,
                    volume: 25.0,
                    completed: true,
                })
                .collect(),
        },
    )
    .unwrap()
}

fn valid_series(snap: &MarketSnapshot) -> OhlcvObservationSeries {
    series(
        snap,
        "source-102",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute5, 800, 105.0, 120.0, 100.0, 110.0)],
    )
}

fn coverage(
    snap: &MarketSnapshot,
    src: Option<&OhlcvObservationSeries>,
    decision: i64,
    limit: u64,
) -> RequiredOhlcvCoverage {
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: limit,
        minute5_ms: limit,
        minute15_ms: limit,
        hour1_ms: limit,
        hour4_ms: limit,
    };
    assess_required_ohlcv_coverage(snap, src, decision, &limits).unwrap()
}

fn checked(
    snap: &MarketSnapshot,
    src: &OhlcvObservationSeries,
    age: &RequiredOhlcvCoverage,
    timeframe: OhlcvTimeframe,
    clock: i64,
) -> Result<gex_strategy::ResearchWickAnatomyAtDecision, ResearchWickAnatomyDecisionError> {
    calculate_research_wick_anatomy_at_decision(snap, src, age, timeframe, clock)
}

#[test]
fn exact_age_and_clock_boundary_preserve_raw_evidence_and_provenance() {
    let snap = snapshot("wick102", "SPY");
    let src = valid_series(&snap);
    let age = coverage(&snap, Some(&src), 1_010, 210);
    let evidence = checked(&snap, &src, &age, OhlcvTimeframe::Minute5, 1_010).unwrap();
    let same = checked(&snap, &src, &age, OhlcvTimeframe::Minute5, 1_010).unwrap();
    assert_eq!(evidence, same);
    assert_eq!(evidence.decision_timestamp_millis(), 1_010);
    assert_eq!(evidence.measured_source_age_millis(), 210);

    let raw = evidence.observation();
    assert_eq!(raw.snapshot_id(), "wick102");
    assert_eq!(raw.symbol(), "SPY");
    assert_eq!(raw.source_id(), "source-102");
    assert_eq!(raw.source_quality(), DataQuality::Structured);
    assert_eq!(raw.timeframe(), OhlcvTimeframe::Minute5);
    assert_eq!(raw.source_timestamp_millis(), 800);
    assert_eq!(raw.wick_asymmetry_ratio(), -0.25);
    assert_eq!(raw.provenance(), RuleProvenance::ValidationRequiredAssumption);
}

#[test]
fn prior_or_later_assessment_clock_cannot_be_reused() {
    let snap = snapshot("wick102", "SPY");
    let src = valid_series(&snap);
    let age = coverage(&snap, Some(&src), 1_010, 210);

    for altered in [1_009, 1_011] {
        assert_eq!(
            checked(&snap, &src, &age, OhlcvTimeframe::Minute5, altered),
            Err(ResearchWickAnatomyDecisionError::AssessmentClockMismatch)
        );
    }

    for invalid in [-1, 999] {
        assert_eq!(
            checked(&snap, &src, &age, OhlcvTimeframe::Minute5, invalid),
            Err(ResearchWickAnatomyDecisionError::InvalidDecisionTimeline)
        );
    }
    let invalid_report = coverage(&snap, Some(&src), 999, 1_000);
    assert_eq!(
        checked(&snap, &src, &invalid_report, OhlcvTimeframe::Minute5, 999),
        Err(ResearchWickAnatomyDecisionError::InvalidDecisionTimeline)
    );
}

#[test]
fn stale_missing_and_wrong_timeframe_never_promote() {
    let snap = snapshot("wick102", "SPY");
    let src = valid_series(&snap);
    let stale = coverage(&snap, Some(&src), 1_011, 210);
    assert_eq!(
        checked(&snap, &src, &stale, OhlcvTimeframe::Minute5, 1_011),
        Err(ResearchWickAnatomyDecisionError::MissingOrInvalidScopeAge)
    );

    let absent = coverage(&snap, None, 1_010, 210);
    assert_eq!(
        checked(&snap, &src, &absent, OhlcvTimeframe::Minute5, 1_010),
        Err(ResearchWickAnatomyDecisionError::SourceMismatch)
    );

    let age = coverage(&snap, Some(&src), 1_010, 210);
    assert_eq!(
        checked(&snap, &src, &age, OhlcvTimeframe::Minute1, 1_010),
        Err(ResearchWickAnatomyDecisionError::MissingOrInvalidScopeAge)
    );
    assert_eq!(
        checked(&snap, &src, &age, OhlcvTimeframe::Daily, 1_010),
        Err(ResearchWickAnatomyDecisionError::UnsupportedTimeframe)
    );
}

#[test]
fn snapshot_symbol_source_and_quality_mismatch_fail_closed() {
    let snap = snapshot("wick102", "SPY");
    let src = valid_series(&snap);
    let age = coverage(&snap, Some(&src), 1_010, 210);
    assert_eq!(
        checked(&snapshot("other", "SPY"), &src, &age, OhlcvTimeframe::Minute5, 1_010),
        Err(ResearchWickAnatomyDecisionError::SnapshotMismatch)
    );
    assert_eq!(
        checked(&snapshot("wick102", "QQQ"), &src, &age, OhlcvTimeframe::Minute5, 1_010),
        Err(ResearchWickAnatomyDecisionError::SymbolMismatch)
    );

    let different = series(
        &snap,
        "other-feed",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute5, 800, 105.0, 120.0, 100.0, 110.0)],
    );
    let wrong_source = coverage(&snap, Some(&different), 1_010, 210);
    assert_eq!(
        checked(&snap, &src, &wrong_source, OhlcvTimeframe::Minute5, 1_010),
        Err(ResearchWickAnatomyDecisionError::SourceMismatch)
    );

    let parsed = series(
        &snap,
        "source-102",
        DataQuality::Parsed,
        &[(OhlcvTimeframe::Minute5, 800, 105.0, 120.0, 100.0, 110.0)],
    );
    let wrong_quality = coverage(&snap, Some(&parsed), 1_010, 210);
    assert_eq!(
        checked(&snap, &src, &wrong_quality, OhlcvTimeframe::Minute5, 1_010),
        Err(ResearchWickAnatomyDecisionError::SourceMismatch)
    );
}

#[test]
fn exact_completed_candle_timestamp_must_match_scope_evidence() {
    let snap = snapshot("wick102", "SPY");
    let src = valid_series(&snap);
    let shifted = series(
        &snap,
        "source-102",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute5, 801, 105.0, 120.0, 100.0, 110.0)],
    );
    let age = coverage(&snap, Some(&shifted), 1_010, 210);
    assert_eq!(
        checked(&snap, &src, &age, OhlcvTimeframe::Minute5, 1_010),
        Err(ResearchWickAnatomyDecisionError::SourceTimestampMismatch)
    );

    let two_scopes = series(
        &snap,
        "source-102",
        DataQuality::Structured,
        &[
            (OhlcvTimeframe::Minute5, 800, 105.0, 120.0, 100.0, 110.0),
            (OhlcvTimeframe::Minute1, 900, 120.0, 120.0, 100.0, 120.0),
        ],
    );
    let both = coverage(&snap, Some(&two_scopes), 1_010, 210);
    let observed = checked(&snap, &two_scopes, &both, OhlcvTimeframe::Minute5, 1_010).unwrap();
    assert_eq!(observed.observation().source_timestamp_millis(), 800);
    assert_eq!(observed.measured_source_age_millis(), 210);
}

#[test]
fn invalid_raw_wick_arithmetic_cannot_be_promoted() {
    let snap = snapshot("wick102", "SPY");
    for (open, high, low, close, expected) in [
        (100.0, 100.0, 100.0, 100.0, ResearchWickAnatomyError::ZeroRangeBar),
        (0.0, f64::MAX, -f64::MAX, 0.0, ResearchWickAnatomyError::NonFiniteCalculation),
    ] {
        let src = series(
            &snap,
            "source-102",
            DataQuality::Structured,
            &[(OhlcvTimeframe::Minute5, 800, open, high, low, close)],
        );
        let age = coverage(&snap, Some(&src), 1_010, 210);
        assert_eq!(
            checked(&snap, &src, &age, OhlcvTimeframe::Minute5, 1_010),
            Err(ResearchWickAnatomyDecisionError::InvalidRawObservation(expected))
        );
    }
}
