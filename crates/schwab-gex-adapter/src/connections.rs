use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::{StreamExt, stream::FuturesUnordered};
use tokio::{
    sync::{Mutex, RwLock, watch},
    time::{sleep, timeout},
};

use crate::{
    AdapterError, ConfigError, ProfileClients, SchwabFleetRuntime, SchwabStreamClient,
    SecretString, StreamCommand, StreamDataBatch, StreamEvent, StreamRequestFactory,
    StreamResponse, StreamResponseAction, StreamResponseCode, StreamerInfo, load_tokens,
    require_response_success, response_action, save_tokens,
};

const RECONNECT_DELAY_ENV: &str = "SCHWAB_STREAM_RECONNECT_DELAY_MS";

#[derive(Debug, Clone)]
pub struct ProfileStreamConnect {
    pub profile_id: String,
    pub streamer_info: StreamerInfo,
    pub access_token: SecretString,
    pub transport_timeout_ms: u64,
}

pub struct ProfileStreamSession {
    profile_id: String,
    client: SchwabStreamClient,
    requests: StreamRequestFactory,
}

impl ProfileStreamSession {
    pub async fn connect(request: ProfileStreamConnect) -> Result<Self, AdapterError> {
        if request.profile_id.trim().is_empty() {
            return Err(AdapterError::InvalidInput("profile id"));
        }
        if request.transport_timeout_ms == 0 {
            return Err(AdapterError::InvalidInput("transport timeout"));
        }
        timeout(
            Duration::from_millis(request.transport_timeout_ms),
            Self::connect_and_login(request),
        )
        .await
        .map_err(|_| AdapterError::TransportTimeout)?
    }

    async fn connect_and_login(request: ProfileStreamConnect) -> Result<Self, AdapterError> {
        let mut client = SchwabStreamClient::connect(
            &request.streamer_info.streamer_socket_url,
            request.transport_timeout_ms,
        )
        .await?;
        let mut requests = StreamRequestFactory::new(&request.streamer_info)?;
        let (request_id, login) = requests.login(request.access_token.expose())?;
        client.send_json(login).await?;
        loop {
            match client.next_event().await? {
                StreamEvent::Responses(responses) => {
                    let Some(response) = responses.iter().find(|r| r.requestid == request_id)
                    else {
                        continue;
                    };
                    require_response_success(response, &request_id, "ADMIN", "LOGIN")?;
                    return Ok(Self {
                        profile_id: request.profile_id,
                        client,
                        requests,
                    });
                }
                StreamEvent::Notify(_) => {}
                StreamEvent::Data(_) => {
                    return Err(AdapterError::ProviderContract(
                        "market data arrived before successful streamer login".to_owned(),
                    ));
                }
                StreamEvent::Closed => {
                    return Err(AdapterError::Transport(
                        "stream closed before successful login".to_owned(),
                    ));
                }
            }
        }
    }

    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn client_mut(&mut self) -> &mut SchwabStreamClient {
        &mut self.client
    }

    pub fn requests_mut(&mut self) -> &mut StreamRequestFactory {
        &mut self.requests
    }
}

pub struct ProfileConnectOutcome {
    pub profile_id: String,
    pub result: Result<ProfileStreamSession, AdapterError>,
}

pub async fn connect_profiles(
    requests: Vec<ProfileStreamConnect>,
) -> Result<Vec<ProfileConnectOutcome>, AdapterError> {
    let mut profile_ids = HashSet::new();
    let mut customer_ids = HashSet::new();
    for request in &requests {
        if !profile_ids.insert(request.profile_id.clone()) {
            return Err(AdapterError::DuplicateProfileConnection);
        }
        if !customer_ids.insert(request.streamer_info.schwab_client_customer_id.clone()) {
            return Err(AdapterError::DuplicateStreamerUser);
        }
    }
    let mut pending = FuturesUnordered::new();
    for request in requests {
        pending.push(async move {
            let profile_id = request.profile_id.clone();
            ProfileConnectOutcome {
                profile_id,
                result: ProfileStreamSession::connect(request).await,
            }
        });
    }
    let mut outcomes = Vec::new();
    while let Some(outcome) = pending.next().await {
        outcomes.push(outcome);
    }
    outcomes.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
    Ok(outcomes)
}

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
        let raw = env::var(RECONNECT_DELAY_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(ConfigError::Missing(RECONNECT_DELAY_ENV))?;
        let delay = raw
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidNumber(RECONNECT_DELAY_ENV))?;
        Self::new(delay)
    }
}

