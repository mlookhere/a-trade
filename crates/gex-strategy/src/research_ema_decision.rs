use crate::{
    MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvCoverage,
    ResearchEmaConfig, ResearchEmaError, ResearchEmaObservation, TemporalStatus,
    calculate_research_ema_stack,
};

/// Immutable research-only EMA evidence bound to a caller decision timestamp,
/// exact source snapshot, full OHLCV history and evaluated coverage report.
/// This cannot certify feed authenticity, full DATA_VALID/DATA_FRESH, EMA
/// direction, a hard-veto PASS, or trade authorization.
///
/// ```compile_fail
/// use gex_strategy::ResearchEmaAtDecision;
/// fn forge(mut evidence: ResearchEmaAtDecision) {
///     evidence.decision_timestamp_millis = 0;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchEmaAtDecision {
    observation: ResearchEmaObservation,
    source_series: OhlcvObservationSeries,
    source_coverage: RequiredOhlcvCoverage,
    measured_source_age_millis: u64,
    decision_timestamp_millis: i64,
}

impl ResearchEmaAtDecision {
    #[must_use]
    pub fn observation(&self) -> &ResearchEmaObservation {
        &self.observation
    }

    #[must_use]
    pub fn source_series(&self) -> &OhlcvObservationSeries {
        &self.source_series
    }

    #[must_use]
    pub fn source_coverage(&self) -> &RequiredOhlcvCoverage {
        &self.source_coverage
    }

    #[must_use]
    pub const fn measured_source_age_millis(&self) -> u64 {
        self.measured_source_age_millis
    }

    #[must_use]
    pub const fn decision_timestamp_millis(&self) -> i64 {
        self.decision_timestamp_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchEmaDecisionError {
    InvalidDecisionTimeline,
    AssessmentClockMismatch,
    SnapshotMismatch,
    SymbolMismatch,
    SnapshotContentsMismatch,
    UnsupportedTimeframe,
    SourceMismatch,
    MissingOrStaleCoverage,
    CoverageBarMismatch,
    InvalidRawObservation(ResearchEmaError),
}

/// Exact caller-clock binding for the research EMA implementation (AI-native
/// §34) within canonical §§5-6,11,16. Coverage and OHLCV series must each have
/// been constructed for precisely this immutable snapshot. The selected scope
/// must have measured source age at the exact caller clock, with the same
/// latest completed bar timestamp and source quality. No timeout, EMA window,
/// seed choice, session semantics, or current time is inferred.
///
/// Older within-age coverage is NOT evidence of freshness at a later clock.
pub fn calculate_research_ema_at_decision(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    coverage: &RequiredOhlcvCoverage,
    config: &ResearchEmaConfig,
    decision_timestamp_millis: i64,
) -> Result<ResearchEmaAtDecision, ResearchEmaDecisionError> {
    if decision_timestamp_millis < 0
        || decision_timestamp_millis < snapshot.received_timestamp_millis()
    {
        return Err(ResearchEmaDecisionError::InvalidDecisionTimeline);
    }
    if coverage.decision_timestamp_millis() != decision_timestamp_millis {
        return Err(ResearchEmaDecisionError::AssessmentClockMismatch);
    }
    if series.snapshot_id() != snapshot.snapshot_id()
        || coverage.snapshot_id() != snapshot.snapshot_id()
    {
        return Err(ResearchEmaDecisionError::SnapshotMismatch);
    }
    if series.symbol() != snapshot.symbol() || coverage.symbol() != snapshot.symbol() {
        return Err(ResearchEmaDecisionError::SymbolMismatch);
    }
    if series.source_snapshot() != snapshot || coverage.source_snapshot() != snapshot {
        return Err(ResearchEmaDecisionError::SnapshotContentsMismatch);
    }
    if matches!(config.timeframe, OhlcvTimeframe::Daily) {
        return Err(ResearchEmaDecisionError::UnsupportedTimeframe);
    }
    if coverage.source_id() != Some(series.source_id())
        || coverage.source_quality() != Some(series.quality())
    {
        return Err(ResearchEmaDecisionError::SourceMismatch);
    }

    let scope = coverage
        .observations()
        .iter()
        .find(|scope| scope.timeframe == config.timeframe)
        .ok_or(ResearchEmaDecisionError::MissingOrStaleCoverage)?;
    if scope.status != TemporalStatus::WithinConfiguredAge {
        return Err(ResearchEmaDecisionError::MissingOrStaleCoverage);
    }
    let age = scope
        .measured_age_millis
        .ok_or(ResearchEmaDecisionError::MissingOrStaleCoverage)?;
    let latest = series
        .latest_completed(config.timeframe)
        .ok_or(ResearchEmaDecisionError::CoverageBarMismatch)?;
    if scope.source_timestamp_millis != Some(latest.source_timestamp_millis()) {
        return Err(ResearchEmaDecisionError::CoverageBarMismatch);
    }

    let observation = calculate_research_ema_stack(snapshot, series, coverage, config)
        .map_err(ResearchEmaDecisionError::InvalidRawObservation)?;

    Ok(ResearchEmaAtDecision {
        observation,
        source_series: series.clone(),
        source_coverage: coverage.clone(),
        measured_source_age_millis: age,
        decision_timestamp_millis,
    })
}
