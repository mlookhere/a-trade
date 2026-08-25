#![forbid(unsafe_code)]

use std::collections::BTreeMap;

/// AI-native GEX spec §§2, 35, 42.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingMode {
    StrictSourceBaseline,
    AiNative,
}

/// AI-native GEX spec §7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataQuality {
    Structured,
    Parsed,
    Vision,
    Unknown,
}

/// AI-native GEX spec §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    Premarket,
    Regular,
    AfterHours,
}

/// AI-native GEX spec §8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GexSign {
    Positive,
    Negative,
    Zero,
}

/// AI-native GEX spec §8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionFromSpot {
    Above,
    Below,
    At,
}

/// Aggregate and expiry-specific GEX are distinct ranking domains per AI-native GEX spec §§7-8.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum GexScope {
    Aggregate,
    Expiry(String),
}

/// AI-native GEX spec §9. Role assignment is intentionally not inferred from magnitude alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GexLevelRole {
    TargetMagnet,
    EntryPivot,
    Support,
    Resistance,
    BreakoutPivot,
    TrapDoor,
    SecondaryTarget,
    Unassigned,
}

/// AI-native GEX spec §10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GammaRegime {
    Positive,
    Negative,
    Mixed,
    Unknown,
}

/// AI-native GEX spec §11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GexInteractionState {
    Approaching,
    Testing,
    Absorbing,
    Rejecting,
    Holding,
    AcceptingAbove,
    AcceptingBelow,
    Breaking,
    AcceleratingAway,
    Ignoring,
    Unknown,
}

/// AI-native GEX spec §§19-23.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playbook {
    PositiveGammaMagnet,
    PositiveGammaPivotReaction,
    NegativeGammaExpansion,
    MixedGammaTransition,
    None,
}

/// AI-native GEX spec §§30-33.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionVerdict {
    Allow,
    Wait,
    Reject,
}

