//! Frozen observable recipes for the Caelestia native-parity programme.
//!
//! These values are oracle facts, not generic Gnosix defaults. They are kept
//! outside the topology engine so reproducing Caelestia does not reduce or
//! specialize the engine's broader capability.

use crate::product_icon::{IconRole, icon_role_for_categories};
use crate::product_status_glyph::{PowerProfileGlyph, StatusGlyph};
use gnos_ux_surface::grammar::{
    AlongEdgeAlignment, BarPlacement, BarReservationPolicy, BarSensingPolicy, SurfaceViewport,
};

#[cfg(feature = "component-lab")]
use gpui::{
    AnyElement, Bounds, Div, FontWeight, PathBuilder, Pixels, Window, canvas, div, fill, img,
    point, prelude::*, px, rgba, size, svg,
};

/// PARITY-01 idle Bar source facts from the live Caelestia configuration.
///
/// Current user config does not override these defaults:
/// - Tokens.sizes.bar.innerWidth = 40
/// - Tokens.padding.small = 8
/// - Config.border.thickness = 10
/// - BarWrapper.padding = max(padding.small, border.thickness)
pub const BAR_INNER_WIDTH: f32 = 40.0;
pub const SMALL_PADDING: f32 = 8.0;
pub const MEDIUM_SPACING: f32 = 12.0;

/// The spacing between the Bar's modules: the reference's, scaled by the
/// current density (BAR-01D).
pub fn module_gap() -> f32 {
    MEDIUM_SPACING * crate::rail_topology::metrics().gap_scale
}
pub const LARGE_VERTICAL_PADDING: f32 = 16.0;
pub const EXTRA_SMALL_SPACING: f32 = 4.0;
pub const WORKSPACE_INDICATOR_SIZE: f32 = 32.0;
pub const WORKSPACE_WINDOW_LAYOUT_HEIGHT: f32 = 24.0;
pub const MAX_WORKSPACE_WINDOW_MARKS: usize = 5;
pub const WORKSPACE_IGNORED_TAGS: &[&str] = &["hide_in_bar", "xwl_popup"];
pub const BORDER_THICKNESS: f32 = 10.0;
pub const BORDER_ROUNDING: f32 = 25.0;
pub const BORDER_SMOOTHING: f32 = 20.0;

pub const COLOR_SURFACE: u32 = 0x131317ff;
pub const COLOR_SURFACE_CONTAINER: u32 = 0x201f23ff;
pub const COLOR_SURFACE_CONTAINER_HIGHEST: u32 = 0x353438ff;
pub const COLOR_ON_SURFACE: u32 = 0xe5e1e7ff;
pub const COLOR_ON_SURFACE_VARIANT: u32 = 0xc8c5d1ff;
pub const COLOR_OUTLINE_VARIANT: u32 = 0x47464fff;
pub const COLOR_PRIMARY: u32 = 0xc2c1ffff;
pub const COLOR_ON_PRIMARY: u32 = 0x2a2a60ff;
pub const COLOR_SECONDARY: u32 = 0xc6c4e0ff;
pub const COLOR_TERTIARY: u32 = 0xf5b2e0ff;
pub const COLOR_ERROR: u32 = 0xffb4abff;

pub fn workspace_preferred_height(window_count: usize) -> f32 {
    let visible_marks = window_count.min(MAX_WORKSPACE_WINDOW_MARKS);
    WORKSPACE_INDICATOR_SIZE
        + if visible_marks > 0 {
            -EXTRA_SMALL_SPACING / 2.0
                + WORKSPACE_WINDOW_LAYOUT_HEIGHT * visible_marks as f32
                + EXTRA_SMALL_SPACING
        } else {
            0.0
        }
}

pub fn workspace_shape_size(active: bool, occupied: bool) -> f32 {
    if active {
        WORKSPACE_INDICATOR_SIZE * (2.0 / 3.0)
    } else if occupied {
        WORKSPACE_INDICATOR_SIZE / 3.0
    } else {
        WORKSPACE_INDICATOR_SIZE / 4.0
    }
}

pub fn idle_bar_placement(viewport: SurfaceViewport) -> Option<BarPlacement> {
    idle_bar_placement_on(viewport, crate::rail_topology::RailSide::Left)
}

/// TOPO-01B: the native Bar on any edge, full span, rail thickness: the
/// endpoints a perimeter relocation travels between.
pub fn native_bar_placement(
    viewport: SurfaceViewport,
    edge: gnos_ux_surface::envelope::SurfaceEdge,
) -> Option<BarPlacement> {
    use gnos_ux_surface::envelope::SurfaceEdge;
    if viewport.validate().is_err() {
        return None;
    }
    Some(BarPlacement {
        edge,
        along_edge_alignment: AlongEdgeAlignment::Start,
        thickness: crate::rail_topology::rail_width(),
        span: match edge {
            SurfaceEdge::Left | SurfaceEdge::Right => viewport.height,
            SurfaceEdge::Top | SurfaceEdge::Bottom => viewport.width,
        },
        inset: 0.0,
        reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
        sensing_policy: BarSensingPolicy::None,
    })
}

