use crate::{
    AiNativeFeatureSet, ClaimStatus, DataQuality, EvidenceClaim, EvidenceKind, EvidenceReference,
    FeatureReference, FeatureSource, GexScope, MarketSnapshot, SpecialistBatch,
};

/// AI-native GEX §§6–8,24–27,33,39,42. This layer verifies actual
/// snapshot-bound observation identity and presence, NOT latency freshness,
/// semantic truth of the specialist's prose, or trading permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceStatus {
    Observed,
    Missing,
    Ambiguous,
    Unsupported,
    DataQualityUnknown,
}

/// Preserve the original numeric domain: timestamps are integers, not
/// approximate floating-point values disguised as market evidence.
#[derive(Debug, Clone, PartialEq)]
pub enum ObservedValue {
    Price(f64),
    TimestampMillis(i64),
    AgeMillis(u64),
    Feature(f64),
    Gex(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceCheck {
    pub reference: EvidenceReference,
    pub status: ReferenceStatus,
    /// Present only when the corresponding numeric observation really exists.
    pub observed_value: Option<ObservedValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimObservationStatus {
    /// No evidence or one or more references missing, ambiguous or unverified.
    Unknown,
    /// References exist in snapshot; data freshness and claim semantics not certified.
    ObservedReferencesOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimObservation {
    pub claim: String,
    pub status: ClaimObservationStatus,
    pub references: Vec<ReferenceCheck>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialistObservation {
    pub agent_id: String,
    pub feature_references: Vec<ReferenceCheck>,
    pub supporting: Vec<ClaimObservation>,
    pub contradicting: Vec<ClaimObservation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceAudit {
    pub snapshot_id: String,
    pub specialists: Vec<SpecialistObservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceAuditError {
    SnapshotMismatch,
    FeatureSnapshotMismatch,
    InvalidGexIdentity,
}

/// Stable, explicitly scope-qualified GEX reference generated from the same
/// identity as a normalized level. A textual level such as "aggregate:610"
/// is NOT silently accepted as proof.
pub fn gex_level_reference(
    scope: &GexScope,
    strike: f64,
) -> Result<EvidenceReference, EvidenceAuditError> {
    if !strike.is_finite() || strike <= 0.0 {
        return Err(EvidenceAuditError::InvalidGexIdentity);
    }
    let prefix = match scope {
        GexScope::Aggregate => "gex/aggregate".to_owned(),
        GexScope::Expiry(expiry) if !expiry.trim().is_empty() => {
            format!("gex/expiry/{}:{}", expiry.len(), expiry)
        }
        _ => return Err(EvidenceAuditError::InvalidGexIdentity),
    };
    Ok(EvidenceReference {
        kind: EvidenceKind::GexLevel,
        reference: format!("{prefix}/{:016x}", strike.to_bits()),
    })
}

/// The only accepted snapshot field names are explicit and backed by the
/// immutable typed MarketSnapshot. Numeric presence alone is not freshness.
fn snapshot_field(snapshot: &MarketSnapshot, key: &str) -> Option<ObservedValue> {
    match key {
        "spot_price" => Some(ObservedValue::Price(snapshot.spot_price())),
        "source_timestamp_millis" => Some(ObservedValue::TimestampMillis(snapshot.source_timestamp_millis())),
        "received_timestamp_millis" => Some(ObservedValue::TimestampMillis(snapshot.received_timestamp_millis())),
        "data_age_millis" => Some(ObservedValue::AgeMillis(snapshot.data_age_millis())),
        _ => None,
    }
}

/// Exact feature registry: do not parse arbitrary LLM-generated keys into
/// new calculations, defaults, normalizations, or strategy semantics.
fn feature_field(features: &AiNativeFeatureSet, key: &str) -> Option<Option<f64>> {
    Some(match key {
        "trend.trend_direction" => features.trend().trend_direction,
        "trend.trend_strength" => features.trend().trend_strength,
        "trend.ema_alignment" => features.trend().ema_alignment,
        "trend.ema_separation" => features.trend().ema_separation,
        "trend.ema_slope" => features.trend().ema_slope,
        "trend.trend_acceleration" => features.trend().trend_acceleration,
        "trend.multi_timeframe_agreement" => features.trend().multi_timeframe_agreement,
        "trend.regime_transition_probability" => features.trend().regime_transition_probability,
        "structure.impulse_strength" => features.structure().impulse_strength,
        "structure.retracement_quality" => features.structure().retracement_quality,
        "structure.range_contraction" => features.structure().range_contraction,
        "structure.volatility_compression" => features.structure().volatility_compression,
        "structure.volume_contraction" => features.structure().volume_contraction,
        "structure.directional_structure_preserved" => {
            features.structure().directional_structure_preserved
        }
        "structure.breakout_pressure" => features.structure().breakout_pressure,
        "candle_event.rejection_strength" => features.candle_event().rejection_strength,
        "candle_event.wick_asymmetry" => features.candle_event().wick_asymmetry,
        "candle_event.close_location_value" => features.candle_event().close_location_value,
        "candle_event.reclaim_strength" => features.candle_event().reclaim_strength,
        "candle_event.absorption_score" => features.candle_event().absorption_score,
        "candle_event.follow_through_score" => features.candle_event().follow_through_score,
        "candle_event.volume_delta" => features.candle_event().volume_delta,
        "candle_event.trade_velocity" => features.candle_event().trade_velocity,
        "candle_event.bid_ask_imbalance" => features.candle_event().bid_ask_imbalance,
        "candle_event.liquidity_response" => features.candle_event().liquidity_response,
        "participation.relative_volume" => features.participation().relative_volume,
        "participation.volume_acceleration" => features.participation().volume_acceleration,
        "participation.trade_count" => features.participation().trade_count,
        "participation.average_trade_size" => features.participation().average_trade_size,
        "participation.buy_sell_imbalance" => features.participation().buy_sell_imbalance,
        "participation.volume_delta" => features.participation().volume_delta,
        "participation.volume_at_price" => features.participation().volume_at_price,
        "participation.liquidity_depletion" => features.participation().liquidity_depletion,
        "participation.spread_quality" => features.participation().spread_quality,
        "participation.quote_replenishment" => features.participation().quote_replenishment,
        "participation.price_movement_per_unit_volume" => {
            features.participation().price_movement_per_unit_volume
        }
        "participation.consolidation_volume_contraction" => {
            features.participation().consolidation_volume_contraction
        }
        "participation.breakout_volume_expansion" => {
            features.participation().breakout_volume_expansion
        }
        "participation.participation_score" => features.participation().participation_score,
        "liquidity_execution.liquidity_depletion" => {
            features.liquidity_execution().liquidity_depletion
        }
        "liquidity_execution.spread_quality" => features.liquidity_execution().spread_quality,
        "liquidity_execution.quote_replenishment" => {
            features.liquidity_execution().quote_replenishment
        }
        "liquidity_execution.expected_spread_cost" => {
            features.liquidity_execution().expected_spread_cost
        }
        "liquidity_execution.expected_slippage" => features.liquidity_execution().expected_slippage,
        "liquidity_execution.fees" => features.liquidity_execution().fees,
        "liquidity_execution.execution_risk_penalty" => {
            features.liquidity_execution().execution_risk_penalty
        }
        "portfolio_broker.gross_exposure" => features.portfolio_broker().gross_exposure,
        "portfolio_broker.net_directional_exposure" => {
            features.portfolio_broker().net_directional_exposure
        }
        "portfolio_broker.same_underlying_exposure" => {
            features.portfolio_broker().same_underlying_exposure
        }
        "portfolio_broker.correlated_underlying_exposure" => {
            features.portfolio_broker().correlated_underlying_exposure
        }
        "portfolio_broker.concentration_exposure" => {
            features.portfolio_broker().concentration_exposure
        }
        "portfolio_broker.aggregate_gamma_exposure" => {
            features.portfolio_broker().aggregate_gamma_exposure
        }
        "portfolio_broker.aggregate_vega_exposure" => {
            features.portfolio_broker().aggregate_vega_exposure
        }
        "portfolio_broker.aggregate_theta_exposure" => {
            features.portfolio_broker().aggregate_theta_exposure
        }
        "portfolio_broker.daily_loss" => features.portfolio_broker().daily_loss,
        "portfolio_broker.open_order_risk" => features.portfolio_broker().open_order_risk,
        _ => return None,
    })
}

fn observed(reference: &EvidenceReference, value: ObservedValue) -> ReferenceCheck {
    ReferenceCheck {
        reference: reference.clone(),
        status: ReferenceStatus::Observed,
        observed_value: Some(value),
    }
}

fn unresolved(reference: &EvidenceReference, status: ReferenceStatus) -> ReferenceCheck {
    ReferenceCheck {
        reference: reference.clone(),
        status,
        observed_value: None,
    }
}

/// Inspect an EvidenceReference against the actual immutable snapshot/feature set.
/// This API has no execution/ALLOW/EV output. Even Observed requires a separate
/// deterministic timestamp, provenance and rule-specific validity gate.
pub fn resolve_reference(
    snapshot: &MarketSnapshot,
    features: &AiNativeFeatureSet,
    reference: &EvidenceReference,
) -> Result<ReferenceCheck, EvidenceAuditError> {
    if snapshot.snapshot_id() != features.snapshot_id() {
        return Err(EvidenceAuditError::FeatureSnapshotMismatch);
    }
    if snapshot.data_quality() == DataQuality::Unknown {
        return Ok(unresolved(reference, ReferenceStatus::DataQualityUnknown));
    }

    let result = match reference.kind {
        EvidenceKind::SnapshotField => snapshot_field(snapshot, &reference.reference)
            .map(|v| observed(reference, v))
            .unwrap_or_else(|| unresolved(reference, ReferenceStatus::Unsupported)),
        EvidenceKind::DeterministicFeature => match feature_field(features, &reference.reference) {
            Some(Some(v)) => observed(reference, ObservedValue::Feature(v)),
            Some(None) => unresolved(reference, ReferenceStatus::Missing),
            None => unresolved(reference, ReferenceStatus::Unsupported),
        },
        EvidenceKind::GexLevel => {
            let mut count = 0;
            let mut value = 0.0;
            for level in snapshot.gex_levels() {
                let identity = gex_level_reference(&level.scope, level.strike)?;
                if identity.reference == reference.reference {
                    count += 1;
                    value = level.gex_value;
                }
            }
            match count {
                0 => unresolved(reference, ReferenceStatus::Missing),
                1 => observed(reference, ObservedValue::Gex(value)),
                _ => unresolved(reference, ReferenceStatus::Ambiguous),
            }
        }
        // No corresponding typed immutable source in MarketSnapshot yet.
        // Broker, event, price interaction, external flow must stay unavailable.
        EvidenceKind::PriceInteraction
        | EvidenceKind::MarketContext
        | EvidenceKind::Catalyst
        | EvidenceKind::PortfolioState
        | EvidenceKind::BrokerState => unresolved(reference, ReferenceStatus::Unsupported),
    };
    Ok(result)
}

/// A feature reference is not independently calculated or treated as evidence
/// merely because it names a feature. The same deterministic source registry
/// is used for the shared feature set.
pub fn resolve_feature_reference(
    snapshot: &MarketSnapshot,
    features: &AiNativeFeatureSet,
    reference: &FeatureReference,
) -> Result<ReferenceCheck, EvidenceAuditError> {
    let kind = match reference.source {
        FeatureSource::Trend
        | FeatureSource::Structure
        | FeatureSource::CandleEvent
        | FeatureSource::Participation
        | FeatureSource::LiquidityExecution
        | FeatureSource::PortfolioBroker => EvidenceKind::DeterministicFeature,
        FeatureSource::Gex => EvidenceKind::GexLevel,
        FeatureSource::Snapshot => EvidenceKind::SnapshotField,
    };
    // Scope and feature category must agree; a valid key cannot be laundered
    // through the wrong source category.
    let valid_group = match reference.source {
        FeatureSource::Trend => reference.key.starts_with("trend."),
        FeatureSource::Structure => reference.key.starts_with("structure."),
        FeatureSource::CandleEvent => reference.key.starts_with("candle_event."),
        FeatureSource::Participation => reference.key.starts_with("participation."),
        FeatureSource::LiquidityExecution => reference.key.starts_with("liquidity_execution."),
        FeatureSource::PortfolioBroker => reference.key.starts_with("portfolio_broker."),
        FeatureSource::Gex => reference.key.starts_with("gex/"),
        FeatureSource::Snapshot => true,
    };
    let mapped = EvidenceReference {
        kind,
        reference: reference.key.clone(),
    };
    if !valid_group {
        return Ok(unresolved(&mapped, ReferenceStatus::Unsupported));
    }
    resolve_reference(snapshot, features, &mapped)
}

fn evaluate_claim(
    snapshot: &MarketSnapshot,
    features: &AiNativeFeatureSet,
    claim: &EvidenceClaim,
) -> Result<ClaimObservation, EvidenceAuditError> {
    let mut checks = Vec::with_capacity(claim.evidence().len());
    for r in claim.evidence() {
        checks.push(resolve_reference(snapshot, features, r)?);
    }
    let fully_observed = claim.status() == ClaimStatus::ReferencedUnverified
        && !checks.is_empty()
        && checks.iter().all(|c| c.status == ReferenceStatus::Observed);
    Ok(ClaimObservation {
        claim: claim.claim().to_owned(),
        status: if fully_observed {
            ClaimObservationStatus::ObservedReferencesOnly
        } else {
            ClaimObservationStatus::Unknown
        },
        references: checks,
    })
}

/// Validate all specialist outputs from one immutable snapshot. This is an
/// evidence audit only; no majority voting, arbiter, probability or trade ALLOW.
pub fn audit_specialist_evidence(
    snapshot: &MarketSnapshot,
    features: &AiNativeFeatureSet,
    batch: &SpecialistBatch,
) -> Result<EvidenceAudit, EvidenceAuditError> {
    if batch.snapshot_id() != snapshot.snapshot_id()
        || batch
            .outputs()
            .iter()
            .any(|o| o.snapshot_id() != snapshot.snapshot_id())
    {
        return Err(EvidenceAuditError::SnapshotMismatch);
    }
    if features.snapshot_id() != snapshot.snapshot_id() {
        return Err(EvidenceAuditError::FeatureSnapshotMismatch);
    }
    let mut specialists = Vec::with_capacity(batch.outputs().len());
    for output in batch.outputs() {
        let mut feature_references = Vec::with_capacity(output.features().len());
        for feature in output.features() {
            feature_references.push(resolve_feature_reference(snapshot, features, feature)?);
        }
        let mut supporting = Vec::with_capacity(output.supporting_evidence().len());
        for claim in output.supporting_evidence() {
            supporting.push(evaluate_claim(snapshot, features, claim)?);
        }
        let mut contradicting = Vec::with_capacity(output.contradicting_evidence().len());
        for claim in output.contradicting_evidence() {
            contradicting.push(evaluate_claim(snapshot, features, claim)?);
        }
        specialists.push(SpecialistObservation {
            agent_id: output.agent_id().to_owned(),
            feature_references,
            supporting,
            contradicting,
        });
    }
    Ok(EvidenceAudit {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        specialists,
    })
}
