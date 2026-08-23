/// Fixed rejection codes from canonical §113.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionCode {
    DataInvalid,
    StructureUnclear,
    Balanced,
    WrongDirection,
    ZoneInsideValue,
    Fib886Invalidation,
    LowParticipation,
    NoAggression,
    AggressionNotAtExtreme,
    AggressorSucceeded,
    NoAbsorption,
    NoDominanceShift,
    NoSecondTest,
    SecondTestInvalid,
    NoReconfirmation,
    TargetTooClose,
    RiskRejected,
    PortfolioRiskRejected,
    CorrelationConflict,
    DuplicateSetup,
    NewsBlackout,
    MissedEntry,
    TimeCutoff,
    ProcessError,
    BrokerUnsafe,
}

impl RejectionCode {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::DataInvalid => "R01",
            Self::StructureUnclear => "R02",
            Self::Balanced => "R03",
            Self::WrongDirection => "R04",
            Self::ZoneInsideValue => "R05",
            Self::Fib886Invalidation => "R06",
            Self::LowParticipation => "R07",
            Self::NoAggression => "R08",
            Self::AggressionNotAtExtreme => "R09",
            Self::AggressorSucceeded => "R10",
            Self::NoAbsorption => "R11",
            Self::NoDominanceShift => "R12",
            Self::NoSecondTest => "R13",
            Self::SecondTestInvalid => "R14",
            Self::NoReconfirmation => "R15",
            Self::TargetTooClose => "R16",
            Self::RiskRejected => "R17",
            Self::PortfolioRiskRejected => "R18",
            Self::CorrelationConflict => "R19",
            Self::DuplicateSetup => "R20",
            Self::NewsBlackout => "R21",
            Self::MissedEntry => "R22",
            Self::TimeCutoff => "R23",
            Self::ProcessError => "R24",
            Self::BrokerUnsafe => "R25",
        }
    }
}
