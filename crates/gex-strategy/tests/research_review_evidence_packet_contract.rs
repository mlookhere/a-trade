//! A research evidence packet is a source-consistency witness, not a trade gate.
//! Canonical §§5–6,11,14–16,70,103,116: unknown source/broker/risk means NO_TRADE.

use gex_strategy::{
    AiNativeFeatureInput, AiNativeFeatureSet, DataQuality, DecisionVerdict, EvidenceAudit,
    ExplicitAgeLimits, GexLevelRole, GexScope, GexTimeObservation, HardCondition, HardVetoInput,
    HardVetoProvenanceError, HardVetoResult, HardVetoVerdict, MarketSnapshot, MarketSnapshotInput,
    RawGexLevel, ResearchGexRoleComparison, ResearchGexRoleComparisonError, ResearchGexRoleInput,
    ResearchGexRoleReport, ResearchGexRoleSubmission, ResearchReviewDecisionContext,
    ResearchReviewEvidencePacket, ResearchReviewPacketError, Session, SnapshotTimingAssessment,
    SnapshotTimingProvenanceError, SpecialistBatch, TemporalStatus, assess_snapshot_timing,
    audit_specialist_evidence, evaluate_hard_vetoes,
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
        &[RawGexLevel {
            strike: 610.0,
            gex_value: 12.0,
            scope: GexScope::Aggregate,
        }],
    )
    .unwrap()
}

fn features(s: &MarketSnapshot, strength: Option<f64>) -> AiNativeFeatureSet {
    let mut input = AiNativeFeatureInput {
        snapshot_id: s.snapshot_id().to_owned(),
        ..Default::default()
    };
    input.trend.trend_strength = strength;
    AiNativeFeatureSet::build(s, input).unwrap()
}

fn audit(s: &MarketSnapshot, strength: Option<f64>) -> EvidenceAudit {
    let f = features(s, strength);
    let batch = SpecialistBatch::build(s, vec![]).unwrap();
    audit_specialist_evidence(s, &f, &batch).unwrap()
}

fn roles(s: &MarketSnapshot, role: Option<GexLevelRole>) -> ResearchGexRoleComparison {
    let submissions = role
        .map(|role| {
            let level = &s.gex_levels()[0];
            let report = ResearchGexRoleReport::build(
                s,
                vec![ResearchGexRoleInput {
                    snapshot_id: s.snapshot_id().to_owned(),
                    level_index: 0,
                    scope: level.scope.clone(),
                    strike: level.strike,
                    role,
                    rationale: "Unverified agent annotation".to_owned(),
                }],
            )
            .unwrap();
            vec![ResearchGexRoleSubmission::new("gamma", report).unwrap()]
        })
        .unwrap_or_default();
    ResearchGexRoleComparison::build(s, submissions).unwrap()
}

fn limits() -> ExplicitAgeLimits {
    // Local synthetic test configuration; never a production age threshold.
    ExplicitAgeLimits {
        market_data_stale_timeout_ms: 20,
        gex_stale_timeout_ms: 20,
        max_decision_age_ms: 10,
    }
}

fn decision(clock: i64, age_limits: ExplicitAgeLimits) -> ResearchReviewDecisionContext {
    ResearchReviewDecisionContext {
        decision_timestamp_millis: clock,
        age_limits,
    }
}

fn gex(s: &MarketSnapshot) -> GexTimeObservation {
    GexTimeObservation {
        snapshot_id: s.snapshot_id().to_owned(),
        symbol: s.symbol().to_owned(),
        source_timestamp_millis: Some(1_002),
        received_timestamp_millis: Some(1_011),
        data_quality: DataQuality::Structured,
    }
}

fn timing(s: &MarketSnapshot, clock: i64, has_gex: bool) -> SnapshotTimingAssessment {
    let gex = if has_gex { Some(gex(s)) } else { None };
    assess_snapshot_timing(s, gex, clock, &limits())
}

