use gex_strategy::{
    DataQuality, MarketSnapshot, MarketSnapshotInput, OhlcvBarInput, OhlcvObservationInput,
    OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvAgeLimits, RequiredOhlcvCoverage,
    ResearchEmaConfig, ResearchEmaDecisionError, ResearchEmaError, ResearchEmaSeed, RuleProvenance,
    Session, assess_required_ohlcv_coverage, calculate_research_ema_at_decision,
};

fn snapshot(id: &str, symbol: &str, spot_price: f64) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: symbol.to_owned(),
            source_timestamp_millis: 950,
            received_timestamp_millis: 1_000,
            spot_price,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn series(
    snap: &MarketSnapshot,
    source_id: &str,
    offset: i64,
    timeframe: OhlcvTimeframe,
    quality: DataQuality,
) -> OhlcvObservationSeries {
    let bars = [(800, 100.0), (850, 110.0), (900, 120.0)]
        .into_iter()
        .map(|(stamp, close)| OhlcvBarInput {
            timeframe,
            source_timestamp_millis: stamp + offset,
            received_timestamp_millis: stamp + offset + 1,
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
            source_id: source_id.to_owned(),
            quality,
            bars,
        },
    )
    .unwrap()
}

fn age(
    snap: &MarketSnapshot,
    data: Option<&OhlcvObservationSeries>,
    clock: i64,
    limit: u64,
) -> RequiredOhlcvCoverage {
    let limits = RequiredOhlcvAgeLimits {
        minute1_ms: limit,
        minute5_ms: limit,
        minute15_ms: limit,
        hour1_ms: limit,
        hour4_ms: limit,
    };
    assess_required_ohlcv_coverage(snap, data, clock, &limits).unwrap()
}

fn cfg(timeframe: OhlcvTimeframe) -> ResearchEmaConfig {
    ResearchEmaConfig {
        timeframe,
        fast_window: 1,
        middle_window: 2,
        slow_window: 3,
        seed: ResearchEmaSeed::InitialSimpleAverage,
    }
}

#[test]
fn exact_age_boundary_preserves_source_history_and_research_provenance() {
    let snap = snapshot("ema114", "SPY", 600.0);
    let data = series(
        &snap,
        "provider-114",
        0,
        OhlcvTimeframe::Minute5,
        DataQuality::Structured,
    );
    let coverage = age(&snap, Some(&data), 1_010, 110);
    let result = calculate_research_ema_at_decision(
        &snap,
        &data,
        &coverage,
        &cfg(OhlcvTimeframe::Minute5),
        1_010,
    )
    .unwrap();
    let repeat = calculate_research_ema_at_decision(
        &snap,
        &data,
        &coverage,
        &cfg(OhlcvTimeframe::Minute5),
        1_010,
    )
    .unwrap();
    assert_eq!(result, repeat);
    assert_eq!(result.decision_timestamp_millis(), 1_010);
    assert_eq!(result.measured_source_age_millis(), 110);
    assert_eq!(result.source_series(), &data);
    assert_eq!(result.source_coverage(), &coverage);
    assert_eq!(result.source_series().source_snapshot(), &snap);
    assert_eq!(result.source_coverage().source_snapshot(), &snap);
    assert_eq!(result.observation().snapshot_id(), "ema114");
    assert_eq!(result.observation().symbol(), "SPY");
    assert_eq!(result.observation().source_id(), "provider-114");
    assert_eq!(result.observation().timeframe(), OhlcvTimeframe::Minute5);
    assert_eq!(result.observation().fast_window(), 1);
    assert_eq!(result.observation().middle_window(), 2);
    assert_eq!(result.observation().slow_window(), 3);
    assert_eq!(
        result.observation().seed(),
        ResearchEmaSeed::InitialSimpleAverage
    );
    assert_eq!(
        result.observation().provenance(),
        RuleProvenance::ValidationRequiredAssumption
    );
}

