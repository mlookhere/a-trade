/// Mandatory condition semantics from canonical §108.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Condition {
    True,
    False,
    #[default]
    Unknown,
}

impl Condition {
    #[must_use]
    pub const fn permits(self) -> bool {
        matches!(self, Self::True)
    }
}

impl From<bool> for Condition {
    fn from(value: bool) -> Self {
        if value { Self::True } else { Self::False }
    }
}
