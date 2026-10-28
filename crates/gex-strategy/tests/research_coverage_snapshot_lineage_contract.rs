use gex_strategy::{
    DataQuality, GexScope, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput,
    OhlcvObservationInput, OhlcvObservationSeries, OhlcvTimeframe, RawGexLevel,
    RequiredOhlcvAgeLimits, RequiredOhlcvCoverage, ResearchCloseLocationAgeError,
    ResearchCloseLocationDecisionError, ResearchEmaConfig, ResearchEmaError, ResearchEmaSeed,
    ResearchWickAnatomyDecisionError, Session, assess_required_ohlcv_coverage,
    calculate_research_close_location_at_decision, calculate_research_close_location_with_age,
    calculate_research_ema_stack, calculate_research_wick_anatomy_at_decision,
};

fn input() -> MarketSnapshotInput {
    MarketSnapshotInput {
        snapshot_id: "lineage116".to_owned(),
        symbol: "SPY".to_owned(),
        source_timestamp_millis: 950,
        received_timestamp_millis: 1_000,
        spot_price: 600.0,
        session: Session::Regular,
        data_quality: DataQuality::Structured,
    }
}

fn snapshot(values: MarketSnapshotInput, gex: &[RawGexLevel]) -> MarketSnapshot {
    MarketSnapshot::build(values, gex).unwrap()
}

fn bars(snapshot: &MarketSnapshot) -> OhlcvObservationSeries {
    let obs = [(800, 100.0), (850, 110.0), (900, 120.0)]
        .into_iter()
        .map(|(stamp, close)| OhlcvBarInput {
            timeframe: OhlcvTimeframe::Minute5,
            source_timestamp_millis: stamp,
            received_timestamp_millis: stamp + 1,
            open: close,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume: 10.0,
            completed: true,
        })
        .collect();
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: "same-provider".to_owned(),
            quality: DataQuality::Structured,
            bars: obs,
        },
    )
    .unwrap()
}

fn coverage(
    snapshot: &MarketSnapshot,
    source: Option<&OhlcvObservationSeries>,
    limit: u64,
) -> RequiredOhlcvCoverage {
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: limit,
        minute5_ms: limit,
        minute15_ms: limit,
        hour1_ms: limit,
        hour4_ms: limit,
    };
    assess_required_ohlcv_coverage(snapshot, source, 1_010, &limits).unwrap()
}

fn config() -> ResearchEmaConfig {
    ResearchEmaConfig {
        timeframe: OhlcvTimeframe::Minute5,
        fast_window: 1,
        middle_window: 2,
        slow_window: 3,
        seed: ResearchEmaSeed::FirstCompletedClose,
    }
}

#[test]
fn exact_snapshot_and_source_coverage_succeed_without_new_strategy_rules() {
    let snap = snapshot(input(), &[]);
    let source = bars(&snap);
    let age = coverage(&snap, Some(&source), 110);
    assert_eq!(age.source_snapshot(), &snap);
    assert_eq!(source.source_snapshot(), &snap);
    assert!(
        calculate_research_close_location_with_age(
            &snap, &source, &age, OhlcvTimeframe::Minute5
        )
        .is_ok()
    );
    assert!(
        calculate_research_close_location_at_decision(
            &snap, &source, &age, OhlcvTimeframe::Minute5, 1_010
        )
        .is_ok()
    );
    assert!(
        calculate_research_wick_anatomy_at_decision(
            &snap, &source, &age, OhlcvTimeframe::Minute5, 1_010
        )
        .is_ok()
    );
    assert!(calculate_research_ema_stack(&snap, &source, &age, &config()).is_ok());
}

