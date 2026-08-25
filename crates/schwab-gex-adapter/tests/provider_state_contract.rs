use schwab_gex_adapter::{GammaQuality, SchwabGexState, StreamApply};

fn chain_json() -> String {
    serde_json::json!({
        "symbol": "SPY",
        "status": "SUCCESS",
        "isDelayed": false,
        "numberOfContracts": 2,
        "underlying": {
            "symbol": "SPY",
            "mark": 500.0,
            "quoteTime": 1_000_000,
            "delayed": false
        },
        "callExpDateMap": {
            "2026-09-25:31": {
                "500.0": [{
                    "symbol": "SPY   260925C00500000",
                    "putCall": "CALL",
                    "strikePrice": 500.0,
                    "expirationDate": "2026-09-25T00:00:00.000+00:00",
                    "gamma": 0.02,
                    "openInterest": 1000,
                    "multiplier": 100.0,
                    "quoteTimeInLong": 1_000_000
                }]
            }
        },
        "putExpDateMap": {
            "2026-09-25:31": {
                "495.0": [{
                    "symbol": "SPY   260925P00495000",
                    "putCall": "PUT",
                    "strikePrice": 495.0,
                    "expirationDate": "2026-09-25T00:00:00.000+00:00",
                    "gamma": 0.01,
                    "openInterest": 800,
                    "multiplier": 100.0,
                    "quoteTimeInLong": 1_000_000
                }]
            }
        }
    })
    .to_string()
}

#[test]
fn bootstrap_exposes_complete_sorted_stream_subscription_symbols() {
    let state = SchwabGexState::from_option_chain_json("SPY", &chain_json()).unwrap();
    assert_eq!(
        state.contract_symbols(),
        vec![
            "SPY   260925C00500000".to_owned(),
            "SPY   260925P00495000".to_owned()
        ]
    );
    assert_eq!(state.expected_contracts(), 2);
    assert_eq!(state.hydrated_contracts(), 2);
}

#[test]
fn partial_stream_update_preserves_absent_fields_instead_of_zeroing_them() {
    let mut state = SchwabGexState::from_option_chain_json("SPY", &chain_json()).unwrap();
    let before = state.reliable_surface(1_000_100, 1_000, 1_000).unwrap();

    let update = serde_json::json!({
        "key": "SPY   260925C00500000",
        "29": 0.03,
        "38": 1_000_050,
        "delayed": false
    });
    assert_eq!(
        state.apply_option_stream_content(&update).unwrap(),
        StreamApply::Applied
    );

    let after = state.reliable_surface(1_000_100, 1_000, 1_000).unwrap();
    assert_eq!(after.expected_contracts, 2);
    assert_eq!(after.hydrated_contracts, 2);
    assert!(after.snapshot.total_call_gex > before.snapshot.total_call_gex);
    assert_eq!(after.snapshot.total_put_gex, before.snapshot.total_put_gex);
}

#[test]
fn unknown_stream_contract_requires_rebootstrap_and_blocks_reliable_output() {
    let mut state = SchwabGexState::from_option_chain_json("SPY", &chain_json()).unwrap();
    let update = serde_json::json!({
        "key": "SPY   260925C00505000",
        "29": 0.02,
        "38": 1_000_050
    });

    assert_eq!(
        state.apply_option_stream_content(&update).unwrap(),
        StreamApply::UnknownContract
    );
    assert!(state.needs_rebootstrap());
    assert!(matches!(
        state.reliable_surface(1_000_100, 1_000, 1_000),
        Err(GammaQuality::RebootstrapRequired)
    ));
}

#[test]
fn stale_option_and_underlying_updates_are_ignored() {
    let mut state = SchwabGexState::from_option_chain_json("SPY", &chain_json()).unwrap();

    let option = serde_json::json!({
        "key": "SPY   260925C00500000",
        "29": 0.5,
        "38": 999_999
    });
    assert_eq!(
        state.apply_option_stream_content(&option).unwrap(),
        StreamApply::StaleIgnored
    );

    let underlying = serde_json::json!({
        "key": "SPY",
        "33": 501.0,
        "34": 999_999
    });
    assert_eq!(
        state.apply_underlying_stream_content(&underlying).unwrap(),
        StreamApply::StaleIgnored
    );
}