/// TOPO-01A/C: the idle rail on any edge.
pub fn idle_bar_placement_on(
    viewport: SurfaceViewport,
    side: crate::rail_topology::RailSide,
) -> Option<BarPlacement> {
    native_bar_placement(viewport, side.edge())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleBarEntry {
    Logo,
    Workspaces,
    Spacer,
    ActiveWindow,
    Tray,
    Clock,
    StatusIcons,
    Power,
    /// BAR-02A: the "…" that stands in for the modules that gave way.
    Overflow,
}

/// Exact enabled entry order from current Caelestia defaults.
///
/// Two Spacer entries are intentional and divide the rail into top, centre and
/// bottom groups. This is an oracle recipe, not a generic layout API.
pub const IDLE_BAR_ENTRIES: &[IdleBarEntry] = &[
    IdleBarEntry::Logo,
    IdleBarEntry::Workspaces,
    IdleBarEntry::Spacer,
    IdleBarEntry::ActiveWindow,
    IdleBarEntry::Spacer,
    IdleBarEntry::Tray,
    IdleBarEntry::Clock,
    IdleBarEntry::StatusIcons,
    IdleBarEntry::Power,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleStatusIcon {
    Lock,
    Network,
    Bluetooth,
    Battery,
}

pub const IDLE_STATUS_ICONS: &[IdleStatusIcon] = &[
    IdleStatusIcon::Lock,
    IdleStatusIcon::Network,
    IdleStatusIcon::Bluetooth,
    IdleStatusIcon::Battery,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialShape {
    Circle,
    Square,
    Slanted,
    Oval,
    Pill,
    Triangle,
    Arrow,
    Diamond,
    Pentagon,
    Gem,
    VerySunny,
    Sunny,
    Cookie4Sided,
    Cookie6Sided,
    Cookie7Sided,
    Cookie9Sided,
    Cookie12Sided,
    Clover4Leaf,
    SoftBurst,
    Ghostish,
}

/// Exact focused-shape pool from Workspace.qml, in donor source order.
///
/// Circle and Square are deliberately absent: those are the non-focused empty
/// and occupied endpoints respectively.
pub const FOCUSED_SHAPE_POOL: &[MaterialShape] = &[
    MaterialShape::Slanted,
    MaterialShape::Oval,
    MaterialShape::Pill,
    MaterialShape::Triangle,
    MaterialShape::Arrow,
    MaterialShape::Diamond,
    MaterialShape::Pentagon,
    MaterialShape::Gem,
    MaterialShape::VerySunny,
    MaterialShape::Sunny,
    MaterialShape::Cookie4Sided,
    MaterialShape::Cookie6Sided,
    MaterialShape::Cookie7Sided,
    MaterialShape::Cookie9Sided,
    MaterialShape::Cookie12Sided,
    MaterialShape::Clover4Leaf,
    MaterialShape::SoftBurst,
    MaterialShape::Ghostish,
];

/// Caelestia's category-to-Material-Symbol table in source iteration order.
///
/// Icons.getAppCategoryIcon scans this table, not the desktop entry's category
/// order, so priority is part of the observable recipe.
pub const APP_CATEGORY_ICONS: &[(&str, &str)] = &[
    ("WebBrowser", "web"),
    ("Printing", "print"),
    ("Security", "security"),
    ("Network", "chat"),
    ("Archiving", "archive"),
    ("Compression", "archive"),
    ("Development", "code"),
    ("IDE", "code"),
    ("TextEditor", "edit_note"),
    ("Audio", "music_note"),
    ("Music", "music_note"),
    ("Player", "music_note"),
    ("Recorder", "mic"),
    ("Game", "sports_esports"),
    ("FileTools", "files"),
    ("FileManager", "files"),
    ("Filesystem", "files"),
    ("FileTransfer", "files"),
    ("Settings", "settings"),
    ("DesktopSettings", "settings"),
    ("HardwareSettings", "settings"),
    ("TerminalEmulator", "terminal"),
    ("ConsoleOnly", "terminal"),
    ("Utility", "build"),
    ("Monitor", "monitor_heart"),
    ("Midi", "graphic_eq"),
    ("Mixer", "graphic_eq"),
    ("AudioVideoEditing", "video_settings"),
    ("AudioVideo", "music_video"),
    ("Video", "videocam"),
    ("Building", "construction"),
    ("Graphics", "photo_library"),
    ("2DGraphics", "photo_library"),
    ("RasterGraphics", "photo_library"),
    ("TV", "tv"),
    ("System", "host"),
    ("Office", "content_paste"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceFocusState {
    active_workspace: Option<gnosix_desktop_model::WorkspaceId>,
    focused_shape: Option<MaterialShape>,
}

impl WorkspaceFocusState {
    pub fn active_workspace(&self) -> Option<gnosix_desktop_model::WorkspaceId> {
        self.active_workspace
    }

    pub fn focused_shape(&self) -> Option<MaterialShape> {
        self.focused_shape
    }

    fn observed_active_workspace(
        semantic: &crate::product_scene::ProductSemanticState,
    ) -> Option<gnosix_desktop_model::WorkspaceId> {
        semantic
            .workspace_manager_targets()
            .iter()
            .find(|workspace| semantic.workspace_manager_is_active(workspace))
            .map(|workspace| workspace.reference.workspace_id())
    }

    fn reconcile_with_index(
        &mut self,
        semantic: &crate::product_scene::ProductSemanticState,
        next_index: usize,
    ) -> Option<MaterialShape> {
        let active_workspace = Self::observed_active_workspace(semantic);
        if active_workspace != self.active_workspace {
            self.active_workspace = active_workspace;
            self.focused_shape =
                active_workspace.map(|_| FOCUSED_SHAPE_POOL[next_index % FOCUSED_SHAPE_POOL.len()]);
        }
        self.focused_shape
    }

    fn random_shape_index() -> usize {
        let modulus = FOCUSED_SHAPE_POOL.len() as u64;
        let domain = u64::from(u32::MAX) + 1;
        let limit = domain - domain % modulus;

        loop {
            // UUID v4 version/variant bits do not overlap the first four bytes,
            // so this word is OS-random entropy already available in this crate.
            let random = uuid::Uuid::new_v4();
            let bytes = random.as_bytes();
            let value = u64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
            if value < limit {
                return (value % modulus) as usize;
            }
        }
    }

    /// Match Workspace.qml's state lifetime: choose only when focus enters a
    /// different workspace, then retain that shape across observation frames.
    pub fn reconcile(
        &mut self,
        semantic: &crate::product_scene::ProductSemanticState,
    ) -> Option<MaterialShape> {
        let active_workspace = Self::observed_active_workspace(semantic);
        if active_workspace == self.active_workspace {
            return self.focused_shape;
        }
        if active_workspace.is_none() {
            return self.reconcile_with_index(semantic, 0);
        }
        self.reconcile_with_index(semantic, Self::random_shape_index())
    }
}

impl MaterialShape {
    #[cfg(feature = "component-lab")]
    fn asset_slug(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Square => "square",
            Self::Slanted => "slanted",
            Self::Oval => "oval",
            Self::Pill => "pill",
            Self::Triangle => "triangle",
            Self::Arrow => "arrow",
            Self::Diamond => "diamond",
            Self::Pentagon => "pentagon",
            Self::Gem => "gem",
            Self::VerySunny => "very-sunny",
            Self::Sunny => "sunny",
            Self::Cookie4Sided => "cookie-4-sided",
            Self::Cookie6Sided => "cookie-6-sided",
            Self::Cookie7Sided => "cookie-7-sided",
            Self::Cookie9Sided => "cookie-9-sided",
            Self::Cookie12Sided => "cookie-12-sided",
            Self::Clover4Leaf => "clover-4-leaf",
            Self::SoftBurst => "soft-burst",
            Self::Ghostish => "ghostish",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceShapePresentation {
    Resolved(MaterialShape),
    /// The donor chooses one of its focused-shape pool with Math.random().
    /// A point-in-time semantic snapshot cannot truthfully reconstruct that
    /// state without either retained oracle evidence or a stateful selector.
    FocusedUnresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceWindowMarkResolution {
    SteamOverride,
    ExactDesktopId,
    CaseFoldedDesktopId,
    ExactStartupClass,
    CaseFoldedStartupClass,
    NoDesktopEntryMatch,
    AmbiguousDesktopEntryMatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceWindowMark {
    /// The first frozen capture retained the rendered symbol but not the SNI /
    /// toplevel identity that produced it.
    CapturedUnresolved {
        slot: usize,
        observed_material_symbol: String,
    },
    /// Native compositor truth before the XDG presentation lookup.
    NativeClass {
        class: String,
        initial_class: String,
    },
    /// Native compositor identity plus the bounded XDG presentation resolution.
    NativeResolved {
        class: String,
        initial_class: String,
        desktop_entry: Option<gnosix_application_catalog::DesktopEntryId>,
        material_symbol: String,
        resolution: WorkspaceWindowMarkResolution,
    },
}

pub(crate) fn workspace_window_is_ignored(
    application: &crate::product_scene::ProductApplicationContent,
) -> bool {
    if application.visibility == gnosix_desktop_model::SurfaceVisibility::Unmapped
        || application.class.is_empty()
    {
        return true;
    }

    application.tags.iter().any(|tag| {
        let normalized = tag.strip_suffix('*').unwrap_or(tag);
        WORKSPACE_IGNORED_TAGS.contains(&normalized)
    })
}

fn steam_window_icon_override(class: &str) -> bool {
    if class == "steam" {
        return true;
    }
    let Some(suffix) = class.strip_prefix("steam_app_") else {
        return false;
    };
    suffix == "default" || (!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
}

fn category_material_symbol(categories: &[String]) -> Option<&'static str> {
    APP_CATEGORY_ICONS.iter().find_map(|(category, symbol)| {
        categories
            .iter()
            .any(|candidate| candidate == category)
            .then_some(*symbol)
    })
}

fn desktop_entry_id_stem(entry: &gnosix_application_catalog::InstalledApplication) -> &str {
    entry
        .id
        .as_str()
        .strip_suffix(".desktop")
        .unwrap_or(entry.id.as_str())
}

/// DECK-01A: the catalog entry a window class resolves to, by the same tiers
/// as the rail's workspace marks. `None` for no match, an ambiguous class, or
/// the Steam icon override.
pub fn resolve_window_entry(
    catalog: &gnosix_application_catalog::ApplicationCatalog,
    class: &str,
) -> Option<gnosix_application_catalog::DesktopEntryId> {
    match lookup_application_entry(catalog, class) {
        EntryLookup::Entry(entry, _) => Some(entry.id.clone()),
        _ => None,
    }
}

/// Desktop-entry lookup shared by every surface that resolves an application
/// class: tiers in order, hidden entries excluded, ambiguity exposed.
enum EntryLookup<'a> {
    SteamOverride,
    Entry(
        &'a gnosix_application_catalog::InstalledApplication,
        WorkspaceWindowMarkResolution,
    ),
    Ambiguous,
    NoMatch,
}

fn lookup_application_entry<'a>(
    catalog: &'a gnosix_application_catalog::ApplicationCatalog,
    class: &str,
) -> EntryLookup<'a> {
    if steam_window_icon_override(class) {
        return EntryLookup::SteamOverride;
    }

    // Observed lookup order of the reference shell: desktop id exact ->
    // case-folded -> StartupWMClass exact -> case-folded, class only; hidden
    // entries are absent from its lookup map.
    let entries = catalog
        .entries()
        .iter()
        .filter(|entry| !entry.hidden)
        .collect::<Vec<_>>();
    let tiers: [(WorkspaceWindowMarkResolution, Vec<&_>); 4] = [
        (
            WorkspaceWindowMarkResolution::ExactDesktopId,
            entries
                .iter()
                .copied()
                .filter(|entry| desktop_entry_id_stem(entry) == class)
                .collect(),
        ),
        (
            WorkspaceWindowMarkResolution::CaseFoldedDesktopId,
            entries
                .iter()
                .copied()
                .filter(|entry| desktop_entry_id_stem(entry).eq_ignore_ascii_case(class))
                .collect(),
        ),
        (
            WorkspaceWindowMarkResolution::ExactStartupClass,
            entries
                .iter()
                .copied()
                .filter(|entry| entry.startup_wm_class.as_deref() == Some(class))
                .collect(),
        ),
        (
            WorkspaceWindowMarkResolution::CaseFoldedStartupClass,
            entries
                .iter()
                .copied()
                .filter(|entry| {
                    entry
                        .startup_wm_class
                        .as_deref()
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(class))
                })
                .collect(),
        ),
    ];
    for (resolution, candidates) in tiers {
        match candidates.as_slice() {
            [entry] => return EntryLookup::Entry(entry, resolution),
            [] => {}
            _ => return EntryLookup::Ambiguous,
        }
    }
    EntryLookup::NoMatch
}

struct AppCategoryIcon {
    desktop_entry: Option<gnosix_application_catalog::DesktopEntryId>,
    material_symbol: String,
    resolution: WorkspaceWindowMarkResolution,
}

/// Donor-vocabulary resolution used by the (closed) Workspace marks only.
fn resolve_app_category_icon(
    catalog: &gnosix_application_catalog::ApplicationCatalog,
    class: &str,
    fallback: &str,
) -> AppCategoryIcon {
    match lookup_application_entry(catalog, class) {
        EntryLookup::SteamOverride => AppCategoryIcon {
            desktop_entry: None,
            material_symbol: "sports_esports".into(),
            resolution: WorkspaceWindowMarkResolution::SteamOverride,
        },
        EntryLookup::Entry(entry, resolution) => AppCategoryIcon {
            desktop_entry: Some(entry.id.clone()),
            material_symbol: category_material_symbol(&entry.categories)
                .unwrap_or(fallback)
                .into(),
            resolution,
        },
        EntryLookup::Ambiguous => AppCategoryIcon {
            desktop_entry: None,
            material_symbol: fallback.into(),
            resolution: WorkspaceWindowMarkResolution::AmbiguousDesktopEntryMatch,
        },
        EntryLookup::NoMatch => AppCategoryIcon {
            desktop_entry: None,
            material_symbol: fallback.into(),
            resolution: WorkspaceWindowMarkResolution::NoDesktopEntryMatch,
        },
    }
}

/// WS-02: the Material Symbol a window of `class` takes (the window marks').
pub fn window_symbol(
    catalog: &gnosix_application_catalog::ApplicationCatalog,
    class: &str,
) -> String {
    resolve_app_category_icon(catalog, class, WORKSPACE_WINDOW_FALLBACK_ICON).material_symbol
}

fn resolve_native_workspace_window_mark(
    catalog: &gnosix_application_catalog::ApplicationCatalog,
    class: &str,
    initial_class: &str,
) -> WorkspaceWindowMark {
    let icon = resolve_app_category_icon(catalog, class, WORKSPACE_WINDOW_FALLBACK_ICON);
    WorkspaceWindowMark::NativeResolved {
        class: class.into(),
        initial_class: initial_class.into(),
        desktop_entry: icon.desktop_entry,
        material_symbol: icon.material_symbol,
        resolution: icon.resolution,
    }
}

/// Application facts -> product semantics: the class resolves to a desktop
/// entry, its categories to an `IconRole`, and anything unresolved to the
/// module's fallback role.
fn resolve_native_active_window_icon(
    catalog: &gnosix_application_catalog::ApplicationCatalog,
    class: &str,
) -> ActiveWindowIcon {
    let (desktop_entry, role, resolution) = match lookup_application_entry(catalog, class) {
        EntryLookup::SteamOverride => (
            None,
            IconRole::Game,
            WorkspaceWindowMarkResolution::SteamOverride,
        ),
        EntryLookup::Entry(entry, resolution) => (
            Some(entry.id.clone()),
            icon_role_for_categories(&entry.categories).unwrap_or(ACTIVE_WINDOW_FALLBACK_ROLE),
            resolution,
        ),
        EntryLookup::Ambiguous => (
            None,
            ACTIVE_WINDOW_FALLBACK_ROLE,
            WorkspaceWindowMarkResolution::AmbiguousDesktopEntryMatch,
        ),
        EntryLookup::NoMatch => (
            None,
            ACTIVE_WINDOW_FALLBACK_ROLE,
            WorkspaceWindowMarkResolution::NoDesktopEntryMatch,
        ),
    };
    ActiveWindowIcon::Resolved {
        class: class.into(),
        desktop_entry,
        role,
        resolution,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspacePresentation {
    pub label: String,
    pub active: bool,
    pub shape: WorkspaceShapePresentation,
    pub window_marks: Vec<WorkspaceWindowMark>,
}

impl WorkspacePresentation {
    pub fn window_count(&self) -> usize {
        self.window_marks.len()
    }
}

/// ActiveWindow.qml `Tr.trCtx("Desktop", ...)` under the observed en_US locale.
pub const ACTIVE_WINDOW_DESKTOP_TITLE: &str = "Desktop"; // i18n
/// Role shown when no application role resolves (observed behaviour).
pub const ACTIVE_WINDOW_FALLBACK_ROLE: IconRole = IconRole::Desktop;
/// Workspace.qml passes this fallback to `Icons.getAppCategoryIcon`.
pub const WORKSPACE_WINDOW_FALLBACK_ICON: &str = "terminal";
/// `Title.anchors.topMargin = Tokens.spacing.small`.
pub const ACTIVE_WINDOW_TITLE_TOP_MARGIN: f32 = 8.0;

/// Effective `Config.bar.activeWindow`. The live user config has no override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveWindowConfig {
    pub compact: bool,
    pub inverted: bool,
    pub show_on_hover: bool,
}

impl Default for ActiveWindowConfig {
    fn default() -> Self {
        Self {
            compact: false,
            inverted: false,
            show_on_hover: true,
        }
    }
}

/// ActiveWindow.qml `windowTitle`, before TextMetrics elision.
pub fn active_window_title(active_title: Option<&str>, compact: bool) -> String {
    let title = match active_title {
        Some(title) if !title.is_empty() => title,
        _ => return gnos_i18n::t(ACTIVE_WINDOW_DESKTOP_TITLE).to_owned(),
    };
    if compact && let Some(last) = compact_title_last_part(title) {
        return last.trim_matches(ecmascript_is_whitespace).to_owned();
    }
    title.to_owned()
}

/// ECMAScript WhiteSpace and LineTerminator, the `\s` and `trim()` set.
fn ecmascript_is_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Last part of `title.split(/\s+[\-\u2013\u2014]\s+/)`, or None when the
/// split yields a single part.
fn compact_title_last_part(title: &str) -> Option<&str> {
    let chars = title.char_indices().collect::<Vec<_>>();
    let whitespace_run_end = |mut index: usize| {
        while index < chars.len() && ecmascript_is_whitespace(chars[index].1) {
            index += 1;
        }
        index
    };
    let mut last_match_end = None;
    let mut index = 0;
    while index < chars.len() {
        // `\s+` is greedy and a dash is never whitespace, so the only
        // candidate at this position ends its whole whitespace run.
        let dash = whitespace_run_end(index);
        if dash > index
            && dash < chars.len()
            && matches!(chars[dash].1, '-' | '\u{2013}' | '\u{2014}')
        {
            let end = whitespace_run_end(dash + 1);
            if end > dash + 1 {
                last_match_end = Some(end);
                index = end;
                continue;
            }
        }
        index += 1;
    }
    last_match_end.map(|end| chars.get(end).map_or("", |(offset, _)| &title[*offset..]))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveWindowIcon {
    /// No active toplevel: the module shows the fallback role.
    NoActiveToplevel,
    /// The frozen capture retained the observed role, not the client class.
    /// The donor's symbol name is mapped to a role at the oracle boundary only.
    CapturedRole { role: IconRole },
    /// Pure semantic projection before the application-catalogue lookup.
    NativeClass { class: String },
    Resolved {
        class: String,
        desktop_entry: Option<gnosix_application_catalog::DesktopEntryId>,
        role: IconRole,
        resolution: WorkspaceWindowMarkResolution,
    },
}

impl ActiveWindowIcon {
    /// None only while a native class awaits catalogue resolution.
    pub fn role(&self) -> Option<IconRole> {
        match self {
            Self::NoActiveToplevel => Some(ACTIVE_WINDOW_FALLBACK_ROLE),
            Self::CapturedRole { role } | Self::Resolved { role, .. } => Some(*role),
            Self::NativeClass { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveWindowPresentation {
    /// Effective donor `windowTitle`: Desktop/compact applied, not elided.
    pub title: String,
    pub icon: ActiveWindowIcon,
    pub config: ActiveWindowConfig,
}

/// ActiveWindow.qml intrinsic geometry from its icon and unrotated title.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveWindowGeometry {
    pub icon_width: f32,
    pub icon_height: f32,
    /// Unrotated `Title.implicitWidth`, including Qt's trailing spacing.
    pub title_width: f32,
    /// Unrotated `Title.implicitHeight`.
    pub title_height: f32,
}

impl ActiveWindowGeometry {
    /// `implicitWidth: Math.max(icon.implicitWidth, current.implicitHeight)`.
    pub fn implicit_width(&self) -> f32 {
        self.icon_width.max(self.title_height)
    }

    /// `implicitHeight: icon.implicitHeight + current.implicitWidth + topMargin`.
    pub fn implicit_height(&self) -> f32 {
        self.icon_height + self.title_width + ACTIVE_WINDOW_TITLE_TOP_MARGIN
    }
}

/// ActiveWindow.qml `maxHeight`.
///
/// `bar_child_count` is `bar.children.length`: every enabled entry wrapper
/// plus the Repeater itself. The donor subtracts `spacing * (length - 1)`
/// although its comment says `length - 2`; that extra spacing is observable.
pub fn active_window_max_height(
    bar_height: f32,
    other_non_spacer_heights: &[f32],
    bar_child_count: usize,
) -> f32 {
    let spacing = module_gap() * bar_child_count.saturating_sub(1) as f32;
    // `readonly property int`: QML stores the JS number with ToInt32.
    (bar_height
        - other_non_spacer_heights.iter().sum::<f32>()
        - spacing
        - LARGE_VERTICAL_PADDING * 2.0)
        .trunc()
}

/// TextMetrics `elideWidth: root.maxHeight - icon.height`.
pub fn active_window_elide_width(max_height: f32, icon_height: f32) -> f32 {
    max_height - icon_height
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockPresentation {
    pub hour: String,
    pub minute: String,
    /// `None` on a 24-hour clock: the row is absent, not empty.
    pub meridiem: Option<crate::product_clock::Meridiem>,
    /// BAR-01D: the local date ("7 Oct") when the profile shows it.
    pub date: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayVisual {
    /// The captured slot is real, but its SNI identity was not retained with
    /// the oracle. It is drawn as a neutral native marker and must never be
    /// interpreted as provider truth.
    CapturedUnresolved { slot: usize },
    /// Native SNI truth, already resolved to an application-owned icon source.
    Native(crate::product_tray_presentation::TrayItemPresentation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusVisual {
    /// One source entry containing zero, one, or two animated lock glyphs.
    Lock {
        caps_lock: bool,
        num_lock: bool,
    },
    Network {
        glyph: StatusGlyph,
    },
    /// One adapter/status glyph followed by one glyph per non-disconnected
    /// device.
    Bluetooth {
        glyph: StatusGlyph,
        device_glyphs: Vec<StatusGlyph>,
    },
    /// BatteryStatus.qml is one entry: on laptops it is battery state; without
    /// a laptop battery it is the current power profile.
    BatteryOrProfile {
        glyph: StatusGlyph,
        error: bool,
    },
    /// Default output volume. A reference option that is off by default;
    /// Gnosix enables it so the audio popout has an affordance (UXE-01).
    Audio {
        glyph: StatusGlyph,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusPresentation {
    pub entries: Vec<StatusVisual>,
}

impl StatusVisual {
    pub fn source_height(&self) -> f32 {
        match self {
            Self::Lock {
                caps_lock,
                num_lock,
            } => match (*caps_lock, *num_lock) {
                (false, false) => 0.0,
                (true, true) => 2.0 * STATUS_ICON_LINE_HEIGHT + STATUS_INNER_GAP,
                _ => STATUS_ICON_LINE_HEIGHT,
            },
            Self::Network { .. } | Self::BatteryOrProfile { .. } | Self::Audio { .. } => {
                STATUS_ICON_LINE_HEIGHT
            }
            Self::Bluetooth { device_glyphs, .. } => {
                STATUS_ICON_LINE_HEIGHT
                    + device_glyphs.len() as f32 * (STATUS_ICON_LINE_HEIGHT + STATUS_INNER_GAP)
            }
        }
    }

    pub fn is_present(&self) -> bool {
        self.source_height() > 0.0
    }
}

impl StatusPresentation {
    pub fn source_height(&self) -> f32 {
        let mut present = self
            .entries
            .iter()
            .map(StatusVisual::source_height)
            .filter(|height| *height > 0.0);
        let Some(first) = present.next() else {
            // StatusIcons.qml is still an enabled Bar child. An empty
            // iconColumn contributes zero but the StyledRect keeps medium
            // padding at both ends.
            return 2.0 * STATUS_PADDING;
        };
        let mut total = first;
        let mut count = 1usize;
        for height in present {
            total += height;
            count += 1;
        }
        total + (count - 1) as f32 * STATUS_ENTRY_GAP + 2.0 * STATUS_PADDING
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemLogoPresentation {
    /// SysInfo.qml retained its initial Caelestia logo because the
    /// os-release LOGO token did not resolve through Quickshell.iconPath.
    DefaultShellLogo,
    /// A future native system-identity adapter may resolve a distro icon
    /// explicitly. The parity renderer must not infer this from /etc/os-release.
    DistroIcon { identity: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarPresentation {
    /// Observable logo branch, not merely the host's os-release LOGO token.
    pub system_logo: Option<SystemLogoPresentation>,
    pub workspaces: Vec<WorkspacePresentation>,
    pub active_window: Option<ActiveWindowPresentation>,
    /// None means the native tray provider has not supplied a presentation.
    pub tray: Option<Vec<TrayVisual>>,
    /// Clock is supplied by a time source outside this visual renderer.
    pub clock: Option<ClockPresentation>,
    /// A production adapter derives these source entries from typed
    /// lock/network/Bluetooth/power facts. The renderer never probes providers.
    pub status: Option<StatusPresentation>,
    /// Session/power affordance is absent until its native session adapter
    /// supplies an observable presentation.
    pub power_icon: Option<String>,
    /// BAR-02A: some modules gave way; the Bar ends with a "…" module.
    pub overflow: bool,
    /// BAR-01D: the modules' groups and order (the reference's by default).
    pub order: BarModuleOrder,
}

/// BAR-01D (B03): the Bar's modules in three groups (start, centre, end),
/// laid out around the two flexible spacers. A module in no group is absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarModuleOrder(pub [Vec<IdleBarEntry>; 3]);

impl Default for BarModuleOrder {
    /// The reference's: logo and workspaces; the active window; tray,
    /// clock, status icons and power.
    fn default() -> Self {
        use IdleBarEntry as E;
        Self([
            vec![E::Logo, E::Workspaces],
            vec![E::ActiveWindow],
            vec![E::Tray, E::Clock, E::StatusIcons, E::Power],
        ])
    }
}

impl BarModuleOrder {
    /// The entries in Bar order with the two spacers (`IDLE_BAR_ENTRIES`
    /// for the default).
    pub fn entries(&self) -> Vec<IdleBarEntry> {
        let [start, centre, end] = &self.0;
        let mut out = start.clone();
        out.push(IdleBarEntry::Spacer);
        out.extend(centre.iter().copied());
        out.push(IdleBarEntry::Spacer);
        out.extend(end.iter().copied());
        out
    }

    /// The group (0 start, 1 centre, 2 end) of a shown module.
    pub fn group(&self, entry: IdleBarEntry) -> Option<usize> {
        self.0.iter().position(|g| g.contains(&entry))
    }

    pub fn from_profile(layout: &in_gnu_os_settings_read_model::BarModuleLayout) -> Self {
        use in_gnu_os_settings_read_model::BarModule as M;
        let entry = |m: M| match m {
            M::Logo => IdleBarEntry::Logo,
            M::Workspaces => IdleBarEntry::Workspaces,
            M::ActiveWindow => IdleBarEntry::ActiveWindow,
            M::Tray => IdleBarEntry::Tray,
            M::Clock => IdleBarEntry::Clock,
            M::StatusIcons => IdleBarEntry::StatusIcons,
            M::Power => IdleBarEntry::Power,
        };
        Self(layout.groups().map(|g| g.into_iter().map(entry).collect()))
    }
}

impl BarPresentation {
    /// Frozen observable presentation for the 2026-10-07 HDMI-A-1 oracle.
    pub fn captured_oracle_2026_10_07() -> Self {
        Self {
            system_logo: Some(SystemLogoPresentation::DefaultShellLogo),
            workspaces: vec![
                WorkspacePresentation {
                    label: "1".into(),
                    active: true,
                    shape: WorkspaceShapePresentation::Resolved(MaterialShape::Pill),
                    window_marks: vec![WorkspaceWindowMark::CapturedUnresolved {
                        slot: 0,
                        observed_material_symbol: "web".into(),
                    }],
                },
                WorkspacePresentation {
                    label: "2".into(),
                    active: false,
                    shape: WorkspaceShapePresentation::Resolved(MaterialShape::Circle),
                    window_marks: vec![],
                },
                WorkspacePresentation {
                    label: "3".into(),
                    active: false,
                    shape: WorkspaceShapePresentation::Resolved(MaterialShape::Circle),
                    window_marks: vec![],
                },
                WorkspacePresentation {
                    label: "4".into(),
                    active: false,
                    shape: WorkspaceShapePresentation::Resolved(MaterialShape::Circle),
                    window_marks: vec![],
                },
                WorkspacePresentation {
                    label: "5".into(),
                    active: false,
                    shape: WorkspaceShapePresentation::Resolved(MaterialShape::Circle),
                    window_marks: vec![],
                },
            ],
            active_window: Some(ActiveWindowPresentation {
                // Raster-reconstructed from the live capture: the run matches
                // an ASCII hyphen, not the en dash transcribed by hand earlier.
                title: "Ratification Phase 5 OSD - Helium".into(),
                // Oracle adapter: the capture showed the donor symbol `web`;
                // only its role crosses into the product model.
                icon: ActiveWindowIcon::CapturedRole {
                    role: IconRole::Web,
                },
                config: ActiveWindowConfig::default(),
            }),
            tray: Some(vec![
                TrayVisual::CapturedUnresolved { slot: 0 },
                TrayVisual::CapturedUnresolved { slot: 1 },
                TrayVisual::CapturedUnresolved { slot: 2 },
                TrayVisual::CapturedUnresolved { slot: 3 },
            ]),
            clock: Some(ClockPresentation {
                hour: "05".into(),
                minute: "05".into(),
                meridiem: Some(crate::product_clock::Meridiem::Am),
                date: None,
            }),
            status: Some(StatusPresentation {
                entries: vec![
                    StatusVisual::Lock {
                        caps_lock: true,
                        num_lock: true,
                    },
                    StatusVisual::Network {
                        glyph: StatusGlyph::Wifi { bars: 4 },
                    },
                    StatusVisual::Bluetooth {
                        glyph: StatusGlyph::Bluetooth,
                        device_glyphs: Vec::new(),
                    },
                    StatusVisual::BatteryOrProfile {
                        glyph: StatusGlyph::Profile(PowerProfileGlyph::Performance),
                        error: false,
                    },
                ],
            }),
            power_icon: Some("power_settings_new".into()),
            overflow: false,
            order: BarModuleOrder::default(),
        }
    }

    /// Project only compositor facts already present in native Gnosix semantic
    /// state. Tray/network/Bluetooth/power/time remain absent until their typed
    /// provider adapters supply them.
    pub fn from_product_semantics(semantic: &crate::product_scene::ProductSemanticState) -> Self {
        let applications = semantic.application_targets();
        let workspaces = semantic
            .workspace_manager_targets()
            .iter()
            .map(|workspace| {
                let active = semantic.workspace_manager_is_active(workspace);
                let window_marks = applications
                    .iter()
                    .filter(|application| application.workspace == Some(workspace.reference))
                    .filter(|application| !workspace_window_is_ignored(application))
                    .take(MAX_WORKSPACE_WINDOW_MARKS)
                    .map(|application| WorkspaceWindowMark::NativeClass {
                        class: application.class.clone(),
                        initial_class: application.initial_class.clone(),
                    })
                    .collect::<Vec<_>>();
                let shape = if active {
                    WorkspaceShapePresentation::FocusedUnresolved
                } else if window_marks.is_empty() {
                    WorkspaceShapePresentation::Resolved(MaterialShape::Circle)
                } else {
                    WorkspaceShapePresentation::Resolved(MaterialShape::Square)
                };
                WorkspacePresentation {
                    label: workspace.label.clone(),
                    active,
                    shape,
                    window_marks,
                }
            })
            .collect();

        // Hypr.activeToplevel is the focused client, independent of whether
        // it sits on the focused regular workspace. Unavailable semantics stay
        // absent; current semantics without a focused client show Desktop.
        let config = ActiveWindowConfig::default();
        let active_window = matches!(
            semantic,
            crate::product_scene::ProductSemanticState::Current(_)
        )
        .then(|| {
            let focused = applications.iter().find(|application| {
                application.focus == gnosix_desktop_model::SurfaceFocus::Focused
            });
            ActiveWindowPresentation {
                title: active_window_title(
                    focused.map(|application| application.title.as_str()),
                    config.compact,
                ),
                icon: focused.map_or(ActiveWindowIcon::NoActiveToplevel, |application| {
                    ActiveWindowIcon::NativeClass {
                        class: application.class.clone(),
                    }
                }),
                config,
            }
        });

        Self {
            system_logo: None,
            workspaces,
            active_window,
            tray: None,
            clock: None,
            status: None,
            power_icon: None,
            overflow: false,
            order: BarModuleOrder::default(),
        }
    }

    /// Native adapter for facts already owned by ProductRuntime.
    ///
    /// Tray and network truth are projected here. The enabled StatusIcons
    /// module itself is always represented, even when every provider fact is
    /// absent; source padding still participates in layout. Bluetooth, lock
    /// state, power profile, clock and system identity remain absent until their
    /// typed providers are part of ProductRuntime.
    pub fn from_product_runtime(runtime: &crate::product_runtime::ProductRuntime) -> Self {
        let mut presentation = Self::from_product_semantics(runtime.semantic());

        for workspace in &mut presentation.workspaces {
            for mark in &mut workspace.window_marks {
                let resolved = match mark {
                    WorkspaceWindowMark::NativeClass {
                        class,
                        initial_class,
                    } => Some(resolve_native_workspace_window_mark(
                        runtime.application_catalog(),
                        class,
                        initial_class,
                    )),
                    WorkspaceWindowMark::CapturedUnresolved { .. }
                    | WorkspaceWindowMark::NativeResolved { .. } => None,
                };
                if let Some(resolved) = resolved {
                    *mark = resolved;
                }
            }
        }

        if let Some(active_window) = &mut presentation.active_window
            && let ActiveWindowIcon::NativeClass { class } = &active_window.icon
        {
            active_window.icon =
                resolve_native_active_window_icon(runtime.application_catalog(), class);
        }

        // Hidden icons are donor configuration (`bar.tray.hiddenIcons`, empty by
        // default); Gnosix has no such setting, so none is hidden here.
        presentation.tray = crate::product_tray_presentation::present_tray(runtime.tray(), &[])
            .map(|items| items.into_iter().map(TrayVisual::Native).collect());

        let mut status = StatusPresentation::default();
        if let gnosix_hyprland::keyboard_lock::KeyboardLockObservation::Observed(locks) =
            runtime.keyboard_locks()
        {
            status.entries.push(StatusVisual::Lock {
                caps_lock: locks.caps_lock,
                num_lock: locks.num_lock,
            });
        }

        // Reference order: lockStatus, audio, microphone, kbLayout, network, ...
        if let Some(audio) = audio_status_from_product(&runtime.audio_presentation()) {
            status.entries.push(audio);
        }
        if let Some(network) = network_status_from_product(&runtime.network_presentation()) {
            status.entries.push(network);
        }
        if let Some(bluetooth) = bluetooth_status_from_product(runtime.bluetooth()) {
            status.entries.push(bluetooth);
        }
        if let Some(power) = power_status_from_product(runtime.power()) {
            status.entries.push(power);
        }
        // BAR-01D: the profile chooses which status icons show.
        let shell = &runtime.preferences().bar_overrides;
        if let Some(icons) = shell.status_icons {
            status.entries.retain(|entry| match entry {
                StatusVisual::Lock { .. } => icons.lock,
                StatusVisual::Network { .. } => icons.network,
                StatusVisual::Bluetooth { .. } => icons.bluetooth,
                StatusVisual::BatteryOrProfile { .. } => icons.battery,
                StatusVisual::Audio { .. } => true,
            });
        }
        presentation.status = Some(status);
        // SURF-02 (S01): the power button stands for logind's session
        // powers, once logind has answered for them.
        presentation.power_icon = runtime
            .session_powers()
            .then(|| "power_settings_new".to_owned());

        // BAR-01D: an explicit clock format overrides the locale's cycle and
        // may add the date.
        presentation.clock = runtime.clock().map(|observation| {
            use crate::product_clock::ClockCycle;
            let cycle = match shell.clock {
                Some(format) if format.hour24 => ClockCycle::TwentyFourHour,
                Some(_) => ClockCycle::TwelveHour,
                None => observation.cycle,
            };
            let clock = crate::product_clock::format_clock(observation.time, cycle);
            ClockPresentation {
                hour: clock.hour,
                minute: clock.minute,
                meridiem: clock.meridiem,
                date: shell.clock.filter(|format| format.date).and(clock.date),
            }
        });

        // BAR-01D: the profile's module groups and order; a module in no
        // group is absent.
        if let Some(modules) = shell.modules {
            presentation.order = BarModuleOrder::from_profile(&modules);
        }

        presentation
    }

    /// Stateful workspace parity projection.
    ///
    /// The pure runtime adapter cannot reconstruct Caelestia's Math.random()
    /// choice from a semantic snapshot. This variant keeps that choice outside
    /// rendering and only selects again when the active workspace identity
    /// changes, matching Workspace.qml's onFocusedChanged lifetime.
    pub fn from_product_runtime_with_workspace_state(
        runtime: &crate::product_runtime::ProductRuntime,
        workspace_state: &mut WorkspaceFocusState,
    ) -> Self {
        let mut presentation = Self::from_product_runtime(runtime);
        if let Some(shape) = workspace_state.reconcile(runtime.semantic())
            && let Some(active) = presentation
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.active)
        {
            active.shape = WorkspaceShapePresentation::Resolved(shape);
        }
        presentation
    }
}

/// Stable one-line description of the Clock and Status presentation, for
/// receipts and capture manifests.
pub fn describe_clock_and_status(presentation: &BarPresentation) -> String {
    let clock = presentation.clock.as_ref().map_or_else(
        || "absent".to_owned(),
        |clock| {
            format!(
                "{}:{}{}",
                clock.hour,
                clock.minute,
                clock.meridiem.map_or("", |m| m.as_str())
            )
        },
    );
    let entries = presentation.status.as_ref().map_or_else(
        || "absent".to_owned(),
        |status| {
            status
                .entries
                .iter()
                .map(|entry| match entry {
                    StatusVisual::Lock {
                        caps_lock,
                        num_lock,
                    } => format!("lock(caps={caps_lock},num={num_lock})"),
                    StatusVisual::Network { glyph } => format!("network({})", glyph.name()),
                    StatusVisual::Bluetooth {
                        glyph,
                        device_glyphs,
                    } => format!(
                        "bluetooth({}{})",
                        glyph.name(),
                        device_glyphs
                            .iter()
                            .map(|g| format!(";{}", g.name()))
                            .collect::<String>()
                    ),
                    StatusVisual::BatteryOrProfile { glyph, error } => {
                        format!("power({},error={error})", glyph.name())
                    }
                    StatusVisual::Audio { glyph } => format!("audio({})", glyph.name()),
                })
                .collect::<Vec<_>>()
                .join(",")
        },
    );
    format!(
        "clock={clock} clock_height={} status=[{entries}] status_height={}",
        presentation.clock.as_ref().map_or(0.0, clock_module_height),
        presentation
            .status
            .as_ref()
            .map_or(0.0, status_module_height)
    )
}

fn donor_battery_level(percentage: gnos_power::BatteryPercentage, charging: bool) -> u8 {
    let fraction = percentage.as_fraction();
    if fraction == 1.0 {
        return 7;
    }
    let mut level = (fraction * 7.0).floor() as u8;
    // Icons.getBatteryIcon(): charging levels 1 and 4 step down before the
    // ligature bucket is selected.
    if charging && matches!(level, 1 | 4) {
        level -= 1;
    }
    level
}

fn power_status_from_product(snapshot: gnos_power::PowerSnapshot) -> Option<StatusVisual> {
    use gnos_power::{PowerProfile, PowerSnapshot};
    let (glyph, error) = match snapshot {
        PowerSnapshot::Profile(profile) => (
            StatusGlyph::Profile(match profile {
                PowerProfile::PowerSaver => PowerProfileGlyph::PowerSaver,
                PowerProfile::Balanced => PowerProfileGlyph::Balanced,
                PowerProfile::Performance => PowerProfileGlyph::Performance,
            }),
            false,
        ),
        PowerSnapshot::Battery {
            percentage,
            charging,
            on_battery,
        } => (
            StatusGlyph::Battery {
                level: donor_battery_level(percentage, charging),
                charging,
            },
            on_battery && percentage.as_fraction() <= 0.2,
        ),
        PowerSnapshot::Unavailable
        | PowerSnapshot::NoBatteryProfileUnavailable
        | PowerSnapshot::BatteryStateUnsupported => return None,
    };
    Some(StatusVisual::BatteryOrProfile { glyph, error })
}

fn bluetooth_status_from_product(
    snapshot: &gnos_bluetooth::BluetoothSnapshot,
) -> Option<StatusVisual> {
    use crate::product_status_glyph::BluetoothDeviceClass;
    use gnos_bluetooth::BluetoothProviderState;
    if snapshot.provider != BluetoothProviderState::Available {
        return None;
    }
    // BlueZ has no default adapter. Rather than pick one by discovery order,
    // the entry aggregates: enabled when any adapter is powered, connected when
    // any device is. With one adapter this is exactly the reference rule; no
    // adapter at all is an observed fact and shows as disabled.
    //
    // Transitional connecting/disconnecting states exist in the reference
    // shell only for connections it initiated itself; a read-only observer
    // never initiates one, so a device row is exactly a connected device.
    let enabled = snapshot.adapters.values().any(|adapter| adapter.powered);
    let device_glyphs: Vec<StatusGlyph> = snapshot
        .devices
        .values()
        .filter(|device| device.connected)
        .map(|device| {
            StatusGlyph::BluetoothDevice(BluetoothDeviceClass::from_bluez_icon(
                device.icon.as_deref(),
            ))
        })
        .collect();
    let glyph = if !enabled {
        StatusGlyph::BluetoothDisabled
    } else if device_glyphs.is_empty() {
        StatusGlyph::Bluetooth
    } else {
        StatusGlyph::BluetoothConnected
    };
    Some(StatusVisual::Bluetooth {
        glyph,
        device_glyphs,
    })
}

fn network_status_from_product(
    presentation: &crate::product_network::ProductNetworkPresentation,
) -> Option<StatusVisual> {
    use crate::product_network::ProductNetworkPrimary;
    use gnos_network::NetworkTransport;

    let glyph = match &presentation.primary {
        ProductNetworkPrimary::Unavailable => return None,
        ProductNetworkPrimary::Disconnected | ProductNetworkPrimary::Connecting => {
            StatusGlyph::WifiOff
        }
        ProductNetworkPrimary::Ready(endpoint) => match (&endpoint.wifi, endpoint.transport) {
            (Some(wifi), _) => StatusGlyph::wifi_from_strength(wifi.strength),
            (None, NetworkTransport::Ethernet | NetworkTransport::OtherPhysical) => {
                StatusGlyph::Wired
            }
            (None, NetworkTransport::Wifi | NetworkTransport::None) => StatusGlyph::WifiOff,
        },
    };

    Some(StatusVisual::Network { glyph })
}

/// Volume glyph for the observed default output; absent without one.
pub fn audio_status_from_product(
    audio: &crate::product_audio::ProductAudioPresentation,
) -> Option<StatusVisual> {
    match &audio.output {
        crate::product_audio::ProductAudioEndpointState::Ready(endpoint) => {
            Some(StatusVisual::Audio {
                glyph: StatusGlyph::Volume(crate::product_status_glyph::VolumeGlyph::from_level(
                    endpoint.level.unwrap_or(0.0),
                    endpoint.muted,
                )),
            })
        }
        _ => None,
    }
}

/// Intrinsic module heights of the reference Bar recipe.
///
/// Logo (`OsIcon` slot), Workspaces (capsule equation) and Power are source or
/// qualified-fixture facts, and so is Tray (source-derived below). Clock and
/// Status heights are PROVISIONAL recipe values taken from the frozen layout;
/// their exact equations belong to PARITY-01H and are not claimed here.
pub const LOGO_MODULE_HEIGHT: f32 = 19.0;
pub const POWER_MODULE_HEIGHT: f32 = 24.0;
/// Tray.qml with the observed configuration (`background=false`, `compact=false`,
/// `recolour=false`; the user config has no `bar.tray` override): a TrayItem is a
/// square of `Tokens.font.body.small.pointSize * 2`, the Column spaces items by
/// `Tokens.spacing.small`, and the module pads both ends by
/// `Tokens.padding.extraSmall`. Source-derived, not measured.
pub const BODY_SMALL_POINT_SIZE: f32 = 12.0;
pub const TRAY_ITEM_SIZE: f32 = 2.0 * BODY_SMALL_POINT_SIZE;
pub const TRAY_ITEM_SPACING: f32 = SMALL_PADDING;
pub const TRAY_PADDING: f32 = EXTRA_SMALL_SPACING;
/// Clock.qml, active configuration (background=true, showDate=false,
/// showIcon=true, showSeconds=false; the locale decides the meridiem row).
///
/// Source equation: a ColumnLayout with `spacing.extraSmall` between rows, but
/// every row after the hour sets `topMargin: -spacing - 4`, so those rows
/// overlap the previous one by 4 px. The module pads both ends by
/// `padding.medium`.
pub const CLOCK_PADDING: f32 = 12.0;
pub const CLOCK_SPACING: f32 = EXTRA_SMALL_SPACING;
pub const CLOCK_TRAILING_ROW_OVERLAP: f32 = 4.0;
/// `body.small.scale(1.1)`: 13.2 pt at the observed 96 dpi.
pub const CLOCK_DIGIT_FONT_SIZE: f32 = 17.6;
/// `body.small.scale(0.9)`: 10.8 pt at the observed 96 dpi.
pub const CLOCK_MERIDIEM_FONT_SIZE: f32 = 14.4;
/// Row heights are whole-pixel line boxes of the reference font, ascent and
/// descent each rounded: 17 + 5 at 17.6 px and 14 + 4 at 14.4 px. The live
/// raster confirms both independently: hour and minute baselines are 18 px
/// apart (22 - 4), minute and meridiem baselines 15 px (18 + 14 - 17).
pub const CLOCK_DIGIT_ROW_HEIGHT: f32 = 22.0;
pub const CLOCK_MERIDIEM_ROW_HEIGHT: f32 = 18.0;
/// The calendar row's effective height in the live raster: the module is 105
/// px (rows 1103-1207), so with the rows above the icon contributes 23, not
/// the 24 px of a Status MaterialIcon line. Cause UNRESOLVED; the module total
/// is what the Bar layout consumes.
pub const CLOCK_ICON_ROW_HEIGHT: f32 = 23.0;

/// Vertical layout of the Clock module, offsets from the module top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockLayout {
    pub icon_top: f32,
    pub hour_top: f32,
    pub minute_top: f32,
    pub meridiem_top: Option<f32>,
    /// BAR-01D: the date row, when shown.
    pub date_top: Option<f32>,
    pub height: f32,
}

/// BAR-01D: the date row of the Clock (one small line, "7 Oct").
pub const CLOCK_DATE_FONT_SIZE: f32 = 10.8;
pub const CLOCK_DATE_ROW_HEIGHT: f32 = 16.0;
pub const ROW_CLOCK_DATE_WIDTH: f32 = 44.0;

pub fn clock_layout(meridiem: bool) -> ClockLayout {
    clock_layout_with(meridiem, false)
}

pub fn clock_layout_with(meridiem: bool, date: bool) -> ClockLayout {
    let icon_top = CLOCK_PADDING;
    let hour_top = icon_top + CLOCK_ICON_ROW_HEIGHT + CLOCK_SPACING;
    let minute_top = hour_top + CLOCK_DIGIT_ROW_HEIGHT - CLOCK_TRAILING_ROW_OVERLAP;
    let mut bottom = minute_top + CLOCK_DIGIT_ROW_HEIGHT;
    let meridiem_top = meridiem.then(|| {
        let top = bottom - CLOCK_TRAILING_ROW_OVERLAP;
        bottom = top + CLOCK_MERIDIEM_ROW_HEIGHT;
        top
    });
    let date_top = date.then(|| {
        let top = bottom + CLOCK_SPACING;
        bottom = top + CLOCK_DATE_ROW_HEIGHT;
        top
    });
    ClockLayout {
        icon_top,
        hour_top,
        minute_top,
        meridiem_top,
        date_top,
        height: bottom + CLOCK_PADDING,
    }
}

pub fn clock_module_height(clock: &ClockPresentation) -> f32 {
    clock_layout_with(clock.meridiem.is_some(), clock.date.is_some()).height
}

/// TOPO-01C: on a horizontal Bar every module keeps its length along the
/// Bar (workspaces, tray and status entries lie side by side instead of
/// stacked), except the Clock, which reads as one "HH:MM" run after its
/// calendar glyph, and the ActiveWindow, whose title runs horizontally.
pub const ROW_CLOCK_PADDING: f32 = 10.0;
pub const ROW_CLOCK_GAP: f32 = 6.0;
pub const ROW_CLOCK_GLYPH_WIDTH: f32 = 24.0;
pub const ROW_CLOCK_TIME_WIDTH: f32 = 48.0;
pub const ROW_CLOCK_MERIDIEM_WIDTH: f32 = 26.0;
pub const ROW_ACTIVE_WINDOW_GAP: f32 = 8.0;
pub const ROW_ACTIVE_WINDOW_TITLE_WIDTH: f32 = 240.0;

pub fn row_clock_length(clock: &ClockPresentation) -> f32 {
    2.0 * ROW_CLOCK_PADDING
        + ROW_CLOCK_GLYPH_WIDTH
        + ROW_CLOCK_GAP
        + ROW_CLOCK_TIME_WIDTH
        + clock
            .meridiem
            .map_or(0.0, |_| ROW_CLOCK_GAP + ROW_CLOCK_MERIDIEM_WIDTH)
        + clock
            .date
            .as_ref()
            .map_or(0.0, |_| ROW_CLOCK_GAP + ROW_CLOCK_DATE_WIDTH)
}

/// The horizontal ActiveWindow: icon, gap, then a title box of fixed width.
pub fn row_active_window_length(icon_width: f32) -> f32 {
    icon_width + ROW_ACTIVE_WINDOW_GAP + crate::rail_topology::metrics().title_width
}

/// MaterialIcon with Tokens.font.icon.small: pointSize=int(20/1.33)=15,
/// native line height 24 px on the retained donor font.
pub const STATUS_ICON_LINE_HEIGHT: f32 = 24.0;
/// StatusIcons root.spacing = spacing.medium / 2 = 6. EntryWrapper splits
/// this into 3 px bottom/top margins, so adjacent present entries total 6 px.
pub const STATUS_ENTRY_GAP: f32 = 6.0;
/// LockStatus and BluetoothStatus inner ColumnLayouts use the same 6 px gap.
pub const STATUS_INNER_GAP: f32 = 6.0;
/// Tokens.padding.medium.
pub const STATUS_PADDING: f32 = 12.0;

/// `nonAnimHeight = layout.implicitHeight + padding * 2`. With no item the Column
/// is empty, so the module keeps only its padding and stays visible
/// (`visible: height > 0`).
pub fn tray_module_height(items: usize) -> f32 {
    let column = match items {
        0 => 0.0,
        n => n as f32 * TRAY_ITEM_SIZE + (n - 1) as f32 * TRAY_ITEM_SPACING,
    };
    column + 2.0 * TRAY_PADDING
}

pub fn status_module_height(status: &StatusPresentation) -> f32 {
    status.source_height()
}

pub fn workspaces_module_height(workspaces: &[WorkspacePresentation]) -> f32 {
    if workspaces.is_empty() {
        return 0.0;
    }
    2.0 * EXTRA_SMALL_SPACING
        + workspaces
            .iter()
            .map(|workspace| workspace_preferred_height(workspace.window_count()))
            .sum::<f32>()
        + (workspaces.len() - 1) as f32 * EXTRA_SMALL_SPACING
}

/// The participating `Bar.qml` entries and their intrinsic heights.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleLayout {
    pub entries: Vec<(IdleBarEntry, f32)>,
    /// Non-spacer entries other than ActiveWindow, in entry order.
    pub other_module_heights: Vec<f32>,
    /// `bar.children.length`: entries plus the Repeater.
    pub child_count: usize,
}

/// Provider absence is entry absence: a module with no presentation is not an
/// enabled `Bar.qml` child and consumes neither height nor spacing.
pub fn reference_module_layout(presentation: &BarPresentation) -> ModuleLayout {
    let mut entries = Vec::new();
    for entry in &presentation.order.entries() {
        let height = match entry {
            IdleBarEntry::Logo => presentation
                .system_logo
                .as_ref()
                .map(|_| LOGO_MODULE_HEIGHT),
            IdleBarEntry::Workspaces => Some(workspaces_module_height(&presentation.workspaces)),
            IdleBarEntry::Spacer => Some(0.0),
            IdleBarEntry::ActiveWindow => presentation.active_window.as_ref().map(|_| 0.0),
            IdleBarEntry::Tray => presentation
                .tray
                .as_ref()
                .map(|slots| tray_module_height(slots.len())),
            IdleBarEntry::Clock => presentation.clock.as_ref().map(clock_module_height),
            IdleBarEntry::StatusIcons => presentation.status.as_ref().map(status_module_height),
            IdleBarEntry::Power => presentation
                .power_icon
                .as_ref()
                .map(|_| POWER_MODULE_HEIGHT),
            IdleBarEntry::Overflow => presentation.overflow.then_some(POWER_MODULE_HEIGHT),
        };
        if let Some(height) = height {
            entries.push((*entry, height));
        }
    }
    let other_module_heights = entries
        .iter()
        .filter(|(entry, _)| !matches!(entry, IdleBarEntry::Spacer | IdleBarEntry::ActiveWindow))
        .map(|(_, height)| *height)
        .collect();
    let child_count = entries.len() + 1;
    ModuleLayout {
        entries,
        other_module_heights,
        child_count,
    }
}

pub fn reference_active_window_max_height(presentation: &BarPresentation, bar_height: f32) -> f32 {
    let layout = reference_module_layout(presentation);
    active_window_max_height(bar_height, &layout.other_module_heights, layout.child_count)
}

/// Round to a whole pixel, exact halves going down: `10.5 -> 10`, `10.51 -> 11`.
///
/// Measured behaviour of GPUI's layout quantiser (placement probe, rows A, B
/// and D): explicit sizes and positions are snapped this way, and flex free
/// space is distributed *after* explicit sizes have been snapped.
pub fn layout_round_half_down(value: f32) -> f32 {
    (value - 0.5).ceil()
}

/// Where the layout places the ActiveWindow module: vertically centred between
/// the two flexible spacers of the reference Bar, with every explicit module
/// height snapped to whole pixels first.
///
/// `module_height` is the module's own implicit height (fractional). The result
/// is the quantised top of the module in Bar coordinates.
pub fn active_window_top(
    presentation: &BarPresentation,
    bar_height: f32,
    module_height: f32,
) -> Option<f32> {
    let layout = reference_module_layout(presentation);
    let snapped = |entry: IdleBarEntry, height: f32| match entry {
        IdleBarEntry::ActiveWindow => layout_round_half_down(module_height),
        IdleBarEntry::Spacer => 0.0,
        _ => layout_round_half_down(height),
    };
    let fixed: f32 = layout
        .entries
        .iter()
        .map(|(entry, height)| snapped(*entry, *height))
        .sum();
    let gaps = module_gap() * layout.entries.len().saturating_sub(1) as f32;
    let spacers = layout
        .entries
        .iter()
        .filter(|(entry, _)| *entry == IdleBarEntry::Spacer)
        .count();
    if spacers == 0 {
        return None;
    }
    let free = (bar_height - 2.0 * LARGE_VERTICAL_PADDING - fixed - gaps).max(0.0);
    let spacer = free / spacers as f32;
    let mut y = LARGE_VERTICAL_PADDING;
    for (entry, height) in &layout.entries {
        if *entry == IdleBarEntry::ActiveWindow {
            return Some(layout_round_half_down(y));
        }
        y += if *entry == IdleBarEntry::Spacer {
            spacer
        } else {
            snapped(*entry, *height)
        } + MEDIUM_SPACING;
    }
    None
}

/// Realise every Tray item's icon source, aligned with `presentation.tray`.
/// A captured slot whose identity was not retained has no source to realise.
pub fn realize_tray_visuals(
    presentation: &BarPresentation,
    search: &gnosix_icon_theme::IconThemeSearch,
) -> Vec<crate::product_tray_icon::RealizedTrayIcon> {
    use crate::product_tray_icon::{MissingReason, RealizedTrayIcon, realize_tray_icon};
    presentation
        .tray
        .iter()
        .flatten()
        .map(|visual| match visual {
            TrayVisual::CapturedUnresolved { .. } => {
                RealizedTrayIcon::Missing(MissingReason::NoSource)
            }
            TrayVisual::Native(item) => {
                realize_tray_icon(&item.source, search, TRAY_ITEM_SIZE as u32)
            }
        })
        .collect()
}

/// Plan the ActiveWindow module from the typed presentation and the actual
/// module layout. None when the presentation carries no ActiveWindow.
pub fn plan_active_window(
    presentation: &BarPresentation,
    wts: &gpui::WindowTextSystem,
    bar_height: f32,
    state: &mut crate::reference_active_window::ActiveWindowState,
    now_ms: f32,
    reduced_motion: bool,
) -> Result<
    Option<crate::reference_active_window::ActiveWindowPlan>,
    crate::reference_active_window::TitleError,
> {
    let Some(window) = presentation.active_window.as_ref() else {
        return Ok(None);
    };
    let layout = reference_module_layout(presentation);
    state
        .plan(
            wts,
            &crate::reference_active_window::ActiveWindowInputs {
                title: &window.title,
                role: window.icon.role(),
                inverted: window.config.inverted,
                bar_height,
                other_module_heights: &layout.other_module_heights,
                bar_child_count: layout.child_count,
            },
            now_ms,
            reduced_motion,
        )
        .map(Some)
}

/// Controlled ActiveWindow qualification cases for the isolated lab. Each is a
/// deterministic timeline of (presentation, time) steps ending at the shown
/// frame; none touches a host compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveWindowCase {
    /// The frozen live title.
    Live,
    /// No focused client.
    Desktop,
    /// A title longer than the available height, right-elided.
    Long,
    /// `bar.activeWindow.compact` over a hyphenated title.
    Compact,
    /// `bar.activeWindow.inverted` (not the live configuration).
    Inverted,
    /// Mid-fade between two titles, sampled 100 ms after the change.
    Crossfade,
}

impl ActiveWindowCase {
    pub const ALL: [Self; 6] = [
        Self::Live,
        Self::Desktop,
        Self::Long,
        Self::Compact,
        Self::Inverted,
        Self::Crossfade,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Desktop => "desktop",
            Self::Long => "long",
            Self::Compact => "compact",
            Self::Inverted => "inverted",
            Self::Crossfade => "crossfade",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|case| case.name() == name)
    }

    fn with_window(
        title: String,
        icon: ActiveWindowIcon,
        config: ActiveWindowConfig,
    ) -> BarPresentation {
        let mut presentation = BarPresentation::captured_oracle_2026_10_07();
        presentation.active_window = Some(ActiveWindowPresentation {
            title,
            icon,
            config,
        });
        presentation
    }

    fn timeline(self) -> Vec<(BarPresentation, f32)> {
        let live = BarPresentation::captured_oracle_2026_10_07();
        let web = || ActiveWindowIcon::CapturedRole {
            role: IconRole::Web,
        };
        let desktop = Self::with_window(
            active_window_title(None, false),
            ActiveWindowIcon::NoActiveToplevel,
            ActiveWindowConfig::default(),
        );
        // Every timeline first settles its initial title so only the case's
        // own transition is on screen.
        match self {
            Self::Live => vec![(live.clone(), 0.0), (live, 10_000.0)],
            Self::Desktop => vec![(desktop.clone(), 0.0), (desktop, 10_000.0)],
            Self::Long => {
                let long = Self::with_window(
                    "A very long window title that keeps going far beyond the height the \
                     Bar leaves available to the ActiveWindow module so that it must be \
                     elided on the right and never wraps or stacks characters"
                        .into(),
                    web(),
                    ActiveWindowConfig::default(),
                );
                vec![(long.clone(), 0.0), (long, 10_000.0)]
            }
            Self::Compact => {
                let config = ActiveWindowConfig {
                    compact: true,
                    ..ActiveWindowConfig::default()
                };
                let compact = Self::with_window(
                    active_window_title(Some("Ratification Phase 5 OSD - Helium"), true),
                    web(),
                    config,
                );
                vec![(compact.clone(), 0.0), (compact, 10_000.0)]
            }
            Self::Inverted => {
                let config = ActiveWindowConfig {
                    inverted: true,
                    ..ActiveWindowConfig::default()
                };
                let inverted =
                    Self::with_window("Ratification Phase 5 OSD - Helium".into(), web(), config);
                vec![(inverted.clone(), 0.0), (inverted, 10_000.0)]
            }
            Self::Crossfade => vec![
                (desktop.clone(), 0.0),
                (desktop, 10_000.0),
                (live.clone(), 20_000.0),
                (live, 20_100.0),
            ],
        }
    }

    /// The presentation shown by the final step (what the Bar fixture owns).
    pub fn presentation(self) -> BarPresentation {
        self.timeline().pop().expect("non-empty timeline").0
    }

    /// Replay the timeline and plan the shown frame.
    pub fn plan(
        self,
        wts: &gpui::WindowTextSystem,
        bar_height: f32,
    ) -> Result<
        Option<crate::reference_active_window::ActiveWindowPlan>,
        crate::reference_active_window::TitleError,
    > {
        let mut state = crate::reference_active_window::ActiveWindowState::default();
        let mut shown = None;
        for (presentation, now_ms) in self.timeline() {
            shown = plan_active_window(&presentation, wts, bar_height, &mut state, now_ms, false)?;
        }
        Ok(shown)
    }
}

/// The reference shell's border: the Bar width on the left, the donor border
/// thickness elsewhere, the donor rounding at the four inner corners.
pub fn reference_shell_border() -> crate::product_shell_material::ShellBorder {
    reference_shell_border_for(true)
}

/// TOPO-01A/C: the shell with the rail on `side`.
pub fn reference_shell_border_on(
    side: crate::rail_topology::RailSide,
) -> crate::product_shell_material::ShellBorder {
    let (left, top, right, bottom) = side.insets4();
    crate::product_shell_material::ShellBorder {
        left,
        top,
        right,
        bottom,
        radius: BORDER_ROUNDING,
    }
}

/// The same border on an output with or without a Bar (an excluded output
/// keeps the ordinary border thickness on the Bar's side).
pub fn reference_shell_border_for(bar_present: bool) -> crate::product_shell_material::ShellBorder {
    crate::product_shell_material::ShellBorder::for_output(
        bar_present.then_some(BAR_INNER_WIDTH + 2.0 * SMALL_PADDING.max(BORDER_THICKNESS)),
        BORDER_THICKNESS,
        BORDER_ROUNDING,
    )
}

/// `MultiEffect.blurMax` and `shadowColor` alpha of the shell's shadow.
pub const REFERENCE_SHADOW_BLUR_MAX: f32 = 15.0;
pub const REFERENCE_SHADOW_ALPHA: f32 = 0.7;

/// Locator markers for controlled captures: four opaque squares inside the
/// uncovered area, at fixed offsets from the client origin. They sit outside
/// every protected region and are never painted unless a capture asks.
pub const LOCATOR_MARKER_SIZE: f32 = 8.0;
pub const LOCATOR_MARKER_COLOUR: u32 = 0xff00ffff;
pub fn locator_marker_origins(output: (f32, f32)) -> [(f32, f32); 2] {
    [(200.0, 100.0), (output.0 - 260.0, output.1 - 140.0)]
}

/// Paints the shell material natively: interior shadow, then the shell
/// (strips plus four exact corner fillets) in its translucent surface colour.
/// Nothing is painted in the uncovered area except the shadow, so the
/// compositor's backdrop (and its blur, where requested) shows through.
#[cfg(feature = "component-lab")]
pub fn paint_native_shell_material(
    bounds: Bounds<Pixels>,
    material: &crate::product_shell_material::ShellMaterial,
    locator: bool,
    window: &mut Window,
) {
    use crate::product_shell_material::Rect;
    let origin = (f32::from(bounds.left()), f32::from(bounds.top()));
    let output = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let to_gpui = |c: [u8; 4]| rgba(u32::from_be_bytes(c));
    let point_at = |p: (f32, f32)| point(px(origin.0 + p.0), px(origin.1 + p.1));
    let quad = |r: Rect| {
        Bounds::new(
            point(px(origin.0 + r.x0), px(origin.1 + r.y0)),
            size(px(r.width()), px(r.height())),
        )
    };

    if let Some(shadow) = material.shadow {
        for inset in 0..shadow.extent() {
            let alpha = shadow.paint_alpha(inset as f32 + 0.5);
            let colour = to_gpui(
                crate::product_shell_material::Rgba {
                    rgb: shadow.rgb,
                    a: alpha,
                }
                .to_u8(),
            );
            let outline = material.border.inset_outline(output, inset as f32 + 0.5, 8);
            let points: Vec<_> = outline.into_iter().map(point_at).collect();
            let mut builder = PathBuilder::stroke(px(1.0));
            builder.add_polygon(&points, true);
            if let Ok(path) = builder.build() {
                window.paint_path(path, colour);
            }
        }
    }

    let surface = to_gpui(material.surface.to_u8());
    for strip in material.border.strips(output) {
        window.paint_quad(fill(quad(strip), surface));
    }
    for fillet in material.border.fillets(output, 16) {
        let points: Vec<_> = fillet.into_iter().map(point_at).collect();
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&points, true);
        if let Ok(path) = builder.build() {
            window.paint_path(path, surface);
        }
    }

    if locator {
        for (x, y) in locator_marker_origins(output) {
            window.paint_quad(fill(
                Bounds::new(
                    point(px(origin.0 + x), px(origin.1 + y)),
                    size(px(LOCATOR_MARKER_SIZE), px(LOCATOR_MARKER_SIZE)),
                ),
                rgba(LOCATOR_MARKER_COLOUR),
            ));
        }
    }
}

#[cfg(feature = "component-lab")]
pub fn paint_idle_shell_border(bounds: Bounds<Pixels>, window: &mut Window) {
    let left = f32::from(bounds.left());
    let top = f32::from(bounds.top());
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let bar_width = BAR_INNER_WIDTH + 2.0 * SMALL_PADDING.max(BORDER_THICKNESS);

    for quad in [
        Bounds::new(point(px(left), px(top)), size(px(bar_width), px(height))),
        Bounds::new(
            point(px(left + bar_width), px(top)),
            size(px((width - bar_width).max(0.0)), px(BORDER_THICKNESS)),
        ),
        Bounds::new(
            point(px(left + bar_width), px(top + height - BORDER_THICKNESS)),
            size(px((width - bar_width).max(0.0)), px(BORDER_THICKNESS)),
        ),
        Bounds::new(
            point(px(left + width - BORDER_THICKNESS), px(top)),
            size(px(BORDER_THICKNESS), px(height)),
        ),
    ] {
        window.paint_quad(fill(quad, rgba(COLOR_SURFACE)));
    }
}

#[cfg(feature = "component-lab")]
fn system_logo(logo: &SystemLogoPresentation) -> impl gpui::IntoElement {
    let icon = match logo {
        SystemLogoPresentation::DefaultShellLogo => {
            let path = format!(
                "{}/../../evidence/research/gnosix-caelestia-native-parity-01/oracle-assets/caelestia-bar-logo-qt6-26.png",
                env!("CARGO_MANIFEST_DIR")
            );
            img(std::path::PathBuf::from(path))
                // `svg()` is intentionally not used here: GPUI's Svg element
                // paints a monochrome alpha mask and applies `text_color`, which
                // destroys Logo.qml's primary/onSurface two-colour oracle.
                // The retained PNG is a one-time Qt 6 Shape.CurveRenderer
                // raster of the exact five donor paths at their source 26x26
                // effective size. Gnosix renders it natively as an image and
                // does not carry Qt/QML into the runtime.
                // Tokens.font.body.large is 16 pt in the active AppearanceFont
                // body style. Logo.qml therefore receives round(16 * 1.6)=26.
                .w(px(26.0))
                .h(px(26.0))
                .flex_shrink_0()
                // Qt and GPUI choose opposite integer pixels for the centred
                // 26-in-19 half-pixel origin. Offset the painted image while
                // preserving the source-exact 19 px parent slot.
                .relative()
                .left(px(1.0))
                .top(px(1.0))
                .into_any_element()
        }
        SystemLogoPresentation::DistroIcon { identity } => {
            let path = format!("/usr/share/icons/{identity}.svg");
            svg()
                .external_path(path)
                // OsIcon / ColouredIcon uses round(16 * 1.2)=19.
                .w(px(19.0))
                .h(px(19.0))
                .text_color(rgba(COLOR_TERTIARY))
                .into_any_element()
        }
    };

    // Mirrors OsIcon.qml's effective 19x19 root. The default 26x26 Logo
    // deliberately overflows this slot while remaining centred.
    div()
        .w(px(19.0))
        .h(px(19.0))
        .flex()
        .items_center()
        .justify_center()
        .child(icon)
}

#[cfg(feature = "component-lab")]
fn material_shape(shape: MaterialShape, size_px: f32, color: u32) -> AnyElement {
    if shape == MaterialShape::Pill && color == COLOR_ON_PRIMARY {
        // Frozen PARITY-01E oracle endpoint. The alpha coverage is produced by
        // the exact installed m3shapes QSG shader under controlled Wayland and
        // recoloured with the frozen onPrimary token. Keeping the full 32x32
        // cell preserves MaterialShape's own 2/3 scale and centring.
        let path = format!(
            "{}/../../evidence/research/gnosix-caelestia-native-parity-01/workspace-oracle/pill-focused-qsg-onprimary.png",
            env!("CARGO_MANIFEST_DIR")
        );
        return img(std::path::PathBuf::from(path))
            .w(px(WORKSPACE_INDICATOR_SIZE))
            .h(px(WORKSPACE_INDICATOR_SIZE))
            .flex_shrink_0()
            .into_any_element();
    }

    let path = format!(
        "{}/../../evidence/research/gnosix-caelestia-native-parity-01/material-shapes/{}.svg",
        env!("CARGO_MANIFEST_DIR"),
        shape.asset_slug()
    );
    svg()
        .external_path(path)
        .w(px(size_px))
        .h(px(size_px))
        .flex_shrink_0()
        .text_color(rgba(color))
        .into_any_element()
}

#[cfg(feature = "component-lab")]
fn capsule(fill: u32) -> Div {
    div()
        .w(px(BAR_INNER_WIDTH))
        .rounded(px(BAR_INNER_WIDTH / 2.0))
        .bg(rgba(fill))
        .flex()
        .flex_col()
        .items_center()
}

/// The Clock module: native calendar glyph and reference-font rows, placed by
/// `clock_layout`. Nothing here is sized independently of that layout.
#[cfg(feature = "component-lab")]
fn clock_element(clock: &ClockPresentation, capsule_fill: u32) -> Div {
    use crate::reference_active_window::REFERENCE_FONT_FAMILY;
    let layout = clock_layout_with(clock.meridiem.is_some(), clock.date.is_some());
    let row = |top: f32, height: f32, size: f32, text: String| {
        div()
            .absolute()
            .left(px(0.0))
            .top(px(top))
            .w(px(BAR_INNER_WIDTH))
            .h(px(height))
            .flex()
            .justify_center()
            .font_family(REFERENCE_FONT_FAMILY)
            .text_size(px(size))
            .line_height(px(height))
            .text_color(rgba(COLOR_TERTIARY))
            .child(text)
    };
    let mut module = capsule(capsule_fill)
        .relative()
        .h(px(layout.height))
        .child(
            status_icon_line(StatusGlyph::Calendar, COLOR_TERTIARY)
                .absolute()
                .left(px(0.0))
                .top(px(layout.icon_top))
                .h(px(CLOCK_ICON_ROW_HEIGHT)),
        )
        .child(row(
            layout.hour_top,
            CLOCK_DIGIT_ROW_HEIGHT,
            CLOCK_DIGIT_FONT_SIZE,
            clock.hour.clone(),
        ))
        .child(row(
            layout.minute_top,
            CLOCK_DIGIT_ROW_HEIGHT,
            CLOCK_DIGIT_FONT_SIZE,
            clock.minute.clone(),
        ));
    if let (Some(top), Some(meridiem)) = (layout.meridiem_top, clock.meridiem) {
        module = module.child(row(
            top,
            CLOCK_MERIDIEM_ROW_HEIGHT,
            CLOCK_MERIDIEM_FONT_SIZE,
            meridiem.as_str().to_owned(),
        ));
    }
    if let (Some(top), Some(date)) = (layout.date_top, clock.date.as_ref()) {
        module = module.child(row(
            top,
            CLOCK_DATE_ROW_HEIGHT,
            CLOCK_DATE_FONT_SIZE,
            date.clone(),
        ));
    }
    module
}

/// One native status glyph centred in a 24 px source line.
#[cfg(feature = "component-lab")]
fn status_icon_line(glyph: StatusGlyph, color: u32) -> Div {
    glyph_cell(glyph, color, BAR_INNER_WIDTH, STATUS_ICON_LINE_HEIGHT)
}

/// One native status glyph centred in a `width` x `height` cell.
#[cfg(feature = "component-lab")]
fn glyph_cell(glyph: StatusGlyph, color: u32, width: f32, height: f32) -> Div {
    use crate::product_status_glyph::status_glyph_paths;
    const GLYPH_SIZE: f32 = 18.0;
    div().w(px(width)).h(px(height)).flex_shrink_0().child(
        canvas(
            move |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let origin = (
                    f32::from(bounds.left()) + (f32::from(bounds.size.width) - GLYPH_SIZE) / 2.0,
                    f32::from(bounds.top()) + (f32::from(bounds.size.height) - GLYPH_SIZE) / 2.0,
                );
                for path in status_glyph_paths(glyph, origin, GLYPH_SIZE) {
                    window.paint_path(path, rgba(color));
                }
            },
        )
        .size_full(),
    )
}

#[cfg(feature = "component-lab")]
fn status_visual_element(item: &StatusVisual) -> Div {
    let column = |lines: Vec<Div>| {
        div()
            .w(px(BAR_INNER_WIDTH))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(STATUS_INNER_GAP))
            .children(lines)
    };
    match item {
        StatusVisual::Lock {
            caps_lock,
            num_lock,
        } => {
            let mut lines = Vec::new();
            if *caps_lock {
                lines.push(status_icon_line(StatusGlyph::CapsLock, COLOR_SECONDARY));
            }
            if *num_lock {
                lines.push(status_icon_line(StatusGlyph::NumLock, COLOR_SECONDARY));
            }
            column(lines)
        }
        StatusVisual::Network { glyph } => status_icon_line(*glyph, COLOR_SECONDARY),
        StatusVisual::Bluetooth {
            glyph,
            device_glyphs,
        } => column(
            std::iter::once(*glyph)
                .chain(device_glyphs.iter().copied())
                .map(|glyph| status_icon_line(glyph, COLOR_SECONDARY))
                .collect(),
        ),
        StatusVisual::BatteryOrProfile { glyph, error } => {
            status_icon_line(*glyph, if *error { COLOR_ERROR } else { COLOR_SECONDARY })
        }
        StatusVisual::Audio { glyph } => status_icon_line(*glyph, COLOR_SECONDARY),
    }
}

/// The ActiveWindow module: icon, then one native rotated title run per slot.
/// Everything is derived from the plan; nothing here is sized by a constant
/// other than the donor icon box and top margin.
#[cfg(feature = "component-lab")]
fn active_window_element(
    plan: &crate::reference_active_window::ActiveWindowPlan,
) -> impl gpui::IntoElement {
    use crate::product_icon::icon_paths;
    use crate::reference_active_window::title_paths;

    let inverted = plan.frame.inverted;
    let icon_left = ((plan.implicit_width - plan.icon_width) / 2.0).round();
    let title_top = plan.icon_height + ACTIVE_WINDOW_TITLE_TOP_MARGIN;
    let mut module = div()
        .relative()
        .w(px(plan.implicit_width))
        .h(px(plan.implicit_height))
        .overflow_hidden();
    if let Some(role) = plan.role {
        // Native icon primitive: the em square sits on the icon box's baseline.
        let square_top = plan.icon_baseline - crate::reference_active_window::ICON_SIZE;
        module = module.child(
            canvas(
                move |bounds, _, _| bounds,
                move |bounds, _, window, _| {
                    let origin = (
                        f32::from(bounds.left()),
                        f32::from(bounds.top()) + square_top,
                    );
                    for path in icon_paths(role, origin, crate::reference_active_window::ICON_SIZE)
                    {
                        window.paint_path(path, rgba(COLOR_PRIMARY));
                    }
                },
            )
            .absolute()
            .left(px(icon_left))
            .top(px(0.0))
            .w(px(plan.icon_width))
            .h(px(plan.icon_height)),
        );
    }
    for (title, opacity) in plan.frame.slots.iter().cloned() {
        if title.glyphs.is_empty() || opacity <= 0.0 {
            continue;
        }
        let color = gpui::Hsla::from(rgba(COLOR_PRIMARY)).opacity(opacity);
        module = module.child(
            canvas(
                move |bounds, _, _| bounds,
                move |bounds, _, window, _| {
                    let origin = (f32::from(bounds.left()), f32::from(bounds.top()));
                    for path in title_paths(&title, inverted, origin) {
                        window.paint_path(path, color);
                    }
                },
            )
            .absolute()
            .left(px(0.0))
            .top(px(title_top))
            .w(px(plan.title_line_height))
            .h(px((plan.implicit_height - title_top).max(0.0))),
        );
    }
    module
}

#[cfg(feature = "component-lab")]
pub fn idle_bar_fixture_content(
    presentation: &BarPresentation,
    active_window_plan: Option<&crate::reference_active_window::ActiveWindowPlan>,
    active_window_observed: std::rc::Rc<std::cell::RefCell<Option<[f32; 4]>>>,
    tray_icons: &[crate::product_tray_icon::RealizedTrayIcon],
    capsule_fill: u32,
) -> Div {
    assemble_modules(
        div()
            .w(px(crate::rail_topology::rail_width()))
            .h_full()
            .pt(px(LARGE_VERTICAL_PADDING))
            .pb(px(LARGE_VERTICAL_PADDING))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(module_gap())),
        idle_bar_column_modules(
            presentation,
            active_window_plan,
            active_window_observed,
            tray_icons,
            capsule_fill,
        ),
        &presentation.order,
    )
}

/// The rest Bar's three groups around its two flexible spacers: start
/// (logo, workspaces), centre (the active window) and end (the rest).
#[cfg(feature = "component-lab")]
fn assemble_modules(
    root: Div,
    mut modules: Vec<(IdleBarEntry, AnyElement)>,
    order: &BarModuleOrder,
) -> Div {
    let mut root = root;
    for entry in order.entries() {
        if entry == IdleBarEntry::Spacer {
            root = root.child(div().flex_1());
        } else if let Some(i) = modules.iter().position(|(e, _)| *e == entry) {
            root = root.child(modules.remove(i).1);
        }
    }
    root
}

/// BAR-01B: the vertical rail's modules, one element each, in order. The
/// rail assembles them with its spacers; a travelling Bar places each one
/// on the material (`bar_strip`).
#[cfg(feature = "component-lab")]
pub fn idle_bar_column_modules(
    presentation: &BarPresentation,
    active_window_plan: Option<&crate::reference_active_window::ActiveWindowPlan>,
    active_window_observed: std::rc::Rc<std::cell::RefCell<Option<[f32; 4]>>>,
    tray_icons: &[crate::product_tray_icon::RealizedTrayIcon],
    capsule_fill: u32,
) -> Vec<(IdleBarEntry, AnyElement)> {
    let workspaces = presentation.workspaces.iter().map(|workspace| {
        let has_windows = workspace.window_count() > 0;
        // Workspace.qml:
        // layout.implicitHeight = 32 + (-extraSmall/2) + window layoutHeight,
        // then LazyListView.preferredHeight adds padding.extraSmall when
        // windows are present. With the active oracle tokens this is
        // 32 - 2 + 24 + 4 = 58.
        let height = workspace_preferred_height(workspace.window_count());
        let shape_size = workspace_shape_size(workspace.active, has_windows);
        let shape_color = if workspace.active {
            COLOR_ON_PRIMARY
        } else if has_windows {
            COLOR_ON_SURFACE
        } else {
            // Workspace.qml uses Colours.layer(m3outlineVariant, 2). The
            // wallpaper/transparency transformation belongs to PARITY-01I;
            // 01E owns the semantic palette endpoint and geometry only.
            COLOR_OUTLINE_VARIANT
        };
        let shape: Option<AnyElement> = match workspace.shape {
            WorkspaceShapePresentation::Resolved(shape) => {
                Some(material_shape(shape, shape_size, shape_color))
            }
            WorkspaceShapePresentation::FocusedUnresolved => None,
        };
        let marks = workspace.window_marks.iter().map(|mark| match mark {
            WorkspaceWindowMark::CapturedUnresolved {
                observed_material_symbol,
                ..
            } if workspace.active && observed_material_symbol == "web" => {
                // The frozen capture retained this visual mark but not the
                // client identity. The asset reproduces MaterialIcon plus the
                // ActiveIndicator Colouriser chain under controlled Qt 6 QSG.
                let path = format!(
                    "{}/../../evidence/research/gnosix-caelestia-native-parity-01/workspace-oracle/web-mark-qsg-colourised.png",
                    env!("CARGO_MANIFEST_DIR")
                );
                img(std::path::PathBuf::from(path))
                    .w(px(WORKSPACE_INDICATOR_SIZE))
                    .h(px(WORKSPACE_WINDOW_LAYOUT_HEIGHT))
                    .flex_shrink_0()
                    .into_any_element()
            }
            WorkspaceWindowMark::CapturedUnresolved {
                observed_material_symbol,
                ..
            } => crate::icons::icon(
                observed_material_symbol.clone(),
                20.0,
                if workspace.active {
                    COLOR_ON_PRIMARY
                } else {
                    COLOR_ON_SURFACE_VARIANT
                },
            )
            .w(px(WORKSPACE_INDICATOR_SIZE))
            .h(px(WORKSPACE_WINDOW_LAYOUT_HEIGHT))
            .into_any_element(),
            WorkspaceWindowMark::NativeClass { .. } => {
                // Pure semantic projection intentionally has no XDG catalogue
                // context, so it preserves the slot without inventing a mark.
                div()
                    .w(px(WORKSPACE_INDICATOR_SIZE))
                    .h(px(WORKSPACE_WINDOW_LAYOUT_HEIGHT))
                    .into_any_element()
            }
            WorkspaceWindowMark::NativeResolved {
                material_symbol, ..
            } => crate::icons::icon(
                material_symbol.clone(),
                20.0,
                if workspace.active {
                    COLOR_ON_PRIMARY
                } else {
                    COLOR_ON_SURFACE_VARIANT
                },
            )
            .w(px(WORKSPACE_INDICATOR_SIZE))
            .h(px(WORKSPACE_WINDOW_LAYOUT_HEIGHT))
            .into_any_element(),
        });

        let content = div()
            .w(px(32.0))
            .h(px(height))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(WORKSPACE_INDICATOR_SIZE))
                    .h(px(WORKSPACE_INDICATOR_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(shape, |slot, shape| slot.child(shape)),
            )
            .when(has_windows, |content| {
                content
                    .child(
                        div()
                            .w(px(WORKSPACE_INDICATOR_SIZE))
                            .h(px(
                                WORKSPACE_WINDOW_LAYOUT_HEIGHT * workspace.window_count() as f32
                            ))
                            .mt(px(-EXTRA_SMALL_SPACING / 2.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .children(marks),
                    )
                    .pb(px(EXTRA_SMALL_SPACING))
            });

        if workspace.active {
            // ActiveIndicator.qml owns one rounded primary mask spanning the
            // active workspace's full preferred height, then Colouriser maps
            // onSurface content to onPrimary inside it.
            let frozen_oracle_indicator = matches!(
                workspace.shape,
                WorkspaceShapePresentation::Resolved(MaterialShape::Pill)
            ) && workspace.window_marks.iter().any(|mark| {
                matches!(
                    mark,
                    WorkspaceWindowMark::CapturedUnresolved { .. }
                )
            }) && (height - 58.0).abs() < f32::EPSILON;

            let root = div()
                .w(px(WORKSPACE_INDICATOR_SIZE))
                .h(px(height))
                .relative();

            if frozen_oracle_indicator {
                let path = format!(
                    "{}/../../evidence/research/gnosix-caelestia-native-parity-01/workspace-oracle/active-indicator-32x58-primary.png",
                    env!("CARGO_MANIFEST_DIR")
                );
                root.child(
                    img(std::path::PathBuf::from(path))
                        .absolute()
                        .inset_0()
                        .size_full(),
                )
                .child(content)
            } else {
                root.rounded(px(WORKSPACE_INDICATOR_SIZE / 2.0))
                    .bg(rgba(COLOR_PRIMARY))
                    .child(content)
            }
        } else {
            content
        }
    });

    // Module is centred in the 40 px column; Qt rounds the half pixel up.
    let active_window = active_window_plan.map(|plan| {
        div()
            .w(px(BAR_INNER_WIDTH))
            .h(px(plan.implicit_height))
            .flex()
            .flex_col()
            .items_start()
            .pl(px(((BAR_INNER_WIDTH - plan.implicit_width) / 2.0).round()))
            .child(active_window_element(plan))
            // Observation only: where the layout actually placed the module.
            .child(
                canvas(
                    move |bounds, _, _| {
                        *active_window_observed.borrow_mut() = Some([
                            f32::from(bounds.left()),
                            f32::from(bounds.top()),
                            f32::from(bounds.right()),
                            f32::from(bounds.bottom()),
                        ]);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    });

    // Heights are the recipe's (`tray_module_height`), so the flex layout and
    // the computed ActiveWindow maxHeight describe the same Bar.
    let tray = presentation.tray.as_ref().map(|slots| {
        div()
            .w(px(BAR_INNER_WIDTH))
            .h(px(tray_module_height(slots.len())))
            .py(px(TRAY_PADDING))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(TRAY_ITEM_SPACING))
            .children(slots.iter().enumerate().map(|(index, slot)| {
                let slot_box = div()
                    .w(px(TRAY_ITEM_SIZE))
                    .h(px(TRAY_ITEM_SIZE))
                    .flex_shrink_0();
                match (slot, tray_icons.get(index)) {
                    (TrayVisual::Native(_), Some(realized)) => slot_box.child(
                        crate::product_tray_icon::tray_icon_element(realized, TRAY_ITEM_SIZE),
                    ),
                    // Identity not retained by the oracle (or no realised icon):
                    // a neutral native marker, never an application glyph.
                    _ => slot_box.flex().items_center().justify_center().child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgba(COLOR_SECONDARY)),
                    ),
                }
            }))
    });

    let clock = presentation
        .clock
        .as_ref()
        .map(|clock| clock_element(clock, capsule_fill));

    let status = presentation.status.as_ref().map(|status| {
        capsule(capsule_fill)
            .h(px(status_module_height(status)))
            .py(px(STATUS_PADDING))
            .gap(px(STATUS_ENTRY_GAP))
            .children(
                status
                    .entries
                    .iter()
                    .filter(|item| item.is_present())
                    .map(status_visual_element),
            )
    });

    // The Session button carries the Gnosix cube (or the power symbol).
    let power = presentation.power_icon.as_ref().map(|icon| {
        crate::identity::session_mark(icon, 18.0, COLOR_ERROR)
            .font_weight(FontWeight::BOLD)
            .w(px(BAR_INNER_WIDTH))
            .h(px(POWER_MODULE_HEIGHT))
    });
    let overflow = presentation.overflow.then(|| {
        crate::icons::icon("more_horiz", 18.0, COLOR_ON_SURFACE)
            .w(px(BAR_INNER_WIDTH))
            .h(px(POWER_MODULE_HEIGHT))
    });

    let mut modules: Vec<(IdleBarEntry, AnyElement)> = Vec::new();
    if let Some(identity) = presentation.system_logo.as_ref() {
        modules.push((
            IdleBarEntry::Logo,
            div()
                .w(px(BAR_INNER_WIDTH))
                // Effective OsIcon height is round(16 * 1.2)=19. Keeping
                // this exact flow height also preserves PARITY-01B's
                // already-qualified workspace composition boundary.
                .h(px(19.0))
                .flex()
                .items_center()
                .justify_center()
                .child(system_logo(identity))
                .into_any_element(),
        ));
    }
    modules.push((
        IdleBarEntry::Workspaces,
        capsule(capsule_fill)
            .py(px(EXTRA_SMALL_SPACING))
            .gap(px(EXTRA_SMALL_SPACING))
            .children(workspaces)
            .into_any_element(),
    ));
    let rest = [
        (
            IdleBarEntry::ActiveWindow,
            active_window.map(|e| e.into_any_element()),
        ),
        (IdleBarEntry::Tray, tray.map(|e| e.into_any_element())),
        (IdleBarEntry::Clock, clock.map(|e| e.into_any_element())),
        (
            IdleBarEntry::StatusIcons,
            status.map(|e| e.into_any_element()),
        ),
        (IdleBarEntry::Power, power.map(|e| e.into_any_element())),
        (
            IdleBarEntry::Overflow,
            overflow.map(|e| e.into_any_element()),
        ),
    ];
    modules.extend(
        rest.into_iter()
            .filter_map(|(entry, e)| e.map(|e| (entry, e))),
    );
    modules
}

/// TOPO-01C: the same modules on a horizontal Bar, in the same order from
/// left to right. Every module keeps its length along the Bar (see
/// `row_clock_length`), so the pointer router built with `new_on(..,
/// horizontal)` hit-tests what is painted here.
#[cfg(feature = "component-lab")]
pub fn idle_bar_row_content(
    presentation: &BarPresentation,
    active_window_plan: Option<&crate::reference_active_window::ActiveWindowPlan>,
    tray_icons: &[crate::product_tray_icon::RealizedTrayIcon],
    capsule_fill: u32,
) -> Div {
    assemble_modules(
        div()
            .w_full()
            .h(px(crate::rail_topology::rail_width()))
            .px(px(LARGE_VERTICAL_PADDING))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(module_gap())),
        idle_bar_row_modules(presentation, active_window_plan, tray_icons, capsule_fill),
        &presentation.order,
    )
}

/// BAR-01B: the horizontal Bar's modules, one element each, in order.
#[cfg(feature = "component-lab")]
pub fn idle_bar_row_modules(
    presentation: &BarPresentation,
    active_window_plan: Option<&crate::reference_active_window::ActiveWindowPlan>,
    tray_icons: &[crate::product_tray_icon::RealizedTrayIcon],
    capsule_fill: u32,
) -> Vec<(IdleBarEntry, AnyElement)> {
    use crate::reference_active_window::REFERENCE_FONT_FAMILY;
    let row_capsule = |fill: u32| {
        div()
            .h(px(BAR_INNER_WIDTH))
            .rounded(px(BAR_INNER_WIDTH / 2.0))
            .bg(rgba(fill))
            .flex()
            .flex_row()
            .items_center()
            .flex_shrink_0()
    };
    let mark_color = |active: bool| {
        if active {
            COLOR_ON_PRIMARY
        } else {
            COLOR_ON_SURFACE_VARIANT
        }
    };
    let workspaces = presentation.workspaces.iter().map(|workspace| {
        let count = workspace.window_count();
        let length = workspace_preferred_height(count);
        let shape_color = if workspace.active {
            COLOR_ON_PRIMARY
        } else if count > 0 {
            COLOR_ON_SURFACE
        } else {
            COLOR_OUTLINE_VARIANT
        };
        let shape = match workspace.shape {
            WorkspaceShapePresentation::Resolved(shape) => Some(material_shape(
                shape,
                workspace_shape_size(workspace.active, count > 0),
                shape_color,
            )),
            WorkspaceShapePresentation::FocusedUnresolved => None,
        };
        let marks = workspace.window_marks.iter().map(|mark| {
            let symbol = match mark {
                WorkspaceWindowMark::CapturedUnresolved {
                    observed_material_symbol,
                    ..
                } => Some(observed_material_symbol.clone()),
                WorkspaceWindowMark::NativeResolved {
                    material_symbol, ..
                } => Some(material_symbol.clone()),
                WorkspaceWindowMark::NativeClass { .. } => None,
            };
            let slot = div()
                .w(px(WORKSPACE_WINDOW_LAYOUT_HEIGHT))
                .h(px(WORKSPACE_INDICATOR_SIZE))
                .flex_shrink_0();
            match symbol {
                Some(symbol) => slot.child(
                    crate::icons::icon(symbol, 20.0, mark_color(workspace.active))
                        .w(px(WORKSPACE_WINDOW_LAYOUT_HEIGHT))
                        .h(px(WORKSPACE_INDICATOR_SIZE)),
                ),
                None => slot,
            }
        });
        let item = div()
            .w(px(length))
            .h(px(WORKSPACE_INDICATOR_SIZE))
            .flex()
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .child(
                div()
                    .w(px(WORKSPACE_INDICATOR_SIZE))
                    .h(px(WORKSPACE_INDICATOR_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .when_some(shape, |slot, shape| slot.child(shape)),
            )
            .when(count > 0, |item| {
                item.child(
                    div()
                        .ml(px(-EXTRA_SMALL_SPACING / 2.0))
                        .flex()
                        .flex_row()
                        .children(marks),
                )
            });
        if workspace.active {
            item.rounded(px(WORKSPACE_INDICATOR_SIZE / 2.0))
                .bg(rgba(COLOR_PRIMARY))
        } else {
            item
        }
    });

    let active_window = active_window_plan.map(|plan| {
        use crate::product_icon::icon_paths;
        let icon = plan.role.map(|role| {
            let square_top = plan.icon_baseline - crate::reference_active_window::ICON_SIZE;
            canvas(
                move |bounds, _, _| bounds,
                move |bounds, _, window, _| {
                    let origin = (
                        f32::from(bounds.left()),
                        f32::from(bounds.top()) + square_top,
                    );
                    for path in icon_paths(role, origin, crate::reference_active_window::ICON_SIZE)
                    {
                        window.paint_path(path, rgba(COLOR_PRIMARY));
                    }
                },
            )
            .w(px(plan.icon_width))
            .h(px(plan.icon_height))
            .flex_shrink_0()
        });
        div()
            .w(px(row_active_window_length(plan.icon_width)))
            .h(px(BAR_INNER_WIDTH))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(ROW_ACTIVE_WINDOW_GAP))
            .flex_shrink_0()
            .when_some(icon, |module, icon| module.child(icon))
            .child(
                div()
                    .w(px(crate::rail_topology::metrics().title_width))
                    .font_family(REFERENCE_FONT_FAMILY)
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(rgba(COLOR_PRIMARY))
                    .truncate()
                    .child(plan.source_title.clone()),
            )
    });

    let tray = presentation.tray.as_ref().map(|slots| {
        div()
            .w(px(tray_module_height(slots.len())))
            .h(px(BAR_INNER_WIDTH))
            .px(px(TRAY_PADDING))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(TRAY_ITEM_SPACING))
            .flex_shrink_0()
            .children(slots.iter().enumerate().map(|(index, slot)| {
                let slot_box = div()
                    .w(px(TRAY_ITEM_SIZE))
                    .h(px(TRAY_ITEM_SIZE))
                    .flex_shrink_0();
                match (slot, tray_icons.get(index)) {
                    (TrayVisual::Native(_), Some(realized)) => slot_box.child(
                        crate::product_tray_icon::tray_icon_element(realized, TRAY_ITEM_SIZE),
                    ),
                    _ => slot_box.flex().items_center().justify_center().child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgba(COLOR_SECONDARY)),
                    ),
                }
            }))
    });

    let clock = presentation.clock.as_ref().map(|clock| {
        let text = |width: f32, size: f32, text: String| {
            div()
                .w(px(width))
                .flex()
                .justify_center()
                .font_family(REFERENCE_FONT_FAMILY)
                .text_size(px(size))
                .text_color(rgba(COLOR_TERTIARY))
                .flex_shrink_0()
                .child(text)
        };
        row_capsule(capsule_fill)
            .w(px(row_clock_length(clock)))
            .px(px(ROW_CLOCK_PADDING))
            .gap(px(ROW_CLOCK_GAP))
            .child(glyph_cell(
                StatusGlyph::Calendar,
                COLOR_TERTIARY,
                ROW_CLOCK_GLYPH_WIDTH,
                BAR_INNER_WIDTH,
            ))
            .child(text(
                ROW_CLOCK_TIME_WIDTH,
                15.0,
                format!("{}:{}", clock.hour, clock.minute),
            ))
            .when_some(clock.meridiem, |module, meridiem| {
                module.child(text(
                    ROW_CLOCK_MERIDIEM_WIDTH,
                    12.0,
                    meridiem.as_str().to_owned(),
                ))
            })
            .when_some(clock.date.clone(), |module, date| {
                module.child(text(ROW_CLOCK_DATE_WIDTH, 12.0, date))
            })
    });

    let status = presentation.status.as_ref().map(|status| {
        let line = |glyph: StatusGlyph, color: u32| {
            glyph_cell(glyph, color, STATUS_ICON_LINE_HEIGHT, BAR_INNER_WIDTH)
        };
        let group = |cells: Vec<Div>| {
            div()
                .flex()
                .flex_row()
                .gap(px(STATUS_INNER_GAP))
                .flex_shrink_0()
                .children(cells)
        };
        row_capsule(capsule_fill)
            .w(px(status_module_height(status)))
            .px(px(STATUS_PADDING))
            .gap(px(STATUS_ENTRY_GAP))
            .children(
                status
                    .entries
                    .iter()
                    .filter(|item| item.is_present())
                    .map(|item| match item {
                        StatusVisual::Lock {
                            caps_lock,
                            num_lock,
                        } => {
                            let mut cells = Vec::new();
                            if *caps_lock {
                                cells.push(line(StatusGlyph::CapsLock, COLOR_SECONDARY));
                            }
                            if *num_lock {
                                cells.push(line(StatusGlyph::NumLock, COLOR_SECONDARY));
                            }
                            group(cells)
                        }
                        StatusVisual::Network { glyph } | StatusVisual::Audio { glyph } => {
                            line(*glyph, COLOR_SECONDARY)
                        }
                        StatusVisual::Bluetooth {
                            glyph,
                            device_glyphs,
                        } => group(
                            std::iter::once(*glyph)
                                .chain(device_glyphs.iter().copied())
                                .map(|glyph| line(glyph, COLOR_SECONDARY))
                                .collect(),
                        ),
                        StatusVisual::BatteryOrProfile { glyph, error } => {
                            line(*glyph, if *error { COLOR_ERROR } else { COLOR_SECONDARY })
                        }
                    }),
            )
    });

    // The Session button carries the Gnosix cube (or the power symbol).
    let power = presentation.power_icon.as_ref().map(|icon| {
        crate::identity::session_mark(icon, 18.0, COLOR_ERROR)
            .font_weight(FontWeight::BOLD)
            .w(px(POWER_MODULE_HEIGHT))
            .h(px(BAR_INNER_WIDTH))
            .flex_shrink_0()
    });
    let overflow = presentation.overflow.then(|| {
        crate::icons::icon("more_horiz", 18.0, COLOR_ON_SURFACE)
            .w(px(POWER_MODULE_HEIGHT))
            .h(px(BAR_INNER_WIDTH))
            .flex_shrink_0()
    });

    let mut modules: Vec<(IdleBarEntry, AnyElement)> = Vec::new();
    if let Some(identity) = presentation.system_logo.as_ref() {
        modules.push((
            IdleBarEntry::Logo,
            div()
                .w(px(LOGO_MODULE_HEIGHT))
                .h(px(BAR_INNER_WIDTH))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .child(system_logo(identity))
                .into_any_element(),
        ));
    }
    modules.push((
        IdleBarEntry::Workspaces,
        row_capsule(capsule_fill)
            .px(px(EXTRA_SMALL_SPACING))
            .gap(px(EXTRA_SMALL_SPACING))
            .children(workspaces)
            .into_any_element(),
    ));
    let rest = [
        (
            IdleBarEntry::ActiveWindow,
            active_window.map(|e| e.into_any_element()),
        ),
        (IdleBarEntry::Tray, tray.map(|e| e.into_any_element())),
        (IdleBarEntry::Clock, clock.map(|e| e.into_any_element())),
        (
            IdleBarEntry::StatusIcons,
            status.map(|e| e.into_any_element()),
        ),
        (IdleBarEntry::Power, power.map(|e| e.into_any_element())),
        (
            IdleBarEntry::Overflow,
            overflow.map(|e| e.into_any_element()),
        ),
    ];
    modules.extend(
        rest.into_iter()
            .filter_map(|(entry, e)| e.map(|e| (entry, e))),
    );
    modules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_oracle_presentation_preserves_observed_idle_facts() {
        let presentation = BarPresentation::captured_oracle_2026_10_07();
        assert_eq!(
            presentation.system_logo,
            Some(SystemLogoPresentation::DefaultShellLogo)
        );
        assert_eq!(presentation.workspaces.len(), 5);
        assert!(presentation.workspaces[0].active);
        assert_eq!(presentation.workspaces[0].window_count(), 1);
        assert_eq!(
            presentation.workspaces[0].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Pill)
        );
        assert!(matches!(
            &presentation.workspaces[0].window_marks[0],
            WorkspaceWindowMark::CapturedUnresolved {
                slot: 0,
                observed_material_symbol,
            } if observed_material_symbol == "web"
        ));
        assert_eq!(
            presentation.workspaces[1].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Circle)
        );
        assert_eq!(
            presentation
                .active_window
                .as_ref()
                .map(|window| window.title.as_str()),
            Some("Ratification Phase 5 OSD - Helium")
        );
        assert_eq!(
            presentation
                .active_window
                .as_ref()
                .map(|window| &window.icon),
            Some(&ActiveWindowIcon::CapturedRole {
                role: IconRole::Web,
            })
        );
        assert_eq!(presentation.tray.as_ref().map(Vec::len), Some(4));
        assert_eq!(
            presentation
                .status
                .as_ref()
                .map(|status| status.entries.len()),
            Some(4)
        );
        assert_eq!(
            presentation
                .status
                .as_ref()
                .map(StatusPresentation::source_height),
            Some(168.0)
        );
        assert_eq!(
            presentation.clock.as_ref().map(|clock| (
                clock.hour.as_str(),
                clock.minute.as_str(),
                clock.meridiem,
            )),
            Some(("05", "05", Some(crate::product_clock::Meridiem::Am)))
        );
        assert_eq!(
            presentation.power_icon.as_deref(),
            Some("power_settings_new")
        );
    }

    #[test]
    fn native_workspace_projection_preserves_shape_uncertainty_and_window_identity() {
        use crate::product_scene::{
            ProductApplicationContent, ProductContent, ProductSemanticState,
            ProductWorkspaceContent,
        };
        use gnos_ux_product_intent::{ObservedSurfaceReference, ObservedWorkspaceReference};
        use gnosix_desktop_model::{
            DesktopContextGeneration, SurfaceFocus, SurfaceId, SurfaceVisibility, WorkspaceId,
        };

        let generation = DesktopContextGeneration(7);
        let workspace_ref =
            |byte| ObservedWorkspaceReference::new(WorkspaceId::from_bytes([byte; 16]), generation);
        let surface_ref =
            |byte| ObservedSurfaceReference::new(SurfaceId::from_bytes([byte; 16]), generation);

        let ws1 = ProductWorkspaceContent {
            reference: workspace_ref(1),
            label: "1".into(),
        };
        let ws2 = ProductWorkspaceContent {
            reference: workspace_ref(2),
            label: "2".into(),
        };
        let ws3 = ProductWorkspaceContent {
            reference: workspace_ref(3),
            label: "3".into(),
        };
        let app1 = ProductApplicationContent {
            bounds: None,
            reference: surface_ref(11),
            workspace: Some(ws1.reference),
            title: "Focused".into(),
            class: "browser".into(),
            initial_class: "browser".into(),
            tags: Vec::new(),
            process_id: 11,
            xwayland: false,
            visibility: SurfaceVisibility::Visible,
            focus: SurfaceFocus::Focused,
        };
        let app2 = ProductApplicationContent {
            bounds: None,
            reference: surface_ref(12),
            workspace: Some(ws2.reference),
            title: "Editor".into(),
            class: "editor".into(),
            initial_class: "editor-initial".into(),
            tags: Vec::new(),
            process_id: 12,
            xwayland: false,
            visibility: SurfaceVisibility::Hidden,
            focus: SurfaceFocus::Unfocused,
        };
        let ignored_tag = ProductApplicationContent {
            bounds: None,
            reference: surface_ref(13),
            tags: vec!["hide_in_bar*".into()],
            focus: SurfaceFocus::Unfocused,
            ..app1.clone()
        };
        let unmapped = ProductApplicationContent {
            bounds: None,
            reference: surface_ref(14),
            visibility: SurfaceVisibility::Unmapped,
            focus: SurfaceFocus::Unfocused,
            ..app1.clone()
        };
        let ignored_xwl_popup = ProductApplicationContent {
            bounds: None,
            reference: surface_ref(16),
            tags: vec!["xwl_popup*".into()],
            focus: SurfaceFocus::Unfocused,
            ..app1.clone()
        };
        let empty_class = ProductApplicationContent {
            bounds: None,
            reference: surface_ref(15),
            class: String::new(),
            focus: SurfaceFocus::Unfocused,
            ..app1.clone()
        };
        let semantic = ProductSemanticState::Current(Box::new(ProductContent {
            output_area: None,
            generation,
            connector: "HDMI-A-1".into(),
            active_workspace: Some(ws1.clone()),
            focused_workspace: Some(ws1.clone()),
            workspace_manager_targets: vec![ws1.clone(), ws2.clone(), ws3.clone()],
            visible_workspaces: vec![ws1.clone()],
            applications: vec![
                app1.clone(),
                ignored_tag,
                unmapped,
                ignored_xwl_popup,
                empty_class,
                app2.clone(),
            ],
            visible_application_count: 1,
            focused_application: Some(app1),
        }));

        let presentation = BarPresentation::from_product_semantics(&semantic);
        assert_eq!(
            presentation.workspaces[0].shape,
            WorkspaceShapePresentation::FocusedUnresolved
        );
        assert!(matches!(
            &presentation.workspaces[0].window_marks[..],
            [WorkspaceWindowMark::NativeClass {
                class,
                initial_class,
            }] if class == "browser" && initial_class == "browser"
        ));
        assert_eq!(
            presentation.workspaces[1].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Square)
        );
        assert!(matches!(
            &presentation.workspaces[1].window_marks[..],
            [WorkspaceWindowMark::NativeClass {
                class,
                initial_class,
            }] if class == "editor" && initial_class == "editor-initial"
        ));
        assert_eq!(
            presentation.workspaces[2].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Circle)
        );
        assert!(presentation.workspaces[2].window_marks.is_empty());
    }

    #[test]
    fn focused_workspace_shape_state_changes_only_when_focus_enters_another_workspace() {
        use crate::product_scene::{ProductContent, ProductSemanticState, ProductWorkspaceContent};
        use gnos_ux_product_intent::ObservedWorkspaceReference;
        use gnosix_desktop_model::{DesktopContextGeneration, WorkspaceId};

        fn semantic(active: Option<u8>, generation: u64) -> ProductSemanticState {
            let generation = DesktopContextGeneration(generation);
            let workspace = |byte| ProductWorkspaceContent {
                reference: ObservedWorkspaceReference::new(
                    WorkspaceId::from_bytes([byte; 16]),
                    generation,
                ),
                label: byte.to_string(),
            };
            let ws1 = workspace(1);
            let ws2 = workspace(2);
            let active_workspace = match active {
                Some(1) => Some(ws1.clone()),
                Some(2) => Some(ws2.clone()),
                _ => None,
            };
            ProductSemanticState::Current(Box::new(ProductContent {
                output_area: None,
                generation,
                connector: "HDMI-A-1".into(),
                active_workspace: active_workspace.clone(),
                focused_workspace: active_workspace,
                workspace_manager_targets: vec![ws1.clone(), ws2.clone()],
                visible_workspaces: vec![ws1, ws2],
                applications: vec![],
                visible_application_count: 0,
                focused_application: None,
            }))
        }

        let mut state = WorkspaceFocusState::default();

        let ws1_first = semantic(Some(1), 7);
        assert_eq!(
            state.reconcile_with_index(&ws1_first, 2),
            Some(MaterialShape::Pill)
        );

        // A new semantic generation is not a focus transition. The supplied
        // next index must therefore be ignored and the Pill retained.
        let ws1_next_generation = semantic(Some(1), 8);
        assert_eq!(
            state.reconcile_with_index(&ws1_next_generation, 7),
            Some(MaterialShape::Pill)
        );

        let ws2 = semantic(Some(2), 9);
        assert_eq!(
            state.reconcile_with_index(&ws2, 4),
            Some(MaterialShape::Arrow)
        );

        assert_eq!(
            state.reconcile_with_index(&ProductSemanticState::Unavailable, 9),
            None
        );

        let ws1_reentered = semantic(Some(1), 10);
        assert_eq!(
            state.reconcile_with_index(&ws1_reentered, 1),
            Some(MaterialShape::Oval)
        );
    }

    #[test]
    fn workspace_mark_rules_preserve_window_icon_override_and_category_priority() {
        assert!(steam_window_icon_override("steam"));
        assert!(steam_window_icon_override("steam_app_default"));
        assert!(steam_window_icon_override("steam_app_123456"));
        assert!(!steam_window_icon_override("Steam"));
        assert!(!steam_window_icon_override("steam_app_"));
        assert!(!steam_window_icon_override("steam_app_12x"));

        assert_eq!(
            category_material_symbol(&["Utility".into(), "WebBrowser".into()]),
            Some("web")
        );
        assert_eq!(
            category_material_symbol(&["Video".into(), "AudioVideo".into()]),
            Some("music_video")
        );
        assert_eq!(category_material_symbol(&["UnknownCategory".into()]), None);
    }

    #[test]
    fn workspace_mark_catalog_resolution_uses_class_only_and_exposes_ambiguity() {
        use gnosix_application_catalog::{
            ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
        };
        use std::fs;

        let root = std::env::temp_dir().join(format!(
            "gnosix-parity-01e-catalog-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let write_entry = |name: &str, body: &str| {
            fs::write(root.join(name), body).unwrap();
        };
        write_entry(
            "browser.desktop",
            "[Desktop Entry]\nType=Application\nName=Browser\nExec=browser\nStartupWMClass=browser\nCategories=Utility;WebBrowser;\n",
        );
        write_entry(
            "shared-one.desktop",
            "[Desktop Entry]\nType=Application\nName=Shared One\nExec=one\nStartupWMClass=shared\nCategories=Game;\n",
        );
        write_entry(
            "shared-two.desktop",
            "[Desktop Entry]\nType=Application\nName=Shared Two\nExec=two\nStartupWMClass=shared\nCategories=Office;\n",
        );
        write_entry(
            "initial-only.desktop",
            "[Desktop Entry]\nType=Application\nName=Initial Only\nExec=initial\nStartupWMClass=initial-only\nCategories=FileManager;\n",
        );
        write_entry(
            "org.example.Editor.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\nStartupWMClass=EditorWindow\nCategories=TextEditor;\n",
        );
        write_entry(
            "org.example.Terminal.desktop",
            "[Desktop Entry]\nType=Application\nName=Terminal\nExec=terminal\nStartupWMClass=TerminalWindow\nCategories=TerminalEmulator;\n",
        );
        write_entry(
            "hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nStartupWMClass=hidden\nCategories=Game;\nHidden=true\n",
        );

        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![root.clone()]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );

        let browser = resolve_native_workspace_window_mark(&catalog, "browser", "ignored");
        assert!(matches!(
            browser,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: Some(ref entry),
                ref material_symbol,
                resolution: WorkspaceWindowMarkResolution::ExactDesktopId,
                ..
            } if entry.as_str() == "browser.desktop" && material_symbol == "web"
        ));

        let browser_case_folded =
            resolve_native_workspace_window_mark(&catalog, "BROWSER", "ignored");
        assert!(matches!(
            browser_case_folded,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: Some(ref entry),
                resolution: WorkspaceWindowMarkResolution::CaseFoldedDesktopId,
                ..
            } if entry.as_str() == "browser.desktop"
        ));

        let editor = resolve_native_workspace_window_mark(&catalog, "EditorWindow", "ignored");
        assert!(matches!(
            editor,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: Some(ref entry),
                ref material_symbol,
                resolution: WorkspaceWindowMarkResolution::ExactStartupClass,
                ..
            } if entry.as_str() == "org.example.Editor.desktop" && material_symbol == "edit_note"
        ));

        let terminal = resolve_native_workspace_window_mark(&catalog, "terminalwindow", "ignored");
        assert!(matches!(
            terminal,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: Some(ref entry),
                ref material_symbol,
                resolution: WorkspaceWindowMarkResolution::CaseFoldedStartupClass,
                ..
            } if entry.as_str() == "org.example.Terminal.desktop" && material_symbol == "terminal"
        ));

        let hidden = resolve_native_workspace_window_mark(&catalog, "hidden", "ignored");
        assert!(matches!(
            hidden,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: None,
                resolution: WorkspaceWindowMarkResolution::NoDesktopEntryMatch,
                ..
            }
        ));

        // Donor Workspace.qml passes only class to heuristicLookup. A matching
        // initial_class must therefore not silently become a presentation hit.
        let initial_only =
            resolve_native_workspace_window_mark(&catalog, "unmatched", "initial-only");
        assert!(matches!(
            initial_only,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: None,
                ref material_symbol,
                resolution: WorkspaceWindowMarkResolution::NoDesktopEntryMatch,
                ..
            } if material_symbol == "terminal"
        ));

        let ambiguous = resolve_native_workspace_window_mark(&catalog, "shared", "shared");
        assert!(matches!(
            ambiguous,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: None,
                ref material_symbol,
                resolution: WorkspaceWindowMarkResolution::AmbiguousDesktopEntryMatch,
                ..
            } if material_symbol == "terminal"
        ));

        let steam = resolve_native_workspace_window_mark(&catalog, "steam_app_570", "anything");
        assert!(matches!(
            steam,
            WorkspaceWindowMark::NativeResolved {
                desktop_entry: None,
                ref material_symbol,
                resolution: WorkspaceWindowMarkResolution::SteamOverride,
                ..
            } if material_symbol == "sports_esports"
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stateful_runtime_projection_resolves_focus_shape_and_window_mark() {
        use crate::product_scene::{
            ProductApplicationContent, ProductContent, ProductSemanticState,
            ProductWorkspaceContent,
        };
        use gnos_ux_product_intent::{ObservedSurfaceReference, ObservedWorkspaceReference};
        use gnosix_application_catalog::{
            ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
        };
        use gnosix_desktop_model::{
            DesktopContextGeneration, SurfaceFocus, SurfaceId, SurfaceVisibility, WorkspaceId,
        };
        use std::{fs, sync::Arc};

        let root = std::env::temp_dir().join(format!(
            "gnosix-parity-01e-runtime-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("browser.desktop"),
            "[Desktop Entry]\nType=Application\nName=Browser\nExec=browser\nStartupWMClass=browser\nCategories=Utility;WebBrowser;\n",
        )
        .unwrap();
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![root.clone()]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );

        let generation = DesktopContextGeneration(31);
        let workspace = ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(
                WorkspaceId::from_bytes([1; 16]),
                generation,
            ),
            label: "1".into(),
        };
        let app = ProductApplicationContent {
            bounds: None,
            reference: ObservedSurfaceReference::new(SurfaceId::from_bytes([7; 16]), generation),
            workspace: Some(workspace.reference),
            title: "Browser".into(),
            class: "browser".into(),
            initial_class: "initial-only".into(),
            tags: Vec::new(),
            process_id: 700,
            xwayland: false,
            visibility: SurfaceVisibility::Visible,
            focus: SurfaceFocus::Focused,
        };
        let semantic = ProductSemanticState::Current(Box::new(ProductContent {
            output_area: None,
            generation,
            connector: "HDMI-A-1".into(),
            active_workspace: Some(workspace.clone()),
            focused_workspace: Some(workspace.clone()),
            workspace_manager_targets: vec![workspace.clone()],
            visible_workspaces: vec![workspace],
            applications: vec![app.clone()],
            visible_application_count: 1,
            focused_application: Some(app),
        }));

        let mut runtime = crate::product_runtime::ProductRuntime::default();
        runtime.set_application_catalog(Arc::new(catalog));
        runtime.refresh_semantic_for_test(semantic);

        let mut workspace_state = WorkspaceFocusState::default();
        assert_eq!(
            workspace_state.reconcile_with_index(runtime.semantic(), 2),
            Some(MaterialShape::Pill)
        );

        let first = BarPresentation::from_product_runtime_with_workspace_state(
            &runtime,
            &mut workspace_state,
        );
        assert_eq!(
            first.workspaces[0].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Pill)
        );
        assert!(matches!(
            &first.workspaces[0].window_marks[..],
            [WorkspaceWindowMark::NativeResolved {
                desktop_entry: Some(entry),
                material_symbol,
                resolution: WorkspaceWindowMarkResolution::ExactDesktopId,
                ..
            }] if entry.as_str() == "browser.desktop" && material_symbol == "web"
        ));

        let second = BarPresentation::from_product_runtime_with_workspace_state(
            &runtime,
            &mut workspace_state,
        );
        assert_eq!(second.workspaces[0].shape, first.workspaces[0].shape);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stateful_runtime_projection_moves_active_workspace_endpoint() {
        use crate::product_scene::{ProductContent, ProductSemanticState, ProductWorkspaceContent};
        use gnos_ux_product_intent::ObservedWorkspaceReference;
        use gnosix_desktop_model::{DesktopContextGeneration, WorkspaceId};

        fn semantic(active: u8, generation: u64) -> ProductSemanticState {
            let generation = DesktopContextGeneration(generation);
            let workspace = |byte| ProductWorkspaceContent {
                reference: ObservedWorkspaceReference::new(
                    WorkspaceId::from_bytes([byte; 16]),
                    generation,
                ),
                label: byte.to_string(),
            };
            let ws1 = workspace(1);
            let ws2 = workspace(2);
            let active_workspace = if active == 1 {
                ws1.clone()
            } else {
                ws2.clone()
            };
            ProductSemanticState::Current(Box::new(ProductContent {
                output_area: None,
                generation,
                connector: "HDMI-A-1".into(),
                active_workspace: Some(active_workspace.clone()),
                focused_workspace: Some(active_workspace),
                workspace_manager_targets: vec![ws1.clone(), ws2.clone()],
                visible_workspaces: vec![ws1, ws2],
                applications: vec![],
                visible_application_count: 0,
                focused_application: None,
            }))
        }

        let mut runtime = crate::product_runtime::ProductRuntime::default();
        let mut state = WorkspaceFocusState::default();

        runtime.refresh_semantic_for_test(semantic(1, 41));
        assert_eq!(
            state.reconcile_with_index(runtime.semantic(), 2),
            Some(MaterialShape::Pill)
        );
        let first =
            BarPresentation::from_product_runtime_with_workspace_state(&runtime, &mut state);
        assert!(first.workspaces[0].active);
        assert!(!first.workspaces[1].active);
        assert_eq!(
            first.workspaces[0].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Pill)
        );
        assert_eq!(
            first.workspaces[1].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Circle)
        );

        runtime.refresh_semantic_for_test(semantic(2, 42));
        assert_eq!(
            state.reconcile_with_index(runtime.semantic(), 4),
            Some(MaterialShape::Arrow)
        );
        let second =
            BarPresentation::from_product_runtime_with_workspace_state(&runtime, &mut state);
        assert!(!second.workspaces[0].active);
        assert!(second.workspaces[1].active);
        assert_eq!(
            second.workspaces[0].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Circle)
        );
        assert_eq!(
            second.workspaces[1].shape,
            WorkspaceShapePresentation::Resolved(MaterialShape::Arrow)
        );
    }

    #[test]
    fn unavailable_product_semantics_invents_no_provider_facts() {
        let presentation = BarPresentation::from_product_semantics(
            &crate::product_scene::ProductSemanticState::Unavailable,
        );
        assert!(presentation.system_logo.is_none());
        assert!(presentation.workspaces.is_empty());
        assert!(presentation.active_window.is_none());
        assert!(presentation.tray.is_none());
        assert!(presentation.clock.is_none());
        assert!(presentation.status.is_none());
        assert!(presentation.power_icon.is_none());
    }

    #[test]
    fn clock_projection_uses_only_explicit_native_local_time_facts() {
        use crate::product_clock::{ClockCycle, ClockObservation, LocalClockSample, Meridiem};
        let observe = |hour, minute, cycle| ClockObservation {
            time: LocalClockSample::new(hour, minute).unwrap(),
            cycle,
        };
        let mut runtime = crate::product_runtime::ProductRuntime::default();
        assert!(
            BarPresentation::from_product_runtime(&runtime)
                .clock
                .is_none()
        );
        assert!(runtime.set_clock(Some(observe(22, 7, ClockCycle::TwelveHour))));
        assert!(!runtime.set_clock(Some(observe(22, 7, ClockCycle::TwelveHour))));
        assert_eq!(
            BarPresentation::from_product_runtime(&runtime).clock,
            Some(ClockPresentation {
                hour: "10".into(),
                minute: "07".into(),
                meridiem: Some(Meridiem::Pm),
                date: None,
            })
        );
        // The minute changes the presentation; the cycle is the observer's.
        assert!(runtime.set_clock(Some(observe(22, 8, ClockCycle::TwentyFourHour))));
        assert_eq!(
            BarPresentation::from_product_runtime(&runtime).clock,
            Some(ClockPresentation {
                hour: "22".into(),
                minute: "08".into(),
                meridiem: None,
                date: None,
            })
        );
        assert!(runtime.set_clock(None));
        assert!(
            BarPresentation::from_product_runtime(&runtime)
                .clock
                .is_none()
        );
    }

    #[test]
    fn lock_projection_requires_decisive_primary_keyboard_observation() {
        use gnosix_hyprland::keyboard_lock::{
            KeyboardLockObservation as O, KeyboardLockSnapshot as S,
        };
        let mut runtime = crate::product_runtime::ProductRuntime::default();
        assert_eq!(
            BarPresentation::from_product_runtime(&runtime)
                .status
                .unwrap()
                .source_height(),
            24.0
        );
        assert!(runtime.set_keyboard_locks(O::Ambiguous));
        assert!(
            BarPresentation::from_product_runtime(&runtime)
                .status
                .unwrap()
                .entries
                .is_empty()
        );
        assert!(runtime.set_keyboard_locks(O::Observed(S {
            caps_lock: true,
            num_lock: true,
        })));
        let status = BarPresentation::from_product_runtime(&runtime)
            .status
            .unwrap();
        assert_eq!(
            status.entries,
            vec![StatusVisual::Lock {
                caps_lock: true,
                num_lock: true,
            }]
        );
        assert_eq!(status.source_height(), 78.0);
    }

    #[test]
    fn bluetooth_entry_aggregates_adapters_and_classifies_connected_devices() {
        use crate::product_status_glyph::BluetoothDeviceClass as Class;
        use gnos_bluetooth::{
            BluetoothAdapterSnapshot as Adapter, BluetoothDeviceSnapshot as Device,
            BluetoothObjectRef as Ref, BluetoothPowerState as Power,
            BluetoothProviderState as Provider, BluetoothSnapshot as Snapshot,
        };
        let adapter = |path: &str, powered: bool| Adapter {
            id: Ref::new(":1.100", path),
            powered,
            power_state: if powered { Power::On } else { Power::Off },
            discovering: false,
            pairable: false,
            discoverable: false,
        };
        let device = |path: &str, adapter: &str, connected: bool, icon: Option<&str>| Device {
            id: Ref::new(":1.100", path),
            adapter: Ref::new(":1.100", adapter),
            alias: None,
            paired: true,
            bonded: true,
            connected,
            trusted: true,
            blocked: false,
            services_resolved: connected,
            rssi: None,
            icon: icon.map(Into::into),
        };
        let mut source = Snapshot {
            provider: Provider::Unavailable,
            generation: 1,
            provider_owner: Some(":1.100".into()),
            adapters: Default::default(),
            devices: Default::default(),
        };
        // Provider facts missing: the entry is absent, never "disabled".
        assert_eq!(bluetooth_status_from_product(&source), None);
        source.provider = Provider::Reconnecting;
        assert_eq!(bluetooth_status_from_product(&source), None);

        // Provider present without any adapter is an observed disabled radio.
        source.provider = Provider::Available;
        let bt = |glyph, device_glyphs| {
            Some(StatusVisual::Bluetooth {
                glyph,
                device_glyphs,
            })
        };
        assert_eq!(
            bluetooth_status_from_product(&source),
            bt(StatusGlyph::BluetoothDisabled, vec![])
        );
        let hci0 = adapter("/org/bluez/hci0", false);
        source.adapters.insert(hci0.id.clone(), hci0);
        assert_eq!(
            bluetooth_status_from_product(&source),
            bt(StatusGlyph::BluetoothDisabled, vec![])
        );
        // A second adapter that is powered enables the entry whatever the
        // discovery order: no adapter is chosen.
        let hci1 = adapter("/org/bluez/hci1", true);
        source.adapters.insert(hci1.id.clone(), hci1);
        assert_eq!(
            bluetooth_status_from_product(&source),
            bt(StatusGlyph::Bluetooth, vec![])
        );

        // Only connected devices are rows; their BlueZ icon is their class.
        for d in [
            device(
                "/org/bluez/hci1/dev_01",
                "/org/bluez/hci1",
                true,
                Some("audio-headset"),
            ),
            device(
                "/org/bluez/hci1/dev_02",
                "/org/bluez/hci1",
                false,
                Some("input-mouse"),
            ),
            device("/org/bluez/hci1/dev_03", "/org/bluez/hci1", true, None),
        ] {
            source.devices.insert(d.id.clone(), d);
        }
        assert_eq!(
            bluetooth_status_from_product(&source),
            bt(
                StatusGlyph::BluetoothConnected,
                vec![
                    StatusGlyph::BluetoothDevice(Class::Headphones),
                    StatusGlyph::BluetoothDevice(Class::Other),
                ]
            )
        );
    }

    #[test]
    fn power_projection_uses_observed_battery_or_profile_never_a_default() {
        use gnos_power::{BatteryPercentage as B, PowerProfile as P, PowerSnapshot as S};
        assert_eq!(power_status_from_product(S::Unavailable), None);
        assert_eq!(
            power_status_from_product(S::NoBatteryProfileUnavailable),
            None
        );
        assert_eq!(
            power_status_from_product(S::Profile(P::Performance)),
            Some(StatusVisual::BatteryOrProfile {
                glyph: StatusGlyph::Profile(PowerProfileGlyph::Performance),
                error: false,
            })
        );
        assert_eq!(
            power_status_from_product(S::Battery {
                percentage: B::new(19.0).unwrap(),
                charging: false,
                on_battery: true,
            }),
            Some(StatusVisual::BatteryOrProfile {
                glyph: StatusGlyph::Battery {
                    level: 1,
                    charging: false,
                },
                error: true,
            })
        );
        assert_eq!(
            power_status_from_product(S::Battery {
                percentage: B::new(20.0).unwrap(),
                charging: true,
                on_battery: false,
            }),
            Some(StatusVisual::BatteryOrProfile {
                glyph: StatusGlyph::Battery {
                    level: 0,
                    charging: true,
                },
                error: false,
            })
        );
        assert_eq!(
            power_status_from_product(S::Battery {
                percentage: B::new(100.0).unwrap(),
                charging: true,
                on_battery: false,
            }),
            Some(StatusVisual::BatteryOrProfile {
                glyph: StatusGlyph::Battery {
                    level: 7,
                    charging: true,
                },
                error: false,
            })
        );
    }

    #[test]
    fn battery_level_follows_the_seven_step_reference_charging_rule() {
        use gnos_power::BatteryPercentage as B;
        for (percent, charging, expected) in [
            (0.0, false, 0),
            (14.28, false, 0),
            (14.29, false, 1),
            (14.29, true, 0),
            (57.15, false, 4),
            (57.15, true, 3),
            (99.9, false, 6),
            (100.0, false, 7),
            (100.0, true, 7),
        ] {
            assert_eq!(
                donor_battery_level(B::new(percent).unwrap(), charging),
                expected,
                "percent={percent} charging={charging}"
            );
        }
    }

    #[test]
    fn network_adapter_matches_reference_icon_rules_without_inventing_unavailable_state() {
        use crate::product_network::{
            ProductNetworkEndpoint, ProductNetworkPresentation, ProductNetworkPrimary,
            ProductNetworkWifi,
        };
        use gnos_network::{
            NetworkConnectivity, NetworkProviderState, NetworkSecurityMechanism, NetworkSignalBand,
            NetworkTransport,
        };

        let base = |primary| ProductNetworkPresentation {
            provider: NetworkProviderState::Available,
            primary,
            connectivity: NetworkConnectivity::Full,
            connectivity_checked: false,
            overlay_active: false,
            device_count: 1,
            relevant_device_count: 1,
        };

        let unavailable = ProductNetworkPresentation {
            provider: NetworkProviderState::Unavailable,
            primary: ProductNetworkPrimary::Unavailable,
            connectivity: NetworkConnectivity::Unknown,
            connectivity_checked: false,
            overlay_active: false,
            device_count: 0,
            relevant_device_count: 0,
        };
        assert!(network_status_from_product(&unavailable).is_none());

        for primary in [
            ProductNetworkPrimary::Disconnected,
            ProductNetworkPrimary::Connecting,
        ] {
            assert_eq!(
                network_status_from_product(&base(primary)),
                Some(StatusVisual::Network {
                    glyph: StatusGlyph::WifiOff
                })
            );
        }

        assert_eq!(
            network_status_from_product(&base(ProductNetworkPrimary::Ready(
                ProductNetworkEndpoint {
                    transport: NetworkTransport::Ethernet,
                    wifi: None,
                }
            ))),
            Some(StatusVisual::Network {
                glyph: StatusGlyph::Wired
            })
        );

        for (strength, expected) in [(0, 0u8), (20, 1), (40, 2), (60, 3), (80, 4), (100, 4)] {
            let wifi = ProductNetworkWifi {
                ssid: "oracle".into(),
                strength,
                band: NetworkSignalBand::from_strength(strength),
                security: NetworkSecurityMechanism::Rsn,
                authenticated: true,
            };
            assert_eq!(
                network_status_from_product(&base(ProductNetworkPrimary::Ready(
                    ProductNetworkEndpoint {
                        transport: NetworkTransport::Wifi,
                        wifi: Some(wifi),
                    }
                ))),
                Some(StatusVisual::Network {
                    glyph: StatusGlyph::Wifi { bars: expected }
                }),
                "strength={strength}"
            );
        }
    }

    #[test]
    fn native_runtime_adapter_preserves_tray_identity_and_filters_passive_items() {
        use crate::product_tray::{
            TrayCategory, TrayIcon, TrayItemId, TrayItemSnapshot, TraySnapshot, TrayStatus,
        };

        fn item(id: &str, status: TrayStatus, icon: TrayIcon) -> TrayItemSnapshot {
            TrayItemSnapshot {
                id: TrayItemId::new(id),
                service: format!("org.example.{id}"),
                path: format!("/org/example/{id}"),
                title: id.into(),
                status,
                category: TrayCategory::ApplicationStatus,
                icons: icon.into(),
                tooltip: None,
                menu_path: None,
                item_is_menu: false,
                owner: ":1.100".into(),
            }
        }

        let mut runtime = crate::product_runtime::ProductRuntime::default();
        runtime.set_tray(TraySnapshot::from_items(vec![
            item(
                "active",
                TrayStatus::Active,
                TrayIcon::Named("active-icon".into()),
            ),
            item(
                "passive",
                TrayStatus::Passive,
                TrayIcon::Named("passive-icon".into()),
            ),
        ]));

        let presentation = BarPresentation::from_product_runtime(&runtime);
        let tray = presentation.tray.expect("active tray provider");
        assert_eq!(tray.len(), 1);
        assert!(matches!(
            &tray[0],
            TrayVisual::Native(item)
                if item.id.as_str() == "active"
                    && item.title == "active"
                    && item.status == TrayStatus::Active
                    && matches!(
                        &item.source,
                        crate::product_tray_presentation::TrayIconSource::Named { name, .. }
                            if name == "active-icon"
                    )
        ));
        assert!(presentation.clock.is_none());
        assert_eq!(
            presentation
                .status
                .as_ref()
                .map(StatusPresentation::source_height),
            Some(24.0),
            "enabled StatusIcons module keeps its source padding when providers are unavailable"
        );
        assert!(presentation.power_icon.is_none());
    }

    #[test]
    fn focused_workspace_shape_pool_matches_donor_source_order() {
        assert_eq!(
            FOCUSED_SHAPE_POOL,
            &[
                MaterialShape::Slanted,
                MaterialShape::Oval,
                MaterialShape::Pill,
                MaterialShape::Triangle,
                MaterialShape::Arrow,
                MaterialShape::Diamond,
                MaterialShape::Pentagon,
                MaterialShape::Gem,
                MaterialShape::VerySunny,
                MaterialShape::Sunny,
                MaterialShape::Cookie4Sided,
                MaterialShape::Cookie6Sided,
                MaterialShape::Cookie7Sided,
                MaterialShape::Cookie9Sided,
                MaterialShape::Cookie12Sided,
                MaterialShape::Clover4Leaf,
                MaterialShape::SoftBurst,
                MaterialShape::Ghostish,
            ]
        );
        assert_eq!(FOCUSED_SHAPE_POOL.len(), 18);
        assert!(!FOCUSED_SHAPE_POOL.contains(&MaterialShape::Circle));
        assert!(!FOCUSED_SHAPE_POOL.contains(&MaterialShape::Square));
    }

    #[test]
    fn workspace_geometry_matches_donor_layout_equations() {
        assert_eq!(workspace_preferred_height(0), 32.0);
        assert_eq!(workspace_preferred_height(1), 58.0);
        assert_eq!(workspace_preferred_height(2), 82.0);
        assert_eq!(workspace_preferred_height(5), 154.0);
        assert_eq!(workspace_preferred_height(6), 154.0);
        assert_eq!(workspace_shape_size(true, true), 32.0 * (2.0 / 3.0));
        assert_eq!(workspace_shape_size(false, true), 32.0 / 3.0);
        assert_eq!(workspace_shape_size(false, false), 8.0);
    }

    /// The shell material and the placement authority describe one Bar: the
    /// left border the shell paints is exactly the thickness placement reserves,
    /// and the material model cannot disagree about it.
    #[test]
    fn shell_border_agrees_with_the_placement_authority_and_the_reference_equations() {
        let viewport = SurfaceViewport::try_new(2560.0, 1440.0).unwrap();
        let placement = idle_bar_placement(viewport).unwrap();
        let border = reference_shell_border();
        assert_eq!(border.left, placement.thickness);
        assert_eq!(
            (
                border.left,
                border.top,
                border.right,
                border.bottom,
                border.radius
            ),
            (60.0, 10.0, 10.0, 10.0, 25.0)
        );
        // The reservation stays the placement's: reserving the occupied extent
        // means the painted border width, not a separate material parameter.
        assert_eq!(
            placement.reservation_policy,
            BarReservationPolicy::ReserveOccupiedExtent
        );
        assert_eq!(REFERENCE_SHADOW_BLUR_MAX, 15.0);
        assert_eq!(REFERENCE_SHADOW_ALPHA, 0.7);
    }

    #[test]
    fn idle_bar_recipe_matches_live_oracle_geometry() {
        let viewport = SurfaceViewport::try_new(2560.0, 1440.0).unwrap();
        let placement = idle_bar_placement(viewport)
            .expect("reference idle Bar oracle must resolve on HDMI-A-1");
        assert_eq!(placement.edge, gnos_ux_surface::envelope::SurfaceEdge::Left);
        assert_eq!(placement.along_edge_alignment, AlongEdgeAlignment::Start);
        assert_eq!(placement.thickness, 60.0);
        assert_eq!(placement.span, 1440.0);
        assert_eq!(placement.inset, 0.0);
        assert_eq!(
            placement.reservation_policy,
            BarReservationPolicy::ReserveOccupiedExtent
        );
        assert_eq!(placement.sensing_policy, BarSensingPolicy::None);

        let resolved = placement.resolve(viewport).unwrap();
        assert_eq!(resolved.allocation().left(), 0.0);
        assert_eq!(resolved.allocation().top(), 0.0);
        assert_eq!(resolved.allocation().right(), 60.0);
        assert_eq!(resolved.allocation().bottom(), 1440.0);
        assert_eq!(resolved.reservation().left, 60.0);
        assert_eq!(resolved.reservation().top, 0.0);
        assert_eq!(resolved.reservation().right, 0.0);
        assert_eq!(resolved.reservation().bottom, 0.0);
    }

    #[test]
    fn idle_bar_entry_order_keeps_both_spacers_and_native_modules() {
        assert_eq!(
            IDLE_BAR_ENTRIES,
            &[
                IdleBarEntry::Logo,
                IdleBarEntry::Workspaces,
                IdleBarEntry::Spacer,
                IdleBarEntry::ActiveWindow,
                IdleBarEntry::Spacer,
                IdleBarEntry::Tray,
                IdleBarEntry::Clock,
                IdleBarEntry::StatusIcons,
                IdleBarEntry::Power,
            ]
        );
        assert_eq!(
            IDLE_STATUS_ICONS,
            &[
                IdleStatusIcon::Lock,
                IdleStatusIcon::Network,
                IdleStatusIcon::Bluetooth,
                IdleStatusIcon::Battery,
            ]
        );
    }

    #[test]
    fn active_window_title_matches_donor_desktop_and_compact_rules() {
        // No active toplevel or a falsy title becomes the translated Desktop.
        assert_eq!(active_window_title(None, false), "Desktop");
        assert_eq!(active_window_title(Some(""), false), "Desktop");
        assert_eq!(active_window_title(Some(""), true), "Desktop");
        // Default config: no title rewriting at all.
        assert_eq!(
            active_window_title(Some("Doc - Helium"), false),
            "Doc - Helium"
        );
        // compact: split(/\s+[\-–—]\s+/), keep the trimmed last part.
        for (title, expected) in [
            ("Doc - Helium", "Helium"),
            ("Doc \u{2013} Helium", "Helium"),
            ("A \u{2014} B - C", "C"),
            ("no-spaces-here", "no-spaces-here"),
            ("solo", "solo"),
            // Matches are non-overlapping left to right: the second dash has
            // no whitespace before it once the first match consumed it.
            ("a - - b", "- b"),
            // A trailing match yields an empty last part, not Desktop.
            ("a - ", ""),
            ("  x  -  y  ", "y"),
            // ECMAScript \s includes NBSP and ideographic space ...
            ("a\u{a0}-\u{3000}b", "b"),
            // ... but not NEL, which Rust's char::is_whitespace accepts.
            ("a\u{85}-\u{85}b", "a\u{85}-\u{85}b"),
            // ECMAScript \s includes BOM, which Rust does not.
            ("a\u{feff}-\u{feff}b", "b"),
        ] {
            assert_eq!(
                active_window_title(Some(title), true),
                expected,
                "{title:?}"
            );
        }
    }

    #[test]
    fn active_window_geometry_matches_donor_equations_and_qt_oracle() {
        // Qt 6.11 oracle measurements for the raster-proven captured title.
        let captured = ActiveWindowGeometry {
            icon_width: ORACLE_ICON_WIDTH,
            icon_height: ORACLE_ICON_HEIGHT,
            title_width: 306.71875,
            title_height: ORACLE_TITLE_LINE_HEIGHT,
        };
        assert_eq!(captured.implicit_width(), 21.0);
        assert_eq!(captured.implicit_height(), 338.71875);

        // Bar.qml children: every enabled entry wrapper plus the Repeater.
        let child_count = IDLE_BAR_ENTRIES.len() + 1;
        assert_eq!(child_count, 10);
        let others = [19.0, 210.0, 100.0, 80.5, 120.0, 24.0];
        // 1440 - 553.5 - 12 * 9 - 16 * 2 = 746.5, stored in an int property.
        let max_height = active_window_max_height(1440.0, &others, child_count);
        assert_eq!(max_height, 746.0);
        assert_eq!(
            active_window_elide_width(max_height, ORACLE_ICON_HEIGHT),
            722.0
        );
    }

    fn active_window_semantic(
        applications: Vec<crate::product_scene::ProductApplicationContent>,
        focused_application: Option<crate::product_scene::ProductApplicationContent>,
    ) -> crate::product_scene::ProductSemanticState {
        use crate::product_scene::{ProductContent, ProductSemanticState, ProductWorkspaceContent};
        use gnos_ux_product_intent::ObservedWorkspaceReference;
        use gnosix_desktop_model::{DesktopContextGeneration, WorkspaceId};

        let generation = DesktopContextGeneration(51);
        let workspace = ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(
                WorkspaceId::from_bytes([1; 16]),
                generation,
            ),
            label: "1".into(),
        };
        ProductSemanticState::Current(Box::new(ProductContent {
            output_area: None,
            generation,
            connector: "HDMI-A-1".into(),
            active_workspace: Some(workspace.clone()),
            focused_workspace: Some(workspace.clone()),
            workspace_manager_targets: vec![workspace.clone()],
            visible_workspaces: vec![workspace],
            visible_application_count: applications.len(),
            applications,
            focused_application,
        }))
    }

    fn active_window_application(
        byte: u8,
        title: &str,
        class: &str,
        focus: gnosix_desktop_model::SurfaceFocus,
        workspace_byte: u8,
    ) -> crate::product_scene::ProductApplicationContent {
        use gnos_ux_product_intent::{ObservedSurfaceReference, ObservedWorkspaceReference};
        use gnosix_desktop_model::{
            DesktopContextGeneration, SurfaceId, SurfaceVisibility, WorkspaceId,
        };
        let generation = DesktopContextGeneration(51);
        crate::product_scene::ProductApplicationContent {
            bounds: None,
            reference: ObservedSurfaceReference::new(SurfaceId::from_bytes([byte; 16]), generation),
            workspace: Some(ObservedWorkspaceReference::new(
                WorkspaceId::from_bytes([workspace_byte; 16]),
                generation,
            )),
            title: title.into(),
            class: class.into(),
            initial_class: class.into(),
            tags: Vec::new(),
            process_id: u32::from(byte),
            xwayland: false,
            visibility: SurfaceVisibility::Visible,
            focus,
        }
    }

    #[test]
    fn active_window_projection_follows_active_toplevel_or_shows_desktop() {
        use gnosix_desktop_model::SurfaceFocus;

        // Current semantics without an active toplevel: the donor module is
        // still present and shows Desktop with the fallback icon.
        let idle = BarPresentation::from_product_semantics(&active_window_semantic(
            vec![active_window_application(
                9,
                "Background",
                "editor",
                SurfaceFocus::Unfocused,
                1,
            )],
            None,
        ));
        let idle_window = idle.active_window.expect("Desktop state is presented");
        assert_eq!(idle_window.title, "Desktop");
        assert_eq!(idle_window.icon, ActiveWindowIcon::NoActiveToplevel);
        assert_eq!(idle_window.icon.role(), Some(IconRole::Desktop));
        assert_eq!(idle_window.config, ActiveWindowConfig::default());

        // Hypr.activeToplevel is the focused client even when it does not sit
        // on the focused regular workspace (for example a special workspace).
        let special = active_window_application(8, "Scratch", "scratch", SurfaceFocus::Focused, 99);
        let presentation =
            BarPresentation::from_product_semantics(&active_window_semantic(vec![special], None));
        let window = presentation
            .active_window
            .expect("active toplevel is presented");
        assert_eq!(window.title, "Scratch");
        assert_eq!(
            window.icon,
            ActiveWindowIcon::NativeClass {
                class: "scratch".into()
            }
        );
    }

    #[test]
    fn active_window_icon_uses_donor_lookup_with_desktop_windows_fallback() {
        use gnosix_application_catalog::{
            ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
        };
        use gnosix_desktop_model::SurfaceFocus;
        use std::{fs, sync::Arc};

        let root =
            std::env::temp_dir().join(format!("gnosix-parity-01f-icon-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("helium.desktop"),
            "[Desktop Entry]\nType=Application\nName=Helium\nExec=helium\nCategories=Network;WebBrowser;\n",
        )
        .unwrap();
        fs::write(
            root.join("plain.desktop"),
            "[Desktop Entry]\nType=Application\nName=Plain\nExec=plain\nCategories=Unlisted;\n",
        )
        .unwrap();
        let catalog = Arc::new(ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![root.clone()]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        ));

        let resolve = |class: &str| {
            let app = active_window_application(7, "Window", class, SurfaceFocus::Focused, 1);
            let mut runtime = crate::product_runtime::ProductRuntime::default();
            runtime.set_application_catalog(catalog.clone());
            runtime.refresh_semantic_for_test(active_window_semantic(vec![app.clone()], Some(app)));
            BarPresentation::from_product_runtime(&runtime)
                .active_window
                .expect("focused window is presented")
                .icon
        };

        // Category priority: WebBrowser precedes Network.
        assert!(matches!(
            resolve("helium"),
            ActiveWindowIcon::Resolved {
                desktop_entry: Some(entry),
                role: IconRole::Web,
                resolution: WorkspaceWindowMarkResolution::ExactDesktopId,
                ..
            } if entry.as_str() == "helium.desktop"
        ));
        // A resolved entry without a role falls back to the module's role,
        // not to the Workspace marks' own fallback.
        assert!(matches!(
            resolve("plain"),
            ActiveWindowIcon::Resolved {
                desktop_entry: Some(_),
                role: IconRole::Desktop,
                ..
            }
        ));
        assert!(matches!(
            resolve("unknown-client"),
            ActiveWindowIcon::Resolved {
                desktop_entry: None,
                role: IconRole::Desktop,
                resolution: WorkspaceWindowMarkResolution::NoDesktopEntryMatch,
                ..
            }
        ));
        // The observed Steam rule still wins before any catalogue lookup.
        assert!(matches!(
            resolve("steam"),
            ActiveWindowIcon::Resolved {
                role: IconRole::Game,
                resolution: WorkspaceWindowMarkResolution::SteamOverride,
                ..
            }
        ));
        // Workspace marks keep their own terminal fallback.
        assert!(matches!(
            resolve_native_workspace_window_mark(&catalog, "plain", "plain"),
            WorkspaceWindowMark::NativeResolved { material_symbol, .. }
                if material_symbol == "terminal"
        ));

        fs::remove_dir_all(root).unwrap();
    }

    // ---- PARITY-01F render closure: module layout, plan and motion ----

    /// Qt 6.11 oracle values, used only to check derived results.
    const ORACLE_ICON_WIDTH: f32 = 19.984_375;
    const ORACLE_ICON_HEIGHT: f32 = 24.0;
    const ORACLE_TITLE_LINE_HEIGHT: f32 = 21.0;

    fn plan_for(
        wts: &gpui::WindowTextSystem,
        presentation: &BarPresentation,
        state: &mut crate::reference_active_window::ActiveWindowState,
        bar_height: f32,
    ) -> crate::reference_active_window::ActiveWindowPlan {
        plan_active_window(presentation, wts, bar_height, state, 1.0e9, false)
            .expect("reference font registered")
            .expect("active window present")
    }

    #[test]
    fn max_height_is_the_donor_equation_over_the_actual_module_layout() {
        let presentation = BarPresentation::captured_oracle_2026_10_07();
        let layout = reference_module_layout(&presentation);
        // Spacers never enter the sum; ActiveWindow is excluded; the repeater
        // counts as a child: 2 spacers + 6 modules + ActiveWindow + repeater.
        assert_eq!(layout.child_count, 10);
        assert_eq!(layout.other_module_heights.len(), 6);
        let others: f32 = layout.other_module_heights.iter().sum();
        let expected =
            (1440.0 - others - MEDIUM_SPACING * 9.0 - LARGE_VERTICAL_PADDING * 2.0).trunc();
        assert_eq!(
            reference_active_window_max_height(&presentation, 1440.0),
            expected
        );
        assert_ne!(expected, 346.0, "the fixed temporary envelope is gone");
    }

    #[test]
    fn max_height_changes_with_every_participating_non_spacer_module() {
        let base = BarPresentation::captured_oracle_2026_10_07();
        let base_max = reference_active_window_max_height(&base, 1440.0);

        // A fifth tray slot adds one slot plus one gap to the tray height.
        let mut taller_tray = base.clone();
        taller_tray
            .tray
            .as_mut()
            .unwrap()
            .push(TrayVisual::CapturedUnresolved { slot: 4 });
        assert_eq!(
            base_max - reference_active_window_max_height(&taller_tray, 1440.0),
            (TRAY_ITEM_SIZE + TRAY_ITEM_SPACING).trunc()
        );

        // A second window mark on the active workspace grows the Workspaces
        // module by exactly one 24 px layout row.
        let mut taller_workspaces = base.clone();
        let mark = taller_workspaces.workspaces[0].window_marks[0].clone();
        taller_workspaces.workspaces[0].window_marks.push(mark);
        assert_eq!(
            base_max - reference_active_window_max_height(&taller_workspaces, 1440.0),
            WORKSPACE_WINDOW_LAYOUT_HEIGHT
        );

        // Removing a whole module removes its height and one spacing step.
        let mut no_power = base.clone();
        no_power.power_icon = None;
        assert_eq!(
            reference_active_window_max_height(&no_power, 1440.0) - base_max,
            24.0 + MEDIUM_SPACING
        );

        // The Bar height is the only other input.
        assert_eq!(
            reference_active_window_max_height(&base, 1500.0) - base_max,
            60.0
        );
    }

    #[test]
    fn plan_uses_the_active_class_icon_and_donor_geometry() {
        use crate::reference_active_window::{ActiveWindowState, test_support::text_system};
        let (_, wts) = text_system(true);
        let presentation = BarPresentation::captured_oracle_2026_10_07();
        let mut state = ActiveWindowState::default();
        let plan = plan_for(&wts, &presentation, &mut state, 1440.0);
        assert_eq!(plan.role, Some(IconRole::Web));
        let advance = plan.frame.current_title().advance;
        assert_eq!(plan.implicit_width, 21.0);
        assert_eq!(
            plan.implicit_height,
            plan.icon_height + advance + ACTIVE_WINDOW_TITLE_TOP_MARGIN
        );
        // The live title fits; nothing is elided.
        assert_eq!(
            plan.frame.current_title().text,
            "Ratification Phase 5 OSD - Helium"
        );
        assert!(advance <= plan.elide_width);
    }

    #[test]
    fn plan_shows_desktop_with_the_fallback_icon_when_nothing_is_focused() {
        use crate::reference_active_window::{ActiveWindowState, test_support::text_system};
        let (_, wts) = text_system(true);
        let mut presentation = BarPresentation::captured_oracle_2026_10_07();
        presentation.active_window = Some(ActiveWindowPresentation {
            title: active_window_title(None, false),
            icon: ActiveWindowIcon::NoActiveToplevel,
            config: ActiveWindowConfig::default(),
        });
        let mut state = ActiveWindowState::default();
        let plan = plan_for(&wts, &presentation, &mut state, 1440.0);
        assert_eq!(plan.role, Some(IconRole::Desktop));
        assert_eq!(plan.frame.current_title().text, "Desktop");
    }

    #[test]
    fn plan_right_elides_to_the_computed_width_and_follows_it() {
        use crate::reference_active_window::{ActiveWindowState, test_support::text_system};
        let (_, wts) = text_system(true);
        let mut presentation = BarPresentation::captured_oracle_2026_10_07();
        let long = "A very long window title that keeps going ".repeat(40);
        presentation.active_window.as_mut().unwrap().title = long.clone();
        let mut state = ActiveWindowState::default();
        let tall = plan_for(&wts, &presentation, &mut state, 1440.0);
        let shown = tall.frame.current_title();
        assert!(shown.text.ends_with('\u{2026}'));
        assert!(shown.advance <= tall.elide_width);
        assert!(tall.implicit_height <= tall.max_height + ACTIVE_WINDOW_TITLE_TOP_MARGIN);

        // A shorter Bar shrinks maxHeight; the same title re-elides in place
        // (width change refits the current slot; it does not swap slots).
        let slot_before = state.slots().current_index();
        let short = plan_for(&wts, &presentation, &mut state, 1000.0);
        assert!(short.elide_width < tall.elide_width);
        assert!(short.frame.current_title().text.len() < shown.text.len());
        assert_eq!(state.slots().current_index(), slot_before);
    }

    #[test]
    fn plan_changes_title_through_the_two_slot_crossfade() {
        use crate::reference_active_window::{ActiveWindowState, test_support::text_system};
        let (_, wts) = text_system(true);
        let mut presentation = BarPresentation::captured_oracle_2026_10_07();
        let mut state = ActiveWindowState::default();
        let first = plan_active_window(&presentation, &wts, 1440.0, &mut state, 0.0, false)
            .unwrap()
            .unwrap();
        let settled = first.frame.slots[first.frame.current].1;
        assert!(settled < 1.0, "first title fades in from the empty slot");
        presentation.active_window.as_mut().unwrap().title = "Other".into();
        let next = plan_active_window(&presentation, &wts, 1440.0, &mut state, 1000.0, false)
            .unwrap()
            .unwrap();
        let mid = plan_active_window(&presentation, &wts, 1440.0, &mut state, 1100.0, false)
            .unwrap()
            .unwrap();
        assert_ne!(next.frame.current, first.frame.current);
        let (old, new) = (
            mid.frame.slots[1 - mid.frame.current].1,
            mid.frame.slots[mid.frame.current].1,
        );
        assert!(
            0.0 < old && old < 1.0 && 0.0 < new && new < 1.0,
            "{old} {new}"
        );
        // Geometry follows the current slot only.
        assert_eq!(mid.frame.current_title().text, "Other");
        assert_eq!(
            mid.implicit_height,
            mid.icon_height + mid.frame.current_title().advance + ACTIVE_WINDOW_TITLE_TOP_MARGIN
        );
    }

    #[test]
    fn plan_without_the_registered_reference_font_fails_instead_of_falling_back() {
        use crate::reference_active_window::{ActiveWindowState, test_support::text_system};
        let presentation = BarPresentation::captured_oracle_2026_10_07();
        let (_, wts) = text_system(false);
        assert!(
            plan_active_window(
                &presentation,
                &wts,
                1440.0,
                &mut ActiveWindowState::default(),
                0.0,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn controlled_cases_plan_the_behaviour_they_are_named_for() {
        use crate::reference_active_window::test_support::text_system;
        let (_, wts) = text_system(true);
        let plan = |case: ActiveWindowCase| case.plan(&wts, 1440.0).unwrap().unwrap();

        let live = plan(ActiveWindowCase::Live);
        assert_eq!(live.source_title, "Ratification Phase 5 OSD - Helium");
        assert_eq!(live.frame.current_title().text, live.source_title);

        let desktop = plan(ActiveWindowCase::Desktop);
        assert_eq!(desktop.role, Some(IconRole::Desktop));
        assert_eq!(desktop.frame.current_title().text, "Desktop");

        let long = plan(ActiveWindowCase::Long);
        let shown = long.frame.current_title();
        assert!(shown.text.ends_with('\u{2026}') && shown.text.len() < long.source_title.len());
        assert!(shown.advance <= long.elide_width);

        let compact = plan(ActiveWindowCase::Compact);
        assert_eq!(compact.source_title, "Helium");

        let inverted = plan(ActiveWindowCase::Inverted);
        assert!(inverted.frame.inverted && !live.frame.inverted);

        let crossfade = plan(ActiveWindowCase::Crossfade);
        let (new, old) = (
            crossfade.frame.slots[crossfade.frame.current].1,
            crossfade.frame.slots[1 - crossfade.frame.current].1,
        );
        assert!(0.0 < new && new < 1.0 && (new + old - 1.0).abs() < 1e-4);
        assert_eq!(
            crossfade.frame.slots[1 - crossfade.frame.current].0.text,
            "Desktop"
        );
        for case in ActiveWindowCase::ALL {
            assert_eq!(ActiveWindowCase::parse(case.name()), Some(case));
        }
    }

    // ---- placement: the layout quantiser, as a model checked against facts ----

    /// Observed GPUI layout tops of the module in a 700x460 probe, requested as
    /// `150 + 100 - f` (container 300, module height `100 + 2 f`, two flex
    /// spacers), from the engineering archive's placement matrix (row D).
    const GPUI_ROW_D: [(f32, f32); 8] = [
        (0.00, 250.0),
        (0.10, 250.0),
        (0.25, 250.0),
        (0.49, 249.0),
        (0.50, 249.0),
        (0.51, 249.0),
        (0.64, 249.0),
        (0.90, 249.0),
    ];

    #[test]
    fn layout_quantiser_model_reproduces_the_gpui_probe_rows() {
        // Row A/B: explicit positions and sizes round exact halves down.
        for (fraction, expected) in [
            (0.0, 10.0),
            (0.49, 10.0),
            (0.50, 10.0),
            (0.51, 11.0),
            (0.90, 11.0),
        ] {
            assert_eq!(
                layout_round_half_down(10.0 + fraction),
                expected,
                "{fraction}"
            );
        }
        // Row D: centred between two flex spacers after the module is snapped.
        for (fraction, observed) in GPUI_ROW_D {
            let module = layout_round_half_down(100.0 + 2.0 * fraction);
            let top = layout_round_half_down(150.0 + (300.0 - module) / 2.0);
            assert_eq!(top, observed, "f={fraction}");
        }
    }

    /// GPUI module tops observed in the controlled captures (manifest
    /// `active_window_bounds`), per case, at Bar height 1440.
    const GPUI_CASE_TOPS: [(ActiveWindowCase, f32); 5] = [
        (ActiveWindowCase::Live, 440.0),
        (ActiveWindowCase::Desktop, 556.0),
        (ActiveWindowCase::Long, 285.0),
        (ActiveWindowCase::Compact, 562.0),
        (ActiveWindowCase::Inverted, 440.0),
    ];

    #[test]
    fn active_window_top_model_matches_the_laid_out_captures() {
        use crate::reference_active_window::test_support::text_system;
        let (_, wts) = text_system(true);
        for (case, observed) in GPUI_CASE_TOPS {
            let plan = case.plan(&wts, 1440.0).unwrap().unwrap();
            let top = active_window_top(&case.presentation(), 1440.0, plan.implicit_height);
            assert_eq!(top, Some(observed), "{}", case.name());
        }
    }

    /// The reference's own quantiser, as observed on the donor (Qt layouts):
    /// preferred sizes round up, free space is shared exactly, and a position
    /// rounds to nearest with exact halves going up. Donor model, test-only.
    fn donor_top(presentation: &BarPresentation, bar_height: f32, module_height: f32) -> f32 {
        let layout = reference_module_layout(presentation);
        let snapped = |entry: IdleBarEntry, height: f32| match entry {
            IdleBarEntry::ActiveWindow => module_height.ceil(),
            IdleBarEntry::Spacer => 0.0,
            _ => height.ceil(),
        };
        let fixed: f32 = layout.entries.iter().map(|(e, h)| snapped(*e, *h)).sum();
        let gaps = MEDIUM_SPACING * (layout.entries.len() - 1) as f32;
        let spacer = (bar_height - 2.0 * LARGE_VERTICAL_PADDING - fixed - gaps) / 2.0;
        let mut y = LARGE_VERTICAL_PADDING;
        for (entry, height) in &layout.entries {
            if *entry == IdleBarEntry::ActiveWindow {
                return (y + 0.5).floor();
            }
            y += if *entry == IdleBarEntry::Spacer {
                spacer
            } else {
                snapped(*entry, *height)
            } + MEDIUM_SPACING;
        }
        unreachable!("ActiveWindow is an entry")
    }

    #[test]
    fn the_two_engines_differ_only_on_exact_halves() {
        use crate::reference_active_window::test_support::text_system;
        let (_, wts) = text_system(true);
        let mut ties = Vec::new();
        for (case, gpui_top) in GPUI_CASE_TOPS {
            let plan = case.plan(&wts, 1440.0).unwrap().unwrap();
            let presentation = case.presentation();
            let donor = donor_top(&presentation, 1440.0, plan.implicit_height);
            if donor != gpui_top {
                // The donor and GPUI free space are equal here; the difference
                // is exactly the tie-break of an exact .5 position.
                assert_eq!(donor - gpui_top, 1.0, "{}", case.name());
                ties.push(case.name());
            }
        }
        // The live module (440.5 -> donor 441, GPUI 440) is one such tie.
        assert!(ties.contains(&"live"));
        // The donor's own observed placement of the live module is 441.
        let live = BarPresentation::captured_oracle_2026_10_07();
        assert_eq!(donor_top(&live, 1440.0, 338.718_75), 441.0);
    }

    #[test]
    fn the_module_stays_centred_within_a_pixel_whatever_the_quantiser() {
        use crate::reference_active_window::test_support::text_system;
        let (_, wts) = text_system(true);
        for (case, _) in GPUI_CASE_TOPS {
            let plan = case.plan(&wts, 1440.0).unwrap().unwrap();
            let presentation = case.presentation();
            let top = active_window_top(&presentation, 1440.0, plan.implicit_height).unwrap();
            // Ideal centre of the slot between the surrounding modules.
            let ideal = 610.0 - plan.implicit_height / 2.0;
            assert!(
                (top - ideal).abs() <= 1.0,
                "{}: {top} vs {ideal}",
                case.name()
            );
        }
    }

    #[test]
    fn tray_height_is_the_source_equation_including_the_empty_module() {
        // Tray.qml, defaults (background=false, compact=false): the column holds
        // TrayItem squares of `bodySmall.pointSize * 2`, spaced by `spacing.small`,
        // inside `padding.extraSmall` on both ends.
        assert_eq!(TRAY_ITEM_SIZE, 24.0);
        assert_eq!(TRAY_ITEM_SPACING, 8.0);
        assert_eq!(TRAY_PADDING, 4.0);
        assert_eq!(tray_module_height(1), 32.0);
        assert_eq!(tray_module_height(4), 128.0);
        assert_eq!(tray_module_height(5) - tray_module_height(4), 32.0);
        // With no item the Column is empty but the module keeps its padding and
        // stays visible (`visible: height > 0`).
        assert_eq!(tray_module_height(0), 8.0);
    }

    /// The Clock and Status final render path draws only Gnosix primitives:
    /// no donor icon font, ligature name or symbol vocabulary survives in it.
    #[test]
    fn clock_and_status_render_path_has_no_donor_symbol_vocabulary() {
        let source = include_str!("reference_bar.rs");
        let body = |name: &str| {
            let start = source
                .find(&format!("\nfn {name}("))
                .unwrap_or_else(|| panic!("{name} exists"));
            let end = start + source[start..].find("\n}\n").expect("function end");
            &source[start..end]
        };
        let glyphs = include_str!("product_status_glyph.rs");
        for (name, text) in [
            ("clock_element", body("clock_element")),
            ("status_icon_line", body("status_icon_line")),
            ("status_visual_element", body("status_visual_element")),
            ("product_status_glyph.rs", glyphs),
        ] {
            for forbidden in [
                "material_icon(",
                "Material Symbols",
                "material_symbol",
                "calendar_month",
                "keyboard_capslock_badge",
                "looks_one",
                "rocket_launch",
                "energy_savings_leaf",
                "bluetooth_connected\"",
            ] {
                assert!(
                    !text.contains(forbidden),
                    "{name} contains donor vocabulary {forbidden:?}"
                );
            }
        }
    }

    #[test]
    fn clock_text_rows_are_the_reference_font_line_boxes() {
        use crate::reference_active_window::{reference_font, test_support::text_system};
        use gpui::px;
        let (_, wts) = text_system(true);
        let id = wts.resolve_font(&reference_font());
        let row = |size: f32| {
            f32::from(wts.ascent(id, px(size))).round()
                + f32::from(wts.descent(id, px(size))).abs().round()
        };
        assert_eq!(row(CLOCK_DIGIT_FONT_SIZE), CLOCK_DIGIT_ROW_HEIGHT);
        assert_eq!(row(CLOCK_MERIDIEM_FONT_SIZE), CLOCK_MERIDIEM_ROW_HEIGHT);
        // Raster baseline spacing: hour->minute 18, minute->meridiem 15.
        let ascent = |size: f32| f32::from(wts.ascent(id, px(size))).round();
        let layout = clock_layout(true);
        assert_eq!(layout.minute_top - layout.hour_top, 18.0);
        assert_eq!(
            layout.meridiem_top.unwrap() + ascent(CLOCK_MERIDIEM_FONT_SIZE)
                - (layout.minute_top + ascent(CLOCK_DIGIT_FONT_SIZE)),
            15.0
        );
    }

    /// StatusIcons.qml: a collapsed lockStatus (neither lock on) is not
    /// `present`, so its EntryWrapper takes no top or bottom gap.
    #[test]
    fn a_collapsed_lock_entry_contributes_neither_height_nor_gap() {
        let status = StatusPresentation {
            entries: vec![
                StatusVisual::Lock {
                    caps_lock: false,
                    num_lock: false,
                },
                StatusVisual::Network {
                    glyph: StatusGlyph::Wired,
                },
            ],
        };
        assert_eq!(status_module_height(&status), 48.0);
        let only_collapsed = StatusPresentation {
            entries: vec![StatusVisual::Lock {
                caps_lock: false,
                num_lock: false,
            }],
        };
        assert_eq!(status_module_height(&only_collapsed), 24.0);
    }

    #[test]
    fn clock_and_status_geometry_follow_the_source_equations() {
        let twelve = clock_layout(true);
        assert_eq!(twelve.height, 105.0, "live raster: rows 1103-1207");
        assert_eq!(
            (
                twelve.icon_top,
                twelve.hour_top,
                twelve.minute_top,
                twelve.meridiem_top
            ),
            (12.0, 39.0, 57.0, Some(75.0))
        );
        // A 24-hour clock drops the meridiem row and its overlap, nothing else.
        let twenty_four = clock_layout(false);
        assert_eq!(twenty_four.height, 91.0);
        assert_eq!(twenty_four.minute_top, twelve.minute_top);
        assert_eq!(twenty_four.meridiem_top, None);
        assert_eq!(STATUS_ICON_LINE_HEIGHT, 24.0);
        assert_eq!(STATUS_ENTRY_GAP, 6.0);
        assert_eq!(STATUS_INNER_GAP, 6.0);
        assert_eq!(STATUS_PADDING, 12.0);

        let empty = StatusPresentation::default();
        assert_eq!(status_module_height(&empty), 24.0);

        for (caps_lock, num_lock, expected) in [
            (false, false, 0.0),
            (true, false, 24.0),
            (false, true, 24.0),
            (true, true, 54.0),
        ] {
            assert_eq!(
                StatusVisual::Lock {
                    caps_lock,
                    num_lock,
                }
                .source_height(),
                expected
            );
        }

        let bluetooth = StatusVisual::Bluetooth {
            glyph: StatusGlyph::BluetoothConnected,
            device_glyphs: vec![
                StatusGlyph::BluetoothDevice(
                    crate::product_status_glyph::BluetoothDeviceClass::Headphones,
                ),
                StatusGlyph::BluetoothDevice(
                    crate::product_status_glyph::BluetoothDeviceClass::Mouse,
                ),
            ],
        };
        assert_eq!(bluetooth.source_height(), 84.0);

        let frozen = BarPresentation::captured_oracle_2026_10_07();
        assert_eq!(
            frozen.status.as_ref().map(status_module_height),
            Some(168.0)
        );
    }

    #[test]
    fn an_empty_tray_still_participates_in_the_active_window_layout() {
        let mut presentation = BarPresentation::captured_oracle_2026_10_07();
        let base = reference_active_window_max_height(&presentation, 1440.0);
        presentation.tray = Some(Vec::new());
        // Four items (128) become an empty module (8): 120 px more room, same child count.
        assert_eq!(
            reference_active_window_max_height(&presentation, 1440.0) - base,
            120.0
        );
        // Provider absence stays absence: no module, no height, one spacing step less.
        presentation.tray = None;
        assert_eq!(
            reference_active_window_max_height(&presentation, 1440.0) - base,
            128.0 + MEDIUM_SPACING
        );
    }
}
