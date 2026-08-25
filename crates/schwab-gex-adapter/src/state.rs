use std::collections::{BTreeMap, HashMap};

use gex_engine::{ContractGexInput, GexBook, GexSnapshot, OptionSide};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::AdapterError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamApply {
    Applied,
    NoChange,
    StaleIgnored,
    UnknownContract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GammaQuality {
    Reliable,
    Delayed,
    EmptyCoverage,
    CoverageIncomplete { expected: usize, hydrated: usize },
    RebootstrapRequired,
    ConflictingProviderState,
    GexStale,
    UnderlyingStale,
    FutureTimestamp,
}

#[derive(Debug, Clone)]
pub struct ReliableGexSurface {
    pub snapshot: GexSnapshot,
    pub expected_contracts: usize,
    pub hydrated_contracts: usize,
}

#[derive(Debug, Clone)]
struct ContractState {
    symbol: String,
    underlying: String,
    expiration: String,
    strike: f64,
    side: OptionSide,
    gamma: Option<f64>,
    open_interest: Option<u64>,
    multiplier: Option<f64>,
    quote_time_millis: Option<i64>,
}

impl ContractState {
    fn complete_input(&self) -> Option<ContractGexInput> {
        Some(ContractGexInput {
            symbol: self.symbol.clone(),
            underlying: self.underlying.clone(),
            expiration: self.expiration.clone(),
            strike: self.strike,
            side: self.side,
            gamma: self.gamma?,
            open_interest: self.open_interest?,
            multiplier: self.multiplier?,
            quote_time_millis: self.quote_time_millis?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SchwabGexState {
    underlying: String,
    contracts: HashMap<String, ContractState>,
    book: GexBook,
    expected_contracts: usize,
    delayed: bool,
    needs_rebootstrap: bool,
    conflict: bool,
    spot: f64,
    spot_time_millis: i64,
}

impl SchwabGexState {
    /// Parse a REST `/marketdata/v1/chains` response created with `includeUnderlyingQuote=true`.
    /// Missing GEX coefficient fields remain partial rather than being replaced with zero.
    pub fn from_option_chain_json(
        expected_underlying: &str,
        json: &str,
    ) -> Result<Self, AdapterError> {
        let expected_underlying = expected_underlying.trim();
        if expected_underlying.is_empty() {
            return Err(AdapterError::InvalidInput("underlying"));
        }
        let response: OptionChainResponse = serde_json::from_str(json)
            .map_err(|error| AdapterError::ProviderContract(error.to_string()))?;
        if response.status != "SUCCESS" {
            return Err(AdapterError::ProviderContract(format!(
                "option chain status is {}",
                response.status
            )));
        }
        if response.symbol != expected_underlying {
            return Err(AdapterError::UnderlyingMismatch);
        }
        let underlying_quote = response.underlying.ok_or_else(|| {
            AdapterError::ProviderContract("option chain omitted underlying quote".to_owned())
        })?;
        if underlying_quote.symbol != expected_underlying {
            return Err(AdapterError::UnderlyingMismatch);
        }
        if !underlying_quote.mark.is_finite() || underlying_quote.mark <= 0.0 {
            return Err(AdapterError::ProviderContract(
                "underlying mark is invalid".to_owned(),
            ));
        }
        if underlying_quote.quote_time < 0 {
            return Err(AdapterError::ProviderContract(
                "underlying quote timestamp is invalid".to_owned(),
            ));
        }

        let mut book = GexBook::new(expected_underlying)
            .map_err(|error| AdapterError::Gex(format!("{error:?}")))?;
        book.set_spot(underlying_quote.mark, underlying_quote.quote_time)
            .map_err(|error| AdapterError::Gex(format!("{error:?}")))?;

        let mut state = Self {
            underlying: expected_underlying.to_owned(),
            contracts: HashMap::new(),
            book,
            expected_contracts: 0,
            delayed: response.is_delayed || underlying_quote.delayed,
            needs_rebootstrap: false,
            conflict: false,
            spot: underlying_quote.mark,
            spot_time_millis: underlying_quote.quote_time,
        };

        state.ingest_map(response.call_exp_date_map, OptionSide::Call)?;
        state.ingest_map(response.put_exp_date_map, OptionSide::Put)?;
        if response
            .number_of_contracts
            .is_some_and(|reported| reported != state.expected_contracts)
        {
            state.needs_rebootstrap = true;
        }
        Ok(state)
    }

    #[must_use]
    pub fn underlying(&self) -> &str {
        &self.underlying
    }

    #[must_use]
    pub const fn expected_contracts(&self) -> usize {
        self.expected_contracts
    }

    #[must_use]
    pub fn hydrated_contracts(&self) -> usize {
        self.contracts
            .values()
            .filter(|contract| contract.complete_input().is_some())
            .count()
    }

    #[must_use]
    pub fn contract_symbols(&self) -> Vec<String> {
        let mut symbols = self.contracts.keys().cloned().collect::<Vec<_>>();
        symbols.sort();
        symbols
    }

    #[must_use]
    pub const fn needs_rebootstrap(&self) -> bool {
        self.needs_rebootstrap
    }

    /// A stream reconnect by itself does not trigger REST. This prevents reconnect storms; a
    /// re-bootstrap is driven only by explicit refresh scheduling or detected coverage drift.
    pub const fn on_stream_disconnect(&mut self) {}

    pub fn apply_option_stream_content(
        &mut self,
        content: &Value,
    ) -> Result<StreamApply, AdapterError> {
        let object = content.as_object().ok_or_else(|| {
            AdapterError::ProviderContract("option stream content is not an object".to_owned())
        })?;
        let symbol = string_field(object, "key")?;
        let Some(current) = self.contracts.get(symbol).cloned() else {
            self.needs_rebootstrap = true;
            return Ok(StreamApply::UnknownContract);
        };
        if bool_field_optional(object, "delayed") == Some(true) {
            self.delayed = true;
        }

        if let Some(quote_time) = i64_field_optional(object, "38")? {
            if quote_time < current.quote_time_millis.unwrap_or(i64::MIN) {
                return Ok(StreamApply::StaleIgnored);
            }
        }

        self.validate_static_option_fields(object, &current)?;
        let mut next = current.clone();
        let mut changed = false;
        if let Some(value) = f64_field_optional(object, "29")? {
            changed |= next.gamma != Some(value);
            next.gamma = Some(value);
        }
        if let Some(value) = u64_field_optional(object, "9")? {
            changed |= next.open_interest != Some(value);
            next.open_interest = Some(value);
        }
        if let Some(value) = f64_field_optional(object, "13")? {
            changed |= next.multiplier != Some(value);
            next.multiplier = Some(value);
        }
        if let Some(value) = i64_field_optional(object, "38")? {
            changed |= next.quote_time_millis != Some(value);
            next.quote_time_millis = Some(value);
        }
        if !changed {
            return Ok(StreamApply::NoChange);
        }

        if let Some(input) = next.complete_input() {
            self.book
                .upsert(input)
                .map_err(|error| AdapterError::Gex(format!("{error:?}")))?;
        }
        self.contracts.insert(symbol.to_owned(), next);
        Ok(StreamApply::Applied)
    }

    pub fn apply_underlying_stream_content(
        &mut self,
        content: &Value,
    ) -> Result<StreamApply, AdapterError> {
        let object = content.as_object().ok_or_else(|| {
            AdapterError::ProviderContract("underlying stream content is not an object".to_owned())
        })?;
        if string_field(object, "key")? != self.underlying {
            self.needs_rebootstrap = true;
            return Err(AdapterError::UnderlyingMismatch);
        }
        if bool_field_optional(object, "delayed") == Some(true) {
            self.delayed = true;
        }
        let mark = f64_field_optional(object, "33")?.unwrap_or(self.spot);
        let quote_time = i64_field_optional(object, "34")?.unwrap_or(self.spot_time_millis);
        if quote_time < self.spot_time_millis {
            return Ok(StreamApply::StaleIgnored);
        }
        if !mark.is_finite() || mark <= 0.0 || quote_time < 0 {
            self.conflict = true;
            return Err(AdapterError::ProviderContract(
                "invalid underlying stream update".to_owned(),
            ));
        }
        if mark == self.spot && quote_time == self.spot_time_millis {
            return Ok(StreamApply::NoChange);
        }
        self.book
            .set_spot(mark, quote_time)
            .map_err(|error| AdapterError::Gex(format!("{error:?}")))?;
        self.spot = mark;
        self.spot_time_millis = quote_time;
        Ok(StreamApply::Applied)
    }

    pub fn reliable_surface(
        &self,
        now_unix_ms: i64,
        gex_stale_timeout_ms: u64,
        market_data_stale_timeout_ms: u64,
    ) -> Result<ReliableGexSurface, GammaQuality> {
        if self.delayed {
            return Err(GammaQuality::Delayed);
        }
        if self.conflict {
            return Err(GammaQuality::ConflictingProviderState);
        }
        if self.needs_rebootstrap {
            return Err(GammaQuality::RebootstrapRequired);
        }
        let hydrated = self.hydrated_contracts();
        if self.expected_contracts == 0 {
            return Err(GammaQuality::EmptyCoverage);
        }
        if hydrated != self.expected_contracts {
            return Err(GammaQuality::CoverageIncomplete {
                expected: self.expected_contracts,
                hydrated,
            });
        }
        let snapshot = self
            .book
            .snapshot()
            .map_err(|_| GammaQuality::CoverageIncomplete {
                expected: self.expected_contracts,
                hydrated,
            })?;
        let oldest = snapshot
            .oldest_contract_quote_time_millis
            .ok_or(GammaQuality::EmptyCoverage)?;
        if oldest > now_unix_ms || snapshot.spot_time_millis > now_unix_ms {
            return Err(GammaQuality::FutureTimestamp);
        }
        if now_unix_ms.saturating_sub(oldest) as u64 > gex_stale_timeout_ms {
            return Err(GammaQuality::GexStale);
        }
        if now_unix_ms.saturating_sub(snapshot.spot_time_millis) as u64
            > market_data_stale_timeout_ms
        {
            return Err(GammaQuality::UnderlyingStale);
        }
        Ok(ReliableGexSurface {
            snapshot,
            expected_contracts: self.expected_contracts,
            hydrated_contracts: hydrated,
        })
    }

    fn ingest_map(
        &mut self,
        map: BTreeMap<String, BTreeMap<String, Vec<ChainContract>>>,
        side: OptionSide,
    ) -> Result<(), AdapterError> {
        for strikes in map.into_values() {
            for contracts in strikes.into_values() {
                for contract in contracts {
                    self.expected_contracts += 1;
                    let state = contract.into_state(&self.underlying, side)?;
                    if self.contracts.contains_key(&state.symbol) {
                        self.conflict = true;
                        return Err(AdapterError::ProviderContract(format!(
                            "duplicate option contract {}",
                            state.symbol
                        )));
                    }
                    if let Some(input) = state.complete_input() {
                        self.book
                            .upsert(input)
                            .map_err(|error| AdapterError::Gex(format!("{error:?}")))?;
                    }
                    self.contracts.insert(state.symbol.clone(), state);
                }
            }
        }
        Ok(())
    }

    fn validate_static_option_fields(
        &mut self,
        object: &Map<String, Value>,
        current: &ContractState,
    ) -> Result<(), AdapterError> {
        let mismatch = f64_field_optional(object, "20")?
            .is_some_and(|value| value != current.strike)
            || string_field_optional(object, "22")?
                .is_some_and(|value| value != current.underlying)
            || string_field_optional(object, "21")?
                .and_then(parse_stream_side)
                .is_some_and(|value| value != current.side)
            || i64_field_optional(object, "12")?.is_some_and(|value| {
                expiration_parts(&current.expiration)
                    .map(|(year, _, _)| value != i64::from(year))
                    .unwrap_or(true)
            })
            || i64_field_optional(object, "23")?.is_some_and(|value| {
                expiration_parts(&current.expiration)
                    .map(|(_, month, _)| value != i64::from(month))
                    .unwrap_or(true)
            })
            || i64_field_optional(object, "26")?.is_some_and(|value| {
                expiration_parts(&current.expiration)
                    .map(|(_, _, day)| value != i64::from(day))
                    .unwrap_or(true)
            });
        if mismatch {
            self.conflict = true;
            return Err(AdapterError::ProviderContract(format!(
                "stream identity mismatch for {}",
                current.symbol
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshError {
    InvalidInterval,
    TimeReversed,
}

#[derive(Debug, Clone)]
pub struct RebootstrapSchedule {
    refresh_interval_ms: u64,
    last_bootstrap_ms: Option<u64>,
}

impl RebootstrapSchedule {
    pub fn new(refresh_interval_ms: u64) -> Result<Self, RefreshError> {
        if refresh_interval_ms == 0 {
            return Err(RefreshError::InvalidInterval);
        }
        Ok(Self {
            refresh_interval_ms,
            last_bootstrap_ms: None,
        })
    }

    pub fn record_bootstrap(&mut self, now_ms: u64) -> Result<(), RefreshError> {
        if self.last_bootstrap_ms.is_some_and(|last| now_ms < last) {
            return Err(RefreshError::TimeReversed);
        }
        self.last_bootstrap_ms = Some(now_ms);
        Ok(())
    }

    #[must_use]
    pub fn should_rebootstrap(&self, now_ms: u64, state: &SchwabGexState) -> bool {
        if state.needs_rebootstrap() {
            return true;
        }
        self.last_bootstrap_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= self.refresh_interval_ms)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionChainResponse {
    symbol: String,
    status: String,
    #[serde(default)]
    is_delayed: bool,
    number_of_contracts: Option<usize>,
    underlying: Option<UnderlyingQuote>,
    #[serde(default)]
    call_exp_date_map: BTreeMap<String, BTreeMap<String, Vec<ChainContract>>>,
    #[serde(default)]
    put_exp_date_map: BTreeMap<String, BTreeMap<String, Vec<ChainContract>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnderlyingQuote {
    symbol: String,
    mark: f64,
    quote_time: i64,
    #[serde(default)]
    delayed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainContract {
    symbol: String,
    put_call: String,
    strike_price: Option<f64>,
    expiration_date: Option<String>,
    gamma: Option<f64>,
    open_interest: Option<u64>,
    multiplier: Option<f64>,
    quote_time_in_long: Option<i64>,
}

impl ChainContract {
    fn into_state(
        self,
        underlying: &str,
        expected_side: OptionSide,
    ) -> Result<ContractState, AdapterError> {
        let observed_side = parse_chain_side(&self.put_call).ok_or_else(|| {
            AdapterError::ProviderContract(format!("invalid putCall value {}", self.put_call))
        })?;
        if observed_side != expected_side {
            return Err(AdapterError::ProviderContract(
                "option side disagrees with chain map".to_owned(),
            ));
        }
        let strike = self.strike_price.ok_or_else(|| {
            AdapterError::ProviderContract("option chain omitted strikePrice".to_owned())
        })?;
        let expiration = self.expiration_date.ok_or_else(|| {
            AdapterError::ProviderContract("option chain omitted expirationDate".to_owned())
        })?;
        if self.symbol.trim().is_empty()
            || !strike.is_finite()
            || strike <= 0.0
            || expiration_parts(&expiration).is_none()
        {
            return Err(AdapterError::ProviderContract(
                "option chain contains invalid contract identity".to_owned(),
            ));
        }
        if self
            .gamma
            .is_some_and(|value| !value.is_finite() || value < 0.0)
            || self
                .multiplier
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
            || self.quote_time_in_long.is_some_and(|value| value < 0)
        {
            return Err(AdapterError::ProviderContract(
                "option chain contains invalid GEX input".to_owned(),
            ));
        }
        Ok(ContractState {
            symbol: self.symbol,
            underlying: underlying.to_owned(),
            expiration,
            strike,
            side: expected_side,
            gamma: self.gamma,
            open_interest: self.open_interest,
            multiplier: self.multiplier,
            quote_time_millis: self.quote_time_in_long,
        })
    }
}

fn parse_chain_side(value: &str) -> Option<OptionSide> {
    match value {
        "CALL" => Some(OptionSide::Call),
        "PUT" => Some(OptionSide::Put),
        _ => None,
    }
}

fn parse_stream_side(value: &str) -> Option<OptionSide> {
    match value {
        "C" | "CALL" => Some(OptionSide::Call),
        "P" | "PUT" => Some(OptionSide::Put),
        _ => None,
    }
}

fn expiration_parts(value: &str) -> Option<(u16, u8, u8)> {
    let date = value.get(..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, AdapterError> {
    string_field_optional(object, key)?
        .ok_or_else(|| AdapterError::ProviderContract(format!("stream field {key} is missing")))
}

fn string_field_optional<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, AdapterError> {
    object
        .get(key)
        .map(|value| {
            value.as_str().ok_or_else(|| {
                AdapterError::ProviderContract(format!("stream field {key} is not a string"))
            })
        })
        .transpose()
}

fn bool_field_optional(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key).and_then(Value::as_bool)
}

fn f64_field_optional(object: &Map<String, Value>, key: &str) -> Result<Option<f64>, AdapterError> {
    object
        .get(key)
        .map(|value| {
            value.as_f64().ok_or_else(|| {
                AdapterError::ProviderContract(format!("stream field {key} is not numeric"))
            })
        })
        .transpose()
}

fn i64_field_optional(object: &Map<String, Value>, key: &str) -> Result<Option<i64>, AdapterError> {
    object
        .get(key)
        .map(|value| {
            value.as_i64().ok_or_else(|| {
                AdapterError::ProviderContract(format!("stream field {key} is not an integer"))
            })
        })
        .transpose()
}

fn u64_field_optional(object: &Map<String, Value>, key: &str) -> Result<Option<u64>, AdapterError> {
    object
        .get(key)
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                AdapterError::ProviderContract(format!(
                    "stream field {key} is not an unsigned integer"
                ))
            })
        })
        .transpose()
}