/// Explicit synthetic research inputs: these values are not evidence of
/// current broker, portfolio or market-data safety.
fn veto_input(s: &MarketSnapshot, value: HardCondition) -> HardVetoInput {
    HardVetoInput {
        snapshot_id: s.snapshot_id().to_owned(),
        data_valid: value,
        data_fresh: value,
        gex_scope_valid: value,
        required_regime_data_valid: value,
        broker_connected: value,
        order_state_valid: value,
        instrument_tradable: value,
        liquidity_within_limits: value,
        max_position_risk_valid: value,
        max_portfolio_risk_valid: value,
        invalidation_defined: value,
        execution_cost_within_limits: value,
        kill_switch_clear: value,
    }
}

fn veto(s: &MarketSnapshot, value: HardCondition) -> HardVetoResult {
    evaluate_hard_vetoes(s, &veto_input(s, value)).unwrap()
}

#[test]
fn reports_from_one_exact_snapshot_form_a_read_only_review_packet() {
    let s = snapshot("same", 600.0);
    let a = audit(&s, None);
    let r = roles(&s, Some(GexLevelRole::Support));
    let t = timing(&s, 1_020, true);
    let v = veto(&s, HardCondition::Unknown);
    let packet =
        ResearchReviewEvidencePacket::build(&s, a.clone(), r.clone(), t.clone(), v.clone())
            .unwrap();

    assert_eq!(packet.source_snapshot(), &s);
    assert_eq!(packet.audit(), &a);
    assert_eq!(packet.role_comparison(), &r);
    assert_eq!(packet.timing(), &t);
    assert_eq!(packet.hard_veto(), &v);
    assert_eq!(packet.hard_veto().verdict(), HardVetoVerdict::Wait);
    assert_eq!(
        packet.hard_veto().blocking_decision(),
        Some(DecisionVerdict::Wait)
    );
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &t, &v, decision(1_020, limits())),
        Ok(())
    );
    assert_eq!(packet.role_comparison().levels().len(), 1);
    // A review packet does not expose an aggregate verdict or ALLOW method.
}

#[test]
fn mixed_snapshot_identity_and_same_id_revisions_are_rejected_at_construction() {
    let s = snapshot("one", 600.0);
    let revised = snapshot("one", 601.0);
    let foreign = snapshot("two", 600.0);
    let a = audit(&s, None);
    let r = roles(&s, None);
    let t = timing(&s, 1_020, true);
    let v = veto(&s, HardCondition::Unknown);

    assert_eq!(
        ResearchReviewEvidencePacket::build(&foreign, a.clone(), r.clone(), t.clone(), v.clone()),
        Err(ResearchReviewPacketError::SnapshotMismatch)
    );
    assert_eq!(
        ResearchReviewEvidencePacket::build(&revised, a.clone(), r.clone(), t.clone(), v.clone()),
        Err(ResearchReviewPacketError::SnapshotContentsMismatch)
    );
    assert_eq!(
        ResearchReviewEvidencePacket::build(
            &s,
            audit(&revised, None),
            r.clone(),
            t.clone(),
            v.clone()
        ),
        Err(ResearchReviewPacketError::SnapshotContentsMismatch)
    );
    assert_eq!(
        ResearchReviewEvidencePacket::build(
            &s,
            a.clone(),
            roles(&foreign, None),
            t.clone(),
            v.clone()
        ),
        Err(ResearchReviewPacketError::SnapshotMismatch)
    );
    assert_eq!(
        ResearchReviewEvidencePacket::build(&s, a, r, timing(&revised, 1_020, true), v),
        Err(ResearchReviewPacketError::SnapshotContentsMismatch)
    );
}

