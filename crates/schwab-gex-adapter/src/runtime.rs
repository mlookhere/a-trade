use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::{
    AdapterError, GammaQuality, ProfileAssignment, ProfilePool, RebootstrapSchedule, RefreshError,
    ReliableGexSurface, SchwabGexState, SchwabProfileConfig, StreamApply, StreamDataBatch,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSubscriptionPlan {
    pub profile_id: String,
    pub underlyings: Vec<String>,
    pub option_symbols: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DispatchReport {
    pub applied: usize,
    pub no_change: usize,
    pub stale_ignored: usize,
    pub unknown_contracts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeQuality {
    UnknownUnderlying,
    ProfileDisabled,
    StreamUnavailable,
    RebootstrapRequired,
    Gamma(GammaQuality),
}

struct UnderlyingRuntime {
    profile_id: String,
    state: SchwabGexState,
    refresh: RebootstrapSchedule,
}

/// Central transport/data-supervisor state for multiple Schwab API profiles.
///
/// One authoritative `SchwabGexState` exists per underlying. Profile streams only deliver updates
/// into that central state; they cannot create competing GEX books for the same underlying.
pub struct SchwabFleetRuntime {
    pool: ProfilePool,
    refresh_interval_ms: u64,
    underlyings: HashMap<String, UnderlyingRuntime>,
    option_owner: HashMap<(String, String), String>,
    connected_profiles: HashSet<String>,
    uncertain_underlyings: HashSet<String>,
}

impl SchwabFleetRuntime {
    pub fn new(
        profiles: Vec<SchwabProfileConfig>,
        refresh_interval_ms: u64,
    ) -> Result<Self, AdapterError> {
        if refresh_interval_ms == 0 {
            return Err(AdapterError::InvalidInput("option-chain refresh interval"));
        }
        Ok(Self {
            pool: ProfilePool::new(profiles)?,
            refresh_interval_ms,
            underlyings: HashMap::new(),
            option_owner: HashMap::new(),
            connected_profiles: HashSet::new(),
            uncertain_underlyings: HashSet::new(),
        })
    }

    #[must_use]
    pub fn profile_count(&self) -> usize {
        self.pool.profile_count()
    }

    /// Atomically replace the authoritative option universe for one underlying.
    ///
    /// All option-ownership collisions and refresh metadata are validated before the previous
    /// universe is mutated. A failed bootstrap therefore cannot leave a partially replaced book or
    /// option-symbol index behind.
    pub fn install_bootstrap(
        &mut self,
        state: SchwabGexState,
        now_ms: u64,
    ) -> Result<ProfileAssignment, AdapterError> {
        let underlying = state.underlying().to_owned();
        let previous_assignment = self.pool.assigned_profile(&underlying).map(str::to_owned);
        let assignment = self.pool.assign(&underlying)?;
        let profile_id = assignment.profile_id.clone();
        let assignment_changed = previous_assignment.as_deref() != Some(profile_id.as_str());
        let symbols = state.contract_symbols();

        let mut refresh =
            RebootstrapSchedule::new(self.refresh_interval_ms).map_err(map_refresh_error)?;
        if let Err(error) = refresh.record_bootstrap(now_ms).map_err(map_refresh_error) {
            if assignment_changed {
                self.pool.release(&underlying);
            }
            return Err(error);
        }

        for symbol in &symbols {
            let key = (profile_id.clone(), symbol.clone());
            if self
                .option_owner
                .get(&key)
                .is_some_and(|owner| owner != &underlying)
            {
                if assignment_changed {
                    self.pool.release(&underlying);
                }
                return Err(AdapterError::ProviderContract(format!(
                    "duplicate option ownership for {symbol}"
                )));
            }
        }

        self.remove_option_index(&underlying);
        for symbol in symbols {
            self.option_owner
                .insert((profile_id.clone(), symbol), underlying.clone());
        }
        self.underlyings.insert(
            underlying.clone(),
            UnderlyingRuntime {
                profile_id,
                state,
                refresh,
            },
        );
        self.uncertain_underlyings.remove(&underlying);
        Ok(assignment)
    }

    pub fn mark_stream_connected(&mut self, profile_id: &str) -> Result<(), AdapterError> {
        if !self.pool.is_enabled(profile_id) {
            return Err(AdapterError::UnknownProfile);
        }
        self.connected_profiles.insert(profile_id.to_owned());
        Ok(())
    }

    pub fn mark_stream_disconnected(&mut self, profile_id: &str) -> Result<(), AdapterError> {
        self.pool.client(profile_id)?;
        self.connected_profiles.remove(profile_id);
        for runtime in self
            .underlyings
            .values_mut()
            .filter(|runtime| runtime.profile_id == profile_id)
        {
            runtime.state.on_stream_disconnect();
        }
        Ok(())
    }

    #[must_use]
    pub fn subscription_plan(&self, profile_id: &str) -> Option<ProfileSubscriptionPlan> {
        if !self.pool.is_enabled(profile_id) {
            return None;
        }
        let mut underlyings = self
            .underlyings
            .iter()
            .filter(|(_, runtime)| runtime.profile_id == profile_id)
            .map(|(underlying, _)| underlying.clone())
            .collect::<Vec<_>>();
        underlyings.sort();

        let mut option_symbols = self
            .option_owner
            .iter()
            .filter(|((owner_profile, _), _)| owner_profile == profile_id)
            .map(|((_, symbol), _)| symbol.clone())
            .collect::<Vec<_>>();
        option_symbols.sort();

        Some(ProfileSubscriptionPlan {
            profile_id: profile_id.to_owned(),
            underlyings,
            option_symbols,
        })
    }

    pub fn apply_data_batch(
        &mut self,
        profile_id: &str,
        batch: &StreamDataBatch,
    ) -> Result<DispatchReport, AdapterError> {
        if !self.connected_profiles.contains(profile_id) {
            return Err(AdapterError::StreamUnavailable);
        }
        let result = match batch.service.as_str() {
            "LEVELONE_OPTIONS" => self.apply_options(profile_id, &batch.content),
            "LEVELONE_EQUITIES" => self.apply_underlyings(profile_id, &batch.content),
            _ => Err(AdapterError::ProviderContract(format!(
                "unsupported market-data service {}",
                batch.service
            ))),
        };
        if result.is_err() {
            // Any malformed or conflicting provider message makes every state fed by this stream
            // uncertain until explicitly reconciled. Keeping the prior surface apparently reliable
            // after an integrity error would violate the fail-closed data contract (§16).
            self.mark_profile_uncertain(profile_id);
        }
        result
    }

    pub fn reliable_surface(
        &self,
        underlying: &str,
        now_unix_ms: i64,
        gex_stale_timeout_ms: u64,
        market_data_stale_timeout_ms: u64,
    ) -> Result<ReliableGexSurface, RuntimeQuality> {
        let runtime = self
            .underlyings
            .get(underlying)
            .ok_or(RuntimeQuality::UnknownUnderlying)?;
        if !self.pool.is_enabled(&runtime.profile_id) {
            return Err(RuntimeQuality::ProfileDisabled);
        }
        if self.pool.assigned_profile(underlying) != Some(runtime.profile_id.as_str()) {
            return Err(RuntimeQuality::RebootstrapRequired);
        }
        if !self.connected_profiles.contains(&runtime.profile_id) {
            return Err(RuntimeQuality::StreamUnavailable);
        }
        if self.uncertain_underlyings.contains(underlying) {
            return Err(RuntimeQuality::RebootstrapRequired);
        }
        runtime
            .state
            .reliable_surface(
                now_unix_ms,
                gex_stale_timeout_ms,
                market_data_stale_timeout_ms,
            )
            .map_err(RuntimeQuality::Gamma)
    }

    #[must_use]
    pub fn should_rebootstrap(&self, underlying: &str, now_ms: u64) -> bool {
        let Some(runtime) = self.underlyings.get(underlying) else {
            return true;
        };
        self.pool.assigned_profile(underlying) != Some(runtime.profile_id.as_str())
            || self.uncertain_underlyings.contains(underlying)
            || runtime.refresh.should_rebootstrap(now_ms, &runtime.state)
    }

    pub fn disable_profile(&mut self, profile_id: &str) -> Result<Vec<String>, AdapterError> {
        self.connected_profiles.remove(profile_id);
        let released = self.pool.disable_profile(profile_id)?;
        for underlying in &released {
            self.remove_underlying(underlying);
        }
        Ok(released)
    }

    pub fn enable_profile(&mut self, profile_id: &str) -> Result<(), AdapterError> {
        self.pool.enable_profile(profile_id)
    }

    fn apply_options(
        &mut self,
        profile_id: &str,
        contents: &[Value],
    ) -> Result<DispatchReport, AdapterError> {
        let mut report = DispatchReport::default();
        for content in contents {
            let object = content.as_object().ok_or_else(|| {
                AdapterError::ProviderContract("option stream content is not an object".to_owned())
            })?;
            validate_present_option_side(object)?;
            let symbol = object.get("key").and_then(Value::as_str).ok_or_else(|| {
                AdapterError::ProviderContract("option stream key missing".to_owned())
            })?;

            let owner = self
                .option_owner
                .get(&(profile_id.to_owned(), symbol.to_owned()))
                .cloned();
            let underlying =
                owner.or_else(|| object.get("22").and_then(Value::as_str).map(str::to_owned));

            let Some(underlying) = underlying else {
                self.mark_profile_uncertain(profile_id);
                report.unknown_contracts += 1;
                continue;
            };
            let Some(runtime) = self.underlyings.get_mut(&underlying) else {
                self.mark_profile_uncertain(profile_id);
                report.unknown_contracts += 1;
                continue;
            };
            if runtime.profile_id != profile_id {
                self.uncertain_underlyings.insert(underlying);
                return Err(AdapterError::ProviderContract(
                    "option update arrived on non-owning profile".to_owned(),
                ));
            }
            match runtime.state.apply_option_stream_content(content)? {
                StreamApply::Applied => report.applied += 1,
                StreamApply::NoChange => report.no_change += 1,
                StreamApply::StaleIgnored => report.stale_ignored += 1,
                StreamApply::UnknownContract => {
                    self.uncertain_underlyings.insert(underlying);
                    report.unknown_contracts += 1;
                }
            }
        }
        Ok(report)
    }

    fn apply_underlyings(
        &mut self,
        profile_id: &str,
        contents: &[Value],
    ) -> Result<DispatchReport, AdapterError> {
        let mut report = DispatchReport::default();
        for content in contents {
            let underlying = content
                .get("key")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AdapterError::ProviderContract("underlying stream key missing".to_owned())
                })?
                .to_owned();
            let Some(runtime) = self.underlyings.get_mut(&underlying) else {
                self.mark_profile_uncertain(profile_id);
                report.unknown_contracts += 1;
                continue;
            };
            if runtime.profile_id != profile_id {
                self.uncertain_underlyings.insert(underlying);
                return Err(AdapterError::ProviderContract(
                    "underlying update arrived on non-owning profile".to_owned(),
                ));
            }
            match runtime.state.apply_underlying_stream_content(content)? {
                StreamApply::Applied => report.applied += 1,
                StreamApply::NoChange => report.no_change += 1,
                StreamApply::StaleIgnored => report.stale_ignored += 1,
                StreamApply::UnknownContract => report.unknown_contracts += 1,
            }
        }
        Ok(report)
    }

    fn mark_profile_uncertain(&mut self, profile_id: &str) {
        self.uncertain_underlyings.extend(
            self.underlyings
                .iter()
                .filter(|(_, runtime)| runtime.profile_id == profile_id)
                .map(|(underlying, _)| underlying.clone()),
        );
    }

    fn remove_underlying(&mut self, underlying: &str) {
        self.remove_option_index(underlying);
        self.underlyings.remove(underlying);
        self.uncertain_underlyings.remove(underlying);
    }

    fn remove_option_index(&mut self, underlying: &str) {
        self.option_owner
            .retain(|_, owner_underlying| owner_underlying != underlying);
    }
}

fn validate_present_option_side(object: &Map<String, Value>) -> Result<(), AdapterError> {
    let Some(value) = object.get("21") else {
        return Ok(());
    };
    let side = value.as_str().ok_or_else(|| {
        AdapterError::ProviderContract("stream field 21 is not a string".to_owned())
    })?;
    if !matches!(side, "C" | "CALL" | "P" | "PUT") {
        return Err(AdapterError::ProviderContract(
            "stream field 21 contains an unknown contract type".to_owned(),
        ));
    }
    Ok(())
}

fn map_refresh_error(error: RefreshError) -> AdapterError {
    AdapterError::ProviderContract(format!("refresh schedule error: {error:?}"))
}
