//! Deterministic ProductRuntime fixtures for the GNOS.UX component lab.
//!
//! Fixtures never observe or mutate the live desktop. They construct the same
//! product models consumed by production views.

use std::sync::Arc;

use crate::{
    product_design::ProductDesignContext,
    product_preferences::{
        EffectiveProductPreferences, PreferenceValueSource, ProductColorScheme,
        ProductPreferenceProjectionState,
    },
    product_runtime::{
        ProductActionKind, ProductActionRecord, ProductActionState, ProductActionTarget,
        ProductActionTerminal, ProductRuntime,
    },
    product_scene::{
        ProductApplicationContent, ProductContent, ProductSemanticState, ProductWorkspaceContent,
    },
};
use gnos_ux_product_intent::{ObservedSurfaceReference, ObservedWorkspaceReference};
use gnosix_application_catalog::ApplicationCatalog;
use gnosix_desktop_model::{
    DesktopContextGeneration, SurfaceFocus, SurfaceId, SurfaceVisibility, WorkspaceId,
};

pub const FIXTURE_NAMES: &[&str] = &[
    "workspace-single",
    "workspace-hidden",
    "workspace-many",
    "apps-empty",
    "apps-duplicates",
    "multi-output",
    "semantic-unavailable",
    "semantic-invalid",
    "stale-selection",
    "profile-missing",
    "profile-unbound",
    "profile-recovery",
    "long-text-2x",
    "notification-long-2x",
    "reduced-motion",
    "action-states",
    "appstack-basic",
    "appstack-duplicate-names",
    "appstack-refused",
    "appstack-empty",
    "appstack-long-title-2x",
    "appstack-joint",
];

pub struct ProductLabOutput {
    pub name: String,
    pub runtime: ProductRuntime,
}

pub struct ProductLabFixture {
    pub name: &'static str,
    pub description: &'static str,
    pub design: ProductDesignContext,
    pub outputs: Vec<ProductLabOutput>,
}

pub fn fixture(name: &str) -> Option<ProductLabFixture> {
    match name {
        "workspace-single" => Some(workspace_single()),
        "workspace-hidden" => Some(workspace_hidden()),
        "workspace-many" => Some(workspace_many()),
        "apps-empty" => Some(apps_empty()),
        "apps-duplicates" => Some(apps_duplicates()),
        "multi-output" => Some(multi_output()),
        "semantic-unavailable" => Some(semantic_unavailable()),
        "semantic-invalid" => Some(semantic_invalid()),
        "stale-selection" => Some(stale_selection()),
        "profile-missing" => Some(profile_state(
            "profile-missing",
            ProductPreferenceProjectionState::ProfileDocumentAbsent,
        )),
        "profile-unbound" => Some(profile_state(
            "profile-unbound",
            ProductPreferenceProjectionState::NoDurableBinding,
        )),
        "profile-recovery" => Some(profile_state(
            "profile-recovery",
            ProductPreferenceProjectionState::RecoveryRequired,
        )),
        "long-text-2x" => Some(long_text_2x()),
        "notification-long-2x" => Some(notification_long_2x()),
        "reduced-motion" => Some(reduced_motion()),
        "action-states" => Some(action_states()),
        "appstack-basic" => Some(appstack_basic()),
        "appstack-duplicate-names" => Some(appstack_duplicate_names()),
        "appstack-refused" => Some(appstack_refused()),
        "appstack-empty" => Some(appstack_empty()),
        "appstack-long-title-2x" => Some(appstack_long_title_2x()),
        "appstack-joint" => Some(appstack_joint()),
        _ => None,
    }
}

fn design(scale: f32, reduced_motion: bool) -> ProductDesignContext {
    ProductDesignContext::new(scale, reduced_motion).expect("lab design context")
}

