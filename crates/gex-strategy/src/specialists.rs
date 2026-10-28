use std::collections::HashSet;

use crate::MarketSnapshot;

/// AI-native GEX spec §§24.1–24.8: specialist domains, not independent decision authorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialistRole {
    TrendMarketState,
    GexDealerPositioning,
    PriceStructureVolatility,
    VolumeFlowLiquidity,
    CrossMarketContext,
    CatalystEvent,
    InstrumentExecution,
    PortfolioRisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialistStatus {
    Ok,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisoryDirection {
    Long,
    Short,
    Neutral,
    Unknown,
}

/// An uncalibrated internal score. It MUST NOT be interpreted as P(win) or used for sizing.
/// §25 gives no numeric score range, so the interface only enforces finiteness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InternalConfidence(f64);

impl InternalConfidence {
    pub fn new(value: f64) -> Result<Self, SpecialistContractError> {
        if !value.is_finite() {
            return Err(SpecialistContractError::NonFiniteConfidence);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureSource {
    Trend,
    Structure,
    CandleEvent,
    Participation,
    LiquidityExecution,
    PortfolioBroker,
    Gex,
    Snapshot,
}

/// A feature *reference*, not a new calculation and not proof of data freshness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureReference {
    pub source: FeatureSource,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    SnapshotField,
    DeterministicFeature,
    GexLevel,
    PriceInteraction,
    MarketContext,
    Catalyst,
    PortfolioState,
    BrokerState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceReference {
    pub kind: EvidenceKind,
    pub reference: String,
}

/// The output contract can validate reference structure but cannot verify external evidence.
/// A separate snapshot-bound deterministic resolver MUST validate references before an arbiter
/// can treat a claim as supported. Ref presence is never equivalent to verified provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatus {
    Unknown,
    ReferencedUnverified,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceClaimInput {
    pub claim: String,
    pub evidence: Vec<EvidenceReference>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceClaim {
    claim: String,
    evidence: Vec<EvidenceReference>,
    confidence: Option<InternalConfidence>,
    status: ClaimStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialistContractError {
    MissingAgentId,
    MissingSnapshotId,
    SnapshotMismatch,
    BlankObservation,
    BlankFeatureReference,
    BlankEvidenceReference,
    BlankClaim,
    BlankUnknown,
    BlankRisk,
    BlankInvalidation,
    BlankReasonCode,
    NonFiniteConfidence,
    DuplicateAgentId,
}

impl EvidenceClaim {
    fn build(input: EvidenceClaimInput) -> Result<Self, SpecialistContractError> {
        if input.claim.trim().is_empty() {
            return Err(SpecialistContractError::BlankClaim);
        }
        if input
            .evidence
            .iter()
            .any(|e| e.reference.trim().is_empty())
        {
            return Err(SpecialistContractError::BlankEvidenceReference);
        }
        let confidence = input
            .confidence
            .map(InternalConfidence::new)
            .transpose()?;
        if input.evidence.is_empty() {
            return Ok(Self {
                claim: input.claim,
                evidence: Vec::new(),
                confidence: None,
                status: ClaimStatus::Unknown,
            });
        }
        Ok(Self {
            claim: input.claim,
            evidence: input.evidence,
            confidence,
            status: ClaimStatus::ReferencedUnverified,
        })
    }

    #[must_use]
    pub fn claim(&self) -> &str {
        &self.claim
    }

    #[must_use]
    pub fn evidence(&self) -> &[EvidenceReference] {
        &self.evidence
    }

    #[must_use]
    pub const fn confidence(&self) -> Option<InternalConfidence> {
        self.confidence
    }

    #[must_use]
    pub const fn status(&self) -> ClaimStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialistOutputInput {
    pub agent_id: String,
    pub role: SpecialistRole,
    pub snapshot_id: String,
    pub status: SpecialistStatus,
    pub direction: AdvisoryDirection,
    pub confidence: Option<f64>,
    pub observations: Vec<String>,
    pub features: Vec<FeatureReference>,
    pub supporting_evidence: Vec<EvidenceClaimInput>,
    pub contradicting_evidence: Vec<EvidenceClaimInput>,
    pub unknowns: Vec<String>,
    pub risks: Vec<String>,
    pub proposed_invalidation: Vec<String>,
    pub hard_veto: bool,
    pub reason_codes: Vec<String>,
}

/// AI-native §25 typed specialist output. No trading authorization or veto override API.
/// The agent-reported hard-veto flag is evidence ONLY; the §16 deterministic gate is separate.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialistOutput {
    agent_id: String,
    role: SpecialistRole,
    snapshot_id: String,
    status: SpecialistStatus,
    direction: AdvisoryDirection,
    confidence: Option<InternalConfidence>,
    observations: Vec<String>,
    features: Vec<FeatureReference>,
    supporting_evidence: Vec<EvidenceClaim>,
    contradicting_evidence: Vec<EvidenceClaim>,
    unknowns: Vec<String>,
    risks: Vec<String>,
    proposed_invalidation: Vec<String>,
    hard_veto_reported: bool,
    reason_codes: Vec<String>,
}

impl SpecialistOutput {
    pub fn build(
        snapshot: &MarketSnapshot,
        input: SpecialistOutputInput,
    ) -> Result<Self, SpecialistContractError> {
        if input.agent_id.trim().is_empty() {
            return Err(SpecialistContractError::MissingAgentId);
        }
        if input.snapshot_id.trim().is_empty() {
            return Err(SpecialistContractError::MissingSnapshotId);
        }
        if input.snapshot_id != snapshot.snapshot_id() {
            return Err(SpecialistContractError::SnapshotMismatch);
        }
        validate_nonblank(&input.observations, SpecialistContractError::BlankObservation)?;
        validate_nonblank(&input.unknowns, SpecialistContractError::BlankUnknown)?;
        validate_nonblank(&input.risks, SpecialistContractError::BlankRisk)?;
        validate_nonblank(
            &input.proposed_invalidation,
            SpecialistContractError::BlankInvalidation,
        )?;
        validate_nonblank(&input.reason_codes, SpecialistContractError::BlankReasonCode)?;
        if input.features.iter().any(|f| f.key.trim().is_empty()) {
            return Err(SpecialistContractError::BlankFeatureReference);
        }

        let confidence = input
            .confidence
            .map(InternalConfidence::new)
            .transpose()?;
        let supporting_evidence = input
            .supporting_evidence
            .into_iter()
            .map(EvidenceClaim::build)
            .collect::<Result<Vec<_>, _>>()?;
        let contradicting_evidence = input
            .contradicting_evidence
            .into_iter()
            .map(EvidenceClaim::build)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            agent_id: input.agent_id,
            role: input.role,
            snapshot_id: input.snapshot_id,
            status: input.status,
            direction: input.direction,
            confidence,
            observations: input.observations,
            features: input.features,
            supporting_evidence,
            contradicting_evidence,
            unknowns: input.unknowns,
            risks: input.risks,
            proposed_invalidation: input.proposed_invalidation,
            hard_veto_reported: input.hard_veto,
            reason_codes: input.reason_codes,
        })
    }

    #[must_use]
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
    #[must_use]
    pub const fn role(&self) -> SpecialistRole {
        self.role
    }
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }
    #[must_use]
    pub const fn status(&self) -> SpecialistStatus {
        self.status
    }
    #[must_use]
    pub const fn direction(&self) -> AdvisoryDirection {
        self.direction
    }
    #[must_use]
    pub const fn confidence(&self) -> Option<InternalConfidence> {
        self.confidence
    }
    #[must_use]
    pub fn observations(&self) -> &[String] {
        &self.observations
    }
    #[must_use]
    pub fn features(&self) -> &[FeatureReference] {
        &self.features
    }
    #[must_use]
    pub fn supporting_evidence(&self) -> &[EvidenceClaim] {
        &self.supporting_evidence
    }
    #[must_use]
    pub fn contradicting_evidence(&self) -> &[EvidenceClaim] {
        &self.contradicting_evidence
    }
    #[must_use]
    pub fn unknowns(&self) -> &[String] {
        &self.unknowns
    }
    #[must_use]
    pub fn risks(&self) -> &[String] {
        &self.risks
    }
    #[must_use]
    pub fn proposed_invalidation(&self) -> &[String] {
        &self.proposed_invalidation
    }
    #[must_use]
    pub const fn hard_veto_reported(&self) -> bool {
        self.hard_veto_reported
    }
    #[must_use]
    pub fn reason_codes(&self) -> &[String] {
        &self.reason_codes
    }
}

fn validate_nonblank(
    values: &[String],
    error: SpecialistContractError,
) -> Result<(), SpecialistContractError> {
    if values.iter().any(|v| v.trim().is_empty()) {
        return Err(error);
    }
    Ok(())
}

/// One-decision collection: prevents mixed snapshots and duplicate agents.
/// §§26,39 prohibit a majority-vote decision; this type exposes no aggregation/ALLOW API.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialistBatch {
    snapshot_id: String,
    outputs: Vec<SpecialistOutput>,
}

impl SpecialistBatch {
    pub fn build(
        snapshot: &MarketSnapshot,
        outputs: Vec<SpecialistOutput>,
    ) -> Result<Self, SpecialistContractError> {
        let mut ids = HashSet::with_capacity(outputs.len());
        for output in &outputs {
            if output.snapshot_id() != snapshot.snapshot_id() {
                return Err(SpecialistContractError::SnapshotMismatch);
            }
            if !ids.insert(output.agent_id().to_owned()) {
                return Err(SpecialistContractError::DuplicateAgentId);
            }
        }
        Ok(Self {
            snapshot_id: snapshot.snapshot_id().to_owned(),
            outputs,
        })
    }

    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }
    #[must_use]
    pub fn outputs(&self) -> &[SpecialistOutput] {
        &self.outputs
    }
}
