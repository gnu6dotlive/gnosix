//! Deterministic ProductRuntime fixtures for the GNOS.UX component lab.
//!
//! Fixtures never observe or mutate the live desktop. They construct the same
//! product models consumed by production views.

use std::sync::Arc;

use crate::{
    product_audio_osd::{AudioOsdPlacement, AudioOsdPresentationState},
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
    product_tray::{
        TrayCategory, TrayIcon, TrayItemId, TrayItemSnapshot, TraySnapshot, TrayStatus, TrayTooltip,
    },
    product_tray_menu::TrayMenuSnapshot,
};
use gnos_network::{NetworkConnectivity, NetworkDeviceState, NetworkProviderState};
use gnos_ux_product_intent::{ObservedSurfaceReference, ObservedWorkspaceReference};
use gnosix_application_catalog::ApplicationCatalog;
use gnosix_desktop_model::{
    DesktopContextGeneration, SurfaceFocus, SurfaceId, SurfaceVisibility, WorkspaceId,
};
use std::time::Duration;

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
    "tray-empty",
    "tray-one-item",
    "tray-many-items",
    "tray-attention",
    "tray-long-tooltip",
    "tray-icon-missing",
    "tray-provider-unavailable",
    "tray-compact",
    "tray-expanded",
    "tray-item-activatable",
    "tray-item-menu-only",
    "tray-menu-basic",
    "tray-menu-nested",
    "tray-menu-disabled",
    "tray-menu-toggle",
    "tray-menu-radio",
    "tray-menu-long-label",
    "tray-menu-layout-updated",
    "tray-item-disappeared",
    "tray-item-owner-replaced",
    "tray-menu-stale-revision",
    "mpris-empty",
    "mpris-playing",
    "mpris-paused",
    "mpris-missing-metadata",
    "mpris-long-title",
    "mpris-no-artwork",
    "mpris-controls-disabled",
    "mpris-multiple-players",
    "mpris-player-disappeared",
    "mpris-owner-replaced",
    "mpris-track-changed",
    "audio-normal",
    "audio-muted",
    "audio-low",
    "audio-high",
    "audio-unavailable",
    "audio-default-unresolved",
    "audio-osd-volume-0",
    "audio-osd-volume-mid",
    "audio-osd-volume-100",
    "audio-osd-muted",
    "audio-osd-unmuted",
    "audio-osd-long-scale-2x",
    "audio-osd-reduced-motion",
    "audio-osd-hidden",
    "audio-osd-single-owner",
    "network-provider-unavailable",
    "network-disconnected",
    "network-connecting",
    "network-wifi-weak",
    "network-wifi-medium",
    "network-wifi-strong",
    "network-wifi-full-connectivity",
    "network-wifi-limited",
    "network-wifi-portal",
    "network-wired",
    "network-overlay-active",
    "network-many-irrelevant-devices",
];

pub struct ProductLabOutput {
    pub name: String,
    pub runtime: ProductRuntime,
    /// Transient observed-audio OSD state for this output.
    ///
    /// Only the output that owns the resolved placement carries one; the others
    /// are `None`. That makes single-output ownership visible in the fixture
    /// itself rather than only in a receipt.
    pub audio_osd: Option<AudioOsdPresentationState>,
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
        "tray-empty" => Some(tray_empty()),
        "tray-one-item" => Some(tray_one_item()),
        "tray-many-items" => Some(tray_many_items()),
        "tray-attention" => Some(tray_attention()),
        "tray-long-tooltip" => Some(tray_long_tooltip()),
        "tray-icon-missing" => Some(tray_icon_missing()),
        "tray-provider-unavailable" => Some(tray_provider_unavailable()),
        "tray-compact" => Some(tray_compact()),
        "tray-expanded" => Some(tray_expanded()),
        "tray-item-activatable" => Some(tray_item_activatable()),
        "tray-item-menu-only" => Some(tray_item_menu_only()),
        "tray-menu-basic" => Some(tray_menu_basic()),
        "tray-menu-nested" => Some(tray_menu_nested()),
        "tray-menu-disabled" => Some(tray_menu_disabled()),
        "tray-menu-toggle" => Some(tray_menu_toggle()),
        "tray-menu-radio" => Some(tray_menu_radio()),
        "tray-menu-long-label" => Some(tray_menu_long_label()),
        "tray-menu-layout-updated" => Some(tray_menu_layout_updated()),
        "tray-item-disappeared" => Some(tray_item_disappeared()),
        "tray-item-owner-replaced" => Some(tray_item_owner_replaced()),
        "tray-menu-stale-revision" => Some(tray_menu_stale_revision()),
        "mpris-empty" => Some(mpris_empty()),
        "mpris-playing" => Some(mpris_playing()),
        "mpris-paused" => Some(mpris_paused()),
        "mpris-missing-metadata" => Some(mpris_missing_metadata()),
        "mpris-long-title" => Some(mpris_long_title()),
        "mpris-no-artwork" => Some(mpris_no_artwork()),
        "mpris-controls-disabled" => Some(mpris_controls_disabled()),
        "mpris-multiple-players" => Some(mpris_multiple_players()),
        "mpris-player-disappeared" => Some(mpris_player_disappeared()),
        "mpris-owner-replaced" => Some(mpris_owner_replaced()),
        "mpris-track-changed" => Some(mpris_track_changed()),
        "audio-normal" => Some(audio_fixture("audio-normal", 0.90, false, true)),
        "audio-muted" => Some(audio_fixture("audio-muted", 0.90, true, true)),
        "audio-low" => Some(audio_fixture("audio-low", 0.20, false, true)),
        "audio-high" => Some(audio_fixture("audio-high", 1.20, false, true)),
        "audio-unavailable" => Some(audio_unavailable()),
        "audio-default-unresolved" => Some(audio_fixture(
            "audio-default-unresolved",
            0.90,
            false,
            false,
        )),
        "audio-osd-volume-0" => Some(audio_osd_fixture(
            "audio-osd-volume-0",
            "Observed OSD at silence",
            0.0,
            false,
            1.0,
            false,
        )),
        "audio-osd-volume-mid" => Some(audio_osd_fixture(
            "audio-osd-volume-mid",
            "Observed OSD at mid level",
            0.45,
            false,
            1.0,
            false,
        )),
        "audio-osd-volume-100" => Some(audio_osd_fixture(
            "audio-osd-volume-100",
            "Observed OSD at full level",
            1.0,
            false,
            1.0,
            false,
        )),
        "audio-osd-muted" => Some(audio_osd_fixture(
            "audio-osd-muted",
            "Observed OSD when muted",
            0.45,
            true,
            1.0,
            false,
        )),
        "audio-osd-unmuted" => Some(audio_osd_fixture(
            "audio-osd-unmuted",
            "Observed OSD when unmuted, reporting the current observed level",
            0.45,
            false,
            1.0,
            false,
        )),
        "audio-osd-long-scale-2x" => Some(audio_osd_fixture(
            "audio-osd-long-scale-2x",
            "Observed OSD at 2x text scale",
            0.45,
            false,
            2.0,
            false,
        )),
        "audio-osd-reduced-motion" => Some(audio_osd_fixture(
            "audio-osd-reduced-motion",
            "Observed OSD under reduced motion",
            0.45,
            false,
            1.0,
            true,
        )),
        "audio-osd-hidden" => Some(audio_osd_hidden_fixture()),
        "audio-osd-single-owner" => Some(audio_osd_single_owner()),
        "network-provider-unavailable" => Some(network_fixture(
            "network-provider-unavailable",
            "Provider absent: explicitly not a disconnected network",
            network_snapshot(NetworkProviderState::Unavailable, None, Vec::new()),
        )),
        "network-disconnected" => Some(network_fixture(
            "network-disconnected",
            "Provider answering with no usable primary transport",
            network_snapshot(
                NetworkProviderState::Available,
                Some(network_globals(
                    NetworkDeviceState::Disconnected,
                    NetworkConnectivity::None,
                )),
                vec![network_ethernet(NetworkDeviceState::Disconnected, false)],
            ),
        )),
        "network-connecting" => Some(network_fixture(
            "network-connecting",
            "A transport is coming up",
            network_snapshot(
                NetworkProviderState::Available,
                Some(network_globals(
                    NetworkDeviceState::Connecting,
                    NetworkConnectivity::Limited,
                )),
                vec![network_wifi(NetworkDeviceState::Connecting, 40)],
            ),
        )),
        "network-wifi-weak" => Some(network_wifi_fixture("network-wifi-weak", 20)),
        "network-wifi-medium" => Some(network_wifi_fixture("network-wifi-medium", 60)),
        "network-wifi-strong" => Some(network_wifi_fixture("network-wifi-strong", 90)),
        "network-wifi-full-connectivity" => Some(network_fixture(
            "network-wifi-full-connectivity",
            "Wi-Fi up; the provider's assessment is not a verified internet check",
            network_snapshot(
                NetworkProviderState::Available,
                Some(network_globals(
                    NetworkDeviceState::ConnectedGlobal,
                    NetworkConnectivity::Full,
                )),
                vec![network_wifi(NetworkDeviceState::Connected, 75)],
            ),
        )),
        "network-wifi-limited" => Some(network_fixture(
            "network-wifi-limited",
            "Wi-Fi up with limited access",
            network_snapshot(
                NetworkProviderState::Available,
                Some(network_globals(
                    NetworkDeviceState::ConnectedGlobal,
                    NetworkConnectivity::Limited,
                )),
                vec![network_wifi(NetworkDeviceState::Connected, 60)],
            ),
        )),
        "network-wifi-portal" => Some(network_fixture(
            "network-wifi-portal",
            "Wi-Fi up behind a captive portal",
            network_snapshot(
                NetworkProviderState::Available,
                Some(network_globals(
                    NetworkDeviceState::ConnectedGlobal,
                    NetworkConnectivity::Portal,
                )),
                vec![network_wifi(NetworkDeviceState::Connected, 60)],
            ),
        )),
        "network-wired" => Some(network_fixture(
            "network-wired",
            "Wired primary transport",
            network_snapshot(
                NetworkProviderState::Available,
                Some(network_globals(
                    NetworkDeviceState::ConnectedGlobal,
                    NetworkConnectivity::Full,
                )),
                vec![network_ethernet(NetworkDeviceState::Connected, true)],
            ),
        )),
        "network-overlay-active" => Some(network_fixture(
            "network-overlay-active",
            "Wi-Fi primary with an active overlay, reported separately",
            network_snapshot(
                NetworkProviderState::Available,
                Some(network_globals(
                    NetworkDeviceState::ConnectedGlobal,
                    NetworkConnectivity::Full,
                )),
                vec![
                    network_wifi(NetworkDeviceState::Connected, 70),
                    network_overlay(NetworkDeviceState::Connected),
                ],
            ),
        )),
        "network-many-irrelevant-devices" => Some(network_many_irrelevant()),
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
        bounds: None,
        reference: ObservedSurfaceReference::new(
            SurfaceId::from_bytes([id; 16]),
            DesktopContextGeneration(generation),
        ),
        workspace: Some(workspace.reference),
        title: title.into(),
        initial_class: class.clone(),
        tags: Vec::new(),
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
        output_area: None,
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
        audio_osd: None,
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
        audio_osd: None,
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
            audio_osd: None,
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
            audio_osd: None,
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
        audio_osd: None,
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

fn tray_output(name: &str, tray: TraySnapshot, compact_override: Option<bool>) -> ProductLabOutput {
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
    let mut prefs = preferences(ProductPreferenceProjectionState::AuthorityUnavailable);
    if let Some(compact) = compact_override {
        prefs.bar_overrides.tray_compact = Some(compact);
    }
    let mut runtime = runtime(semantic, prefs, ProductActionState::Idle);
    runtime.set_tray(tray);
    ProductLabOutput {
        name: name.to_owned(),
        runtime,
        audio_osd: None,
    }
}

fn mock_tray_item(
    id: &str,
    title: &str,
    icon: TrayIcon,
    status: TrayStatus,
    tooltip: Option<TrayTooltip>,
) -> TrayItemSnapshot {
    TrayItemSnapshot {
        id: TrayItemId::new(id),
        service: format!("org.kde.StatusNotifierItem-{id}"),
        path: format!("/StatusNotifierItem/{id}"),
        title: title.to_owned(),
        status,
        category: TrayCategory::ApplicationStatus,
        icons: icon.into(),
        tooltip,
        item_is_menu: false,
        menu_path: None,
        owner: format!(":1.donor15-{id}"),
    }
}

fn tray_empty() -> ProductLabFixture {
    ProductLabFixture {
        name: "tray-empty",
        description: "Tray provider available with zero registered items",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "empty",
            TraySnapshot::Active { items: vec![] },
            None,
        )],
    }
}

