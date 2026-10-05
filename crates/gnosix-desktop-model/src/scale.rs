use std::num::NonZeroU32;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ScaleError {
    #[error("Scale milli must be strictly non-zero")]
    Zero,
}

/// Integer milliscale representation ensuring deterministic comparisons (Eq, Ord, Hash).
///
/// Invariant: `0 < value <= u32::MAX`.
/// 1000 represents 1.0x, 1250 represents 1.25x, 1500 represents 1.5x, 2000 represents 2.0x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScaleMilli(NonZeroU32);

impl ScaleMilli {
    pub const ONE: Self = match NonZeroU32::new(1000) {
        Some(v) => Self(v),
        None => unreachable!(),
    };

    pub fn from_milli(milli: u32) -> Result<Self, ScaleError> {
        NonZeroU32::new(milli).map(Self).ok_or(ScaleError::Zero)
    }

    pub fn as_milli(self) -> u32 {
        self.0.get()
    }
}
