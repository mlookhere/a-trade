use crate::timing::classify_pair;
use crate::{DataQuality, MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, TemporalStatus};

/// Five required OHLCV scopes in canonical auction §11, assessed as data
/// coverage evidence only. An AI-native playbook may have additional explicit
/// requirements; this check does not authorize either strategy.
pub const CANONICAL_REQUIRED_OHLCV_SCOPES: [OhlcvTimeframe; 5] = [
    OhlcvTimeframe::Minute1,
    OhlcvTimeframe::Minute5,
    OhlcvTimeframe::Minute15,
    OhlcvTimeframe::Hour1,
    OhlcvTimeframe::Hour4,
];

/// Caller-supplied, versioned maximum ages; no production defaults or
/// generalized timeout across unlike candle timeframes are invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequiredOhlcvAgeLimits {
    pub minute1_ms: u64,
    pub minute5_ms: u64,
    pub minute15_ms: u64,
    pub hour1_ms: u64,
    pub hour4_ms: u64,
}

impl RequiredOhlcvAgeLimits {
    const fn limit_for(self, timeframe: OhlcvTimeframe) -> Option<u64> {
        match timeframe {
            OhlcvTimeframe::Minute1 => Some(self.minute1_ms),
            OhlcvTimeframe::Minute5 => Some(self.minute5_ms),
            OhlcvTimeframe::Minute15 => Some(self.minute15_ms),
            OhlcvTimeframe::Hour1 => Some(self.hour1_ms),
            OhlcvTimeframe::Hour4 => Some(self.hour4_ms),
            OhlcvTimeframe::Daily => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OhlcvCoverageError {
    SnapshotMismatch,
    SymbolMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OhlcvScopeObservation {
    pub timeframe: OhlcvTimeframe,
    pub status: TemporalStatus,
    /// The source timestamp is supplied by the feed; it is not a derived bar
    /// close time or evidence of the provider's candle-construction algorithm.
    pub source_timestamp_millis: Option<i64>,
    /// Present only when the observed timeline is structurally valid.
    pub measured_age_millis: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredOhlcvCoverage {
    snapshot_id: String,
    symbol: String,
    source_id: Option<String>,
    quality: Option<DataQuality>,
    decision_timestamp_millis: i64,
    observations: Vec<OhlcvScopeObservation>,
}

impl RequiredOhlcvCoverage {
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    #[must_use]
    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    #[must_use]
    pub const fn source_quality(&self) -> Option<DataQuality> {
        self.quality
    }

    /// The exact decision clock used for this assessment. A previous
    /// within-age status is not a claim of freshness at a later clock.
    #[must_use]
    pub const fn decision_timestamp_millis(&self) -> i64 {
        self.decision_timestamp_millis
    }

    /// Order is fixed: 1m, 5m, 15m, 1h, 4h. No authorization score is derived.
    #[must_use]
    pub fn observations(&self) -> &[OhlcvScopeObservation] {
        &self.observations
    }
}

/// This report records its assessment decision time; reusing it later does
/// not revalidate the same observations at the new decision time.
/// Partial completeness and time evidence only, NOT canonical DATA_VALID,
/// entire AI-native DATA_FRESH, a provider trust certificate, or trade ALLOW.
///
/// Missing scopes stay Missing, unrelated daily bars do not substitute for
/// required intraday scopes, and quote-level freshness is never borrowed.
pub fn assess_required_ohlcv_coverage(
    snapshot: &MarketSnapshot,
    series: Option<&OhlcvObservationSeries>,
    decision_timestamp_millis: i64,
    limits: &RequiredOhlcvAgeLimits,
) -> Result<RequiredOhlcvCoverage, OhlcvCoverageError> {
    if let Some(series) = series {
        if series.snapshot_id() != snapshot.snapshot_id() {
            return Err(OhlcvCoverageError::SnapshotMismatch);
        }
        if series.symbol() != snapshot.symbol() {
            return Err(OhlcvCoverageError::SymbolMismatch);
        }
    }

    let invalid_decision_clock = decision_timestamp_millis < snapshot.received_timestamp_millis()
        || decision_timestamp_millis < 0;
    let mut observations = Vec::with_capacity(CANONICAL_REQUIRED_OHLCV_SCOPES.len());

    for timeframe in CANONICAL_REQUIRED_OHLCV_SCOPES {
        let latest = series.and_then(|s| s.latest_completed(timeframe));
        let source_timestamp_millis = latest.map(|bar| bar.source_timestamp_millis());
        let (status, measured_age_millis) = match (series, latest) {
            (_, _) if invalid_decision_clock => (TemporalStatus::InvalidTimeline, None),
            (None, _) | (_, None) => (TemporalStatus::Missing, None),
            (Some(series), Some(bar)) => {
                // The fixed canonical set contains no Daily scope. If this
                // contract changes, an unconfigured scope must not borrow a
                // timeout from another timeframe or panic.
                let Some(limit) = limits.limit_for(timeframe) else {
                    observations.push(OhlcvScopeObservation {
                        timeframe,
                        status: TemporalStatus::Missing,
                        source_timestamp_millis,
                        measured_age_millis: None,
                    });
                    continue;
                };
                let status = classify_pair(
                    Some(bar.source_timestamp_millis()),
                    Some(bar.received_timestamp_millis()),
                    decision_timestamp_millis,
                    limit,
                    series.quality(),
                );
                let age = if matches!(
                    status,
                    TemporalStatus::WithinConfiguredAge | TemporalStatus::Stale
                ) {
                    decision_timestamp_millis
                        .checked_sub(bar.source_timestamp_millis())
                        .and_then(|v| u64::try_from(v).ok())
                } else {
                    None
                };
                (status, age)
            }
        };
        observations.push(OhlcvScopeObservation {
            timeframe,
            status,
            source_timestamp_millis,
            measured_age_millis,
        });
    }

    Ok(RequiredOhlcvCoverage {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        symbol: snapshot.symbol().to_owned(),
        source_id: series.map(|s| s.source_id().to_owned()),
        quality: series.map(OhlcvObservationSeries::quality),
        decision_timestamp_millis,
        observations,
    })
}