fn tray_one_item() -> ProductLabFixture {
    let item = mock_tray_item(
        "bluetooth",
        "Bluetooth",
        TrayIcon::Named("bluetooth-active".to_owned()),
        TrayStatus::Active,
        Some(TrayTooltip {
            title: "Bluetooth".to_owned(),
            description: "1 device connected".to_owned(),
        }),
    );
    ProductLabFixture {
        name: "tray-one-item",
        description: "Tray with a single active item",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "one-item",
            TraySnapshot::Active { items: vec![item] },
            None,
        )],
    }
}

fn tray_many_items() -> ProductLabFixture {
    let items = vec![
        mock_tray_item(
            "audio",
            "Volume",
            TrayIcon::Named("audio-volume-high".to_owned()),
            TrayStatus::Active,
            Some(TrayTooltip {
                title: "Volume".to_owned(),
                description: "80%".to_owned(),
            }),
        ),
        mock_tray_item(
            "network",
            "Network",
            TrayIcon::Named("network-wireless-connected".to_owned()),
            TrayStatus::Active,
            Some(TrayTooltip {
                title: "Wi-Fi".to_owned(),
                description: "Connected to Gnosix-Lab".to_owned(),
            }),
        ),
        mock_tray_item(
            "battery",
            "Power",
            TrayIcon::Named("battery-good".to_owned()),
            TrayStatus::Active,
            Some(TrayTooltip {
                title: "Battery".to_owned(),
                description: "95% (discharging)".to_owned(),
            }),
        ),
        mock_tray_item(
            "steam",
            "Steam",
            TrayIcon::Named("steam".to_owned()),
            TrayStatus::Passive,
            None,
        ),
    ];
    ProductLabFixture {
        name: "tray-many-items",
        description: "Tray with multiple active and passive items",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "many-items",
            TraySnapshot::Active { items },
            None,
        )],
    }
}

fn tray_attention() -> ProductLabFixture {
    let item = mock_tray_item(
        "updates",
        "System Updates",
        TrayIcon::Named("bell".to_owned()),
        TrayStatus::NeedsAttention,
        Some(TrayTooltip {
            title: "System Update".to_owned(),
            description: "Security updates pending installation".to_owned(),
        }),
    );
    ProductLabFixture {
        name: "tray-attention",
        description: "Tray item in NeedsAttention status with warning tone",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "attention",
            TraySnapshot::Active { items: vec![item] },
            None,
        )],
    }
}

fn tray_long_tooltip() -> ProductLabFixture {
    let item = mock_tray_item(
        "sync",
        "Cloud Storage Synchronizer",
        TrayIcon::Named("network".to_owned()),
        TrayStatus::Active,
        Some(TrayTooltip {
            title: "Synchronisation avancée en arrière-plan".to_owned(),
            description: "Synchronisation en cours de 14 382 fichiers restants (42.8 Gio / 120.0 Gio) — Débit estimé 12.4 Mio/s avec latence réseau stable".to_owned(),
        }),
    );
    ProductLabFixture {
        name: "tray-long-tooltip",
        description: "Tray item with long title and tooltip at 2x text scale",
        design: design(2.0, false),
        outputs: vec![tray_output(
            "long-tooltip",
            TraySnapshot::Active { items: vec![item] },
            None,
        )],
    }
}

fn tray_icon_missing() -> ProductLabFixture {
    let item = mock_tray_item(
        "unknown-app",
        "Legacy Application",
        TrayIcon::None,
        TrayStatus::Active,
        Some(TrayTooltip {
            title: "Legacy Tool".to_owned(),
            description: "Running with fallback icon".to_owned(),
        }),
    );
    ProductLabFixture {
        name: "tray-icon-missing",
        description: "Tray item with missing icon using fallback glyph",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "icon-missing",
            TraySnapshot::Active { items: vec![item] },
            None,
        )],
    }
}

fn tray_provider_unavailable() -> ProductLabFixture {
    ProductLabFixture {
        name: "tray-provider-unavailable",
        description: "Tray provider unavailable / disconnected",
        design: design(1.0, false),
        outputs: vec![tray_output("unavailable", TraySnapshot::Unavailable, None)],
    }
}

fn tray_compact() -> ProductLabFixture {
    let items = vec![
        mock_tray_item(
            "audio",
            "Audio",
            TrayIcon::Named("audio".to_owned()),
            TrayStatus::Active,
            None,
        ),
        mock_tray_item(
            "wifi",
            "Wi-Fi",
            TrayIcon::Named("wifi".to_owned()),
            TrayStatus::Active,
            None,
        ),
    ];
    ProductLabFixture {
        name: "tray-compact",
        description: "Tray rendered in compact mode (icons only, tighter spacing)",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "compact",
            TraySnapshot::Active { items },
            Some(true),
        )],
    }
}

fn tray_expanded() -> ProductLabFixture {
    let items = vec![
        mock_tray_item(
            "audio",
            "Audio",
            TrayIcon::Named("audio".to_owned()),
            TrayStatus::Active,
            None,
        ),
        mock_tray_item(
            "wifi",
            "Wi-Fi",
            TrayIcon::Named("wifi".to_owned()),
            TrayStatus::Active,
            None,
        ),
    ];
    ProductLabFixture {
        name: "tray-expanded",
        description: "Tray rendered in expanded mode (labels visible, standard spacing) at 2x scale",
        design: design(2.0, false),
        outputs: vec![tray_output(
            "expanded",
            TraySnapshot::Active { items },
            Some(false),
        )],
    }
}

// ---------------------------------------------------------------------------
// DONOR-15 interaction fixtures.
//
// These simulate receipts. They do not constitute live protocol
// qualification; that evidence lives in the DONOR-15 run receipt.
// ---------------------------------------------------------------------------

/// Menu item factories used by the interaction fixtures.
fn lab_menu_entry(
    id: i32,
    label: Option<&str>,
    enabled: bool,
) -> crate::product_tray_menu::TrayMenuItem {
    crate::product_tray_menu::TrayMenuItem {
        id,
        label: label.map(str::to_owned),
        enabled,
        visible: true,
        icon: crate::product_tray_menu::TrayMenuIcon::None,
        item_type: crate::product_tray_menu::TrayMenuItemType::Standard,
        toggle_type: crate::product_tray_menu::TrayMenuToggleType::NotCheckable,
        toggle_state: crate::product_tray_menu::TrayMenuToggleState::Unchecked,
        children_display: crate::product_tray_menu::TrayMenuChildrenDisplay::Submenu,
        children: Vec::new(),
    }
}

