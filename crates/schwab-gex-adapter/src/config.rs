use std::{env, fmt, fs, path::PathBuf};

use serde::Deserialize;

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

/// One independent Schwab API application/token/streaming identity.
///
/// The adapter intentionally places no software maximum on the number of profiles. Provider-side
/// connection/application/account limits remain external constraints and are never inferred here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchwabProfileConfig {
    pub profile_id: String,
    pub client_id: SecretString,
    pub client_secret: SecretString,
    pub callback_url: String,
    pub token_path: PathBuf,
    pub rest_requests_per_minute: u32,
    pub rest_headroom_requests_per_minute: u32,
    pub transport_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchwabConfig {
    pub profiles: Vec<SchwabProfileConfig>,
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
    InvalidTokenPath,
    InvalidRestBudget,
    InvalidProfilesFile,
    EmptyProfiles,
    MissingProfileId,
    DuplicateProfileId,
    DuplicateTokenPath,
    InvalidTimeout,
}

#[derive(Debug, Deserialize)]
struct ProfilesFile {
    profiles: Vec<ProfileFileEntry>,
}

#[derive(Debug, Deserialize)]
struct ProfileFileEntry {
    profile_id: String,
    client_id: String,
    client_secret: String,
    callback_url: String,
    token_path: PathBuf,
    rest_requests_per_minute: u32,
    rest_headroom_requests_per_minute: u32,
    transport_timeout_ms: u64,
}

impl SchwabProfileConfig {
    fn validate(self) -> Result<Self, ConfigError> {
        if self.profile_id.trim().is_empty() {
            return Err(ConfigError::MissingProfileId);
        }
        validate_callback(&self.callback_url)?;
        if self.token_path.as_os_str().is_empty() {
            return Err(ConfigError::InvalidTokenPath);
        }
        validate_budget(
            self.rest_requests_per_minute,
            self.rest_headroom_requests_per_minute,
        )?;
        if self.transport_timeout_ms == 0 {
            return Err(ConfigError::InvalidTimeout);
        }
        Ok(self)
    }

    #[must_use]
    pub const fn effective_rest_limit(&self) -> u32 {
        self.rest_requests_per_minute - self.rest_headroom_requests_per_minute
    }
}

impl SchwabConfig {
    /// Runtime/provider configuration only. No strategy threshold is defined here.
    ///
    /// Multi-profile mode is enabled by `SCHWAB_PROFILES_FILE`, a runtime JSON file containing an
    /// arbitrary-length `profiles` array. Secrets remain outside the repository. If that variable
    /// is absent, the legacy single-profile environment variables remain supported.
    pub fn from_env() -> Result<Self, ConfigError> {
        let profiles = match env::var("SCHWAB_PROFILES_FILE") {
            Ok(path) if !path.trim().is_empty() => load_profiles_file(PathBuf::from(path))?,
            _ => vec![single_profile_from_env()?],
        };
        Self::new(
            profiles,
            parse_u64("SCHWAB_GEX_STALE_TIMEOUT_MS")?,
            parse_u64("SCHWAB_MARKET_DATA_STALE_TIMEOUT_MS")?,
            parse_u64("SCHWAB_OPTION_CHAIN_REFRESH_INTERVAL_MS")?,
        )
    }

    pub fn new(
        profiles: Vec<SchwabProfileConfig>,
        gex_stale_timeout_ms: u64,
        market_data_stale_timeout_ms: u64,
        option_chain_refresh_interval_ms: u64,
    ) -> Result<Self, ConfigError> {
        validate_profile_set(&profiles)?;
        if gex_stale_timeout_ms == 0
            || market_data_stale_timeout_ms == 0
            || option_chain_refresh_interval_ms == 0
        {
            return Err(ConfigError::InvalidTimeout);
        }
        Ok(Self {
            profiles,
            gex_stale_timeout_ms,
            market_data_stale_timeout_ms,
            option_chain_refresh_interval_ms,
        })
    }

    #[must_use]
    pub fn profile(&self, profile_id: &str) -> Option<&SchwabProfileConfig> {
        self.profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
    }
}

fn load_profiles_file(path: PathBuf) -> Result<Vec<SchwabProfileConfig>, ConfigError> {
    let bytes = fs::read(path).map_err(|_| ConfigError::InvalidProfilesFile)?;
    let file: ProfilesFile =
        serde_json::from_slice(&bytes).map_err(|_| ConfigError::InvalidProfilesFile)?;
    if file.profiles.is_empty() {
        return Err(ConfigError::EmptyProfiles);
    }
    file.profiles
        .into_iter()
        .map(|entry| {
            SchwabProfileConfig {
                profile_id: entry.profile_id,
                client_id: SecretString::new(entry.client_id)?,
                client_secret: SecretString::new(entry.client_secret)?,
                callback_url: entry.callback_url,
                token_path: entry.token_path,
                rest_requests_per_minute: entry.rest_requests_per_minute,
                rest_headroom_requests_per_minute: entry.rest_headroom_requests_per_minute,
                transport_timeout_ms: entry.transport_timeout_ms,
            }
            .validate()
        })
        .collect()
}

fn single_profile_from_env() -> Result<SchwabProfileConfig, ConfigError> {
    SchwabProfileConfig {
        profile_id: env::var("SCHWAB_PROFILE_ID").unwrap_or_else(|_| "default".to_owned()),
        client_id: SecretString::new(required("SCHWAB_CLIENT_ID")?)?,
        client_secret: SecretString::new(required("SCHWAB_CLIENT_SECRET")?)?,
        callback_url: required("SCHWAB_CALLBACK_URL")?,
        token_path: PathBuf::from(required("SCHWAB_TOKEN_PATH")?),
        rest_requests_per_minute: parse_u32("SCHWAB_REST_REQUESTS_PER_MINUTE")?,
        rest_headroom_requests_per_minute: parse_u32("SCHWAB_REST_HEADROOM_REQUESTS_PER_MINUTE")?,
        transport_timeout_ms: parse_u64("SCHWAB_TRANSPORT_TIMEOUT_MS")?,
    }
    .validate()
}

fn validate_profile_set(profiles: &[SchwabProfileConfig]) -> Result<(), ConfigError> {
    if profiles.is_empty() {
        return Err(ConfigError::EmptyProfiles);
    }
    for profile in profiles {
        profile.clone().validate()?;
    }
    for (index, profile) in profiles.iter().enumerate() {
        for other in &profiles[..index] {
            if profile.profile_id == other.profile_id {
                return Err(ConfigError::DuplicateProfileId);
            }
            if profile.token_path == other.token_path {
                return Err(ConfigError::DuplicateTokenPath);
            }
        }
    }
    Ok(())
}

fn validate_callback(callback_url: &str) -> Result<(), ConfigError> {
    let parsed = url::Url::parse(callback_url).map_err(|_| ConfigError::InvalidCallbackUrl)?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(ConfigError::InvalidCallbackUrl);
    }
    Ok(())
}

fn validate_budget(provider_limit: u32, reserved_headroom: u32) -> Result<(), ConfigError> {
    if provider_limit == 0 || reserved_headroom >= provider_limit {
        return Err(ConfigError::InvalidRestBudget);
    }
    Ok(())
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
