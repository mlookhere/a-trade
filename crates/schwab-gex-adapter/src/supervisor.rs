use std::{
    collections::HashMap,
    env,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::{
    sync::{Mutex, RwLock, watch},
    time::sleep,
};

use crate::{
    AdapterError, ConfigError, ProfileClients, ProfileStreamConnect, ProfileStreamSession,
    SchwabFleetRuntime, SecretString, StreamCommand, StreamDataBatch, StreamEvent,
    StreamResponse, StreamResponseAction, StreamResponseCode, load_tokens, require_response_success,
    response_action, save_tokens,
};

const RECONNECT_DELAY_ENV: &str = "SCHWAB_STREAM_RECONNECT_DELAY_MS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSupervisorConfig {
    reconnect_delay_ms: u64,
}

impl StreamSupervisorConfig {
    pub fn new(reconnect_delay_ms: u64) -> Result<Self, ConfigError> {
        if reconnect_delay_ms == 0 {
            return Err(ConfigError::InvalidTimeout);
        }
        Ok(Self { reconnect_delay_ms })
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        let value = env::var(RECONNECT_DELAY_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(ConfigError::Missing(RECONNECT_DELAY_ENV))?;
        let reconnect_delay_ms = value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidNumber(RECONNECT_DELAY_ENV))?;
        Self::new(reconnect_delay_ms)
    }

    #[must_use]
    pub const fn reconnect_delay_ms(self) -> u64 {
        self.reconnect_delay_ms
    }
}

#[derive(Debug, Default)]
pub struct StreamerIdentityRegistry {
    owners: HashMap<String, String>,
}

impl StreamerIdentityRegistry {
    pub fn reserve(&mut self, profile_id: &str, customer_id: &str) -> Result<(), AdapterError> {
        if profile_id.trim().is_empty() || customer_id.trim().is_empty() {
            return Err(AdapterError::InvalidInput("streamer identity"));
        }
        if self
            .owners
            .get(customer_id)
            .is_some_and(|owner| owner != profile_id)
        {
            return Err(AdapterError::DuplicateStreamerUser);
        }
        self.owners
            .insert(customer_id.to_owned(), profile_id.to_owned());
        Ok(())
    }

    pub fn release(&mut self, profile_id: &str, customer_id: &str) {
        if self
            .owners
            .get(customer_id)
            .is_some_and(|owner| owner == profile_id)
        {
            self.owners.remove(customer_id);
        }
    }
}

#[derive(Debug, Default)]
struct SubscriptionBarrier {
    pending: HashMap<String, (String, String)>,
    buffered: Vec<StreamDataBatch>,
}

impl SubscriptionBarrier {
    fn expect(&mut self, request_id: String, service: &str, command: &str) {
        self.pending
            .insert(request_id, (service.to_owned(), command.to_owned()));
    }

    fn on_response(&mut self, response: &StreamResponse) -> Result<(), AdapterError> {
        let Some((service, command)) = self.pending.get(&response.requestid).cloned() else {
            return Err(AdapterError::ProviderContract(
                "unexpected streamer response while subscriptions are pending".to_owned(),
            ));
        };
        require_response_success(response, &response.requestid, &service, &command)?;
        self.pending.remove(&response.requestid);
        Ok(())
    }

    fn buffer(&mut self, batches: Vec<StreamDataBatch>) {
        self.buffered.extend(batches);
    }

    fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    fn activate(
        self,
        runtime: &mut SchwabFleetRuntime,
        profile_id: &str,
    ) -> Result<(), AdapterError> {
        if !self.is_complete() {
            return Err(AdapterError::ProviderContract(
                "stream subscriptions activated before acknowledgements completed".to_owned(),
            ));
        }
        runtime.mark_stream_connected(profile_id)?;
        for batch in &self.buffered {
            runtime.apply_data_batch(profile_id, batch)?;
        }
        Ok(())
    }
}