#[test]
fn altered_reports_and_later_clocks_cannot_replace_original_review_inputs() {
    let s = snapshot("one", 600.0);
    let a = audit(&s, None);
    let r = roles(&s, None);
    let t = timing(&s, 1_020, true);
    let v = veto(&s, HardCondition::Unknown);
    let packet =
        ResearchReviewEvidencePacket::build(&s, a.clone(), r.clone(), t.clone(), v.clone())
            .unwrap();

    assert_eq!(
        packet.validate_inputs(
            &s,
            &audit(&s, Some(3.0)),
            &r,
            &t,
            &v,
            decision(1_020, limits())
        ),
        Err(ResearchReviewPacketError::AuditChanged)
    );
    assert_eq!(
        packet.validate_inputs(
            &s,
            &a,
            &roles(&s, Some(GexLevelRole::Support)),
            &t,
            &v,
            decision(1_020, limits()),
        ),
        Err(ResearchReviewPacketError::RoleComparisonChanged)
    );
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &t, &v, decision(1_021, limits())),
        Err(ResearchReviewPacketError::TimingProvenance(
            SnapshotTimingProvenanceError::DecisionClockMismatch
        ))
    );
    let mut changed_limits = limits();
    changed_limits.gex_stale_timeout_ms += 1;
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &t, &v, decision(1_020, changed_limits)),
        Err(ResearchReviewPacketError::TimingProvenance(
            SnapshotTimingProvenanceError::AgeLimitsMismatch
        ))
    );
    let later = timing(&s, 1_021, true);
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &later, &v, decision(1_021, limits())),
        Err(ResearchReviewPacketError::TimingAssessmentChanged)
    );
    assert_eq!(
        packet.validate_inputs(
            &snapshot("other", 600.0),
            &a,
            &r,
            &t,
            &v,
            decision(1_020, limits())
        ),
        Err(ResearchReviewPacketError::SnapshotMismatch)
    );
    assert_eq!(
        packet.validate_inputs(
            &snapshot("one", 601.0),
            &a,
            &r,
            &t,
            &v,
            decision(1_020, limits())
        ),
        Err(ResearchReviewPacketError::SnapshotContentsMismatch)
    );
}

#[test]
fn missing_independent_gex_stays_missing_in_a_consistent_packet() {
    let s = snapshot("empty", 600.0);
    let a = audit(&s, None);
    let r = roles(&s, None);
    let t = timing(&s, 1_020, false);
    let v = veto(&s, HardCondition::Unknown);
    assert_eq!(t.gex_data(), TemporalStatus::Missing);
    let packet =
        ResearchReviewEvidencePacket::build(&s, a.clone(), r.clone(), t.clone(), v.clone())
            .unwrap();
    assert!(packet.role_comparison().levels().is_empty());
    assert_eq!(packet.timing().gex_data(), TemporalStatus::Missing);
    assert!(!packet.timing().observed_clocks_within_limits());
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &t, &v, decision(1_020, limits())),
        Ok(())
    );
    let present_gex = timing(&s, 1_020, true);
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &present_gex, &v, decision(1_020, limits())),
        Err(ResearchReviewPacketError::TimingAssessmentChanged)
    );
}

#[test]
fn independent_role_report_snapshot_mismatch_remains_rejected() {
    let s = snapshot("original", 600.0);
    let r = roles(&s, None);
    let changed = snapshot("original", 602.0);
    assert_eq!(
        r.validate_snapshot(&changed),
        Err(ResearchGexRoleComparisonError::SnapshotContentsMismatch)
    );
}

#[test]
fn hard_veto_report_is_required_and_cannot_be_relabelled_after_review() {
    let s = snapshot("veto", 600.0);
    let a = audit(&s, None);
    let r = roles(&s, None);
    let t = timing(&s, 1_020, true);
    let waiting = veto(&s, HardCondition::Unknown);
    let cleared = veto(&s, HardCondition::Pass);
    let rejected = veto(&s, HardCondition::Fail);
    let packet =
        ResearchReviewEvidencePacket::build(&s, a.clone(), r.clone(), t.clone(), waiting.clone())
            .unwrap();

    assert_eq!(packet.hard_veto().verdict(), HardVetoVerdict::Wait);
    assert_eq!(packet.hard_veto().unknown_conditions().len(), 13);
    assert_eq!(
        packet.hard_veto().blocking_decision(),
        Some(DecisionVerdict::Wait)
    );
    assert_eq!(cleared.verdict(), HardVetoVerdict::Cleared);
    assert_eq!(cleared.blocking_decision(), None);
    assert_ne!(cleared.blocking_decision(), Some(DecisionVerdict::Allow));
    assert_eq!(rejected.verdict(), HardVetoVerdict::Reject);
    assert_eq!(rejected.blocking_decision(), Some(DecisionVerdict::Reject));
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &t, &cleared, decision(1_020, limits())),
        Err(ResearchReviewPacketError::HardVetoChanged)
    );
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &t, &rejected, decision(1_020, limits())),
        Err(ResearchReviewPacketError::HardVetoChanged)
    );
    assert_eq!(
        waiting.validate_inputs(&s, &veto_input(&s, HardCondition::Pass)),
        Err(HardVetoProvenanceError::InputContentsMismatch)
    );
    assert_eq!(
        packet.validate_inputs(&s, &a, &r, &t, &waiting, decision(1_020, limits())),
        Ok(())
    );

    let changed = snapshot("veto", 601.0);
    assert_eq!(
        ResearchReviewEvidencePacket::build(
            &s,
            a.clone(),
            r.clone(),
            t.clone(),
            veto(&changed, HardCondition::Unknown),
        ),
        Err(ResearchReviewPacketError::SnapshotContentsMismatch)
    );
    let different = snapshot("another", 600.0);
    assert_eq!(
        ResearchReviewEvidencePacket::build(&s, a, r, t, veto(&different, HardCondition::Unknown),),
        Err(ResearchReviewPacketError::SnapshotMismatch)
    );
}

