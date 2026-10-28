use crate::{DataQuality, MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, RuleProvenance};

/// AI-native research §12 names wick asymmetry as a feature but does not
/// prescribe a formula. This signed wick-difference ratio is a
/// ValidationRequiredAssumption, not a directional trading rule or threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchWickAnatomyError {
    SnapshotMismatch,
    SymbolMismatch,
    MissingCompletedBar,
    ZeroRangeBar,
    NonFiniteCalculation,
}

/// Immutable source-bound arithmetic observation, NOT freshness, DATA_VALID,
/// a hard-veto clearance, or any trading permission. The source timestamp is
/// feed-supplied and is not an inferred candle-close timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchWickAnatomyObservation {
    snapshot_id: String,
    symbol: String,
    source_id: String,
    source_quality: DataQuality,
    timeframe: OhlcvTimeframe,
    source_timestamp_millis: i64,
    wick_asymmetry_ratio: f64,
    provenance: RuleProvenance,
}

impl ResearchWickAnatomyObservation {
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
    pub const fn source_quality(&self) -> DataQuality {
        self.source_quality
    }

    #[must_use]
    pub const fn timeframe(&self) -> OhlcvTimeframe {
        self.timeframe
    }

    #[must_use]
    pub const fn source_timestamp_millis(&self) -> i64 {
        self.source_timestamp_millis
    }

    #[must_use]
    pub const fn wick_asymmetry_ratio(&self) -> f64 {
        self.wick_asymmetry_ratio
    }

    #[must_use]
    pub const fn provenance(&self) -> RuleProvenance {
        self.provenance
    }
}

/// Research formalization: upper = high - max(open, close), lower =
/// min(open, close) - low, ratio = (lower - upper) / (high - low).
///
/// Range zero is undefined, not an inferred zero. The signed result is a
/// description of the observed completed candle, not a direction signal.
/// No time freshness, candle continuity or provider trust is certified.
pub fn calculate_research_wick_anatomy(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    timeframe: OhlcvTimeframe,
) -> Result<ResearchWickAnatomyObservation, ResearchWickAnatomyError> {
    if snapshot.snapshot_id() != series.snapshot_id() {
        return Err(ResearchWickAnatomyError::SnapshotMismatch);
    }
    if snapshot.symbol() != series.symbol() {
        return Err(ResearchWickAnatomyError::SymbolMismatch);
    }
    let bar = series
        .latest_completed(timeframe)
        .ok_or(ResearchWickAnatomyError::MissingCompletedBar)?;

    let range = bar.high() - bar.low();
    if range == 0.0 {
        return Err(ResearchWickAnatomyError::ZeroRangeBar);
    }
    if !range.is_finite() || range < 0.0 {
        return Err(ResearchWickAnatomyError::NonFiniteCalculation);
    }

    let upper_wick = bar.high() - bar.open().max(bar.close());
    let lower_wick = bar.open().min(bar.close()) - bar.low();
    let signed_difference = lower_wick - upper_wick;
    let ratio = signed_difference / range;
    if ![upper_wick, lower_wick, signed_difference, ratio]
        .into_iter()
        .all(f64::is_finite)
        || !(0.0..=range).contains(&upper_wick)
        || !(0.0..=range).contains(&lower_wick)
        || !(-1.0..=1.0).contains(&ratio)
    {
        return Err(ResearchWickAnatomyError::NonFiniteCalculation);
    }

    Ok(ResearchWickAnatomyObservation {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        symbol: snapshot.symbol().to_owned(),
        source_id: series.source_id().to_owned(),
        source_quality: series.quality(),
        timeframe,
        source_timestamp_millis: bar.source_timestamp_millis(),
        wick_asymmetry_ratio: ratio,
        provenance: RuleProvenance::ValidationRequiredAssumption,
    })
}
