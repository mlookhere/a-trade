use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::{AdapterError, rate::RestRateLimiter, state::SchwabGexState};

const OPTION_CHAIN_URL: &str = "https://api.schwabapi.com/marketdata/v1/chains";
const USER_PREFERENCE_URL: &str = "https://api.schwabapi.com/trader/v1/userPreference";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamerInfo {
    pub streamer_socket_url: String,
    pub schwab_client_customer_id: String,
    pub schwab_client_correl_id: String,
    pub schwab_client_channel: String,
    pub schwab_client_function_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPreferenceResponse {
    streamer_info: Vec<StreamerInfo>,
}

#[derive(Clone)]
pub struct SchwabRestClient {
    http: Client,
    limiter: RestRateLimiter,
}

impl SchwabRestClient {
    pub fn new(limiter: RestRateLimiter, transport_timeout_ms: u64) -> Result<Self, AdapterError> {
        if transport_timeout_ms == 0 {
            return Err(AdapterError::InvalidInput("transport timeout"));
        }
        let http = Client::builder()
            .user_agent("a-trade-schwab-gex/0.1")
            .timeout(Duration::from_millis(transport_timeout_ms))
            .build()
            .map_err(|error| AdapterError::Transport(error.to_string()))?;
        Ok(Self { http, limiter })
    }

    pub async fn bootstrap_gex(
        &self,
        access_token: &str,
        underlying: &str,
    ) -> Result<SchwabGexState, AdapterError> {
        if access_token.trim().is_empty() {
            return Err(AdapterError::InvalidInput("access token"));
        }
        if underlying.trim().is_empty() {
            return Err(AdapterError::InvalidInput("underlying"));
        }
        self.limiter.acquire().await.map_err(AdapterError::from)?;
        let response = self
            .http
            .get(OPTION_CHAIN_URL)
            .bearer_auth(access_token)
            .query(&[
                ("symbol", underlying),
                ("contractType", "ALL"),
                ("includeUnderlyingQuote", "true"),
                ("strategy", "SINGLE"),
            ])
            .send()
            .await
            .map_err(map_transport_error)?
            .error_for_status()
            .map_err(|error| AdapterError::Provider(error.to_string()))?;
        let body = response.text().await.map_err(map_transport_error)?;
        SchwabGexState::from_option_chain_json(underlying, &body)
    }

    pub async fn streamer_info(&self, access_token: &str) -> Result<StreamerInfo, AdapterError> {
        if access_token.trim().is_empty() {
            return Err(AdapterError::InvalidInput("access token"));
        }
        self.limiter.acquire().await.map_err(AdapterError::from)?;
        let response = self
            .http
            .get(USER_PREFERENCE_URL)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(map_transport_error)?
            .error_for_status()
            .map_err(|error| AdapterError::Provider(error.to_string()))?;
        let payload: UserPreferenceResponse = response
            .json()
            .await
            .map_err(map_transport_or_contract_error)?;
        let info = payload.streamer_info.into_iter().next().ok_or_else(|| {
            AdapterError::ProviderContract("userPreference omitted streamerInfo".to_owned())
        })?;
        validate_streamer_info(&info)?;
        Ok(info)
    }
}

fn map_transport_error(error: reqwest::Error) -> AdapterError {
    if error.is_timeout() {
        AdapterError::TransportTimeout
    } else {
        AdapterError::Transport(error.to_string())
    }
}

fn map_transport_or_contract_error(error: reqwest::Error) -> AdapterError {
    if error.is_timeout() {
        AdapterError::TransportTimeout
    } else {
        AdapterError::ProviderContract(error.to_string())
    }
}

fn validate_streamer_info(info: &StreamerInfo) -> Result<(), AdapterError> {
    if info.streamer_socket_url.trim().is_empty()
        || info.schwab_client_customer_id.trim().is_empty()
        || info.schwab_client_correl_id.trim().is_empty()
        || info.schwab_client_channel.trim().is_empty()
        || info.schwab_client_function_id.trim().is_empty()
    {
        return Err(AdapterError::ProviderContract(
            "streamerInfo omitted required fields".to_owned(),
        ));
    }
    let url = url::Url::parse(&info.streamer_socket_url)
        .map_err(|error| AdapterError::ProviderContract(error.to_string()))?;
    if url.scheme() != "wss" {
        return Err(AdapterError::ProviderContract(
            "streamerSocketUrl must use wss".to_owned(),
        ));
    }
    Ok(())
}
