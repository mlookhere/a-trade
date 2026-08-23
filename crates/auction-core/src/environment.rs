use crate::{Condition, MarketState};

/// Completed regular-session value references used by canonical §§19-21.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionValue {
    pub vah: f64,
    pub val: f64,
    pub poc: f64,
}

impl SessionValue {
    fn valid(self) -> bool {
        self.vah.is_finite() && self.val.is_finite() && self.poc.is_finite() && self.vah >= self.val
    }

    fn midpoint(self) -> f64 {
        (self.vah + self.val) / 2.0
    }
}

/// Latest and previous confirmed 1H swings used by canonical §§20-21.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HourlyStructure {
    pub latest_swing_high: f64,
    pub previous_swing_high: f64,
    pub latest_swing_low: f64,
    pub previous_swing_low: f64,
}

impl HourlyStructure {
    fn valid(self) -> bool {
        self.latest_swing_high.is_finite()
            && self.previous_swing_high.is_finite()
            && self.latest_swing_low.is_finite()
            && self.previous_swing_low.is_finite()
    }

    fn bullish(self) -> bool {
        self.latest_swing_high > self.previous_swing_high
            && self.latest_swing_low > self.previous_swing_low
    }

    fn bearish(self) -> bool {
        self.latest_swing_high < self.previous_swing_high
            && self.latest_swing_low < self.previous_swing_low
    }
}

/// Inputs for canonical §§18-23. D-1 is the most recently completed RTH session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvironmentInput {
    pub d1: SessionValue,
    pub d2: SessionValue,
    pub d3: SessionValue,
    pub hourly: HourlyStructure,
    /// Canonical §22 requires "substantial" prior-value overlap but provides no numeric
    /// algorithm. A separately validated component must therefore supply this tri-state fact.
    pub prior_value_areas_substantially_overlap: Condition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvironmentEvaluation {
    pub market_state: MarketState,
    pub value_up_migration: Condition,
    pub value_down_migration: Condition,
    pub htf_bull_structure: Condition,
    pub htf_bear_structure: Condition,
    pub prior_value_areas_substantially_overlap: Condition,
}

/// Canonical §§18-23: directional states require 3 of 4 value-migration tests plus the
/// matching 1H structure. BALANCED is used only when the externally established substantial
/// overlap fact is TRUE. FALSE or UNKNOWN overlap falls through to UNCLEAR.
#[must_use]
pub fn evaluate_environment(input: EnvironmentInput) -> EnvironmentEvaluation {
    if !input.d1.valid() || !input.d2.valid() || !input.d3.valid() || !input.hourly.valid() {
        return EnvironmentEvaluation {
            market_state: MarketState::Unclear,
            value_up_migration: Condition::Unknown,
            value_down_migration: Condition::Unknown,
            htf_bull_structure: Condition::Unknown,
            htf_bear_structure: Condition::Unknown,
            prior_value_areas_substantially_overlap: input.prior_value_areas_substantially_overlap,
        };
    }

    let up_passes = [
        input.d1.poc > input.d2.poc,
        input.d2.poc > input.d3.poc,
        input.d1.midpoint() > input.d2.midpoint(),
        input.d2.midpoint() > input.d3.midpoint(),
    ]
    .into_iter()
    .filter(|passed| *passed)
    .count();

    let down_passes = [
        input.d1.poc < input.d2.poc,
        input.d2.poc < input.d3.poc,
        input.d1.midpoint() < input.d2.midpoint(),
        input.d2.midpoint() < input.d3.midpoint(),
    ]
    .into_iter()
    .filter(|passed| *passed)
    .count();

    let value_up_migration = Condition::from(up_passes >= 3);
    let value_down_migration = Condition::from(down_passes >= 3);
    let htf_bull_structure = Condition::from(input.hourly.bullish());
    let htf_bear_structure = Condition::from(input.hourly.bearish());

    let market_state = if value_up_migration.permits() && htf_bull_structure.permits() {
        MarketState::ValueUp
    } else if value_down_migration.permits() && htf_bear_structure.permits() {
        MarketState::ValueDown
    } else if input.prior_value_areas_substantially_overlap.permits() {
        MarketState::Balanced
    } else {
        MarketState::Unclear
    };

    EnvironmentEvaluation {
        market_state,
        value_up_migration,
        value_down_migration,
        htf_bull_structure,
        htf_bear_structure,
        prior_value_areas_substantially_overlap: input.prior_value_areas_substantially_overlap,
    }
}

/// Canonical §25 premarket references. Optional gamma levels remain absent when unavailable;
/// they are never fabricated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PremarketReferences {
    pub reference_vah: f64,
    pub reference_val: f64,
    pub reference_poc: f64,
    pub prior_rth_high: f64,
    pub prior_rth_low: f64,
    pub overnight_high: f64,
    pub overnight_low: f64,
    pub call_wall: Option<f64>,
    pub put_wall: Option<f64>,
    pub gamma_flip: Option<f64>,
}
