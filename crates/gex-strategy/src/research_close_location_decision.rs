use crate::{
    MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvCoverage,
    ResearchCloseLocationAgeError, ResearchCloseLocationWithAge,
    calculate_research_close_location_with_age,
};

/// Separate, immutable research evidence tied to the exact decision clock
/// supplied by its caller. It is not real-time clock authentication, complete
/// DATA_VALID/DATA_FRESH, a hard-veto pass, or execution authorization.
///
/// ```compile_fail
/// use gex_strategy::ResearchCloseLocationAtDecision;
/// fn rewrite(mut evidence: ResearchCloseLocationAtDecision) {
///     evidence.decision_timestamp_millis = 999;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchCloseLocationAtDecision {
    checked_age: ResearchCloseLocationWithAge,
    decision_timestamp_millis: i64,
}

impl ResearchCloseLocationAtDecision {
    #[must_use]
    pub fn checked_age(&self) -> &ResearchCloseLocationWithAge {
        &self.checked_age
    }

    #[must_use]
    pub const fn decision_timestamp_millis(&self) -> i64 {
        self.decision_timestamp_millis
    }

    #[must_use]
    pub const fn measured_source_age_millis(&self) -> u64 {
        self.checked_age.measured_source_age_millis()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchCloseLocationDecisionError {
    InvalidDecisionTimeline,
    AssessmentClockMismatch,
    InvalidScopeAgeEvidence(ResearchCloseLocationAgeError),
}

/// Research decision-clock binding for AI-native §§6,12,33–34,37–42
/// within canonical data-safety §§11,16.
///
/// The supplied decision clock must be nonnegative, not earlier than snapshot
/// receipt, and EXACTLY equal to the clock recorded in the immutable coverage
/// assessment. An earlier within-age verdict cannot silently be reused at a
/// later decision instant, even one millisecond later. The existing scope-age
/// calculator remains the sole authority for exact source/quality/timeframe
/// and validated candle evidence; no age formula or default is duplicated.
pub fn calculate_research_close_location_at_decision(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    coverage: &RequiredOhlcvCoverage,
    timeframe: OhlcvTimeframe,
    decision_timestamp_millis: i64,
) -> Result<ResearchCloseLocationAtDecision, ResearchCloseLocationDecisionError> {
    if decision_timestamp_millis < 0
        || decision_timestamp_millis < snapshot.received_timestamp_millis()
    {
        return Err(ResearchCloseLocationDecisionError::InvalidDecisionTimeline);
    }

    if decision_timestamp_millis != coverage.decision_timestamp_millis() {
        return Err(ResearchCloseLocationDecisionError::AssessmentClockMismatch);
    }

    let checked_age =
        calculate_research_close_location_with_age(snapshot, series, coverage, timeframe)
            .map_err(ResearchCloseLocationDecisionError::InvalidScopeAgeEvidence)?;

    Ok(ResearchCloseLocationAtDecision {
        checked_age,
        decision_timestamp_millis,
    })
}