fn preferences(state: ProductPreferenceProjectionState) -> EffectiveProductPreferences {
    EffectiveProductPreferences {
        color_scheme: ProductColorScheme::Dark,
        color_scheme_source: PreferenceValueSource::ProductDefault,
        reduced_motion: false,
        reduced_motion_source: PreferenceValueSource::ProductDefault,
        text_scale_basis_points: 10_000,
        text_scale_source: PreferenceValueSource::ProductDefault,
        bar_edge: Some(in_gnu_os_settings_read_model::BarEdgePreference::Top),
        bar_edge_source: Some(PreferenceValueSource::ProductDefault),
        bar_overrides: in_gnu_os_settings_read_model::BarPreferenceOverrides::default(),
        projection_state: state,
        identity_revision: None,
        profile_revision: None,
    }
}

fn runtime(
    semantic: ProductSemanticState,
    preferences: EffectiveProductPreferences,
    action: ProductActionState,
) -> ProductRuntime {
    ProductRuntime::from_lab_parts(
        semantic,
        preferences,
        Arc::new(ApplicationCatalog::empty()),
        action,
    )
}

fn workspace(id: u8, label: impl Into<String>, generation: u64) -> ProductWorkspaceContent {
    ProductWorkspaceContent {
        reference: ObservedWorkspaceReference::new(
            WorkspaceId::from_bytes([id; 16]),
            DesktopContextGeneration(generation),
        ),
        label: label.into(),
    }
}

fn application(
    id: u8,
    title: impl Into<String>,
    class: impl Into<String>,
    workspace: &ProductWorkspaceContent,
    generation: u64,
    visibility: SurfaceVisibility,
    focus: SurfaceFocus,
) -> ProductApplicationContent {
    let class = class.into();
    ProductApplicationContent {
        reference: ObservedSurfaceReference::new(
            SurfaceId::from_bytes([id; 16]),
            DesktopContextGeneration(generation),
        ),
        workspace: Some(workspace.reference),
        title: title.into(),
        initial_class: class.clone(),
        class,
        process_id: 10_000 + u32::from(id),
        xwayland: false,
        visibility,
        focus,
    }
}

fn current(
    connector: impl Into<String>,
    generation: u64,
    targets: Vec<ProductWorkspaceContent>,
    visible: Vec<ProductWorkspaceContent>,
    active_index: Option<usize>,
    applications: Vec<ProductApplicationContent>,
    focused_application_index: Option<usize>,
) -> ProductSemanticState {
    let active_workspace = active_index.and_then(|index| targets.get(index)).cloned();
    let focused_workspace = active_workspace.clone();
    let focused_application = focused_application_index
        .and_then(|index| applications.get(index))
        .cloned();
    let visible_application_count = applications
        .iter()
        .filter(|application| application.visibility == SurfaceVisibility::Visible)
        .count();
    ProductSemanticState::Current(Box::new(ProductContent {
        generation: DesktopContextGeneration(generation),
        connector: connector.into(),
        active_workspace,
        focused_workspace,
        workspace_manager_targets: targets,
        visible_workspaces: visible,
        applications,
        visible_application_count,
        focused_application,
    }))
}

/// Build a deterministic catalog from desktop-entry text.
///
/// Synthetic rather than discovered, for two reasons. A visual fixture that
/// depended on the test machine's installed applications would render
/// differently on every machine, and a fixture that discovered real entries
/// would be a live observation smuggled into something that claims to be
/// deterministic. The entries are handed to the real catalog parser, so the
/// fixtures exercise the same discovery path production uses.
fn synthetic_catalog(id: &str, entries: &[(&str, &str)]) -> ApplicationCatalog {
    // A per-call sequence number, not just the fixture id. Keyed only by id, two
    // tests that both build `appstack-basic` shared one scratch directory, and
    // because cargo runs tests on several threads one test's `remove_dir_all`
    // could delete the other's entries between writing them and discovering
    // them - producing a fixture that silently offered nothing. That failure
    // reproduced on two runs out of three, which is the worst kind: mostly
    // green, and green for a reason that has nothing to do with the product.
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "gnosix-lab-catalog-{id}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create lab catalog root");
    for (name, body) in entries {
        std::fs::write(root.join(name), body).expect("write lab desktop entry");
    }
    ApplicationCatalog::discover(
        &gnosix_application_catalog::ApplicationCatalogRoots::from_application_dirs(vec![
            root.clone(),
        ]),
        &gnosix_application_catalog::LocalePreferences::neutral(),
        gnosix_application_catalog::DesktopEnvironment::from_names(["GNOSIX".to_owned()]),
    )
}

