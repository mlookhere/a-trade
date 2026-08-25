use schwab_gex_adapter::{GammaQuality, SchwabGexState, StreamApply};

const CHAIN_REPLAY: &str = include_str!("fixtures/spy_chain_replay.json");
const PARTIAL_OPTION_REPLAY: &str = include_str!("fixtures/spy_option_partial_replay.json");

#[test]
fn bootstrap_exposes_complete_sorted_stream_subscription_symbols() {
    let state = SchwabGexState::from_option_chain_json("SPY", CHAIN_REPLAY).unwrap();
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
    let mut state = SchwabGexState::from_option_chain_json("SPY", CHAIN_REPLAY).unwrap();
    let before = state.reliable_surface(1_000_100, 1_000, 1_000).unwrap();
    let update: serde_json::Value = serde_json::from_str(PARTIAL_OPTION_REPLAY).unwrap();

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
    let mut state = SchwabGexState::from_option_chain_json("SPY", CHAIN_REPLAY).unwrap();
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
    let mut state = SchwabGexState::from_option_chain_json("SPY", CHAIN_REPLAY).unwrap();

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
