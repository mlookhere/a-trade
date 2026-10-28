use gex_strategy::{
    AdvisoryDirection, AiNativeFeatureInput, AiNativeFeatureSet, ClaimObservationStatus,
    DataQuality, EvidenceClaimInput, EvidenceKind, EvidenceReference, ExplicitAgeLimits,
    HardCondition, HardVetoInput, MarketSnapshot, MarketSnapshotInput, Playbook,
    ResearchAdversarialChallengeInput, ResearchAdversarialReview, ResearchAdversarialReviewError,
    ResearchBaselineObservationInput, ResearchChallengeClaimSide, ResearchDecisionProposal,
    ResearchDecisionProposalError, ResearchDecisionProposalInput, ResearchEvaluationPhase,
    ResearchGexRoleComparison, ResearchPairedEvaluationManifest, ResearchPairedManifestError,
    ResearchPairedReplayCase, ResearchPairedReplayError, ResearchReviewEvidencePacket, Session,
    SpecialistBatch, SpecialistOutput, SpecialistOutputInput, SpecialistRole, SpecialistStatus,
    assess_snapshot_timing, audit_specialist_evidence, evaluate_hard_vetoes,
    summarize_research_pairs,
};

fn snapshot(id: &str, spot: f64) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: spot,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn packet(s: &MarketSnapshot, strength: Option<f64>) -> ResearchReviewEvidencePacket {
    let mut feature_input = AiNativeFeatureInput {
        snapshot_id: s.snapshot_id().to_owned(),
        ..Default::default()
    };
    feature_input.trend.trend_strength = strength;
    let features = AiNativeFeatureSet::build(s, feature_input).unwrap();
    let specialist = SpecialistOutput::build(
        s,
        SpecialistOutputInput {
            agent_id: "trend-specialist".to_owned(),
            role: SpecialistRole::TrendMarketState,
            snapshot_id: s.snapshot_id().to_owned(),
            status: SpecialistStatus::Ok,
            direction: AdvisoryDirection::Neutral,
            confidence: None,
            observations: vec![],
            features: vec![],
            supporting_evidence: vec![EvidenceClaimInput {
                claim: "Spot may inform a trend".to_owned(),
                evidence: vec![EvidenceReference {
                    kind: EvidenceKind::SnapshotField,
                    reference: "spot_price".to_owned(),
                }],
                confidence: None,
            }],
            contradicting_evidence: vec![EvidenceClaimInput {
                claim: "External catalyst may invalidate a trend".to_owned(),
                evidence: vec![EvidenceReference {
                    kind: EvidenceKind::Catalyst,
                    reference: "unverified-external".to_owned(),
                }],
                confidence: None,
            }],
            unknowns: vec![],
            risks: vec![],
            proposed_invalidation: vec![],
            hard_veto: false,
            reason_codes: vec![],
        },
    )
    .unwrap();
    let batch = SpecialistBatch::build(s, vec![specialist]).unwrap();
    let audit = audit_specialist_evidence(s, &features, &batch).unwrap();
    let roles = ResearchGexRoleComparison::build(s, vec![]).unwrap();
    let limits = ExplicitAgeLimits {
        market_data_stale_timeout_ms: 20,
        gex_stale_timeout_ms: 20,
        max_decision_age_ms: 10,
    };
    let timing = assess_snapshot_timing(s, None, 1_020, &limits);
    let veto = evaluate_hard_vetoes(
        s,
        &HardVetoInput {
            snapshot_id: s.snapshot_id().to_owned(),
            data_valid: HardCondition::Unknown,
            data_fresh: HardCondition::Unknown,
            gex_scope_valid: HardCondition::Unknown,
            required_regime_data_valid: HardCondition::Unknown,
            broker_connected: HardCondition::Unknown,
            order_state_valid: HardCondition::Unknown,
            instrument_tradable: HardCondition::Unknown,
            liquidity_within_limits: HardCondition::Unknown,
            max_position_risk_valid: HardCondition::Unknown,
            max_portfolio_risk_valid: HardCondition::Unknown,
            invalidation_defined: HardCondition::Unknown,
            execution_cost_within_limits: HardCondition::Unknown,
            kill_switch_clear: HardCondition::Unknown,
        },
    )
    .unwrap();
    ResearchReviewEvidencePacket::build(s, audit, roles, timing, veto).unwrap()
}

fn challenge(
    s: &MarketSnapshot,
    side: ResearchChallengeClaimSide,
) -> ResearchAdversarialChallengeInput {
    ResearchAdversarialChallengeInput {
        snapshot_id: s.snapshot_id().to_owned(),
        reviewer_id: "adversarial-reviewer".to_owned(),
        target_agent_id: "trend-specialist".to_owned(),
        claim_side: side,
        claim_index: 0,
        rationale: "The referenced data do not prove the narrative".to_owned(),
    }
}

