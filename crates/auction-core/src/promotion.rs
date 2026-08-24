use crate::{Condition, ReplayReport, ValidationPhase};

/// Canonical §116 promotion sequence. These are evidence/process states only; none can activate
/// a live strategy version or modify production rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionState {
    HypothesisRecorded,
    BacktestRecorded,
    OutOfSampleRecorded,
    PaperShadowRecorded,
    Approved,
    Rejected,
    VersionRecorded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperShadowMode {
    Paper,
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionError {
    MissingChangeId,
    MissingBaseVersion,
    MissingHypothesis,
    MissingEvidenceId,
    WrongStage,
    WrongValidationPhase,
    InvalidReplayReport,
    ApprovalUnknown,
    ProposalRejected,
    MissingNewVersion,
    NewVersionMatchesBase,
}

/// Non-live strategy-change evidence coordinator for canonical §116.
///
/// The state machine proves ordering and evidence presence only. It does not decide whether a
/// replay result is sufficiently profitable, simulate paper/shadow execution, authenticate a
/// human approver, activate a version, or alter any production strategy value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyChangePromotion {
    change_id: String,
    base_strategy_version: String,
    hypothesis: String,
    state: PromotionState,
    backtest_evidence_id: Option<String>,
    out_of_sample_evidence_id: Option<String>,
    paper_shadow_evidence_id: Option<String>,
    paper_shadow_mode: Option<PaperShadowMode>,
    new_strategy_version: Option<String>,
}

impl StrategyChangePromotion {
    pub fn new(
        change_id: &str,
        base_strategy_version: &str,
        hypothesis: &str,
    ) -> Result<Self, PromotionError> {
        if change_id.trim().is_empty() {
            return Err(PromotionError::MissingChangeId);
        }
        if base_strategy_version.trim().is_empty() {
            return Err(PromotionError::MissingBaseVersion);
        }
        if hypothesis.trim().is_empty() {
            return Err(PromotionError::MissingHypothesis);
        }

        Ok(Self {
            change_id: change_id.to_owned(),
            base_strategy_version: base_strategy_version.to_owned(),
            hypothesis: hypothesis.to_owned(),
            state: PromotionState::HypothesisRecorded,
            backtest_evidence_id: None,
            out_of_sample_evidence_id: None,
            paper_shadow_evidence_id: None,
            paper_shadow_mode: None,
            new_strategy_version: None,
        })
    }

    #[must_use]
    pub fn change_id(&self) -> &str {
        &self.change_id
    }

    #[must_use]
    pub fn base_strategy_version(&self) -> &str {
        &self.base_strategy_version
    }

    #[must_use]
    pub fn hypothesis(&self) -> &str {
        &self.hypothesis
    }

    #[must_use]
    pub const fn state(&self) -> PromotionState {
        self.state
    }

    #[must_use]
    pub fn backtest_evidence_id(&self) -> Option<&str> {
        self.backtest_evidence_id.as_deref()
    }

    #[must_use]
    pub fn out_of_sample_evidence_id(&self) -> Option<&str> {
        self.out_of_sample_evidence_id.as_deref()
    }

    #[must_use]
    pub fn paper_shadow_evidence_id(&self) -> Option<&str> {
        self.paper_shadow_evidence_id.as_deref()
    }

    #[must_use]
    pub const fn paper_shadow_mode(&self) -> Option<PaperShadowMode> {
        self.paper_shadow_mode
    }

    #[must_use]
    pub fn new_strategy_version(&self) -> Option<&str> {
        self.new_strategy_version.as_deref()
    }

    /// Records that the required backtest stage was performed. Report quality is intentionally
    /// not converted into an automatic pass/fail threshold because §116 specifies none.
    pub fn record_backtest(
        &mut self,
        evidence_id: &str,
        report: ReplayReport,
    ) -> Result<(), PromotionError> {
        self.record_validation_stage(
            PromotionState::HypothesisRecorded,
            PromotionState::BacktestRecorded,
            ValidationPhase::Backtest,
            evidence_id,
            report,
        )?;
        self.backtest_evidence_id = Some(evidence_id.to_owned());
        Ok(())
    }

    /// Records that the required out-of-sample stage was performed after backtest evidence.
    pub fn record_out_of_sample(
        &mut self,
        evidence_id: &str,
        report: ReplayReport,
    ) -> Result<(), PromotionError> {
        self.record_validation_stage(
            PromotionState::BacktestRecorded,
            PromotionState::OutOfSampleRecorded,
            ValidationPhase::OutOfSample,
            evidence_id,
            report,
        )?;
        self.out_of_sample_evidence_id = Some(evidence_id.to_owned());
        Ok(())
    }

    /// Records opaque evidence that paper or shadow execution occurred. No provider, market-data,
    /// fill, slippage, or broker semantics are inferred here because canonical knowledge does not
    /// define them.
    pub fn record_paper_shadow(
        &mut self,
        evidence_id: &str,
        mode: PaperShadowMode,
    ) -> Result<(), PromotionError> {
        if self.state != PromotionState::OutOfSampleRecorded {
            return Err(PromotionError::WrongStage);
        }
        if evidence_id.trim().is_empty() {
            return Err(PromotionError::MissingEvidenceId);
        }
        self.paper_shadow_evidence_id = Some(evidence_id.to_owned());
        self.paper_shadow_mode = Some(mode);
        self.state = PromotionState::PaperShadowRecorded;
        Ok(())
    }

    /// Human approval is an externally supplied decision. UNKNOWN cannot advance. FALSE records
    /// a terminal rejection; TRUE records approval but still does not activate a strategy.
    pub fn record_approval(&mut self, approval: Condition) -> Result<(), PromotionError> {
        if self.state != PromotionState::PaperShadowRecorded {
            return Err(PromotionError::WrongStage);
        }
        match approval {
            Condition::True => {
                self.state = PromotionState::Approved;
                Ok(())
            }
            Condition::False => {
                self.state = PromotionState::Rejected;
                Ok(())
            }
            Condition::Unknown => Err(PromotionError::ApprovalUnknown),
        }
    }

    /// Final §116 bookkeeping step. The version identifier is externally supplied. This method
    /// records it only; there is intentionally no live-version activation API.
    pub fn record_new_version(&mut self, new_version: &str) -> Result<(), PromotionError> {
        if self.state == PromotionState::Rejected {
            return Err(PromotionError::ProposalRejected);
        }
        if self.state != PromotionState::Approved {
            return Err(PromotionError::WrongStage);
        }
        if new_version.trim().is_empty() {
            return Err(PromotionError::MissingNewVersion);
        }
        if new_version == self.base_strategy_version {
            return Err(PromotionError::NewVersionMatchesBase);
        }
        self.new_strategy_version = Some(new_version.to_owned());
        self.state = PromotionState::VersionRecorded;
        Ok(())
    }

    fn record_validation_stage(
        &mut self,
        required_state: PromotionState,
        next_state: PromotionState,
        required_phase: ValidationPhase,
        evidence_id: &str,
        report: ReplayReport,
    ) -> Result<(), PromotionError> {
        if self.state != required_state {
            return Err(PromotionError::WrongStage);
        }
        if evidence_id.trim().is_empty() {
            return Err(PromotionError::MissingEvidenceId);
        }
        if report.phase != required_phase {
            return Err(PromotionError::WrongValidationPhase);
        }
        if !report.structurally_valid() {
            return Err(PromotionError::InvalidReplayReport);
        }
        self.state = next_state;
        Ok(())
    }
}
