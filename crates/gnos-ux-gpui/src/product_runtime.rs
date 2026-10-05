//! Coherent GNOS.UX product state shared by product views.
//!
//! The runtime is passive: it derives product state from the already-selected
//! desktop projection and records typed product-action lifecycle. It owns no
//! compositor adapter, D-Bus connection, systemd handle, or authority token.

use crate::{
    product_preferences::EffectiveProductPreferences,
    product_scene::{ProductApplicationContent, ProductSemanticState, ProductWorkspaceContent},
};
use gnos_ux_product_intent::{ObservedSurfaceReference, ObservedWorkspaceReference};
use gnos_ux_surface::envelope::RuntimeId;
use gnosix_application_catalog::{ApplicationCatalog, CatalogGeneration, DesktopEntryId};
use gnosix_application_launch::LaunchReference;
use gnosix_desktop_model::{DesktopContextGeneration, DesktopOutputProjection};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductActionKind {
    ActivateWorkspace,
    FocusApplication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductActionTarget {
    Workspace(ObservedWorkspaceReference),
    Application(ObservedSurfaceReference),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductActionTerminal {
    Applied,
    AlreadySatisfied,
    Stale,
    UnknownTarget,
    NotAuthorized,
    TargetDisappeared,
    SessionUnavailable,
    BackendRejected,
    ObservationFailure,
    ProtocolFailure,
    TransportFailure,
    PostconditionMismatch,
    ExecutionBindingChanged,
    LocalStale,
    SemanticUnavailable,
    ClientTransport,
    ClientProtocol,
}

impl ProductActionTerminal {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "Applied",
            Self::AlreadySatisfied => "AlreadySatisfied",
            Self::Stale => "Stale",
            Self::UnknownTarget => "UnknownTarget",
            Self::NotAuthorized => "NotAuthorized",
            Self::TargetDisappeared => "TargetDisappeared",
            Self::SessionUnavailable => "SessionUnavailable",
            Self::BackendRejected => "BackendRejected",
            Self::ObservationFailure => "ObservationFailure",
            Self::ProtocolFailure => "ProtocolFailure",
            Self::TransportFailure => "TransportFailure",
            Self::PostconditionMismatch => "PostconditionMismatch",
            Self::ExecutionBindingChanged => "ExecutionBindingChanged",
            Self::LocalStale => "LocalStale",
            Self::SemanticUnavailable => "SemanticUnavailable",
            Self::ClientTransport => "ClientTransport",
            Self::ClientProtocol => "ClientProtocol",
        }
    }

    pub const fn is_success(self) -> bool {
        matches!(self, Self::Applied | Self::AlreadySatisfied)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductActionRecord {
    pub kind: ProductActionKind,
    pub target: ProductActionTarget,
    pub label: String,
    pub selected_generation: DesktopContextGeneration,
    pub current_generation: Option<DesktopContextGeneration>,
    pub terminal: Option<ProductActionTerminal>,
    pub detail: String,
}

/// One installed application as a product presents it.
///
/// The `reference` is captured from a catalog snapshot, not built at click
/// time. That matters: it is what lets the launch capability refuse a
/// selection that was made against a catalog which has since changed,
/// instead of launching whatever the catalog holds now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductInstalledApplicationContent {
    pub reference: LaunchReference,
    pub name: String,
    pub icon: Option<String>,
    pub terminal: bool,
}

/// The outcome of one launch, in the vocabulary of launching.
///
/// Deliberately not [`ProductActionTerminal`]. That vocabulary belongs to the
/// activation authority, where `Applied` and `UnknownTarget` describe a
/// decision somebody else made. A launch is resolved and started locally, and
/// its failures are local and specific: a missing `TryExec`, an unconfigured
/// terminal, a program that would not start. Reusing the authority words here
/// would make a local refusal look like a remote decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductLaunchOutcome {
    Launched { pid: u32 },
    SpawnFailed { detail: String },
    Refused { code: &'static str, detail: String },
}

impl ProductLaunchOutcome {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Launched { .. } => "LAUNCHED",
            Self::SpawnFailed { .. } => "SPAWN_FAILED",
            Self::Refused { code, .. } => code,
        }
    }

    pub fn is_launched(&self) -> bool {
        matches!(self, Self::Launched { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductLaunchRecord {
    pub entry: DesktopEntryId,
    pub catalog_generation: CatalogGeneration,
    pub label: String,
    pub terminal: Option<ProductLaunchOutcome>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ProductLaunchState {
    #[default]
    Idle,
    Pending(ProductLaunchRecord),
    Terminal(ProductLaunchRecord),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ProductActionState {
    #[default]
    Idle,
    Pending(ProductActionRecord),
    Terminal(ProductActionRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductRuntime {
    semantic: ProductSemanticState,
    preferences: EffectiveProductPreferences,
    application_catalog: Arc<ApplicationCatalog>,
    launch_action: ProductLaunchState,
    action: ProductActionState,
}

impl Default for ProductRuntime {
    fn default() -> Self {
        Self {
            semantic: ProductSemanticState::Unavailable,
            preferences: EffectiveProductPreferences::default(),
            application_catalog: Arc::new(ApplicationCatalog::empty()),
            launch_action: ProductLaunchState::Idle,
            action: ProductActionState::Idle,
        }
    }
}

impl ProductRuntime {
    pub fn from_projection(
        expected_output: Option<&RuntimeId>,
        projection: Option<&DesktopOutputProjection>,
    ) -> Self {
        let mut runtime = Self::default();
        runtime.refresh(expected_output, projection);
        runtime
    }

    #[cfg(feature = "component-lab")]
    pub fn from_lab_parts(
        semantic: ProductSemanticState,
        preferences: EffectiveProductPreferences,
        application_catalog: Arc<ApplicationCatalog>,
        action: ProductActionState,
    ) -> Self {
        Self {
            semantic,
            preferences,
            application_catalog,
            // Lab fixtures model an already-settled session, so the launch
            // lifecycle starts idle rather than with invented launch state.
            launch_action: ProductLaunchState::Idle,
            action,
        }
    }

    pub fn refresh(
        &mut self,
        expected_output: Option<&RuntimeId>,
        projection: Option<&DesktopOutputProjection>,
    ) -> bool {
        let next = expected_output.map_or(ProductSemanticState::Unavailable, |output| {
            ProductSemanticState::from_projection(output, projection)
        });
        let changed = self.semantic != next;
        self.semantic = next;
        changed
    }

    pub fn semantic(&self) -> &ProductSemanticState {
        &self.semantic
    }

    pub fn current_generation(&self) -> Option<DesktopContextGeneration> {
        self.semantic.current_generation()
    }

    pub fn preferences(&self) -> &EffectiveProductPreferences {
        &self.preferences
    }

    pub fn application_catalog(&self) -> &ApplicationCatalog {
        self.application_catalog.as_ref()
    }

    /// Installed applications the product may offer, as content bound to the
    /// catalog snapshot that produced them.
    ///
    /// The reference is built here, from the catalog the runtime currently
    /// holds, so every content item is already fenced to a real generation. A
    /// catalog refresh therefore makes previously-built content stale, which
    /// is exactly the intended behaviour: the user picked an entry from the
    /// catalog they could see, and not from the one that replaced it.
    pub fn installed_applications(&self) -> Vec<ProductInstalledApplicationContent> {
        let catalog = self.application_catalog.as_ref();
        catalog
            .visible_entries()
            .map(|entry| ProductInstalledApplicationContent {
                reference: LaunchReference::new(entry.id.clone(), catalog.generation().clone()),
                name: entry.name.clone(),
                icon: entry.icon.clone(),
                terminal: entry.terminal,
            })
            .collect()
    }

    pub fn launch_state(&self) -> &ProductLaunchState {
        &self.launch_action
    }

    pub(crate) fn begin_launch_action(&mut self, selected: &ProductInstalledApplicationContent) {
        self.launch_action = ProductLaunchState::Pending(ProductLaunchRecord {
            entry: selected.reference.entry().clone(),
            catalog_generation: selected.reference.generation().clone(),
            label: selected.name.clone(),
            terminal: None,
            detail: String::new(),
        });
    }

    pub(crate) fn finish_launch_action(
        &mut self,
        outcome: ProductLaunchOutcome,
        detail: impl Into<String>,
    ) -> ProductLaunchRecord {
        let mut record = match &self.launch_action {
            ProductLaunchState::Pending(record) => record.clone(),
            ProductLaunchState::Idle | ProductLaunchState::Terminal(_) => {
                panic!("launch action must be pending before terminal state")
            }
        };
        record.terminal = Some(outcome);
        record.detail = detail.into();
        self.launch_action = ProductLaunchState::Terminal(record.clone());
        record
    }

    pub fn set_application_catalog(&mut self, catalog: Arc<ApplicationCatalog>) -> bool {
        let changed = !Arc::ptr_eq(&self.application_catalog, &catalog)
            && self.application_catalog.as_ref() != catalog.as_ref();
        self.application_catalog = catalog;
        changed
    }

    pub fn set_preferences(&mut self, preferences: EffectiveProductPreferences) -> bool {
        let changed = self.preferences != preferences;
        self.preferences = preferences;
        changed
    }

    pub fn action_state(&self) -> &ProductActionState {
        &self.action
    }

    #[cfg(test)]
    pub(crate) fn refresh_semantic_for_test(&mut self, semantic: ProductSemanticState) {
        self.semantic = semantic;
    }

    pub fn workspace_manager_targets(&self) -> &[ProductWorkspaceContent] {
        self.semantic.workspace_manager_targets()
    }

    pub fn workspace_manager_is_active(&self, target: &ProductWorkspaceContent) -> bool {
        self.semantic.workspace_manager_is_active(target)
    }

    pub fn application_targets(&self) -> &[ProductApplicationContent] {
        self.semantic.application_targets()
    }

    pub fn application_is_focused(&self, target: &ProductApplicationContent) -> bool {
        self.semantic.application_is_focused(target)
    }

    pub fn workspace_activation_feedback(&self) -> Option<String> {
        let record = match &self.action {
            ProductActionState::Idle => return None,
            ProductActionState::Pending(record) => {
                return Some(format!("{} · en cours", record.label));
            }
            ProductActionState::Terminal(record) => record,
        };
        let terminal = record.terminal?;
        Some(if terminal.is_success() {
            format!("{} · {}", record.label, terminal.as_str())
        } else if matches!(
            terminal,
            ProductActionTerminal::SemanticUnavailable
                | ProductActionTerminal::SessionUnavailable
                | ProductActionTerminal::ClientTransport
        ) {
            format!("{} · indisponible", record.label)
        } else {
            format!("{} · refusé", record.label)
        })
    }

    #[cfg(any(test, feature = "r13-production-client"))]
    pub(crate) fn begin_workspace_action(
        &mut self,
        selected: &ProductWorkspaceContent,
        current_generation: Option<DesktopContextGeneration>,
    ) {
        self.begin_action(
            ProductActionKind::ActivateWorkspace,
            ProductActionTarget::Workspace(selected.reference),
            selected.label.clone(),
            selected.reference.observed_generation(),
            current_generation,
        );
    }

    #[cfg(feature = "r13-production-client")]
    pub(crate) fn begin_application_action(
        &mut self,
        selected: &ProductApplicationContent,
        current_generation: Option<DesktopContextGeneration>,
    ) {
        self.begin_action(
            ProductActionKind::FocusApplication,
            ProductActionTarget::Application(selected.reference),
            selected.title.clone(),
            selected.reference.observed_generation(),
            current_generation,
        );
    }

    #[cfg(any(test, feature = "r13-production-client"))]
    fn begin_action(
        &mut self,
        kind: ProductActionKind,
        target: ProductActionTarget,
        label: String,
        selected_generation: DesktopContextGeneration,
        current_generation: Option<DesktopContextGeneration>,
    ) {
        self.action = ProductActionState::Pending(ProductActionRecord {
            kind,
            target,
            label,
            selected_generation,
            current_generation,
            terminal: None,
            detail: String::new(),
        });
    }

    #[cfg(any(test, feature = "r13-production-client"))]
    pub(crate) fn finish_action(
        &mut self,
        terminal: ProductActionTerminal,
        detail: impl Into<String>,
    ) -> ProductActionRecord {
        let mut record = match &self.action {
            ProductActionState::Pending(record) => record.clone(),
            ProductActionState::Idle | ProductActionState::Terminal(_) => {
                panic!("workspace action must be pending before terminal state")
            }
        };
        record.terminal = Some(terminal);
        record.detail = detail.into();
        self.action = ProductActionState::Terminal(record.clone());
        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_ux_product_intent::ObservedWorkspaceReference;
    use gnosix_desktop_model::WorkspaceId;

    fn workspace(label: &str, generation: u64) -> ProductWorkspaceContent {
        ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(
                WorkspaceId::from_bytes([7; 16]),
                DesktopContextGeneration(generation),
            ),
            label: label.to_owned(),
        }
    }

    #[test]
    fn passive_runtime_starts_unavailable_and_action_idle() {
        let runtime = ProductRuntime::default();
        assert_eq!(runtime.semantic(), &ProductSemanticState::Unavailable);
        assert_eq!(runtime.action_state(), &ProductActionState::Idle);
        assert_eq!(runtime.workspace_activation_feedback(), None);
    }

    #[test]
    fn application_catalog_snapshot_is_available_without_xdg_io_in_runtime() {
        let runtime = ProductRuntime::default();
        assert!(runtime.application_catalog().entries().is_empty());
        assert!(!runtime.application_catalog().generation().hex().is_empty());
    }

    #[test]
    fn refresh_drops_last_good_semantics_when_projection_disappears() {
        let mut runtime = ProductRuntime::default();
        runtime.refresh_semantic_for_test(ProductSemanticState::Current(Box::new(
            crate::product_scene::ProductContent {
                generation: DesktopContextGeneration(7),
                connector: "HDMI-A-1".to_owned(),
                active_workspace: None,
                focused_workspace: None,
                workspace_manager_targets: vec![],
                visible_workspaces: vec![],
                applications: vec![],
                visible_application_count: 0,
                focused_application: None,
            },
        )));
        assert_eq!(
            runtime.current_generation(),
            Some(DesktopContextGeneration(7))
        );

        assert!(runtime.refresh(Some(&RuntimeId::new("HDMI-A-1")), None));
        assert_eq!(runtime.semantic(), &ProductSemanticState::Unavailable);
        assert_eq!(runtime.current_generation(), None);
    }

    #[test]
    fn action_lifecycle_is_typed_and_feedback_is_derived() {
        let mut runtime = ProductRuntime::default();
        let target = workspace("2", 7);
        runtime.begin_workspace_action(&target, Some(DesktopContextGeneration(7)));
        assert_eq!(
            runtime.workspace_activation_feedback().as_deref(),
            Some("2 · en cours")
        );

        let record =
            runtime.finish_action(ProductActionTerminal::Applied, "Applied: workspace focused");
        assert_eq!(record.terminal, Some(ProductActionTerminal::Applied));
        assert_eq!(
            runtime.workspace_activation_feedback().as_deref(),
            Some("2 · Applied")
        );
    }

    #[test]
    fn unavailable_terminal_maps_to_unavailable_feedback() {
        let mut runtime = ProductRuntime::default();
        let target = workspace("2", 7);
        runtime.begin_workspace_action(&target, None);
        runtime.finish_action(
            ProductActionTerminal::SemanticUnavailable,
            "semantic unavailable",
        );
        assert_eq!(
            runtime.workspace_activation_feedback().as_deref(),
            Some("2 · indisponible")
        );
    }
}