#[derive(Debug, Default)]
pub struct StreamerIdentityRegistry(HashMap<String, String>);

impl StreamerIdentityRegistry {
    pub fn reserve(&mut self, profile_id: &str, customer_id: &str) -> Result<(), AdapterError> {
        if profile_id.trim().is_empty() || customer_id.trim().is_empty() {
            return Err(AdapterError::InvalidInput("streamer identity"));
        }
        if self
            .0
            .get(customer_id)
            .is_some_and(|owner| owner != profile_id)
        {
            return Err(AdapterError::DuplicateStreamerUser);
        }
        self.0.insert(customer_id.to_owned(), profile_id.to_owned());
        Ok(())
    }

    pub fn release(&mut self, profile_id: &str, customer_id: &str) {
        if self
            .0
            .get(customer_id)
            .is_some_and(|owner| owner == profile_id)
        {
            self.0.remove(customer_id);
        }
    }
}

/// Keeps one profile's streamer session alive until shutdown. Reconnect delay is explicit runtime
/// configuration. Reconnect restores only the existing subscription plan and never initiates an
/// option-chain bootstrap. Reliable state is restored only after all SUBS acknowledgements pass.
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
    let reconnect_delay = Duration::from_millis(config.reconnect_delay_ms);
    let mut force_refresh = false;
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }
        let request = match prepare_connect_request(&clients, force_refresh).await {
            Ok(request) => request,
            Err(error) if reconnectable(&error) => {
                force_refresh = matches!(error, AdapterError::StreamLoginDenied);
                if wait_retry(reconnect_delay, &mut shutdown).await {
                    return Ok(());
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let customer_id = request.streamer_info.schwab_client_customer_id.clone();
        identities.lock().await.reserve(&profile_id, &customer_id)?;
        let attempt = match ProfileStreamSession::connect(request).await {
            Ok(session) => run_connected(&profile_id, session, &runtime, &mut shutdown).await,
            Err(error) => Err(error),
        };
        identities.lock().await.release(&profile_id, &customer_id);
        runtime
            .write()
            .await
            .mark_stream_disconnected(&profile_id)?;
        match attempt {
            Ok(()) => return Ok(()),
            Err(error) if reconnectable(&error) => {
                force_refresh = matches!(error, AdapterError::StreamLoginDenied);
                if wait_retry(reconnect_delay, &mut shutdown).await {
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
    let now = unix_ms_now()?;
    let mut tokens = load_tokens(&clients.config.token_path).await?;
    if force_refresh || tokens.access_expired_at(now) {
        tokens = clients.oauth.refresh_access_token(&tokens, now).await?;
        save_tokens(&clients.config.token_path, &tokens).await?;
    }
    let streamer_info = clients.rest.streamer_info(tokens.access_token()).await?;
    Ok(ProfileStreamConnect {
        profile_id: clients.profile_id.clone(),
        streamer_info,
        access_token: SecretString::new(tokens.access_token().to_owned())
            .map_err(|_| AdapterError::InvalidInput("access token"))?,
        transport_timeout_ms: clients.config.transport_timeout_ms,
    })
}

async fn run_connected(
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
    let mut pending: HashMap<String, (String, String)> = HashMap::new();
    let mut buffered = Vec::new();
    if !plan.underlyings.is_empty() {
        let (id, json) = session
            .requests_mut()
            .subscribe_underlyings(&plan.underlyings, StreamCommand::Subs)?;
        session.client_mut().send_json(json).await?;
        pending.insert(id, ("LEVELONE_EQUITIES".to_owned(), "SUBS".to_owned()));
    }
    if !plan.option_symbols.is_empty() {
        let (id, json) = session
            .requests_mut()
            .subscribe_options(&plan.option_symbols, StreamCommand::Subs)?;
        session.client_mut().send_json(json).await?;
        pending.insert(id, ("LEVELONE_OPTIONS".to_owned(), "SUBS".to_owned()));
    }
    while !pending.is_empty() {
        let Some(event) = next_or_shutdown(&mut session, shutdown).await? else {
            return Ok(());
        };
        match event {
            StreamEvent::Responses(responses) => acknowledge(&mut pending, &responses)?,
            StreamEvent::Data(batches) => buffered.extend(batches),
            StreamEvent::Notify(_) => {}
            StreamEvent::Closed => return Err(AdapterError::StreamUnavailable),
        }
    }
    activate_after_subscriptions(
        &mut *runtime.write().await,
        profile_id,
        &pending,
        &buffered,
    )?;
    loop {
        let Some(event) = next_or_shutdown(&mut session, shutdown).await? else {
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
                let response = responses.first().ok_or_else(|| {
                    AdapterError::ProviderContract("empty live streamer response".to_owned())
                })?;
                return Err(unexpected_live_response(response));
            }
            StreamEvent::Notify(_) => {}
            StreamEvent::Closed => return Err(AdapterError::StreamUnavailable),
        }
    }
}

fn acknowledge(
    pending: &mut HashMap<String, (String, String)>,
    responses: &[StreamResponse],
) -> Result<(), AdapterError> {
    for response in responses {
        let Some((service, command)) = pending.get(&response.requestid).cloned() else {
            return Err(AdapterError::ProviderContract(
                "unexpected streamer response while subscriptions are pending".to_owned(),
            ));
        };
        require_response_success(response, &response.requestid, &service, &command)?;
        pending.remove(&response.requestid);
    }
    Ok(())
}

fn activate_after_subscriptions(
    runtime: &mut SchwabFleetRuntime,
    profile_id: &str,
    pending: &HashMap<String, (String, String)>,
    buffered: &[StreamDataBatch],
) -> Result<(), AdapterError> {
    if !pending.is_empty() {
        return Err(AdapterError::ProviderContract(
            "stream subscriptions activated before acknowledgements completed".to_owned(),
        ));
    }
    runtime.mark_stream_connected(profile_id)?;
    for batch in buffered {
        runtime.apply_data_batch(profile_id, batch)?;
    }
    Ok(())
}

async fn next_or_shutdown(
    session: &mut ProfileStreamSession,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<Option<StreamEvent>, AdapterError> {
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { return Ok(None); }
            }
            event = session.client_mut().next_event() => return event.map(Some),
        }
    }
}

async fn wait_retry(delay: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = sleep(delay) => false,
        changed = shutdown.changed() => changed.is_err() || *shutdown.borrow(),
    }
}

fn unexpected_live_response(response: &StreamResponse) -> AdapterError {
    match response_action(StreamResponseCode::from(response.content.code)) {
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
    use super::*;

    #[test]
    fn reconnect_configuration_and_provider_limits_fail_closed() {
        assert_eq!(
            StreamSupervisorConfig::new(0),
            Err(ConfigError::InvalidTimeout)
        );
        assert!(!reconnectable(&AdapterError::StreamConnectionLimit));
        assert!(!reconnectable(&AdapterError::StreamSymbolLimit));
        assert!(reconnectable(&AdapterError::TransportTimeout));
        assert!(reconnectable(&AdapterError::StreamLoginDenied));
    }

    #[test]
    fn streamer_identity_is_exclusive_until_release() {
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
    fn subscription_ack_identity_is_exact() {
        let mut pending = HashMap::from([(
            "7".to_owned(),
            ("LEVELONE_OPTIONS".to_owned(), "SUBS".to_owned()),
        )]);
        let response = StreamResponse {
            service: "LEVELONE_OPTIONS".to_owned(),
            command: "SUBS".to_owned(),
            requestid: "7".to_owned(),
            schwab_client_correl_id: "correl".to_owned(),
            timestamp: 1,
            content: crate::StreamResponseContent {
                code: 0,
                msg: "OK".to_owned(),
            },
        };
        acknowledge(&mut pending, &[response]).unwrap();
        assert!(pending.is_empty());
    }
}
