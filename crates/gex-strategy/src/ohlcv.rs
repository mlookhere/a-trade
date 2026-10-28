use std::collections::BTreeMap;

use crate::{DataQuality, MarketSnapshot};

/// Observation scopes referenced by the AI-native GEX spec §§6,12 and
/// canonical §11. This type never implies that a provider supplies every scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OhlcvTimeframe {
    Minute1,
    Minute5,
    Minute15,
    Hour1,
    Hour4,
    Daily,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OhlcvBarInput {
    pub timeframe: OhlcvTimeframe,
    pub source_timestamp_millis: i64,
    pub received_timestamp_millis: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OhlcvObservationInput {
    pub snapshot_id: String,
    pub symbol: String,
    pub source_id: String,
    pub quality: DataQuality,
    pub bars: Vec<OhlcvBarInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OhlcvError {
    MissingSnapshotId,
    MissingSymbol,
    MissingSourceId,
    SnapshotMismatch,
    SymbolMismatch,
    UnknownDataQuality,
    NonFinitePriceOrVolume,
    InvalidPriceGeometry,
    NegativeVolume,
    InvalidTimestamp,
    IncompleteBar,
    DuplicateOrOutOfOrderBar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservedOhlcvBar {
    timeframe: OhlcvTimeframe,
    source_timestamp_millis: i64,
    received_timestamp_millis: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl ObservedOhlcvBar {
    #[must_use]
    pub const fn timeframe(&self) -> OhlcvTimeframe {
        self.timeframe
    }

    #[must_use]
    pub const fn source_timestamp_millis(&self) -> i64 {
        self.source_timestamp_millis
    }

    #[must_use]
    pub const fn received_timestamp_millis(&self) -> i64 {
        self.received_timestamp_millis
    }

    #[must_use]
    pub const fn open(&self) -> f64 {
        self.open
    }

    #[must_use]
    pub const fn high(&self) -> f64 {
        self.high
    }

    #[must_use]
    pub const fn low(&self) -> f64 {
        self.low
    }

    #[must_use]
    pub const fn close(&self) -> f64 {
        self.close
    }

    #[must_use]
    pub const fn volume(&self) -> f64 {
        self.volume
    }
}

/// Immutable, structurally checked source observations, not a DATA_VALID proof.
/// Every field remains source-supplied: no interpolation, candle resampling,
/// synthetic values, EMA, volatility score, trading signal or broker authority.
#[derive(Debug, Clone, PartialEq)]
pub struct OhlcvObservationSeries {
    source_snapshot: MarketSnapshot,
    snapshot_id: String,
    symbol: String,
    source_id: String,
    quality: DataQuality,
    bars_by_timeframe: BTreeMap<OhlcvTimeframe, Vec<ObservedOhlcvBar>>,
}

impl OhlcvObservationSeries {
    /// Immutable full source snapshot used to build this structurally checked series.
    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
    }

    pub fn build(
        snapshot: &MarketSnapshot,
        input: OhlcvObservationInput,
    ) -> Result<Self, OhlcvError> {
        if input.snapshot_id.trim().is_empty() {
            return Err(OhlcvError::MissingSnapshotId);
        }
        if input.symbol.trim().is_empty() {
            return Err(OhlcvError::MissingSymbol);
        }
        if input.source_id.trim().is_empty() {
            return Err(OhlcvError::MissingSourceId);
        }
        if input.snapshot_id != snapshot.snapshot_id() {
            return Err(OhlcvError::SnapshotMismatch);
        }
        if input.symbol != snapshot.symbol() {
            return Err(OhlcvError::SymbolMismatch);
        }
        if input.quality == DataQuality::Unknown || snapshot.data_quality() == DataQuality::Unknown
        {
            return Err(OhlcvError::UnknownDataQuality);
        }

        let mut bars_by_timeframe: BTreeMap<OhlcvTimeframe, Vec<ObservedOhlcvBar>> =
            BTreeMap::new();
        for bar in input.bars {
            if !bar.completed {
                return Err(OhlcvError::IncompleteBar);
            }
            if ![bar.open, bar.high, bar.low, bar.close, bar.volume]
                .into_iter()
                .all(f64::is_finite)
            {
                return Err(OhlcvError::NonFinitePriceOrVolume);
            }
            if bar.volume < 0.0 {
                return Err(OhlcvError::NegativeVolume);
            }
            if bar.high < bar.low
                || bar.open < bar.low
                || bar.open > bar.high
                || bar.close < bar.low
                || bar.close > bar.high
            {
                return Err(OhlcvError::InvalidPriceGeometry);
            }
            if bar.source_timestamp_millis < 0
                || bar.received_timestamp_millis < 0
                || bar.source_timestamp_millis > bar.received_timestamp_millis
                || bar.received_timestamp_millis > snapshot.received_timestamp_millis()
            {
                return Err(OhlcvError::InvalidTimestamp);
            }

            let entries = bars_by_timeframe.entry(bar.timeframe).or_default();
            if entries
                .last()
                .is_some_and(|prior| bar.source_timestamp_millis <= prior.source_timestamp_millis)
            {
                return Err(OhlcvError::DuplicateOrOutOfOrderBar);
            }
            entries.push(ObservedOhlcvBar {
                timeframe: bar.timeframe,
                source_timestamp_millis: bar.source_timestamp_millis,
                received_timestamp_millis: bar.received_timestamp_millis,
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
            });
        }

        Ok(Self {
            source_snapshot: snapshot.clone(),
            snapshot_id: input.snapshot_id,
            symbol: input.symbol,
            source_id: input.source_id,
            quality: input.quality,
            bars_by_timeframe,
        })
    }

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
    pub const fn quality(&self) -> DataQuality {
        self.quality
    }

    /// Absence is explicit None, never an invented zero or bar from another scope.
    #[must_use]
    pub fn latest_completed(&self, timeframe: OhlcvTimeframe) -> Option<&ObservedOhlcvBar> {
        self.bars_by_timeframe.get(&timeframe)?.last()
    }

    #[must_use]
    pub fn completed_bars(&self, timeframe: OhlcvTimeframe) -> &[ObservedOhlcvBar] {
        self.bars_by_timeframe
            .get(&timeframe)
            .map_or(&[], Vec::as_slice)
    }
}
