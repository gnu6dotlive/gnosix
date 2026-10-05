use crate::{scale::ScaleMilli, spatial::LogicalRect, workspace::WorkspaceId};
use std::num::NonZeroU32;
use thiserror::Error;

/// Opaque logical identifier for an Output Hub.
///
/// Invariant: Fixed-width 128-bit token. Equality asserts referential identity within the
/// model. Makes no claim of uniqueness, collision resistance, allocation, or persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputHubId([u8; 16]);

impl OutputHubId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Observed physical output descriptors supplied by an external observation adapter.
///
/// Invariant: Preserves exact strings supplied by observer. Equality represents identical
/// recorded descriptors; does not assert unique device identity or matching success.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PhysicalOutputDescriptors {
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
}

/// Typed Wayland output orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OutputTransform {
    #[default]
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

/// Physical mode pixel resolution before scaling and rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalPixelSize {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

impl PhysicalPixelSize {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            width: NonZeroU32::new(width)?,
            height: NonZeroU32::new(height)?,
        })
    }
}

/// Logical compositor-space point. Negative coordinates are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LogicalPoint {
    pub x: i32,
    pub y: i32,
}

impl LogicalPoint {
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Logical pixel dimensions after scaling and rotation as seen by compositor layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LogicalSize {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

impl LogicalSize {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            width: NonZeroU32::new(width)?,
            height: NonZeroU32::new(height)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("Connector name cannot be empty")]
pub struct EmptyConnectorNameError;

/// Strongly typed connector name (e.g. "HDMI-A-1", "DP-2").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectorName(String);

impl ConnectorName {
    pub fn new(name: impl Into<String>) -> Result<Self, EmptyConnectorNameError> {
        let s = name.into();
        if s.trim().is_empty() {
            Err(EmptyConnectorNameError)
        } else {
            Ok(Self(s))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Passive composite representation of an output state.
///
/// Invariant: Authority-neutral aggregation of caller-supplied OutputHubId
/// and admitted physical/logical output attributes. Does not perform matching,
/// allocation, persistence, normalization, or hardware querying.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OutputState {
    pub output_hub_id: OutputHubId,
    pub connector: ConnectorName,
    /// Current workspace presented by this output; independent of global focus.
    pub active_workspace_id: Option<WorkspaceId>,
    pub descriptors: PhysicalOutputDescriptors,
    pub physical_size: PhysicalPixelSize,
    pub logical_position: LogicalPoint,
    pub logical_size: LogicalSize,
    pub scale: ScaleMilli,
    pub transform: OutputTransform,
}

impl OutputState {
    pub const fn logical_bounds(&self) -> LogicalRect {
        LogicalRect::new(self.logical_position, self.logical_size)
    }
}

/// Opaque, validated collection of output states.
///
/// Invariants:
/// - Contains 0..N `OutputState` values.
/// - Uniqueness: Each `OutputState.output_hub_id` projection within the collection is strictly unique.
/// - Safe construction: Enforced at construction boundary via `from_outputs`.
/// - Order: Representation-only and non-semantic. Iteration order is unstable and carries no
///   priority, primary status, or z-order semantics.
/// - Equality: Unordered structural equality. Two collections are equal iff they have the same
///   length and every element in one collection matches an element in the other with the same
///   `output_hub_id` and equal attributes.
#[derive(Debug)]
pub struct ValidatedOutputCollection {
    outputs: Vec<OutputState>,
}

impl ValidatedOutputCollection {
    /// Validates and constructs an opaque collection from a vector of output states.
    ///
    /// Returns `Some(Self)` iff every `OutputState.output_hub_id` in `outputs` is strictly unique.
    /// Returns `None` if any duplicate `OutputHubId` is detected.
    ///
    /// Preserves all `OutputState` values unchanged. An empty vector produces a valid empty collection.
    pub fn from_outputs(outputs: Vec<OutputState>) -> Option<Self> {
        let len = outputs.len();
        for i in 0..len {
            for j in (i + 1)..len {
                if outputs[i].output_hub_id == outputs[j].output_hub_id {
                    return None;
                }
            }
        }
        Some(Self { outputs })
    }

    /// Returns the number of outputs in the validated collection.
    pub fn len(&self) -> usize {
        self.outputs.len()
    }

    /// Returns `true` if the collection contains zero outputs.
    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }

    /// Returns an iterator over references to the outputs in the collection.
    ///
    /// Note: Iteration order is representation-only and non-stable. Callers must not infer
    /// priority, primary display status, or spatial ordering from position.
    pub fn iter(&self) -> impl Iterator<Item = &OutputState> {
        self.outputs.iter()
    }

    pub fn contains_hub_id(&self, output_hub_id: OutputHubId) -> bool {
        self.outputs
            .iter()
            .any(|output| output.output_hub_id == output_hub_id)
    }
}

impl PartialEq for ValidatedOutputCollection {
    fn eq(&self, other: &Self) -> bool {
        if self.outputs.len() != other.outputs.len() {
            return false;
        }
        self.outputs.iter().all(|a| {
            other
                .outputs
                .iter()
                .any(|b| a.output_hub_id == b.output_hub_id && a == b)
        })
    }
}

impl Eq for ValidatedOutputCollection {}
