use std::path::PathBuf;

use schwab_gex_adapter::{
    AdapterError, GammaQuality, ProfileStreamConnect, RuntimeQuality, SchwabFleetRuntime,
    SchwabGexState, SchwabProfileConfig, SecretString, StreamDataBatch, StreamResponse,
    StreamResponseAction, StreamResponseCode, StreamResponseContent, StreamerInfo,
    connect_profiles, require_response_success, response_action,
};

fn profile(id: &str) -> SchwabProfileConfig {
    SchwabProfileConfig {
        profile_id: id.to_owned(),
        client_id: SecretString::new(format!("client-{id}")).unwrap(),
        client_secret: SecretString::new(format!("secret-{id}")).unwrap(),
        callback_url: format!("https://localhost/{id}/callback"),
        token_path: PathBuf::from(format!("tokens/{id}.json")),
        rest_requests_per_minute: 120,
        rest_headroom_requests_per_minute: 20,
    }
}

fn chain_json(symbol: &str, option_symbol: &str) -> String {
    serde_json::json!({
        "symbol": symbol,
        "status": "SUCCESS",
        "isDelayed": false,
        "numberOfContracts": 1,
        "underlying": {
            "symbol": symbol,
            "mark": 500.0,
            "quoteTime": 1_000_000,
            "delayed": false
        },
        "callExpDateMap": {
            "2026-09-25:31": {
                "500.0": [{
                    "symbol": option_symbol,
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
        "putExpDateMap": {}
    })
    .to_string()
}

#[test]
fn provider_connection_and_symbol_limits_are_explicit_fail_closed_outcomes() {
    assert_eq!(
        response_action(StreamResponseCode::CloseConnection),
        StreamResponseAction::ConnectionLimitReached
    );
    assert_eq!(
        response_action(StreamResponseCode::ReachedSymbolLimit),
        StreamResponseAction::SymbolLimitReached
    );

    let response = StreamResponse {
        service: "ADMIN".to_owned(),
        command: "LOGIN".to_owned(),
        requestid: "1".to_owned(),
        schwab_client_correl_id: "correl".to_owned(),
        timestamp: 1,
        content: StreamResponseContent {
            code: 12,
            msg: "connection limit".to_owned(),
        },
    };
    assert_eq!(
        require_response_success(&response, "1", "ADMIN", "LOGIN"),
        Err(AdapterError::StreamConnectionLimit)
    );
}

#[tokio::test]
async fn duplicate_schwab_streamer_user_is_rejected_before_any_parallel_dial() {
    let streamer = |profile_id: &str| ProfileStreamConnect {
        profile_id: profile_id.to_owned(),
        streamer_info: StreamerInfo {
            streamer_socket_url: "wss://example.invalid/ws".to_owned(),
            schwab_client_customer_id: "same-user".to_owned(),
            schwab_client_correl_id: format!("correl-{profile_id}"),
            schwab_client_channel: "channel".to_owned(),
            schwab_client_function_id: "function".to_owned(),
        },
        access_token: SecretString::new(format!("token-{profile_id}")).unwrap(),
    };

    assert!(matches!(
        connect_profiles(vec![streamer("a"), streamer("b")]).await,
        Err(AdapterError::DuplicateStreamerUser)
    ));
}

#[test]
fn fleet_dispatches_each_underlying_only_through_its_assigned_profile() {
    let mut runtime = SchwabFleetRuntime::new(vec![profile("a"), profile("b")], 60_000).unwrap();
    let spy =
        SchwabGexState::from_option_chain_json("SPY", &chain_json("SPY", "SPY   260925C00500000"))
            .unwrap();
    let qqq =
        SchwabGexState::from_option_chain_json("QQQ", &chain_json("QQQ", "QQQ   260925C00500000"))
            .unwrap();

    assert_eq!(
        runtime
            .install_bootstrap(spy, 1_000_000)
            .unwrap()
            .profile_id,
        "a"
    );
    assert_eq!(
        runtime
            .install_bootstrap(qqq, 1_000_000)
            .unwrap()
            .profile_id,
        "b"
    );
    runtime.mark_stream_connected("a").unwrap();
    runtime.mark_stream_connected("b").unwrap();

    let plan_a = runtime.subscription_plan("a").unwrap();
    assert_eq!(plan_a.underlyings, vec!["SPY"]);
    assert_eq!(plan_a.option_symbols, vec!["SPY   260925C00500000"]);

    let batch = StreamDataBatch {
        service: "LEVELONE_OPTIONS".to_owned(),
        timestamp: 1_000_100,
        command: "SUBS".to_owned(),
        content: vec![serde_json::json!({
            "key": "SPY   260925C00500000",
            "29": 0.03,
            "38": 1_000_100
        })],
    };
    assert_eq!(runtime.apply_data_batch("a", &batch).unwrap().applied, 1);
    assert!(
        runtime
            .reliable_surface("SPY", 1_000_100, 1_000, 1_000)
            .is_ok()
    );
    assert!(matches!(
        runtime.apply_data_batch("b", &batch),
        Err(AdapterError::ProviderContract(_))
    ));
}

#[test]
fn disconnect_blocks_live_reliable_output_without_forcing_rest_rebootstrap() {
    let mut runtime = SchwabFleetRuntime::new(vec![profile("a")], 60_000).unwrap();
    let state =
        SchwabGexState::from_option_chain_json("SPY", &chain_json("SPY", "SPY   260925C00500000"))
            .unwrap();
    runtime.install_bootstrap(state, 1_000_000).unwrap();
    runtime.mark_stream_connected("a").unwrap();
    assert!(
        runtime
            .reliable_surface("SPY", 1_000_100, 1_000, 1_000)
            .is_ok()
    );

    runtime.mark_stream_disconnected("a").unwrap();
    assert!(matches!(
        runtime.reliable_surface("SPY", 1_000_100, 1_000, 1_000),
        Err(RuntimeQuality::StreamUnavailable)
    ));
    assert!(!runtime.should_rebootstrap("SPY", 1_000_100));
}

#[test]
fn unknown_option_without_identity_marks_profile_states_for_rebootstrap() {
    let mut runtime = SchwabFleetRuntime::new(vec![profile("a")], 60_000).unwrap();
    let state =
        SchwabGexState::from_option_chain_json("SPY", &chain_json("SPY", "SPY   260925C00500000"))
            .unwrap();
    runtime.install_bootstrap(state, 1_000_000).unwrap();
    runtime.mark_stream_connected("a").unwrap();

    let batch = StreamDataBatch {
        service: "LEVELONE_OPTIONS".to_owned(),
        timestamp: 1_000_100,
        command: "ADD".to_owned(),
        content: vec![serde_json::json!({
            "key": "UNKNOWN-CONTRACT",
            "29": 0.02,
            "38": 1_000_100
        })],
    };
    assert_eq!(
        runtime
            .apply_data_batch("a", &batch)
            .unwrap()
            .unknown_contracts,
        1
    );
    assert!(runtime.should_rebootstrap("SPY", 1_000_100));
    assert!(matches!(
        runtime.reliable_surface("SPY", 1_000_100, 1_000, 1_000),
        Err(RuntimeQuality::RebootstrapRequired)
            | Err(RuntimeQuality::Gamma(GammaQuality::RebootstrapRequired))
    ));
}
