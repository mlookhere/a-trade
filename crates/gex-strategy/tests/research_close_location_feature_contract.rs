use gex_strategy::{
    AiNativeFeatureInput, CandleEventFeatures, DataQuality, MarketSnapshot, MarketSnapshotInput,
    OhlcvBarInput, OhlcvObservationInput, OhlcvObservationSeries, OhlcvTimeframe,
    RequiredOhlcvAgeLimits, ResearchCloseLocationFeatureError, RuleProvenance, Session,
    TrendFeatures, StructureFeatures, ParticipationFeatures, LiquidityExecutionFeatures,
    PortfolioBrokerFeatures, assess_required_ohlcv_coverage,
    bind_research_close_location_feature, calculate_research_close_location_at_decision,
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

fn series(snapshot: &MarketSnapshot) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: "feature-feed-98".to_owned(),
            quality: DataQuality::Structured,
            bars: vec![OhlcvBarInput {
                timeframe: OhlcvTimeframe::Minute5,
                source_timestamp_millis: 800,
                received_timestamp_millis: 801,
                open: 100.0,
                high: 120.0,
                low: 100.0,
                close: 115.0,
                volume: 30.0,
                completed: true,
            }],
        },
    )
    .unwrap()
}

fn verified_source(
    snap: &MarketSnapshot,
    decision_time: i64,
) -> gex_strategy::ResearchCloseLocationAtDecision {
    let bars = series(snap);
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: 210,
        minute5_ms: 210,
        minute15_ms: 210,
        hour1_ms: 210,
        hour4_ms: 210,
    };
    let coverage = assess_required_ohlcv_coverage(snap, Some(&bars), decision_time, &limits).unwrap();
    calculate_research_close_location_at_decision(
        snap,
        &bars,
        &coverage,
        OhlcvTimeframe::Minute5,
        decision_time,
    )
    .unwrap()
}

#[test]
fn exact_snapshot_and_decision_clock_produce_source_bound_feature() {
    let snap = snapshot("s98", "SPY");
    let source = verified_source(&snap, 1_010);
    let result = bind_research_close_location_feature(&snap, &source, 1_010).unwrap();
    let again = bind_research_close_location_feature(&snap, &source, 1_010).unwrap();

    assert_eq!(result, again);
    assert_eq!(result.decision_timestamp_millis(), 1_010);
    assert_eq!(result.features().snapshot_id(), "s98");
    assert_eq!(result.features().candle_event().close_location_value, Some(0.75));
    assert_eq!(result.source().decision_timestamp_millis(), 1_010);
    assert_eq!(result.source().measured_source_age_millis(), 210);

    let observation = result.source().checked_age().observation();
    assert_eq!(observation.snapshot_id(), "s98");
    assert_eq!(observation.symbol(), "SPY");
    assert_eq!(observation.source_id(), "feature-feed-98");
    assert_eq!(observation.source_quality(), DataQuality::Structured);
    assert_eq!(observation.timeframe(), OhlcvTimeframe::Minute5);
    assert_eq!(observation.source_timestamp_millis(), 800);
    assert_eq!(
        observation.provenance(),
        RuleProvenance::ValidationRequiredAssumption
    );
}

#[test]
fn all_other_feature_fields_remain_unknown_without_zero_fill() {
    let snap = snapshot("s98", "SPY");
    let source = verified_source(&snap, 1_010);
    let result = bind_research_close_location_feature(&snap, &source, 1_010).unwrap();
    let features = result.features();

    let expected_candle = CandleEventFeatures {
        close_location_value: Some(0.75),
        ..CandleEventFeatures::default()
    };
    assert_eq!(features.candle_event(), &expected_candle);
    assert_eq!(features.trend(), &TrendFeatures::default());
    assert_eq!(features.structure(), &StructureFeatures::default());
    assert_eq!(features.participation(), &ParticipationFeatures::default());
    assert_eq!(features.liquidity_execution(), &LiquidityExecutionFeatures::default());
    assert_eq!(features.portfolio_broker(), &PortfolioBrokerFeatures::default());
    assert_eq!(AiNativeFeatureInput::default().candle_event.close_location_value, None);
}

#[test]
fn different_snapshot_identity_and_symbol_fail_without_feature_output() {
    let snap = snapshot("s98", "SPY");
    let source = verified_source(&snap, 1_010);

    assert_eq!(
        bind_research_close_location_feature(&snapshot("other", "SPY"), &source, 1_010),
        Err(ResearchCloseLocationFeatureError::SnapshotMismatch)
    );
    assert_eq!(
        bind_research_close_location_feature(&snapshot("s98", "QQQ"), &source, 1_010),
        Err(ResearchCloseLocationFeatureError::SymbolMismatch)
    );
}

#[test]
fn earlier_and_later_decision_clock_fail_even_by_one_millisecond() {
    let snap = snapshot("s98", "SPY");
    let source = verified_source(&snap, 1_010);
    for decision_clock in [1_009, 1_011, -1] {
        assert_eq!(
            bind_research_close_location_feature(&snap, &source, decision_clock),
            Err(ResearchCloseLocationFeatureError::AssessmentClockMismatch)
        );
    }
}
