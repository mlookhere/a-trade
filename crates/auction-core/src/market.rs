/// Canonical market states from §§18-24.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketState {
    ValueUp,
    ValueDown,
    Balanced,
    Unclear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Long,
    Short,
}

/// Direction permission is exact canonical §24 behavior.
#[must_use]
pub const fn direction_allowed(state: MarketState, direction: Direction) -> bool {
    matches!(
        (state, direction),
        (MarketState::ValueUp, Direction::Long) | (MarketState::ValueDown, Direction::Short)
    )
}
