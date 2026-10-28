use gex_strategy::{
    AiNativeFeatureInput, CandleEventFeatures, DataQuality, LiquidityExecutionFeatures,
    MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, ParticipationFeatures, PortfolioBrokerFeatures,
    RequiredOhlcvAgeLimits, ResearchWickAnatomyFeatureError, RuleProvenance, Session,
    StructureFeatures, TrendFeatures, assess_required_ohlcv_coverage,
    bind_research_wick_anatomy_feature, calculate_research_wick_anatomy_at_decision,
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

fn series(snapshot: &MarketSnapshot, open: f64, close: f64) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: "wick-feature-104".to_owned(),
            quality: DataQuality::Structured,
            bars: vec![OhlcvBarInput {
                timeframe: OhlcvTimeframe::Minute5,
                source_timestamp_millis: 800,
                received_timestamp_millis: 801,
                open,
                high: 120.0,
                low: 100.0,
                close,
                volume: 25.0,
                completed: true,
            }],
        },
    )
    .unwrap()
}

fn verified_source(
    snap: &MarketSnapshot,
    open: f64,
    close: f64,
) -> gex_strategy::ResearchWickAnatomyAtDecision {
    let bars = series(snap, open, close);
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: 210,
        minute5_ms: 210,
        minute15_ms: 210,
        hour1_ms: 210,
        hour4_ms: 210,
    };
    let coverage = assess_required_ohlcv_coverage(snap, Some(&bars), 1_010, &limits).unwrap();
    calculate_research_wick_anatomy_at_decision(
        snap,
        &bars,
        &coverage,
        OhlcvTimeframe::Minute5,
        1_010,
    )
    .unwrap()
}

#[test]
fn exact_snapshot_and_decision_clock_preserve_calculator_source_and_sign() {
    let snap = snapshot("s104", "SPY");
    for (open, close, expected) in [(105.0, 110.0, -0.25), (115.0, 110.0, 0.25)] {
        let source = verified_source(&snap, open, close);
        let result = bind_research_wick_anatomy_feature(&snap, &source, 1_010).unwrap();
        let again = bind_research_wick_anatomy_feature(&snap, &source, 1_010).unwrap();

        assert_eq!(result, again);
        assert_eq!(result.features().snapshot_id(), "s104");
        assert_eq!(
            result.features().candle_event().wick_asymmetry,
            Some(expected)
        );
        assert_eq!(result.decision_timestamp_millis(), 1_010);
        assert_eq!(result.source().decision_timestamp_millis(), 1_010);
        assert_eq!(result.source().measured_source_age_millis(), 210);

        let observed = result.source().observation();
        assert_eq!(observed.snapshot_id(), "s104");
        assert_eq!(observed.symbol(), "SPY");
        assert_eq!(observed.source_id(), "wick-feature-104");
        assert_eq!(observed.source_quality(), DataQuality::Structured);
        assert_eq!(observed.timeframe(), OhlcvTimeframe::Minute5);
        assert_eq!(observed.source_timestamp_millis(), 800);
        assert_eq!(observed.wick_asymmetry_ratio(), expected);
        assert_eq!(
            observed.provenance(),
            RuleProvenance::ValidationRequiredAssumption
        );
    }
}

#[test]
fn unrelated_numeric_features_remain_none_even_when_wick_feature_is_present() {
    let snap = snapshot("s104", "SPY");
    let source = verified_source(&snap, 105.0, 110.0);
    let result = bind_research_wick_anatomy_feature(&snap, &source, 1_010).unwrap();
    let features = result.features();

    assert_eq!(
        features.candle_event(),
        &CandleEventFeatures {
            wick_asymmetry: Some(-0.25),
            ..CandleEventFeatures::default()
        }
    );
    assert_eq!(features.trend(), &TrendFeatures::default());
    assert_eq!(features.structure(), &StructureFeatures::default());
    assert_eq!(features.participation(), &ParticipationFeatures::default());
    assert_eq!(
        features.liquidity_execution(),
        &LiquidityExecutionFeatures::default()
    );
    assert_eq!(
        features.portfolio_broker(),
        &PortfolioBrokerFeatures::default()
    );
    assert_eq!(
        AiNativeFeatureInput::default().candle_event.wick_asymmetry,
        None
    );
    assert_eq!(features.candle_event().close_location_value, None);
}

#[test]
fn wrong_snapshot_or_symbol_cannot_receive_a_source_bound_feature() {
    let snap = snapshot("s104", "SPY");
    let source = verified_source(&snap, 105.0, 110.0);

    assert_eq!(
        bind_research_wick_anatomy_feature(&snapshot("other", "SPY"), &source, 1_010),
        Err(ResearchWickAnatomyFeatureError::SnapshotMismatch)
    );
    assert_eq!(
        bind_research_wick_anatomy_feature(&snapshot("s104", "QQQ"), &source, 1_010),
        Err(ResearchWickAnatomyFeatureError::SymbolMismatch)
    );
}

#[test]
fn stale_or_different_decision_clock_never_reuses_research_evidence() {
    let snap = snapshot("s104", "SPY");
    let source = verified_source(&snap, 105.0, 110.0);
    for clock in [1_009, 1_011, -1] {
        assert_eq!(
            bind_research_wick_anatomy_feature(&snap, &source, clock),
            Err(ResearchWickAnatomyFeatureError::AssessmentClockMismatch)
        );
    }
}

#[test]
fn wick_extremes_symmetry_and_full_body_remain_signed_research_descriptions() {
    let snap = snapshot("s104", "SPY");
    for (open, close, expected) in [
        (100.0, 100.0, -1.0),
        (120.0, 120.0, 1.0),
        (110.0, 110.0, 0.0),
        (100.0, 120.0, 0.0),
    ] {
        let source = verified_source(&snap, open, close);
        let evidence = bind_research_wick_anatomy_feature(&snap, &source, 1_010).unwrap();
        assert_eq!(
            evidence.features().candle_event().wick_asymmetry,
            Some(expected)
        );
        assert_eq!(
            evidence.source().observation().wick_asymmetry_ratio(),
            expected
        );
        assert_eq!(
            evidence.features().candle_event().close_location_value,
            None
        );
    }
}