fn lab_menu(
    revision: u32,
    children: Vec<crate::product_tray_menu::TrayMenuItem>,
) -> TrayMenuSnapshot {
    TrayMenuSnapshot {
        revision,
        status: crate::product_tray_menu::TrayMenuStatus::Normal,
        version: 3,
        text_direction: crate::product_tray_menu::TrayMenuTextDirection::LeftToRight,
        icon_theme_path: None,
        root: crate::product_tray_menu::TrayMenuItem {
            id: 0,
            children,
            ..crate::product_tray_menu::TrayMenuItem::default()
        },
    }
}

/// An item whose primary interaction activates the exporter.
fn tray_item_activatable() -> ProductLabFixture {
    let item = TrayItemSnapshot {
        item_is_menu: false,
        menu_path: None,
        id: TrayItemId::new("activatable"),
        title: "Activatable".to_owned(),
        icons: crate::product_tray::TrayIconFacts::named("audio-volume-high"),
        ..mock_tray_item(
            "activatable",
            "Activatable",
            TrayIcon::Named("audio-volume-high".to_owned()),
            TrayStatus::Active,
            None,
        )
    };
    ProductLabFixture {
        name: "tray-item-activatable",
        description: "Item whose primary interaction is Activate",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "activatable",
            TraySnapshot::Active { items: vec![item] },
            None,
        )],
    }
}

/// An item that declares itself a menu.
fn tray_item_menu_only() -> ProductLabFixture {
    let item = TrayItemSnapshot {
        item_is_menu: true,
        menu_path: Some("/MenuBar".to_owned()),
        id: TrayItemId::new("menuonly"),
        title: "Menu Only".to_owned(),
        icons: crate::product_tray::TrayIconFacts::named("audio-volume-high"),
        ..mock_tray_item(
            "menuonly",
            "Menu Only",
            TrayIcon::Named("audio-volume-high".to_owned()),
            TrayStatus::Active,
            None,
        )
    };
    ProductLabFixture {
        name: "tray-item-menu-only",
        description: "ItemIsMenu item whose primary interaction opens the menu",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "menu-only",
            TraySnapshot::Active { items: vec![item] },
            None,
        )],
    }
}

fn tray_menu_fixture(
    name: &'static str,
    description: &'static str,
    menu: TrayMenuSnapshot,
) -> ProductLabFixture {
    let item = TrayItemSnapshot {
        item_is_menu: true,
        menu_path: Some("/MenuBar".to_owned()),
        id: TrayItemId::new("menuowner"),
        title: "Menu Owner".to_owned(),
        icons: crate::product_tray::TrayIconFacts::named("audio-volume-high"),
        ..mock_tray_item(
            "menuowner",
            "Menu Owner",
            TrayIcon::Named("audio-volume-high".to_owned()),
            TrayStatus::Active,
            None,
        )
    };
    let mut output = tray_output("menu", TraySnapshot::Active { items: vec![item] }, None);
    output.runtime.set_menu(menu);
    ProductLabFixture {
        name,
        description,
        design: design(1.0, false),
        outputs: vec![output],
    }
}

fn tray_menu_basic() -> ProductLabFixture {
    tray_menu_fixture(
        "tray-menu-basic",
        "Projected menu with plain actionable rows",
        lab_menu(
            10,
            vec![
                lab_menu_entry(1, Some("Open"), true),
                lab_menu_entry(2, Some("Preferences"), true),
            ],
        ),
    )
}

fn tray_menu_nested() -> ProductLabFixture {
    let mut submenu = lab_menu_entry(5, Some("Store"), true);
    submenu.children = vec![
        lab_menu_entry(6, Some("Wishlists"), true),
        lab_menu_entry(7, Some("Cart"), true),
    ];
    tray_menu_fixture(
        "tray-menu-nested",
        "Projected menu with a submenu branch",
        lab_menu(11, vec![lab_menu_entry(1, Some("Top"), true), submenu]),
    )
}

fn tray_menu_disabled() -> ProductLabFixture {
    tray_menu_fixture(
        "tray-menu-disabled",
        "Projected menu carrying a disabled row",
        lab_menu(
            12,
            vec![
                lab_menu_entry(1, Some("Enabled"), true),
                lab_menu_entry(2, Some("Disabled"), false),
            ],
        ),
    )
}

fn tray_menu_toggle() -> ProductLabFixture {
    let mut toggled = lab_menu_entry(1, Some("Mute"), true);
    toggled.toggle_type = crate::product_tray_menu::TrayMenuToggleType::Checkable;
    toggled.toggle_state = crate::product_tray_menu::TrayMenuToggleState::Checked;
    tray_menu_fixture(
        "tray-menu-toggle",
        "Projected menu with a checked toggle row",
        lab_menu(13, vec![toggled]),
    )
}

fn tray_menu_radio() -> ProductLabFixture {
    let mut selected = lab_menu_entry(1, Some("Auto"), true);
    selected.toggle_type = crate::product_tray_menu::TrayMenuToggleType::Radio;
    selected.toggle_state = crate::product_tray_menu::TrayMenuToggleState::Checked;
    let mut other = lab_menu_entry(2, Some("Manual"), true);
    other.toggle_type = crate::product_tray_menu::TrayMenuToggleType::Radio;
    other.toggle_state = crate::product_tray_menu::TrayMenuToggleState::Unchecked;
    tray_menu_fixture(
        "tray-menu-radio",
        "Projected menu with a radio group",
        lab_menu(14, vec![selected, other]),
    )
}

fn tray_menu_long_label() -> ProductLabFixture {
    tray_menu_fixture(
        "tray-menu-long-label",
        "Projected menu with an untranslated long label at 2x scale",
        lab_menu(
            15,
            vec![lab_menu_entry(
                1,
                Some("Synchronisation avancée en arrière-plan — 14 382 fichiers restants"),
                true,
            )],
        ),
    )
}

fn tray_menu_layout_updated() -> ProductLabFixture {
    // The same layout observed at two revisions: the second is what a
    // LayoutUpdated produces, and it invalidates any action bound to the first.
    let mut fixture = tray_menu_fixture(
        "tray-menu-layout-updated",
        "Menu observed across a layout revision change",
        lab_menu(16, vec![lab_menu_entry(1, Some("Before"), true)]),
    );
    let after = lab_menu(17, vec![lab_menu_entry(1, Some("After"), true)]);
    fixture.outputs.push(tray_output(
        "menu-after-update",
        TraySnapshot::Active { items: Vec::new() },
        None,
    ));
    fixture.outputs[1].runtime.set_menu(after);
    fixture.outputs[0]
        .runtime
        .set_menu(lab_menu(16, vec![lab_menu_entry(1, Some("Before"), true)]));
    fixture
}

fn tray_item_disappeared() -> ProductLabFixture {
    // The rendered item is gone from the tray, so a click on it must be refused.
    ProductLabFixture {
        name: "tray-item-disappeared",
        description: "Rendered item no longer a member of the tray",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "disappeared",
            TraySnapshot::Active { items: Vec::new() },
            None,
        )],
    }
}

fn tray_item_owner_replaced() -> ProductLabFixture {
    // Same identity, different unique-name owner: the old icon must not act.
    let item = TrayItemSnapshot {
        owner: ":1.replaced".to_owned(),
        ..mock_tray_item(
            "recycled",
            "Recycled",
            TrayIcon::Named("audio-volume-high".to_owned()),
            TrayStatus::Active,
            None,
        )
    };
    ProductLabFixture {
        name: "tray-item-owner-replaced",
        description: "Bus name re-requested by a different owner",
        design: design(1.0, false),
        outputs: vec![tray_output(
            "owner-replaced",
            TraySnapshot::Active { items: vec![item] },
            None,
        )],
    }
}

fn tray_menu_stale_revision() -> ProductLabFixture {
    let mut fixture = tray_menu_fixture(
        "tray-menu-stale-revision",
        "Action bound to a revision the menu has since left",
        lab_menu(18, vec![lab_menu_entry(1, Some("Quit"), true)]),
    );
    // Present the current revision as 19, so a binding made against 18 is stale.
    fixture.outputs[0]
        .runtime
        .set_menu(lab_menu(19, vec![lab_menu_entry(1, Some("Restart"), true)]));
    fixture
}

// ---------------------------------------------------------------------------
// Native audio fixtures.
//
// These carry typed PipeWire facts into the same ProductRuntime consumed by the
// production Bar. They prove presentation semantics only; live causality is
// established by the AUDIO-01 qualification gates.
// ---------------------------------------------------------------------------

fn audio_output(
    name: &str,
    user_level: f32,
    muted: bool,
    resolve_default: bool,
) -> ProductLabOutput {
    audio_output_with_osd(name, user_level, muted, resolve_default, None)
}

/// Builds an audio output whose observed state can be paired with a transient
/// OSD presentation state.
fn audio_output_with_osd(
    name: &str,
    user_level: f32,
    muted: bool,
    resolve_default: bool,
    audio_osd: Option<AudioOsdPresentationState>,
) -> ProductLabOutput {
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
    let mut state = gnos_audio::AudioState::new(7);
    let endpoint = gnos_audio::AudioEndpointSnapshot::new(
        gnos_audio::AudioEndpointId::new(7, 42),
        gnos_audio::AudioEndpointKind::Output,
        "sink.lab".into(),
        "Lab Audio".into(),
        gnos_audio::AudioVolume::from_parts(
            None,
            vec![gnos_audio::NativeVolume::new(user_level.powi(3)).unwrap(); 2],
            vec![gnos_audio::NativeVolume::new(1.0).unwrap(); 2],
        ),
        muted,
    );
    state.upsert_endpoint(endpoint);
    state.set_effective_default_name(
        gnos_audio::DefaultRoleKind::Output,
        Some(
            if resolve_default {
                "sink.lab"
            } else {
                "sink.missing"
            }
            .into(),
        ),
    );
    runtime.set_audio(state.snapshot());
    ProductLabOutput {
        name: name.to_owned(),
        runtime,
        audio_osd,
    }
}

