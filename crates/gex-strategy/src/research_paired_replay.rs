use std::collections::HashSet;

use crate::{
    AdvisoryDirection, MarketSnapshot, ResearchDecisionProposal, ResearchDecisionProposalError,
};

/// Research split labels, not broker execution or strategy authorization states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchEvaluationPhase {
    Backtest,
    OutOfSample,
}

/// The version strings and session partitions are externally supplied,
/// not independently verified research outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchPairedEvaluationManifest {
    pub experiment_id: String,
    pub strict_baseline_version: String,
    pub ai_native_version: String,
    pub backtest_session_ids: Vec<String>,
    pub out_of_sample_session_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchPairedManifestError {
    MissingExperimentId,
    MissingBaselineVersion,
    MissingAiNativeVersion,
    MissingBacktestSessions,
    MissingOutOfSampleSessions,
    BlankSessionId,
    DuplicateSessionId,
    SplitOverlap,
}

impl ResearchPairedEvaluationManifest {
    pub fn validate(&self) -> Result<(), ResearchPairedManifestError> {
        if self.experiment_id.trim().is_empty() {
            return Err(ResearchPairedManifestError::MissingExperimentId);
        }
        if self.strict_baseline_version.trim().is_empty() {
            return Err(ResearchPairedManifestError::MissingBaselineVersion);
        }
        if self.ai_native_version.trim().is_empty() {
            return Err(ResearchPairedManifestError::MissingAiNativeVersion);
        }
        if self.backtest_session_ids.is_empty() {
            return Err(ResearchPairedManifestError::MissingBacktestSessions);
        }
        if self.out_of_sample_session_ids.is_empty() {
            return Err(ResearchPairedManifestError::MissingOutOfSampleSessions);
        }
        let mut seen = HashSet::new();
        for session in &self.backtest_session_ids {
            if session.trim().is_empty() {
                return Err(ResearchPairedManifestError::BlankSessionId);
            }
            if !seen.insert(session.as_str()) {
                return Err(ResearchPairedManifestError::DuplicateSessionId);
            }
        }
        let mut out_of_sample = HashSet::new();
        for session in &self.out_of_sample_session_ids {
            if session.trim().is_empty() {
                return Err(ResearchPairedManifestError::BlankSessionId);
            }
            if !out_of_sample.insert(session.as_str()) {
                return Err(ResearchPairedManifestError::DuplicateSessionId);
            }
            if seen.contains(session.as_str()) {
                return Err(ResearchPairedManifestError::SplitOverlap);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn phase_for(&self, session_id: &str) -> Option<ResearchEvaluationPhase> {
        if self.backtest_session_ids.iter().any(|id| id == session_id) {
            Some(ResearchEvaluationPhase::Backtest)
        } else if self.out_of_sample_session_ids.iter().any(|id| id == session_id) {
            Some(ResearchEvaluationPhase::OutOfSample)
        } else {
            None
        }
    }
}

/// A caller's UNVERIFIED StrictSourceBaseline mode annotation for one
/// snapshot. This does not run, simulate or certify a baseline strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchBaselineObservationInput {
    pub snapshot_id: String,
    pub source_record_id: String,
    pub direction: AdvisoryDirection,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchPairedReplayError {
    InvalidManifest(ResearchPairedManifestError),
    MissingCaseId,
    MissingSessionId,
    UnknownSession,
    MissingBaselineSnapshotId,
    MissingBaselineRecordId,
    BlankBaselineRationale,
    BaselineSnapshotMismatch,
    ProposalSnapshotMismatch,
    SnapshotContentsMismatch,
    ProposalProvenance(ResearchDecisionProposalError),
    EmptyCases,
    ManifestChanged,
    MixedPhases,
    DuplicateCaseId,
    DuplicateSessionSnapshot,
}

/// One exactly aligned research comparison. The baseline and AI-native
/// directions are observations, NOT trade decisions or profitability signals.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchPairedReplayCase {
    source_manifest: ResearchPairedEvaluationManifest,
    source_snapshot: MarketSnapshot,
    case_id: String,
    session_id: String,
    phase: ResearchEvaluationPhase,
    baseline: ResearchBaselineObservationInput,
    ai_native: ResearchDecisionProposal,
}

impl ResearchPairedReplayCase {
    pub fn record(
        manifest: &ResearchPairedEvaluationManifest,
        case_id: &str,
        session_id: &str,
        snapshot: &MarketSnapshot,
        baseline: ResearchBaselineObservationInput,
        ai_native: &ResearchDecisionProposal,
    ) -> Result<Self, ResearchPairedReplayError> {
        manifest
            .validate()
            .map_err(ResearchPairedReplayError::InvalidManifest)?;
        if case_id.trim().is_empty() {
            return Err(ResearchPairedReplayError::MissingCaseId);
        }
        if session_id.trim().is_empty() {
            return Err(ResearchPairedReplayError::MissingSessionId);
        }
        let phase = manifest
            .phase_for(session_id)
            .ok_or(ResearchPairedReplayError::UnknownSession)?;
        if baseline.snapshot_id.trim().is_empty() {
            return Err(ResearchPairedReplayError::MissingBaselineSnapshotId);
        }
        if baseline.source_record_id.trim().is_empty() {
            return Err(ResearchPairedReplayError::MissingBaselineRecordId);
        }
        if baseline.rationale.trim().is_empty() {
            return Err(ResearchPairedReplayError::BlankBaselineRationale);
        }
        if baseline.snapshot_id != snapshot.snapshot_id() {
            return Err(ResearchPairedReplayError::BaselineSnapshotMismatch);
        }
        if ai_native.source_snapshot().snapshot_id() != snapshot.snapshot_id() {
            return Err(ResearchPairedReplayError::ProposalSnapshotMismatch);
        }
        if ai_native.source_snapshot() != snapshot {
            return Err(ResearchPairedReplayError::SnapshotContentsMismatch);
        }
        ai_native
            .validate_inputs(snapshot, ai_native.source_review(), ai_native.source_input())
            .map_err(ResearchPairedReplayError::ProposalProvenance)?;

        Ok(Self {
            source_manifest: manifest.clone(),
            source_snapshot: snapshot.clone(),
            case_id: case_id.to_owned(),
            session_id: session_id.to_owned(),
            phase,
            baseline,
            ai_native: ai_native.clone(),
        })
    }

    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
    }

    #[must_use]
    pub fn case_id(&self) -> &str {
        &self.case_id
    }

    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    pub const fn phase(&self) -> ResearchEvaluationPhase {
        self.phase
    }

    #[must_use]
    pub fn baseline(&self) -> &ResearchBaselineObservationInput {
        &self.baseline
    }

    #[must_use]
    pub fn ai_native(&self) -> &ResearchDecisionProposal {
        &self.ai_native
    }
}

/// Direction concordance only. There is deliberately no performance score,
/// expected return, trade authorization, eligibility or promotion result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchPairedReplaySummary {
    phase: ResearchEvaluationPhase,
    total_cases: usize,
    same_known_direction: usize,
    different_known_direction: usize,
    unknown_direction: usize,
}