fn lab_entry(name: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec=/usr/bin/true\n")
}

/// An output whose runtime carries a synthetic installed-application catalog.
fn appstack_output(id: &str, entries: &[(&str, &str)]) -> ProductLabOutput {
    let ws = workspace(1, "1", 1);
    let semantic = current(
        "wl-1",
        1,
        vec![ws.clone()],
        vec![ws],
        Some(0),
        Vec::new(),
        None,
    );
    let mut runtime = runtime(
        semantic,
        preferences(ProductPreferenceProjectionState::AuthorityUnavailable),
        ProductActionState::Idle,
    );
    runtime.set_application_catalog(Arc::new(synthetic_catalog(id, entries)));
    ProductLabOutput {
        name: "wl-1".to_owned(),
        runtime,
    }
}

fn output(name: impl Into<String>, semantic: ProductSemanticState) -> ProductLabOutput {
    ProductLabOutput {
        name: name.into(),
        runtime: runtime(
            semantic,
            preferences(ProductPreferenceProjectionState::AuthorityUnavailable),
            ProductActionState::Idle,
        ),
    }
}

fn workspace_single() -> ProductLabFixture {
    let ws1 = workspace(1, "1", 1);
    ProductLabFixture {
        name: "workspace-single",
        description: "One active workspace at normal text scale.",
        design: design(1.0, false),
        outputs: vec![output(
            "HDMI-A-1",
            current(
                "HDMI-A-1",
                1,
                vec![ws1.clone()],
                vec![ws1],
                Some(0),
                vec![],
                None,
            ),
        )],
    }
}

fn workspace_hidden() -> ProductLabFixture {
    let ws1 = workspace(1, "1", 2);
    let ws2 = workspace(2, "2", 2);
    ProductLabFixture {
        name: "workspace-hidden",
        description: "Active workspace plus an inactive/hidden manager target.",
        design: design(1.0, false),
        outputs: vec![output(
            "HDMI-A-1",
            current(
                "HDMI-A-1",
                2,
                vec![ws1.clone(), ws2],
                vec![ws1],
                Some(0),
                vec![],
                None,
            ),
        )],
    }
}

fn workspace_many() -> ProductLabFixture {
    let targets = (1..=7)
        .map(|id| workspace(id, id.to_string(), 3))
        .collect::<Vec<_>>();
    ProductLabFixture {
        name: "workspace-many",
        description: "Seven workspace targets exercise density and list wrapping.",
        design: design(1.0, false),
        outputs: vec![output(
            "DP-2",
            current("DP-2", 3, targets.clone(), targets, Some(3), vec![], None),
        )],
    }
}

fn apps_empty() -> ProductLabFixture {
    let ws = workspace(1, "main", 4);
    ProductLabFixture {
        name: "apps-empty",
        description: "Valid current workspace with no observed applications.",
        design: design(1.0, false),
        outputs: vec![output(
            "HDMI-A-1",
            current(
                "HDMI-A-1",
                4,
                vec![ws.clone()],
                vec![ws],
                Some(0),
                vec![],
                None,
            ),
        )],
    }
}

