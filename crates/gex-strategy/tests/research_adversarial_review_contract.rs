use gex_strategy::{
    AdvisoryDirection, AiNativeFeatureInput, AiNativeFeatureSet, ClaimObservationStatus,
    DataQuality, EvidenceClaimInput, EvidenceKind, EvidenceReference, ExplicitAgeLimits,
    HardCondition, HardVetoInput, MarketSnapshot, MarketSnapshotInput,
    ResearchAdversarialChallengeInput, ResearchAdversarialReview, ResearchAdversarialReviewError,
    ResearchChallengeClaimSide, ResearchGexRoleComparison, ResearchReviewEvidencePacket, Session,
    SpecialistBatch, SpecialistOutput, SpecialistOutputInput, SpecialistRole, SpecialistStatus,
    assess_snapshot_timing, audit_specialist_evidence, evaluate_hard_vetoes,
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
