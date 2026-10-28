use std::collections::HashSet;

use crate::{ClaimObservationStatus, MarketSnapshot, ResearchReviewEvidencePacket};

/// Research-only location of a claim within one original specialist audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResearchChallengeClaimSide {
    Supporting,
    Contradicting,
}

/// An objection to an existing claim, not an assertion of truth or a vote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchAdversarialChallengeInput {
    pub snapshot_id: String,
    pub reviewer_id: String,
    pub target_agent_id: String,
    pub claim_side: ResearchChallengeClaimSide,
    pub claim_index: usize,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchAdversarialReviewError {
    MissingSnapshotId,
    SnapshotMismatch,
    SnapshotContentsMismatch,
    PacketContentsMismatch,
    MissingReviewerId,
    MissingTargetAgentId,
    BlankRationale,
    UnknownTargetAgent,
    InvalidClaimIndex,
    DuplicateChallenge,
}

/// A challenge preserves the original audited claim text and resolution.
/// Neither an observation nor an objection proves the claim's semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchAdversarialChallenge {
    reviewer_id: String,
    target_agent_id: String,
    claim_side: ResearchChallengeClaimSide,
    claim_index: usize,
    target_claim: String,
    observed_status: ClaimObservationStatus,
    rationale: String,
}

impl ResearchAdversarialChallenge {
    #[must_use]
    pub fn reviewer_id(&self) -> &str {
        &self.reviewer_id
    }

    #[must_use]
    pub fn target_agent_id(&self) -> &str {
        &self.target_agent_id
    }

    #[must_use]
    pub const fn claim_side(&self) -> ResearchChallengeClaimSide {
        self.claim_side
    }

    #[must_use]
    pub const fn claim_index(&self) -> usize {
        self.claim_index
    }

    #[must_use]
    pub fn target_claim(&self) -> &str {
        &self.target_claim
    }

    #[must_use]
    pub const fn observed_status(&self) -> ClaimObservationStatus {
        self.observed_status
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
}

/// Non-authorizing observational challenge log, bound to the complete original
/// four-witness review packet. An empty log is NOT a favorable review.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchAdversarialReview {
    source_packet: ResearchReviewEvidencePacket,
    challenges: Vec<ResearchAdversarialChallenge>,
}

impl ResearchAdversarialReview {
    pub fn build(
        snapshot: &MarketSnapshot,
        packet: &ResearchReviewEvidencePacket,
        inputs: Vec<ResearchAdversarialChallengeInput>,
    ) -> Result<Self, ResearchAdversarialReviewError> {
        if snapshot.snapshot_id() != packet.source_snapshot().snapshot_id() {
            return Err(ResearchAdversarialReviewError::SnapshotMismatch);
        }
        if snapshot != packet.source_snapshot() {
            return Err(ResearchAdversarialReviewError::SnapshotContentsMismatch);
        }

        let mut seen = HashSet::new();
        let mut challenges = Vec::with_capacity(inputs.len());

        for input in inputs {
            if input.snapshot_id.trim().is_empty() {
                return Err(ResearchAdversarialReviewError::MissingSnapshotId);
            }
            if input.snapshot_id != snapshot.snapshot_id() {
                return Err(ResearchAdversarialReviewError::SnapshotMismatch);
            }
            if input.reviewer_id.trim().is_empty() {
                return Err(ResearchAdversarialReviewError::MissingReviewerId);
            }
            if input.target_agent_id.trim().is_empty() {
                return Err(ResearchAdversarialReviewError::MissingTargetAgentId);
            }
            if input.rationale.trim().is_empty() {
                return Err(ResearchAdversarialReviewError::BlankRationale);
            }

            let specialist = packet
                .audit()
                .specialists()
                .iter()
                .find(|agent| agent.agent_id() == input.target_agent_id)
                .ok_or(ResearchAdversarialReviewError::UnknownTargetAgent)?;
            let claims = match input.claim_side {
                ResearchChallengeClaimSide::Supporting => specialist.supporting(),
                ResearchChallengeClaimSide::Contradicting => specialist.contradicting(),
            };
            let claim = claims
                .get(input.claim_index)
                .ok_or(ResearchAdversarialReviewError::InvalidClaimIndex)?;

            if !seen.insert((
                input.reviewer_id.clone(),
                input.target_agent_id.clone(),
                input.claim_side,
                input.claim_index,
            )) {
                return Err(ResearchAdversarialReviewError::DuplicateChallenge);
            }
            challenges.push(ResearchAdversarialChallenge {
                reviewer_id: input.reviewer_id,
                target_agent_id: input.target_agent_id,
                claim_side: input.claim_side,
                claim_index: input.claim_index,
                target_claim: claim.claim().to_owned(),
                observed_status: claim.status(),
                rationale: input.rationale,
            });
        }

        Ok(Self {
            source_packet: packet.clone(),
            challenges,
        })
    }

    #[must_use]
    pub fn source_packet(&self) -> &ResearchReviewEvidencePacket {
        &self.source_packet
    }

    #[must_use]
    pub fn challenges(&self) -> &[ResearchAdversarialChallenge] {
        &self.challenges
    }

    /// Exact historical witness check, not freshness or approval.
    pub fn validate_inputs(
        &self,
        snapshot: &MarketSnapshot,
        packet: &ResearchReviewEvidencePacket,
    ) -> Result<(), ResearchAdversarialReviewError> {
        if snapshot.snapshot_id() != self.source_packet.source_snapshot().snapshot_id()
            || packet.source_snapshot().snapshot_id() != snapshot.snapshot_id()
        {
            return Err(ResearchAdversarialReviewError::SnapshotMismatch);
        }
        if snapshot != self.source_packet.source_snapshot()
            || packet.source_snapshot() != snapshot
        {
            return Err(ResearchAdversarialReviewError::SnapshotContentsMismatch);
        }
        if packet != &self.source_packet {
            return Err(ResearchAdversarialReviewError::PacketContentsMismatch);
        }
        Ok(())
    }
}
