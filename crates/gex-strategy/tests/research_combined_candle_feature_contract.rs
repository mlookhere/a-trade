use gex_strategy::{
    AiNativeFeatureInput, CandleEventFeatures, DataQuality, LiquidityExecutionFeatures,
    MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, ParticipationFeatures, PortfolioBrokerFeatures,
    RequiredOhlcvAgeLimits, ResearchCloseLocationAtDecision, ResearchCombinedCandleFeatureError,
    ResearchWickAnatomyAtDecision, RuleProvenance, Session, StructureFeatures, TrendFeatures,
    assess_required_ohlcv_coverage, bind_research_combined_candle_feature,
    calculate_research_close_location_at_decision, calculate_research_wick_anatomy_at_decision,
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

fn checked_pair(
    snap: &MarketSnapshot,
    source_id: &str,
    quality: DataQuality,
    timeframe: OhlcvTimeframe,
    bar_time: i64,
    decision_time: i64,
) -> (
    ResearchCloseLocationAtDecision,
    ResearchWickAnatomyAtDecision,
) {
    checked_pair_for_bar(
        snap,
        source_id,
        quality,
        OhlcvBarInput {
            timeframe,
            source_timestamp_millis: bar_time,
            received_timestamp_millis: bar_time + 1,
            open: 105.0,
            high: 120.0,
            low: 100.0,
            close: 110.0,
            volume: 25.0,
            completed: true,
        },
        decision_time,
    )
}

fn checked_pair_for_bar(
    snap: &MarketSnapshot,
    source_id: &str,
    quality: DataQuality,
    bar: OhlcvBarInput,
    decision_time: i64,
) -> (
    ResearchCloseLocationAtDecision,
    ResearchWickAnatomyAtDecision,
) {
    let timeframe = bar.timeframe;
    let bars = OhlcvObservationSeries::build(
        snap,
        OhlcvObservationInput {
            snapshot_id: snap.snapshot_id().to_owned(),
            symbol: snap.symbol().to_owned(),
            source_id: source_id.to_owned(),
            quality,
            bars: vec![bar],
        },
    )
    .unwrap();
    // Test-only, explicit caller configuration; no production default age.
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: 400,
        minute5_ms: 400,
        minute15_ms: 400,
        hour1_ms: 400,
        hour4_ms: 400,
    };
    let coverage =
        assess_required_ohlcv_coverage(snap, Some(&bars), decision_time, &limits).unwrap();
    (
        calculate_research_close_location_at_decision(
            snap,
            &bars,
            &coverage,
            timeframe,
            decision_time,
        )
        .unwrap(),
        calculate_research_wick_anatomy_at_decision(
            snap,
            &bars,
            &coverage,
            timeframe,
            decision_time,
        )
        .unwrap(),
    )
}

#[test]
fn matching_source_candle_yields_only_two_calculator_owned_values() {
    let snap = snapshot("s106", "SPY");
    let (close, wick) = checked_pair(
        &snap,
        "provider-106",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        1_010,
    );
    let a = bind_research_combined_candle_feature(&snap, &close, &wick, 1_010).unwrap();
    let b = bind_research_combined_candle_feature(&snap, &close, &wick, 1_010).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.decision_timestamp_millis(), 1_010);
    assert_eq!(a.features().snapshot_id(), "s106");
    assert_eq!(
        a.features().candle_event(),
        &CandleEventFeatures {
            close_location_value: Some(0.5),
            wick_asymmetry: Some(-0.25),
            ..CandleEventFeatures::default()
        }
    );
    assert_eq!(a.features().trend(), &TrendFeatures::default());
    assert_eq!(a.features().structure(), &StructureFeatures::default());
    assert_eq!(
        a.features().participation(),
        &ParticipationFeatures::default()
    );
    assert_eq!(
        a.features().liquidity_execution(),
        &LiquidityExecutionFeatures::default()
    );
    assert_eq!(
        a.features().portfolio_broker(),
        &PortfolioBrokerFeatures::default()
    );
    assert_eq!(
        AiNativeFeatureInput::default().candle_event.wick_asymmetry,
        None
    );

    let close_obs = a.close_location().checked_age().observation();
    let wick_obs = a.wick_anatomy().observation();
    assert_eq!(close_obs.snapshot_id(), "s106");
    assert_eq!(close_obs.symbol(), "SPY");
    assert_eq!(close_obs.source_id(), "provider-106");
    assert_eq!(close_obs.source_quality(), DataQuality::Structured);
    assert_eq!(close_obs.timeframe(), OhlcvTimeframe::Minute5);
    assert_eq!(close_obs.source_timestamp_millis(), 800);
    assert_eq!(
        close_obs.provenance(),
        RuleProvenance::ValidationRequiredAssumption
    );
    assert_eq!(
        wick_obs.provenance(),
        RuleProvenance::ValidationRequiredAssumption
    );
    assert_eq!(a.close_location().measured_source_age_millis(), 210);
    assert_eq!(a.wick_anatomy().measured_source_age_millis(), 210);
    assert_eq!(a.close_location().decision_timestamp_millis(), 1_010);
    assert_eq!(a.wick_anatomy().decision_timestamp_millis(), 1_010);
}

