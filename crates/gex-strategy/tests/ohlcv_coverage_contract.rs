use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvCoverageError,
    OhlcvObservationInput, OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvAgeLimits,
    Session, TemporalStatus, assess_required_ohlcv_coverage, CANONICAL_REQUIRED_OHLCV_SCOPES,
};

fn snapshot(id: &str, symbol: &str) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: symbol.to_owned(),
            source_timestamp_millis: 1_150,
            received_timestamp_millis: 1_200,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn bar(timeframe: OhlcvTimeframe, source: i64) -> OhlcvBarInput {
    OhlcvBarInput {
        timeframe,
        source_timestamp_millis: source,
        received_timestamp_millis: source + 1,
        open: 600.0,
        high: 603.0,
        low: 599.0,
        close: 602.0,
        volume: 200.0,
        completed: true,
    }
}

fn series(snapshot: &MarketSnapshot, bars: Vec<OhlcvBarInput>) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: "structured-provider".to_owned(),
            quality: DataQuality::Structured,
            bars,
        },
    )
    .unwrap()
}

fn limits(age_ms: u64) -> RequiredOhlcvAgeLimits {
    // Test-only explicit configuration; not a production rule.
    RequiredOhlcvAgeLimits {
        minute1_ms: age_ms,
        minute5_ms: age_ms,
        minute15_ms: age_ms,
        hour1_ms: age_ms,
        hour4_ms: age_ms,
    }
}

#[test]
fn all_five_required_scopes_preserve_identity_and_age_at_exact_limit() {
    let snap = snapshot("s80", "SPY");
    let bars = CANONICAL_REQUIRED_OHLCV_SCOPES
        .into_iter()
        .map(|tf| bar(tf, 1_000))
        .collect();
    let data = series(&snap, bars);
    let report = assess_required_ohlcv_coverage(&snap, Some(&data), 1_210, &limits(210)).unwrap();

    assert_eq!(report.snapshot_id(), "s80");
    assert_eq!(report.symbol(), "SPY");
    assert_eq!(report.source_id(), Some("structured-provider"));
    assert_eq!(report.source_quality(), Some(DataQuality::Structured));
    assert_eq!(report.observations().len(), 5);
    for (observation, expected) in report
        .observations()
        .iter()
        .zip(CANONICAL_REQUIRED_OHLCV_SCOPES)
    {
        assert_eq!(observation.timeframe, expected);
        assert_eq!(observation.status, TemporalStatus::WithinConfiguredAge);
        assert_eq!(observation.source_timestamp_millis, Some(1_000));
        assert_eq!(observation.measured_age_millis, Some(210));
    }
}

#[test]
fn one_millisecond_after_each_timeframe_limit_is_stale() {
    let snap = snapshot("s80", "SPY");
    let data = series(
        &snap,
        CANONICAL_REQUIRED_OHLCV_SCOPES
            .into_iter()
            .map(|tf| bar(tf, 1_000))
            .collect(),
    );
    let report = assess_required_ohlcv_coverage(&snap, Some(&data), 1_211, &limits(210)).unwrap();
    assert!(report.observations().iter().all(|o| {
        o.status == TemporalStatus::Stale && o.measured_age_millis == Some(211)
    }));
}

#[test]
fn partial_or_absent_feeds_remain_missing_and_daily_cannot_substitute() {
    let snap = snapshot("s80", "SPY");
    let report = assess_required_ohlcv_coverage(&snap, None, 1_210, &limits(210)).unwrap();
    assert!(report.observations().iter().all(|o| {
        o.status == TemporalStatus::Missing
            && o.source_timestamp_millis.is_none()
            && o.measured_age_millis.is_none()
    }));
    assert_eq!(report.source_id(), None);

    let data = series(
        &snap,
        vec![
            bar(OhlcvTimeframe::Daily, 1_000),
            bar(OhlcvTimeframe::Minute1, 1_000),
        ],
    );
    let report = assess_required_ohlcv_coverage(&snap, Some(&data), 1_210, &limits(210)).unwrap();
    assert_eq!(report.observations()[0].status, TemporalStatus::WithinConfiguredAge);
    for item in &report.observations()[1..] {
        assert_eq!(item.status, TemporalStatus::Missing);
    }
}

#[test]
fn fresh_one_minute_bar_cannot_hide_stale_four_hour_bar() {
    let snap = snapshot("s80", "SPY");
    let data = series(
        &snap,
        vec![
            bar(OhlcvTimeframe::Minute1, 1_190),
            bar(OhlcvTimeframe::Hour4, 1_000),
        ],
    );
    let report = assess_required_ohlcv_coverage(
        &snap,
        Some(&data),
        1_210,
        &RequiredOhlcvAgeLimits {
            minute1_ms: 20,
            minute5_ms: 60,
            minute15_ms: 60,
            hour1_ms: 60,
            hour4_ms: 200,
        },
    )
    .unwrap();
    assert_eq!(report.observations()[0].status, TemporalStatus::WithinConfiguredAge);
    assert_eq!(report.observations()[1].status, TemporalStatus::Missing);
    assert_eq!(report.observations()[4].status, TemporalStatus::Stale);
}

#[test]
fn source_identity_mismatch_fails_closed_even_with_matching_bar_values() {
    let one = snapshot("s80", "SPY");
    let another = snapshot("s81", "SPY");
    let other_symbol = snapshot("s80", "QQQ");
    let data = series(&one, vec![bar(OhlcvTimeframe::Minute1, 1_190)]);
    assert_eq!(
        assess_required_ohlcv_coverage(&another, Some(&data), 1_210, &limits(210)),
        Err(OhlcvCoverageError::SnapshotMismatch)
    );
    assert_eq!(
        assess_required_ohlcv_coverage(&other_symbol, Some(&data), 1_210, &limits(210)),
        Err(OhlcvCoverageError::SymbolMismatch)
    );
}

#[test]
fn invalid_decision_clock_cannot_certify_timeframe_freshness() {
    let snap = snapshot("s80", "SPY");
    let data = series(&snap, vec![bar(OhlcvTimeframe::Minute1, 1_190)]);
    for now in [1_199, -1] {
        let report = assess_required_ohlcv_coverage(&snap, Some(&data), now, &limits(210))
            .unwrap();
        assert!(report.observations().iter().all(|o| {
            o.status == TemporalStatus::InvalidTimeline && o.measured_age_millis.is_none()
        }));
    }
}

#[test]
fn per_timeframe_zero_limits_are_explicit_not_hidden_defaults() {
    let snap = snapshot("s80", "SPY");
    let data = series(&snap, vec![bar(OhlcvTimeframe::Minute5, 1_190)]);
    let report = assess_required_ohlcv_coverage(&snap, Some(&data), 1_210, &limits(0)).unwrap();
    assert_eq!(report.observations()[1].status, TemporalStatus::Stale);
    assert_eq!(report.observations()[1].measured_age_millis, Some(20));
    // This report does not claim full DATA_VALID, DATA_FRESH or trade ALLOW.
}
