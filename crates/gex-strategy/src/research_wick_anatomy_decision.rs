use crate::{
    MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvCoverage,
    ResearchWickAnatomyError, ResearchWickAnatomyObservation, TemporalStatus,
    calculate_research_wick_anatomy,
};

/// Research-only source-age and decision-time binding of ONE selected completed
/// candle's wick anatomy. These fields cannot be fabricated by external callers.
///
/// Does NOT certify overall DATA_VALID, DATA_FRESH, provider authenticity,
/// market-session continuity, a hard-veto PASS, or trade authorization.
///
/// ```compile_fail
/// use gex_strategy::ResearchWickAnatomyAtDecision;
/// fn forge(mut evidence: ResearchWickAnatomyAtDecision) {
///     evidence.measured_source_age_millis = 0;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchWickAnatomyAtDecision {
    observation: ResearchWickAnatomyObservation,
    measured_source_age_millis: u64,
    decision_timestamp_millis: i64,
}

impl ResearchWickAnatomyAtDecision {
    #[must_use]
    pub fn observation(&self) -> &ResearchWickAnatomyObservation {
        &self.observation
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
pub enum ResearchWickAnatomyDecisionError {
    InvalidDecisionTimeline,
    AssessmentClockMismatch,
    SnapshotMismatch,
    SymbolMismatch,
    UnsupportedTimeframe,
    SourceMismatch,
    MissingOrInvalidScopeAge,
    SourceTimestampMismatch,
    InvalidRawObservation(ResearchWickAnatomyError),
}

/// Research-only safety contract for canonical market data §§11,16,
/// data-supervisor boundaries §§5–6, and AI-native research §§12,33–34,42.
///
/// Coverage must have been assessed at precisely the caller's decision clock,
/// must refer to the exact series source, and must report this selected scope
/// WithinConfiguredAge. No default age limit, substitute timeframe, inferred
/// clock, provider trust, directional threshold or execution state is created.
pub fn calculate_research_wick_anatomy_at_decision(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    coverage: &RequiredOhlcvCoverage,
    timeframe: OhlcvTimeframe,
    decision_timestamp_millis: i64,
) -> Result<ResearchWickAnatomyAtDecision, ResearchWickAnatomyDecisionError> {
    if decision_timestamp_millis < 0
        || decision_timestamp_millis < snapshot.received_timestamp_millis()
    {
        return Err(ResearchWickAnatomyDecisionError::InvalidDecisionTimeline);
    }
    if decision_timestamp_millis != coverage.decision_timestamp_millis() {
        return Err(ResearchWickAnatomyDecisionError::AssessmentClockMismatch);
    }
    if snapshot.snapshot_id() != series.snapshot_id()
        || snapshot.snapshot_id() != coverage.snapshot_id()
    {
        return Err(ResearchWickAnatomyDecisionError::SnapshotMismatch);
    }
    if snapshot.symbol() != series.symbol() || snapshot.symbol() != coverage.symbol() {
        return Err(ResearchWickAnatomyDecisionError::SymbolMismatch);
    }
    if matches!(timeframe, OhlcvTimeframe::Daily) {
        // RequiredOhlcvCoverage has no configured daily source-age scope.
        return Err(ResearchWickAnatomyDecisionError::UnsupportedTimeframe);
    }
    if coverage.source_id() != Some(series.source_id())
        || coverage.source_quality() != Some(series.quality())
    {
        return Err(ResearchWickAnatomyDecisionError::SourceMismatch);
    }

    let scope = coverage
        .observations()
        .iter()
        .find(|scope| scope.timeframe == timeframe)
        .ok_or(ResearchWickAnatomyDecisionError::MissingOrInvalidScopeAge)?;
    if scope.status != TemporalStatus::WithinConfiguredAge {
        return Err(ResearchWickAnatomyDecisionError::MissingOrInvalidScopeAge);
    }
    let age = scope
        .measured_age_millis
        .ok_or(ResearchWickAnatomyDecisionError::MissingOrInvalidScopeAge)?;

    let observation = calculate_research_wick_anatomy(snapshot, series, timeframe)
        .map_err(ResearchWickAnatomyDecisionError::InvalidRawObservation)?;
    if scope.source_timestamp_millis != Some(observation.source_timestamp_millis()) {
        return Err(ResearchWickAnatomyDecisionError::SourceTimestampMismatch);
    }

    Ok(ResearchWickAnatomyAtDecision {
        observation,
        measured_source_age_millis: age,
        decision_timestamp_millis,
    })
}
