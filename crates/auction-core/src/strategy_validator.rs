use crate::{
    Condition, DataCycleReadiness, Direction, EnvironmentInput, EtTime, FrozenPremarketScenario,
    LocationSetup, LongOrderflowSequence, MarketState, NewsBlackoutWindow, RejectionCode,
    SessionPermissions, SetupState, ShortOrderflowSequence, TerminalState, bearish_fib,
    bullish_fib, direction_allowed, evaluate_environment, evaluate_news_gate, long_location_valid,
    short_location_valid,
};

/// Direction-specific deterministic sequence evidence. Each coordinator is bound to an actual
/// LOCATION_REACHED structure and can reach FINAL_RECONFIRMATION only through the canonical
/// §102 state order.
#[derive(Debug, Clone, Copy)]
pub enum OrderflowSequenceEvidence<'a> {
    Long(&'a LongOrderflowSequence),
    Short(&'a ShortOrderflowSequence),
}

impl OrderflowSequenceEvidence<'_> {
    const fn direction(self) -> Direction {
        match self {
            Self::Long(_) => Direction::Long,
            Self::Short(_) => Direction::Short,
        }
    }

    const fn state(self) -> SetupState {
        match self {
            Self::Long(sequence) => sequence.state(),
            Self::Short(sequence) => sequence.state(),
        }
    }

    const fn structure(self) -> crate::StructureKey {
        match self {
            Self::Long(sequence) => sequence.structure(),
            Self::Short(sequence) => sequence.structure(),
        }
    }

    fn first_failure_extreme(self) -> Option<f64> {
        match self {
            Self::Long(sequence) => sequence.first_failure_extreme(),
            Self::Short(sequence) => sequence.first_failure_extreme(),
        }
    }

    fn reconfirmation_extreme(self) -> Option<f64> {
        match self {
            Self::Long(sequence) => sequence.reconfirmation_extreme(),
            Self::Short(sequence) => sequence.reconfirmation_extreme(),
        }
    }
}

/// Inputs owned by the deterministic Strategy Validator under canonical §§6, 109, 117 and 122.
/// Exact strategy facts are recomputed or verified through existing typed deterministic modules;
/// callers do not supply a free-form STRATEGY_VALIDATOR_PASS boolean.
#[derive(Debug, Clone, Copy)]
pub struct StrategyValidationInputs<'a> {
    pub setup_id: &'a str,
    pub owner_agent_id: &'a str,
    pub instrument: &'a str,
    pub now_et: EtTime,
    pub data_readiness: DataCycleReadiness,
    pub premarket_scenario: &'a FrozenPremarketScenario,
    pub environment: EnvironmentInput,
    pub location: &'a LocationSetup,
    pub orderflow: OrderflowSequenceEvidence<'a>,
    pub news_schedule_valid: Condition,
    pub news_windows: &'a [NewsBlackoutWindow],
}

/// Opaque proof that the Strategy Validator passed its pre-risk responsibilities. It has no
/// public constructor. Accepted first-failure and reconfirmation evidence remains sealed inside
/// the bound order-flow sequence until the later order/risk authority unit consumes it.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyValidationProof {
    setup_id: String,
    owner_agent_id: String,
    instrument: String,
    direction: Direction,
    market_state: MarketState,
}

impl StrategyValidationProof {
    #[must_use]
    pub fn setup_id(&self) -> &str {
        &self.setup_id
    }

    #[must_use]
    pub fn owner_agent_id(&self) -> &str {
        &self.owner_agent_id
    }

    #[must_use]
    pub fn instrument(&self) -> &str {
        &self.instrument
    }

    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    #[must_use]
    pub const fn market_state(&self) -> MarketState {
        self.market_state
    }
}

