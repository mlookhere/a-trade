use std::{env, fmt, path::PathBuf};

#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Result<Self, ConfigError> {
        if value.trim().is_empty() {
            return Err(ConfigError::MissingSecret);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("***REDACTED***")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchwabConfig {
    pub client_id: SecretString,
    pub client_secret: SecretString,
    pub callback_url: String,
    pub token_path: PathBuf,
    pub rest_requests_per_minute: u32,
    pub rest_headroom_requests_per_minute: u32,
    pub gex_stale_timeout_ms: u64,
    pub market_data_stale_timeout_ms: u64,
    pub option_chain_refresh_interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Missing(&'static str),
    MissingSecret,
    InvalidNumber(&'static str),
    InvalidCallbackUrl,
    InvalidRestBudget,
}

impl SchwabConfig {
    /// Provider/deployment configuration only. No strategy threshold is defined here.
    ///
    /// `SCHWAB_REST_HEADROOM_REQUESTS_PER_MINUTE` is deliberately required separately from the
    /// provider cap so the adapter never invents a reserve percentage or silently consumes the
    /// full application allowance.
    pub fn from_env() -> Result<Self, ConfigError> {
        let client_id = SecretString::new(required("SCHWAB_CLIENT_ID")?)?;
        let client_secret = SecretString::new(required("SCHWAB_CLIENT_SECRET")?)?;
        let callback_url = required("SCHWAB_CALLBACK_URL")?;
        let parsed_callback = url::Url::parse(&callback_url).map_err(|_| ConfigError::InvalidCallbackUrl)?;
        if !matches!(parsed_callback.scheme(), "https" | "http") || parsed_callback.host_str().is_none() {
            return Err(ConfigError::InvalidCallbackUrl);
        }

        let token_path = PathBuf::from(required("SCHWAB_TOKEN_PATH")?);
        let rest_requests_per_minute = parse_u32("SCHWAB_REST_REQUESTS_PER_MINUTE")?;
        let rest_headroom_requests_per_minute =
            parse_u32("SCHWAB_REST_HEADROOM_REQUESTS_PER_MINUTE")?;
        if rest_requests_per_minute == 0
            || rest_headroom_requests_per_minute >= rest_requests_per_minute
        {
            return Err(ConfigError::InvalidRestBudget);
        }

        Ok(Self {
            client_id,
            client_secret,
            callback_url,
            token_path,
            rest_requests_per_minute,
            rest_headroom_requests_per_minute,
            gex_stale_timeout_ms: parse_u64("SCHWAB_GEX_STALE_TIMEOUT_MS")?,
            market_data_stale_timeout_ms: parse_u64("SCHWAB_MARKET_DATA_STALE_TIMEOUT_MS")?,
            option_chain_refresh_interval_ms: parse_u64(
                "SCHWAB_OPTION_CHAIN_REFRESH_INTERVAL_MS",
            )?,
        })
    }

    #[must_use]
    pub const fn effective_rest_limit(&self) -> u32 {
        self.rest_requests_per_minute - self.rest_headroom_requests_per_minute
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn parse_u32(name: &'static str) -> Result<u32, ConfigError> {
    required(name)?
        .parse()
        .map_err(|_| ConfigError::InvalidNumber(name))
}

fn parse_u64(name: &'static str) -> Result<u64, ConfigError> {
    let value: u64 = required(name)?
        .parse()
        .map_err(|_| ConfigError::InvalidNumber(name))?;
    if value == 0 {
        return Err(ConfigError::InvalidNumber(name));
    }
    Ok(value)
}
