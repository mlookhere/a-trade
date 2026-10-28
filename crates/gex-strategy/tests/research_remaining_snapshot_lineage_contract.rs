use gex_strategy::{
    DataQuality, GexScope, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput,
    OhlcvObservationInput, OhlcvObservationSeries, OhlcvSourceSpacingError, OhlcvTimeframe,
    RawGexLevel, RequiredOhlcvAgeLimits, ResearchEmaConfig, ResearchEmaError, ResearchEmaSeed,
    Session, assess_required_ohlcv_coverage, calculate_research_ema_stack,
    measure_ohlcv_source_spacing,
};

fn snapshot(
    id: &str,
    symbol: &str,
    source_time: i64,
    received_time: i64,
    spot: f64,
    session: Session,
    quality: DataQuality,
    gex: bool,
) -> MarketSnapshot {
    let levels = if gex {
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
            snapshot_id: id.to_owned(),
            symbol: symbol.to_owned(),
            source_timestamp_millis: source_time,
            received_timestamp_millis: received_time,
            spot_price: spot,
            session,
            data_quality: quality,
        },
        &levels,
    )
    .unwrap()
}

fn base() -> MarketSnapshot {
    snapshot(
        "same-ID",
        "SPY",
        950,
        1_000,
        600.0,
        Session::Regular,
        DataQuality::Structured,
        false,
    )
}

fn series(snap: &MarketSnapshot) -> OhlcvObservationSeries {
    let bars = [(800, 100.0), (850, 110.0), (900, 120.0)]
        .into_iter()
        .map(|(timestamp, close)| OhlcvBarInput {
            timeframe: OhlcvTimeframe::Minute5,
            source_timestamp_millis: timestamp,
            received_timestamp_millis: timestamp + 1,
            open: close,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume: 10.0,
            completed: true,
        })
        .collect();
    OhlcvObservationSeries::build(
        snap,
        OhlcvObservationInput {
            snapshot_id: snap.snapshot_id().to_owned(),
            symbol: snap.symbol().to_owned(),
            source_id: "lineage-112".to_owned(),
            quality: DataQuality::Structured,
            bars,
        },
    )
    .unwrap()
}

fn age_limits(max_ms: u64) -> RequiredOhlcvAgeLimits {
    RequiredOhlcvAgeLimits {
        minute1_ms: max_ms,
        minute5_ms: max_ms,
        minute15_ms: max_ms,
        hour1_ms: max_ms,
        hour4_ms: max_ms,
    }
}

fn ema_config() -> ResearchEmaConfig {
    // Test-only windows, not production configuration.
    ResearchEmaConfig {
        timeframe: OhlcvTimeframe::Minute5,
        fast_window: 1,
        middle_window: 2,
        slow_window: 3,
        seed: ResearchEmaSeed::InitialSimpleAverage,
    }
}

#[test]
fn exactly_matching_immutable_source_snapshot_allows_research_observations() {
    let snap = base();
    let src = series(&snap);
    assert_eq!(src.source_snapshot(), &snap);
    let coverage =
        assess_required_ohlcv_coverage(&snap, Some(&src), 1_010, &age_limits(210)).unwrap();
    let calculated = calculate_research_ema_stack(&snap, &src, &coverage, &ema_config()).unwrap();
    assert_eq!(calculated.snapshot_id(), snap.snapshot_id());
    assert_eq!(calculated.source_id(), "lineage-112");
    assert_eq!(calculated.fast_window(), 1);
    assert_eq!(calculated.middle_window(), 2);
    assert_eq!(calculated.slow_window(), 3);
    assert_eq!(calculated.seed(), ResearchEmaSeed::InitialSimpleAverage);

    let observed = measure_ohlcv_source_spacing(&snap, &src, OhlcvTimeframe::Minute5).unwrap();
    assert_eq!(observed.snapshot_id(), "same-ID");
    assert_eq!(observed.source_id(), "lineage-112");
    assert_eq!(observed.intervals().len(), 2);
    assert_eq!(observed.intervals()[0].elapsed_millis(), 50);
    assert_eq!(observed.intervals()[1].elapsed_millis(), 50);
    assert_eq!(
        measure_ohlcv_source_spacing(&snap, &src, OhlcvTimeframe::Minute5).unwrap(),
        observed
    );
}

