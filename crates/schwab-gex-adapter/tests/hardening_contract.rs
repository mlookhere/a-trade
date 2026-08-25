use std::path::PathBuf;

use schwab_gex_adapter::{
    AdapterError, RuntimeQuality, SchwabFleetRuntime, SchwabGexState, SchwabProfileConfig,
    SecretString, StreamDataBatch, TokenSet, load_tokens,
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

fn chain_json() -> String {
    serde_json::json!({
        "symbol": "SPY",
        "status": "SUCCESS",
        "isDelayed": false,
        "numberOfContracts": 1,
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
        "putExpDateMap": {}
    })
    .to_string()
}

fn connected_runtime() -> SchwabFleetRuntime {
    let mut runtime = SchwabFleetRuntime::new(vec![profile("a")], 60_000).unwrap();
    let state = SchwabGexState::from_option_chain_json("SPY", &chain_json()).unwrap();
    runtime.install_bootstrap(state, 1_000_000).unwrap();
    runtime.mark_stream_connected("a").unwrap();
    assert!(
        runtime
            .reliable_surface("SPY", 1_000_100, 1_000, 1_000)
            .is_ok()
    );
    runtime
}

#[test]
fn malformed_present_option_identity_latches_provider_state_uncertain() {
    let mut runtime = connected_runtime();
    let batch = StreamDataBatch {
        service: "LEVELONE_OPTIONS".to_owned(),
        timestamp: 1_000_100,
        command: "SUBS".to_owned(),
        content: vec![serde_json::json!({
            "key": "SPY   260925C00500000",
            "21": "UNKNOWN_SIDE",
            "29": 0.03,
            "38": 1_000_100
        })],
    };

    assert!(matches!(
        runtime.apply_data_batch("a", &batch),
        Err(AdapterError::ProviderContract(_))
    ));
    assert!(matches!(
        runtime.reliable_surface("SPY", 1_000_100, 1_000, 1_000),
        Err(RuntimeQuality::RebootstrapRequired)
    ));
    assert!(runtime.should_rebootstrap("SPY", 1_000_100));
}

#[test]
fn malformed_numeric_stream_data_cannot_leave_old_surface_reliable() {
    let mut runtime = connected_runtime();
    let batch = StreamDataBatch {
        service: "LEVELONE_OPTIONS".to_owned(),
        timestamp: 1_000_100,
        command: "SUBS".to_owned(),
        content: vec![serde_json::json!({
            "key": "SPY   260925C00500000",
            "29": "not-a-number",
            "38": 1_000_100
        })],
    };

    assert!(matches!(
        runtime.apply_data_batch("a", &batch),
        Err(AdapterError::ProviderContract(_))
    ));
    assert!(matches!(
        runtime.reliable_surface("SPY", 1_000_100, 1_000, 1_000),
        Err(RuntimeQuality::RebootstrapRequired)
    ));
}

#[test]
fn unsupported_stream_service_latches_owned_state_uncertain() {
    let mut runtime = connected_runtime();
    let batch = StreamDataBatch {
        service: "UNSUPPORTED".to_owned(),
        timestamp: 1_000_100,
        command: "SUBS".to_owned(),
        content: vec![],
    };

    assert!(matches!(
        runtime.apply_data_batch("a", &batch),
        Err(AdapterError::ProviderContract(_))
    ));
    assert!(matches!(
        runtime.reliable_surface("SPY", 1_000_100, 1_000, 1_000),
        Err(RuntimeQuality::RebootstrapRequired)
    ));
}

#[test]
fn token_debug_never_renders_access_or_refresh_secret() {
    let tokens: TokenSet = serde_json::from_value(serde_json::json!({
        "access_token": "access-do-not-log",
        "refresh_token": "refresh-do-not-log",
        "token_type": "Bearer",
        "expires_in": 1800,
        "scope": "api",
        "obtained_at_unix_ms": 1
    }))
    .unwrap();

    let rendered = format!("{tokens:?}");
    assert!(!rendered.contains("access-do-not-log"));
    assert!(!rendered.contains("refresh-do-not-log"));
    assert!(rendered.contains("***REDACTED***"));
}

#[tokio::test]
async fn invalid_persisted_token_set_fails_closed_on_load() {
    let path = std::env::temp_dir().join(format!(
        "a-trade-invalid-schwab-token-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let invalid = serde_json::json!({
        "access_token": "access",
        "refresh_token": "",
        "token_type": "Bearer",
        "expires_in": 1800,
        "scope": null,
        "obtained_at_unix_ms": 1
    });
    tokio::fs::write(&path, serde_json::to_vec(&invalid).unwrap())
        .await
        .unwrap();

    assert!(matches!(load_tokens(&path).await, Err(AdapterError::TokenStore(_))));
    let _ = tokio::fs::remove_file(path).await;
}