#[test]
fn exact_claims_and_unresolved_evidence_remain_observational() {
    let s = snapshot("review-1", 600.0);
    let p = packet(&s, None);
    let review = ResearchAdversarialReview::build(
        &s,
        &p,
        vec![
            challenge(&s, ResearchChallengeClaimSide::Supporting),
            challenge(&s, ResearchChallengeClaimSide::Contradicting),
        ],
    )
    .unwrap();
    assert_eq!(review.source_packet(), &p);
    assert_eq!(review.validate_inputs(&s, &p), Ok(()));
    assert_eq!(review.challenges().len(), 2);
    let first = &review.challenges()[0];
    assert_eq!(first.reviewer_id(), "adversarial-reviewer");
    assert_eq!(first.target_agent_id(), "trend-specialist");
    assert_eq!(first.claim_side(), ResearchChallengeClaimSide::Supporting);
    assert_eq!(first.claim_index(), 0);
    assert_eq!(first.target_claim(), "Spot may inform a trend");
    assert_eq!(
        first.observed_status(),
        ClaimObservationStatus::ObservedReferencesOnly
    );
    assert!(!first.rationale().is_empty());
    let second = &review.challenges()[1];
    assert_eq!(
        second.claim_side(),
        ResearchChallengeClaimSide::Contradicting
    );
    assert_eq!(second.observed_status(), ClaimObservationStatus::Unknown);
    // An objection cannot turn a cleared or a missing source into trade authorization.
    assert!(p.timing().source_gex().is_none());
}

#[test]
fn zero_challenges_does_not_imply_a_favorable_review() {
    let s = snapshot("review-2", 600.0);
    let p = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &p, vec![]).unwrap();
    assert!(review.challenges().is_empty());
    assert_eq!(review.validate_inputs(&s, &p), Ok(()));
}

#[test]
fn invalid_or_duplicate_claim_targets_reject_without_inferred_matches() {
    let s = snapshot("review-3", 600.0);
    let p = packet(&s, None);
    let valid = challenge(&s, ResearchChallengeClaimSide::Supporting);

    let mut no_snapshot = valid.clone();
    no_snapshot.snapshot_id = " ".to_owned();
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![no_snapshot]),
        Err(ResearchAdversarialReviewError::MissingSnapshotId)
    );
    let mut wrong_id = valid.clone();
    wrong_id.snapshot_id = "other".to_owned();
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![wrong_id]),
        Err(ResearchAdversarialReviewError::SnapshotMismatch)
    );
    let mut no_reviewer = valid.clone();
    no_reviewer.reviewer_id = " ".to_owned();
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![no_reviewer]),
        Err(ResearchAdversarialReviewError::MissingReviewerId)
    );
    let mut no_target = valid.clone();
    no_target.target_agent_id = "".to_owned();
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![no_target]),
        Err(ResearchAdversarialReviewError::MissingTargetAgentId)
    );
    let mut no_reason = valid.clone();
    no_reason.rationale = " ".to_owned();
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![no_reason]),
        Err(ResearchAdversarialReviewError::BlankRationale)
    );
    let mut missing_agent = valid.clone();
    missing_agent.target_agent_id = "not-in-this-batch".to_owned();
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![missing_agent]),
        Err(ResearchAdversarialReviewError::UnknownTargetAgent)
    );
    let mut wrong_claim = valid.clone();
    wrong_claim.claim_index = 1;
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![wrong_claim]),
        Err(ResearchAdversarialReviewError::InvalidClaimIndex)
    );
    assert_eq!(
        ResearchAdversarialReview::build(&s, &p, vec![valid.clone(), valid]),
        Err(ResearchAdversarialReviewError::DuplicateChallenge)
    );
}

#[test]
fn same_id_revisions_and_swapped_packet_contents_reject() {
    let s = snapshot("review-4", 600.0);
    let p = packet(&s, None);
    let review = ResearchAdversarialReview::build(
        &s,
        &p,
        vec![challenge(&s, ResearchChallengeClaimSide::Supporting)],
    )
    .unwrap();
    let revised = snapshot("review-4", 601.0);
    let foreign = snapshot("other", 600.0);

    assert_eq!(
        ResearchAdversarialReview::build(&revised, &p, vec![]),
        Err(ResearchAdversarialReviewError::SnapshotContentsMismatch)
    );
    assert_eq!(
        review.validate_inputs(&revised, &p),
        Err(ResearchAdversarialReviewError::SnapshotContentsMismatch)
    );
    assert_eq!(
        review.validate_inputs(&foreign, &p),
        Err(ResearchAdversarialReviewError::SnapshotMismatch)
    );
    assert_eq!(
        review.validate_inputs(&s, &packet(&s, Some(2.0))),
        Err(ResearchAdversarialReviewError::PacketContentsMismatch)
    );
    assert_eq!(review.validate_inputs(&s, &p), Ok(()));
}

