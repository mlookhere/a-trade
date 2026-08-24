use crate::swings::SwingImpulse;
use crate::{
    Direction, EtTime, FibLevels, MarketState, SessionPermissions, SetupState, TerminalState,
    bearish_fib, bullish_fib, direction_allowed, long_location_reached, long_location_valid,
    short_location_reached, short_location_valid,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureKey {
    pub direction: Direction,
    pub start_index: usize,
    pub end_index: usize,
}

impl From<SwingImpulse> for StructureKey {
    fn from(impulse: SwingImpulse) -> Self {
        Self {
            direction: impulse.direction,
            start_index: impulse.start_index,
            end_index: impulse.end_index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationEvent {
    None,
    Reached,
    Invalidated,
}

/// Opening/location lifecycle for canonical §§34-37 and §117. It retains the exact qualified
/// impulse and creation time so the later deterministic validator can independently recompute
/// Fib/value-location facts and news-reset ordering rather than trusting caller booleans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocationSetup {
    impulse: SwingImpulse,
    created_at: EtTime,
    levels: FibLevels,
    tick_size: f64,
    state: SetupState,
}

impl LocationSetup {
    /// Canonical §§24, 29-35: the initial opening setup starts at exactly 09:30 ET, in the
    /// direction authorized by market state, with a qualified impulse and a Fib zone completely
    /// outside value.
    #[must_use]
    pub fn at_open(
        time_et: EtTime,
        market_state: MarketState,
        impulse: SwingImpulse,
        reference_value_boundary: f64,
        tick_size: f64,
    ) -> Option<Self> {
        let open = EtTime::from_hms(9, 30, 0)?;
        if time_et != open {
            return None;
        }
        Self::build_waiting(
            time_et,
            market_state,
            impulse,
            reference_value_boundary,
            tick_size,
        )
    }

    /// Canonical §37 and §117: while new entries are enabled, genuinely new confirmed structure
    /// may create a fresh setup. The Setup Coordinator supplies every structure key already used
    /// for the instrument/session; reusing any known structure fails closed.
    #[must_use]
    pub fn from_new_structure(
        time_et: EtTime,
        market_state: MarketState,
        impulse: SwingImpulse,
        reference_value_boundary: f64,
        tick_size: f64,
        known_structures: &[StructureKey],
    ) -> Option<Self> {
        if !SessionPermissions::at(time_et).allow_new_entries {
            return None;
        }
        let structure = StructureKey::from(impulse);
        if known_structures.contains(&structure) {
            return None;
        }
        Self::build_waiting(
            time_et,
            market_state,
            impulse,
            reference_value_boundary,
            tick_size,
        )
    }

    fn build_waiting(
        created_at: EtTime,
        market_state: MarketState,
        impulse: SwingImpulse,
        reference_value_boundary: f64,
        tick_size: f64,
    ) -> Option<Self> {
        if impulse.start_index >= impulse.end_index
            || !direction_allowed(market_state, impulse.direction)
            || !reference_value_boundary.is_finite()
            || !tick_size.is_finite()
            || tick_size <= 0.0
        {
            return None;
        }

        let levels = match impulse.direction {
            Direction::Long => bullish_fib(impulse.swing_low, impulse.swing_high)?,
            Direction::Short => bearish_fib(impulse.swing_high, impulse.swing_low)?,
        };

        let location_valid = match impulse.direction {
            Direction::Long => long_location_valid(levels, reference_value_boundary),
            Direction::Short => short_location_valid(levels, reference_value_boundary),
        };
        if !location_valid {
            return None;
        }

        let state = SetupState::Created
            .advance(SetupState::WaitingForLocation)
            .ok()?;

        Some(Self {
            impulse,
            created_at,
            levels,
            tick_size,
            state,
        })
    }

    #[must_use]
    pub const fn state(self) -> SetupState {
        self.state
    }

    #[must_use]
    pub const fn levels(self) -> FibLevels {
        self.levels
    }

    #[must_use]
    pub const fn structure(self) -> StructureKey {
        StructureKey {
            direction: self.impulse.direction,
            start_index: self.impulse.start_index,
            end_index: self.impulse.end_index,
        }
    }

    #[must_use]
    pub(crate) const fn impulse(self) -> SwingImpulse {
        self.impulse
    }

    #[must_use]
    pub const fn created_at(self) -> EtTime {
        self.created_at
    }

    /// Canonical §§35-37: a location touch can only advance WAITING_FOR_LOCATION to
    /// LOCATION_REACHED. Before valid reversal confirmation, a strict 0.886 breach terminates
    /// the setup. No event here can authorize a trade.
    pub fn observe_price(&mut self, price: f64) -> LocationEvent {
        if !price.is_finite() || matches!(self.state, SetupState::Terminal(_)) {
            return LocationEvent::None;
        }

        let invalidated = match self.impulse.direction {
            Direction::Long => long_886_invalidated(self.levels, self.tick_size, price, self.state),
            Direction::Short => {
                short_886_invalidated(self.levels, self.tick_size, price, self.state)
            }
        };
        if invalidated {
            self.state = self.state.terminate(TerminalState::Invalidated);
            return LocationEvent::Invalidated;
        }

        if self.state == SetupState::WaitingForLocation {
            let reached = match self.impulse.direction {
                Direction::Long => long_location_reached(self.levels, price),
                Direction::Short => short_location_reached(self.levels, price),
            };
            if reached && let Ok(next) = self.state.advance(SetupState::LocationReached) {
                self.state = next;
                return LocationEvent::Reached;
            }
        }

        LocationEvent::None
    }

    /// Canonical §37: an invalidated structure cannot be reused. A coordinator may issue a new
    /// SETUP_ID only when genuinely new confirmed swing structure produces a different key.
    #[must_use]
    pub fn requires_new_setup_id_for(self, candidate: StructureKey) -> bool {
        matches!(self.state, SetupState::Terminal(TerminalState::Invalidated))
            && candidate != self.structure()
    }
}

/// Strict long §36 rule. Equality at FIB_886 - one tick does not invalidate because the
/// canonical operator is `<`.
#[must_use]
pub fn long_886_invalidated(
    levels: FibLevels,
    tick_size: f64,
    price: f64,
    state: SetupState,
) -> bool {
    levels.long_ordered()
        && tick_size.is_finite()
        && tick_size > 0.0
        && price.is_finite()
        && invalidation_active(state)
        && price < levels.level_886 - tick_size
}

/// Mirrored bearish implementation under canonical §85. The detailed source example is
/// bullish; this is explicitly the symmetric premium-side application of §36, not an
/// independently source-demonstrated short threshold.
#[must_use]
pub fn short_886_invalidated(
    levels: FibLevels,
    tick_size: f64,
    price: f64,
    state: SetupState,
) -> bool {
    levels.short_ordered()
        && tick_size.is_finite()
        && tick_size > 0.0
        && price.is_finite()
        && invalidation_active(state)
        && price > levels.level_886 + tick_size
}

const fn invalidation_active(state: SetupState) -> bool {
    matches!(
        state,
        SetupState::Created
            | SetupState::WaitingForLocation
            | SetupState::LocationReached
            | SetupState::AggressionPresent
            | SetupState::PotentialAbsorption
            | SetupState::FirstDominanceShift
            | SetupState::WaitingSecondTest
            | SetupState::SecondTest
            | SetupState::SecondFailure
    )
}
