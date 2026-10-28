use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvSourceSpacingError, OhlcvTimeframe, RuleProvenance,
    Session, measure_ohlcv_source_spacing,
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
    observations: &[(OhlcvTimeframe, i64)],
) -> OhlcvObservationSeries {
    let bars = observations
        .iter()
        .map(|(timeframe, stamp)| OhlcvBarInput {
            timeframe: *timeframe,
            source_timestamp_millis: *stamp,
            received_timestamp_millis: stamp + 1,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 10.0,
            completed: true,
        })
        .collect();
    OhlcvObservationSeries::build(
        snapshot,
        OhlcvObservationInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            symbol: snapshot.symbol().to_owned(),
            source_id: "source-feed-88".to_owned(),
            quality: DataQuality::Structured,
            bars,
        },
    )
    .unwrap()
}

#[test]
fn source_intervals_preserve_observed_pairs_and_identity_without_gap_verdict() {
    let snap = snapshot("s88", "SPY");
    let data = series(
        &snap,
        &[
            (OhlcvTimeframe::Minute5, 100),
            (OhlcvTimeframe::Minute1, 120),
            (OhlcvTimeframe::Minute5, 200),
            (OhlcvTimeframe::Minute1, 225),
            (OhlcvTimeframe::Minute5, 350),
            (OhlcvTimeframe::Minute5, 900),
        ],
    );
    let observation = measure_ohlcv_source_spacing(&snap, &data, OhlcvTimeframe::Minute5)
        .unwrap();
    assert_eq!(observation.snapshot_id(), "s88");
    assert_eq!(observation.symbol(), "SPY");
    assert_eq!(observation.source_id(), "source-feed-88");
    assert_eq!(observation.timeframe(), OhlcvTimeframe::Minute5);
    assert_eq!(
        observation.provenance(),
        RuleProvenance::ValidationRequiredAssumption
    );
    let intervals = observation.intervals();
    assert_eq!(intervals.len(), 3);
    let actual: Vec<(i64, i64, u64)> = intervals
        .iter()
        .map(|interval| (
            interval.previous_source_timestamp_millis(),
            interval.next_source_timestamp_millis(),
            interval.elapsed_millis(),
        ))
        .collect();
    assert_eq!(
        actual,
        vec![(100, 200, 100), (200, 350, 150), (350, 900, 550)]
    );
    // No inferred missed-bar count, expected timeframe spacing, or trading veto.
}

#[test]
fn selected_timeframe_is_strictly_isolated_from_others() {
    let snap = snapshot("s88", "SPY");
    let data = series(
        &snap,
        &[
            (OhlcvTimeframe::Minute1, 120),
            (OhlcvTimeframe::Minute5, 300),
            (OhlcvTimeframe::Minute1, 225),
            (OhlcvTimeframe::Minute5, 700),
        ],
    );
    let minute1 =
        measure_ohlcv_source_spacing(&snap, &data, OhlcvTimeframe::Minute1).unwrap();
    let minute5 =
        measure_ohlcv_source_spacing(&snap, &data, OhlcvTimeframe::Minute5).unwrap();
    assert_eq!(minute1.intervals()[0].elapsed_millis(), 105);
    assert_eq!(minute5.intervals()[0].elapsed_millis(), 400);
    assert_eq!(minute1.intervals().len(), 1);
    assert_eq!(minute5.intervals().len(), 1);
}

#[test]
fn missing_or_single_observation_never_fabricates_timing() {
    let snap = snapshot("s88", "SPY");
    let one = series(&snap, &[(OhlcvTimeframe::Minute5, 100)]);
    for timeframe in [OhlcvTimeframe::Minute1, OhlcvTimeframe::Minute5] {
        assert_eq!(
            measure_ohlcv_source_spacing(&snap, &one, timeframe),
            Err(OhlcvSourceSpacingError::InsufficientCompletedHistory)
        );
    }
    let empty = series(&snap, &[]);
    assert_eq!(
        measure_ohlcv_source_spacing(&snap, &empty, OhlcvTimeframe::Minute5),
        Err(OhlcvSourceSpacingError::InsufficientCompletedHistory)
    );
}

#[test]
fn snapshot_and_symbol_mismatch_fail_closed() {
    let snap = snapshot("s88", "SPY");
    let data = series(
        &snap,
        &[
            (OhlcvTimeframe::Minute5, 100),
            (OhlcvTimeframe::Minute5, 200),
        ],
    );
    assert_eq!(
        measure_ohlcv_source_spacing(
            &snapshot("other", "SPY"),
            &data,
            OhlcvTimeframe::Minute5
        ),
        Err(OhlcvSourceSpacingError::SnapshotMismatch)
    );
    assert_eq!(
        measure_ohlcv_source_spacing(
            &snapshot("s88", "QQQ"),
            &data,
            OhlcvTimeframe::Minute5
        ),
        Err(OhlcvSourceSpacingError::SymbolMismatch)
    );
}