#[test]
fn independent_reviewers_preserve_separate_objections_without_consensus() {
    let s = snapshot("two-reviewers", 600.0);
    let packet = packet(&s, None);
    let first = challenge(&s, ResearchChallengeClaimSide::Supporting);
    let mut second = first.clone();
    second.reviewer_id = "reviewer-b".to_owned();
    second.rationale = "Source is present but not sufficient for the narrative".to_owned();

    let review = ResearchAdversarialReview::build(&s, &packet, vec![first, second]).unwrap();
    assert_eq!(review.challenges().len(), 2);
    assert_eq!(review.challenges()[0].reviewer_id(), "adversarial-reviewer");
    assert_eq!(review.challenges()[1].reviewer_id(), "reviewer-b");
    assert_eq!(
        review.challenges()[0].target_claim(),
        review.challenges()[1].target_claim()
    );
    assert_eq!(
        review.challenges()[1].observed_status(),
        ClaimObservationStatus::ObservedReferencesOnly
    );
    assert_eq!(review.validate_inputs(&s, &packet), Ok(()));
}

#[test]
fn same_id_timestamp_and_quality_revisions_cannot_reuse_original_review() {
    let s = snapshot("same-id", 600.0);
    let packet = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &packet, vec![]).unwrap();

    for input in [
        MarketSnapshotInput {
            snapshot_id: "same-id".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 999,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        MarketSnapshotInput {
            snapshot_id: "same-id".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_011,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        MarketSnapshotInput {
            snapshot_id: "same-id".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Unknown,
        },
    ] {
        let altered = MarketSnapshot::build(input, &[]).unwrap();
        assert_ne!(altered, s);
        assert_eq!(
            review.validate_inputs(&altered, &packet),
            Err(ResearchAdversarialReviewError::SnapshotContentsMismatch)
        );
    }
}

fn proposal_input(s: &MarketSnapshot) -> ResearchDecisionProposalInput {
    ResearchDecisionProposalInput {
        snapshot_id: s.snapshot_id().to_owned(),
        proposer_id: "research-proposer".to_owned(),
        playbook: Playbook::None,
        direction: AdvisoryDirection::Unknown,
        rationale: "Unverified external hypothesis requiring independent validation".to_owned(),
    }
}

#[test]
fn proposal_preserves_unknown_playbook_direction_and_all_original_review_witnesses() {
    let s = snapshot("proposal-1", 600.0);
    let p = packet(&s, None);
    let review = ResearchAdversarialReview::build(
        &s,
        &p,
        vec![challenge(&s, ResearchChallengeClaimSide::Supporting)],
    )
    .unwrap();
    let input = proposal_input(&s);
    let proposal = ResearchDecisionProposal::build(&s, &review, input.clone()).unwrap();

    assert_eq!(proposal.source_snapshot(), &s);
    assert_eq!(proposal.source_review(), &review);
    assert_eq!(proposal.source_input(), &input);
    assert_eq!(proposal.source_input().playbook, Playbook::None);
    assert_eq!(
        proposal.source_input().direction,
        AdvisoryDirection::Unknown
    );
    assert_eq!(proposal.source_review().challenges().len(), 1);
    assert_eq!(proposal.validate_inputs(&s, &review, &input), Ok(()));
    assert_eq!(
        proposal
            .source_review()
            .source_packet()
            .timing()
            .source_gex(),
        None
    );
}

#[test]
fn even_explicit_playbook_and_long_direction_are_only_hypotheses() {
    let s = snapshot("proposal-2", 600.0);
    let p = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &p, vec![]).unwrap();
    let mut input = proposal_input(&s);
    input.playbook = Playbook::PositiveGammaMagnet;
    input.direction = AdvisoryDirection::Long;
    let proposal = ResearchDecisionProposal::build(&s, &review, input.clone()).unwrap();
    assert_eq!(
        proposal.source_input().playbook,
        Playbook::PositiveGammaMagnet
    );
    assert_eq!(proposal.source_input().direction, AdvisoryDirection::Long);
    assert!(proposal.source_review().challenges().is_empty());
    assert_eq!(proposal.validate_inputs(&s, &review, &input), Ok(()));
    assert_eq!(
        p.hard_veto().blocking_decision(),
        Some(gex_strategy::DecisionVerdict::Wait)
    );
}