// ---------------------------------------------------------------------------
// Observed audio OSD fixtures.
//
// These construct the same transient presentation state the scene builds from
// an accepted PipeWire observation, without a live provider and without a
// display. They prove appearance and lifetime semantics; the audio control loop
// itself is established by the Phase 4 action gate and the Phase 5 live gate.
// ---------------------------------------------------------------------------

/// Builds a presented OSD state for an observed output at `user_level`/`muted`.
///
/// The state is produced by the real transition path — a baseline observation
/// followed by the change that actually happened — so a fixture cannot drift
/// from production presentation semantics.
fn osd_state(output: &str, user_level: f32, muted: bool) -> AudioOsdPresentationState {
    osd_state_motion(output, user_level, muted, false)
}

fn osd_state_motion(
    output: &str,
    user_level: f32,
    muted: bool,
    reduced_motion: bool,
) -> AudioOsdPresentationState {
    let mut presentation = AudioOsdPresentationState::new(
        AudioOsdPlacement::new(vec![output.to_owned()]),
        crate::product_audio_osd::AUDIO_OSD_DWELL,
        if reduced_motion {
            Duration::ZERO
        } else {
            Duration::from_millis(180)
        },
        if reduced_motion {
            Duration::ZERO
        } else {
            Duration::from_millis(180)
        },
    );
    let baseline = lab_audio_snapshot(0.90, false);
    let observed = lab_audio_snapshot(user_level, muted);
    let now = std::time::Instant::now();
    assert!(
        presentation.observe(&baseline, now).is_none(),
        "a baseline must not present"
    );
    presentation.observe(&observed, now);
    presentation
}

/// A hidden OSD: a baseline that never changed, so no surface exists.
fn osd_state_hidden(output: &str) -> AudioOsdPresentationState {
    let mut presentation = AudioOsdPresentationState::new(
        AudioOsdPlacement::new(vec![output.to_owned()]),
        crate::product_audio_osd::AUDIO_OSD_DWELL,
        Duration::from_millis(180),
        Duration::from_millis(180),
    );
    let now = std::time::Instant::now();
    presentation.observe(&lab_audio_snapshot(0.90, false), now);
    presentation
}

/// The same observed output facts `audio_output` projects, in snapshot form.
fn lab_audio_snapshot(user_level: f32, muted: bool) -> gnos_audio::AudioSnapshot {
    let id = gnos_audio::AudioEndpointId::new(7, 42);
    let endpoint = gnos_audio::AudioEndpointSnapshot::new(
        id,
        gnos_audio::AudioEndpointKind::Output,
        "sink.lab".into(),
        "Lab Audio".into(),
        gnos_audio::AudioVolume::from_parts(
            None,
            vec![gnos_audio::NativeVolume::new(user_level.powi(3)).unwrap(); 2],
            vec![gnos_audio::NativeVolume::new(1.0).unwrap(); 2],
        ),
        muted,
    );
    let mut state = gnos_audio::AudioState::new(7);
    state.upsert_endpoint(endpoint);
    state.set_effective_default_name(gnos_audio::DefaultRoleKind::Output, Some("sink.lab".into()));
    state.snapshot()
}

/// One OSD fixture: a single output that owns the surface.
fn audio_osd_fixture(
    name: &'static str,
    description: &'static str,
    user_level: f32,
    muted: bool,
    text_scale: f32,
    reduced_motion: bool,
) -> ProductLabFixture {
    let owner = "DP-1";
    ProductLabFixture {
        name,
        description,
        design: design(text_scale, reduced_motion),
        outputs: vec![audio_output_with_osd(
            name,
            user_level,
            muted,
            true,
            // Text scale belongs to the fixture's design context, so the same
            // state renders at 1x and 2x from one construction path.
            Some(osd_state_motion(owner, user_level, muted, reduced_motion)),
        )],
    }
}

/// The hidden fixture: a baseline that never changed, so nothing is presented.
fn audio_osd_hidden_fixture() -> ProductLabFixture {
    let owner = "DP-1";
    ProductLabFixture {
        name: "audio-osd-hidden",
        description: "No OSD presented, because the observed state did not change",
        design: design(1.0, false),
        outputs: vec![audio_output_with_osd(
            "audio-osd-hidden",
            0.90,
            false,
            true,
            Some(osd_state_hidden(owner)),
        )],
    }
}

/// The single-output ownership fixture: the same observed state on two outputs,
/// presented on only the output that owns the resolved placement.
fn audio_osd_single_owner() -> ProductLabFixture {
    let presentation = osd_state("DP-1", 0.35, false);
    ProductLabFixture {
        name: "audio-osd-single-owner",
        description: "One global audio event resolves to exactly one output",
        design: design(1.0, false),
        outputs: vec![
            audio_output_with_osd("DP-1 owner", 0.35, false, true, Some(presentation.clone())),
            audio_output_with_osd("DP-2 observer", 0.35, false, true, None),
        ],
    }
}

// ---------------------------------------------------------------------------
// Native network fixtures.
//
// These construct typed NetworkSnapshot facts and reduce them through the same
// production presentation path the Bar uses. They prove presentation semantics
// only; live causality is established by the NETWORK-01 qualification gates.
// ---------------------------------------------------------------------------

const NETWORK_OWNER: &str = ":1.7";

fn network_ref(path: &str) -> gnos_network::NetworkObjectRef {
    gnos_network::NetworkObjectRef::new(NETWORK_OWNER, path)
}

fn network_wifi_facts(ssid: &str, strength: u8) -> gnos_network::NetworkWifiFacts {
    gnos_network::NetworkWifiFacts {
        access_point: Some(network_ref("/org/freedesktop/NetworkManager/AccessPoint/1")),
        bssid: Some("0E:AC:8A:58:8B:61".into()),
        ssid: ssid.into(),
        strength,
        frequency_mhz: 5220,
        security: gnos_network::decode_rsn_security(3, 0, 392),
    }
}

fn network_wifi(
    state: gnos_network::NetworkDeviceState,
    strength: u8,
) -> gnos_network::NetworkDeviceSnapshot {
    gnos_network::NetworkDeviceSnapshot {
        id: network_ref("/org/freedesktop/NetworkManager/Devices/3"),
        interface: "wlan0".into(),
        kind: gnos_network::NetworkDeviceKind::Wifi,
        state,
        managed: true,
        primary: state.is_connected(),
        active_connection: state
            .is_connected()
            .then(|| network_ref("/org/freedesktop/NetworkManager/ActiveConnection/4")),
        settings_connection: None,
        relevance: gnos_network::NetworkRelevance::Unsupported,
        wifi: state
            .is_connected()
            .then(|| network_wifi_facts("Cartier50", strength)),
    }
}

fn network_ethernet(
    state: gnos_network::NetworkDeviceState,
    primary: bool,
) -> gnos_network::NetworkDeviceSnapshot {
    gnos_network::NetworkDeviceSnapshot {
        id: network_ref("/org/freedesktop/NetworkManager/Devices/2"),
        interface: "enp3s0".into(),
        kind: gnos_network::NetworkDeviceKind::Ethernet,
        state,
        managed: true,
        primary,
        active_connection: state
            .is_connected()
            .then(|| network_ref("/org/freedesktop/NetworkManager/ActiveConnection/9")),
        settings_connection: None,
        relevance: gnos_network::NetworkRelevance::Unsupported,
        wifi: None,
    }
}

fn network_overlay(state: gnos_network::NetworkDeviceState) -> gnos_network::NetworkDeviceSnapshot {
    gnos_network::NetworkDeviceSnapshot {
        id: network_ref("/org/freedesktop/NetworkManager/Devices/4"),
        interface: "wg-gnu6".into(),
        kind: gnos_network::NetworkDeviceKind::Wireguard,
        state,
        managed: true,
        primary: false,
        active_connection: state
            .is_connected()
            .then(|| network_ref("/org/freedesktop/NetworkManager/ActiveConnection/2")),
        settings_connection: None,
        relevance: gnos_network::NetworkRelevance::Unsupported,
        wifi: None,
    }
}

/// Container/VM infrastructure, present in bulk on the host.
fn network_infrastructure(count: usize) -> Vec<gnos_network::NetworkDeviceSnapshot> {
    (0..count)
        .map(|index| gnos_network::NetworkDeviceSnapshot {
            id: network_ref(&format!(
                "/org/freedesktop/NetworkManager/Devices/{}",
                100 + index
            )),
            interface: format!("br-{index:012x}"),
            kind: gnos_network::NetworkDeviceKind::Bridge,
            state: gnos_network::NetworkDeviceState::Connected,
            managed: true,
            primary: false,
            active_connection: Some(network_ref(&format!(
                "/org/freedesktop/NetworkManager/ActiveConnection/{}",
                200 + index
            ))),
            settings_connection: None,
            relevance: gnos_network::NetworkRelevance::Unsupported,
            wifi: None,
        })
        .collect()
}

fn network_globals(
    state: gnos_network::NetworkDeviceState,
    connectivity: gnos_network::NetworkConnectivity,
) -> gnos_network::NetworkGlobals {
    gnos_network::NetworkGlobals {
        state,
        connectivity: gnos_network::NetworkConnectivityAssessment {
            value: connectivity,
            // Connects to the host fact that the connectivity check is disabled,
            // so `Full` may never be presented as verified internet.
            checked: false,
        },
        primary_connection: Some(network_ref(
            "/org/freedesktop/NetworkManager/ActiveConnection/4",
        )),
        primary_connection_type: Some("802-11-wireless".into()),
        networking_enabled: true,
        wireless_enabled: true,
    }
}