fn apps_duplicates() -> ProductLabFixture {
    let ws = workspace(1, "dev", 5);
    let apps = vec![
        application(
            11,
            "Editor",
            "editor",
            &ws,
            5,
            SurfaceVisibility::Visible,
            SurfaceFocus::Focused,
        ),
        application(
            12,
            "Editor",
            "editor",
            &ws,
            5,
            SurfaceVisibility::Visible,
            SurfaceFocus::Unfocused,
        ),
        application(
            13,
            "Build Log",
            "terminal",
            &ws,
            5,
            SurfaceVisibility::Hidden,
            SurfaceFocus::Unfocused,
        ),
    ];
    ProductLabFixture {
        name: "apps-duplicates",
        description: "Duplicate titles/classes remain distinct by SurfaceId; includes hidden/unfocused app.",
        design: design(1.0, false),
        outputs: vec![output(
            "HDMI-A-1",
            current(
                "HDMI-A-1",
                5,
                vec![ws.clone()],
                vec![ws],
                Some(0),
                apps,
                Some(0),
            ),
        )],
    }
}

fn multi_output() -> ProductLabFixture {
    let left_ws = workspace(1, "1", 6);
    let right_ws = workspace(11, "11", 6);
    let left_app = application(
        21,
        "Browser",
        "browser",
        &left_ws,
        6,
        SurfaceVisibility::Visible,
        SurfaceFocus::Focused,
    );
    let right_app = application(
        22,
        "Terminal",
        "terminal",
        &right_ws,
        6,
        SurfaceVisibility::Visible,
        SurfaceFocus::Focused,
    );
    ProductLabFixture {
        name: "multi-output",
        description: "Two independent output-local ProductRuntime snapshots.",
        design: design(1.0, false),
        outputs: vec![
            output(
                "HDMI-A-1",
                current(
                    "HDMI-A-1",
                    6,
                    vec![left_ws.clone()],
                    vec![left_ws],
                    Some(0),
                    vec![left_app],
                    Some(0),
                ),
            ),
            output(
                "DP-2",
                current(
                    "DP-2",
                    6,
                    vec![right_ws.clone()],
                    vec![right_ws],
                    Some(0),
                    vec![right_app],
                    Some(0),
                ),
            ),
        ],
    }
}

fn semantic_unavailable() -> ProductLabFixture {
    ProductLabFixture {
        name: "semantic-unavailable",
        description: "No semantic projection is currently available.",
        design: design(1.0, false),
        outputs: vec![output("HDMI-A-1", ProductSemanticState::Unavailable)],
    }
}

fn semantic_invalid() -> ProductLabFixture {
    ProductLabFixture {
        name: "semantic-invalid",
        description: "Semantic projection exists but fails product validation.",
        design: design(1.0, false),
        outputs: vec![output("HDMI-A-1", ProductSemanticState::Invalid)],
    }
}

fn stale_selection() -> ProductLabFixture {
    let current_ws = workspace(1, "1", 8);
    let selected_ws = workspace(1, "1", 7);
    let action = ProductActionState::Terminal(ProductActionRecord {
        kind: ProductActionKind::ActivateWorkspace,
        target: ProductActionTarget::Workspace(selected_ws.reference),
        label: "1".to_owned(),
        selected_generation: DesktopContextGeneration(7),
        current_generation: Some(DesktopContextGeneration(8)),
        terminal: Some(ProductActionTerminal::LocalStale),
        detail: "selected generation 7 differs from current generation 8".to_owned(),
    });
    ProductLabFixture {
        name: "stale-selection",
        description: "Selected workspace reference is stale relative to runtime generation.",
        design: design(1.0, false),
        outputs: vec![ProductLabOutput {
            name: "HDMI-A-1".to_owned(),
            runtime: runtime(
                current(
                    "HDMI-A-1",
                    8,
                    vec![current_ws.clone()],
                    vec![current_ws],
                    Some(0),
                    vec![],
                    None,
                ),
                preferences(ProductPreferenceProjectionState::AuthorityUnavailable),
                action,
            ),
        }],
    }
}