#[test]
fn blank_or_foreign_proposals_and_changed_fields_reject_without_reinterpretation() {
    let s = snapshot("proposal-3", 600.0);
    let p = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &p, vec![]).unwrap();
    let input = proposal_input(&s);

    let mut missing = input.clone();
    missing.snapshot_id = " ".to_owned();
    assert_eq!(
        ResearchDecisionProposal::build(&s, &review, missing),
        Err(ResearchDecisionProposalError::MissingSnapshotId)
    );
    let mut wrong = input.clone();
    wrong.snapshot_id = "other".to_owned();
    assert_eq!(
        ResearchDecisionProposal::build(&s, &review, wrong),
        Err(ResearchDecisionProposalError::SnapshotMismatch)
    );
    let mut no_proposer = input.clone();
    no_proposer.proposer_id = " ".to_owned();
    assert_eq!(
        ResearchDecisionProposal::build(&s, &review, no_proposer),
        Err(ResearchDecisionProposalError::MissingProposerId)
    );
    let mut no_reason = input.clone();
    no_reason.rationale = " ".to_owned();
    assert_eq!(
        ResearchDecisionProposal::build(&s, &review, no_reason),
        Err(ResearchDecisionProposalError::BlankRationale)
    );

    let proposal = ResearchDecisionProposal::build(&s, &review, input.clone()).unwrap();
    let mut changed_playbook = input.clone();
    changed_playbook.playbook = Playbook::NegativeGammaExpansion;
    assert_eq!(
        proposal.validate_inputs(&s, &review, &changed_playbook),
        Err(ResearchDecisionProposalError::InputChanged)
    );
    let mut changed_direction = input.clone();
    changed_direction.direction = AdvisoryDirection::Short;
    assert_eq!(
        proposal.validate_inputs(&s, &review, &changed_direction),
        Err(ResearchDecisionProposalError::InputChanged)
    );
    let mut changed_rationale = input.clone();
    changed_rationale.rationale = "Other hypothesis".to_owned();
    assert_eq!(
        proposal.validate_inputs(&s, &review, &changed_rationale),
        Err(ResearchDecisionProposalError::InputChanged)
    );
    let mut changed_id = input.clone();
    changed_id.snapshot_id = "other".to_owned();
    assert_eq!(
        proposal.validate_inputs(&s, &review, &changed_id),
        Err(ResearchDecisionProposalError::SnapshotMismatch)
    );
}

#[test]
fn proposal_never_reuses_changed_market_or_review_packet_contents() {
    let s = snapshot("proposal-4", 600.0);
    let p = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &p, vec![]).unwrap();
    let input = proposal_input(&s);
    let proposal = ResearchDecisionProposal::build(&s, &review, input.clone()).unwrap();

    let revised = snapshot("proposal-4", 601.0);
    assert_eq!(
        ResearchDecisionProposal::build(&revised, &review, input.clone()),
        Err(ResearchDecisionProposalError::SnapshotContentsMismatch)
    );
    assert_eq!(
        proposal.validate_inputs(&revised, &review, &input),
        Err(ResearchDecisionProposalError::SnapshotContentsMismatch)
    );

    for modified in [
        MarketSnapshotInput {
            snapshot_id: "proposal-4".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 999,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        MarketSnapshotInput {
            snapshot_id: "proposal-4".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Unknown,
        },
    ] {
        let altered = MarketSnapshot::build(modified, &[]).unwrap();
        assert_eq!(
            proposal.validate_inputs(&altered, &review, &input),
            Err(ResearchDecisionProposalError::SnapshotContentsMismatch)
        );
    }

    let foreign = snapshot("other", 600.0);
    assert_eq!(
        proposal.validate_inputs(&foreign, &review, &input),
        Err(ResearchDecisionProposalError::SnapshotMismatch)
    );
    let updated_packet = packet(&s, Some(3.0));
    let swapped_review = ResearchAdversarialReview::build(&s, &updated_packet, vec![]).unwrap();
    assert_eq!(
        proposal.validate_inputs(&s, &swapped_review, &input),
        Err(ResearchDecisionProposalError::ReviewChanged)
    );
    let challenged = ResearchAdversarialReview::build(
        &s,
        &p,
        vec![challenge(&s, ResearchChallengeClaimSide::Supporting)],
    )
    .unwrap();
    assert_eq!(
        proposal.validate_inputs(&s, &challenged, &input),
        Err(ResearchDecisionProposalError::ReviewChanged)
    );
}

fn evaluation_manifest() -> ResearchPairedEvaluationManifest {
    ResearchPairedEvaluationManifest {
        experiment_id: "offline-study-1".to_owned(),
        strict_baseline_version: "source-baseline-unverified".to_owned(),
        ai_native_version: "gex-research-unverified".to_owned(),
        backtest_session_ids: vec!["2026-01-05".to_owned()],
        out_of_sample_session_ids: vec!["2026-02-05".to_owned()],
    }
}

