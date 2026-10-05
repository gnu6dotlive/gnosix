use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("DesktopContextGeneration counter exhausted; integer overflow reached")]
pub struct GenerationExhausted;

/// Monotonic generation counter governing desktop state updates.
///
/// Invariant: Monotonically increasing; wrap-around to 0 is strictly forbidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DesktopContextGeneration(pub u64);

impl DesktopContextGeneration {
    pub const INITIAL: Self = Self(0);

    pub fn checked_next(self) -> Result<Self, GenerationExhausted> {
        self.0.checked_add(1).map(Self).ok_or(GenerationExhausted)
    }
}