fn profile_state(name: &'static str, state: ProductPreferenceProjectionState) -> ProductLabFixture {
    let ws = workspace(1, "1", 9);
    ProductLabFixture {
        name,
        description: match state {
            ProductPreferenceProjectionState::ProfileDocumentAbsent => {
                "Durable identity is available but profile document is absent."
            }
            ProductPreferenceProjectionState::NoDurableBinding => {
                "Current output has no durable R11 binding."
            }
            ProductPreferenceProjectionState::RecoveryRequired => {
                "Profile store requires recovery; no previous profile is treated as current."
            }
            _ => "Preference projection state fixture.",
        },
        design: design(1.0, false),
        outputs: vec![ProductLabOutput {
            name: "HDMI-A-1".to_owned(),
            runtime: runtime(
                current(
                    "HDMI-A-1",
                    9,
                    vec![ws.clone()],
                    vec![ws],
                    Some(0),
                    vec![],
                    None,
                ),
                preferences(state),
                ProductActionState::Idle,
            ),
        }],
    }
}

fn long_text_2x() -> ProductLabFixture {
    let ws = workspace(
        1,
        "workspace-de-développement-avec-un-nom-excessivement-long-pour-tester-la-troncature",
        10,
    );
    let app = application(
        31,
        "Une application avec un titre localisé volontairement très long — paramètres et diagnostics avancés",
        "org.example.application-extremement-longue",
        &ws,
        10,
        SurfaceVisibility::Visible,
        SurfaceFocus::Focused,
    );
    ProductLabFixture {
        name: "long-text-2x",
        description: "Long localized text rendered at 2× toolkit scale.",
        design: design(2.0, false),
        outputs: vec![output(
            "DISPLAYPORT-ULTRAWIDE-OUTPUT-NAME",
            current(
                "DISPLAYPORT-ULTRAWIDE-OUTPUT-NAME",
                10,
                vec![ws.clone()],
                vec![ws],
                Some(0),
                vec![app],
                Some(0),
            ),
        )],
    }
}

fn notification_long_2x() -> ProductLabFixture {
    let ws = workspace(1, "notifications", 13);
    ProductLabFixture {
        name: "notification-long-2x",
        description: "Long notification content at 2× must grow vertically; the parent lab scrolls instead of clipping the card.",
        design: design(2.0, false),
        outputs: vec![output(
            "HDMI-A-1",
            current(
                "HDMI-A-1",
                13,
                vec![ws.clone()],
                vec![ws],
                Some(0),
                vec![],
                None,
            ),
        )],
    }
}

fn reduced_motion() -> ProductLabFixture {
    let ws = workspace(1, "1", 11);
    ProductLabFixture {
        name: "reduced-motion",
        description: "Same component semantics with temporal toolkit durations collapsed to zero.",
        design: design(1.0, true),
        outputs: vec![output(
            "HDMI-A-1",
            current(
                "HDMI-A-1",
                11,
                vec![ws.clone()],
                vec![ws],
                Some(0),
                vec![],
                None,
            ),
        )],
    }
}

fn action_runtime(
    name: &str,
    terminal: Option<ProductActionTerminal>,
    pending: bool,
) -> ProductLabOutput {
    let ws = workspace(1, "1", 12);
    let record = ProductActionRecord {
        kind: ProductActionKind::ActivateWorkspace,
        target: ProductActionTarget::Workspace(ws.reference),
        label: "1".to_owned(),
        selected_generation: DesktopContextGeneration(12),
        current_generation: Some(DesktopContextGeneration(12)),
        terminal,
        detail: terminal.map_or_else(
            || "request pending".to_owned(),
            |value| value.as_str().to_owned(),
        ),
    };
    let action = if pending {
        ProductActionState::Pending(record)
    } else {
        ProductActionState::Terminal(record)
    };
    ProductLabOutput {
        name: name.to_owned(),
        runtime: runtime(
            current(name, 12, vec![ws.clone()], vec![ws], Some(0), vec![], None),
            preferences(ProductPreferenceProjectionState::AuthorityUnavailable),
            action,
        ),
    }
}

