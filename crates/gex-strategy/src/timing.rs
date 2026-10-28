use crate::{DataQuality, MarketSnapshot};

/// AI-native GEX spec §§6,16,33–34,39,42. All three age limits MUST be
/// supplied by the caller's reviewed versioned configuration. There is no
/// strategy or implementation default. Equality with a limit is within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitAgeLimits {
    pub market_data_stale_timeout_ms: u64,
    pub gex_stale_timeout_ms: u64,
    pub max_decision_age_ms: u64,
}

/// A separate provider-sourced GEX timestamp pair. The market snapshot's
/// timestamp MUST NOT be substituted for independently sampled GEX metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GexTimeObservation {
    /// Must identify the exact decision snapshot and underlying; matching
    /// timestamps from another instrument cannot serve as freshness proof.
    pub snapshot_id: String,
    pub symbol: String,
    pub source_timestamp_millis: Option<i64>,
    pub received_timestamp_millis: Option<i64>,
    pub data_quality: DataQuality,
}

/// An observed timestamp assessment, NOT DATA_VALID, complete DATA_FRESH,
/// a claim about provider authenticity, or a strategy authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalStatus {
    WithinConfiguredAge,
    Stale,
    Missing,
    InvalidTimeline,
    UnknownQuality,
    IdentityMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotTimingProvenanceError {
    SnapshotMismatch,
    SnapshotContentsMismatch,
    GexObservationMismatch,
    DecisionClockMismatch,
    AgeLimitsMismatch,
}

/// Source-bound informational timing evidence. Its verdict is never a
/// globally valid DATA_FRESH or trade-authorization certificate.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotTimingAssessment {
    snapshot_id: String,
    market_data: TemporalStatus,
    gex_data: TemporalStatus,
    decision_age: TemporalStatus,
    source_snapshot: MarketSnapshot,
    source_gex: Option<GexTimeObservation>,
    decision_timestamp_millis: i64,
    age_limits: ExplicitAgeLimits,
}

impl SnapshotTimingAssessment {
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    #[must_use]
    pub const fn market_data(&self) -> TemporalStatus {
        self.market_data
    }

    #[must_use]
    pub const fn gex_data(&self) -> TemporalStatus {
        self.gex_data
    }

    #[must_use]
    pub const fn decision_age(&self) -> TemporalStatus {
        self.decision_age
    }

    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
    }

    #[must_use]
    pub fn source_gex(&self) -> Option<&GexTimeObservation> {
        self.source_gex.as_ref()
    }

    #[must_use]
    pub const fn decision_timestamp_millis(&self) -> i64 {
        self.decision_timestamp_millis
    }

    #[must_use]
    pub const fn age_limits(&self) -> ExplicitAgeLimits {
        self.age_limits
    }

    /// Revalidate original evidence identity, not current freshness.
    /// Reassess at every new decision time instead of reusing prior status.
    pub fn validate_inputs(
        &self,
        snapshot: &MarketSnapshot,
        gex: Option<&GexTimeObservation>,
        decision_timestamp_millis: i64,
        limits: &ExplicitAgeLimits,
    ) -> Result<(), SnapshotTimingProvenanceError> {
        if self.snapshot_id != snapshot.snapshot_id() {
            return Err(SnapshotTimingProvenanceError::SnapshotMismatch);
        }
        if self.source_snapshot != *snapshot {
            return Err(SnapshotTimingProvenanceError::SnapshotContentsMismatch);
        }
        if self.source_gex.as_ref() != gex {
            return Err(SnapshotTimingProvenanceError::GexObservationMismatch);
        }
        if self.decision_timestamp_millis != decision_timestamp_millis {
            return Err(SnapshotTimingProvenanceError::DecisionClockMismatch);
        }
        if self.age_limits != *limits {
            return Err(SnapshotTimingProvenanceError::AgeLimitsMismatch);
        }
        Ok(())
    }

    /// This only means the explicitly observed clocks are within the supplied
    /// limits. The complete strategy §16 hard veto still needs independent
    /// market families, GEX scope/coverage, broker and risk provenance.
    #[must_use]
    pub fn observed_clocks_within_limits(&self) -> bool {
        self.market_data == TemporalStatus::WithinConfiguredAge
            && self.gex_data == TemporalStatus::WithinConfiguredAge
            && self.decision_age == TemporalStatus::WithinConfiguredAge
    }
}

pub(crate) fn classify_pair(
    source_timestamp_millis: Option<i64>,
    received_timestamp_millis: Option<i64>,
    observation_time_millis: i64,
    max_age_ms: u64,
    quality: DataQuality,
) -> TemporalStatus {
    if observation_time_millis < 0 {
        return TemporalStatus::InvalidTimeline;
    }
    let (Some(source), Some(received)) = (source_timestamp_millis, received_timestamp_millis)
    else {
        return TemporalStatus::Missing;
    };
    if source < 0 || received < 0 || source > received || received > observation_time_millis {
        return TemporalStatus::InvalidTimeline;
    }
    if quality == DataQuality::Unknown {
        return TemporalStatus::UnknownQuality;
    }
    // Monotonicity ensures nonnegative integer arithmetic with no floating
    // precision loss. It must never wrap or silently treat future data as fresh.
    match u64::try_from(observation_time_millis - source) {
        Ok(age) if age <= max_age_ms => TemporalStatus::WithinConfiguredAge,
        Ok(_) => TemporalStatus::Stale,
        Err(_) => TemporalStatus::InvalidTimeline,
    }
}

/// Evaluate only what the immutable snapshot and separately provided GEX
/// timestamps can prove. This is not a complete provider health, DATA_FRESH,
/// DATA_VALID, or trade-authorization calculation.
pub fn assess_snapshot_timing(
    snapshot: &MarketSnapshot,
    gex: Option<GexTimeObservation>,
    decision_timestamp_millis: i64,
    limits: &ExplicitAgeLimits,
) -> SnapshotTimingAssessment {
    let market_data = classify_pair(
        Some(snapshot.source_timestamp_millis()),
        Some(snapshot.received_timestamp_millis()),
        decision_timestamp_millis,
        limits.market_data_stale_timeout_ms,
        snapshot.data_quality(),
    );

    let decision_age = classify_pair(
        Some(snapshot.received_timestamp_millis()),
        Some(snapshot.received_timestamp_millis()),
        decision_timestamp_millis,
        limits.max_decision_age_ms,
        snapshot.data_quality(),
    );

    let gex_data = match gex.as_ref() {
        Some(gex)
            if gex.snapshot_id != snapshot.snapshot_id() || gex.symbol != snapshot.symbol() =>
        {
            TemporalStatus::IdentityMismatch
        }
        Some(gex) => classify_pair(
            gex.source_timestamp_millis,
            gex.received_timestamp_millis,
            decision_timestamp_millis,
            limits.gex_stale_timeout_ms,
            gex.data_quality,
        ),
        None => TemporalStatus::Missing,
    };

    SnapshotTimingAssessment {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        market_data,
        gex_data,
        decision_age,
        source_snapshot: snapshot.clone(),
        source_gex: gex,
        decision_timestamp_millis,
        age_limits: *limits,
    }
}
