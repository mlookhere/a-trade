use crate::{
    EvidenceAudit, EvidenceAuditProvenanceError, ExplicitAgeLimits, HardVetoProvenanceError,
    HardVetoResult, MarketSnapshot, ResearchGexRoleComparison, ResearchGexRoleComparisonError,
    SnapshotTimingAssessment, SnapshotTimingProvenanceError,
};

/// Research evidence-consistency errors only; not strategy rejection codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchReviewPacketError {
    SnapshotMismatch,
    SnapshotContentsMismatch,
    AuditProvenance(EvidenceAuditProvenanceError),
    RoleProvenance(ResearchGexRoleComparisonError),
    TimingProvenance(SnapshotTimingProvenanceError),
    HardVetoProvenance(HardVetoProvenanceError),
    AuditChanged,
    RoleComparisonChanged,
    TimingAssessmentChanged,
    HardVetoChanged,
}

/// A read-only collection of original reports for review by an agent.
/// It does NOT certify truth, calibrated likelihood, data freshness, risk,
/// playbook eligibility or the right to send an order.
///
/// Never treat successful construction as an ALLOW decision. The independent
/// source clock may need to be reassessed before every later decision.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchReviewEvidencePacket {
    source_snapshot: MarketSnapshot,
    audit: EvidenceAudit,
    role_comparison: ResearchGexRoleComparison,
    timing: SnapshotTimingAssessment,
    hard_veto: HardVetoResult,
}

impl ResearchReviewEvidencePacket {
    pub fn build(
        snapshot: &MarketSnapshot,
        audit: EvidenceAudit,
        role_comparison: ResearchGexRoleComparison,
        timing: SnapshotTimingAssessment,
        hard_veto: HardVetoResult,
    ) -> Result<Self, ResearchReviewPacketError> {
        if audit.snapshot_id() != snapshot.snapshot_id()
            || role_comparison.source_snapshot().snapshot_id() != snapshot.snapshot_id()
            || timing.snapshot_id() != snapshot.snapshot_id()
            || hard_veto.snapshot_id() != snapshot.snapshot_id()
        {
            return Err(ResearchReviewPacketError::SnapshotMismatch);
        }
        if audit.source_snapshot() != snapshot
            || role_comparison.source_snapshot() != snapshot
            || timing.source_snapshot() != snapshot
            || hard_veto.source_snapshot() != snapshot
        {
            return Err(ResearchReviewPacketError::SnapshotContentsMismatch);
        }
        audit
            .validate_inputs(snapshot, audit.source_features(), audit.source_batch())
            .map_err(ResearchReviewPacketError::AuditProvenance)?;
        role_comparison
            .validate_snapshot(snapshot)
            .map_err(ResearchReviewPacketError::RoleProvenance)?;
        timing
            .validate_inputs(
                snapshot,
                timing.source_gex(),
                timing.decision_timestamp_millis(),
                &timing.age_limits(),
            )
            .map_err(ResearchReviewPacketError::TimingProvenance)?;
        hard_veto
            .validate_inputs(snapshot, hard_veto.source_input())
            .map_err(ResearchReviewPacketError::HardVetoProvenance)?;

        Ok(Self {
            source_snapshot: snapshot.clone(),
            audit,
            role_comparison,
            timing,
            hard_veto,
        })
    }

    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
    }

    #[must_use]
    pub fn audit(&self) -> &EvidenceAudit {
        &self.audit
    }

    #[must_use]
    pub fn role_comparison(&self) -> &ResearchGexRoleComparison {
        &self.role_comparison
    }

    #[must_use]
    pub fn timing(&self) -> &SnapshotTimingAssessment {
        &self.timing
    }

    /// Exact original caller-supplied 13-condition report, never permission
    /// to trade. Even Cleared is not an Allow verdict.
    #[must_use]
    pub fn hard_veto(&self) -> &HardVetoResult {
        &self.hard_veto
    }

    /// Compare exact source and original report witnesses. A new clock must
    /// yield a new timing assessment; a historical report is never refreshed
    /// by merely revalidating this packet.
    pub fn validate_inputs(
        &self,
        snapshot: &MarketSnapshot,
        audit: &EvidenceAudit,
        role_comparison: &ResearchGexRoleComparison,
        timing: &SnapshotTimingAssessment,
        hard_veto: &HardVetoResult,
        decision_timestamp_millis: i64,
        limits: &ExplicitAgeLimits,
    ) -> Result<(), ResearchReviewPacketError> {
        if snapshot.snapshot_id() != self.source_snapshot.snapshot_id() {
            return Err(ResearchReviewPacketError::SnapshotMismatch);
        }
        if snapshot != &self.source_snapshot {
            return Err(ResearchReviewPacketError::SnapshotContentsMismatch);
        }
        audit
            .validate_inputs(snapshot, audit.source_features(), audit.source_batch())
            .map_err(ResearchReviewPacketError::AuditProvenance)?;
        role_comparison
            .validate_snapshot(snapshot)
            .map_err(ResearchReviewPacketError::RoleProvenance)?;
        timing
            .validate_inputs(
                snapshot,
                timing.source_gex(),
                decision_timestamp_millis,
                limits,
            )
            .map_err(ResearchReviewPacketError::TimingProvenance)?;
        hard_veto
            .validate_inputs(snapshot, hard_veto.source_input())
            .map_err(ResearchReviewPacketError::HardVetoProvenance)?;

        if audit != &self.audit {
            return Err(ResearchReviewPacketError::AuditChanged);
        }
        if role_comparison != &self.role_comparison {
            return Err(ResearchReviewPacketError::RoleComparisonChanged);
        }
        if timing != &self.timing {
            return Err(ResearchReviewPacketError::TimingAssessmentChanged);
        }
        if hard_veto != &self.hard_veto {
            return Err(ResearchReviewPacketError::HardVetoChanged);
        }
        Ok(())
    }
}
