use std::{fmt, path::Path, time::Duration};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{AdapterError, config::SchwabProfileConfig, rate::RestRateLimiter};

const AUTHORIZE_URL: &str = "https://api.schwabapi.com/v1/oauth/authorize";
const TOKEN_URL: &str = "https://api.schwabapi.com/v1/oauth/token";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenSet {
    #[serde(skip_serializing_if = "String::is_empty")]
    access_token: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    refresh_token: String,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
    obtained_at_unix_ms: u64,
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenSet")
            .field("access_token", &"***REDACTED***")
            .field("refresh_token", &"***REDACTED***")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .field("obtained_at_unix_ms", &self.obtained_at_unix_ms)
            .finish()
    }
}

impl TokenSet {
    #[must_use]
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    #[must_use]
    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }

    #[must_use]
    pub fn access_expired_at(&self, now_unix_ms: u64) -> bool {
        let lifetime_ms = self.expires_in.saturating_mul(1_000);
        now_unix_ms >= self.obtained_at_unix_ms.saturating_add(lifetime_ms)
    }

    fn validate(&self) -> Result<(), AdapterError> {
        if self.access_token.trim().is_empty()
            || self.refresh_token.trim().is_empty()
            || self.token_type.trim().is_empty()
            || self.expires_in == 0
        {
            return Err(AdapterError::TokenStore(
                "token set missing required fields".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
}

#[derive(Clone)]
pub struct OAuthClient {
    http: Client,
    config: SchwabProfileConfig,
    limiter: RestRateLimiter,
}

impl OAuthClient {
    pub fn new(
        config: SchwabProfileConfig,
        limiter: RestRateLimiter,
    ) -> Result<Self, AdapterError> {
        let http = Client::builder()
            .user_agent("a-trade-schwab-gex/0.1")
            .timeout(Duration::from_millis(config.transport_timeout_ms))
            .build()
            .map_err(|error| AdapterError::Transport(error.to_string()))?;
        Ok(Self {
            http,
            config,
            limiter,
        })
    }

    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.config.profile_id
    }

    pub fn authorization_url(&self, state: &str) -> Result<String, AdapterError> {
        if state.trim().is_empty() {
            return Err(AdapterError::InvalidInput("oauth state"));
        }
        let mut url = url::Url::parse(AUTHORIZE_URL)
            .map_err(|error| AdapterError::ProviderContract(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("client_id", self.config.client_id.expose())
            .append_pair("redirect_uri", &self.config.callback_url)
            .append_pair("state", state);
        Ok(url.into())
    }

    pub async fn exchange_authorization_code(
        &self,
        code: &str,
        now_unix_ms: u64,
    ) -> Result<TokenSet, AdapterError> {
        if code.trim().is_empty() {
            return Err(AdapterError::InvalidInput("authorization code"));
        }
        self.limiter.acquire().await.map_err(AdapterError::from)?;
        let response = self
            .http
            .post(TOKEN_URL)
            .basic_auth(
                self.config.client_id.expose(),
                Some(self.config.client_secret.expose()),
            )
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.config.callback_url.as_str()),
            ])
            .send()
            .await
            .map_err(map_transport_error)?
            .error_for_status()
            .map_err(|error| AdapterError::Provider(error.to_string()))?;
        let mut payload: TokenResponse = response
            .json()
            .await
            .map_err(map_transport_or_contract_error)?;
        let refresh_token = payload
            .refresh_token
            .take()
            .filter(|value| !value.trim().is_empty())
            .ok_or(AdapterError::ProviderContract(
                "authorization response omitted refresh_token".to_owned(),
            ))?;
        build_token_set(payload, refresh_token, now_unix_ms)
    }

    pub async fn refresh_access_token(
        &self,
        current: &TokenSet,
        now_unix_ms: u64,
    ) -> Result<TokenSet, AdapterError> {
        current.validate()?;
        self.limiter.acquire().await.map_err(AdapterError::from)?;
        let response = self
            .http
            .post(TOKEN_URL)
            .basic_auth(
                self.config.client_id.expose(),
                Some(self.config.client_secret.expose()),
            )
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", current.refresh_token.as_str()),
            ])
            .send()
            .await
            .map_err(map_transport_error)?
            .error_for_status()
            .map_err(|error| AdapterError::Provider(error.to_string()))?;
        let mut payload: TokenResponse = response
            .json()
            .await
            .map_err(map_transport_or_contract_error)?;
        let refresh_token = payload
            .refresh_token
            .take()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| current.refresh_token.clone());
        build_token_set(payload, refresh_token, now_unix_ms)
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

fn build_token_set(
    payload: TokenResponse,
    refresh_token: String,
    obtained_at_unix_ms: u64,
) -> Result<TokenSet, AdapterError> {
    if payload.access_token.trim().is_empty()
        || refresh_token.trim().is_empty()
        || payload.token_type.trim().is_empty()
        || payload.expires_in == 0
    {
        return Err(AdapterError::ProviderContract(
            "token response missing required fields".to_owned(),
        ));
    }
    let tokens = TokenSet {
        access_token: payload.access_token,
        refresh_token,
        token_type: payload.token_type,
        expires_in: payload.expires_in,
        scope: payload.scope,
        obtained_at_unix_ms,
    };
    tokens.validate().map_err(|_| {
        AdapterError::ProviderContract("token response missing required fields".to_owned())
    })?;
    Ok(tokens)
}

pub async fn load_tokens(path: &Path) -> Result<TokenSet, AdapterError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| AdapterError::TokenStore(error.to_string()))?;
    let tokens: TokenSet = serde_json::from_slice(&bytes)
        .map_err(|error| AdapterError::TokenStore(error.to_string()))?;
    tokens.validate()?;
    Ok(tokens)
}

pub async fn save_tokens(path: &Path, tokens: &TokenSet) -> Result<(), AdapterError> {
    tokens.validate()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| AdapterError::TokenStore(error.to_string()))?;
    }
    let bytes =
        serde_json::to_vec(tokens).map_err(|error| AdapterError::TokenStore(error.to_string()))?;
    let temporary = path.with_extension("tmp");
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| AdapterError::TokenStore(error.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        tokio::fs::set_permissions(&temporary, permissions)
            .await
            .map_err(|error| AdapterError::TokenStore(error.to_string()))?;
    }

    tokio::fs::rename(&temporary, path)
        .await
        .map_err(|error| AdapterError::TokenStore(error.to_string()))
}
