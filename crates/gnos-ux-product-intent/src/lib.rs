#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

//! Passive GNOS.UX product-intent vocabulary.
//!
//! These types preserve exact semantic identity selected from a read-only
//! desktop projection. They carry no compositor capability, authority session
//! epoch, executor handle, transport, retry policy, or authorization decision.

use gnosix_desktop_model::{DesktopContextGeneration, SurfaceId, WorkspaceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProductIntentRequestId([u8; 16]);

impl ProductIntentRequestId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Exact workspace identity as observed by GNOS.UX at one semantic generation.
///
/// A display label cannot construct this reference:
///
/// ```compile_fail
/// use gnos_ux_product_intent::ObservedWorkspaceReference;
/// use gnosix_desktop_model::DesktopContextGeneration;
/// let _ = ObservedWorkspaceReference::new(
///     "workspace 11",
///     DesktopContextGeneration(7),
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObservedWorkspaceReference {
    workspace_id: WorkspaceId,
    observed_generation: DesktopContextGeneration,
}

impl ObservedWorkspaceReference {
    pub const fn new(
        workspace_id: WorkspaceId,
        observed_generation: DesktopContextGeneration,
    ) -> Self {
        Self {
            workspace_id,
            observed_generation,
        }
    }
    pub const fn workspace_id(self) -> WorkspaceId {
        self.workspace_id
    }

    pub const fn observed_generation(self) -> DesktopContextGeneration {
        self.observed_generation
    }
}

/// Exact application surface identity as observed by GNOS.UX at one semantic generation.
///
/// An application title cannot construct this reference:
///
/// ```compile_fail
/// use gnos_ux_product_intent::ObservedSurfaceReference;
/// use gnosix_desktop_model::DesktopContextGeneration;
/// let _ = ObservedSurfaceReference::new(
///     "Terminal",
///     DesktopContextGeneration(7),
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObservedSurfaceReference {
    surface_id: SurfaceId,
    observed_generation: DesktopContextGeneration,
}

impl ObservedSurfaceReference {
    pub const fn new(surface_id: SurfaceId, observed_generation: DesktopContextGeneration) -> Self {
        Self {
            surface_id,
            observed_generation,
        }
    }

    pub const fn surface_id(self) -> SurfaceId {
        self.surface_id
    }

    pub const fn observed_generation(self) -> DesktopContextGeneration {
        self.observed_generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProductIntent {
    ActivateObservedWorkspace {
        request_id: ProductIntentRequestId,
        target: ObservedWorkspaceReference,
    },
    FocusObservedApplication {
        request_id: ProductIntentRequestId,
        target: ObservedSurfaceReference,
    },
}

impl ProductIntent {
    pub const fn activate_observed_workspace(
        request_id: ProductIntentRequestId,
        target: ObservedWorkspaceReference,
    ) -> Self {
        Self::ActivateObservedWorkspace { request_id, target }
    }

    pub const fn focus_observed_application(
        request_id: ProductIntentRequestId,
        target: ObservedSurfaceReference,
    ) -> Self {
        Self::FocusObservedApplication { request_id, target }
    }

    pub const fn request_id(self) -> ProductIntentRequestId {
        match self {
            Self::ActivateObservedWorkspace { request_id, .. }
            | Self::FocusObservedApplication { request_id, .. } => request_id,
        }
    }
}
