use crate::{MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, RuleProvenance};

/// Two timestamps actually supplied by one feed for adjacent completed
/// observations of the same timeframe. The difference is not a candle duration,
/// exchange-session boundary, or verified missing-bar count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedSourceInterval {
    previous_source_timestamp_millis: i64,
    next_source_timestamp_millis: i64,
    elapsed_millis: u64,
}

impl ObservedSourceInterval {
    #[must_use]
    pub const fn previous_source_timestamp_millis(&self) -> i64 {
        self.previous_source_timestamp_millis
    }

    #[must_use]
    pub const fn next_source_timestamp_millis(&self) -> i64 {
        self.next_source_timestamp_millis
    }

    #[must_use]
    pub const fn elapsed_millis(&self) -> u64 {
        self.elapsed_millis
    }
}

/// Research-only, read-only source timing evidence, not DATA_VALID, an
/// inter-bar-continuity certificate, or hard-veto/trade authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OhlcvSourceSpacingObservation {
    snapshot_id: String,
    symbol: String,
    source_id: String,
    timeframe: OhlcvTimeframe,
    intervals: Vec<ObservedSourceInterval>,
    provenance: RuleProvenance,
}

impl OhlcvSourceSpacingObservation {
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    #[must_use]
    pub const fn timeframe(&self) -> OhlcvTimeframe {
        self.timeframe
    }

    #[must_use]
    pub fn intervals(&self) -> &[ObservedSourceInterval] {
        &self.intervals
    }

    #[must_use]
    pub const fn provenance(&self) -> RuleProvenance {
        self.provenance
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OhlcvSourceSpacingError {
    SnapshotMismatch,
    SymbolMismatch,
    SnapshotContentsMismatch,
    InsufficientCompletedHistory,
    InvalidSourceTimestampSequence,
}

/// AI-native research §42, canonical §§11,16: only measure observed
/// source-timestamp differences. Actual candle construction, missing bars,
/// provider timestamp meanings and exchange sessions remain UNKNOWN.
/// No inferred 1m/5m/etc. interval, gap threshold, score or veto is supplied.
pub fn measure_ohlcv_source_spacing(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    timeframe: OhlcvTimeframe,
) -> Result<OhlcvSourceSpacingObservation, OhlcvSourceSpacingError> {
    if series.snapshot_id() != snapshot.snapshot_id() {
        return Err(OhlcvSourceSpacingError::SnapshotMismatch);
    }
    if series.symbol() != snapshot.symbol() {
        return Err(OhlcvSourceSpacingError::SymbolMismatch);
    }

    if series.source_snapshot() != snapshot {
        return Err(OhlcvSourceSpacingError::SnapshotContentsMismatch);
    }
    let bars = series.completed_bars(timeframe);
    if bars.len() < 2 {
        return Err(OhlcvSourceSpacingError::InsufficientCompletedHistory);
    }

    let mut intervals = Vec::with_capacity(bars.len() - 1);
    for pair in bars.windows(2) {
        let previous = pair[0].source_timestamp_millis();
        let next = pair[1].source_timestamp_millis();
        let elapsed_millis = next
            .checked_sub(previous)
            .and_then(|difference| u64::try_from(difference).ok())
            .filter(|difference| *difference > 0)
            .ok_or(OhlcvSourceSpacingError::InvalidSourceTimestampSequence)?;
        intervals.push(ObservedSourceInterval {
            previous_source_timestamp_millis: previous,
            next_source_timestamp_millis: next,
            elapsed_millis,
        });
    }

    Ok(OhlcvSourceSpacingObservation {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        symbol: snapshot.symbol().to_owned(),
        source_id: series.source_id().to_owned(),
        timeframe,
        intervals,
        // Not a source-prescribed missing-bar algorithm; observational
        // timestamp subtraction is isolated from all production authority.
        provenance: RuleProvenance::ValidationRequiredAssumption,
    })
}
