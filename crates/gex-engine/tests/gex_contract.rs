// Governing canonical context: §§5-6,14-15,25,33,77,83,111,117.
use gex_engine::{
    ContractGexInput, ExposureModel, GammaFlipModel, GexBook, GexError, NetGammaRegime, OptionSide,
};

fn assert_close(actual: f64, expected: f64) {
    let tolerance = expected.abs().max(1.0) * 1e-12;
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

fn contract(
    symbol: &str,
    strike: f64,
    side: OptionSide,
    gamma: f64,
    open_interest: u64,
    multiplier: f64,
    quote_time_millis: i64,
) -> ContractGexInput {
    ContractGexInput {
        symbol: symbol.to_owned(),
        underlying: "SPY".to_owned(),
        expiration: "2026-09-18".to_owned(),
        strike,
        side,
        gamma,
        open_interest,
        multiplier,
        quote_time_millis,
    }
}

#[test]
fn computes_signed_gex_using_provider_multiplier() {
    let mut book = GexBook::new("SPY").unwrap();
    book.set_spot(500.0, 10).unwrap();
    book.upsert(contract(
        "C510",
        510.0,
        OptionSide::Call,
        0.02,
        1_000,
        100.0,
        11,
    ))
    .unwrap();
    book.upsert(contract(
        "P490",
        490.0,
        OptionSide::Put,
        0.01,
        2_000,
        50.0,
        12,
    ))
    .unwrap();

    let snapshot = book.snapshot().unwrap();
    assert_close(snapshot.total_call_gex, 5_000_000.0);
    assert_close(snapshot.total_put_gex, -2_500_000.0);
    assert_close(snapshot.net_gex, 2_500_000.0);
    assert_eq!(snapshot.regime, NetGammaRegime::Positive);
    assert_eq!(snapshot.contract_count, 2);
    assert_eq!(
        snapshot.exposure_model,
        ExposureModel::SideSignedOpenInterest
    );
    assert_eq!(snapshot.as_of_millis, 10);
    assert_eq!(snapshot.oldest_contract_quote_time_millis, Some(11));
    assert_eq!(snapshot.newest_contract_quote_time_millis, Some(12));
    assert_eq!(snapshot.call_wall, Some(510.0));
    assert_eq!(snapshot.put_wall, Some(490.0));
}

#[test]
fn spot_update_rescales_existing_book_without_contract_rebuild() {
    let mut book = GexBook::new("SPY").unwrap();
    book.upsert(contract(
        "C510",
        510.0,
        OptionSide::Call,
        0.02,
        1_000,
        100.0,
        10,
    ))
    .unwrap();
    book.set_spot(500.0, 11).unwrap();
    let first = book.snapshot().unwrap();

    book.set_spot(510.0, 12).unwrap();
    let second = book.snapshot().unwrap();
    assert_eq!(book.contract_count(), 1);
    assert_close(
        second.total_call_gex / first.total_call_gex,
        510.0_f64.powi(2) / 500.0_f64.powi(2),
    );
}

#[test]
fn replacing_contract_removes_previous_contribution() {
    let mut book = GexBook::new("SPY").unwrap();
    book.set_spot(500.0, 10).unwrap();
    book.upsert(contract(
        "C510",
        510.0,
        OptionSide::Call,
        0.02,
        1_000,
        100.0,
        11,
    ))
    .unwrap();
    book.upsert(contract(
        "C510",
        510.0,
        OptionSide::Call,
        0.01,
        500,
        100.0,
        12,
    ))
    .unwrap();

    let snapshot = book.snapshot().unwrap();
    assert_close(snapshot.total_call_gex, 1_250_000.0);
    assert_eq!(snapshot.contract_count, 1);
    assert_eq!(snapshot.by_strike.len(), 1);
}

#[test]
fn removal_cleans_empty_buckets() {
    let mut book = GexBook::new("SPY").unwrap();
    book.set_spot(500.0, 10).unwrap();
    book.upsert(contract(
        "C510",
        510.0,
        OptionSide::Call,
        0.02,
        1_000,
        100.0,
        11,
    ))
    .unwrap();
    assert!(book.remove("C510"));
    assert!(!book.remove("C510"));

    let snapshot = book.snapshot().unwrap();
    assert_eq!(snapshot.regime, NetGammaRegime::Flat);
    assert!(snapshot.by_strike.is_empty());
    assert!(snapshot.by_expiration.is_empty());
    assert_eq!(snapshot.call_wall, None);
    assert_eq!(snapshot.put_wall, None);
}

#[test]
fn walls_use_largest_exposure_outside_spot_only() {
    let mut book = GexBook::new("SPY").unwrap();
    book.set_spot(500.0, 10).unwrap();
    for input in [
        contract("C495", 495.0, OptionSide::Call, 0.20, 10_000, 100.0, 11),
        contract("C505", 505.0, OptionSide::Call, 0.02, 2_000, 100.0, 11),
        contract("C510", 510.0, OptionSide::Call, 0.03, 2_000, 100.0, 11),
        contract("P505", 505.0, OptionSide::Put, 0.20, 10_000, 100.0, 11),
        contract("P495", 495.0, OptionSide::Put, 0.02, 2_000, 100.0, 11),
        contract("P490", 490.0, OptionSide::Put, 0.03, 2_000, 100.0, 11),
    ] {
        book.upsert(input).unwrap();
    }

    let snapshot = book.snapshot().unwrap();
    assert_eq!(snapshot.call_wall, Some(510.0));
    assert_eq!(snapshot.put_wall, Some(490.0));
}

#[test]
fn gamma_flip_stays_unresolved_instead_of_inventing_strategy_math() {
    let mut book = GexBook::new("SPY").unwrap();
    book.set_spot(500.0, 10).unwrap();
    let snapshot = book.snapshot().unwrap();
    assert_eq!(snapshot.gamma_flip, None);
    assert_eq!(snapshot.gamma_flip_model, GammaFlipModel::Unresolved);
}

#[test]
fn invalid_and_stale_inputs_fail_closed() {
    assert_eq!(GexBook::new(" ").unwrap_err(), GexError::MissingUnderlying);

    let mut book = GexBook::new("SPY").unwrap();
    assert_eq!(book.snapshot().unwrap_err(), GexError::SpotUnavailable);
    assert_eq!(book.set_spot(f64::NAN, 1), Err(GexError::InvalidSpot));
    book.set_spot(500.0, 10).unwrap();
    assert_eq!(book.set_spot(501.0, 9), Err(GexError::StaleSpotUpdate));

    let mut bad = contract("C510", 510.0, OptionSide::Call, -0.01, 1_000, 100.0, 11);
    assert_eq!(book.upsert(bad.clone()), Err(GexError::InvalidGamma));
    bad.gamma = 0.01;
    bad.multiplier = 0.0;
    assert_eq!(book.upsert(bad.clone()), Err(GexError::InvalidMultiplier));
    bad.multiplier = 100.0;
    bad.underlying = "QQQ".to_owned();
    assert_eq!(book.upsert(bad), Err(GexError::UnderlyingMismatch));

    book.upsert(contract(
        "C510",
        510.0,
        OptionSide::Call,
        0.02,
        1_000,
        100.0,
        12,
    ))
    .unwrap();
    assert_eq!(
        book.upsert(contract(
            "C510",
            510.0,
            OptionSide::Call,
            0.03,
            1_000,
            100.0,
            11,
        )),
        Err(GexError::StaleContractUpdate)
    );
}
