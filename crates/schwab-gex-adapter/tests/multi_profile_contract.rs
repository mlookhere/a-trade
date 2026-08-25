use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use schwab_gex_adapter::{
    AdapterError, ConfigError, ProfilePool, SchwabConfig, SchwabProfileConfig, SecretString,
    StreamCommand, StreamRequestFactory, StreamerInfo, TokenSet, load_tokens,
};

fn profile(id: &str, token_path: &str) -> SchwabProfileConfig {
    SchwabProfileConfig {
        profile_id: id.to_owned(),
        client_id: SecretString::new(format!("client-{id}")).unwrap(),
        client_secret: SecretString::new(format!("secret-{id}")).unwrap(),
        callback_url: format!("https://localhost/{id}/callback"),
        token_path: PathBuf::from(token_path),
        rest_requests_per_minute: 120,
        rest_headroom_requests_per_minute: 20,
        transport_timeout_ms: 5_000,
    }
}

#[test]
fn arbitrary_profile_count_has_no_software_ceiling() {
    let profiles = (0..256)
        .map(|index| profile(&format!("p{index}"), &format!("tokens/p{index}.json")))
        .collect::<Vec<_>>();

    let config = SchwabConfig::new(profiles, 5_000, 1_000, 60_000).unwrap();
    assert_eq!(config.profiles.len(), 256);
    assert!(config.profile("p255").is_some());
}

#[test]
fn secrets_never_render_through_debug() {
    let secret = SecretString::new("do-not-log-me".to_owned()).unwrap();
    assert_eq!(format!("{secret:?}"), "***REDACTED***");

    let tokens: TokenSet = serde_json::from_value(serde_json::json!({
        "access_token": "access-do-not-log",
        "refresh_token": "refresh-do-not-log",
        "token_type": "Bearer",
        "expires_in": 1800,
        "scope": null,
        "obtained_at_unix_ms": 1
    }))
    .unwrap();
    let rendered = format!("{tokens:?}");
    assert!(!rendered.contains("access-do-not-log"));
    assert!(!rendered.contains("refresh-do-not-log"));
    assert!(rendered.contains("***REDACTED***"));
}

#[test]
fn profile_identity_and_token_files_must_not_collide() {
    let duplicate_id = SchwabConfig::new(
        vec![profile("a", "tokens/a.json"), profile("a", "tokens/b.json")],
        5_000,
        1_000,
        60_000,
    );
    assert_eq!(duplicate_id, Err(ConfigError::DuplicateProfileId));

    let duplicate_token = SchwabConfig::new(
        vec![
            profile("a", "tokens/shared.json"),
            profile("b", "tokens/shared.json"),
        ],
        5_000,
        1_000,
        60_000,
    );
    assert_eq!(duplicate_token, Err(ConfigError::DuplicateTokenPath));
}

#[test]
fn insecure_callback_empty_token_path_or_unbounded_timeout_fails_closed() {
    let mut insecure = profile("a", "tokens/a.json");
    insecure.callback_url = "http://localhost/callback".to_owned();
    assert_eq!(
        SchwabConfig::new(vec![insecure], 5_000, 1_000, 60_000),
        Err(ConfigError::InvalidCallbackUrl)
    );

    let mut missing_path = profile("a", "tokens/a.json");
    missing_path.token_path = PathBuf::new();
    assert_eq!(
        SchwabConfig::new(vec![missing_path], 5_000, 1_000, 60_000),
        Err(ConfigError::InvalidTokenPath)
    );

    let mut no_timeout = profile("a", "tokens/a.json");
    no_timeout.transport_timeout_ms = 0;
    assert_eq!(
        SchwabConfig::new(vec![no_timeout], 5_000, 1_000, 60_000),
        Err(ConfigError::InvalidTimeout)
    );
}

#[tokio::test]
async fn malformed_stored_tokens_are_rejected_before_use() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "a-trade-invalid-schwab-token-{}-{nonce}.json",
        std::process::id()
    ));
    tokio::fs::write(
        &path,
        serde_json::json!({
            "access_token": "",
            "refresh_token": "",
            "token_type": "Bearer",
            "expires_in": 0,
            "scope": null,
            "obtained_at_unix_ms": 1
        })
        .to_string(),
    )
    .await
    .unwrap();

    assert!(matches!(load_tokens(&path).await, Err(AdapterError::TokenStore(_))));
    let _ = tokio::fs::remove_file(path).await;
}

