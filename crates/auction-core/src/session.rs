/// New-entry timing is governed by canonical §§3-4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EtTime(u32);

impl EtTime {
    pub const fn from_hms(hour: u8, minute: u8, second: u8) -> Option<Self> {
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(Self(
            hour as u32 * 3600 + minute as u32 * 60 + second as u32,
        ))
    }

    const fn seconds(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionPermissions {
    pub allow_new_entries: bool,
    pub allow_position_management: bool,
    pub allow_position_adds: bool,
}

impl SessionPermissions {
    #[must_use]
    pub fn at(time_et: EtTime) -> Self {
        const ENTRY_START: u32 = 9 * 3600 + 30 * 60;
        const ENTRY_END_EXCLUSIVE: u32 = 11 * 3600;

        let allow_new_entries = (ENTRY_START..ENTRY_END_EXCLUSIVE).contains(&time_et.seconds());

        Self {
            allow_new_entries,
            // Canonical §4 explicitly preserves existing-position management at/after 11:00.
            // Safety management is therefore never disabled by the entry-window gate.
            allow_position_management: true,
            allow_position_adds: allow_new_entries,
        }
    }
}
