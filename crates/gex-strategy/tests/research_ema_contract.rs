use gex_strategy::{
    DataQuality, EmaStackState, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput,
    OhlcvObservationInput, OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvAgeLimits,
    RequiredOhlcvCoverage, ResearchEmaConfig, ResearchEmaError, ResearchEmaSeed,
    RuleProvenance, Session, assess_required_ohlcv_coverage, calculate_research_ema_stack,
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
    prices: &[f64],
    timeframe: OhlcvTimeframe,
    offset: i64,
) -> OhlcvObservationSeries {
    let bars = prices
        .iter()
        .enumerate()
        .map(|(i, close)| OhlcvBarInput {
            timeframe,
            source_timestamp_millis: offset + i as i64,
            received_timestamp_millis: offset + i as i64 + 1,
            open: *close,
            high: close + 1.0,
            low: close - 1.0,
            close: *close,
            volume: 100.0,
            completed: true,
        })
        .collect();
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: source_id.to_owned(),
            quality: DataQuality::Structured,
            bars,
        },
    )
    .unwrap()
}

fn coverage(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    allowed_age: u64,
) -> RequiredOhlcvCoverage {
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: allowed_age,
        minute5_ms: allowed_age,
        minute15_ms: allowed_age,
        hour1_ms: allowed_age,
        hour4_ms: allowed_age,
    };
    assess_required_ohlcv_coverage(snapshot, Some(series), 1_010, &limits).unwrap()
}

fn config(seed: ResearchEmaSeed) -> ResearchEmaConfig {
    ResearchEmaConfig {
        timeframe: OhlcvTimeframe::Minute5,
        fast_window: 9,
        middle_window: 21,
        slow_window: 50,
        seed,
    }
}

#[test]
fn research_ema_preserves_explicit_windows_and_bullish_baseline() {
    let snap = snapshot("s82", "SPY");
    let prices: Vec<f64> = (0..60).map(|i| 100.0 + f64::from(i)).collect();
    let data = series(&snap, "source", &prices, OhlcvTimeframe::Minute5, 1);
    let time = coverage(&snap, &data, 2_000);
    let result = calculate_research_ema_stack(
        &snap,
        &data,
        &time,
        &config(ResearchEmaSeed::InitialSimpleAverage),
    )
    .unwrap();
    assert_eq!(result.baseline_stack, EmaStackState::Bullish);
    assert!(result.fast_value > result.middle_value);
    assert!(result.middle_value > result.slow_value);
    assert_eq!(result.snapshot_id, "s82");
    assert_eq!(result.source_id, "source");
    assert_eq!(result.fast_window, 9);
    assert_eq!(result.middle_window, 21);
    assert_eq!(result.slow_window, 50);
    assert_eq!(result.provenance, RuleProvenance::ValidationRequiredAssumption);
}

#[test]
fn research_ema_recognizes_bearish_and_equal_stacks_without_a_veto() {
    let snap = snapshot("s82", "SPY");
    let down: Vec<f64> = (0..60).map(|i| 300.0 - f64::from(i)).collect();
    let data = series(&snap, "source", &down, OhlcvTimeframe::Minute5, 1);
    let time = coverage(&snap, &data, 2_000);
    let result = calculate_research_ema_stack(
        &snap,
        &data,
        &time,
        &config(ResearchEmaSeed::FirstCompletedClose),
    )
    .unwrap();
    assert_eq!(result.baseline_stack, EmaStackState::Bearish);

    let flat = series(&snap, "source", &vec![200.0; 60], OhlcvTimeframe::Minute5, 1);
    let flat_time = coverage(&snap, &flat, 2_000);
    let same = calculate_research_ema_stack(
        &snap,
        &flat,
        &flat_time,
        &config(ResearchEmaSeed::InitialSimpleAverage),
    )
    .unwrap();
    assert_eq!(same.baseline_stack, EmaStackState::Mixed);
    assert!((same.fast_value - 200.0).abs() < 1e-10);
}

#[test]
fn different_explicit_seed_formalizations_are_not_hidden() {
    let snap = snapshot("s82", "SPY");
    let prices: Vec<f64> = (0..60).map(|i| (i * i) as f64 + 10.0).collect();
    let data = series(&snap, "source", &prices, OhlcvTimeframe::Minute5, 1);
    let time = coverage(&snap, &data, 2_000);
    let sma = calculate_research_ema_stack(
        &snap,
        &data,
        &time,
        &config(ResearchEmaSeed::InitialSimpleAverage),
    )
    .unwrap();
    let first = calculate_research_ema_stack(
        &snap,
        &data,
        &time,
        &config(ResearchEmaSeed::FirstCompletedClose),
    )
    .unwrap();
    assert_ne!(sma.slow_value, first.slow_value);
    assert_eq!(sma.seed, ResearchEmaSeed::InitialSimpleAverage);
    assert_eq!(first.seed, ResearchEmaSeed::FirstCompletedClose);
}

