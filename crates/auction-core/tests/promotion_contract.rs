use auction_core::{
    Condition, PaperShadowMode, PromotionError, PromotionState, ReplayReport,
    StrategyChangePromotion, ValidationPhase,
};

fn report(phase: ValidationPhase) -> ReplayReport {
    ReplayReport {
        phase,
        total_cases: 10,
        exact_matches: 9,
        mismatches: 1,
        authorization_mismatches: 1,
        rejection_mismatches: 0,
        state_mismatches: 0,
    }
}

fn proposal() -> StrategyChangePromotion {
    StrategyChangePromotion::new(
        "CHANGE-34",
        "strategy-v1",
        "Evaluate one explicitly proposed strategy modification.",
    )
    .unwrap()
}

fn through_paper_shadow() -> StrategyChangePromotion {
    let mut promotion = proposal();
    promotion
        .record_backtest("BACKTEST-EVIDENCE", report(ValidationPhase::Backtest))
        .unwrap();
    promotion
        .record_out_of_sample("OOS-EVIDENCE", report(ValidationPhase::OutOfSample))
        .unwrap();
    promotion
        .record_paper_shadow("SHADOW-EVIDENCE", PaperShadowMode::Shadow)
        .unwrap();
    promotion
}

#[test]
fn section_116_requires_nonempty_change_identity_base_version_and_hypothesis() {
    assert_eq!(
        StrategyChangePromotion::new("", "strategy-v1", "hypothesis"),
        Err(PromotionError::MissingChangeId)
    );
    assert_eq!(
        StrategyChangePromotion::new("CHANGE", "", "hypothesis"),
        Err(PromotionError::MissingBaseVersion)
    );
    assert_eq!(
        StrategyChangePromotion::new("CHANGE", "strategy-v1", " "),
        Err(PromotionError::MissingHypothesis)
    );
}

#[test]
fn section_116_exact_sequence_reaches_version_recorded_without_live_activation() {
    let mut promotion = proposal();
    assert_eq!(promotion.state(), PromotionState::HypothesisRecorded);

    promotion
        .record_backtest("BACKTEST-EVIDENCE", report(ValidationPhase::Backtest))
        .unwrap();
    assert_eq!(promotion.state(), PromotionState::BacktestRecorded);
    assert_eq!(
        promotion.backtest_evidence_id(),
        Some("BACKTEST-EVIDENCE")
    );

    promotion
        .record_out_of_sample("OOS-EVIDENCE", report(ValidationPhase::OutOfSample))
        .unwrap();
    assert_eq!(promotion.state(), PromotionState::OutOfSampleRecorded);

    promotion
        .record_paper_shadow("PAPER-EVIDENCE", PaperShadowMode::Paper)
        .unwrap();
    assert_eq!(promotion.state(), PromotionState::PaperShadowRecorded);
    assert_eq!(promotion.paper_shadow_mode(), Some(PaperShadowMode::Paper));

    promotion.record_approval(Condition::True).unwrap();
    assert_eq!(promotion.state(), PromotionState::Approved);

    promotion.record_new_version("strategy-v2").unwrap();
    assert_eq!(promotion.state(), PromotionState::VersionRecorded);
    assert_eq!(promotion.new_strategy_version(), Some("strategy-v2"));
}

#[test]
fn section_116_stages_cannot_be_skipped_repeated_or_reordered() {
    let mut promotion = proposal();
    assert_eq!(
        promotion.record_out_of_sample("OOS", report(ValidationPhase::OutOfSample)),
        Err(PromotionError::WrongStage)
    );
    assert_eq!(
        promotion.record_paper_shadow("PAPER", PaperShadowMode::Paper),
        Err(PromotionError::WrongStage)
    );
    assert_eq!(
        promotion.record_approval(Condition::True),
        Err(PromotionError::WrongStage)
    );
    assert_eq!(
        promotion.record_new_version("strategy-v2"),
        Err(PromotionError::WrongStage)
    );

    promotion
        .record_backtest("BACKTEST", report(ValidationPhase::Backtest))
        .unwrap();
    assert_eq!(
        promotion.record_backtest("BACKTEST-2", report(ValidationPhase::Backtest)),
        Err(PromotionError::WrongStage)
    );
}

