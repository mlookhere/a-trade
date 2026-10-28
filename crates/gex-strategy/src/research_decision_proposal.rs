use crate::{
    AdvisoryDirection, MarketSnapshot, Playbook, ResearchAdversarialReview,
    ResearchAdversarialReviewError,
};

/// Explicitly submitted research hypothesis. No playbook is calculated or
/// granted eligibility from this input, even if all observations are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchDecisionProposalInput {
    pub snapshot_id: String,
    pub proposer_id: String,
    pub playbook: Playbook,
    pub direction: AdvisoryDirection,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchDecisionProposalError {
    MissingSnapshotId,
    SnapshotMismatch,
    SnapshotContentsMismatch,
    MissingProposerId,
    BlankRationale,
    ReviewProvenance(ResearchAdversarialReviewError),
    ReviewChanged,
    InputChanged,
}

/// A snapshot-bound hypothesis and its complete original adversarial evidence.
///
/// Construction attests only to structural consistency, not source truth,
/// timing freshness, playbook eligibility, calibration, risk, a strategy
/// decision or permission to trade. There is deliberately no ALLOW API.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchDecisionProposal {
    source_snapshot: MarketSnapshot,
    source_review: ResearchAdversarialReview,
    source_input: ResearchDecisionProposalInput,
}

impl ResearchDecisionProposal {
    pub fn build(
        snapshot: &MarketSnapshot,
        review: &ResearchAdversarialReview,
        input: ResearchDecisionProposalInput,
    ) -> Result<Self, ResearchDecisionProposalError> {
        if input.snapshot_id.trim().is_empty() {
            return Err(ResearchDecisionProposalError::MissingSnapshotId);
        }
        if input.snapshot_id != snapshot.snapshot_id()
            || review.source_packet().source_snapshot().snapshot_id() != snapshot.snapshot_id()
        {
            return Err(ResearchDecisionProposalError::SnapshotMismatch);
        }
        if review.source_packet().source_snapshot() != snapshot {
            return Err(ResearchDecisionProposalError::SnapshotContentsMismatch);
        }
        if input.proposer_id.trim().is_empty() {
            return Err(ResearchDecisionProposalError::MissingProposerId);
        }
        if input.rationale.trim().is_empty() {
            return Err(ResearchDecisionProposalError::BlankRationale);
        }
        review
            .validate_inputs(snapshot, review.source_packet())
            .map_err(ResearchDecisionProposalError::ReviewProvenance)?;

        Ok(Self {
            source_snapshot: snapshot.clone(),
            source_review: review.clone(),
            source_input: input,
        })
    }

    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
    }

    #[must_use]
    pub fn source_review(&self) -> &ResearchAdversarialReview {
        &self.source_review
    }

    /// This is a caller-supplied hypothesis only, not a confirmed result.
    #[must_use]
    pub fn source_input(&self) -> &ResearchDecisionProposalInput {
        &self.source_input
    }

    /// Historical source consistency only. A new decision clock or changed
    /// market state requires new research, never implicit reauthorization.
    pub fn validate_inputs(
        &self,
        snapshot: &MarketSnapshot,
        review: &ResearchAdversarialReview,
        input: &ResearchDecisionProposalInput,
    ) -> Result<(), ResearchDecisionProposalError> {
        if snapshot.snapshot_id() != self.source_snapshot.snapshot_id()
            || review.source_packet().source_snapshot().snapshot_id() != snapshot.snapshot_id()
            || input.snapshot_id != snapshot.snapshot_id()
        {
            return Err(ResearchDecisionProposalError::SnapshotMismatch);
        }
        if snapshot != &self.source_snapshot || review.source_packet().source_snapshot() != snapshot
        {
            return Err(ResearchDecisionProposalError::SnapshotContentsMismatch);
        }
        review
            .validate_inputs(snapshot, review.source_packet())
            .map_err(ResearchDecisionProposalError::ReviewProvenance)?;
        if review != &self.source_review {
            return Err(ResearchDecisionProposalError::ReviewChanged);
        }
        if input != &self.source_input {
            return Err(ResearchDecisionProposalError::InputChanged);
        }
        Ok(())
    }
}