/// Canonical §§6, 109, 117 and 122 deterministic pre-risk validation. This function recomputes
/// time, environment/direction, Fib construction, locked value location, setup/order-flow
/// binding, and news-reset validity. Position sizing, portfolio exposure, duplicate state, and
/// broker state remain in their later owning deterministic components.
pub fn validate_strategy(
    inputs: StrategyValidationInputs<'_>,
) -> Result<StrategyValidationProof, RejectionCode> {
    if inputs.setup_id.trim().is_empty()
        || inputs.owner_agent_id.trim().is_empty()
        || inputs.instrument.trim().is_empty()
    {
        return Err(RejectionCode::ProcessError);
    }

    if !SessionPermissions::at(inputs.now_et).allow_new_entries {
        return Err(RejectionCode::TimeCutoff);
    }
    if inputs.data_readiness.validity() != Condition::True {
        return Err(RejectionCode::DataInvalid);
    }

    if inputs.premarket_scenario.agent_id() != inputs.owner_agent_id
        || inputs.premarket_scenario.instrument() != inputs.instrument
        || inputs.premarket_scenario.premarket_plan_complete() != Condition::True
    {
        return Err(RejectionCode::ProcessError);
    }

    let environment = evaluate_environment(inputs.environment);
    let market_state = environment.market_state;
    match market_state {
        MarketState::Balanced => return Err(RejectionCode::Balanced),
        MarketState::Unclear => return Err(RejectionCode::StructureUnclear),
        MarketState::ValueUp | MarketState::ValueDown => {}
    }

    let structure = inputs.location.structure();
    let direction = structure.direction;
    if inputs.orderflow.direction() != direction || !direction_allowed(market_state, direction) {
        return Err(RejectionCode::WrongDirection);
    }
    if inputs.orderflow.structure() != structure {
        return Err(RejectionCode::ProcessError);
    }

    let impulse = inputs.location.impulse();
    let recalculated_levels = match direction {
        Direction::Long => bullish_fib(impulse.swing_low, impulse.swing_high),
        Direction::Short => bearish_fib(impulse.swing_high, impulse.swing_low),
    }
    .ok_or(RejectionCode::ProcessError)?;
    if recalculated_levels != inputs.location.levels() {
        return Err(RejectionCode::ProcessError);
    }

    let references = inputs.premarket_scenario.references();
    let zone_outside_locked_value = match direction {
        Direction::Long => long_location_valid(recalculated_levels, references.reference_val),
        Direction::Short => short_location_valid(recalculated_levels, references.reference_vah),
    };
    if !zone_outside_locked_value {
        return Err(RejectionCode::ZoneInsideValue);
    }

    match inputs.location.state() {
        SetupState::Terminal(TerminalState::Invalidated) => {
            return Err(RejectionCode::Fib886Invalidation);
        }
        SetupState::LocationReached => {}
        SetupState::Created
        | SetupState::WaitingForLocation
        | SetupState::AggressionPresent
        | SetupState::PotentialAbsorption
        | SetupState::FirstDominanceShift
        | SetupState::WaitingSecondTest
        | SetupState::SecondTest
        | SetupState::SecondFailure
        | SetupState::FinalReconfirmation
        | SetupState::EntryAuthorized
        | SetupState::OrderPending
        | SetupState::Filled
        | SetupState::PositionManagement
        | SetupState::Closed
        | SetupState::Terminal(_) => return Err(RejectionCode::ProcessError),
    }

    if inputs.orderflow.state() != SetupState::FinalReconfirmation {
        return Err(RejectionCode::NoReconfirmation);
    }

    if inputs.location.created_at() > inputs.now_et {
        return Err(RejectionCode::ProcessError);
    }
    let news = evaluate_news_gate(
        inputs.now_et,
        Some(inputs.location.created_at()),
        inputs.news_schedule_valid,
        inputs.news_windows,
    );
    if news.news_blackout_clear != Condition::True || !news.allow_new_entries {
        return Err(RejectionCode::NewsBlackout);
    }

    inputs
        .orderflow
        .first_failure_extreme()
        .filter(|value| value.is_finite())
        .ok_or(RejectionCode::ProcessError)?;
    inputs
        .orderflow
        .reconfirmation_extreme()
        .filter(|value| value.is_finite())
        .ok_or(RejectionCode::NoReconfirmation)?;

    Ok(StrategyValidationProof {
        setup_id: inputs.setup_id.to_owned(),
        owner_agent_id: inputs.owner_agent_id.to_owned(),
        instrument: inputs.instrument.to_owned(),
        direction,
        market_state,
    })
}
