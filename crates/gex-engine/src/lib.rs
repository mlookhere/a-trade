use std::collections::{BTreeMap, HashMap};

/// Options side used by the provider-neutral GEX analytics engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionSide {
    Call,
    Put,
}

/// Complete contract state required to maintain a GEX coefficient.
///
/// Provider adapters must merge partial quote updates before submitting this type.
#[derive(Debug, Clone, PartialEq)]
pub struct ContractGexInput {
    pub symbol: String,
    pub underlying: String,
    pub expiration: String,
    pub strike: f64,
    pub side: OptionSide,
    pub gamma: f64,
    pub open_interest: u64,
    pub multiplier: f64,
    pub quote_time_millis: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetGammaRegime {
    Positive,
    Negative,
    Flat,
}

/// Exposure model used by this analytics layer. Calls are treated as positive and puts as
/// negative because actual dealer inventory is not available from the market-data feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExposureModel {
    SideSignedOpenInterest,
}

/// Gamma-flip calculation is deliberately unresolved. Canonical §14 requires a reliable gamma
/// flip when one is available but does not define the computation. The reference UI's heuristic
/// is not promoted into production strategy logic without validation and approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GammaFlipModel {
    Unresolved,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrikeGex {
    pub strike: f64,
    pub call_gex: f64,
    pub put_gex: f64,
    pub net_gex: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpirationGex {
    pub expiration: String,
    pub call_gex: f64,
    pub put_gex: f64,
    pub net_gex: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GexSnapshot {
    pub underlying: String,
    pub spot: f64,
    pub as_of_millis: i64,
    pub spot_time_millis: i64,
    pub oldest_contract_quote_time_millis: Option<i64>,
    pub newest_contract_quote_time_millis: Option<i64>,
    pub contract_count: usize,
    pub exposure_model: ExposureModel,
    pub total_call_gex: f64,
    pub total_put_gex: f64,
    pub net_gex: f64,
    pub regime: NetGammaRegime,
    pub call_wall: Option<f64>,
    pub put_wall: Option<f64>,
    pub gamma_flip: Option<f64>,
    pub gamma_flip_model: GammaFlipModel,
    pub by_strike: Vec<StrikeGex>,
    pub by_expiration: Vec<ExpirationGex>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GexError {
    MissingUnderlying,
    MissingContractSymbol,
    MissingExpiration,
    UnderlyingMismatch,
    InvalidStrike,
    InvalidGamma,
    InvalidMultiplier,
    InvalidSpot,
    InvalidQuoteTime,
    StaleContractUpdate,
    StaleSpotUpdate,
    ExposureOverflow,
    SpotUnavailable,
}

#[derive(Debug, Clone)]
struct StoredContract {
    expiration: String,
    strike: f64,
    side: OptionSide,
    coefficient: f64,
    quote_time_millis: i64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Coefficients {
    call: f64,
    put: f64,
    count: usize,
}

impl Coefficients {
    fn apply(&mut self, side: OptionSide, coefficient: f64, count_delta: isize) {
        let coefficient_delta = coefficient * count_delta as f64;
        match side {
            OptionSide::Call => self.call += coefficient_delta,
            OptionSide::Put => self.put += coefficient_delta,
        }
        self.count = self.count.saturating_add_signed(count_delta);
    }

    fn net(self) -> f64 {
        self.call + self.put
    }
}

#[derive(Debug, Clone, Copy)]
struct StrikeCoefficients {
    strike: f64,
    coefficients: Coefficients,
}

/// Incremental provider-neutral GEX book.
///
/// Contract updates maintain `gamma * open_interest * multiplier * 0.01` coefficients. Because
/// spot squared is a common factor, an underlying-price update changes the total GEX in O(1)
/// without rebuilding every contract. Per-strike/per-expiration values are materialized only when
/// a snapshot is requested.
#[derive(Debug, Clone)]
pub struct GexBook {
    underlying: String,
    contracts: HashMap<String, StoredContract>,
    strikes: Vec<StrikeCoefficients>,
    expirations: BTreeMap<String, Coefficients>,
    totals: Coefficients,
    spot: Option<(f64, i64)>,
}

impl GexBook {
    pub fn new(underlying: &str) -> Result<Self, GexError> {
        let underlying = underlying.trim();
        if underlying.is_empty() {
            return Err(GexError::MissingUnderlying);
        }
        Ok(Self {
            underlying: underlying.to_owned(),
            contracts: HashMap::new(),
            strikes: Vec::new(),
            expirations: BTreeMap::new(),
            totals: Coefficients::default(),
            spot: None,
        })
    }

    #[must_use]
    pub fn underlying(&self) -> &str {
        &self.underlying
    }

    #[must_use]
    pub fn contract_count(&self) -> usize {
        self.contracts.len()
    }

    pub fn set_spot(&mut self, spot: f64, quote_time_millis: i64) -> Result<(), GexError> {
        if !spot.is_finite() || spot <= 0.0 {
            return Err(GexError::InvalidSpot);
        }
        if quote_time_millis < 0 {
            return Err(GexError::InvalidQuoteTime);
        }
        if self
            .spot
            .is_some_and(|(_, current_time)| quote_time_millis < current_time)
        {
            return Err(GexError::StaleSpotUpdate);
        }
        self.spot = Some((spot, quote_time_millis));
        Ok(())
    }

    pub fn upsert(&mut self, input: ContractGexInput) -> Result<(), GexError> {
        validate_contract(&self.underlying, &input)?;
        if self
            .contracts
            .get(&input.symbol)
            .is_some_and(|current| input.quote_time_millis < current.quote_time_millis)
        {
            return Err(GexError::StaleContractUpdate);
        }

        let coefficient = signed_coefficient(&input)?;
        if let Some(previous) = self.contracts.remove(&input.symbol) {
            self.apply_contract(&previous, -1);
        }

        let contract = StoredContract {
            expiration: input.expiration,
            strike: input.strike,
            side: input.side,
            coefficient,
            quote_time_millis: input.quote_time_millis,
        };
        self.apply_contract(&contract, 1);
        self.contracts.insert(input.symbol, contract);
        Ok(())
    }

    pub fn remove(&mut self, symbol: &str) -> bool {
        let Some(contract) = self.contracts.remove(symbol) else {
            return false;
        };
        self.apply_contract(&contract, -1);
        true
    }

    pub fn snapshot(&self) -> Result<GexSnapshot, GexError> {
        let (spot, spot_time) = self.spot.ok_or(GexError::SpotUnavailable)?;
        let spot_squared = spot * spot;
        if !spot_squared.is_finite() {
            return Err(GexError::ExposureOverflow);
        }

        let total_call_gex = checked_scale(self.totals.call, spot_squared)?;
        let total_put_gex = checked_scale(self.totals.put, spot_squared)?;
        let net_gex = checked_scale(self.totals.net(), spot_squared)?;
        let regime = if net_gex > 0.0 {
            NetGammaRegime::Positive
        } else if net_gex < 0.0 {
            NetGammaRegime::Negative
        } else {
            NetGammaRegime::Flat
        };

        let by_strike = self
            .strikes
            .iter()
            .map(|bucket| {
                let call_gex = bucket.coefficients.call * spot_squared;
                let put_gex = bucket.coefficients.put * spot_squared;
                StrikeGex {
                    strike: bucket.strike,
                    call_gex,
                    put_gex,
                    net_gex: call_gex + put_gex,
                }
            })
            .collect();

        let by_expiration = self
            .expirations
            .iter()
            .map(|(expiration, coefficients)| {
                let call_gex = coefficients.call * spot_squared;
                let put_gex = coefficients.put * spot_squared;
                ExpirationGex {
                    expiration: expiration.clone(),
                    call_gex,
                    put_gex,
                    net_gex: call_gex + put_gex,
                }
            })
            .collect();

        Ok(GexSnapshot {
            underlying: self.underlying.clone(),
            spot,
            as_of_millis: self.conservative_as_of(spot_time),
            spot_time_millis: spot_time,
            oldest_contract_quote_time_millis: self.oldest_contract_time(),
            newest_contract_quote_time_millis: self.newest_contract_time(),
            contract_count: self.contracts.len(),
            exposure_model: ExposureModel::SideSignedOpenInterest,
            total_call_gex,
            total_put_gex,
            net_gex,
            regime,
            call_wall: self.call_wall(spot),
            put_wall: self.put_wall(spot),
            gamma_flip: None,
            gamma_flip_model: GammaFlipModel::Unresolved,
            by_strike,
            by_expiration,
        })
    }

    fn apply_contract(&mut self, contract: &StoredContract, count_delta: isize) {
        self.totals
            .apply(contract.side, contract.coefficient, count_delta);

        let expiration = self
            .expirations
            .entry(contract.expiration.clone())
            .or_default();
        expiration.apply(contract.side, contract.coefficient, count_delta);
        if expiration.count == 0 {
            self.expirations.remove(&contract.expiration);
        }

        match self
            .strikes
            .binary_search_by(|bucket| bucket.strike.total_cmp(&contract.strike))
        {
            Ok(index) => {
                self.strikes[index].coefficients.apply(
                    contract.side,
                    contract.coefficient,
                    count_delta,
                );
                if self.strikes[index].coefficients.count == 0 {
                    self.strikes.remove(index);
                }
            }
            Err(index) if count_delta > 0 => {
                let mut coefficients = Coefficients::default();
                coefficients.apply(contract.side, contract.coefficient, count_delta);
                self.strikes.insert(
                    index,
                    StrikeCoefficients {
                        strike: contract.strike,
                        coefficients,
                    },
                );
            }
            Err(_) => {}
        }
    }

    fn oldest_contract_time(&self) -> Option<i64> {
        self.contracts
            .values()
            .map(|contract| contract.quote_time_millis)
            .min()
    }

    fn newest_contract_time(&self) -> Option<i64> {
        self.contracts
            .values()
            .map(|contract| contract.quote_time_millis)
            .max()
    }

    fn conservative_as_of(&self, spot_time: i64) -> i64 {
        self.oldest_contract_time()
            .map_or(spot_time, |contract_time| contract_time.min(spot_time))
    }

    fn call_wall(&self, spot: f64) -> Option<f64> {
        self.strikes
            .iter()
            .filter(|bucket| bucket.strike > spot && bucket.coefficients.call > 0.0)
            .max_by(|left, right| {
                left.coefficients
                    .call
                    .total_cmp(&right.coefficients.call)
                    .then_with(|| (right.strike - spot).total_cmp(&(left.strike - spot)))
            })
            .map(|bucket| bucket.strike)
    }

    fn put_wall(&self, spot: f64) -> Option<f64> {
        self.strikes
            .iter()
            .filter(|bucket| bucket.strike < spot && bucket.coefficients.put < 0.0)
            .max_by(|left, right| {
                left.coefficients
                    .put
                    .abs()
                    .total_cmp(&right.coefficients.put.abs())
                    .then_with(|| (spot - right.strike).total_cmp(&(spot - left.strike)))
            })
            .map(|bucket| bucket.strike)
    }
}

fn validate_contract(expected_underlying: &str, input: &ContractGexInput) -> Result<(), GexError> {
    if input.symbol.trim().is_empty() {
        return Err(GexError::MissingContractSymbol);
    }
    if input.underlying.trim().is_empty() {
        return Err(GexError::MissingUnderlying);
    }
    if input.underlying != expected_underlying {
        return Err(GexError::UnderlyingMismatch);
    }
    if input.expiration.trim().is_empty() {
        return Err(GexError::MissingExpiration);
    }
    if !input.strike.is_finite() || input.strike <= 0.0 {
        return Err(GexError::InvalidStrike);
    }
    if !input.gamma.is_finite() || input.gamma < 0.0 {
        return Err(GexError::InvalidGamma);
    }
    if !input.multiplier.is_finite() || input.multiplier <= 0.0 {
        return Err(GexError::InvalidMultiplier);
    }
    if input.quote_time_millis < 0 {
        return Err(GexError::InvalidQuoteTime);
    }
    Ok(())
}

fn signed_coefficient(input: &ContractGexInput) -> Result<f64, GexError> {
    let magnitude = input.gamma * input.open_interest as f64 * input.multiplier * 0.01;
    if !magnitude.is_finite() {
        return Err(GexError::ExposureOverflow);
    }
    Ok(match input.side {
        OptionSide::Call => magnitude,
        OptionSide::Put => -magnitude,
    })
}

fn checked_scale(coefficient: f64, spot_squared: f64) -> Result<f64, GexError> {
    let value = coefficient * spot_squared;
    value
        .is_finite()
        .then_some(value)
        .ok_or(GexError::ExposureOverflow)
}