fn action_states() -> ProductLabFixture {
    ProductLabFixture {
        name: "action-states",
        description: "Pending, successful, refused, and backend-failed action lifecycle states.",
        design: design(1.0, false),
        outputs: vec![
            action_runtime("pending", None, true),
            action_runtime("applied", Some(ProductActionTerminal::Applied), false),
            action_runtime("stale", Some(ProductActionTerminal::LocalStale), false),
            action_runtime(
                "backend-failed",
                Some(ProductActionTerminal::BackendRejected),
                false,
            ),
        ],
    }
}

/// A AppStack with a representative set of installed applications.
fn appstack_basic() -> ProductLabFixture {
    ProductLabFixture {
        name: "appstack-basic",
        description: "AppStack with installed applications, in product order",
        design: design(1.0, false),
        outputs: vec![appstack_output(
            "basic",
            &[
                ("firefox.desktop", &lab_entry("Firefox")),
                ("thunderbird.desktop", &lab_entry("Thunderbird")),
                ("files.desktop", &lab_entry("Files")),
            ],
        )],
    }
}

/// Two applications sharing a visible title.
///
/// The case that makes identity rules observable: if a control were identified
/// by its label, these two rows would be indistinguishable to a keyboard user
/// and to a screen reader.
fn appstack_duplicate_names() -> ProductLabFixture {
    ProductLabFixture {
        name: "appstack-duplicate-names",
        description: "Two installed applications with the same visible name",
        design: design(1.0, false),
        outputs: vec![appstack_output(
            "duplicates",
            &[
                ("alpha-editor.desktop", &lab_entry("Editor")),
                ("beta-editor.desktop", &lab_entry("Editor")),
            ],
        )],
    }
}

/// A AppStack whose launch is refused.
///
/// The refusal presentation, so the "nothing happened" state is a state a human
/// can see before merge rather than one discovered in production.
fn appstack_refused() -> ProductLabFixture {
    let mut output = appstack_output(
        "refused",
        &[
            (
                "console.desktop",
                "[Desktop Entry]\nType=Application\nName=Console\nExec=/usr/bin/true\nTerminal=true\n",
            ),
            ("notes.desktop", &lab_entry("Notes")),
        ],
    );
    let selected = output
        .runtime
        .installed_applications()
        .into_iter()
        .find(|candidate| candidate.reference.entry().as_str() == "console.desktop")
        .expect("the synthetic terminal entry is installed");
    // Use the real capability path so the runtime captured by the lab already
    // carries the same terminal refusal production would present.
    crate::product_launch::launch_lab_application(&mut output.runtime, &selected);
    ProductLabFixture {
        name: "appstack-refused",
        description: "AppStack with a terminal entry the capability refuses",
        design: design(1.0, false),
        outputs: vec![output],
    }
}

/// A AppStack with nothing installed.
///
/// Distinct from a AppStack that failed to open: an empty surface that renders
/// nothing at all is indistinguishable from a broken one.
fn appstack_empty() -> ProductLabFixture {
    ProductLabFixture {
        name: "appstack-empty",
        description: "AppStack with no presentable installed applications",
        design: design(1.0, false),
        outputs: vec![appstack_output("empty", &[])],
    }
}

/// A long application title at 2x text scale.
///
/// The layout case most likely to clip, which is why it is a fixture rather than
/// something noticed later.
fn appstack_long_title_2x() -> ProductLabFixture {
    ProductLabFixture {
        name: "appstack-long-title-2x",
        description: "AppStack with a long application title at 2x text scale",
        design: design(2.0, false),
        outputs: vec![appstack_output(
            "long-title",
            &[(
                "long.desktop",
                &lab_entry("Gestionnaire de périphériques réseau avancés et_material"),
            )],
        )],
    }
}