#[test]
fn stale_or_replayed_assessment_clock_never_promotes_ema_evidence() {
    let snap = snapshot("ema114", "SPY", 600.0);
    let data = series(
        &snap,
        "provider-114",
        0,
        OhlcvTimeframe::Minute5,
        DataQuality::Structured,
    );
    let coverage = age(&snap, Some(&data), 1_010, 110);
    for different in [1_009, 1_011] {
        assert_eq!(
            calculate_research_ema_at_decision(
                &snap,
                &data,
                &coverage,
                &cfg(OhlcvTimeframe::Minute5),
                different,
            ),
            Err(ResearchEmaDecisionError::AssessmentClockMismatch)
        );
    }
    for invalid in [-1, 999] {
        assert_eq!(
            calculate_research_ema_at_decision(
                &snap,
                &data,
                &coverage,
                &cfg(OhlcvTimeframe::Minute5),
                invalid,
            ),
            Err(ResearchEmaDecisionError::InvalidDecisionTimeline)
        );
    }
    let stale = age(&snap, Some(&data), 1_010, 109);
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &stale,
            &cfg(OhlcvTimeframe::Minute5),
            1_010,
        ),
        Err(ResearchEmaDecisionError::MissingOrStaleCoverage)
    );
    let absent = age(&snap, None, 1_010, 110);
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &absent,
            &cfg(OhlcvTimeframe::Minute5),
            1_010,
        ),
        Err(ResearchEmaDecisionError::SourceMismatch)
    );
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &coverage,
            &cfg(OhlcvTimeframe::Daily),
            1_010,
        ),
        Err(ResearchEmaDecisionError::UnsupportedTimeframe)
    );
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &coverage,
            &cfg(OhlcvTimeframe::Minute1),
            1_010,
        ),
        Err(ResearchEmaDecisionError::MissingOrStaleCoverage)
    );
}

#[test]
fn snapshot_id_symbol_contents_and_source_identity_must_match() {
    let snap = snapshot("ema114", "SPY", 600.0);
    let data = series(
        &snap,
        "provider-114",
        0,
        OhlcvTimeframe::Minute5,
        DataQuality::Structured,
    );
    let coverage = age(&snap, Some(&data), 1_010, 110);
    for (other, expected) in [
        (
            snapshot("foreign", "SPY", 600.0),
            ResearchEmaDecisionError::SnapshotMismatch,
        ),
        (
            snapshot("ema114", "QQQ", 600.0),
            ResearchEmaDecisionError::SymbolMismatch,
        ),
        (
            snapshot("ema114", "SPY", 601.0),
            ResearchEmaDecisionError::SnapshotContentsMismatch,
        ),
    ] {
        assert_eq!(
            calculate_research_ema_at_decision(
                &other,
                &data,
                &coverage,
                &cfg(OhlcvTimeframe::Minute5),
                1_010,
            ),
            Err(expected)
        );
    }

    let revised = snapshot("ema114", "SPY", 601.0);
    let revised_data = series(
        &revised,
        "provider-114",
        0,
        OhlcvTimeframe::Minute5,
        DataQuality::Structured,
    );
    let revised_coverage = age(&revised, Some(&revised_data), 1_010, 110);
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &revised_coverage,
            &cfg(OhlcvTimeframe::Minute5),
            1_010,
        ),
        Err(ResearchEmaDecisionError::SnapshotContentsMismatch)
    );

    let alternate = series(
        &snap,
        "other-provider",
        0,
        OhlcvTimeframe::Minute5,
        DataQuality::Structured,
    );
    let alternate_coverage = age(&snap, Some(&alternate), 1_010, 110);
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &alternate_coverage,
            &cfg(OhlcvTimeframe::Minute5),
            1_010,
        ),
        Err(ResearchEmaDecisionError::SourceMismatch)
    );
    let parsed = series(
        &snap,
        "provider-114",
        0,
        OhlcvTimeframe::Minute5,
        DataQuality::Parsed,
    );
    let parsed_coverage = age(&snap, Some(&parsed), 1_010, 110);
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &parsed_coverage,
            &cfg(OhlcvTimeframe::Minute5),
            1_010,
        ),
        Err(ResearchEmaDecisionError::SourceMismatch)
    );
}

#[test]
fn mismatched_latest_completed_source_bar_and_invalid_ema_configuration_reject() {
    let snap = snapshot("ema114", "SPY", 600.0);
    let data = series(
        &snap,
        "provider-114",
        0,
        OhlcvTimeframe::Minute5,
        DataQuality::Structured,
    );
    let shifted = series(
        &snap,
        "provider-114",
        1,
        OhlcvTimeframe::Minute5,
        DataQuality::Structured,
    );
    let shifted_age = age(&snap, Some(&shifted), 1_010, 110);
    assert_eq!(
        calculate_research_ema_at_decision(
            &snap,
            &data,
            &shifted_age,
            &cfg(OhlcvTimeframe::Minute5),
            1_010,
        ),
        Err(ResearchEmaDecisionError::CoverageBarMismatch)
    );
    let coverage = age(&snap, Some(&data), 1_010, 110);
    let mut invalid = cfg(OhlcvTimeframe::Minute5);
    invalid.fast_window = 0;
    assert_eq!(
        calculate_research_ema_at_decision(&snap, &data, &coverage, &invalid, 1_010),
        Err(ResearchEmaDecisionError::InvalidRawObservation(
            ResearchEmaError::InvalidWindows
        ))
    );
}
