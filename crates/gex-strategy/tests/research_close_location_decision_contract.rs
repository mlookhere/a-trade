use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvAgeLimits, RequiredOhlcvCoverage,
    ResearchCloseLocationAgeError, ResearchCloseLocationDecisionError, ResearchCloseLocationError,
    RuleProvenance, Session, assess_required_ohlcv_coverage,
    calculate_research_close_location_at_decision,
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
    timeframe: OhlcvTimeframe,
    timestamp: i64,
    low: f64,
    high: f64,
    close: f64,
) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snap,
        OhlcvObservationInput {
            snapshot_id: snap.snapshot_id().to_owned(),
            symbol: snap.symbol().to_owned(),
            source_id: source_id.to_owned(),
            quality,
            bars: vec![OhlcvBarInput {
                timeframe,
                source_timestamp_millis: timestamp,
                received_timestamp_millis: timestamp + 1,
                open: low,
                high,
                low,
                close,
                volume: 10.0,
                completed: true,
            }],
        },
    )
    .unwrap()
}

fn valid_series(snap: &MarketSnapshot) -> OhlcvObservationSeries {
    series(
        snap,
        "source-96",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        100.0,
        120.0,
        115.0,
    )
}

fn coverage(
    snap: &MarketSnapshot,
    source: Option<&OhlcvObservationSeries>,
    decision_time: i64,
    age_limit: u64,
) -> RequiredOhlcvCoverage {
    let explicit_limits = RequiredOhlcvAgeLimits {
        minute1_ms: age_limit,
        minute5_ms: age_limit,
        minute15_ms: age_limit,
        hour1_ms: age_limit,
        hour4_ms: age_limit,
    };
    assess_required_ohlcv_coverage(snap, source, decision_time, &explicit_limits).unwrap()
}

#[test]
fn matching_exact_decision_clock_and_age_boundary_yield_immutable_research_evidence() {
    let snap = snapshot("s96", "SPY");
    let source = valid_series(&snap);
    let age = coverage(&snap, Some(&source), 1_010, 210);
    let evidence = calculate_research_close_location_at_decision(
        &snap,
        &source,
        &age,
        OhlcvTimeframe::Minute5,
        1_010,
    )
    .unwrap();
    let repeated = calculate_research_close_location_at_decision(
        &snap,
        &source,
        &age,
        OhlcvTimeframe::Minute5,
        1_010,
    )
    .unwrap();

    assert_eq!(evidence, repeated);
    assert_eq!(evidence.decision_timestamp_millis(), 1_010);
    assert_eq!(evidence.measured_source_age_millis(), 210);
    assert_eq!(evidence.checked_age().measured_source_age_millis(), 210);
    let observed = evidence.checked_age().observation();
    assert_eq!(observed.snapshot_id(), "s96");
    assert_eq!(observed.symbol(), "SPY");
    assert_eq!(observed.source_id(), "source-96");
    assert_eq!(observed.source_quality(), DataQuality::Structured);
    assert_eq!(observed.timeframe(), OhlcvTimeframe::Minute5);
    assert_eq!(observed.source_timestamp_millis(), 800);
    assert_eq!(observed.close_location_ratio(), 0.75);
    assert_eq!(
        observed.provenance(),
        RuleProvenance::ValidationRequiredAssumption
    );
}

#[test]
fn one_millisecond_earlier_or_later_never_reuses_assessment() {
    let snap = snapshot("s96", "SPY");
    let source = valid_series(&snap);
    let age = coverage(&snap, Some(&source), 1_010, 210);
    for changed_decision_time in [1_009, 1_011] {
        assert_eq!(
            calculate_research_close_location_at_decision(
                &snap,
                &source,
                &age,
                OhlcvTimeframe::Minute5,
                changed_decision_time
            ),
            Err(ResearchCloseLocationDecisionError::AssessmentClockMismatch)
        );
    }
}

