use crate::{
    MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvCoverage,
    ResearchCloseLocationError, ResearchCloseLocationObservation, TemporalStatus,
    calculate_research_close_location,
};

/// Configured-age observation for ONE OHLCV source/timeframe only. Neither
/// provider authenticity, contiguous candles, full DATA_FRESH nor trade
/// authorization is implied. This cannot be constructed by external callers.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchCloseLocationWithAge {
    observation: ResearchCloseLocationObservation,
    measured_source_age_millis: u64,
}

impl ResearchCloseLocationWithAge {
    #[must_use]
    pub fn observation(&self) -> &ResearchCloseLocationObservation {
        &self.observation
    }

    #[must_use]
    pub const fn measured_source_age_millis(&self) -> u64 {
        self.measured_source_age_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchCloseLocationAgeError {
    SnapshotMismatch,
    SymbolMismatch,
    SourceMismatch,
    UnsupportedTimeframe,
    MissingOrInvalidScopeAge,
    SourceTimestampMismatch,
    InvalidRawObservation(ResearchCloseLocationError),
}

/// AI-native research §§12,33–34,42 and canonical data §§11,16.
/// Check the exact source, timeframe and completed-bar timestamp against the
/// existing caller-configured RequiredOhlcvCoverage report. No fixed age
/// limits, cross-timeframe substitution, confidence or veto are introduced.
pub fn calculate_research_close_location_with_age(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    coverage: &RequiredOhlcvCoverage,
    timeframe: OhlcvTimeframe,
) -> Result<ResearchCloseLocationWithAge, ResearchCloseLocationAgeError> {
    if snapshot.snapshot_id() != series.snapshot_id()
        || snapshot.snapshot_id() != coverage.snapshot_id()
    {
        return Err(ResearchCloseLocationAgeError::SnapshotMismatch);
    }
    if snapshot.symbol() != series.symbol() || snapshot.symbol() != coverage.symbol() {
        return Err(ResearchCloseLocationAgeError::SymbolMismatch);
    }
    if matches!(timeframe, OhlcvTimeframe::Daily) {
        // RequiredOhlcvCoverage has no configured Daily source-age field.
        return Err(ResearchCloseLocationAgeError::UnsupportedTimeframe);
    }
    if coverage.source_id() != Some(series.source_id())
        || coverage.source_quality() != Some(series.quality())
    {
        return Err(ResearchCloseLocationAgeError::SourceMismatch);
    }

    let scope = coverage
        .observations()
        .iter()
        .find(|scope| scope.timeframe == timeframe)
        .ok_or(ResearchCloseLocationAgeError::MissingOrInvalidScopeAge)?;
    if scope.status != TemporalStatus::WithinConfiguredAge {
        return Err(ResearchCloseLocationAgeError::MissingOrInvalidScopeAge);
    }
    let measured_age = scope
        .measured_age_millis
        .ok_or(ResearchCloseLocationAgeError::MissingOrInvalidScopeAge)?;

    let observation = calculate_research_close_location(snapshot, series, timeframe)
        .map_err(ResearchCloseLocationAgeError::InvalidRawObservation)?;
    if scope.source_timestamp_millis != Some(observation.source_timestamp_millis()) {
        return Err(ResearchCloseLocationAgeError::SourceTimestampMismatch);
    }
    Ok(ResearchCloseLocationWithAge {
        observation,
        measured_source_age_millis: measured_age,
    })
}