fn network_snapshot(
    provider: gnos_network::NetworkProviderState,
    globals: Option<gnos_network::NetworkGlobals>,
    devices: Vec<gnos_network::NetworkDeviceSnapshot>,
) -> gnos_network::NetworkSnapshot {
    gnos_network::NetworkSnapshot {
        provider,
        generation: 1,
        provider_owner: (provider == gnos_network::NetworkProviderState::Unavailable
            && globals.is_none())
        .then(|| NETWORK_OWNER.to_owned())
        .or_else(|| Some(NETWORK_OWNER.to_owned())),
        globals,
        devices: devices
            .into_iter()
            .map(|device| (device.id.clone(), device))
            .collect(),
        active_connections: Default::default(),
        settings_connections: Default::default(),
    }
}

/// A ready Wi-Fi network at `strength`, as NetworkManager would publish it.
/// Shared with the controlled reference Status fixture.
pub(crate) fn wifi_ready_network(strength: u8) -> gnos_network::NetworkSnapshot {
    network_snapshot(
        gnos_network::NetworkProviderState::Available,
        Some(network_globals(
            gnos_network::NetworkDeviceState::ConnectedGlobal,
            gnos_network::NetworkConnectivity::Full,
        )),
        vec![network_wifi(
            gnos_network::NetworkDeviceState::Connected,
            strength,
        )],
    )
}

fn network_output(name: &str, snapshot: gnos_network::NetworkSnapshot) -> ProductLabOutput {
    let ws = workspace(1, "1", 1);
    let mut runtime = runtime(
        current(
            "wl-1",
            1,
            vec![ws.clone()],
            vec![ws],
            Some(0),
            Vec::new(),
            None,
        ),
        preferences(ProductPreferenceProjectionState::AuthorityUnavailable),
        ProductActionState::Idle,
    );
    runtime.set_network(snapshot);
    ProductLabOutput {
        name: name.to_owned(),
        runtime,
        audio_osd: None,
    }
}

fn network_fixture(
    name: &'static str,
    description: &'static str,
    snapshot: gnos_network::NetworkSnapshot,
) -> ProductLabFixture {
    ProductLabFixture {
        name,
        description,
        design: design(1.0, false),
        outputs: vec![network_output(name, snapshot)],
    }
}

fn network_wifi_fixture(name: &'static str, strength: u8) -> ProductLabFixture {
    network_fixture(
        name,
        "Observed Wi-Fi signal band",
        network_snapshot(
            NetworkProviderState::Available,
            Some(network_globals(
                NetworkDeviceState::ConnectedGlobal,
                NetworkConnectivity::Full,
            )),
            vec![NetworkDeviceState::Connected]
                .into_iter()
                .map(|state| network_wifi(state, strength))
                .collect(),
        ),
    )
}

/// The hostile fixture: one relevant physical network, one overlay, and a mass of
/// bridges, veth and loopback facts. The Bar projection must stay stable and
/// unambiguous.
fn network_many_irrelevant() -> ProductLabFixture {
    let mut devices = vec![
        network_wifi(NetworkDeviceState::Connected, 68),
        network_overlay(NetworkDeviceState::Connected),
        network_ethernet(NetworkDeviceState::Disconnected, false),
    ];
    devices.extend(network_infrastructure(54));
    network_fixture(
        "network-many-irrelevant-devices",
        "1 physical + 1 overlay + 54 infrastructure devices",
        network_snapshot(
            NetworkProviderState::Available,
            Some(network_globals(
                NetworkDeviceState::ConnectedGlobal,
                NetworkConnectivity::Full,
            )),
            devices,
        ),
    )
}

fn audio_fixture(
    name: &'static str,
    user_level: f32,
    muted: bool,
    resolve_default: bool,
) -> ProductLabFixture {
    ProductLabFixture {
        name,
        description: "Typed read-only PipeWire audio state projected into the Bar",
        design: design(1.0, false),
        outputs: vec![audio_output(name, user_level, muted, resolve_default)],
    }
}

fn audio_unavailable() -> ProductLabFixture {
    let ws = workspace(1, "1", 1);
    ProductLabFixture {
        name: "audio-unavailable",
        description: "Audio provider unavailable without fabricating zero volume",
        design: design(1.0, false),
        outputs: vec![ProductLabOutput {
            name: "audio-unavailable".into(),
            runtime: runtime(
                current(
                    "wl-1",
                    1,
                    vec![ws.clone()],
                    vec![ws],
                    Some(0),
                    Vec::new(),
                    None,
                ),
                preferences(ProductPreferenceProjectionState::AuthorityUnavailable),
                ProductActionState::Idle,
            ),
            audio_osd: None,
        }],
    }
}

// ---------------------------------------------------------------------------
// MPRIS Now Playing fixtures.
//
// These fixtures exist to prove *rendering semantics*, not live transport
// behaviour. Each one carries state a live exporter was observed to publish.
// ---------------------------------------------------------------------------

/// Builds a lab player. Defaults describe a healthy playing track; each fixture
/// overrides only what its observation actually changed.
fn lab_player(
    service: &str,
    owner: &str,
    label: &str,
    playback: crate::product_mpris::PlaybackState,
    capabilities: crate::product_mpris::PlaybackCapabilities,
) -> crate::product_mpris::MprisPlayerSnapshot {
    use crate::product_mpris::{
        ExactMprisPlayerRef, MprisPlayerId, MprisPlayerSnapshot, PlaybackPosition, TrackIdentity,
        TrackMetadata,
    };
    MprisPlayerSnapshot {
        identity: ExactMprisPlayerRef {
            id: MprisPlayerId::new(format!("{service}@{owner}")),
            service: service.to_owned(),
            path: "/org/mpris/MediaPlayer2".to_owned(),
            owner: owner.to_owned(),
        },
        label: label.to_owned(),
        application_key: Some("mpris-demo".to_owned()),
        playback,
        track: TrackMetadata {
            identity: TrackIdentity(Some("/track/1".to_owned())),
            title: Some("Weightless".to_owned()),
            artists: vec!["Marconi Union".to_owned()],
            album: Some("Ambient Transmissions, Vol. 1".to_owned()),
            artwork: Some(crate::product_mpris::MprisArtwork::Renderable(
                "file:///tmp/cover.png".to_owned(),
            )),
            length: Some(std::time::Duration::from_secs(481)),
            url: Some("file:///music/weightless.flac".to_owned()),
        },
        position: PlaybackPosition::observed(
            std::time::Duration::from_secs(96),
            std::time::Instant::now(),
        ),
        rate: 1.0,
        volume: 0.8,
        capabilities,
        diagnostics: Vec::new(),
    }
}

fn all_capabilities() -> crate::product_mpris::PlaybackCapabilities {
    crate::product_mpris::PlaybackCapabilities {
        can_control: true,
        can_play: true,
        can_pause: true,
        can_go_next: true,
        can_go_previous: true,
        can_seek: true,
    }
}

/// One output whose runtime carries the given players.
fn mpris_output(
    name: &str,
    players: Vec<crate::product_mpris::MprisPlayerSnapshot>,
) -> ProductLabOutput {
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
    runtime.set_mpris(crate::product_mpris::MprisSnapshot::new(players));
    ProductLabOutput {
        name: name.to_owned(),
        runtime,
        audio_osd: None,
    }
}

fn mpris_fixture(
    name: &'static str,
    description: &'static str,
    players: Vec<crate::product_mpris::MprisPlayerSnapshot>,
) -> ProductLabFixture {
    ProductLabFixture {
        name,
        description,
        design: design(1.0, false),
        outputs: vec![mpris_output(name, players)],
    }
}

fn mpris_empty() -> ProductLabFixture {
    mpris_fixture("mpris-empty", "No player on the bus", Vec::new())
}

fn mpris_playing() -> ProductLabFixture {
    mpris_fixture(
        "mpris-playing",
        "A playing track with a known length and artwork",
        vec![lab_player(
            "org.mpris.MediaPlayer2.demo",
            ":1.10",
            "Demo Player",
            crate::product_mpris::PlaybackState::Playing,
            all_capabilities(),
        )],
    )
}

fn mpris_paused() -> ProductLabFixture {
    // Finding 9: a live player reported Rate 0.0 while Paused. The fixture
    // reproduces that, so the surface cannot come to rely on rate.
    let mut player = lab_player(
        "org.mpris.MediaPlayer2.demo",
        ":1.10",
        "Demo Player",
        crate::product_mpris::PlaybackState::Paused,
        all_capabilities(),
    );
    player.rate = 0.0;
    mpris_fixture(
        "mpris-paused",
        "Paused with Rate 0.0, as observed live on chromium",
        vec![player],
    )
}

fn mpris_missing_metadata() -> ProductLabFixture {
    // Finding 5: a live VLC published no title, artist, album or artwork, only
    // a file URL and vendor keys.
    let mut player = lab_player(
        "org.mpris.MediaPlayer2.vlc",
        ":1.5065",
        "VLC media player",
        crate::product_mpris::PlaybackState::Playing,
        all_capabilities(),
    );
    player.track.title = None;
    player.track.artists = Vec::new();
    player.track.album = None;
    player.track.artwork = None;
    player.track.identity = Default::default();
    player.track.url = Some("file:///home/u/Music/Interlude.ogg".to_owned());
    player.application_key = Some("vlc".to_owned());
    mpris_fixture(
        "mpris-missing-metadata",
        "No title/artist/album/artwork; only a file URL, as observed live on vlc",
        vec![player],
    )
}