#[test]
fn foreign_snapshot_and_symbol_cannot_be_composed() {
    let snap = snapshot("s106", "SPY");
    let (close, _) = checked_pair(
        &snap,
        "provider-106",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        1_010,
    );
    for (other_snapshot, expected) in [
        (
            snapshot("other", "SPY"),
            ResearchCombinedCandleFeatureError::SnapshotMismatch,
        ),
        (
            snapshot("s106", "QQQ"),
            ResearchCombinedCandleFeatureError::SymbolMismatch,
        ),
    ] {
        let (_, foreign_wick) = checked_pair(
            &other_snapshot,
            "provider-106",
            DataQuality::Structured,
            OhlcvTimeframe::Minute5,
            800,
            1_010,
        );
        assert_eq!(
            bind_research_combined_candle_feature(&snap, &close, &foreign_wick, 1_010),
            Err(expected)
        );
    }
    let (_, wick) = checked_pair(
        &snap,
        "provider-106",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        1_010,
    );
    assert_eq!(
        bind_research_combined_candle_feature(&snapshot("other", "SPY"), &close, &wick, 1_010),
        Err(ResearchCombinedCandleFeatureError::SnapshotMismatch)
    );
}

#[test]
fn different_provider_quality_timeframe_and_candle_timestamp_reject() {
    let snap = snapshot("s106", "SPY");
    let (close, _) = checked_pair(
        &snap,
        "provider-106",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        1_010,
    );
    for (source, quality, timeframe, timestamp, expected) in [
        (
            "different-provider",
            DataQuality::Structured,
            OhlcvTimeframe::Minute5,
            800,
            ResearchCombinedCandleFeatureError::SourceMismatch,
        ),
        (
            "provider-106",
            DataQuality::Parsed,
            OhlcvTimeframe::Minute5,
            800,
            ResearchCombinedCandleFeatureError::SourceQualityMismatch,
        ),
        (
            "provider-106",
            DataQuality::Structured,
            OhlcvTimeframe::Minute1,
            800,
            ResearchCombinedCandleFeatureError::TimeframeMismatch,
        ),
        (
            "provider-106",
            DataQuality::Structured,
            OhlcvTimeframe::Minute5,
            801,
            ResearchCombinedCandleFeatureError::BarTimestampMismatch,
        ),
    ] {
        let (_, wick) = checked_pair(&snap, source, quality, timeframe, timestamp, 1_010);
        assert_eq!(
            bind_research_combined_candle_feature(&snap, &close, &wick, 1_010),
            Err(expected)
        );
    }
}

#[test]
fn mismatched_assessment_age_and_caller_clock_fail_closed() {
    let snap = snapshot("s106", "SPY");
    let (close, wick) = checked_pair(
        &snap,
        "provider-106",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        1_010,
    );
    for different_clock in [1_009, 1_011, -1] {
        assert_eq!(
            bind_research_combined_candle_feature(&snap, &close, &wick, different_clock),
            Err(ResearchCombinedCandleFeatureError::AssessmentClockMismatch)
        );
    }

    // A genuinely different source-assessment clock necessarily changes
    // measured age for the same bar timestamp; that disagreement must reject.
    let (_, later_wick) = checked_pair(
        &snap,
        "provider-106",
        DataQuality::Structured,
        OhlcvTimeframe::Minute5,
        800,
        1_011,
    );
    assert_eq!(
        bind_research_combined_candle_feature(&snap, &close, &later_wick, 1_010),
        Err(ResearchCombinedCandleFeatureError::SourceAgeMismatch)
    );
}

#[test]
fn same_timestamp_and_provider_cannot_hide_conflicting_completed_ohlcv() {
    let snap = snapshot("s108", "SPY");
    let initial = OhlcvBarInput {
        timeframe: OhlcvTimeframe::Minute5,
        source_timestamp_millis: 800,
        received_timestamp_millis: 801,
        open: 105.0,
        high: 120.0,
        low: 100.0,
        close: 110.0,
        volume: 25.0,
        completed: true,
    };
    let (close, wick) = checked_pair_for_bar(
        &snap,
        "same-provider",
        DataQuality::Structured,
        initial.clone(),
        1_010,
    );
    let observed_close = close.checked_age().observation();
    let observed_wick = wick.observation();

    assert_eq!(observed_close.completed_bar(), observed_wick.completed_bar());
    assert_eq!(observed_close.completed_bar().volume(), 25.0);
    assert_eq!(observed_wick.completed_bar().received_timestamp_millis(), 801);
    assert!(bind_research_combined_candle_feature(&snap, &close, &wick, 1_010).is_ok());

    // Revision 1 has IDENTICAL calculated feature ratios but different OHLC
    // prices. Metadata-only and feature-ratio-only comparisons both miss it.
    let changed_prices = OhlcvBarInput {
        open: 115.0,
        high: 130.0,
        low: 110.0,
        close: 120.0,
        ..initial.clone()
    };
    let changed_volume = OhlcvBarInput {
        volume: 26.0,
        ..initial.clone()
    };
    let changed_received_time = OhlcvBarInput {
        received_timestamp_millis: 802,
        ..initial
    };
    for conflicting in [changed_prices, changed_volume, changed_received_time] {
        let (_, revised_wick) = checked_pair_for_bar(
            &snap,
            "same-provider",
            DataQuality::Structured,
            conflicting,
            1_010,
        );
        let revised = revised_wick.observation();
        assert_eq!(revised.source_id(), observed_close.source_id());
        assert_eq!(revised.timeframe(), observed_close.timeframe());
        assert_eq!(
            revised.source_timestamp_millis(),
            observed_close.source_timestamp_millis()
        );
        assert_eq!(revised.snapshot_id(), observed_close.snapshot_id());
        assert_eq!(revised.symbol(), observed_close.symbol());
        assert_eq!(
            revised.wick_asymmetry_ratio(),
            observed_wick.wick_asymmetry_ratio()
        );
        assert_eq!(
            bind_research_combined_candle_feature(&snap, &close, &revised_wick, 1_010),
            Err(ResearchCombinedCandleFeatureError::BarContentMismatch)
        );
    }
}
