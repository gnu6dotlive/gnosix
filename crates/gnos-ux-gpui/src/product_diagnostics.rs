//! In-process diagnostics derived from the canonical ProductRuntime.
//!
//! This module performs no observation, IPC, mutation, or authority work. It
//! formats state already owned by the product host.

use crate::{
    product_runtime::{ProductActionKind, ProductActionState, ProductActionTarget, ProductRuntime},
    product_scene::ProductSemanticState,
};
use gnosix_desktop_model::{SurfaceVisibility, WorkspaceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductDiagnosticSemanticState {
    Unavailable,
    Invalid,
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductDiagnosticWorkspace {
    pub id: String,
    pub label: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductDiagnosticApplication {
    pub id: String,
    pub title: String,
    pub class: String,
    pub focused: bool,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductDiagnosticSnapshot {
    pub semantic_state: ProductDiagnosticSemanticState,
    pub generation: Option<u64>,
    pub connector: Option<String>,
    pub workspaces: Vec<ProductDiagnosticWorkspace>,
    pub applications: Vec<ProductDiagnosticApplication>,
    pub preference_summary: String,
    pub action_summary: String,
    pub catalog_summary: String,
    /// Whether the AppStack surface is open on this output, and how much it is
    /// offering.
    ///
    /// Observation only. Diagnostics must be able to say what the product is
    /// doing without being able to make it do anything, so there is no argv, no
    /// process handle, and no way to trigger a launch from here.
    pub appstack_summary: String,
}

impl ProductDiagnosticSnapshot {
    pub fn from_runtime(runtime: &ProductRuntime) -> Self {
        let (semantic_state, generation, connector, workspaces, applications) =
            match runtime.semantic() {
                ProductSemanticState::Unavailable => (
                    ProductDiagnosticSemanticState::Unavailable,
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                ),
                ProductSemanticState::Invalid => (
                    ProductDiagnosticSemanticState::Invalid,
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                ),
                ProductSemanticState::Current(content) => {
                    let active = content.active_workspace.as_ref().map(|item| item.reference);
                    let workspaces = content
                        .workspace_manager_targets
                        .iter()
                        .map(|workspace| ProductDiagnosticWorkspace {
                            id: short_id(workspace.reference.workspace_id().as_bytes()),
                            label: workspace.label.clone(),
                            active: active == Some(workspace.reference),
                        })
                        .collect();
                    let applications = content
                        .applications
                        .iter()
                        .map(|application| ProductDiagnosticApplication {
                            id: short_id(application.reference.surface_id().as_bytes()),
                            title: application.title.clone(),
                            class: application.class.clone(),
                            focused: content
                                .focused_application
                                .as_ref()
                                .is_some_and(|focused| focused.reference == application.reference),
                            visible: application.visibility == SurfaceVisibility::Visible,
                        })
                        .collect();
                    (
                        ProductDiagnosticSemanticState::Current,
                        Some(content.generation.0),
                        Some(content.connector.clone()),
                        workspaces,
                        applications,
                    )
                }
            };

        let preferences = runtime.preferences();
        let preference_summary = format!(
            "color={:?}/{:?} motion={}/{:?} scale={}bp/{:?} projection={:?}",
            preferences.color_scheme,
            preferences.color_scheme_source,
            preferences.reduced_motion,
            preferences.reduced_motion_source,
            preferences.text_scale_basis_points,
            preferences.text_scale_source,
            preferences.projection_state,
        );

        let action_summary = match runtime.action_state() {
            ProductActionState::Idle => "idle".to_owned(),
            ProductActionState::Pending(record) => format!(
                "pending {:?} target={} selected_gen={} current_gen={}",
                record.kind,
                action_target(record.target),
                record.selected_generation.0,
                record
                    .current_generation
                    .map_or_else(|| "none".to_owned(), |generation| generation.0.to_string())
            ),
            ProductActionState::Terminal(record) => format!(
                "terminal {:?} target={} outcome={} selected_gen={} current_gen={}",
                record.kind,
                action_target(record.target),
                record.terminal.map_or("none", |terminal| terminal.as_str()),
                record.selected_generation.0,
                record
                    .current_generation
                    .map_or_else(|| "none".to_owned(), |generation| generation.0.to_string())
            ),
        };

        let catalog = runtime.application_catalog();
        let catalog_summary = format!(
            "{} entries / {} visible / {} issues / gen {}",
            catalog.entries().len(),
            catalog.visible_entries().count(),
            catalog.issues().len(),
            &catalog.generation().hex()[..12],
        );

        let installed = runtime.installed_applications();
        let mut appstack_summary = format!("closed installed={}", installed.len());
        match runtime.launch_state() {
            crate::product_runtime::ProductLaunchState::Idle => {}
            crate::product_runtime::ProductLaunchState::Pending(_) => {
                appstack_summary.push_str(" launch=PENDING");
            }
            crate::product_runtime::ProductLaunchState::Terminal(record) => {
                let code = record
                    .terminal
                    .as_ref()
                    .map_or("NONE", |terminal| terminal.code());
                appstack_summary
                    .push_str(&format!(" launch={code} entry={}", record.entry.as_str()));
            }
        }

        Self {
            semantic_state,
            generation,
            connector,
            workspaces,
            applications,
            preference_summary,
            action_summary,
            catalog_summary,
            appstack_summary,
        }
    }

    pub fn compact_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "semantic={:?} generation={} output={}",
            self.semantic_state,
            self.generation
                .map_or_else(|| "none".to_owned(), |generation| generation.to_string()),
            self.connector.as_deref().unwrap_or("none"),
        ));
        lines.push(format!(
            "workspaces={} applications={}",
            self.workspaces.len(),
            self.applications.len()
        ));
        for workspace in &self.workspaces {
            lines.push(format!(
                "ws {} {}{}",
                workspace.id,
                workspace.label,
                if workspace.active { " [active]" } else { "" }
            ));
        }
        for application in self.applications.iter().take(4) {
            lines.push(format!(
                "app {} {} / {}{}{}",
                application.id,
                application.title,
                application.class,
                if application.focused {
                    " [focused]"
                } else {
                    ""
                },
                if application.visible { "" } else { " [hidden]" }
            ));
        }
        if self.applications.len() > 4 {
            lines.push(format!("… {} more app(s)", self.applications.len() - 4));
        }
        lines.push(self.preference_summary.clone());
        lines.push(format!("action={}", self.action_summary));
        lines.push(format!("catalog={}", self.catalog_summary));
        lines.push(format!("appstack={}", self.appstack_summary));
        lines.join("\n")
    }
}

fn short_id(bytes: &[u8; 16]) -> String {
    bytes
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn action_target(target: ProductActionTarget) -> String {
    match target {
        ProductActionTarget::Workspace(reference) => {
            format!(
                "workspace:{}",
                short_id(reference.workspace_id().as_bytes())
            )
        }
        ProductActionTarget::Application(reference) => {
            format!("surface:{}", short_id(reference.surface_id().as_bytes()))
        }
    }
}

pub fn action_kind_label(kind: ProductActionKind) -> &'static str {
    match kind {
        ProductActionKind::ActivateWorkspace => "activate-workspace",
        ProductActionKind::FocusApplication => "focus-application",
    }
}

pub fn workspace_id_label(id: WorkspaceId) -> String {
    short_id(id.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        product_runtime::ProductRuntime,
        product_scene::{ProductContent, ProductWorkspaceContent},
    };
    use gnos_ux_product_intent::ObservedWorkspaceReference;
    use gnosix_desktop_model::{DesktopContextGeneration, WorkspaceId};

    #[test]
    fn unavailable_runtime_has_no_fake_last_good_data() {
        let snapshot = ProductDiagnosticSnapshot::from_runtime(&ProductRuntime::default());
        assert_eq!(
            snapshot.semantic_state,
            ProductDiagnosticSemanticState::Unavailable
        );
        assert_eq!(snapshot.generation, None);
        assert!(snapshot.workspaces.is_empty());
        assert!(snapshot.applications.is_empty());
        assert!(snapshot.compact_text().contains("semantic=Unavailable"));
    }

    #[test]
    fn current_runtime_reports_exact_generation_and_abbreviated_workspace_ids() {
        let workspace_id = WorkspaceId::from_bytes([
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0, 1, 2, 3, 4, 5, 6, 7,
        ]);
        let workspace = ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(workspace_id, DesktopContextGeneration(42)),
            label: "dev".to_owned(),
        };
        let mut runtime = ProductRuntime::default();
        runtime.refresh_semantic_for_test(ProductSemanticState::Current(Box::new(
            ProductContent {
                generation: DesktopContextGeneration(42),
                connector: "HDMI-A-1".to_owned(),
                active_workspace: Some(workspace.clone()),
                focused_workspace: Some(workspace.clone()),
                workspace_manager_targets: vec![workspace.clone()],
                visible_workspaces: vec![workspace],
                applications: vec![],
                visible_application_count: 0,
                focused_application: None,
            },
        )));

        let snapshot = ProductDiagnosticSnapshot::from_runtime(&runtime);
        assert_eq!(
            snapshot.semantic_state,
            ProductDiagnosticSemanticState::Current
        );
        assert_eq!(snapshot.generation, Some(42));
        assert_eq!(snapshot.connector.as_deref(), Some("HDMI-A-1"));
        assert_eq!(snapshot.workspaces[0].id, "123456789abc");
        assert!(snapshot.workspaces[0].active);
    }
}
