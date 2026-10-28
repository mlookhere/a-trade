use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvError,
    OhlcvObservationInput, OhlcvObservationSeries, OhlcvTimeframe, Session,
};

fn snapshot() -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "s78".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_200,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn bar(timeframe: OhlcvTimeframe, timestamp: i64) -> OhlcvBarInput {
    OhlcvBarInput {
        timeframe,
        source_timestamp_millis: timestamp,
        received_timestamp_millis: timestamp + 1,
        open: 600.0,
        high: 603.0,
        low: 599.0,
        close: 602.0,
        volume: 500.0,
        completed: true,
    }
}

fn input(bars: Vec<OhlcvBarInput>) -> OhlcvObservationInput {
    OhlcvObservationInput {
        snapshot_id: "s78".to_owned(),
        symbol: "SPY".to_owned(),
        source_id: "provider-primary".to_owned(),
        quality: DataQuality::Structured,
        bars,
    }
}

#[test]
fn completed_bars_preserve_value_scope_and_provenance() {
    let snap = snapshot();
    let mut bars = Vec::new();
    for tf in [
        OhlcvTimeframe::Minute1,
        OhlcvTimeframe::Minute5,
        OhlcvTimeframe::Minute15,
        OhlcvTimeframe::Hour1,
        OhlcvTimeframe::Hour4,
        OhlcvTimeframe::Daily,
    ] {
        bars.push(bar(tf, 1_020));
    }
    bars.push(bar(OhlcvTimeframe::Minute1, 1_060));
    let data = OhlcvObservationSeries::build(&snap, input(bars)).unwrap();
    assert_eq!(data.snapshot_id(), "s78");
    assert_eq!(data.symbol(), "SPY");
    assert_eq!(data.source_id(), "provider-primary");
    assert_eq!(data.quality(), DataQuality::Structured);
    assert_eq!(data.completed_bars(OhlcvTimeframe::Minute1).len(), 2);
    assert_eq!(
        data.latest_completed(OhlcvTimeframe::Minute1)
            .unwrap()
            .source_timestamp_millis(),
        1_060
    );
    assert_eq!(
        data.latest_completed(OhlcvTimeframe::Minute15)
            .unwrap()
            .close(),
        602.0
    );
    assert_eq!(
        data.latest_completed(OhlcvTimeframe::Minute15)
            .unwrap()
            .volume(),
        500.0
    );
}

#[test]
fn missing_timeframe_remains_missing_and_is_not_zero_filled() {
    let snap = snapshot();
    let data = OhlcvObservationSeries::build(
        &snap,
        input(vec![bar(OhlcvTimeframe::Minute5, 1_020)]),
    )
    .unwrap();
    assert!(data.latest_completed(OhlcvTimeframe::Minute1).is_none());
    assert!(data.completed_bars(OhlcvTimeframe::Daily).is_empty());
    assert_eq!(data.completed_bars(OhlcvTimeframe::Minute5).len(), 1);
}

#[test]
fn unknown_source_or_snapshot_quality_is_not_trusted() {
    let snap = snapshot();
    let mut data = input(vec![bar(OhlcvTimeframe::Minute5, 1_020)]);
    data.quality = DataQuality::Unknown;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, data),
        Err(OhlcvError::UnknownDataQuality)
    );

    let other = MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "s78".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_200,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Unknown,
        },
        &[],
    )
    .unwrap();
    assert_eq!(
        OhlcvObservationSeries::build(
            &other,
            input(vec![bar(OhlcvTimeframe::Minute5, 1_020)])
        ),
        Err(OhlcvError::UnknownDataQuality)
    );
}

#[test]
fn provenance_identity_and_missing_fields_fail_closed() {
    let snap = snapshot();
    let mut missing_id = input(Vec::new());
    missing_id.snapshot_id.clear();
    assert_eq!(
        OhlcvObservationSeries::build(&snap, missing_id),
        Err(OhlcvError::MissingSnapshotId)
    );

    let mut wrong_id = input(Vec::new());
    wrong_id.snapshot_id = "another".to_owned();
    assert_eq!(
        OhlcvObservationSeries::build(&snap, wrong_id),
        Err(OhlcvError::SnapshotMismatch)
    );

    let mut wrong_symbol = input(Vec::new());
    wrong_symbol.symbol = "QQQ".to_owned();
    assert_eq!(
        OhlcvObservationSeries::build(&snap, wrong_symbol),
        Err(OhlcvError::SymbolMismatch)
    );

    let mut blank_symbol = input(Vec::new());
    blank_symbol.symbol.clear();
    assert_eq!(
        OhlcvObservationSeries::build(&snap, blank_symbol),
        Err(OhlcvError::MissingSymbol)
    );

    let mut blank_source = input(Vec::new());
    blank_source.source_id = " ".to_owned();
    assert_eq!(
        OhlcvObservationSeries::build(&snap, blank_source),
        Err(OhlcvError::MissingSourceId)
    );
}

