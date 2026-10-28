use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvAgeLimits, RequiredOhlcvCoverage,
    ResearchCloseLocationAgeError, ResearchCloseLocationError, RuleProvenance, Session,
    assess_required_ohlcv_coverage, calculate_research_close_location_with_age,
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
    snapshot: &MarketSnapshot,
    source_id: &str,
    quality: DataQuality,
    bars: &[(OhlcvTimeframe, i64, f64, f64, f64)],
) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: source_id.to_owned(),
            quality,
            bars: bars
                .iter()
                .map(|(timeframe, timestamp, low, high, close)| OhlcvBarInput {
                    timeframe: *timeframe,
                    source_timestamp_millis: *timestamp,
                    received_timestamp_millis: *timestamp + 1,
                    open: *low,
                    high: *high,
                    low: *low,
                    close: *close,
                    volume: 10.0,
                    completed: true,
                })
                .collect(),
        },
    )
    .unwrap()
}

fn coverage(
    snapshot: &MarketSnapshot,
    source: Option<&OhlcvObservationSeries>,
    decision_time: i64,
    limit: u64,
) -> RequiredOhlcvCoverage {
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: limit,
        minute5_ms: limit,
        minute15_ms: limit,
        hour1_ms: limit,
        hour4_ms: limit,
    };
    assess_required_ohlcv_coverage(snapshot, source, decision_time, &limits).unwrap()
}

fn valid_series(snapshot: &MarketSnapshot) -> OhlcvObservationSeries {
    series(
        snapshot,
        "source-92",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute5, 800, 100.0, 120.0, 115.0)],
    )
}

#[test]
fn exact_age_boundary_passes_and_remains_research_only() {
    let snap = snapshot("s92", "SPY");
    let source = valid_series(&snap);
    let age = coverage(&snap, Some(&source), 1_010, 210);
    let proof =
        calculate_research_close_location_with_age(&snap, &source, &age, OhlcvTimeframe::Minute5)
            .unwrap();
    assert_eq!(proof.measured_source_age_millis(), 210);
    assert_eq!(proof.observation().close_location_ratio(), 0.75);
    assert_eq!(proof.observation().snapshot_id(), "s92");
    assert_eq!(proof.observation().symbol(), "SPY");
    assert_eq!(proof.observation().source_id(), "source-92");
    assert_eq!(
        proof.observation().source_quality(),
        DataQuality::Structured
    );
    assert_eq!(proof.observation().timeframe(), OhlcvTimeframe::Minute5);
    assert_eq!(proof.observation().source_timestamp_millis(), 800);
    assert_eq!(
        proof.observation().provenance(),
        RuleProvenance::ValidationRequiredAssumption
    );
}

#[test]
fn source_age_stale_missing_or_invalid_timeline_rejects() {
    let snap = snapshot("s92", "SPY");
    let source = valid_series(&snap);
    for (decision, limit) in [(1_010, 209), (999, 1_000)] {
        let age = coverage(&snap, Some(&source), decision, limit);
        assert_eq!(
            calculate_research_close_location_with_age(
                &snap,
                &source,
                &age,
                OhlcvTimeframe::Minute5
            ),
            Err(ResearchCloseLocationAgeError::MissingOrInvalidScopeAge)
        );
    }
    let only_other = series(
        &snap,
        "source-92",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute15, 800, 100.0, 120.0, 115.0)],
    );
    let absent = coverage(&snap, Some(&only_other), 1_010, 1_000);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap,
            &only_other,
            &absent,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::MissingOrInvalidScopeAge)
    );
    let no_feed = coverage(&snap, None, 1_010, 1_000);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap,
            &source,
            &no_feed,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::SourceMismatch)
    );
}

#[test]
fn mismatch_in_snapshot_symbol_source_or_quality_rejects() {
    let snap = snapshot("s92", "SPY");
    let source = valid_series(&snap);
    let age = coverage(&snap, Some(&source), 1_010, 210);
    for (other_snapshot, error) in [
        (
            snapshot("other", "SPY"),
            ResearchCloseLocationAgeError::SnapshotMismatch,
        ),
        (
            snapshot("s92", "QQQ"),
            ResearchCloseLocationAgeError::SymbolMismatch,
        ),
    ] {
        assert_eq!(
            calculate_research_close_location_with_age(
                &other_snapshot,
                &source,
                &age,
                OhlcvTimeframe::Minute5
            ),
            Err(error)
        );
    }
    let other_source = series(
        &snap,
        "different-source",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute5, 800, 100.0, 120.0, 115.0)],
    );
    let other_age = coverage(&snap, Some(&other_source), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap,
            &source,
            &other_age,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::SourceMismatch)
    );
    let parsed_source = series(
        &snap,
        "source-92",
        DataQuality::Parsed,
        &[(OhlcvTimeframe::Minute5, 800, 100.0, 120.0, 115.0)],
    );
    let parsed_age = coverage(&snap, Some(&parsed_source), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap,
            &source,
            &parsed_age,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::SourceMismatch)
    );
}

#[test]
fn selected_source_timestamp_must_match_exact_last_completed_bar() {
    let snap = snapshot("s92", "SPY");
    let source = valid_series(&snap);
    let same_source_later = series(
        &snap,
        "source-92",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute5, 801, 100.0, 120.0, 115.0)],
    );
    let other_age = coverage(&snap, Some(&same_source_later), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap,
            &source,
            &other_age,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::SourceTimestampMismatch)
    );
    let unrelated = series(
        &snap,
        "source-92",
        DataQuality::Structured,
        &[
            (OhlcvTimeframe::Minute5, 800, 100.0, 120.0, 115.0),
            (OhlcvTimeframe::Minute1, 950, 100.0, 120.0, 120.0),
        ],
    );
    let coverage_other = coverage(&snap, Some(&unrelated), 1_010, 210);
    let checked = calculate_research_close_location_with_age(
        &snap,
        &unrelated,
        &coverage_other,
        OhlcvTimeframe::Minute5,
    )
    .unwrap();
    assert_eq!(checked.measured_source_age_millis(), 210);
    assert_eq!(checked.observation().source_timestamp_millis(), 800);
}

#[test]
fn unsupported_daily_scope_and_invalid_raw_math_never_promote() {
    let snap = snapshot("s92", "SPY");
    let source = valid_series(&snap);
    let age = coverage(&snap, Some(&source), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_with_age(&snap, &source, &age, OhlcvTimeframe::Daily),
        Err(ResearchCloseLocationAgeError::UnsupportedTimeframe)
    );
    let zero_range = series(
        &snap,
        "source-92",
        DataQuality::Structured,
        &[(OhlcvTimeframe::Minute5, 800, 100.0, 100.0, 100.0)],
    );
    let zero_age = coverage(&snap, Some(&zero_range), 1_010, 210);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap,
            &zero_range,
            &zero_age,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::InvalidRawObservation(
            ResearchCloseLocationError::ZeroRangeBar
        ))
    );
}
