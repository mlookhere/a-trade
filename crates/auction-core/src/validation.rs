use std::collections::HashSet;

use crate::{RejectionCode, SetupState};

/// Offline validation phases from canonical §116. Paper/shadow is deliberately excluded from
/// this harness because it requires a later execution environment rather than offline replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationPhase {
    Backtest,
    OutOfSample,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationDatasetManifest {
    pub strategy_version: String,
    pub backtest_session_ids: Vec<String>,
    pub out_of_sample_session_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationManifestError {
    MissingStrategyVersion,
    MissingBacktestSessions,
    MissingOutOfSampleSessions,
    EmptySessionId,
    DuplicateSessionId,
    SplitOverlap,
}

impl ValidationDatasetManifest {
    /// Deterministic split validation for §116 evidence. Requiring both sets and preventing
    /// overlap is validation infrastructure only; it does not alter any production trade rule.
    pub fn validate(&self) -> Result<(), ValidationManifestError> {
        if self.strategy_version.trim().is_empty() {
            return Err(ValidationManifestError::MissingStrategyVersion);
        }
        if self.backtest_session_ids.is_empty() {
            return Err(ValidationManifestError::MissingBacktestSessions);
        }
        if self.out_of_sample_session_ids.is_empty() {
            return Err(ValidationManifestError::MissingOutOfSampleSessions);
        }

        let mut backtest = HashSet::with_capacity(self.backtest_session_ids.len());
        for session_id in &self.backtest_session_ids {
            if session_id.trim().is_empty() {
                return Err(ValidationManifestError::EmptySessionId);
            }
            if !backtest.insert(session_id.as_str()) {
                return Err(ValidationManifestError::DuplicateSessionId);
            }
        }

        let mut out_of_sample = HashSet::with_capacity(self.out_of_sample_session_ids.len());
        for session_id in &self.out_of_sample_session_ids {
            if session_id.trim().is_empty() {
                return Err(ValidationManifestError::EmptySessionId);
            }
            if !out_of_sample.insert(session_id.as_str()) {
                return Err(ValidationManifestError::DuplicateSessionId);
            }
            if backtest.contains(session_id.as_str()) {
                return Err(ValidationManifestError::SplitOverlap);
            }
        }
        Ok(())
    }

    pub fn phase_for(
        &self,
        session_id: &str,
    ) -> Result<Option<ValidationPhase>, ValidationManifestError> {
        self.validate()?;
        if self.backtest_session_ids.iter().any(|id| id == session_id) {
            Ok(Some(ValidationPhase::Backtest))
        } else if self
            .out_of_sample_session_ids
            .iter()
            .any(|id| id == session_id)
        {
            Ok(Some(ValidationPhase::OutOfSample))
        } else {
            Ok(None)
        }
    }
}

/// Final deterministic result captured by an offline replay case. A rejection code is optional
/// because not every fail-closed condition is assigned a unique terminal rejection at this layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayOutcome {
    pub trade_authorized: bool,
    pub rejection_code: Option<RejectionCode>,
    pub setup_state: Option<SetupState>,
}