#[test]
fn duplicate_and_out_of_order_bars_reject_without_repair() {
    let snap = snapshot();
    for times in [[1_020, 1_020], [1_050, 1_020]] {
        let candidate = input(times.into_iter().map(|t| {
            bar(OhlcvTimeframe::Minute5, t)
        }).collect());
        assert_eq!(
            OhlcvObservationSeries::build(&snap, candidate),
            Err(OhlcvError::DuplicateOrOutOfOrderBar)
        );
    }
    // Interleaved scopes are tracked independently, without timeframe mixing.
    let distinct = input(vec![
        bar(OhlcvTimeframe::Minute1, 1_050),
        bar(OhlcvTimeframe::Minute5, 1_020),
        bar(OhlcvTimeframe::Minute1, 1_060),
    ]);
    assert!(OhlcvObservationSeries::build(&snap, distinct).is_ok());
}

#[test]
fn incomplete_bars_reject_instead_of_becoming_completed_data() {
    let snap = snapshot();
    let mut incomplete = bar(OhlcvTimeframe::Minute1, 1_020);
    incomplete.completed = false;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, input(vec![incomplete])),
        Err(OhlcvError::IncompleteBar)
    );
}

#[test]
fn price_geometry_and_finite_values_are_structural_only() {
    let snap = snapshot();
    let mut invalid = bar(OhlcvTimeframe::Minute5, 1_020);
    invalid.high = 598.0;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, input(vec![invalid])),
        Err(OhlcvError::InvalidPriceGeometry)
    );

    let mut invalid = bar(OhlcvTimeframe::Minute5, 1_020);
    invalid.close = 603.001;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, input(vec![invalid])),
        Err(OhlcvError::InvalidPriceGeometry)
    );

    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut invalid = bar(OhlcvTimeframe::Minute5, 1_020);
        invalid.volume = value;
        assert_eq!(
            OhlcvObservationSeries::build(&snap, input(vec![invalid])),
            Err(OhlcvError::NonFinitePriceOrVolume)
        );
        let mut invalid = bar(OhlcvTimeframe::Minute5, 1_020);
        invalid.open = value;
        assert_eq!(
            OhlcvObservationSeries::build(&snap, input(vec![invalid])),
            Err(OhlcvError::NonFinitePriceOrVolume)
        );
    }

    let mut negative_volume = bar(OhlcvTimeframe::Minute5, 1_020);
    negative_volume.volume = -1.0;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, input(vec![negative_volume])),
        Err(OhlcvError::NegativeVolume)
    );

    let mut flat = bar(OhlcvTimeframe::Minute5, 1_020);
    flat.high = 600.0;
    flat.low = 600.0;
    flat.close = 600.0;
    flat.volume = 0.0;
    assert!(OhlcvObservationSeries::build(&snap, input(vec![flat])).is_ok());
}

#[test]
fn future_reversed_and_negative_timestamps_reject() {
    let snap = snapshot();
    let mut future = bar(OhlcvTimeframe::Minute5, 1_020);
    future.received_timestamp_millis = 1_201;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, input(vec![future])),
        Err(OhlcvError::InvalidTimestamp)
    );

    let mut reversed = bar(OhlcvTimeframe::Minute5, 1_020);
    reversed.received_timestamp_millis = 1_019;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, input(vec![reversed])),
        Err(OhlcvError::InvalidTimestamp)
    );

    let mut negative = bar(OhlcvTimeframe::Minute5, 1_020);
    negative.source_timestamp_millis = -1;
    assert_eq!(
        OhlcvObservationSeries::build(&snap, input(vec![negative])),
        Err(OhlcvError::InvalidTimestamp)
    );
}