#[test]
fn section_116_validation_evidence_must_have_identity_phase_and_structural_integrity() {
    let mut promotion = proposal();
    assert_eq!(
        promotion.record_backtest("", report(ValidationPhase::Backtest)),
        Err(PromotionError::MissingEvidenceId)
    );
    assert_eq!(
        promotion.record_backtest("WRONG-PHASE", report(ValidationPhase::OutOfSample)),
        Err(PromotionError::WrongValidationPhase)
    );

    let malformed = ReplayReport {
        phase: ValidationPhase::Backtest,
        total_cases: 10,
        exact_matches: 10,
        mismatches: 1,
        authorization_mismatches: 0,
        rejection_mismatches: 0,
        state_mismatches: 0,
    };
    assert!(!malformed.structurally_valid());
    assert_eq!(
        promotion.record_backtest("MALFORMED", malformed),
        Err(PromotionError::InvalidReplayReport)
    );
    assert_eq!(promotion.state(), PromotionState::HypothesisRecorded);
}

#[test]
fn section_116_replay_quality_is_not_promoted_into_an_invented_threshold() {
    let poor_report = ReplayReport {
        phase: ValidationPhase::Backtest,
        total_cases: 10,
        exact_matches: 0,
        mismatches: 10,
        authorization_mismatches: 10,
        rejection_mismatches: 10,
        state_mismatches: 10,
    };
    assert!(poor_report.structurally_valid());

    let mut promotion = proposal();
    promotion.record_backtest("BACKTEST", poor_report).unwrap();
    assert_eq!(promotion.state(), PromotionState::BacktestRecorded);
}

#[test]
fn section_116_paper_shadow_is_opaque_evidence_not_a_simulated_broker() {
    let mut promotion = proposal();
    promotion
        .record_backtest("BACKTEST", report(ValidationPhase::Backtest))
        .unwrap();
    promotion
        .record_out_of_sample("OOS", report(ValidationPhase::OutOfSample))
        .unwrap();

    assert_eq!(
        promotion.record_paper_shadow("", PaperShadowMode::Shadow),
        Err(PromotionError::MissingEvidenceId)
    );
    promotion
        .record_paper_shadow("EXTERNAL-SHADOW-EVIDENCE", PaperShadowMode::Shadow)
        .unwrap();
    assert_eq!(
        promotion.paper_shadow_evidence_id(),
        Some("EXTERNAL-SHADOW-EVIDENCE")
    );
    assert_eq!(promotion.paper_shadow_mode(), Some(PaperShadowMode::Shadow));
}

#[test]
fn section_116_unknown_approval_cannot_advance_and_false_approval_rejects() {
    let mut unknown = through_paper_shadow();
    assert_eq!(
        unknown.record_approval(Condition::Unknown),
        Err(PromotionError::ApprovalUnknown)
    );
    assert_eq!(unknown.state(), PromotionState::PaperShadowRecorded);

    let mut rejected = through_paper_shadow();
    rejected.record_approval(Condition::False).unwrap();
    assert_eq!(rejected.state(), PromotionState::Rejected);
    assert_eq!(
        rejected.record_new_version("strategy-v2"),
        Err(PromotionError::ProposalRejected)
    );
}

#[test]
fn section_116_new_version_identity_is_external_nonempty_and_distinct() {
    let mut promotion = through_paper_shadow();
    promotion.record_approval(Condition::True).unwrap();

    assert_eq!(
        promotion.record_new_version(""),
        Err(PromotionError::MissingNewVersion)
    );
    assert_eq!(
        promotion.record_new_version("strategy-v1"),
        Err(PromotionError::NewVersionMatchesBase)
    );
    promotion.record_new_version("strategy-v2").unwrap();
    assert_eq!(promotion.state(), PromotionState::VersionRecorded);
    assert_eq!(
        promotion.record_new_version("strategy-v3"),
        Err(PromotionError::WrongStage)
    );
}
