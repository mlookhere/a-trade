use std::{collections::HashSet, time::Duration};

use futures_util::{StreamExt, stream::FuturesUnordered};
use tokio::time::timeout;

use crate::{
    AdapterError, SchwabStreamClient, SecretString, StreamEvent, StreamRequestFactory,
    StreamerInfo, require_response_success,
};

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
        let timeout_duration = Duration::from_millis(request.transport_timeout_ms);

        timeout(timeout_duration, Self::connect_and_login(request))
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
                    let Some(response) = responses
                        .iter()
                        .find(|response| response.requestid == request_id)
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

/// Connect independent Schwab streamer identities concurrently without a software profile limit.
///
/// Schwab's current streamer contract documents a maximum of one streamer connection per user.
/// `schwab_client_customer_id` is therefore treated as the provider user identity for connection
/// deduplication: multiple API applications resolving to the same customer ID are not dialed in
/// parallel. Distinct customer IDs may be connected concurrently, subject to provider enforcement.
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