#[test]
fn different_snapshot_contents_with_identical_labels_and_candle_are_not_interchangeable() {
    let original = snapshot(input(), &[]);
    let source = bars(&original);
    let original_report = coverage(&original, Some(&source), 110);
    let altered_gex = [RawGexLevel {
        strike: 601.0,
        gex_value: 10.0,
        scope: GexScope::Aggregate,
    }];
    let changes = [
        (MarketSnapshotInput { source_timestamp_millis: 951, ..input() }, false),
        (MarketSnapshotInput { received_timestamp_millis: 1_001, ..input() }, false),
        (MarketSnapshotInput { spot_price: 601.0, ..input() }, false),
        (MarketSnapshotInput { session: Session::Premarket, ..input() }, false),
        (MarketSnapshotInput { data_quality: DataQuality::Parsed, ..input() }, false),
        (input(), true),
    ];
    for (revised_input, with_gex) in changes {
        let levels: &[RawGexLevel] = if with_gex { &altered_gex } else { &[] };
        let revised_snapshot = snapshot(revised_input, levels);
        let revised_series = bars(&revised_snapshot);
        let revised_report = coverage(&revised_snapshot, Some(&revised_series), 110);
        assert_eq!(revised_snapshot.snapshot_id(), original.snapshot_id());
        assert_eq!(revised_snapshot.symbol(), original.symbol());
        assert_eq!(revised_series.source_id(), source.source_id());
        assert_eq!(
            revised_series.latest_completed(OhlcvTimeframe::Minute5),
            source.latest_completed(OhlcvTimeframe::Minute5)
        );
        assert_eq!(revised_report.decision_timestamp_millis(), 1_010);
        assert_ne!(revised_report.source_snapshot(), original_report.source_snapshot());
        assert_eq!(
            calculate_research_close_location_with_age(
                &original, &source, &revised_report, OhlcvTimeframe::Minute5
            ),
            Err(ResearchCloseLocationAgeError::SnapshotContentsMismatch)
        );
        assert_eq!(
            calculate_research_close_location_at_decision(
                &original, &source, &revised_report, OhlcvTimeframe::Minute5, 1_010
            ),
            Err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence(
                ResearchCloseLocationAgeError::SnapshotContentsMismatch
            ))
        );
        assert_eq!(
            calculate_research_wick_anatomy_at_decision(
                &original, &source, &revised_report, OhlcvTimeframe::Minute5, 1_010
            ),
            Err(ResearchWickAnatomyDecisionError::SnapshotContentsMismatch)
        );
        assert_eq!(
            calculate_research_ema_stack(&original, &source, &revised_report, &config()),
            Err(ResearchEmaError::SnapshotContentsMismatch)
        );
    }
}

#[test]
fn mismatched_id_symbol_and_missing_or_stale_inputs_preserve_existing_rejections() {
    let snap = snapshot(input(), &[]);
    let source = bars(&snap);
    let wrong_id = snapshot(
        MarketSnapshotInput { snapshot_id: "other".to_owned(), ..input() },
        &[],
    );
    let wrong_symbol = snapshot(
        MarketSnapshotInput { symbol: "QQQ".to_owned(), ..input() },
        &[],
    );
    let foreign_report = coverage(&wrong_id, Some(&bars(&wrong_id)), 110);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap, &source, &foreign_report, OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::SnapshotMismatch)
    );
    assert_eq!(
        calculate_research_wick_anatomy_at_decision(
            &snap, &source, &foreign_report, OhlcvTimeframe::Minute5, 1_010
        ),
        Err(ResearchWickAnatomyDecisionError::SnapshotMismatch)
    );
    let foreign_symbol_report = coverage(&wrong_symbol, Some(&bars(&wrong_symbol)), 110);
    assert_eq!(
        calculate_research_ema_stack(&snap, &source, &foreign_symbol_report, &config()),
        Err(ResearchEmaError::SymbolMismatch)
    );
    let stale = coverage(&snap, Some(&source), 109);
    assert_eq!(
        calculate_research_close_location_with_age(
            &snap, &source, &stale, OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationAgeError::MissingOrInvalidScopeAge)
    );
    assert_eq!(
        calculate_research_wick_anatomy_at_decision(
            &snap, &source, &stale, OhlcvTimeframe::Minute5, 1_010
        ),
        Err(ResearchWickAnatomyDecisionError::MissingOrInvalidScopeAge)
    );
    assert_eq!(
        calculate_research_ema_stack(&snap, &source, &stale, &config()),
        Err(ResearchEmaError::MissingOrStaleCoverage)
    );
    let missing = coverage(&snap, None, 110);
    assert_eq!(
        calculate_research_ema_stack(&snap, &source, &missing, &config()),
        Err(ResearchEmaError::SourceMismatch)
    );
}
