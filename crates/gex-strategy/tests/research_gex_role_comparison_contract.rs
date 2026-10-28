use gex_strategy::{
    DataQuality, GexLevelRole, GexScope, MarketSnapshot, MarketSnapshotInput, RawGexLevel,
    ResearchGexRoleComparison, ResearchGexRoleComparisonError, ResearchGexRoleInput,
    ResearchGexRoleObservationStatus, ResearchGexRoleReport, ResearchGexRoleSubmission, Session,
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
        &[
            RawGexLevel {
                strike: 610.0,
                gex_value: 15.0,
                scope: GexScope::Aggregate,
            },
            RawGexLevel {
                strike: 610.0,
                gex_value: -100.0,
                scope: GexScope::Expiry("2026-12-18".to_owned()),
            },
            RawGexLevel {
                strike: 590.0,
                gex_value: 1.0,
                scope: GexScope::Aggregate,
            },
        ],
    )
    .unwrap()
}

fn submit(
    snap: &MarketSnapshot,
    agent_id: &str,
    annotations: &[(usize, GexLevelRole)],
) -> ResearchGexRoleSubmission {
    let inputs = annotations
        .iter()
        .map(|(index, role)| {
            let level = &snap.gex_levels()[*index];
            ResearchGexRoleInput {
                snapshot_id: snap.snapshot_id().to_owned(),
                level_index: *index,
                scope: level.scope.clone(),
                strike: level.strike,
                role: *role,
                rationale: format!("Unverified research annotation from {agent_id}"),
            }
        })
        .collect();
    let report = ResearchGexRoleReport::build(snap, inputs).unwrap();
    ResearchGexRoleSubmission::new(agent_id, report).unwrap()
}

#[test]
fn aligned_agents_remain_unverified_without_a_winning_role_or_vote() {
    let snap = snapshot("same", 600.0);
    let a = submit(&snap, "gamma-a", &[(0, GexLevelRole::Support)]);
    let b = submit(&snap, "gamma-b", &[(0, GexLevelRole::Support)]);
    let comparison = ResearchGexRoleComparison::build(&snap, vec![a, b]).unwrap();

    assert_eq!(comparison.source_snapshot(), &snap);
    assert_eq!(comparison.validate_snapshot(&snap), Ok(()));
    assert_eq!(comparison.submissions().len(), 2);
    assert_eq!(comparison.levels().len(), 1);
    let level = comparison.for_level(0).unwrap();
    assert_eq!(level.level_index(), 0);
    assert_eq!(level.status(), ResearchGexRoleObservationStatus::Unverified);
    assert_eq!(level.claims().len(), 2);
    assert_eq!(level.claims()[0].agent_id(), "gamma-a");
    assert_eq!(level.claims()[1].agent_id(), "gamma-b");
    assert_eq!(level.claims()[0].role(), GexLevelRole::Support);
    assert_eq!(level.claims()[0].scope(), &GexScope::Aggregate);
    assert_eq!(level.claims()[0].strike(), 610.0);
    assert!(level.claims()[0].rationale().contains("gamma-a"));
    assert_eq!(comparison.for_level(1), None);
    assert_eq!(snap.gex_levels()[0].role, GexLevelRole::Unassigned);
}

#[test]
fn conflicting_agents_remain_explicitly_conflicting_even_when_one_is_unassigned() {
    let snap = snapshot("conflict", 600.0);
    let a = submit(&snap, "gamma-a", &[(0, GexLevelRole::Support)]);
    let b = submit(&snap, "gamma-b", &[(0, GexLevelRole::Resistance)]);
    let c = submit(&snap, "gamma-c", &[(0, GexLevelRole::Support)]);

    let report = ResearchGexRoleComparison::build(&snap, vec![a, b, c]).unwrap();
    let level = report.for_level(0).unwrap();
    assert_eq!(
        level.status(),
        ResearchGexRoleObservationStatus::Conflicting
    );
    assert_eq!(level.claims().len(), 3);
    assert_eq!(level.claims()[1].role(), GexLevelRole::Resistance);
    assert_eq!(level.claims()[2].role(), GexLevelRole::Support);

    let unassigned = submit(&snap, "gamma-a", &[(0, GexLevelRole::Unassigned)]);
    let assigned = submit(&snap, "gamma-b", &[(0, GexLevelRole::Support)]);
    let mixed = ResearchGexRoleComparison::build(&snap, vec![unassigned, assigned]).unwrap();
    assert_eq!(
        mixed.for_level(0).unwrap().status(),
        ResearchGexRoleObservationStatus::Conflicting
    );
}

