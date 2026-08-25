use schwab_gex_adapter::{BudgetDecision, BudgetError, RestBudget};

#[test]
fn rest_budget_reserves_explicit_headroom_and_releases_after_exact_window() {
    let mut budget = RestBudget::new(120, 20).unwrap();
    assert_eq!(budget.effective_limit(), 100);

    for _ in 0..100 {
        assert_eq!(budget.reserve(1_000).unwrap(), BudgetDecision::Granted);
    }
    assert_eq!(
        budget.reserve(1_000).unwrap(),
        BudgetDecision::RetryAfterMs(60_000)
    );
    assert_eq!(
        budget.reserve(60_999).unwrap(),
        BudgetDecision::RetryAfterMs(1)
    );
    assert_eq!(budget.reserve(61_000).unwrap(), BudgetDecision::Granted);
}

#[test]
fn each_profile_budget_is_independent_by_construction() {
    let mut a = RestBudget::new(120, 20).unwrap();
    let mut b = RestBudget::new(120, 20).unwrap();

    for _ in 0..100 {
        assert_eq!(a.reserve(0).unwrap(), BudgetDecision::Granted);
    }
    assert!(matches!(
        a.reserve(0).unwrap(),
        BudgetDecision::RetryAfterMs(_)
    ));
    assert_eq!(b.reserve(0).unwrap(), BudgetDecision::Granted);
}

#[test]
fn invalid_or_reversed_budget_time_fails_closed() {
    assert_eq!(RestBudget::new(0, 0), Err(BudgetError::InvalidLimit));
    assert_eq!(RestBudget::new(120, 120), Err(BudgetError::InvalidLimit));

    let mut budget = RestBudget::new(120, 20).unwrap();
    budget.reserve(100).unwrap();
    assert_eq!(budget.reserve(99), Err(BudgetError::TimeReversed));
}