/// The AppStack with a single application, so the derived hull, its joint and
/// its shoulders are fully visible.
///
/// The other AppStack fixtures are dense enough to cover content: 119 rows, long
/// titles, duplicate names, a refusal. A seam or a broken shoulder can hide
/// completely behind a wall of rows, and a fixture that only ever renders dense
/// content cannot catch one. This one is deliberately near-empty for that
/// reason, and it is a real derived body like the others - only its content is
/// small.
fn appstack_joint() -> ProductLabFixture {
    ProductLabFixture {
        name: "appstack-joint",
        description: "Derived AppStack with one row, exposing the joint and shoulders",
        design: design(1.0, false),
        outputs: vec![appstack_output(
            "joint",
            &[("only.desktop", &lab_entry("Notes"))],
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_diagnostics::ProductDiagnosticSnapshot;

    #[test]
    fn the_appstack_fixtures_offer_what_they_claim_to() {
        // A visual fixture that renders the wrong content is worse than no
        // fixture, because it is reviewed and then trusted.
        let basic = fixture("appstack-basic").expect("basic");
        let rows = crate::product_appstack::appstack_rows(&basic.outputs[0].runtime);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Files", "Firefox", "Thunderbird"],
            "the basic fixture must render its entries in product order"
        );

        let duplicates = fixture("appstack-duplicate-names").expect("duplicates");
        let rows = crate::product_appstack::appstack_rows(&duplicates.outputs[0].runtime);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.name == "Editor"));
        assert_ne!(rows[0].entry, rows[1].entry);

        let empty = fixture("appstack-empty").expect("empty");
        assert!(
            crate::product_appstack::appstack_rows(&empty.outputs[0].runtime).is_empty(),
            "the empty fixture must offer nothing"
        );

        let long = fixture("appstack-long-title-2x").expect("long");
        let rows = crate::product_appstack::appstack_rows(&long.outputs[0].runtime);
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].name.chars().count() > 40,
            "the long title must be long"
        );
    }

    #[test]
    fn the_refused_fixture_really_is_refused() {
        // The captured fixture runtime itself must already contain the refusal.
        let fixture = fixture("appstack-refused").expect("refused");
        let runtime = &fixture.outputs[0].runtime;
        let crate::product_runtime::ProductLaunchState::Terminal(record) = runtime.launch_state()
        else {
            panic!("fixture runtime must already be terminal");
        };
        assert_eq!(
            record.terminal.as_ref().map(|t| t.code()),
            Some("TERMINAL_REQUIRED"),
            "the refused fixture must actually refuse"
        );
        assert!(
            !record
                .terminal
                .as_ref()
                .is_some_and(|outcome| outcome.is_launched())
        );
        let feedback = crate::product_appstack::launch_feedback(runtime.launch_state())
            .expect("refusal feedback is product-visible");
        assert!(!feedback.launched);
        assert_eq!(feedback.code, "TERMINAL_REQUIRED");
        assert!(feedback.message.contains("lancement refusé"));
        assert!(feedback.message.contains("TERMINAL_REQUIRED"));
        assert!(feedback.message.contains("Console"));
        assert!(matches!(
            record.terminal,
            Some(crate::product_runtime::ProductLaunchOutcome::Refused { .. })
        ));
        let _presentation = crate::product_appstack_view::feedback(&feedback, fixture.design);
    }

    #[test]
    fn ordinary_appstack_fixtures_have_no_stale_refusal_feedback() {
        for name in [
            "appstack-basic",
            "appstack-duplicate-names",
            "appstack-empty",
            "appstack-joint",
            "appstack-long-title-2x",
        ] {
            let fixture = fixture(name).expect("ordinary AppStack fixture");
            assert!(
                crate::product_appstack::launch_feedback(fixture.outputs[0].runtime.launch_state())
                    .is_none(),
                "{name} must not inherit stale launch feedback"
            );
        }
    }

    #[test]
    fn the_lab_catalog_is_synthetic_rather_than_discovered() {
        // If a fixture discovered the machine's real applications, a visual
        // review would be reviewing the reviewer's own desktop.
        let fixture = fixture("appstack-basic").expect("basic");
        let rows = crate::product_appstack::appstack_rows(&fixture.outputs[0].runtime);
        for row in &rows {
            assert!(
                ["firefox.desktop", "thunderbird.desktop", "files.desktop"]
                    .contains(&row.entry.as_str()),
                "unexpected entry {} in a synthetic fixture",
                row.entry.as_str()
            );
        }
    }

    #[test]
    fn the_joint_fixture_stays_sparse_enough_to_expose_the_hull() {
        // A seam hidden behind a wall of rows is a seam in production too. If
        // this fixture ever grows dense, it stops being able to catch one.
        let fixture = fixture("appstack-joint").expect("joint");
        let rows = crate::product_appstack::appstack_rows(&fixture.outputs[0].runtime);
        assert!(
            rows.len() <= 1,
            "the joint fixture must stay near-empty, found {} rows",
            rows.len()
        );
    }

    #[test]
    fn the_dense_appstack_fixtures_still_carry_real_content() {
        // The complement of the joint fixture: the content fixtures have to
        // keep enough rows to exercise scrolling, duplicates and long names.
        for name in ["appstack-basic", "appstack-duplicate-names"] {
            let fixture = fixture(name).expect(name);
            let rows = crate::product_appstack::appstack_rows(&fixture.outputs[0].runtime);
            assert!(!rows.is_empty(), "{name} must offer applications");
        }
        let duplicates = fixture("appstack-duplicate-names").expect("duplicates");
        let rows = crate::product_appstack::appstack_rows(&duplicates.outputs[0].runtime);
        assert_eq!(rows.len(), 2, "the duplicate fixture must keep both rows");
        assert!(rows.iter().all(|row| row.name == "Editor"));
    }

    #[test]
    fn all_declared_fixture_names_resolve() {
        for name in FIXTURE_NAMES {
            let fixture = fixture(name).unwrap_or_else(|| panic!("fixture {name} missing"));
            assert_eq!(fixture.name, *name);
            assert!(!fixture.outputs.is_empty(), "{name}");
        }
    }

    #[test]
    fn duplicate_application_fixture_preserves_distinct_exact_surface_ids() {
        let fixture = fixture("apps-duplicates").unwrap();
        let runtime = &fixture.outputs[0].runtime;
        let apps = runtime.application_targets();
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0].title, apps[1].title);
        assert_eq!(apps[0].class, apps[1].class);
        assert_ne!(apps[0].reference, apps[1].reference);
    }

    #[test]
    fn stale_fixture_exposes_selected_and_current_generation_mismatch() {
        let fixture = fixture("stale-selection").unwrap();
        let snapshot = ProductDiagnosticSnapshot::from_runtime(&fixture.outputs[0].runtime);
        assert!(snapshot.action_summary.contains("LocalStale"));
        assert!(snapshot.action_summary.contains("selected_gen=7"));
        assert!(snapshot.action_summary.contains("current_gen=8"));
    }

    #[test]
    fn multi_output_fixture_uses_distinct_runtime_snapshots() {
        let fixture = fixture("multi-output").unwrap();
        assert_eq!(fixture.outputs.len(), 2);
        let connectors = fixture
            .outputs
            .iter()
            .map(|output| {
                ProductDiagnosticSnapshot::from_runtime(&output.runtime)
                    .connector
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(connectors, vec!["HDMI-A-1", "DP-2"]);
    }

    #[test]
    fn long_text_and_reduced_motion_are_fixture_level_design_inputs() {
        assert_eq!(fixture("long-text-2x").unwrap().design.text_scale(), 2.0);
        assert_eq!(
            fixture("notification-long-2x").unwrap().design.text_scale(),
            2.0
        );
        assert!(fixture("reduced-motion").unwrap().design.reduced_motion());
    }
}