/// Required provenance categories from the AI-native GEX spec §0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleProvenance {
    SourceDerived,
    AiNativeAdaptation,
    Configurable,
    ValidationRequiredAssumption,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawGexLevel {
    pub strike: f64,
    pub gex_value: f64,
    pub scope: GexScope,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedGexLevel {
    pub strike: f64,
    pub gex_value: f64,
    pub sign: GexSign,
    pub magnitude: f64,
    pub distance_from_spot: f64,
    pub direction_from_spot: DirectionFromSpot,
    pub scope: GexScope,
    pub rank_within_scope: usize,
    pub role: GexLevelRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GexNormalizationError {
    InvalidSpot,
    InvalidStrike,
    InvalidGexValue,
    InvalidScope,
}

/// Deterministic AI-native GEX normalization from spec §8.
///
/// Ranking is performed independently for each scope. Aggregate and expiry-specific levels are
/// therefore never directly ranked against each other.
pub fn normalize_gex_levels(
    spot_price: f64,
    raw_levels: &[RawGexLevel],
) -> Result<Vec<NormalizedGexLevel>, GexNormalizationError> {
    if !spot_price.is_finite() || spot_price <= 0.0 {
        return Err(GexNormalizationError::InvalidSpot);
    }

    let mut normalized = Vec::with_capacity(raw_levels.len());
    for raw in raw_levels {
        if !raw.strike.is_finite() || raw.strike <= 0.0 {
            return Err(GexNormalizationError::InvalidStrike);
        }
        if !raw.gex_value.is_finite() {
            return Err(GexNormalizationError::InvalidGexValue);
        }
        if matches!(&raw.scope, GexScope::Expiry(expiry) if expiry.trim().is_empty()) {
            return Err(GexNormalizationError::InvalidScope);
        }

        let sign = if raw.gex_value > 0.0 {
            GexSign::Positive
        } else if raw.gex_value < 0.0 {
            GexSign::Negative
        } else {
            GexSign::Zero
        };
        let direction_from_spot = if raw.strike > spot_price {
            DirectionFromSpot::Above
        } else if raw.strike < spot_price {
            DirectionFromSpot::Below
        } else {
            DirectionFromSpot::At
        };

        normalized.push(NormalizedGexLevel {
            strike: raw.strike,
            gex_value: raw.gex_value,
            sign,
            magnitude: raw.gex_value.abs(),
            distance_from_spot: (raw.strike - spot_price).abs(),
            direction_from_spot,
            scope: raw.scope.clone(),
            rank_within_scope: 0,
            role: GexLevelRole::Unassigned,
        });
    }

    let mut scope_indices: BTreeMap<GexScope, Vec<usize>> = BTreeMap::new();
    for (index, level) in normalized.iter().enumerate() {
        scope_indices
            .entry(level.scope.clone())
            .or_default()
            .push(index);
    }

    for indices in scope_indices.values_mut() {
        indices.sort_by(|left_index, right_index| {
            let left = &normalized[*left_index];
            let right = &normalized[*right_index];
            right
                .magnitude
                .total_cmp(&left.magnitude)
                .then_with(|| left.strike.total_cmp(&right.strike))
        });
        for (rank, index) in indices.iter().enumerate() {
            normalized[*index].rank_within_scope = rank + 1;
        }
    }

    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketSnapshotInput {
    pub snapshot_id: String,
    pub symbol: String,
    pub source_timestamp_millis: i64,
    pub received_timestamp_millis: i64,
    pub spot_price: f64,
    pub session: Session,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketSnapshot {
    snapshot_id: String,
    symbol: String,
    source_timestamp_millis: i64,
    received_timestamp_millis: i64,
    spot_price: f64,
    session: Session,
    data_quality: DataQuality,
    gex_levels: Vec<NormalizedGexLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError {
    MissingSnapshotId,
    MissingSymbol,
    InvalidSourceTimestamp,
    InvalidReceivedTimestamp,
    SourceAfterReceived,
    InvalidSpot,
    InvalidGex,
}

impl MarketSnapshot {
    /// AI-native GEX spec §§6-8, 28, 39. The snapshot is immutable after construction.
    pub fn build(
        input: MarketSnapshotInput,
        raw_gex_levels: &[RawGexLevel],
    ) -> Result<Self, SnapshotError> {
        if input.snapshot_id.trim().is_empty() {
            return Err(SnapshotError::MissingSnapshotId);
        }
        if input.symbol.trim().is_empty() {
            return Err(SnapshotError::MissingSymbol);
        }
        if input.source_timestamp_millis < 0 {
            return Err(SnapshotError::InvalidSourceTimestamp);
        }
        if input.received_timestamp_millis < 0 {
            return Err(SnapshotError::InvalidReceivedTimestamp);
        }
        if input.source_timestamp_millis > input.received_timestamp_millis {
            return Err(SnapshotError::SourceAfterReceived);
        }
        if !input.spot_price.is_finite() || input.spot_price <= 0.0 {
            return Err(SnapshotError::InvalidSpot);
        }

        let gex_levels =
            normalize_gex_levels(input.spot_price, raw_gex_levels).map_err(|_| SnapshotError::InvalidGex)?;

        Ok(Self {
            snapshot_id: input.snapshot_id,
            symbol: input.symbol,
            source_timestamp_millis: input.source_timestamp_millis,
            received_timestamp_millis: input.received_timestamp_millis,
            spot_price: input.spot_price,
            session: input.session,
            data_quality: input.data_quality,
            gex_levels,
        })
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
    pub const fn source_timestamp_millis(&self) -> i64 {
        self.source_timestamp_millis
    }

    #[must_use]
    pub const fn received_timestamp_millis(&self) -> i64 {
        self.received_timestamp_millis
    }

    #[must_use]
    pub const fn data_age_millis(&self) -> u64 {
        (self.received_timestamp_millis - self.source_timestamp_millis) as u64
    }

    #[must_use]
    pub const fn spot_price(&self) -> f64 {
        self.spot_price
    }

    #[must_use]
    pub const fn session(&self) -> Session {
        self.session
    }

    #[must_use]
    pub const fn data_quality(&self) -> DataQuality {
        self.data_quality
    }

    #[must_use]
    pub fn gex_levels(&self) -> &[NormalizedGexLevel] {
        &self.gex_levels
    }
}

/// Research-only classifier configuration for AI-native GEX spec §§10, 34, 36, 42.
///
/// `window_points` and `mixed_share_threshold` have no defaults. The caller must supply reviewed
/// values. `mixed_share_threshold` is interpreted by this research implementation as the minimum
/// opposing-side share of total absolute GEX required to classify the selected scope as MIXED.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGammaRegimeConfig {
    pub window_points: f64,
    pub mixed_share_threshold: f64,
    pub scope: GexScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchRegimeError {
    InvalidWindow,
    InvalidMixedShareThreshold,
    InvalidScope,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGammaRegimeResult {
    pub regime: GammaRegime,
    pub positive_magnitude: f64,
    pub negative_magnitude: f64,
    pub levels_considered: usize,
    pub provenance: RuleProvenance,
}

/// AI-native research adaptation for spec §10.
///
/// This deterministic formula is intentionally classified as `ValidationRequiredAssumption` and
/// MUST NOT be treated as a live production rule until the project's versioned validation and
/// approval process promotes it.
pub fn classify_research_gamma_regime(
    levels: &[NormalizedGexLevel],
    config: &ResearchGammaRegimeConfig,
) -> Result<ResearchGammaRegimeResult, ResearchRegimeError> {
    if !config.window_points.is_finite() || config.window_points <= 0.0 {
        return Err(ResearchRegimeError::InvalidWindow);
    }
    if !config.mixed_share_threshold.is_finite()
        || config.mixed_share_threshold <= 0.0
        || config.mixed_share_threshold > 0.5
    {
        return Err(ResearchRegimeError::InvalidMixedShareThreshold);
    }
    if matches!(&config.scope, GexScope::Expiry(expiry) if expiry.trim().is_empty()) {
        return Err(ResearchRegimeError::InvalidScope);
    }

    let mut positive_magnitude = 0.0;
    let mut negative_magnitude = 0.0;
    let mut levels_considered = 0;

    for level in levels {
        if level.scope != config.scope || level.distance_from_spot > config.window_points {
            continue;
        }
        levels_considered += 1;
        match level.sign {
            GexSign::Positive => positive_magnitude += level.magnitude,
            GexSign::Negative => negative_magnitude += level.magnitude,
            GexSign::Zero => {}
        }
    }

    let total = positive_magnitude + negative_magnitude;
    let regime = if total == 0.0 {
        GammaRegime::Unknown
    } else if positive_magnitude > 0.0 && negative_magnitude > 0.0 {
        let opposing_share = positive_magnitude.min(negative_magnitude) / total;
        if opposing_share >= config.mixed_share_threshold {
            GammaRegime::Mixed
        } else if positive_magnitude > negative_magnitude {
            GammaRegime::Positive
        } else {
            GammaRegime::Negative
        }
    } else if positive_magnitude > 0.0 {
        GammaRegime::Positive
    } else {
        GammaRegime::Negative
    };

    Ok(ResearchGammaRegimeResult {
        regime,
        positive_magnitude,
        negative_magnitude,
        levels_considered,
        provenance: RuleProvenance::ValidationRequiredAssumption,
    })
}