use gex_strategy::{
    AdvisoryDirection, EvidenceClaimInput, EvidenceKind, EvidenceReference, SpecialistOutput,
    SpecialistOutputInput, SpecialistRole, SpecialistStatus,
};

fn audit_with_references(s: &MarketSnapshot, references: Vec<EvidenceReference>) -> EvidenceAudit {
    let specialist = SpecialistOutput::build(
        s,
        SpecialistOutputInput {
            agent_id: "research-reviewer".to_owned(),
            role: SpecialistRole::GexDealerPositioning,
            snapshot_id: s.snapshot_id().to_owned(),
            status: SpecialistStatus::Ok,
            direction: AdvisoryDirection::Neutral,
            confidence: None,
            observations: vec![],
            features: vec![],
            supporting_evidence: vec![EvidenceClaimInput {
                claim: "Presence of references, not semantic proof".to_owned(),
                evidence: references,
                confidence: None,
            }],
            contradicting_evidence: vec![],
            unknowns: vec![],
            risks: vec![],
            proposed_invalidation: vec![],
            hard_veto: false,
            reason_codes: vec![],
        },
    )
    .unwrap();
    let batch = SpecialistBatch::build(s, vec![specialist]).unwrap();
    audit_specialist_evidence(s, &features(s, None), &batch).unwrap()
}

#[test]
fn diagnostics_do_not_upgrade_cleared_veto_or_missing_independent_gex() {
    let s = snapshot("diagnostic-clear", 600.0);
    let a = audit(&s, None);
    let r = roles(&s, Some(GexLevelRole::Support));
    let t = timing(&s, 1_020, false);
    let v = veto(&s, HardCondition::Pass);
    let packet = ResearchReviewEvidencePacket::build(&s, a, r, t, v).unwrap();
    let summary = packet.diagnostic_summary();

    assert_eq!(summary.reference_counts().observed(), 0);
    assert_eq!(summary.reference_counts().missing(), 0);
    assert_eq!(summary.reference_counts().ambiguous(), 0);
    assert_eq!(summary.reference_counts().unsupported(), 0);
    assert_eq!(summary.reference_counts().data_quality_unknown(), 0);
    assert_eq!(summary.unknown_claims(), 0);
    assert_eq!(summary.unverified_role_levels(), 1);
    assert_eq!(summary.conflicting_role_levels(), 0);
    assert_eq!(summary.independent_gex_timing(), TemporalStatus::Missing);
    assert_eq!(summary.hard_veto_verdict(), HardVetoVerdict::Cleared);
    assert_eq!(summary.failed_hard_conditions(), 0);
    assert_eq!(summary.unknown_hard_conditions(), 0);
    assert_eq!(packet.hard_veto().blocking_decision(), None);
    assert_ne!(
        packet.hard_veto().blocking_decision(),
        Some(DecisionVerdict::Allow)
    );
}