fn mpris_long_title() -> ProductLabFixture {
    let mut player = lab_player(
        "org.mpris.MediaPlayer2.demo",
        ":1.10",
        "Demo Player",
        crate::product_mpris::PlaybackState::Playing,
        all_capabilities(),
    );
    player.track.title = Some(
        "Synchronisation avancée en arrière-plan — 14 382 fichiers restants (remix \
         interstellar, édition deluxe.annotation mixée)"
            .to_owned(),
    );
    player.track.artists = vec![
        "Un Artiste".to_owned(),
        "Deuxième Artiste".to_owned(),
        "Third".to_owned(),
    ];
    mpris_fixture(
        "mpris-long-title",
        "Untranslated long title and multiple artists at 2x scale",
        vec![player],
    )
}

fn mpris_no_artwork() -> ProductLabFixture {
    let mut player = lab_player(
        "org.mpris.MediaPlayer2.chromium.i1",
        ":1.44",
        "Helium",
        crate::product_mpris::PlaybackState::Paused,
        all_capabilities(),
    );
    player.track.artwork = None;
    // Finding 4: chromium answered Identity but not DesktopEntry.
    player.application_key = None;
    mpris_fixture(
        "mpris-no-artwork",
        "No artwork and no DesktopEntry, as observed live on chromium",
        vec![player],
    )
}

fn mpris_controls_disabled() -> ProductLabFixture {
    // CanControl true with every individual capability false is the sharpest
    // case: a surface must disable every control, not just the missing ones.
    let mut player = lab_player(
        "org.mpris.MediaPlayer2.locked",
        ":1.20",
        "Locked Player",
        crate::product_mpris::PlaybackState::Paused,
        crate::product_mpris::PlaybackCapabilities {
            can_control: true,
            ..Default::default()
        },
    );
    player.track.title = Some("Unavailable".to_owned());
    mpris_fixture(
        "mpris-controls-disabled",
        "Control accepted but every capability absent",
        vec![player],
    )
}

fn mpris_multiple_players() -> ProductLabFixture {
    // Finding 3: one logical player under two names with two owners, plus a
    // genuinely different second player. The duplicate must fold and the other
    // must survive.
    let first = lab_player(
        "org.mpris.MediaPlayer2.vlc",
        ":1.5065",
        "VLC media player",
        crate::product_mpris::PlaybackState::Playing,
        all_capabilities(),
    );
    let mut duplicate = first.clone();
    duplicate.identity.id =
        crate::product_mpris::MprisPlayerId::new("org.mpris.MediaPlayer2.vlc.i@:1.5066");
    duplicate.identity.service = "org.mpris.MediaPlayer2.vlc.instance1040494".to_owned();
    duplicate.identity.owner = ":1.5066".to_owned();

    let mut second = lab_player(
        "org.mpris.MediaPlayer2.chromium.i1",
        ":1.44",
        "Helium",
        crate::product_mpris::PlaybackState::Paused,
        all_capabilities(),
    );
    second.track.title = Some("Andre The Giant".to_owned());
    second.track.artists = vec!["Prof".to_owned()];
    second.track.album = Some("Pookie Baby".to_owned());

    mpris_fixture(
        "mpris-multiple-players",
        "One player duplicated across two names, plus a distinct second player",
        vec![first, duplicate, second],
    )
}

fn mpris_player_disappeared() -> ProductLabFixture {
    // The rendered player is gone from the bus, so a control bound to it must
    // refuse rather than act.
    mpris_fixture(
        "mpris-player-disappeared",
        "Rendered player no longer a member of the snapshot",
        Vec::new(),
    )
}

fn mpris_owner_replaced() -> ProductLabFixture {
    // Same well-known name, different unique owner. This is the case that makes
    // a stale binding dangerous: the name resolves, but to somebody else.
    let replacement = lab_player(
        "org.mpris.MediaPlayer2.demo",
        ":1.999",
        "Demo Player",
        crate::product_mpris::PlaybackState::Playing,
        all_capabilities(),
    );
    mpris_fixture(
        "mpris-owner-replaced",
        "Well-known name re-requested by a different owner",
        vec![replacement],
    )
}