#[test]
fn invalid_windows_and_unsupported_daily_scope_reject() {
    let snap = snapshot("s82", "SPY");
    let data = series(&snap, "source", &vec![200.0; 60], OhlcvTimeframe::Minute5, 1);
    let time = coverage(&snap, &data, 2_000);
    for (fast, mid, slow) in [(0, 21, 50), (9, 9, 50), (21, 9, 50), (9, 50, 50)] {
        let mut cfg = config(ResearchEmaSeed::InitialSimpleAverage);
        cfg.fast_window = fast;
        cfg.middle_window = mid;
        cfg.slow_window = slow;
        assert_eq!(
            calculate_research_ema_stack(&snap, &data, &time, &cfg),
            Err(ResearchEmaError::InvalidWindows)
        );
    }
    let mut daily = config(ResearchEmaSeed::FirstCompletedClose);
    daily.timeframe = OhlcvTimeframe::Daily;
    assert_eq!(
        calculate_research_ema_stack(&snap, &data, &time, &daily),
        Err(ResearchEmaError::UnsupportedTimeframe)
    );
}

#[test]
fn unknown_or_stale_source_coverage_never_creates_an_ema() {
    let snap = snapshot("s82", "SPY");
    let data = series(&snap, "source", &vec![200.0; 60], OhlcvTimeframe::Minute5, 1);
    let stale = coverage(&snap, &data, 1);
    assert_eq!(
        calculate_research_ema_stack(
            &snap,
            &data,
            &stale,
            &config(ResearchEmaSeed::InitialSimpleAverage),
        ),
        Err(ResearchEmaError::MissingOrStaleCoverage)
    );
    let missing = series(&snap, "source", &vec![200.0; 60], OhlcvTimeframe::Minute15, 1);
    let missing_age = coverage(&snap, &missing, 2_000);
    assert_eq!(
        calculate_research_ema_stack(
            &snap,
            &missing,
            &missing_age,
            &config(ResearchEmaSeed::FirstCompletedClose),
        ),
        Err(ResearchEmaError::InsufficientCompletedHistory)
    );
}

#[test]
fn insufficient_history_never_zero_fills_missing_ema_warmup() {
    let snap = snapshot("s82", "SPY");
    let data = series(&snap, "source", &vec![200.0; 49], OhlcvTimeframe::Minute5, 1);
    let time = coverage(&snap, &data, 2_000);
    for seed in [ResearchEmaSeed::FirstCompletedClose, ResearchEmaSeed::InitialSimpleAverage] {
        assert_eq!(
            calculate_research_ema_stack(&snap, &data, &time, &config(seed)),
            Err(ResearchEmaError::InsufficientCompletedHistory)
        );
    }
}

#[test]
fn mismatched_snapshot_symbol_or_source_cannot_be_used() {
    let snap = snapshot("s82", "SPY");
    let prices = vec![200.0; 60];
    let data = series(&snap, "source", &prices, OhlcvTimeframe::Minute5, 1);
    let time = coverage(&snap, &data, 2_000);
    let cfg = config(ResearchEmaSeed::InitialSimpleAverage);
    assert_eq!(
        calculate_research_ema_stack(&snapshot("other", "SPY"), &data, &time, &cfg),
        Err(ResearchEmaError::SnapshotMismatch)
    );
    assert_eq!(
        calculate_research_ema_stack(&snapshot("s82", "QQQ"), &data, &time, &cfg),
        Err(ResearchEmaError::SymbolMismatch)
    );
    let alt_source = series(&snap, "alternate", &prices, OhlcvTimeframe::Minute5, 1);
    assert_eq!(
        calculate_research_ema_stack(&snap, &alt_source, &time, &cfg),
        Err(ResearchEmaError::SourceMismatch)
    );
}

#[test]
fn coverage_must_match_the_actual_latest_completed_source_bar() {
    let snap = snapshot("s82", "SPY");
    let prices = vec![200.0; 60];
    let source = series(&snap, "source", &prices, OhlcvTimeframe::Minute5, 1);
    let old = series(&snap, "source", &prices, OhlcvTimeframe::Minute5, 2);
    let old_coverage = coverage(&snap, &old, 2_000);
    assert_eq!(
        calculate_research_ema_stack(
            &snap,
            &source,
            &old_coverage,
            &config(ResearchEmaSeed::InitialSimpleAverage),
        ),
        Err(ResearchEmaError::CoverageBarMismatch)
    );
}
