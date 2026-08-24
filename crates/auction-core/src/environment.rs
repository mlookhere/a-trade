use crate::{Condition, EtTime, MarketState};

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

impl PremarketReferences {
    /// Structural validity for the levels that canonical §25 requires to be locked. Optional
    /// gamma levels may be absent, but a present value must be finite.
    #[must_use]
    pub fn validity(self) -> Condition {
        Condition::from(
            self.reference_vah.is_finite()
                && self.reference_val.is_finite()
                && self.reference_poc.is_finite()
                && self.prior_rth_high.is_finite()
                && self.prior_rth_low.is_finite()
                && self.overnight_high.is_finite()
                && self.overnight_low.is_finite()
                && self.reference_vah >= self.reference_val
                && self.prior_rth_high >= self.prior_rth_low
                && self.overnight_high >= self.overnight_low
                && self.call_wall.is_none_or(f64::is_finite)
                && self.put_wall.is_none_or(f64::is_finite)
                && self.gamma_flip.is_none_or(f64::is_finite),
        )
    }
}

/// Exact premarket concepts that canonical §17 requires to be established before an instrument
/// may trade. TRUE means the concept has been deterministically resolved; it does not mean the
/// resolved direction or environment itself is necessarily tradable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PremarketPlanComponents {
    pub market_environment_established: Condition,
    pub direction_permission_established: Condition,
    pub relevant_swing_structure_established: Condition,
    pub fib_location_established: Condition,
    pub gamma_regime_established: Condition,
    pub structural_targets_established: Condition,
    pub invalidations_established: Condition,
}

impl PremarketPlanComponents {
    #[must_use]
    pub fn readiness(self) -> Condition {
        all_conditions(&[
            self.market_environment_established,
            self.direction_permission_established,
            self.relevant_swing_structure_established,
            self.fib_location_established,
            self.gamma_regime_established,
            self.structural_targets_established,
            self.invalidations_established,
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PremarketScenarioError {
    MissingAgentId,
    MissingInstrument,
    MissingScenarioId,
    FreezeAtOrAfterOpen,
    InvalidReferences,
    IncompletePlan,
    UnknownPlan,
}

/// Opaque frozen §33 scenario record. The canonical example is intentionally not promoted into
/// a universal schema. The record proves identity, pre-open freeze timing, locked §25 references,
/// and completion of the §17 concepts; it has no order, risk, broker, or mutation API.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenPremarketScenario {
    agent_id: String,
    instrument: String,
    scenario_id: String,
    frozen_at_et: EtTime,
    references: PremarketReferences,
    components: PremarketPlanComponents,
}

impl FrozenPremarketScenario {
    pub fn freeze(
        agent_id: &str,
        instrument: &str,
        scenario_id: &str,
        frozen_at_et: EtTime,
        references: PremarketReferences,
        components: PremarketPlanComponents,
    ) -> Result<Self, PremarketScenarioError> {
        if agent_id.trim().is_empty() {
            return Err(PremarketScenarioError::MissingAgentId);
        }
        if instrument.trim().is_empty() {
            return Err(PremarketScenarioError::MissingInstrument);
        }
        if scenario_id.trim().is_empty() {
            return Err(PremarketScenarioError::MissingScenarioId);
        }

        let open = EtTime::from_hms(9, 30, 0).expect("09:30 ET is a valid time");
        if frozen_at_et >= open {
            return Err(PremarketScenarioError::FreezeAtOrAfterOpen);
        }
        if references.validity() != Condition::True {
            return Err(PremarketScenarioError::InvalidReferences);
        }

        match components.readiness() {
            Condition::True => {}
            Condition::False => return Err(PremarketScenarioError::IncompletePlan),
            Condition::Unknown => return Err(PremarketScenarioError::UnknownPlan),
        }

        Ok(Self {
            agent_id: agent_id.to_owned(),
            instrument: instrument.to_owned(),
            scenario_id: scenario_id.to_owned(),
            frozen_at_et,
            references,
            components,
        })
    }

    #[must_use]
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    #[must_use]
    pub fn instrument(&self) -> &str {
        &self.instrument
    }

    #[must_use]
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    #[must_use]
    pub const fn frozen_at_et(&self) -> EtTime {
        self.frozen_at_et
    }

    #[must_use]
    pub const fn references(&self) -> PremarketReferences {
        self.references
    }

    #[must_use]
    pub const fn components(&self) -> PremarketPlanComponents {
        self.components
    }

    /// Existence of this validated frozen record is the deterministic §17 handoff into the
    /// existing production authorization gate.
    #[must_use]
    pub const fn premarket_plan_complete(&self) -> Condition {
        Condition::True
    }
}

fn all_conditions(conditions: &[Condition]) -> Condition {
    if conditions.contains(&Condition::False) {
        Condition::False
    } else if conditions.contains(&Condition::Unknown) {
        Condition::Unknown
    } else {
        Condition::True
    }
}
