use crate::{
    DataQuality, MarketSnapshot, ObservedOhlcvBar, OhlcvObservationSeries, OhlcvTimeframe,
    RuleProvenance,
};

/// AI-native research §12 lists close location as a feature but supplies no
/// required formula. The chosen high-low ratio is a validation-required
/// mathematical formalization, never an auction or AI-native direction gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchCloseLocationError {
    SnapshotMismatch,
    SymbolMismatch,
    SnapshotContentsMismatch,
    MissingCompletedBar,
    ZeroRangeBar,
    NonFiniteCalculation,
}

/// Calculator-owned immutable evidence of where one observed completed bar
/// closed within its *own* observed price range. No provider freshness,
/// continuity, feature normalization outside this ratio, or trading permission.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchCloseLocationObservation {
    source_snapshot: MarketSnapshot,
    snapshot_id: String,
    symbol: String,
    source_id: String,
    source_quality: DataQuality,
    timeframe: OhlcvTimeframe,
    source_timestamp_millis: i64,
    completed_bar: ObservedOhlcvBar,
    close_location_ratio: f64,
    provenance: RuleProvenance,
}

impl ResearchCloseLocationObservation {
    /// Immutable complete snapshot supplied to the validated source calculator.
    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
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

    /// Exact immutable completed OHLCV observation used for this calculation.
    /// Its equality is a research consistency invariant, not feed authenticity.
    #[must_use]
    pub fn completed_bar(&self) -> &ObservedOhlcvBar {
        &self.completed_bar
    }

    #[must_use]
    pub const fn close_location_ratio(&self) -> f64 {
        self.close_location_ratio
    }

    #[must_use]
    pub const fn provenance(&self) -> RuleProvenance {
        self.provenance
    }
}

/// Research formalization: (close - low) / (high - low).
///
/// A zero high-low range is undefined rather than assigned 0, 0.5 or 1.
/// Timestamp/source identity is preserved. The existing source series is
/// structurally checked; this calculation does NOT certify source freshness,
/// calendar/candle continuity, trade direction, a veto, or full DATA_VALID.
pub fn calculate_research_close_location(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    timeframe: OhlcvTimeframe,
) -> Result<ResearchCloseLocationObservation, ResearchCloseLocationError> {
    if snapshot.snapshot_id() != series.snapshot_id() {
        return Err(ResearchCloseLocationError::SnapshotMismatch);
    }
    if snapshot.symbol() != series.symbol() {
        return Err(ResearchCloseLocationError::SymbolMismatch);
    }
    if series.source_snapshot() != snapshot {
        return Err(ResearchCloseLocationError::SnapshotContentsMismatch);
    }
    let bar = series
        .latest_completed(timeframe)
        .ok_or(ResearchCloseLocationError::MissingCompletedBar)?;
    let range = bar.high() - bar.low();
    if range == 0.0 {
        return Err(ResearchCloseLocationError::ZeroRangeBar);
    }
    if !range.is_finite() || range < 0.0 {
        return Err(ResearchCloseLocationError::NonFiniteCalculation);
    }
    let distance_above_low = bar.close() - bar.low();
    let ratio = distance_above_low / range;
    if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
        return Err(ResearchCloseLocationError::NonFiniteCalculation);
    }
    Ok(ResearchCloseLocationObservation {
        source_snapshot: snapshot.clone(),
        snapshot_id: snapshot.snapshot_id().to_owned(),
        symbol: snapshot.symbol().to_owned(),
        source_id: series.source_id().to_owned(),
        source_quality: series.quality(),
        timeframe,
        source_timestamp_millis: bar.source_timestamp_millis(),
        completed_bar: bar.clone(),
        close_location_ratio: ratio,
        provenance: RuleProvenance::ValidationRequiredAssumption,
    })
}
