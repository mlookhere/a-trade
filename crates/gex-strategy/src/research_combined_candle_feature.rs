use crate::{
    AiNativeFeatureInput, AiNativeFeatureSet, CandleEventFeatures, FeatureSetError, MarketSnapshot,
    ResearchCloseLocationAtDecision, ResearchWickAnatomyAtDecision,
};

/// Research-only composition of two independently validated observations of
/// the SAME source-identified completed candle. This is NOT a proof of feed
/// authenticity, overall DATA_VALID/DATA_FRESH, a hard-veto PASS, or trade ALLOW.
///
/// ```compile_fail
/// use gex_strategy::ResearchCombinedCandleFeatureEvidence;
/// fn forge(mut evidence: ResearchCombinedCandleFeatureEvidence) {
///     evidence.decision_timestamp_millis = 999;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchCombinedCandleFeatureEvidence {
    features: AiNativeFeatureSet,
    close_location: ResearchCloseLocationAtDecision,
    wick_anatomy: ResearchWickAnatomyAtDecision,
    decision_timestamp_millis: i64,
}

impl ResearchCombinedCandleFeatureEvidence {
    #[must_use]
    pub fn features(&self) -> &AiNativeFeatureSet {
        &self.features
    }

    #[must_use]
    pub fn close_location(&self) -> &ResearchCloseLocationAtDecision {
        &self.close_location
    }

    #[must_use]
    pub fn wick_anatomy(&self) -> &ResearchWickAnatomyAtDecision {
        &self.wick_anatomy
    }

    #[must_use]
    pub const fn decision_timestamp_millis(&self) -> i64 {
        self.decision_timestamp_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchCombinedCandleFeatureError {
    SnapshotMismatch,
    SymbolMismatch,
    SourceMismatch,
    SourceQualityMismatch,
    TimeframeMismatch,
    BarTimestampMismatch,
    BarContentMismatch,
    SourceAgeMismatch,
    AssessmentClockMismatch,
    InvalidFeatureSet(FeatureSetError),
}

/// Consistency-only composition for canonical data §§11,16 and agent
/// boundaries §§5–6; AI-native research §§6,12,33–34,37–42.
///
/// Require identical source identity, selected timeframe/bar timestamp,
/// measured age and exact caller decision clock. No synthetic source merge,
/// inferred time, default age limit, direction or threshold is introduced.
/// Both input formulas remain ValidationRequiredAssumption.
///
/// The retained raw completed-bar observations must also match exactly,
/// including OHLC, volume, received time, source time and timeframe. This
/// only checks consistency between provided observations, not provider truth.
pub fn bind_research_combined_candle_feature(
    snapshot: &MarketSnapshot,
    close_location: &ResearchCloseLocationAtDecision,
    wick_anatomy: &ResearchWickAnatomyAtDecision,
    decision_timestamp_millis: i64,
) -> Result<ResearchCombinedCandleFeatureEvidence, ResearchCombinedCandleFeatureError> {
    let close = close_location.checked_age().observation();
    let wick = wick_anatomy.observation();

    if close.snapshot_id() != snapshot.snapshot_id() || wick.snapshot_id() != snapshot.snapshot_id()
    {
        return Err(ResearchCombinedCandleFeatureError::SnapshotMismatch);
    }
    if close.symbol() != snapshot.symbol() || wick.symbol() != snapshot.symbol() {
        return Err(ResearchCombinedCandleFeatureError::SymbolMismatch);
    }
    if close.source_id() != wick.source_id() {
        return Err(ResearchCombinedCandleFeatureError::SourceMismatch);
    }
    if close.source_quality() != wick.source_quality() {
        return Err(ResearchCombinedCandleFeatureError::SourceQualityMismatch);
    }
    if close.timeframe() != wick.timeframe() {
        return Err(ResearchCombinedCandleFeatureError::TimeframeMismatch);
    }
    if close.source_timestamp_millis() != wick.source_timestamp_millis() {
        return Err(ResearchCombinedCandleFeatureError::BarTimestampMismatch);
    }
    if close_location.measured_source_age_millis() != wick_anatomy.measured_source_age_millis() {
        return Err(ResearchCombinedCandleFeatureError::SourceAgeMismatch);
    }
    if close.completed_bar() != wick.completed_bar() {
        return Err(ResearchCombinedCandleFeatureError::BarContentMismatch);
    }
    if decision_timestamp_millis != close_location.decision_timestamp_millis()
        || decision_timestamp_millis != wick_anatomy.decision_timestamp_millis()
    {
        return Err(ResearchCombinedCandleFeatureError::AssessmentClockMismatch);
    }

    let features = AiNativeFeatureSet::build(
        snapshot,
        AiNativeFeatureInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            candle_event: CandleEventFeatures {
                close_location_value: Some(close.close_location_ratio()),
                wick_asymmetry: Some(wick.wick_asymmetry_ratio()),
                ..CandleEventFeatures::default()
            },
            ..AiNativeFeatureInput::default()
        },
    )
    .map_err(ResearchCombinedCandleFeatureError::InvalidFeatureSet)?;

    Ok(ResearchCombinedCandleFeatureEvidence {
        features,
        close_location: close_location.clone(),
        wick_anatomy: wick_anatomy.clone(),
        decision_timestamp_millis,
    })
}