#[test]
fn each_profile_keeps_an_independent_rest_budget() {
    let config = SchwabConfig::new(
        vec![profile("a", "tokens/a.json"), profile("b", "tokens/b.json")],
        5_000,
        1_000,
        60_000,
    )
    .unwrap();
    let pool = ProfilePool::new(config.profiles).unwrap();

    assert_eq!(pool.clients()[0].config.effective_rest_limit(), 100);
    assert_eq!(pool.clients()[1].config.effective_rest_limit(), 100);
    assert_eq!(pool.clients()[0].limiter.effective_limit(), 100);
    assert_eq!(pool.clients()[1].limiter.effective_limit(), 100);
}

#[test]
fn assignments_are_sticky_balanced_and_explicitly_reassigned_after_profile_failure() {
    let mut pool = ProfilePool::new(vec![
        profile("a", "tokens/a.json"),
        profile("b", "tokens/b.json"),
        profile("c", "tokens/c.json"),
    ])
    .unwrap();

    assert_eq!(pool.assign("SPY").unwrap().profile_id, "a");
    assert_eq!(pool.assign("QQQ").unwrap().profile_id, "b");
    assert_eq!(pool.assign("IWM").unwrap().profile_id, "c");
    assert_eq!(pool.assign("SPY").unwrap().profile_id, "a");

    let released = pool.disable_profile("a").unwrap();
    assert_eq!(released, vec!["SPY"]);
    assert_eq!(pool.assigned_profile("SPY"), None);

    let reassigned = pool.assign("SPY").unwrap();
    assert_ne!(reassigned.profile_id, "a");
    assert_eq!(
        pool.assigned_profile("SPY"),
        Some(reassigned.profile_id.as_str())
    );
}

#[test]
fn no_enabled_profiles_fails_closed() {
    let mut pool = ProfilePool::new(vec![profile("a", "tokens/a.json")]).unwrap();
    pool.disable_profile("a").unwrap();
    assert_eq!(pool.assign("SPY"), Err(AdapterError::NoHealthyProfile));
}

fn streamer_info() -> StreamerInfo {
    StreamerInfo {
        streamer_socket_url: "wss://stream.example.test/ws".to_owned(),
        schwab_client_customer_id: "customer".to_owned(),
        schwab_client_correl_id: "correl".to_owned(),
        schwab_client_channel: "channel".to_owned(),
        schwab_client_function_id: "function".to_owned(),
    }
}

#[test]
fn stream_requests_use_change_only_gex_fields_and_unique_request_ids() {
    let mut factory = StreamRequestFactory::new(&streamer_info()).unwrap();

    let (login_id, login_json) = factory.login("access-token").unwrap();
    assert_eq!(login_id, "0");
    let login: serde_json::Value = serde_json::from_str(&login_json).unwrap();
    assert_eq!(login["requests"][0]["service"], "ADMIN");
    assert_eq!(login["requests"][0]["command"], "LOGIN");
    assert_eq!(
        login["requests"][0]["parameters"]["Authorization"],
        "access-token"
    );

    let symbols = vec![
        "SPY   260925C00500000".to_owned(),
        "SPY   260925P00500000".to_owned(),
    ];
    let (options_id, options_json) = factory
        .subscribe_options(&symbols, StreamCommand::Subs)
        .unwrap();
    assert_eq!(options_id, "1");
    let options: serde_json::Value = serde_json::from_str(&options_json).unwrap();
    assert_eq!(options["requests"][0]["service"], "LEVELONE_OPTIONS");
    assert_eq!(
        options["requests"][0]["parameters"]["fields"],
        "0,9,12,13,20,21,22,23,26,29,38"
    );

    let (spot_id, spot_json) = factory
        .subscribe_underlyings(&["SPY".to_owned()], StreamCommand::Subs)
        .unwrap();
    assert_eq!(spot_id, "2");
    let spot: serde_json::Value = serde_json::from_str(&spot_json).unwrap();
    assert_eq!(spot["requests"][0]["service"], "LEVELONE_EQUITIES");
    assert_eq!(spot["requests"][0]["parameters"]["fields"], "0,33,34");
}
