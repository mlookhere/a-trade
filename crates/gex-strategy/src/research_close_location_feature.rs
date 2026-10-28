use crate::{
    AiNativeFeatureInput, AiNativeFeatureSet, CandleEventFeatures, FeatureSetError, MarketSnapshot,
    ResearchCloseLocationAtDecision,
};

/// Research-only bridge from calculator-owned OHLCV evidence to one optional
/// AI-native feature value. The generic feature set alone does not certify
/// source integrity; preserve the typed source evidence alongside it.
///
/// Neither this wrapper nor a populated close-location field constitutes
/// global DATA_VALID/DATA_FRESH, a hard-veto PASS, or trade authorization.
///
/// ```compile_fail
/// use gex_strategy::ResearchCloseLocationFeatureEvidence;
/// fn forge(mut evidence: ResearchCloseLocationFeatureEvidence) {
///     evidence.decision_timestamp_millis = 1;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchCloseLocationFeatureEvidence {
    features: AiNativeFeatureSet,
    source: ResearchCloseLocationAtDecision,
    decision_timestamp_millis: i64,
}

impl ResearchCloseLocationFeatureEvidence {
    #[must_use]
    pub fn features(&self) -> &AiNativeFeatureSet {
        &self.features
    }

    #[must_use]
    pub fn source(&self) -> &ResearchCloseLocationAtDecision {
        &self.source
    }

    #[must_use]
    pub const fn decision_timestamp_millis(&self) -> i64 {
        self.decision_timestamp_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchCloseLocationFeatureError {
    SnapshotMismatch,
    SymbolMismatch,
    SnapshotContentsMismatch,
    AssessmentClockMismatch,
    InvalidFeatureSet(FeatureSetError),
}

/// AI-native research §§6,12,33–34,37–42; canonical data §§11,16.
///
/// The only non-UNKNOWN feature is close_location_value, copied from
/// an immutable source-bound calculation with the same snapshot, symbol and
/// decision time. All other fields remain None. The source's
/// ValidationRequiredAssumption provenance is retained, not promoted.
pub fn bind_research_close_location_feature(
    snapshot: &MarketSnapshot,
    source: &ResearchCloseLocationAtDecision,
    decision_timestamp_millis: i64,
) -> Result<ResearchCloseLocationFeatureEvidence, ResearchCloseLocationFeatureError> {
    let observation = source.checked_age().observation();

    if observation.snapshot_id() != snapshot.snapshot_id() {
        return Err(ResearchCloseLocationFeatureError::SnapshotMismatch);
    }
    if observation.symbol() != snapshot.symbol() {
        return Err(ResearchCloseLocationFeatureError::SymbolMismatch);
    }
    if observation.source_snapshot() != snapshot {
        return Err(ResearchCloseLocationFeatureError::SnapshotContentsMismatch);
    }
    if decision_timestamp_millis != source.decision_timestamp_millis() {
        return Err(ResearchCloseLocationFeatureError::AssessmentClockMismatch);
    }

    let features = AiNativeFeatureSet::build(
        snapshot,
        AiNativeFeatureInput {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            candle_event: CandleEventFeatures {
                close_location_value: Some(observation.close_location_ratio()),
                ..CandleEventFeatures::default()
            },
            ..AiNativeFeatureInput::default()
        },
    )
    .map_err(ResearchCloseLocationFeatureError::InvalidFeatureSet)?;

    Ok(ResearchCloseLocationFeatureEvidence {
        features,
        source: source.clone(),
        decision_timestamp_millis,
    })
}