#[test]
fn same_strike_different_scope_does_not_create_false_conflicts() {
    let snap = snapshot("scopes", 600.0);
    let aggregate = submit(&snap, "gamma-a", &[(0, GexLevelRole::Support)]);
    let expiry = submit(&snap, "gamma-b", &[(1, GexLevelRole::Resistance)]);
    let report = ResearchGexRoleComparison::build(&snap, vec![aggregate, expiry]).unwrap();

    assert_eq!(report.levels().len(), 2);
    assert_eq!(report.levels()[0].level_index(), 0);
    assert_eq!(report.levels()[1].level_index(), 1);
    assert_eq!(
        report.for_level(0).unwrap().status(),
        ResearchGexRoleObservationStatus::Unverified
    );
    assert_eq!(
        report.for_level(1).unwrap().status(),
        ResearchGexRoleObservationStatus::Unverified
    );
    assert_eq!(
        report.for_level(1).unwrap().claims()[0].scope(),
        &GexScope::Expiry("2026-12-18".to_owned())
    );
    assert_eq!(report.for_level(2), None);
}

#[test]
fn empty_reports_and_unmentioned_levels_stay_missing_not_inferred() {
    let snap = snapshot("empty", 600.0);
    let empty = ResearchGexRoleComparison::build(&snap, vec![]).unwrap();
    assert_eq!(empty.validate_snapshot(&snap), Ok(()));
    assert!(empty.levels().is_empty());
    assert_eq!(empty.for_level(0), None);

    let agent = submit(&snap, "gamma", &[]);
    let report = ResearchGexRoleComparison::build(&snap, vec![agent]).unwrap();
    assert_eq!(report.submissions().len(), 1);
    assert!(report.levels().is_empty());
    assert_eq!(report.for_level(1), None);
}

#[test]
fn duplicate_agent_or_blank_identity_fails_closed_without_a_vote() {
    let snap = snapshot("agents", 600.0);
    let report = ResearchGexRoleReport::build(&snap, vec![]).unwrap();
    assert_eq!(
        ResearchGexRoleSubmission::new("  ", report),
        Err(ResearchGexRoleComparisonError::MissingAgentId)
    );
    let a = submit(&snap, "same-agent", &[(0, GexLevelRole::Support)]);
    let b = submit(&snap, "same-agent", &[(1, GexLevelRole::Resistance)]);
    assert_eq!(
        ResearchGexRoleComparison::build(&snap, vec![a, b]),
        Err(ResearchGexRoleComparisonError::DuplicateAgentId)
    );
}

#[test]
fn wrong_id_and_same_id_revised_snapshot_cannot_launder_agent_annotations() {
    let snap = snapshot("source", 600.0);
    let original = submit(&snap, "gamma", &[(0, GexLevelRole::Support)]);
    let same_id_changed = snapshot("source", 601.0);
    assert_eq!(
        ResearchGexRoleComparison::build(&same_id_changed, vec![original.clone()]),
        Err(ResearchGexRoleComparisonError::SnapshotContentsMismatch)
    );
    let foreign = snapshot("other", 600.0);
    assert_eq!(
        ResearchGexRoleComparison::build(&foreign, vec![original.clone()]),
        Err(ResearchGexRoleComparisonError::SnapshotMismatch)
    );

    let result = ResearchGexRoleComparison::build(&snap, vec![original]).unwrap();
    assert_eq!(
        result.validate_snapshot(&same_id_changed),
        Err(ResearchGexRoleComparisonError::SnapshotContentsMismatch)
    );
    assert_eq!(
        result.validate_snapshot(&foreign),
        Err(ResearchGexRoleComparisonError::SnapshotMismatch)
    );
    assert_eq!(result.validate_snapshot(&snap), Ok(()));
}