#[test]
fn negative_and_before_receipt_decision_clocks_fail_closed() {
    let snap = snapshot("s96", "SPY");
    let source = valid_series(&snap);
    let age = coverage(&snap, Some(&source), 1_010, 210);
    for invalid_time in [-1, 999] {
        assert_eq!(
            calculate_research_close_location_at_decision(
                &snap,
                &source,
                &age,
                OhlcvTimeframe::Minute5,
                invalid_time
            ),
            Err(ResearchCloseLocationDecisionError::InvalidDecisionTimeline)
        );
    }

    let invalid_report = coverage(&snap, Some(&source), 999, 1_000);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &source,
            &invalid_report,
            OhlcvTimeframe::Minute5,
            999
        ),
        Err(ResearchCloseLocationDecisionError::InvalidDecisionTimeline)
    );
}

#[test]
fn same_clock_cannot_override_stale_or_missing_scope() {
    let snap = snapshot("s96", "SPY");
    let source = valid_series(&snap);
    let stale = coverage(&snap, Some(&source), 1_011, 210);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &source,
            &stale,
            OhlcvTimeframe::Minute5,
            1_011
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::MissingOrInvalidScopeAge
        ))
    );

    let absent_scope = coverage(&snap, None, 1_010, 210);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &source,
            &absent_scope,
            OhlcvTimeframe::Minute5,
            1_010
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::SourceMismatch
        ))
    );

    let fresh_5m = coverage(&snap, Some(&source), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &source,
            &fresh_5m,
            OhlcvTimeframe::Minute1,
            1_010
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::MissingOrInvalidScopeAge
        ))
    );
}

#[test]
fn mismatched_identity_and_provider_evidence_never_pass() {
    let snap = snapshot("s96", "SPY");
    let source = valid_series(&snap);
    let age = coverage(&snap, Some(&source), 1_010, 210);

    for (other_snapshot, reason) in [
        (
            snapshot("another", "SPY"),
            ResearchCloseLocationAgeError::SnapshotMismatch,
        ),
        (
            snapshot("s96", "QQQ"),
            ResearchCloseLocationAgeError::SymbolMismatch,
        ),
    ] {
        assert_eq!(
            calculate_research_close_location_at_decision(
                &other_snapshot,
                &source,
                &age,
                OhlcvTimeframe::Minute5,
                1_010
            ),
            Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
                reason
            ))
        );
    }

    let other_source = series(
        &snap,
        "different-feed",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        100.0,
        120.0,
        115.0,
    );
    let other_coverage = coverage(&snap, Some(&other_source), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &source,
            &other_coverage,
            OhlcvTimeframe::Minute5,
            1_010
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::SourceMismatch
        ))
    );

    let other_timestamp = series(
        &snap,
        "source-96",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        801,
        100.0,
        120.0,
        115.0,
    );
    let other_time_coverage = coverage(&snap, Some(&other_timestamp), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &source,
            &other_time_coverage,
            OhlcvTimeframe::Minute5,
            1_010
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::SourceTimestampMismatch
        ))
    );

    let other_quality = series(
        &snap,
        "source-96",
        DataQuality::Parsed,
        OhlcvTimeframe::Minute5,
        800,
        100.0,
        120.0,
        115.0,
    );
    let other_quality_coverage = coverage(&snap, Some(&other_quality), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &source,
            &other_quality_coverage,
            OhlcvTimeframe::Minute5,
            1_010
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::SourceMismatch
        ))
    );
}

#[test]
fn undefined_candle_ratio_and_unconfigured_daily_scope_stay_rejected() {
    let snap = snapshot("s96", "SPY");
    let flat = series(
        &snap,
        "source-96",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        100.0,
        100.0,
        100.0,
    );
    let age = coverage(&snap, Some(&flat), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &flat,
            &age,
            OhlcvTimeframe::Minute5,
            1_010
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::InvalidRawObservation(
                ResearchCloseLocationError::ZeroRangeBar
            )
        ))
    );
    assert_eq!(
        calculate_research_close_location_at_decision(
            &snap,
            &flat,
            &age,
            OhlcvTimeframe::Daily,
            1_010
        ),
        Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
            ResearchCloseLocationAgeError::UnsupportedTimeframe
        ))
    );
}
