use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, ResearchWickAnatomyError, RuleProvenance, Session,
    calculate_research_wick_anatomy,
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
    bars: &[(OhlcvTimeframe, i64, f64, f64, f64, f64)],
) -> OhlcvObservationSeries {
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: "wick-source-100".to_owned(),
            quality: DataQuality::Structured,
            bars: bars
                .iter()
                .map(|(timeframe, stamp, open, high, low, close)| OhlcvBarInput {
                    timeframe: *timeframe,
                    source_timestamp_millis: *stamp,
                    received_timestamp_millis: *stamp + 1,
                    open: *open,
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
fn signed_wick_anatomy_is_deterministic_and_not_a_direction_gate() {
    let snap = snapshot("wick100", "SPY");
    for (open, close, expected) in [
        (100.0, 100.0, -1.0),
        (120.0, 120.0, 1.0),
        (110.0, 110.0, 0.0),
        (100.0, 120.0, 0.0),
        (120.0, 100.0, 0.0),
        (115.0, 110.0, 0.25),
        (105.0, 110.0, -0.25),
    ] {
        let data = series(
            &snap,
            &[(OhlcvTimeframe::Minute5, 750, open, 120.0, 100.0, close)],
        );
        let observed =
            calculate_research_wick_anatomy(&snap, &data, OhlcvTimeframe::Minute5).unwrap();
        let again = calculate_research_wick_anatomy(&snap, &data, OhlcvTimeframe::Minute5).unwrap();
        assert_eq!(observed, again);
        assert_eq!(observed.wick_asymmetry_ratio(), expected);
        assert_eq!(observed.snapshot_id(), "wick100");
        assert_eq!(observed.symbol(), "SPY");
        assert_eq!(observed.source_id(), "wick-source-100");
        assert_eq!(observed.source_quality(), DataQuality::Structured);
        assert_eq!(observed.timeframe(), OhlcvTimeframe::Minute5);
        assert_eq!(observed.source_timestamp_millis(), 750);
        assert_eq!(
            observed.provenance(),
            RuleProvenance::ValidationRequiredAssumption
        );
    }
}

#[test]
fn latest_completed_selected_timeframe_does_not_borrow_another_bar() {
    let snap = snapshot("wick100", "SPY");
    let data = series(
        &snap,
        &[
            (OhlcvTimeframe::Minute5, 150, 100.0, 120.0, 100.0, 100.0),
            (OhlcvTimeframe::Minute1, 300, 120.0, 120.0, 100.0, 120.0),
            (OhlcvTimeframe::Minute5, 450, 110.0, 120.0, 100.0, 110.0),
            (OhlcvTimeframe::Minute1, 500, 100.0, 120.0, 100.0, 100.0),
            (OhlcvTimeframe::Minute5, 650, 120.0, 120.0, 100.0, 120.0),
        ],
    );
    let five = calculate_research_wick_anatomy(&snap, &data, OhlcvTimeframe::Minute5).unwrap();
    assert_eq!(five.source_timestamp_millis(), 650);
    assert_eq!(five.wick_asymmetry_ratio(), 1.0);

    let one = calculate_research_wick_anatomy(&snap, &data, OhlcvTimeframe::Minute1).unwrap();
    assert_eq!(one.source_timestamp_millis(), 500);
    assert_eq!(one.wick_asymmetry_ratio(), -1.0);
}

#[test]
fn zero_range_and_overflow_reject_instead_of_fabricating_wick_ratio() {
    let snap = snapshot("wick100", "SPY");
    let flat = series(
        &snap,
        &[(OhlcvTimeframe::Minute5, 800, 100.0, 100.0, 100.0, 100.0)],
    );
    assert_eq!(
        calculate_research_wick_anatomy(&snap, &flat, OhlcvTimeframe::Minute5),
        Err(ResearchWickAnatomyError::ZeroRangeBar)
    );

    let extreme = series(
        &snap,
        &[(OhlcvTimeframe::Minute5, 800, 0.0, f64::MAX, -f64::MAX, 0.0)],
    );
    assert_eq!(
        calculate_research_wick_anatomy(&snap, &extreme, OhlcvTimeframe::Minute5),
        Err(ResearchWickAnatomyError::NonFiniteCalculation)
    );
}

#[test]
fn absent_scope_or_mismatched_snapshot_and_symbol_fail_closed() {
    let snap = snapshot("wick100", "SPY");
    let data = series(
        &snap,
        &[(OhlcvTimeframe::Minute5, 800, 110.0, 120.0, 100.0, 110.0)],
    );
    assert_eq!(
        calculate_research_wick_anatomy(&snap, &data, OhlcvTimeframe::Hour4),
        Err(ResearchWickAnatomyError::MissingCompletedBar)
    );
    assert_eq!(
        calculate_research_wick_anatomy(&snapshot("other", "SPY"), &data, OhlcvTimeframe::Minute5),
        Err(ResearchWickAnatomyError::SnapshotMismatch)
    );
    assert_eq!(
        calculate_research_wick_anatomy(
            &snapshot("wick100", "QQQ"),
            &data,
            OhlcvTimeframe::Minute5
        ),
        Err(ResearchWickAnatomyError::SymbolMismatch)
    );
}