impl ReplayOutcome {
    fn valid(self) -> bool {
        !(self.trade_authorized && self.rejection_code.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayCaseEvidence {
    case_id: String,
    setup_id: String,
    session_id: String,
    phase: ValidationPhase,
    expected: ReplayOutcome,
    observed: ReplayOutcome,
}

impl ReplayCaseEvidence {
    #[must_use]
    pub fn case_id(&self) -> &str {
        &self.case_id
    }

    #[must_use]
    pub fn setup_id(&self) -> &str {
        &self.setup_id
    }

    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    pub const fn phase(&self) -> ValidationPhase {
        self.phase
    }

    #[must_use]
    pub const fn expected(&self) -> ReplayOutcome {
        self.expected
    }

    #[must_use]
    pub const fn observed(&self) -> ReplayOutcome {
        self.observed
    }

    #[must_use]
    pub fn exact_match(&self) -> bool {
        self.expected == self.observed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayEvidenceError {
    InvalidManifest(ValidationManifestError),
    MissingCaseId,
    MissingSetupId,
    MissingSessionId,
    UnknownSession,
    PhaseMismatch,
    InvalidOutcome,
    EmptyEvidence,
    MixedPhases,
    DuplicateCaseId,
    DuplicateSetupEvidence,
}

impl From<ValidationManifestError> for ReplayEvidenceError {
    fn from(value: ValidationManifestError) -> Self {
        Self::InvalidManifest(value)
    }
}

pub fn record_replay_case(
    manifest: &ValidationDatasetManifest,
    case_id: &str,
    setup_id: &str,
    session_id: &str,
    phase: ValidationPhase,
    expected: ReplayOutcome,
    observed: ReplayOutcome,
) -> Result<ReplayCaseEvidence, ReplayEvidenceError> {
    manifest.validate()?;
    if case_id.trim().is_empty() {
        return Err(ReplayEvidenceError::MissingCaseId);
    }
    if setup_id.trim().is_empty() {
        return Err(ReplayEvidenceError::MissingSetupId);
    }
    if session_id.trim().is_empty() {
        return Err(ReplayEvidenceError::MissingSessionId);
    }
    if !expected.valid() || !observed.valid() {
        return Err(ReplayEvidenceError::InvalidOutcome);
    }

    let Some(manifest_phase) = manifest.phase_for(session_id)? else {
        return Err(ReplayEvidenceError::UnknownSession);
    };
    if manifest_phase != phase {
        return Err(ReplayEvidenceError::PhaseMismatch);
    }

    Ok(ReplayCaseEvidence {
        case_id: case_id.to_owned(),
        setup_id: setup_id.to_owned(),
        session_id: session_id.to_owned(),
        phase,
        expected,
        observed,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayReport {
    pub phase: ValidationPhase,
    pub total_cases: usize,
    pub exact_matches: usize,
    pub mismatches: usize,
    pub authorization_mismatches: usize,
    pub rejection_mismatches: usize,
    pub state_mismatches: usize,
}

impl ReplayReport {
    /// Structural integrity only. This does not decide whether results are good enough for
    /// promotion; canonical §116 defines no numeric acceptance threshold.
    #[must_use]
    pub const fn structurally_valid(self) -> bool {
        self.total_cases > 0
            && self.mismatches <= self.total_cases
            && self.exact_matches == self.total_cases - self.mismatches
            && self.authorization_mismatches <= self.mismatches
            && self.rejection_mismatches <= self.mismatches
            && self.state_mismatches <= self.mismatches
    }
}

/// Summarizes exact replay evidence only. Canonical knowledge defines no performance threshold
/// that would permit this report to approve, promote, or version a strategy automatically.
pub fn summarize_replay(
    phase: ValidationPhase,
    evidence: &[ReplayCaseEvidence],
) -> Result<ReplayReport, ReplayEvidenceError> {
    if evidence.is_empty() {
        return Err(ReplayEvidenceError::EmptyEvidence);
    }
    if evidence.iter().any(|case| case.phase != phase) {
        return Err(ReplayEvidenceError::MixedPhases);
    }

    let mut case_ids = HashSet::with_capacity(evidence.len());
    let mut setup_keys = HashSet::with_capacity(evidence.len());
    for case in evidence {
        if !case_ids.insert(case.case_id.as_str()) {
            return Err(ReplayEvidenceError::DuplicateCaseId);
        }
        if !setup_keys.insert((case.session_id.as_str(), case.setup_id.as_str())) {
            return Err(ReplayEvidenceError::DuplicateSetupEvidence);
        }
    }

    let total_cases = evidence.len();
    let exact_matches = evidence.iter().filter(|case| case.exact_match()).count();
    let authorization_mismatches = evidence
        .iter()
        .filter(|case| case.expected.trade_authorized != case.observed.trade_authorized)
        .count();
    let rejection_mismatches = evidence
        .iter()
        .filter(|case| case.expected.rejection_code != case.observed.rejection_code)
        .count();
    let state_mismatches = evidence
        .iter()
        .filter(|case| case.expected.setup_state != case.observed.setup_state)
        .count();

    Ok(ReplayReport {
        phase,
        total_cases,
        exact_matches,
        mismatches: total_cases - exact_matches,
        authorization_mismatches,
        rejection_mismatches,
        state_mismatches,
    })
}
