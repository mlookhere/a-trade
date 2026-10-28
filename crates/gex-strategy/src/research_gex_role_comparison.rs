use std::collections::{BTreeMap, HashSet};

use crate::{GexLevelRole, GexScope, MarketSnapshot, ResearchGexRoleReport};

/// A named research contributor, not a source of authenticated market truth.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGexRoleSubmission {
    agent_id: String,
    report: ResearchGexRoleReport,
}

/// Deterministic transport validation only. These are not trading rejection
/// codes and none can be converted into an ALLOW state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchGexRoleComparisonError {
    MissingAgentId,
    DuplicateAgentId,
    SnapshotMismatch,
    SnapshotContentsMismatch,
}

impl ResearchGexRoleSubmission {
    pub fn new(
        agent_id: &str,
        report: ResearchGexRoleReport,
    ) -> Result<Self, ResearchGexRoleComparisonError> {
        if agent_id.trim().is_empty() {
            return Err(ResearchGexRoleComparisonError::MissingAgentId);
        }
        Ok(Self {
            agent_id: agent_id.to_owned(),
            report,
        })
    }

    #[must_use]
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    #[must_use]
    pub fn report(&self) -> &ResearchGexRoleReport {
        &self.report
    }
}

/// Even fully matching independent annotations remain unverified; agreement
/// is not proof of the intended role or permission to act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchGexRoleObservationStatus {
    Unverified,
    Conflicting,
}

/// A single agent's exact asserted role, with no ranking or vote weight.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGexRoleAgentObservation {
    agent_id: String,
    role: GexLevelRole,
    scope: GexScope,
    strike: f64,
    rationale: String,
}

impl ResearchGexRoleAgentObservation {
    #[must_use]
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    #[must_use]
    pub const fn role(&self) -> GexLevelRole {
        self.role
    }

    #[must_use]
    pub fn scope(&self) -> &GexScope {
        &self.scope
    }

    #[must_use]
    pub const fn strike(&self) -> f64 {
        self.strike
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGexRoleLevelObservation {
    level_index: usize,
    status: ResearchGexRoleObservationStatus,
    claims: Vec<ResearchGexRoleAgentObservation>,
}

impl ResearchGexRoleLevelObservation {
    #[must_use]
    pub const fn level_index(&self) -> usize {
        self.level_index
    }

    #[must_use]
    pub const fn status(&self) -> ResearchGexRoleObservationStatus {
        self.status
    }

    /// There is deliberately no winning or consensus role accessor.
    #[must_use]
    pub fn claims(&self) -> &[ResearchGexRoleAgentObservation] {
        &self.claims
    }
}

/// Observational multi-agent comparison. Identical role claims cannot become
/// verified merely by repetition, and any contradictory role remains visible.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGexRoleComparison {
    source_snapshot: MarketSnapshot,
    submissions: Vec<ResearchGexRoleSubmission>,
    levels: Vec<ResearchGexRoleLevelObservation>,
}

impl ResearchGexRoleComparison {
    pub fn build(
        snapshot: &MarketSnapshot,
        submissions: Vec<ResearchGexRoleSubmission>,
    ) -> Result<Self, ResearchGexRoleComparisonError> {
        let mut agent_ids = HashSet::new();
        let mut by_level: BTreeMap<usize, Vec<ResearchGexRoleAgentObservation>> = BTreeMap::new();

        for submission in &submissions {
            if !agent_ids.insert(submission.agent_id().to_owned()) {
                return Err(ResearchGexRoleComparisonError::DuplicateAgentId);
            }
            let source = submission.report().source_snapshot();
            if source.snapshot_id() != snapshot.snapshot_id() {
                return Err(ResearchGexRoleComparisonError::SnapshotMismatch);
            }
            if source != snapshot {
                return Err(ResearchGexRoleComparisonError::SnapshotContentsMismatch);
            }
            for annotation in submission.report().annotations() {
                by_level.entry(annotation.level_index()).or_default().push(
                    ResearchGexRoleAgentObservation {
                        agent_id: submission.agent_id().to_owned(),
                        role: annotation.role(),
                        scope: annotation.scope().clone(),
                        strike: annotation.strike(),
                        rationale: annotation.rationale().to_owned(),
                    },
                );
            }
        }

        let levels = by_level
            .into_iter()
            .map(|(level_index, claims)| {
                let conflicting = claims.iter().any(|claim| claim.role != claims[0].role);
                ResearchGexRoleLevelObservation {
                    level_index,
                    status: if conflicting {
                        ResearchGexRoleObservationStatus::Conflicting
                    } else {
                        ResearchGexRoleObservationStatus::Unverified
                    },
                    claims,
                }
            })
            .collect();

        Ok(Self {
            source_snapshot: snapshot.clone(),
            submissions,
            levels,
        })
    }

    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
    }

    #[must_use]
    pub fn submissions(&self) -> &[ResearchGexRoleSubmission] {
        &self.submissions
    }

    /// Only explicitly annotated indices occur here; missing indices remain
    /// unknown with no inferred role or default signal.
    #[must_use]
    pub fn levels(&self) -> &[ResearchGexRoleLevelObservation] {
        &self.levels
    }

    #[must_use]
    pub fn for_level(&self, index: usize) -> Option<&ResearchGexRoleLevelObservation> {
        self.levels.iter().find(|level| level.level_index == index)
    }

    pub fn validate_snapshot(
        &self,
        snapshot: &MarketSnapshot,
    ) -> Result<(), ResearchGexRoleComparisonError> {
        if self.source_snapshot.snapshot_id() != snapshot.snapshot_id() {
            return Err(ResearchGexRoleComparisonError::SnapshotMismatch);
        }
        if &self.source_snapshot != snapshot {
            return Err(ResearchGexRoleComparisonError::SnapshotContentsMismatch);
        }
        Ok(())
    }
}