fn explicit_baseline(
    s: &MarketSnapshot,
    direction: AdvisoryDirection,
) -> ResearchBaselineObservationInput {
    ResearchBaselineObservationInput {
        snapshot_id: s.snapshot_id().to_owned(),
        source_record_id: "external-baseline-record".to_owned(),
        direction,
        rationale: "Unverified baseline annotation, not a strategy computation".to_owned(),
    }
}

fn paired_case(
    manifest: &ResearchPairedEvaluationManifest,
    case_id: &str,
    session_id: &str,
    s: &MarketSnapshot,
    baseline_direction: AdvisoryDirection,
    proposal_direction: AdvisoryDirection,
) -> ResearchPairedReplayCase {
    let packet = packet(s, None);
    let review = ResearchAdversarialReview::build(s, &packet, vec![]).unwrap();
    let mut input = proposal_input(s);
    input.direction = proposal_direction;
    let proposal = ResearchDecisionProposal::build(s, &review, input).unwrap();
    ResearchPairedReplayCase::record(
        manifest,
        case_id,
        session_id,
        s,
        explicit_baseline(s, baseline_direction),
        &proposal,
    )
    .unwrap()
}

#[test]
fn paired_manifest_rejects_missing_duplicate_and_leaking_sessions() {
    let manifest = evaluation_manifest();
    assert_eq!(manifest.validate(), Ok(()));
    assert_eq!(
        manifest.phase_for("2026-01-05"),
        Some(ResearchEvaluationPhase::Backtest)
    );
    assert_eq!(
        manifest.phase_for("2026-02-05"),
        Some(ResearchEvaluationPhase::OutOfSample)
    );
    assert_eq!(manifest.phase_for("unlisted"), None);

    let mut missing_id = manifest.clone();
    missing_id.experiment_id = " ".to_owned();
    assert_eq!(
        missing_id.validate(),
        Err(ResearchPairedManifestError::MissingExperimentId)
    );
    let mut missing_baseline = manifest.clone();
    missing_baseline.strict_baseline_version = "".to_owned();
    assert_eq!(
        missing_baseline.validate(),
        Err(ResearchPairedManifestError::MissingBaselineVersion)
    );
    let mut missing_native = manifest.clone();
    missing_native.ai_native_version = " ".to_owned();
    assert_eq!(
        missing_native.validate(),
        Err(ResearchPairedManifestError::MissingAiNativeVersion)
    );
    let mut empty_backtest = manifest.clone();
    empty_backtest.backtest_session_ids.clear();
    assert_eq!(
        empty_backtest.validate(),
        Err(ResearchPairedManifestError::MissingBacktestSessions)
    );
    let mut empty_oos = manifest.clone();
    empty_oos.out_of_sample_session_ids.clear();
    assert_eq!(
        empty_oos.validate(),
        Err(ResearchPairedManifestError::MissingOutOfSampleSessions)
    );
    let mut blank = manifest.clone();
    blank.backtest_session_ids.push(" ".to_owned());
    assert_eq!(
        blank.validate(),
        Err(ResearchPairedManifestError::BlankSessionId)
    );
    let mut duplicate = manifest.clone();
    duplicate.backtest_session_ids.push("2026-01-05".to_owned());
    assert_eq!(
        duplicate.validate(),
        Err(ResearchPairedManifestError::DuplicateSessionId)
    );
    let mut overlap = manifest;
    overlap
        .out_of_sample_session_ids
        .push("2026-01-05".to_owned());
    assert_eq!(
        overlap.validate(),
        Err(ResearchPairedManifestError::SplitOverlap)
    );
}

#[test]
fn paired_replay_preserves_exact_sources_and_keeps_unknown_direction_out_of_agreement() {
    let manifest = evaluation_manifest();
    let s = snapshot("paired-source", 600.0);
    let unknown = paired_case(
        &manifest,
        "case-unknown",
        "2026-01-05",
        &s,
        AdvisoryDirection::Unknown,
        AdvisoryDirection::Long,
    );
    assert_eq!(unknown.source_snapshot(), &s);
    assert_eq!(unknown.case_id(), "case-unknown");
    assert_eq!(unknown.session_id(), "2026-01-05");
    assert_eq!(unknown.phase(), ResearchEvaluationPhase::Backtest);
    assert_eq!(unknown.baseline().direction, AdvisoryDirection::Unknown);
    assert_eq!(
        unknown.ai_native().source_input().direction,
        AdvisoryDirection::Long
    );

    let same = paired_case(
        &manifest,
        "case-same",
        "2026-01-05",
        &snapshot("different-snapshot", 600.0),
        AdvisoryDirection::Long,
        AdvisoryDirection::Long,
    );
    let different = paired_case(
        &manifest,
        "case-different",
        "2026-01-05",
        &snapshot("third-snapshot", 600.0),
        AdvisoryDirection::Short,
        AdvisoryDirection::Neutral,
    );
    let report = summarize_research_pairs(
        &manifest,
        ResearchEvaluationPhase::Backtest,
        &[unknown, same, different],
    )
    .unwrap();
    assert_eq!(report.total_cases(), 3);
    assert_eq!(report.same_known_direction(), 1);
    assert_eq!(report.different_known_direction(), 1);
    assert_eq!(report.unknown_direction(), 1);
}

