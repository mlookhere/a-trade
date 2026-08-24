/// Ordered setup states from canonical §102. No live state may be skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupState {
    Created,
    WaitingForLocation,
    LocationReached,
    AggressionPresent,
    PotentialAbsorption,
    FirstDominanceShift,
    WaitingSecondTest,
    SecondTest,
    SecondFailure,
    FinalReconfirmation,
    EntryAuthorized,
    OrderPending,
    Filled,
    PositionManagement,
    Closed,
    Terminal(TerminalState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalState {
    Invalidated,
    Expired,
    Rejected,
    Missed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStateError {
    StateSkip,
    AlreadyTerminal,
}

impl SetupState {
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Created => Some(Self::WaitingForLocation),
            Self::WaitingForLocation => Some(Self::LocationReached),
            Self::LocationReached => Some(Self::AggressionPresent),
            Self::AggressionPresent => Some(Self::PotentialAbsorption),
            Self::PotentialAbsorption => Some(Self::FirstDominanceShift),
            Self::FirstDominanceShift => Some(Self::WaitingSecondTest),
            Self::WaitingSecondTest => Some(Self::SecondTest),
            Self::SecondTest => Some(Self::SecondFailure),
            Self::SecondFailure => Some(Self::FinalReconfirmation),
            Self::FinalReconfirmation => Some(Self::EntryAuthorized),
            Self::EntryAuthorized => Some(Self::OrderPending),
            Self::OrderPending => Some(Self::Filled),
            Self::Filled => Some(Self::PositionManagement),
            Self::PositionManagement => Some(Self::Closed),
            Self::Closed | Self::Terminal(_) => None,
        }
    }

    pub const fn advance(self, requested: Self) -> Result<Self, SetupStateError> {
        if matches!(self, Self::Terminal(_)) {
            return Err(SetupStateError::AlreadyTerminal);
        }
        match self.next() {
            Some(next) if next == requested => Ok(requested),
            _ => Err(SetupStateError::StateSkip),
        }
    }

    #[must_use]
    pub const fn terminate(self, terminal: TerminalState) -> Self {
        match self {
            Self::Closed | Self::Terminal(_) => self,
            _ => Self::Terminal(terminal),
        }
    }
}