fn mpris_track_changed() -> ProductLabFixture {
    // A track that has just advanced: new identity, new title, position reset to
    // zero. Mirrors what a real Next() produced.
    let mut player = lab_player(
        "org.mpris.MediaPlayer2.demo",
        ":1.10",
        "Demo Player",
        crate::product_mpris::PlaybackState::Playing,
        all_capabilities(),
    );
    player.track.identity = crate::product_mpris::TrackIdentity(Some("/track/2".to_owned()));
    player.track.title = Some("Second Song".to_owned());
    player.track.artists = vec!["Artist Two".to_owned()];
    player.track.album = Some("Album Beta".to_owned());
    player.position = crate::product_mpris::PlaybackPosition::observed(
        std::time::Duration::ZERO,
        std::time::Instant::now(),
    );
    mpris_fixture(
        "mpris-track-changed",
        "Track advanced: new identity and title, position reset to zero",
        vec![player],
    )
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
    fn the_mpris_fixtures_offer_what_they_claim_to() {
        use crate::product_now_playing::NowPlayingPresentation;

        // A visual fixture that renders the wrong content is worse than no
        // fixture, because it is reviewed and then trusted.

        let empty = fixture("mpris-empty").expect("empty");
        assert!(
            empty.outputs[0].runtime.mpris().is_empty(),
            "the empty fixture must offer no player"
        );
        let derived = NowPlayingPresentation::derive(&empty.outputs[0].runtime.now_playing());
        assert!(derived.empty);
        assert!(
            derived.controls.is_empty(),
            "an empty state must offer no transport control, because there is nobody to act on"
        );

        let playing = fixture("mpris-playing").expect("playing");
        let runtime = &playing.outputs[0].runtime;
        let derived = NowPlayingPresentation::derive(&runtime.now_playing());
        assert_eq!(derived.title.as_deref(), Some("Weightless"));
        assert_eq!(derived.artists, vec!["Marconi Union".to_owned()]);
        assert_eq!(derived.playback_label, "Lecture");
        assert_eq!(derived.progress, Some(96.0 / 481.0), "96s of 481s");
        assert!(derived.controls.iter().all(|c| c.enabled));
        assert!(derived.artwork.is_some());

        let paused = fixture("mpris-paused").expect("paused");
        let runtime = &paused.outputs[0].runtime;
        assert_eq!(
            runtime.mpris().players()[0].rate,
            0.0,
            "rate 0.0 while paused"
        );
        let derived = NowPlayingPresentation::derive(&runtime.now_playing());
        assert_eq!(
            derived.playback_label, "En pause",
            "playback state must come from PlaybackStatus, never from rate"
        );
        let play_pause = derived
            .controls
            .iter()
            .find(|c| c.operation == crate::product_mpris::MprisOperation::PlayPause)
            .expect("play/pause");
        assert_eq!(play_pause.glyph, "▶", "a paused player offers play");
    }

    #[test]
    fn the_missing_metadata_fixture_really_is_missing_metadata() {
        use crate::product_now_playing::NowPlayingPresentation;

        let fixture = fixture("mpris-missing-metadata").expect("missing metadata");
        let runtime = &fixture.outputs[0].runtime;
        let player = &runtime.mpris().players()[0];
        assert!(player.track.title.is_none());
        assert!(player.track.artists.is_empty());
        assert!(player.track.artwork.is_none());

        let derived = NowPlayingPresentation::derive(&runtime.now_playing());
        assert_eq!(
            derived.title.as_deref(),
            Some("Interlude.ogg"),
            "the URL's final segment is the only identifying information an exporter gave"
        );
        assert!(
            derived.notes.iter().any(|n| n.contains("titre absent")),
            "the fallback must be disclosed"
        );
    }

    #[test]
    fn the_disabled_controls_fixture_disables_every_control() {
        use crate::product_now_playing::NowPlayingPresentation;

        let fixture = fixture("mpris-controls-disabled").expect("disabled");
        let runtime = &fixture.outputs[0].runtime;
        let player = &runtime.mpris().players()[0];
        assert!(
            player.capabilities.can_control,
            "the fixture's point is that control is accepted while no capability exists"
        );
        assert!(!player.capabilities.can_play);
        assert!(!player.capabilities.can_go_next);

        let derived = NowPlayingPresentation::derive(&runtime.now_playing());
        assert_eq!(derived.controls.len(), 3);
        assert!(
            derived.controls.iter().all(|c| !c.enabled),
            "a control with no capability must be disabled, never silently clickable"
        );
        assert!(
            derived
                .controls
                .iter()
                .all(|c| c.disabled_reason == Some("capacité absente"))
        );
        // Progress is deliberately unaffected by capabilities: this fixture
        // still publishes a length and an observed position, so the bar is
        // truthful even though every control is inert. Hiding it would imply
        // the track is unknown when it is merely uncontrollable.
        assert_eq!(
            derived.progress,
            Some(96.0 / 481.0),
            "a known length and observed position remain meaningful without capabilities"
        );
    }

    #[test]
    fn the_multiple_players_fixture_folds_the_duplicate() {
        use crate::product_now_playing::NowPlayingPresentation;

        let fixture = fixture("mpris-multiple-players").expect("multiple");
        let runtime = &fixture.outputs[0].runtime;
        assert_eq!(
            runtime.mpris().len(),
            3,
            "three names were observed: two duplicate reporters and one distinct player"
        );

        let presentation = runtime.now_playing();
        assert_eq!(
            presentation.distinct_players().len(),
            2,
            "the duplicate reporters must fold to one player, leaving two distinct"
        );
        let derived = NowPlayingPresentation::derive(&presentation);
        assert_eq!(
            derived.player_label, "Helium",
            "selection is deterministic by well-known name: chromium before vlc"
        );
        assert_eq!(derived.title.as_deref(), Some("Andre The Giant"));
        assert_eq!(derived.other_players.len(), 1);
    }

    #[test]
    fn the_disappeared_and_replaced_fixtures_separate_the_two_cases() {
        // Disappearance and owner replacement are different failures and must
        // not be conflated: one is an absent player, the other is a live player
        // that is not the one a control was bound to.
        let disappeared = fixture("mpris-player-disappeared").expect("disappeared");
        assert!(disappeared.outputs[0].runtime.mpris().is_empty());

        let replaced = fixture("mpris-owner-replaced").expect("replaced");
        let runtime = &replaced.outputs[0].runtime;
        let player = &runtime.mpris().players()[0];
        assert_eq!(player.identity.service, "org.mpris.MediaPlayer2.demo");
        assert_eq!(
            player.identity.owner, ":1.999",
            "the name resolves, but to a different owner than a stale binding would carry"
        );
    }

    #[test]
    fn a_binding_to_a_replaced_owner_does_not_match_the_replacement() {
        // The regression this fixture exists to guard: identity is keyed on the
        // owner, so a control painted for :1.998 cannot act on :1.999 even
        // though the well-known name and every visible field are identical.
        let fixture = fixture("mpris-owner-replaced").expect("replaced");
        let runtime = &fixture.outputs[0].runtime;
        let current = &runtime.mpris().players()[0];

        let mut stale = current.identity.clone();
        stale.owner = ":1.998".to_owned();
        assert!(
            !runtime.mpris().contains(&stale),
            "a stale binding must not match the replacement instance"
        );
        assert!(runtime.mpris().contains(&current.identity));
    }

    #[test]
    fn the_track_changed_fixture_shows_the_reset() {
        let fixture = fixture("mpris-track-changed").expect("track changed");
        let player = &fixture.outputs[0].runtime.mpris().players()[0];
        assert_eq!(player.track.identity.0.as_deref(), Some("/track/2"));
        assert_eq!(player.track.title.as_deref(), Some("Second Song"));
        assert_eq!(
            player.position.position(),
            std::time::Duration::ZERO,
            "a new track restarts from zero"
        );
    }

    #[test]
    fn the_long_title_fixture_is_actually_long() {
        let fixture = fixture("mpris-long-title").expect("long");
        let derived = crate::product_now_playing::NowPlayingPresentation::derive(
            &fixture.outputs[0].runtime.now_playing(),
        );
        let title = derived.title.expect("title");
        assert!(title.chars().count() > 60, "the long title must be long");
        assert_eq!(derived.artists.len(), 3, "multiple artists stay a list");
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

    #[test]
    fn interaction_fixtures_cover_the_donor15_matrix() {
        for name in [
            "tray-item-activatable",
            "tray-item-menu-only",
            "tray-menu-basic",
            "tray-menu-nested",
            "tray-menu-disabled",
            "tray-menu-toggle",
            "tray-menu-radio",
            "tray-menu-long-label",
            "tray-menu-layout-updated",
            "tray-item-disappeared",
            "tray-item-owner-replaced",
            "tray-menu-stale-revision",
        ] {
            let fixture = fixture(name).unwrap_or_else(|| panic!("fixture {name}"));
            assert!(!fixture.outputs.is_empty(), "{name} carries an output");
            for output in &fixture.outputs {
                assert!(!output.name.is_empty(), "{name} output carries a name");
            }
        }
    }

    #[test]
    fn an_item_that_is_a_menu_renders_the_menu_policy() {
        use crate::product_tray_action::{PrimaryTrayInteraction, primary_interaction};

        let activatable = fixture("tray-item-activatable").unwrap();
        let item = activatable.outputs[0]
            .runtime
            .tray()
            .items()
            .first()
            .cloned()
            .expect("one item");
        assert!(!item.item_is_menu);
        assert_eq!(primary_interaction(&item), PrimaryTrayInteraction::Activate);

        let menu_only = fixture("tray-item-menu-only").unwrap();
        let item = menu_only.outputs[0]
            .runtime
            .tray()
            .items()
            .first()
            .cloned()
            .expect("one item");
        assert!(item.item_is_menu);
        assert_eq!(primary_interaction(&item), PrimaryTrayInteraction::OpenMenu);
    }

    #[test]
    fn menu_fixtures_project_actionable_rows() {
        let basic = fixture("tray-menu-basic").unwrap();
        let menu = basic.outputs[0].runtime.menu().expect("projected menu");
        assert_eq!(menu.actionable_items().len(), 2);

        let disabled = fixture("tray-menu-disabled").unwrap();
        let menu = disabled.outputs[0].runtime.menu().expect("projected menu");
        assert_eq!(menu.item_count(), 2, "a disabled row is still presentable");
        assert_eq!(
            menu.actionable_items().len(),
            1,
            "a disabled row is not actionable"
        );

        let nested = fixture("tray-menu-nested").unwrap();
        let menu = nested.outputs[0].runtime.menu().expect("projected menu");
        assert!(
            menu.find_item(6).is_some(),
            "nested entries are reachable by id"
        );
    }

    #[test]
    fn the_layout_updated_fixture_presents_two_revisions() {
        let fixture = fixture("tray-menu-layout-updated").unwrap();
        assert_eq!(fixture.outputs.len(), 2);
        let before = fixture.outputs[0].runtime.menu().expect("before").revision;
        let after = fixture.outputs[1].runtime.menu().expect("after").revision;
        assert!(
            after > before,
            "the second output shows the menu after a layout update"
        );
    }

    #[test]
    fn the_stale_revision_fixture_leaves_its_action_invalid() {
        use crate::product_tray_action::TrayMenuEvent;

        let fixture = fixture("tray-menu-stale-revision").unwrap();
        let runtime = &fixture.outputs[0].runtime;
        let menu = runtime.menu().expect("projected menu");
        let observed = runtime.tray().items().first().cloned().expect("item");
        let bound = crate::product_tray_action::ExactTrayItemRef::from_observed(
            &observed,
            observed.owner.clone(),
        );

        // Bind against an earlier revision than the one now projected.
        let stale = crate::product_tray_action::TrayMenuEventBinding {
            item: bound,
            menu_id: 1,
            revision: menu.revision - 1,
            event: TrayMenuEvent::Clicked,
            rendered_label: Some("Quit".to_owned()),
        };
        assert_eq!(
            menu.revalidate(&stale),
            Err(crate::product_tray_action::StaleReason::MenuRevisionChanged),
            "the lab fixture must demonstrate a refused stale action"
        );
    }

    #[test]
    fn the_owner_replaced_fixture_records_a_different_owner() {
        let fixture = fixture("tray-item-owner-replaced").unwrap();
        let item = fixture.outputs[0]
            .runtime
            .tray()
            .items()
            .first()
            .cloned()
            .expect("one item");
        assert_eq!(item.owner, ":1.replaced");
        assert!(
            item.owner.starts_with(':'),
            "owner is unique-name provenance"
        );
    }

    #[test]
    fn tray_fixtures_carry_expected_tray_snapshots() {
        let empty = fixture("tray-empty").unwrap();
        assert!(matches!(
            empty.outputs[0].runtime.tray(),
            TraySnapshot::Active { items } if items.is_empty()
        ));

        let one = fixture("tray-one-item").unwrap();
        assert!(matches!(
            one.outputs[0].runtime.tray(),
            TraySnapshot::Active { items } if items.len() == 1
        ));

        let many = fixture("tray-many-items").unwrap();
        assert!(matches!(
            many.outputs[0].runtime.tray(),
            TraySnapshot::Active { items } if items.len() == 4
        ));

        let attention = fixture("tray-attention").unwrap();
        match attention.outputs[0].runtime.tray() {
            TraySnapshot::Active { items } => {
                assert_eq!(items[0].status, TrayStatus::NeedsAttention);
            }
            _ => panic!("expected active tray with attention item"),
        }

        let unavail = fixture("tray-provider-unavailable").unwrap();
        assert_eq!(
            unavail.outputs[0].runtime.tray(),
            &TraySnapshot::Unavailable
        );

        let compact = fixture("tray-compact").unwrap();
        assert_eq!(
            compact.outputs[0]
                .runtime
                .preferences()
                .bar_overrides
                .tray_compact,
            Some(true)
        );

        let expanded = fixture("tray-expanded").unwrap();
        assert_eq!(
            expanded.outputs[0]
                .runtime
                .preferences()
                .bar_overrides
                .tray_compact,
            Some(false)
        );
        assert_eq!(expanded.design.text_scale(), 2.0);
    }

    #[test]
    fn audio_osd_fixtures_present_their_observed_states() {
        use crate::product_audio_osd::{AudioOsdPhase, AudioOsdPresentation};

        // Each fixture renders a real presentation derived from the same
        // transition path production uses, so the rendered facts are the
        // observed ones.
        let expectations = [
            ("audio-osd-volume-0", "0%", None, false),
            ("audio-osd-volume-mid", "45%", None, false),
            ("audio-osd-volume-100", "100%", None, false),
            ("audio-osd-muted", "45%", Some("muet"), true),
            ("audio-osd-unmuted", "45%", None, false),
        ];
        for (name, percent, word, muted) in expectations {
            let fixture = fixture(name).unwrap_or_else(|| panic!("{name} must exist"));
            let state = fixture.outputs[0]
                .audio_osd
                .as_ref()
                .unwrap_or_else(|| panic!("{name} must present an OSD"));
            assert!(state.visible(), "{name} must be presented");
            assert_eq!(state.phase(), AudioOsdPhase::Appearing);
            let rendered = AudioOsdPresentation::derive(state).expect("rendered");
            assert_eq!(rendered.percent.as_deref(), Some(percent), "{name}");
            assert_eq!(rendered.state_word, word, "{name}");
            assert_eq!(rendered.muted, muted, "{name}");
            // The rendered level is the observed one, and the fixture's Bar
            // reflects the same truth the OSD reports.
            let observed = fixture.outputs[0].runtime.audio_presentation();
            let crate::product_audio::ProductAudioEndpointState::Ready(output) = observed.output
            else {
                panic!("{name} must resolve its output");
            };
            assert_eq!(output.muted, muted, "{name}");
        }
    }

    #[test]
    fn audio_osd_hidden_fixture_presents_nothing() {
        use crate::product_audio_osd::{AudioOsdPhase, render_audio_osd};

        let fixture = fixture("audio-osd-hidden").unwrap();
        let state = fixture.outputs[0]
            .audio_osd
            .as_ref()
            .expect("the fixture still carries the state");
        assert!(!state.visible());
        assert_eq!(state.phase(), AudioOsdPhase::Hidden);
        assert!(
            render_audio_osd(state.clone(), fixture.design).is_none(),
            "a hidden OSD must not paint an empty surface"
        );
    }

    #[test]
    fn audio_osd_long_scale_and_reduced_motion_fixtures_keep_the_same_truth() {
        use crate::product_audio_osd::AudioOsdPresentation;

        let normal = fixture("audio-osd-volume-mid").unwrap();
        let scaled = fixture("audio-osd-long-scale-2x").unwrap();
        let reduced = fixture("audio-osd-reduced-motion").unwrap();

        assert_eq!(scaled.design.text_scale(), 2.0);
        assert_eq!(normal.design.text_scale(), 1.0);
        assert!(
            reduced.design.reduced_motion(),
            "the reduced-motion fixture must carry the preference"
        );
        assert!(!normal.design.reduced_motion());

        // Scaling and motion preference change rendering, never reported truth.
        for (label, fixture) in [("scaled", &scaled), ("reduced", &reduced)] {
            let state = fixture.outputs[0].audio_osd.as_ref().expect("state");
            let rendered = AudioOsdPresentation::derive(state).expect("rendered");
            assert_eq!(rendered.percent.as_deref(), Some("45%"), "{label}");
            assert_eq!(rendered.aria_label, "Audio · Lab Audio · 45%", "{label}");
        }

        // Reduced motion reaches its resting phase immediately; full motion
        // passes through a transition.
        use crate::product_audio_osd::AudioOsdPhase;
        assert_eq!(
            normal.outputs[0].audio_osd.as_ref().unwrap().phase(),
            AudioOsdPhase::Appearing
        );
        assert_eq!(
            reduced.outputs[0].audio_osd.as_ref().unwrap().phase(),
            AudioOsdPhase::Shown
        );
    }

    #[test]
    fn audio_osd_single_output_ownership_is_visible_in_the_fixture() {
        let fixture = fixture("audio-osd-single-owner").unwrap();
        assert_eq!(fixture.outputs.len(), 2);
        // Exactly one output carries the surface, and it is the resolved owner.
        assert!(fixture.outputs[0].audio_osd.is_some());
        assert!(fixture.outputs[1].audio_osd.is_none());
        let owner = fixture.outputs[0]
            .audio_osd
            .as_ref()
            .and_then(|state| state.owner())
            .expect("an owner");
        assert_eq!(owner, "DP-1");
        // Both outputs report the same observed audio truth, so the difference
        // is placement alone.
        assert_eq!(
            fixture.outputs[0]
                .runtime
                .audio_presentation()
                .output
                .is_ready(),
            fixture.outputs[1]
                .runtime
                .audio_presentation()
                .output
                .is_ready()
        );
    }

    #[test]
    fn audio_fixtures_cover_ready_muted_amplified_unavailable_and_unresolved() {
        let normal = fixture("audio-normal").unwrap();
        let ready = normal.outputs[0].runtime.audio_presentation();
        let crate::product_audio::ProductAudioEndpointState::Ready(output) = ready.output else {
            panic!("audio-normal must resolve output");
        };
        assert!((output.level.unwrap() - 0.90).abs() < 0.0001);

        let muted = fixture("audio-muted").unwrap();
        let crate::product_audio::ProductAudioEndpointState::Ready(output) =
            muted.outputs[0].runtime.audio_presentation().output
        else {
            panic!("audio-muted must resolve output");
        };
        assert!(output.muted);

        let high = fixture("audio-high").unwrap();
        let crate::product_audio::ProductAudioEndpointState::Ready(output) =
            high.outputs[0].runtime.audio_presentation().output
        else {
            panic!("audio-high must resolve output");
        };
        assert!(output.level.unwrap() > 1.0);

        assert!(matches!(
            fixture("audio-unavailable").unwrap().outputs[0]
                .runtime
                .audio_presentation()
                .output,
            crate::product_audio::ProductAudioEndpointState::ProviderUnavailable
        ));
        assert!(matches!(
            fixture("audio-default-unresolved").unwrap().outputs[0]
                .runtime
                .audio_presentation()
                .output,
            crate::product_audio::ProductAudioEndpointState::DefaultUnresolved { .. }
        ));
    }
}

#[cfg(test)]
mod network_lab_tests {
    use super::*;
    use crate::product_network::ProductNetworkPrimary;

    fn presentation(name: &str) -> crate::product_network::ProductNetworkPresentation {
        let fixture = fixture(name).unwrap_or_else(|| panic!("{name} must exist"));
        fixture.outputs[0].runtime.network_presentation()
    }

    #[test]
    fn every_network_fixture_resolves_through_the_production_projection() {
        // Every name in the catalogue must be constructible and must reduce
        // without panicking, so the lab cannot present a state production could
        // not produce.
        for name in FIXTURE_NAMES
            .iter()
            .filter(|name| name.starts_with("network-"))
        {
            let fixture = fixture(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(fixture.outputs.len(), 1, "{name}");
            let _ = fixture.outputs[0].runtime.network_presentation();
        }
    }

    #[test]
    fn provider_unavailable_is_never_presented_as_disconnected() {
        let p = presentation("network-provider-unavailable");
        assert_eq!(p.provider, gnos_network::NetworkProviderState::Unavailable);
        assert_eq!(p.primary, ProductNetworkPrimary::Unavailable);
        assert_ne!(p.primary, ProductNetworkPrimary::Disconnected);
        assert_eq!(p.connectivity, gnos_network::NetworkConnectivity::Unknown);
        assert!(
            p.device_count == 0,
            "nothing is known, so nothing is reported"
        );
    }

    #[test]
    fn signal_band_fixtures_differ_only_in_the_observed_band() {
        for (name, band) in [
            ("network-wifi-weak", gnos_network::NetworkSignalBand::Weak),
            (
                "network-wifi-medium",
                gnos_network::NetworkSignalBand::Medium,
            ),
            (
                "network-wifi-strong",
                gnos_network::NetworkSignalBand::Strong,
            ),
        ] {
            let p = presentation(name);
            let ProductNetworkPrimary::Ready(endpoint) = &p.primary else {
                panic!("{name} must be ready");
            };
            assert_eq!(endpoint.wifi.as_ref().expect("wifi").band, band, "{name}");
            assert!(!p.overlay_active, "{name}");
        }
    }

    #[test]
    fn connectivity_fixtures_keep_connectivity_separate_from_transport() {
        for (name, connectivity) in [
            (
                "network-wifi-full-connectivity",
                gnos_network::NetworkConnectivity::Full,
            ),
            (
                "network-wifi-limited",
                gnos_network::NetworkConnectivity::Limited,
            ),
            (
                "network-wifi-portal",
                gnos_network::NetworkConnectivity::Portal,
            ),
        ] {
            let p = presentation(name);
            assert!(
                p.has_transport(),
                "{name} is connected regardless of its assessment"
            );
            assert_eq!(p.connectivity, connectivity, "{name}");
            assert!(
                !p.connectivity_checked,
                "{name}: the host check is disabled"
            );
        }
    }

    #[test]
    fn the_overlay_fixture_reports_the_overlay_separately_from_the_transport() {
        let p = presentation("network-overlay-active");
        assert!(p.overlay_active);
        let ProductNetworkPrimary::Ready(endpoint) = &p.primary else {
            panic!("wifi stays the transport");
        };
        assert_eq!(endpoint.transport, gnos_network::NetworkTransport::Wifi);
    }

    #[test]
    fn the_hostile_fixture_keeps_the_bar_projection_stable() {
        let fixture = fixture("network-many-irrelevant-devices").unwrap();
        let p = fixture.outputs[0].runtime.network_presentation();
        assert_eq!(p.device_count, 57, "wifi + overlay + ethernet + 54 bridges");
        assert_eq!(
            p.relevant_device_count, 3,
            "Wi-Fi, the overlay and the managed-but-disconnected wired port; \
the 54 bridges never count"
        );
        let ProductNetworkPrimary::Ready(endpoint) = &p.primary else {
            panic!("a mass of bridges must not disturb the primary transport");
        };
        assert_eq!(
            endpoint.transport,
            gnos_network::NetworkTransport::Wifi,
            "a bridge cannot become the Bar's network"
        );
        let status = crate::product_bar::BarNetworkStatus::derive(&p);
        assert!(status.available);
        assert_eq!(status.text, "Cartier50");
        assert_eq!(status.glyph, "▄");
    }
}
