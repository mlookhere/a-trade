use crate::{
    ClaimObservationStatus, HardVetoVerdict, ReferenceStatus, ResearchGexRoleObservationStatus,
    ResearchReviewEvidencePacket, TemporalStatus,
};

/// Counts individual reference occurrences without inventing a reliability
/// score or certifying the meaning of any natural-language specialist claim.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResearchReferenceStatusCounts {
    observed: usize,
    missing: usize,
    ambiguous: usize,
    unsupported: usize,
    data_quality_unknown: usize,
}

impl ResearchReferenceStatusCounts {
    fn record(&mut self, status: ReferenceStatus) {
        match status {
            ReferenceStatus::Observed => self.observed += 1,
            ReferenceStatus::Missing => self.missing += 1,
            ReferenceStatus::Ambiguous => self.ambiguous += 1,
            ReferenceStatus::Unsupported => self.unsupported += 1,
            ReferenceStatus::DataQualityUnknown => self.data_quality_unknown += 1,
        }
    }

    #[must_use]
    pub const fn observed(&self) -> usize {
        self.observed
    }

    #[must_use]
    pub const fn missing(&self) -> usize {
        self.missing
    }

    #[must_use]
    pub const fn ambiguous(&self) -> usize {
        self.ambiguous
    }

    #[must_use]
    pub const fn unsupported(&self) -> usize {
        self.unsupported
    }

    #[must_use]
    pub const fn data_quality_unknown(&self) -> usize {
        self.data_quality_unknown
    }
}

/// Read-only observational facts computed from one immutable original packet.
/// This is NOT a verdict, evidence score, freshness assertion, strategy
/// validation, position/risk decision or permission to trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchReviewDiagnosticSummary {
    reference_counts: ResearchReferenceStatusCounts,
    unknown_claims: usize,
    unverified_role_levels: usize,
    conflicting_role_levels: usize,
    independent_gex_timing: TemporalStatus,
    hard_veto_verdict: HardVetoVerdict,
    failed_hard_conditions: usize,
    unknown_hard_conditions: usize,
}

impl ResearchReviewDiagnosticSummary {
    #[must_use]
    pub const fn reference_counts(&self) -> &ResearchReferenceStatusCounts {
        &self.reference_counts
    }

    #[must_use]
    pub const fn unknown_claims(&self) -> usize {
        self.unknown_claims
    }

    #[must_use]
    pub const fn unverified_role_levels(&self) -> usize {
        self.unverified_role_levels
    }

    #[must_use]
    pub const fn conflicting_role_levels(&self) -> usize {
        self.conflicting_role_levels
    }

    #[must_use]
    pub const fn independent_gex_timing(&self) -> TemporalStatus {
        self.independent_gex_timing
    }

    #[must_use]
    pub const fn hard_veto_verdict(&self) -> HardVetoVerdict {
        self.hard_veto_verdict
    }

    #[must_use]
    pub const fn failed_hard_conditions(&self) -> usize {
        self.failed_hard_conditions
    }

    #[must_use]
    pub const fn unknown_hard_conditions(&self) -> usize {
        self.unknown_hard_conditions
    }
}

impl ResearchReviewEvidencePacket {
    /// Project all reference and review statuses from the original packet;
    /// it intentionally has no consensus, aggregate score, or ALLOW output.
    #[must_use]
    pub fn diagnostic_summary(&self) -> ResearchReviewDiagnosticSummary {
        let mut references = ResearchReferenceStatusCounts::default();
        let mut unknown_claims = 0;
        for specialist in self.audit().specialists() {
            for reference in specialist.feature_references() {
                references.record(reference.status());
            }
            for claim in specialist
                .supporting()
                .iter()
                .chain(specialist.contradicting())
            {
                if claim.status() == ClaimObservationStatus::Unknown {
                    unknown_claims += 1;
                }
                for reference in claim.references() {
                    references.record(reference.status());
                }
            }
        }

        let mut unverified_role_levels = 0;
        let mut conflicting_role_levels = 0;
        for level in self.role_comparison().levels() {
            match level.status() {
                ResearchGexRoleObservationStatus::Unverified => unverified_role_levels += 1,
                ResearchGexRoleObservationStatus::Conflicting => conflicting_role_levels += 1,
            }
        }

        ResearchReviewDiagnosticSummary {
            reference_counts: references,
            unknown_claims,
            unverified_role_levels,
            conflicting_role_levels,
            independent_gex_timing: self.timing().gex_data(),
            hard_veto_verdict: self.hard_veto().verdict(),
            failed_hard_conditions: self.hard_veto().failed_conditions().len(),
            unknown_hard_conditions: self.hard_veto().unknown_conditions().len(),
        }
    }
}
