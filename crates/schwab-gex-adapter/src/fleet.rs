use std::collections::{HashMap, HashSet};

use crate::{AdapterError, OAuthClient, RestRateLimiter, SchwabProfileConfig, SchwabRestClient};

#[derive(Clone)]
pub struct ProfileClients {
    pub profile_id: String,
    pub config: SchwabProfileConfig,
    pub limiter: RestRateLimiter,
    pub oauth: OAuthClient,
    pub rest: SchwabRestClient,
}

impl ProfileClients {
    pub fn build(config: SchwabProfileConfig) -> Result<Self, AdapterError> {
        let limiter = RestRateLimiter::new(
            config.rest_requests_per_minute,
            config.rest_headroom_requests_per_minute,
        )?;
        let oauth = OAuthClient::new(config.clone(), limiter.clone())?;
        let rest = SchwabRestClient::new(limiter.clone(), config.transport_timeout_ms)?;
        Ok(Self {
            profile_id: config.profile_id.clone(),
            config,
            limiter,
            oauth,
            rest,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileAssignment {
    pub underlying: String,
    pub profile_id: String,
}

/// Arbitrary-length Schwab application pool.
///
/// Each profile owns its own OAuth identity, token path, REST limiter and streamer connection.
/// Underlyings are assigned to one profile at a time so multiple credentials scale data transport
/// without allowing duplicate provider streams to silently create conflicting state.
#[derive(Clone)]
pub struct ProfilePool {
    clients: Vec<ProfileClients>,
    disabled: HashSet<String>,
    assignments: HashMap<String, usize>,
    next_index: usize,
}

impl ProfilePool {
    pub fn new(configs: Vec<SchwabProfileConfig>) -> Result<Self, AdapterError> {
        if configs.is_empty() {
            return Err(AdapterError::NoHealthyProfile);
        }
        let clients = configs
            .into_iter()
            .map(ProfileClients::build)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            clients,
            disabled: HashSet::new(),
            assignments: HashMap::new(),
            next_index: 0,
        })
    }

    #[must_use]
    pub fn profile_count(&self) -> usize {
        self.clients.len()
    }

    #[must_use]
    pub fn clients(&self) -> &[ProfileClients] {
        &self.clients
    }

    pub fn client(&self, profile_id: &str) -> Result<&ProfileClients, AdapterError> {
        self.clients
            .iter()
            .find(|client| client.profile_id == profile_id)
            .ok_or(AdapterError::UnknownProfile)
    }

    pub fn assign(&mut self, underlying: &str) -> Result<ProfileAssignment, AdapterError> {
        let underlying = underlying.trim();
        if underlying.is_empty() {
            return Err(AdapterError::InvalidInput("underlying"));
        }

        if let Some(&index) = self.assignments.get(underlying) {
            let client = &self.clients[index];
            if !self.disabled.contains(&client.profile_id) {
                return Ok(ProfileAssignment {
                    underlying: underlying.to_owned(),
                    profile_id: client.profile_id.clone(),
                });
            }
        }

        let count = self.clients.len();
        for offset in 0..count {
            let index = (self.next_index + offset) % count;
            let client = &self.clients[index];
            if self.disabled.contains(&client.profile_id) {
                continue;
            }
            self.assignments.insert(underlying.to_owned(), index);
            self.next_index = (index + 1) % count;
            return Ok(ProfileAssignment {
                underlying: underlying.to_owned(),
                profile_id: client.profile_id.clone(),
            });
        }

        Err(AdapterError::NoHealthyProfile)
    }

    pub fn release(&mut self, underlying: &str) {
        self.assignments.remove(underlying.trim());
    }

    /// Disable a transport identity after profile-scoped OAuth/provider/stream failure.
    ///
    /// Returns the underlyings whose assignments were cleared so the supervisor can explicitly
    /// rebuild them on another profile. Cross-profile reassignment is never hidden from audit.
    pub fn disable_profile(&mut self, profile_id: &str) -> Result<Vec<String>, AdapterError> {
        if !self
            .clients
            .iter()
            .any(|client| client.profile_id == profile_id)
        {
            return Err(AdapterError::UnknownProfile);
        }
        self.disabled.insert(profile_id.to_owned());

        let mut released = Vec::new();
        self.assignments.retain(|underlying, index| {
            if self.clients[*index].profile_id == profile_id {
                released.push(underlying.clone());
                false
            } else {
                true
            }
        });
        released.sort();
        Ok(released)
    }

    pub fn enable_profile(&mut self, profile_id: &str) -> Result<(), AdapterError> {
        if !self
            .clients
            .iter()
            .any(|client| client.profile_id == profile_id)
        {
            return Err(AdapterError::UnknownProfile);
        }
        self.disabled.remove(profile_id);
        Ok(())
    }

    #[must_use]
    pub fn is_enabled(&self, profile_id: &str) -> bool {
        self.clients
            .iter()
            .any(|client| client.profile_id == profile_id)
            && !self.disabled.contains(profile_id)
    }

    #[must_use]
    pub fn assigned_profile(&self, underlying: &str) -> Option<&str> {
        let index = *self.assignments.get(underlying.trim())?;
        self.clients
            .get(index)
            .map(|client| client.profile_id.as_str())
    }
}
