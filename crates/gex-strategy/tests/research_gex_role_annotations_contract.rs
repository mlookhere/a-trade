use gex_strategy::{
    DataQuality, GexLevelRole, GexScope, MarketSnapshot, MarketSnapshotInput, RawGexLevel,
    ResearchGexRoleError, ResearchGexRoleInput, ResearchGexRoleReport, Session,
};

fn snapshot(id: &str, spot: f64) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_012,
            spot_price: spot,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[
            RawGexLevel {
                strike: 510.0,
                gex_value: 15.0,
                scope: GexScope::Aggregate,
            },
            RawGexLevel {
                strike: 510.0,
                gex_value: -100.0,
                scope: GexScope::Expiry("2026-12-18".to_owned()),
            },
        ],
    )
    .unwrap()
}

fn annotation(snapshot: &MarketSnapshot, index: usize, role: GexLevelRole) -> ResearchGexRoleInput {
    let level = &snapshot.gex_levels()[index];
    ResearchGexRoleInput {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        level_index: index,
        scope: level.scope.clone(),
        strike: level.strike,
        role,
        rationale: "Unverified external research annotation; no role inference".to_owned(),
    }
}

#[test]
fn explicit_role_annotations_are_snapshot_bound_and_do_not_mutate_gex_levels() {
    // Canonical §§5-6,14-16,116: observational transport only.
    let snap = snapshot("role-1", 500.0);
    let input = annotation(&snap, 0, GexLevelRole::TargetMagnet);
    let report = ResearchGexRoleReport::build(&snap, vec![input]).unwrap();

    assert_eq!(report.source_snapshot(), &snap);
    assert_eq!(report.validate_snapshot(&snap), Ok(()));
    assert_eq!(report.annotations().len(), 1);
    assert_eq!(report.annotations()[0].level_index(), 0);
    assert_eq!(report.annotations()[0].role(), GexLevelRole::TargetMagnet);
    assert_eq!(report.annotations()[0].scope(), &GexScope::Aggregate);
    assert_eq!(report.annotations()[0].strike(), 510.0);
    assert!(!report.annotations()[0].rationale().trim().is_empty());
    assert_eq!(
        report.explicit_role_for(0),
        Some(GexLevelRole::TargetMagnet)
    );
    assert_eq!(report.explicit_role_for(1), None);

    // A role attached by a caller is never automatically part of raw GEX.
    assert_eq!(snap.gex_levels()[0].role, GexLevelRole::Unassigned);
    assert_eq!(snap.gex_levels()[1].role, GexLevelRole::Unassigned);

    // Same label, revised market data: never reuse the historical report.
    let revised = snapshot("role-1", 501.0);
    assert_eq!(
        report.validate_snapshot(&revised),
        Err(ResearchGexRoleError::SnapshotContentsMismatch)
    );
    assert_eq!(
        report.validate_snapshot(&snapshot("other-id", 500.0)),
        Err(ResearchGexRoleError::SnapshotMismatch)
    );
}

#[test]
fn unannotated_and_unassigned_levels_never_infer_a_role() {
    let snap = snapshot("role-2", 500.0);
    let empty = ResearchGexRoleReport::build(&snap, vec![]).unwrap();
    assert!(empty.annotations().is_empty());
    assert_eq!(empty.explicit_role_for(0), None);
    assert_eq!(empty.explicit_role_for(1), None);
    assert_eq!(empty.validate_snapshot(&snap), Ok(()));

    let explicit_unassigned =
        ResearchGexRoleReport::build(&snap, vec![annotation(&snap, 1, GexLevelRole::Unassigned)])
            .unwrap();
    assert_eq!(
        explicit_unassigned.explicit_role_for(1),
        Some(GexLevelRole::Unassigned)
    );
    assert_eq!(explicit_unassigned.explicit_role_for(0), None);
}

#[test]
fn exact_scope_strike_and_index_are_required_even_if_expiry_has_larger_gex() {
    let snap = snapshot("role-3", 500.0);

    let mut wrong_scope = annotation(&snap, 0, GexLevelRole::Support);
    wrong_scope.scope = snap.gex_levels()[1].scope.clone();
    assert_eq!(
        ResearchGexRoleReport::build(&snap, vec![wrong_scope]),
        Err(ResearchGexRoleError::ScopeMismatch)
    );

    let mut wrong_strike = annotation(&snap, 0, GexLevelRole::Support);
    wrong_strike.strike = 510.00001;
    assert_eq!(
        ResearchGexRoleReport::build(&snap, vec![wrong_strike]),
        Err(ResearchGexRoleError::StrikeMismatch)
    );

    let expiry = annotation(&snap, 1, GexLevelRole::Resistance);
    let result = ResearchGexRoleReport::build(&snap, vec![expiry]).unwrap();
    assert_eq!(result.explicit_role_for(0), None);
    assert_eq!(result.explicit_role_for(1), Some(GexLevelRole::Resistance));
}

#[test]
fn invalid_duplicate_or_missing_inputs_fail_closed_without_role_certification() {
    let snap = snapshot("role-4", 500.0);
    let good = annotation(&snap, 0, GexLevelRole::BreakoutPivot);

    let mut missing_id = good.clone();
    missing_id.snapshot_id = "  ".to_owned();
    assert_eq!(
        ResearchGexRoleReport::build(&snap, vec![missing_id]),
        Err(ResearchGexRoleError::MissingSnapshotId)
    );

    let mut wrong_id = good.clone();
    wrong_id.snapshot_id = "different".to_owned();
    assert_eq!(
        ResearchGexRoleReport::build(&snap, vec![wrong_id]),
        Err(ResearchGexRoleError::SnapshotMismatch)
    );

    let mut out_of_range = good.clone();
    out_of_range.level_index = 2;
    assert_eq!(
        ResearchGexRoleReport::build(&snap, vec![out_of_range]),
        Err(ResearchGexRoleError::InvalidLevelIndex)
    );

    for strike in [f64::NAN, f64::INFINITY, 0.0, -2.0] {
        let mut invalid_strike = good.clone();
        invalid_strike.strike = strike;
        assert_eq!(
            ResearchGexRoleReport::build(&snap, vec![invalid_strike]),
            Err(ResearchGexRoleError::InvalidStrike)
        );
    }

    let mut blank = good.clone();
    blank.rationale = " ".to_owned();
    assert_eq!(
        ResearchGexRoleReport::build(&snap, vec![blank]),
        Err(ResearchGexRoleError::BlankRationale)
    );
    assert_eq!(
        ResearchGexRoleReport::build(&snap, vec![good.clone(), good]),
        Err(ResearchGexRoleError::DuplicateLevelIndex)
    );
}
