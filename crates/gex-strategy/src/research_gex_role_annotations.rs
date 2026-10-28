use std::collections::HashSet;

use crate::{GexLevelRole, GexScope, MarketSnapshot};

/// Explicit research annotation only. The caller, not GEX arithmetic, supplies
/// the role and rationale. This does not establish whether the role is true.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGexRoleInput {
    pub snapshot_id: String,
    pub level_index: usize,
    pub scope: GexScope,
    pub strike: f64,
    pub role: GexLevelRole,
    pub rationale: String,
}

/// Structural integrity errors, not role-eligibility or trade decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchGexRoleError {
    MissingSnapshotId,
    SnapshotMismatch,
    SnapshotContentsMismatch,
    InvalidLevelIndex,
    InvalidStrike,
    ScopeMismatch,
    StrikeMismatch,
    BlankRationale,
    DuplicateLevelIndex,
}

/// Immutable original annotations, preserving their exact source-level index.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGexRoleAnnotation {
    level_index: usize,
    scope: GexScope,
    strike: f64,
    role: GexLevelRole,
    rationale: String,
}

impl ResearchGexRoleAnnotation {
    #[must_use]
    pub const fn level_index(&self) -> usize {
        self.level_index
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
    pub const fn role(&self) -> GexLevelRole {
        self.role
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
}

/// An isolated, non-authoritative research transport. No level's role is
/// inferred from sign, relative magnitude, scope or proximity to spot.
///
/// A caller MUST NOT interpret a present annotation as proof that its
/// rationale is correct, that GEX data are fresh or that a trade is allowed.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchGexRoleReport {
    source_snapshot: MarketSnapshot,
    annotations: Vec<ResearchGexRoleAnnotation>,
}

impl ResearchGexRoleReport {
    pub fn build(
        snapshot: &MarketSnapshot,
        inputs: Vec<ResearchGexRoleInput>,
    ) -> Result<Self, ResearchGexRoleError> {
        let mut seen_indices = HashSet::new();
        let mut annotations = Vec::with_capacity(inputs.len());

        for input in inputs {
            if input.snapshot_id.trim().is_empty() {
                return Err(ResearchGexRoleError::MissingSnapshotId);
            }
            if input.snapshot_id != snapshot.snapshot_id() {
                return Err(ResearchGexRoleError::SnapshotMismatch);
            }
            let Some(level) = snapshot.gex_levels().get(input.level_index) else {
                return Err(ResearchGexRoleError::InvalidLevelIndex);
            };
            if !input.strike.is_finite() || input.strike <= 0.0 {
                return Err(ResearchGexRoleError::InvalidStrike);
            }
            if input.scope != level.scope {
                return Err(ResearchGexRoleError::ScopeMismatch);
            }
            // Exactly the same indexed level, not a proximity approximation.
            if input.strike.to_bits() != level.strike.to_bits() {
                return Err(ResearchGexRoleError::StrikeMismatch);
            }
            if input.rationale.trim().is_empty() {
                return Err(ResearchGexRoleError::BlankRationale);
            }
            if !seen_indices.insert(input.level_index) {
                return Err(ResearchGexRoleError::DuplicateLevelIndex);
            }
            annotations.push(ResearchGexRoleAnnotation {
                level_index: input.level_index,
                scope: input.scope,
                strike: input.strike,
                role: input.role,
                rationale: input.rationale,
            });
        }

        Ok(Self {
            source_snapshot: snapshot.clone(),
            annotations,
        })
    }

    #[must_use]
    pub fn source_snapshot(&self) -> &MarketSnapshot {
        &self.source_snapshot
    }

    #[must_use]
    pub fn annotations(&self) -> &[ResearchGexRoleAnnotation] {
        &self.annotations
    }

    /// None means no independently supplied role annotation. It is not
    /// automatically equivalent to a positive or negative trading signal.
    #[must_use]
    pub fn explicit_role_for(&self, level_index: usize) -> Option<GexLevelRole> {
        self.annotations
            .iter()
            .find(|item| item.level_index == level_index)
            .map(|item| item.role)
    }

    /// Re-check the complete snapshot witness, even if the label remains
    /// unchanged; this never verifies the *meaning* of the stated role.
    pub fn validate_snapshot(&self, snapshot: &MarketSnapshot) -> Result<(), ResearchGexRoleError> {
        if self.source_snapshot.snapshot_id() != snapshot.snapshot_id() {
            return Err(ResearchGexRoleError::SnapshotMismatch);
        }
        if &self.source_snapshot != snapshot {
            return Err(ResearchGexRoleError::SnapshotContentsMismatch);
        }
        Ok(())
    }
}
