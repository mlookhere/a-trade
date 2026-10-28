use gex_strategy::{
    DataQuality, GexScope, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvCoverageError,
    OhlcvObservationInput, OhlcvObservationSeries, OhlcvTimeframe, RawGexLevel,
    RequiredOhlcvAgeLimits, ResearchCloseLocationError, ResearchCloseLocationFeatureError,
    ResearchCombinedCandleFeatureError, ResearchWickAnatomyError, ResearchWickAnatomyFeatureError,
    Session, assess_required_ohlcv_coverage, bind_research_close_location_feature,
    bind_research_combined_candle_feature, bind_research_wick_anatomy_feature,
    calculate_research_close_location, calculate_research_close_location_at_decision,
    calculate_research_wick_anatomy, calculate_research_wick_anatomy_at_decision,
};

fn snapshot(
    source_timestamp_millis: i64,
    received_timestamp_millis: i64,
    spot_price: f64,
    session: Session,
    quality: DataQuality,
    with_gex: bool,
) -> MarketSnapshot {
    let levels = if with_gex {
        vec![RawGexLevel {
            strike: 600.0,
            gex_value: 10.0,
            scope: GexScope::Aggregate,
        }]
    } else {
        vec![]
    };
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "same-ID".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis,
            received_timestamp_millis,
            spot_price,
            session,
            data_quality: quality,
        },
        &levels,
    )
    .unwrap()
}

fn base() -> MarketSnapshot {
    snapshot(950, 1_000, 600.0, Session::Regular, DataQuality::Structured, false)
}

fn series(snap: &MarketSnapshot) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snap,
        OhlcvObservationInput {
            snapshot_id: snap.snapshot_id().to_owned(),
            symbol: snap.symbol().to_owned(),
            source_id: "snapshot-lineage-110".to_owned(),
            quality: DataQuality::Structured,
            bars: vec![OhlcvBarInput {
                timeframe: OhlcvTimeframe::Minute5,
                source_timestamp_millis: 800,
                received_timestamp_millis: 801,
                open: 105.0,
                high: 120.0,
                low: 100.0,
                close: 110.0,
                volume: 25.0,
                completed: true,
            }],
        },
    )
    .unwrap()
}

fn limits() -> RequiredOhlcvAgeLimits {
    RequiredOhlcvAgeLimits {
        minute1_ms: 210,
        minute5_ms: 210,
        minute15_ms: 210,
        hour1_ms: 210,
        hour4_ms: 210,
    }
}

#[test]
fn source_snapshot_retained_and_exact_original_still_accepted() {
    let snap = base();
    let src = series(&snap);
    assert_eq!(src.source_snapshot(), &snap);
    let coverage = assess_required_ohlcv_coverage(&snap, Some(&src), 1_010, &limits()).unwrap();
    let close = calculate_research_close_location_at_decision(
        &snap, &src, &coverage, OhlcvTimeframe::Minute5, 1_010,
    )
    .unwrap();
    let wick = calculate_research_wick_anatomy_at_decision(
        &snap, &src, &coverage, OhlcvTimeframe::Minute5, 1_010,
    )
    .unwrap();
    assert_eq!(close.checked_age().observation().source_snapshot(), &snap);
    assert_eq!(wick.observation().source_snapshot(), &snap);
    assert_eq!(
        bind_research_combined_candle_feature(&snap, &close, &wick, 1_010)
            .unwrap()
            .features()
            .candle_event()
            .close_location_value,
        Some(0.5)
    );
    assert_eq!(
        bind_research_close_location_feature(&snap, &close, 1_010)
            .unwrap()
            .features()
            .candle_event()
            .close_location_value,
        Some(0.5)
    );
    assert_eq!(
        bind_research_wick_anatomy_feature(&snap, &wick, 1_010)
            .unwrap()
            .features()
            .candle_event()
            .wick_asymmetry,
        Some(-0.25)
    );
}

#[test]
fn matching_labels_cannot_substitute_revised_snapshot_contents() {
    let snap = base();
    let src = series(&snap);
    let coverage = assess_required_ohlcv_coverage(&snap, Some(&src), 1_010, &limits()).unwrap();
    let close = calculate_research_close_location_at_decision(
        &snap, &src, &coverage, OhlcvTimeframe::Minute5, 1_010,
    )
    .unwrap();
    let wick = calculate_research_wick_anatomy_at_decision(
        &snap, &src, &coverage, OhlcvTimeframe::Minute5, 1_010,
    )
    .unwrap();
    for changed in [
        snapshot(951, 1_000, 600.0, Session::Regular, DataQuality::Structured, false),
        snapshot(950, 1_001, 600.0, Session::Regular, DataQuality::Structured, false),
        snapshot(950, 1_000, 601.0, Session::Regular, DataQuality::Structured, false),
        snapshot(950, 1_000, 600.0, Session::Premarket, DataQuality::Structured, false),
        snapshot(950, 1_000, 600.0, Session::Regular, DataQuality::Parsed, false),
        snapshot(950, 1_000, 600.0, Session::Regular, DataQuality::Structured, true),
    ] {
        assert_eq!(changed.snapshot_id(), snap.snapshot_id());
        assert_eq!(changed.symbol(), snap.symbol());
        assert_ne!(changed, snap);

        assert_eq!(
            assess_required_ohlcv_coverage(&changed, Some(&src), 1_010, &limits()),
            Err(OhlcvCoverageError::SnapshotContentsMismatch)
        );
        assert_eq!(
            calculate_research_close_location(&changed, &src, OhlcvTimeframe::Minute5),
            Err(ResearchCloseLocationError::SnapshotContentsMismatch)
        );
        assert_eq!(
            calculate_research_wick_anatomy(&changed, &src, OhlcvTimeframe::Minute5),
            Err(ResearchWickAnatomyError::SnapshotContentsMismatch)
        );
        assert_eq!(
            bind_research_close_location_feature(&changed, &close, 1_010),
            Err(ResearchCloseLocationFeatureError::SnapshotContentsMismatch)
        );
        assert_eq!(
            bind_research_wick_anatomy_feature(&changed, &wick, 1_010),
            Err(ResearchWickAnatomyFeatureError::SnapshotContentsMismatch)
        );
        assert_eq!(
            bind_research_combined_candle_feature(&changed, &close, &wick, 1_010),
            Err(ResearchCombinedCandleFeatureError::SnapshotContentsMismatch)
        );
    }
}

#[test]
fn separate_snapshot_identifier_or_symbol_preserves_existing_typed_rejection() {
    let snap = base();
    let src = series(&snap);
    let foreign_id = MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "foreign-ID".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 950,
            received_timestamp_millis: 1_000,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap();
    let foreign_symbol = MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "same-ID".to_owned(),
            symbol: "QQQ".to_owned(),
            source_timestamp_millis: 950,
            received_timestamp_millis: 1_000,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap();

    assert_eq!(
        assess_required_ohlcv_coverage(&foreign_id, Some(&src), 1_010, &limits()),
        Err(OhlcvCoverageError::SnapshotMismatch)
    );
    assert_eq!(
        calculate_research_close_location(&foreign_id, &src, OhlcvTimeframe::Minute5),
        Err(ResearchCloseLocationError::SnapshotMismatch)
    );
    assert_eq!(
        calculate_research_wick_anatomy(&foreign_symbol, &src, OhlcvTimeframe::Minute5),
        Err(ResearchWickAnatomyError::SymbolMismatch)
    );
}