#[test]
fn paired_replay_rejects_unlisted_sessions_blank_baseline_evidence_and_changed_sources() {
    let manifest = evaluation_manifest();
    let s = snapshot("paired-validate", 600.0);
    let packet = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &packet, vec![]).unwrap();
    let proposal = ResearchDecisionProposal::build(&s, &review, proposal_input(&s)).unwrap();
    let baseline = explicit_baseline(&s, AdvisoryDirection::Neutral);

    assert_eq!(
        ResearchPairedReplayCase::record(
            &manifest,
            "case",
            "not-listed",
            &s,
            baseline.clone(),
            &proposal
        ),
        Err(ResearchPairedReplayError::UnknownSession)
    );
    assert_eq!(
        ResearchPairedReplayCase::record(
            &manifest,
            "",
            "2026-01-05",
            &s,
            baseline.clone(),
            &proposal
        ),
        Err(ResearchPairedReplayError::MissingCaseId)
    );
    let mut no_record = baseline.clone();
    no_record.source_record_id = " ".to_owned();
    assert_eq!(
        ResearchPairedReplayCase::record(&manifest, "case", "2026-01-05", &s, no_record, &proposal),
        Err(ResearchPairedReplayError::MissingBaselineRecordId)
    );
    let mut no_rationale = baseline.clone();
    no_rationale.rationale = " ".to_owned();
    assert_eq!(
        ResearchPairedReplayCase::record(
            &manifest,
            "case",
            "2026-01-05",
            &s,
            no_rationale,
            &proposal
        ),
        Err(ResearchPairedReplayError::BlankBaselineRationale)
    );
    let mut wrong_source = baseline.clone();
    wrong_source.snapshot_id = "foreign".to_owned();
    assert_eq!(
        ResearchPairedReplayCase::record(
            &manifest,
            "case",
            "2026-01-05",
            &s,
            wrong_source,
            &proposal
        ),
        Err(ResearchPairedReplayError::BaselineSnapshotMismatch)
    );
    let changed = snapshot("paired-validate", 601.0);
    assert_eq!(
        ResearchPairedReplayCase::record(
            &manifest,
            "case",
            "2026-01-05",
            &changed,
            baseline.clone(),
            &proposal
        ),
        Err(ResearchPairedReplayError::SnapshotContentsMismatch)
    );
    let foreign = snapshot("foreign", 600.0);
    assert_eq!(
        ResearchPairedReplayCase::record(
            &manifest,
            "case",
            "2026-01-05",
            &foreign,
            baseline,
            &proposal
        ),
        Err(ResearchPairedReplayError::BaselineSnapshotMismatch)
    );
}

#[test]
fn paired_summary_rejects_duplicate_case_and_snapshot_keys_mixed_phases_and_manifest() {
    let manifest = evaluation_manifest();
    let a = paired_case(
        &manifest,
        "one",
        "2026-01-05",
        &snapshot("same-snapshot", 600.0),
        AdvisoryDirection::Long,
        AdvisoryDirection::Long,
    );
    let b = paired_case(
        &manifest,
        "two",
        "2026-01-05",
        &snapshot("same-snapshot", 600.0),
        AdvisoryDirection::Short,
        AdvisoryDirection::Short,
    );
    let same_case_id = paired_case(
        &manifest,
        "one",
        "2026-01-05",
        &snapshot("different", 600.0),
        AdvisoryDirection::Short,
        AdvisoryDirection::Long,
    );
    assert_eq!(
        summarize_research_pairs(&manifest, ResearchEvaluationPhase::Backtest, &[]),
        Err(ResearchPairedReplayError::EmptyCases)
    );
    assert_eq!(
        summarize_research_pairs(
            &manifest,
            ResearchEvaluationPhase::Backtest,
            &[a.clone(), b]
        ),
        Err(ResearchPairedReplayError::DuplicateSessionSnapshot)
    );
    assert_eq!(
        summarize_research_pairs(
            &manifest,
            ResearchEvaluationPhase::Backtest,
            &[a.clone(), same_case_id]
        ),
        Err(ResearchPairedReplayError::DuplicateCaseId)
    );
    let oos = paired_case(
        &manifest,
        "out",
        "2026-02-05",
        &snapshot("other", 600.0),
        AdvisoryDirection::Unknown,
        AdvisoryDirection::Unknown,
    );
    assert_eq!(
        summarize_research_pairs(
            &manifest,
            ResearchEvaluationPhase::Backtest,
            &[a.clone(), oos]
        ),
        Err(ResearchPairedReplayError::MixedPhases)
    );
    let mut altered_manifest = manifest.clone();
    altered_manifest.ai_native_version = "other-version".to_owned();
    assert_eq!(
        summarize_research_pairs(&altered_manifest, ResearchEvaluationPhase::Backtest, &[a]),
        Err(ResearchPairedReplayError::ManifestChanged)
    );
}