impl ResearchPairedReplaySummary {
    #[must_use]
    pub const fn phase(&self) -> ResearchEvaluationPhase {
        self.phase
    }

    #[must_use]
    pub const fn total_cases(&self) -> usize {
        self.total_cases
    }

    #[must_use]
    pub const fn same_known_direction(&self) -> usize {
        self.same_known_direction
    }

    #[must_use]
    pub const fn different_known_direction(&self) -> usize {
        self.different_known_direction
    }

    #[must_use]
    pub const fn unknown_direction(&self) -> usize {
        self.unknown_direction
    }
}

pub fn summarize_research_pairs(
    manifest: &ResearchPairedEvaluationManifest,
    phase: ResearchEvaluationPhase,
    cases: &[ResearchPairedReplayCase],
) -> Result<ResearchPairedReplaySummary, ResearchPairedReplayError> {
    manifest
        .validate()
        .map_err(ResearchPairedReplayError::InvalidManifest)?;
    if cases.is_empty() {
        return Err(ResearchPairedReplayError::EmptyCases);
    }

    let mut case_ids = HashSet::new();
    let mut snapshot_keys = HashSet::new();
    let mut same_known_direction = 0;
    let mut different_known_direction = 0;
    let mut unknown_direction = 0;
    for case in cases {
        if case.source_manifest != *manifest {
            return Err(ResearchPairedReplayError::ManifestChanged);
        }
        if case.phase != phase {
            return Err(ResearchPairedReplayError::MixedPhases);
        }
        if !case_ids.insert(case.case_id.as_str()) {
            return Err(ResearchPairedReplayError::DuplicateCaseId);
        }
        if !snapshot_keys.insert((case.session_id.as_str(), case.source_snapshot.snapshot_id())) {
            return Err(ResearchPairedReplayError::DuplicateSessionSnapshot);
        }
        let baseline = case.baseline.direction;
        let proposed = case.ai_native.source_input().direction;
        if baseline == AdvisoryDirection::Unknown || proposed == AdvisoryDirection::Unknown {
            unknown_direction += 1;
        } else if baseline == proposed {
            same_known_direction += 1;
        } else {
            different_known_direction += 1;
        }
    }
    Ok(ResearchPairedReplaySummary {
        phase,
        total_cases: cases.len(),
        same_known_direction,
        different_known_direction,
        unknown_direction,
    })
}