#[test]
fn identical_id_and_symbol_with_revised_snapshot_contents_fail_closed() {
    let original = base();
    let src = series(&original);
    let coverage =
        assess_required_ohlcv_coverage(&original, Some(&src), 1_010, &age_limits(210)).unwrap();
    for changed in [
        snapshot(
            "same-ID",
            "SPY",
            951,
            1_000,
            600.0,
            Session::Regular,
            DataQuality::Structured,
            false,
        ),
        snapshot(
            "same-ID",
            "SPY",
            950,
            1_001,
            600.0,
            Session::Regular,
            DataQuality::Structured,
            false,
        ),
        snapshot(
            "same-ID",
            "SPY",
            950,
            1_000,
            601.0,
            Session::Regular,
            DataQuality::Structured,
            false,
        ),
        snapshot(
            "same-ID",
            "SPY",
            950,
            1_000,
            600.0,
            Session::Premarket,
            DataQuality::Structured,
            false,
        ),
        snapshot(
            "same-ID",
            "SPY",
            950,
            1_000,
            600.0,
            Session::Regular,
            DataQuality::Parsed,
            false,
        ),
        snapshot(
            "same-ID",
            "SPY",
            950,
            1_000,
            600.0,
            Session::Regular,
            DataQuality::Structured,
            true,
        ),
    ] {
        assert_eq!(changed.snapshot_id(), original.snapshot_id());
        assert_eq!(changed.symbol(), original.symbol());
        assert_ne!(changed, original);
        assert_eq!(
            calculate_research_ema_stack(&changed, &src, &coverage, &ema_config()),
            Err(ResearchEmaError::SnapshotContentsMismatch)
        );
        assert_eq!(
            measure_ohlcv_source_spacing(&changed, &src, OhlcvTimeframe::Minute5),
            Err(OhlcvSourceSpacingError::SnapshotContentsMismatch)
        );
    }
}

#[test]
fn different_id_and_symbol_keep_preexisting_rejection_codes() {
    let snap = base();
    let src = series(&snap);
    let coverage =
        assess_required_ohlcv_coverage(&snap, Some(&src), 1_010, &age_limits(210)).unwrap();
    let wrong_id = snapshot(
        "different",
        "SPY",
        950,
        1_000,
        600.0,
        Session::Regular,
        DataQuality::Structured,
        false,
    );
    let wrong_symbol = snapshot(
        "same-ID",
        "QQQ",
        950,
        1_000,
        600.0,
        Session::Regular,
        DataQuality::Structured,
        false,
    );
    assert_eq!(
        calculate_research_ema_stack(&wrong_id, &src, &coverage, &ema_config()),
        Err(ResearchEmaError::SnapshotMismatch)
    );
    assert_eq!(
        measure_ohlcv_source_spacing(&wrong_id, &src, OhlcvTimeframe::Minute5),
        Err(OhlcvSourceSpacingError::SnapshotMismatch)
    );
    assert_eq!(
        calculate_research_ema_stack(&wrong_symbol, &src, &coverage, &ema_config()),
        Err(ResearchEmaError::SymbolMismatch)
    );
    assert_eq!(
        measure_ohlcv_source_spacing(&wrong_symbol, &src, OhlcvTimeframe::Minute5),
        Err(OhlcvSourceSpacingError::SymbolMismatch)
    );
}

#[test]
fn stale_and_missing_ema_scope_still_reject_without_substituting_data() {
    let snap = base();
    let src = series(&snap);
    let stale = assess_required_ohlcv_coverage(&snap, Some(&src), 1_010, &age_limits(109)).unwrap();
    assert_eq!(
        calculate_research_ema_stack(&snap, &src, &stale, &ema_config()),
        Err(ResearchEmaError::MissingOrStaleCoverage)
    );
    let missing = assess_required_ohlcv_coverage(&snap, None, 1_010, &age_limits(210)).unwrap();
    assert_eq!(
        calculate_research_ema_stack(&snap, &src, &missing, &ema_config()),
        Err(ResearchEmaError::SourceMismatch)
    );
}