#[test]
fn paired_record_rejects_same_id_timestamp_and_quality_substitutions() {
    let manifest = evaluation_manifest();
    let s = snapshot("paired-revised", 600.0);
    let original_packet = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &original_packet, vec![]).unwrap();
    let proposal = ResearchDecisionProposal::build(&s, &review, proposal_input(&s)).unwrap();
    let baseline = explicit_baseline(&s, AdvisoryDirection::Unknown);

    let mut missing_baseline_id = baseline.clone();
    missing_baseline_id.snapshot_id = " ".to_owned();
    assert_eq!(
        ResearchPairedReplayCase::record(
            &manifest,
            "blank-baseline",
            "2026-01-05",
            &s,
            missing_baseline_id,
            &proposal
        ),
        Err(ResearchPairedReplayError::MissingBaselineSnapshotId)
    );

    for input in [
        MarketSnapshotInput {
            snapshot_id: "paired-revised".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 999,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        MarketSnapshotInput {
            snapshot_id: "paired-revised".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_011,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        MarketSnapshotInput {
            snapshot_id: "paired-revised".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Unknown,
        },
    ] {
        let changed = MarketSnapshot::build(input, &[]).unwrap();
        assert_eq!(changed.snapshot_id(), s.snapshot_id());
        assert_ne!(changed, s);
        assert_eq!(
            ResearchPairedReplayCase::record(
                &manifest,
                "revised-case",
                "2026-01-05",
                &changed,
                baseline.clone(),
                &proposal
            ),
            Err(ResearchPairedReplayError::SnapshotContentsMismatch)
        );
    }
}

#[test]
fn out_of_sample_cases_remain_separate_with_unknown_direction_preserved() {
    let manifest = evaluation_manifest();
    let source = snapshot("paired-oos", 600.0);
    let oos = paired_case(
        &manifest,
        "oos-only",
        "2026-02-05",
        &source,
        AdvisoryDirection::Long,
        AdvisoryDirection::Unknown,
    );
    let report =
        summarize_research_pairs(&manifest, ResearchEvaluationPhase::OutOfSample, &[oos]).unwrap();
    assert_eq!(report.phase(), ResearchEvaluationPhase::OutOfSample);
    assert_eq!(report.total_cases(), 1);
    assert_eq!(report.same_known_direction(), 0);
    assert_eq!(report.different_known_direction(), 0);
    assert_eq!(report.unknown_direction(), 1);
}

#[test]
fn stored_paired_replay_revalidates_original_manifest_baseline_and_proposal() {
    let manifest = evaluation_manifest();
    let s = snapshot("revalidate-oos", 600.0);
    let packet = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &packet, vec![]).unwrap();
    let input = proposal_input(&s);
    let proposal = ResearchDecisionProposal::build(&s, &review, input.clone()).unwrap();
    let baseline = explicit_baseline(&s, AdvisoryDirection::Unknown);
    let saved = ResearchPairedReplayCase::record(
        &manifest,
        "stored-oos-case",
        "2026-02-05",
        &s,
        baseline.clone(),
        &proposal,
    )
    .unwrap();

    assert_eq!(saved.phase(), ResearchEvaluationPhase::OutOfSample);
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &baseline, &proposal),
        Ok(())
    );
    assert_eq!(saved.baseline().direction, AdvisoryDirection::Unknown);
    assert_eq!(saved.ai_native().source_input().direction, AdvisoryDirection::Unknown);

    let mut new_version = manifest.clone();
    new_version.ai_native_version = "unapproved-version-change".to_owned();
    assert_eq!(
        saved.validate_inputs(&new_version, &s, &baseline, &proposal),
        Err(ResearchPairedReplayError::ManifestChanged)
    );
    let mut changed_split = manifest.clone();
    changed_split.out_of_sample_session_ids.push("2026-02-06".to_owned());
    assert_eq!(
        saved.validate_inputs(&changed_split, &s, &baseline, &proposal),
        Err(ResearchPairedReplayError::ManifestChanged)
    );

    let mut different_baseline = baseline.clone();
    different_baseline.direction = AdvisoryDirection::Long;
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &different_baseline, &proposal),
        Err(ResearchPairedReplayError::BaselineChanged)
    );
    let mut different_source_record = baseline.clone();
    different_source_record.source_record_id = "swapped-record".to_owned();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &different_source_record, &proposal),
        Err(ResearchPairedReplayError::BaselineChanged)
    );
    let mut different_rationale = baseline.clone();
    different_rationale.rationale = "new baseline explanation".to_owned();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &different_rationale, &proposal),
        Err(ResearchPairedReplayError::BaselineChanged)
    );
    let mut missing_source_record = baseline.clone();
    missing_source_record.source_record_id = " ".to_owned();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &missing_source_record, &proposal),
        Err(ResearchPairedReplayError::MissingBaselineRecordId)
    );
    let mut missing_snapshot = baseline.clone();
    missing_snapshot.snapshot_id = " ".to_owned();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &missing_snapshot, &proposal),
        Err(ResearchPairedReplayError::MissingBaselineSnapshotId)
    );
    let mut foreign_baseline = baseline.clone();
    foreign_baseline.snapshot_id = "foreign".to_owned();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &foreign_baseline, &proposal),
        Err(ResearchPairedReplayError::BaselineSnapshotMismatch)
    );

    let mut different_proposal_input = input.clone();
    different_proposal_input.direction = AdvisoryDirection::Short;
    let different_proposal =
        ResearchDecisionProposal::build(&s, &review, different_proposal_input).unwrap();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &baseline, &different_proposal),
        Err(ResearchPairedReplayError::ProposalChanged)
    );
    let different_review = ResearchAdversarialReview::build(
        &s,
        &packet,
        vec![challenge(&s, ResearchChallengeClaimSide::Supporting)],
    )
    .unwrap();
    let changed_review_proposal =
        ResearchDecisionProposal::build(&s, &different_review, input).unwrap();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &baseline, &changed_review_proposal),
        Err(ResearchPairedReplayError::ProposalChanged)
    );
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &baseline, &proposal),
        Ok(())
    );
}