#[test]
fn diagnostics_preserve_distinct_observed_missing_ambiguous_and_unsupported_evidence() {
    let s = MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "ambiguous-diagnostics".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[
            RawGexLevel {
                strike: 610.0,
                gex_value: 5.0,
                scope: GexScope::Aggregate,
            },
            RawGexLevel {
                strike: 610.0,
                gex_value: 8.0,
                scope: GexScope::Aggregate,
            },
        ],
    )
    .unwrap();
    let a = audit_with_references(
        &s,
        vec![
            EvidenceReference {
                kind: EvidenceKind::SnapshotField,
                reference: "spot_price".to_owned(),
            },
            EvidenceReference {
                kind: EvidenceKind::DeterministicFeature,
                reference: "trend.ema_slope".to_owned(),
            },
            gex_strategy::gex_level_reference(&GexScope::Aggregate, 610.0).unwrap(),
            EvidenceReference {
                kind: EvidenceKind::Catalyst,
                reference: "not-a-verified-input".to_owned(),
            },
        ],
    );
    let packet = ResearchReviewEvidencePacket::build(
        &s,
        a,
        roles(&s, None),
        timing(&s, 1_020, true),
        veto(&s, HardCondition::Unknown),
    )
    .unwrap();
    let summary = packet.diagnostic_summary();
    assert_eq!(summary.reference_counts().observed(), 1);
    assert_eq!(summary.reference_counts().missing(), 1);
    assert_eq!(summary.reference_counts().ambiguous(), 1);
    assert_eq!(summary.reference_counts().unsupported(), 1);
    assert_eq!(summary.reference_counts().data_quality_unknown(), 0);
    assert_eq!(summary.unknown_claims(), 1);
    assert_eq!(summary.unverified_role_levels(), 0);
    assert_eq!(summary.conflicting_role_levels(), 0);
    assert_eq!(summary.hard_veto_verdict(), HardVetoVerdict::Wait);
    assert_eq!(summary.failed_hard_conditions(), 0);
    assert_eq!(summary.unknown_hard_conditions(), 13);
}

#[test]
fn diagnostics_keep_unknown_quality_and_conflicting_roles_separate_from_veto_failure() {
    let s = MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "unknown-diagnostics".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Unknown,
        },
        &[RawGexLevel {
            strike: 610.0,
            gex_value: 5.0,
            scope: GexScope::Aggregate,
        }],
    )
    .unwrap();
    let a = audit_with_references(
        &s,
        vec![EvidenceReference {
            kind: EvidenceKind::SnapshotField,
            reference: "spot_price".to_owned(),
        }],
    );
    let first = roles(&s, Some(GexLevelRole::Support));
    let other = ResearchGexRoleReport::build(
        &s,
        vec![ResearchGexRoleInput {
            snapshot_id: s.snapshot_id().to_owned(),
            level_index: 0,
            scope: GexScope::Aggregate,
            strike: 610.0,
            role: GexLevelRole::Resistance,
            rationale: "Unverified contradictory research claim".to_owned(),
        }],
    )
    .unwrap();
    let competing = ResearchGexRoleSubmission::new("gamma-other", other).unwrap();
    let comparison =
        ResearchGexRoleComparison::build(&s, vec![first.submissions()[0].clone(), competing])
            .unwrap();
    let packet = ResearchReviewEvidencePacket::build(
        &s,
        a,
        comparison,
        timing(&s, 1_020, false),
        veto(&s, HardCondition::Fail),
    )
    .unwrap();
    let summary = packet.diagnostic_summary();
    assert_eq!(summary.reference_counts().data_quality_unknown(), 1);
    assert_eq!(summary.reference_counts().observed(), 0);
    assert_eq!(summary.unknown_claims(), 1);
    assert_eq!(summary.conflicting_role_levels(), 1);
    assert_eq!(summary.unverified_role_levels(), 0);
    assert_eq!(summary.independent_gex_timing(), TemporalStatus::Missing);
    assert_eq!(summary.hard_veto_verdict(), HardVetoVerdict::Reject);
    assert_eq!(summary.failed_hard_conditions(), 13);
    assert_eq!(summary.unknown_hard_conditions(), 0);
}