/// Run one Schwab streaming profile until shutdown.
///
/// The loop reconnects only after an explicit deployment-configured delay. It refreshes an expired
/// token before connection and forces a refresh after a provider login denial. Reconnection only
/// restores the existing stream subscription plan; it never launches an option-chain REST bootstrap.
/// Reliable GEX status is restored only after every initial subscription acknowledgement succeeds
/// and any change-only data received during acknowledgement is replayed into the central runtime.
pub async fn run_profile_stream_supervisor(
    profile_id: String,
    runtime: Arc<RwLock<SchwabFleetRuntime>>,
    identities: Arc<Mutex<StreamerIdentityRegistry>>,
    config: StreamSupervisorConfig,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), AdapterError> {
    if profile_id.trim().is_empty() {
        return Err(AdapterError::InvalidInput("profile id"));
    }
    let clients = runtime.read().await.profile_clients(&profile_id)?;
    let reconnect_delay = Duration::from_millis(config.reconnect_delay_ms());
    let mut force_refresh = false;

    loop {
        if *shutdown.borrow() {
            return Ok(());
        }

        let request = match prepare_connect_request(&clients, force_refresh).await {
            Ok(request) => request,
            Err(error) if reconnectable(&error) => {
                force_refresh = matches!(error, AdapterError::StreamLoginDenied);
                if wait_for_retry_or_shutdown(reconnect_delay, &mut shutdown).await {
                    return Ok(());
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        force_refresh = false;

        let customer_id = request.streamer_info.schwab_client_customer_id.clone();
        identities
            .lock()
            .await
            .reserve(&profile_id, &customer_id)?;

        let attempt = match ProfileStreamSession::connect(request).await {
            Ok(session) => {
                run_connected_session(&profile_id, session, &runtime, &mut shutdown).await
            }
            Err(error) => Err(error),
        };

        identities
            .lock()
            .await
            .release(&profile_id, &customer_id);
        runtime
            .write()
            .await
            .mark_stream_disconnected(&profile_id)?;

        match attempt {
            Ok(()) => return Ok(()),
            Err(error) if reconnectable(&error) => {
                force_refresh = matches!(error, AdapterError::StreamLoginDenied);
                if wait_for_retry_or_shutdown(reconnect_delay, &mut shutdown).await {
                    return Ok(());
                }
            }
            Err(error) => return Err(error),
        }
    }
}

async fn prepare_connect_request(
    clients: &ProfileClients,
    force_refresh: bool,
) -> Result<ProfileStreamConnect, AdapterError> {
    let now_unix_ms = unix_ms_now()?;
    let mut tokens = load_tokens(&clients.config.token_path).await?;
    if force_refresh || tokens.access_expired_at(now_unix_ms) {
        tokens = clients
            .oauth
            .refresh_access_token(&tokens, now_unix_ms)
            .await?;
        save_tokens(&clients.config.token_path, &tokens).await?;
    }
    let streamer_info = clients.rest.streamer_info(tokens.access_token()).await?;
    let access_token = SecretString::new(tokens.access_token().to_owned())
        .map_err(|_| AdapterError::InvalidInput("access token"))?;
    Ok(ProfileStreamConnect {
        profile_id: clients.profile_id.clone(),
        streamer_info,
        access_token,
        transport_timeout_ms: clients.config.transport_timeout_ms,
    })
}

async fn run_connected_session(
    profile_id: &str,
    mut session: ProfileStreamSession,
    runtime: &Arc<RwLock<SchwabFleetRuntime>>,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<(), AdapterError> {
    if session.profile_id() != profile_id {
        return Err(AdapterError::ProviderContract(
            "connected streamer profile identity mismatch".to_owned(),
        ));
    }
    let plan = runtime
        .read()
        .await
        .subscription_plan(profile_id)
        .ok_or(AdapterError::StreamUnavailable)?;

    let mut barrier = SubscriptionBarrier::default();
    if !plan.underlyings.is_empty() {
        let (request_id, json) = session
            .requests_mut()
            .subscribe_underlyings(&plan.underlyings, StreamCommand::Subs)?;
        session.client_mut().send_json(json).await?;
        barrier.expect(request_id, "LEVELONE_EQUITIES", "SUBS");
    }
    if !plan.option_symbols.is_empty() {
        let (request_id, json) = session
            .requests_mut()
            .subscribe_options(&plan.option_symbols, StreamCommand::Subs)?;
        session.client_mut().send_json(json).await?;
        barrier.expect(request_id, "LEVELONE_OPTIONS", "SUBS");
    }

    while !barrier.is_complete() {
        let Some(event) = next_event_or_shutdown(&mut session, shutdown).await? else {
            return Ok(());
        };
        match event {
            StreamEvent::Responses(responses) => {
                for response in &responses {
                    barrier.on_response(response)?;
                }
            }
            StreamEvent::Data(batches) => barrier.buffer(batches),
            StreamEvent::Notify(_) => {}
            StreamEvent::Closed => {
                return Err(AdapterError::Transport(
                    "stream closed while subscriptions were pending".to_owned(),
                ));
            }
        }
    }

    barrier.activate(&mut runtime.write().await, profile_id)?;

    loop {
        let Some(event) = next_event_or_shutdown(&mut session, shutdown).await? else {
            return Ok(());
        };
        match event {
            StreamEvent::Data(batches) => {
                let mut runtime = runtime.write().await;
                for batch in &batches {
                    runtime.apply_data_batch(profile_id, batch)?;
                }
            }
            StreamEvent::Responses(responses) => {
                for response in &responses {
                    return Err(unexpected_live_response(response));
                }
            }
            StreamEvent::Notify(_) => {}
            StreamEvent::Closed => {
                return Err(AdapterError::Transport("stream closed".to_owned()));
            }
        }
    }
}

async fn next_event_or_shutdown(
    session: &mut ProfileStreamSession,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<Option<StreamEvent>, AdapterError> {
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Ok(None);
                }
            }
            event = session.client_mut().next_event() => return event.map(Some),
        }
    }
}

async fn wait_for_retry_or_shutdown(
    delay: Duration,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        _ = sleep(delay) => false,
        changed = shutdown.changed() => changed.is_err() || *shutdown.borrow(),
    }
}