#[test]
fn stored_paired_replay_rejects_same_id_source_revisions_and_foreign_proposals() {
    let manifest = evaluation_manifest();
    let s = snapshot("revalidate-source", 600.0);
    let source_packet = packet(&s, None);
    let review = ResearchAdversarialReview::build(&s, &source_packet, vec![]).unwrap();
    let proposal = ResearchDecisionProposal::build(&s, &review, proposal_input(&s)).unwrap();
    let baseline = explicit_baseline(&s, AdvisoryDirection::Long);
    let saved = ResearchPairedReplayCase::record(
        &manifest,
        "revalidate-backtest",
        "2026-01-05",
        &s,
        baseline.clone(),
        &proposal,
    )
    .unwrap();

    let foreign = snapshot("different-source", 600.0);
    assert_eq!(
        saved.validate_inputs(&manifest, &foreign, &baseline, &proposal),
        Err(ResearchPairedReplayError::SnapshotMismatch)
    );
    let changed_spot = snapshot("revalidate-source", 601.0);
    assert_eq!(
        saved.validate_inputs(&manifest, &changed_spot, &baseline, &proposal),
        Err(ResearchPairedReplayError::SnapshotContentsMismatch)
    );
    for input in [
        MarketSnapshotInput {
            snapshot_id: "revalidate-source".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 999,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        MarketSnapshotInput {
            snapshot_id: "revalidate-source".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_011,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        MarketSnapshotInput {
            snapshot_id: "revalidate-source".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Unknown,
        },
    ] {
        let changed = MarketSnapshot::build(input, &[]).unwrap();
        assert_eq!(
            saved.validate_inputs(&manifest, &changed, &baseline, &proposal),
            Err(ResearchPairedReplayError::SnapshotContentsMismatch)
        );
    }
    let foreign_packet = packet(&foreign, None);
    let foreign_review =
        ResearchAdversarialReview::build(&foreign, &foreign_packet, vec![]).unwrap();
    let foreign_proposal =
        ResearchDecisionProposal::build(&foreign, &foreign_review, proposal_input(&foreign))
            .unwrap();
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &baseline, &foreign_proposal),
        Err(ResearchPairedReplayError::ProposalSnapshotMismatch)
    );
    assert_eq!(
        saved.validate_inputs(&manifest, &s, &baseline, &proposal),
        Ok(())
    );
}
