use crate::{
    AiNativeFeatureInput, AiNativeFeatureSet, CandleEventFeatures, FeatureSetError, MarketSnapshot,
    ResearchWickAnatomyAtDecision,
};

/// Research-only transport of calculator-owned signed wick evidence into the
/// optional AI-native wick_asymmetry feature. The generic feature set alone
/// does not establish provenance, freshness, full DATA_VALID or trade ALLOW.
///
/// ```compile_fail
/// use gex_strategy::ResearchWickAnatomyFeatureEvidence;
/// fn rewrite(mut evidence: ResearchWickAnatomyFeatureEvidence) {
///     evidence.decision_timestamp_millis = 100;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchWickAnatomyFeatureEvidence {
    features: AiNativeFeatureSet,
    source: ResearchWickAnatomyAtDecision,
    decision_timestamp_millis: i64,
}

impl ResearchWickAnatomyFeatureEvidence {
    #[must_use]
    pub fn features(&self) -> &AiNativeFeatureSet {
        &self.features
    }

    #[must_use]
    pub fn source(&self) -> &ResearchWickAnatomyAtDecision {
        &self.source
    }

    #[must_use]
    pub const fn decision_timestamp_millis(&self) -> i64 {
        self.decision_timestamp_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchWickAnatomyFeatureError {
    SnapshotMismatch,
    SymbolMismatch,
    SnapshotContentsMismatch,
    AssessmentClockMismatch,
    InvalidFeatureSet(FeatureSetError),
}

/// AI-native research §§6,12,33–34,37–42 and canonical data §§11,16.
///
/// Only candle_event.wick_asymmetry is populated; all other numeric fields
/// stay None/UNKNOWN. The original source observation retains its
/// ValidationRequiredAssumption provenance, source ID, quality, timeframe,
/// timestamp, measured age and decision clock. The signed value cannot be
/// treated as a direction, hard-veto clearance or production strategy rule.
pub fn bind_research_wick_anatomy_feature(
    snapshot: &MarketSnapshot,
    source: &ResearchWickAnatomyAtDecision,
    decision_timestamp_millis: i64,
) -> Result<ResearchWickAnatomyFeatureEvidence, ResearchWickAnatomyFeatureError> {
    let observation = source.observation();
    if observation.snapshot_id() != snapshot.snapshot_id() {
        return Err(ResearchWickAnatomyFeatureError::SnapshotMismatch);
    }
    if observation.symbol() != snapshot.symbol() {
        return Err(ResearchWickAnatomyFeatureError::SymbolMismatch);
    }
    if observation.source_snapshot() != snapshot {
        return Err(ResearchWickAnatomyFeatureError::SnapshotContentsMismatch);
    }
    if decision_timestamp_millis != source.decision_timestamp_millis() {
        return Err(ResearchWickAnatomyFeatureError::AssessmentClockMismatch);
    }

    let features = AiNativeFeatureSet::build(
        snapshot,
        AiNativeFeatureInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            candle_event: CandleEventFeatures {
                wick_asymmetry: Some(observation.wick_asymmetry_ratio()),
                ..CandleEventFeatures::default()
            },
            ..AiNativeFeatureInput::default()
        },
    )
    .map_err(ResearchWickAnatomyFeatureError::InvalidFeatureSet)?;

    Ok(ResearchWickAnatomyFeatureEvidence {
        features,
        source: source.clone(),
        decision_timestamp_millis,
    })
}