fn unexpected_live_response(response: &StreamResponse) -> AdapterError {
    let code = StreamResponseCode::from(response.content.code);
    match response_action(code) {
        StreamResponseAction::RefreshTokenAndReconnect => AdapterError::StreamLoginDenied,
        StreamResponseAction::ConnectionLimitReached => AdapterError::StreamConnectionLimit,
        StreamResponseAction::SymbolLimitReached => AdapterError::StreamSymbolLimit,
        StreamResponseAction::Reconnect => AdapterError::StreamUnavailable,
        StreamResponseAction::FailClosed => AdapterError::Provider(format!(
            "unsolicited stream response failed with code {}: {}",
            response.content.code, response.content.msg
        )),
        StreamResponseAction::Continue => AdapterError::ProviderContract(
            "unexpected successful streamer response with no pending request".to_owned(),
        ),
    }
}

fn reconnectable(error: &AdapterError) -> bool {
    matches!(
        error,
        AdapterError::Transport(_)
            | AdapterError::TransportTimeout
            | AdapterError::StreamLoginDenied
            | AdapterError::StreamUnavailable
    )
}

fn unix_ms_now() -> Result<u64, AdapterError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AdapterError::ProviderContract("system clock before unix epoch".to_owned()))?;
    u64::try_from(duration.as_millis())
        .map_err(|_| AdapterError::ProviderContract("system clock overflow".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{SchwabProfileConfig, SchwabGexState};

    fn profile() -> SchwabProfileConfig {
        SchwabProfileConfig {
            profile_id: "a".to_owned(),
            client_id: SecretString::new("client".to_owned()).unwrap(),
            client_secret: SecretString::new("secret".to_owned()).unwrap(),
            callback_url: "https://localhost/callback".to_owned(),
            token_path: PathBuf::from("tokens/a.json"),
            rest_requests_per_minute: 120,
            rest_headroom_requests_per_minute: 20,
            transport_timeout_ms: 5_000,
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

    fn response(request_id: &str, service: &str) -> StreamResponse {
        StreamResponse {
            service: service.to_owned(),
            command: "SUBS".to_owned(),
            requestid: request_id.to_owned(),
            schwab_client_correl_id: "correl".to_owned(),
            timestamp: 1_000_100,
            content: crate::StreamResponseContent {
                code: 0,
                msg: "OK".to_owned(),
            },
        }
    }

    #[test]
    fn reconnect_delay_is_explicit_and_zero_fails_closed() {
        assert_eq!(
            StreamSupervisorConfig::new(0),
            Err(ConfigError::InvalidTimeout)
        );
        assert_eq!(
            StreamSupervisorConfig::new(250).unwrap().reconnect_delay_ms(),
            250
        );
    }

    #[test]
    fn duplicate_streamer_identity_is_rejected_until_owner_releases_it() {
        let mut registry = StreamerIdentityRegistry::default();
        registry.reserve("a", "customer").unwrap();
        assert_eq!(
            registry.reserve("b", "customer"),
            Err(AdapterError::DuplicateStreamerUser)
        );
        registry.release("a", "customer");
        registry.reserve("b", "customer").unwrap();
    }

    #[test]
    fn reliable_status_waits_for_all_subscription_acks_and_replays_buffered_changes() {
        let mut runtime = SchwabFleetRuntime::new(vec![profile()], 60_000).unwrap();
        let state = SchwabGexState::from_option_chain_json("SPY", &chain_json()).unwrap();
        runtime.install_bootstrap(state, 1_000_000).unwrap();

        let mut barrier = SubscriptionBarrier::default();
        barrier.expect("1".to_owned(), "LEVELONE_EQUITIES", "SUBS");
        barrier.expect("2".to_owned(), "LEVELONE_OPTIONS", "SUBS");
        barrier.buffer(vec![StreamDataBatch {
            service: "LEVELONE_OPTIONS".to_owned(),
            timestamp: 1_000_100,
            command: "SUBS".to_owned(),
            content: vec![serde_json::json!({
                "key": "SPY   260925C00500000",
                "29": 0.03,
                "38": 1_000_100
            })],
        }]);

        assert!(matches!(
            runtime.reliable_surface("SPY", 1_000_100, 1_000, 1_000),
            Err(crate::RuntimeQuality::StreamUnavailable)
        ));
        barrier
            .on_response(&response("1", "LEVELONE_EQUITIES"))
            .unwrap();
        assert!(!barrier.is_complete());
        barrier
            .on_response(&response("2", "LEVELONE_OPTIONS"))
            .unwrap();
        assert!(barrier.is_complete());
        barrier.activate(&mut runtime, "a").unwrap();
        assert!(
            runtime
                .reliable_surface("SPY", 1_000_100, 1_000, 1_000)
                .is_ok()
        );
    }

    #[test]
    fn disconnect_removes_reliability_and_same_plan_can_be_reactivated_without_rest_bootstrap() {
        let mut runtime = SchwabFleetRuntime::new(vec![profile()], 60_000).unwrap();
        let state = SchwabGexState::from_option_chain_json("SPY", &chain_json()).unwrap();
        runtime.install_bootstrap(state, 1_000_000).unwrap();
        runtime.mark_stream_connected("a").unwrap();
        runtime.mark_stream_disconnected("a").unwrap();
        assert!(matches!(
            runtime.reliable_surface("SPY", 1_000_100, 1_000, 1_000),
            Err(crate::RuntimeQuality::StreamUnavailable)
        ));
        assert!(!runtime.should_rebootstrap("SPY", 1_000_100));

        let plan = runtime.subscription_plan("a").unwrap();
        assert_eq!(plan.underlyings, vec!["SPY"]);
        assert_eq!(plan.option_symbols, vec!["SPY   260925C00500000"]);

        let barrier = SubscriptionBarrier::default();
        barrier.activate(&mut runtime, "a").unwrap();
        assert!(
            runtime
                .reliable_surface("SPY", 1_000_100, 1_000, 1_000)
                .is_ok()
        );
    }

    #[test]
    fn provider_limits_do_not_enter_reconnect_loop() {
        assert!(!reconnectable(&AdapterError::StreamConnectionLimit));
        assert!(!reconnectable(&AdapterError::StreamSymbolLimit));
        assert!(!reconnectable(&AdapterError::ProviderContract(
            "bad frame".to_owned()
        )));
        assert!(reconnectable(&AdapterError::TransportTimeout));
        assert!(reconnectable(&AdapterError::StreamLoginDenied));
    }
}
