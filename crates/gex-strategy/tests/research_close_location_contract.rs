use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, ResearchCloseLocationError, RuleProvenance, Session,
    calculate_research_close_location,
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
    bars: &[(OhlcvTimeframe, i64, f64, f64, f64)],
) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: "ohlcv-feed-90".to_owned(),
            quality: DataQuality::Structured,
            bars: bars
                .iter()
                .map(|(timeframe, stamp, low, high, close)| OhlcvBarInput {
                    timeframe: *timeframe,
                    source_timestamp_millis: *stamp,
                    received_timestamp_millis: *stamp + 1,
                    open: *low,
                    high: *high,
                    low: *low,
                    close: *close,
                    volume: 20.0,
                    completed: true,
                })
                .collect(),
        },
    )
    .unwrap()
}

#[test]
fn close_location_low_midpoint_high_and_partial_position_are_exact() {
    let snap = snapshot("clv90", "SPY");
    for (close, expected) in [(100.0, 0.0), (110.0, 0.5), (120.0, 1.0), (115.0, 0.75)] {
        let data = series(
            &snap,
            &[(OhlcvTimeframe::Minute5, 750, 100.0, 120.0, close)],
        );
        let observation =
            calculate_research_close_location(&snap, &data, OhlcvTimeframe::Minute5).unwrap();
        assert_eq!(observation.close_location_ratio(), expected);
        assert_eq!(observation.snapshot_id(), "clv90");
        assert_eq!(observation.symbol(), "SPY");
        assert_eq!(observation.source_id(), "ohlcv-feed-90");
        assert_eq!(observation.source_quality(), DataQuality::Structured);
        assert_eq!(observation.timeframe(), OhlcvTimeframe::Minute5);
        assert_eq!(observation.source_timestamp_millis(), 750);
        assert_eq!(
            observation.provenance(),
            RuleProvenance::ValidationRequiredAssumption
        );
    }
}

#[test]
fn latest_completed_source_candle_only_and_repeatable() {
    let snap = snapshot("clv90", "SPY");
    let data = series(
        &snap,
        &[
            (OhlcvTimeframe::Minute5, 200, 100.0, 120.0, 120.0),
            (OhlcvTimeframe::Minute1, 250, 100.0, 120.0, 100.0),
            (OhlcvTimeframe::Minute5, 450, 100.0, 120.0, 110.0),
            (OhlcvTimeframe::Minute1, 650, 100.0, 120.0, 120.0),
        ],
    );
    let one = calculate_research_close_location(&snap, &data, OhlcvTimeframe::Minute5).unwrap();
    let again = calculate_research_close_location(&snap, &data, OhlcvTimeframe::Minute5).unwrap();
    assert_eq!(one, again);
    assert_eq!(one.source_timestamp_millis(), 450);
    assert_eq!(one.close_location_ratio(), 0.5);
    let other = calculate_research_close_location(&snap, &data, OhlcvTimeframe::Minute1).unwrap();
    assert_eq!(other.source_timestamp_millis(), 650);
    assert_eq!(other.close_location_ratio(), 1.0);
}

#[test]
fn zero_range_and_arithmetic_overflow_reject_without_fabricated_ratio() {
    let snap = snapshot("clv90", "SPY");
    let flat = series(
        &snap,
        &[(OhlcvTimeframe::Minute5, 800, 100.0, 100.0, 100.0)],
    );
    assert_eq!(
        calculate_research_close_location(&snap, &flat, OhlcvTimeframe::Minute5),
        Err(ResearchCloseLocationError::ZeroRangeBar)
    );
    let extreme = series(
        &snap,
        &[(OhlcvTimeframe::Minute5, 800, -f64::MAX, f64::MAX, 0.0)],
    );
    assert_eq!(
        calculate_research_close_location(&snap, &extreme, OhlcvTimeframe::Minute5),
        Err(ResearchCloseLocationError::NonFiniteCalculation)
    );
}

#[test]
fn missing_timeframe_or_mismatched_snapshot_and_symbol_fail_closed() {
    let snap = snapshot("clv90", "SPY");
    let data = series(
        &snap,
        &[(OhlcvTimeframe::Minute5, 800, 100.0, 120.0, 110.0)],
    );
    assert_eq!(
        calculate_research_close_location(&snap, &data, OhlcvTimeframe::Hour4),
        Err(ResearchCloseLocationError::MissingCompletedBar)
    );
    assert_eq!(
        calculate_research_close_location(
            &snapshot("other", "SPY"),
            &data,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationError::SnapshotMismatch)
    );
    assert_eq!(
        calculate_research_close_location(
            &snapshot("clv90", "QQQ"),
            &data,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchCloseLocationError::SymbolMismatch)
    );
}
