//! SET-02A: the operator's settings for the surfaces.
//!
//! Every value a surface used to hard-code is a setting with that value as
//! its default, in `~/.config/gnosix/surfaces.json`. Surfaces read them
//! through [`current`], which re-reads the file when it changed (checked at
//! most twice a second); tests always see the defaults.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime};

/// The OSD's lives a control offers, in milliseconds.
pub const OSD_DWELL_PRESETS: [u32; 6] = [800, 1200, 1600, 2500, 4000, 6000];
/// The holds a control offers, in milliseconds.
pub const HOLD_PRESETS: [u32; 5] = [500, 700, 1000, 1500, 2000];
/// SET-02D: the dashboard's widths a control offers, in pixels.
pub const DASHBOARD_WIDTH_PRESETS: [u32; 5] = [440, 480, 520, 600, 680];
/// SET-02D: the desktop modules' sizes a control offers, in percent.
/// WS-02: how many windows a workspace lists in the workspaces panel.
pub const WORKSPACE_WINDOWS_PRESETS: [u32; 6] = [2, 3, 4, 6, 8, 12];
pub const MODULE_SCALE_PRESETS: [u32; 7] = [50, 75, 100, 125, 150, 175, 200];
/// The shell's opacities a control offers, in percent.
pub const SHELL_OPACITY_PRESETS: [u32; 6] = [40, 55, 70, 85, 95, 100];
/// The module layers' opacities a control offers, in percent.
pub const LAYER_OPACITY_PRESETS: [u32; 6] = [0, 20, 40, 60, 80, 100];

/// SET-02B: the surfaces painted in the shell's material that may take
/// their own opacity, by slug, with their label.
#[rustfmt::skip]
pub const MATERIAL_SURFACES: [(&str, &str); 8] = [
    (
        "popouts",
        "Bar popouts and dashboard", // i18n
    ),
    (
        "utilities",
        "Utilities", // i18n
    ),
    (
        "osd",
        "On-screen display", // i18n
    ),
    (
        "session",
        "Session", // i18n
    ),
    (
        "toasts",
        "Notification toasts", // i18n
    ),
    (
        "centre",
        "Notification centre", // i18n
    ),
    (
        "settings",
        "Settings", // i18n
    ),
    (
        "workspaces",
        "Workspace manager", // i18n
    ),
];

/// SET-02C: the Bar's modules a short edge can hide, by slug, with their
/// label, in the default order they give way.
#[rustfmt::skip]
pub const BAR_MODULES: [(&str, &str); 7] = [
    (
        "active_window",
        "Active window", // i18n
    ),
    (
        "tray",
        "Tray", // i18n
    ),
    (
        "workspaces",
        "Workspaces", // i18n
    ),
    (
        "clock",
        "Clock", // i18n
    ),
    (
        "status_icons",
        "Status icons", // i18n
    ),
    (
        "logo",
        "Logo", // i18n
    ),
    (
        "power",
        "Power", // i18n
    ),
];

/// SET-02C: what the Bar does on an edge too short for its modules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BarFitPrefs {
    /// Hide modules until the rest fit (otherwise they run off the edge).
    pub hide_what_does_not_fit: bool,
    /// The order modules give way in, by slug.
    pub give_way: Vec<String>,
}

impl Default for BarFitPrefs {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl BarFitPrefs {
    /// Hide, in the default order (`give_way` empty).
    pub const DEFAULT: Self = Self {
        hide_what_does_not_fit: true,
        give_way: Vec::new(),
    };

    pub fn is_valid(&self) -> bool {
        self.give_way.len() <= BAR_MODULES.len()
            && self
                .give_way
                .iter()
                .all(|s| BAR_MODULES.iter().any(|(slug, _)| slug == s))
            && self.give_way.iter().collect::<BTreeSet<_>>().len() == self.give_way.len()
    }

    /// Every module in order: the operator's, then any they left out.
    pub fn order(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self
            .give_way
            .iter()
            .filter_map(|s| {
                BAR_MODULES
                    .iter()
                    .find(|(slug, _)| slug == s)
                    .map(|(slug, _)| *slug)
            })
            .collect();
        for (slug, _) in BAR_MODULES {
            if !out.contains(&slug) {
                out.push(slug);
            }
        }
        out
    }

    /// Move a module one place earlier (-1) or later (1) in the order.
    pub fn move_module(&mut self, slug: &str, dir: i8) {
        let mut order: Vec<String> = self.order().into_iter().map(str::to_owned).collect();
        if let Some(at) = order.iter().position(|s| s == slug) {
            let to = (at as isize + isize::from(dir.signum())).clamp(0, order.len() as isize - 1);
            order.swap(at, to as usize);
        }
        self.give_way = order;
    }

    /// The modules in the order they give way.
    pub fn give_way_entries(&self) -> Vec<crate::reference_bar::IdleBarEntry> {
        use crate::reference_bar::IdleBarEntry as E;
        self.order()
            .into_iter()
            .map(|slug| match slug {
                "active_window" => E::ActiveWindow,
                "tray" => E::Tray,
                "workspaces" => E::Workspaces,
                "clock" => E::Clock,
                "status_icons" => E::StatusIcons,
                "logo" => E::Logo,
                _ => E::Power,
            })
            .collect()
    }
}

/// FOCUS-01D: what a surface opened by the pointer does when one opened
/// deliberately (a click, a command) is out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfacesMeeting {
    /// It does not come out over it.
    #[default]
    OneAtATime,
    /// Both stay out; the deliberate one stands back into its edge.
    MakeRoom,
    /// Both stay out; the pointer's one above, with its own outline.
    Layer,
}

impl SurfacesMeeting {
    pub const ALL: [Self; 3] = [Self::OneAtATime, Self::MakeRoom, Self::Layer];

    pub fn label(self) -> &'static str {
        match self {
            Self::OneAtATime => "One at a time", // i18n
            Self::MakeRoom => "Make room",       // i18n
            Self::Layer => "Layer",              // i18n
        }
    }

    pub fn step(self, dir: i8) -> Self {
        let at = Self::ALL.iter().position(|m| *m == self).unwrap_or(0) as isize;
        let n = Self::ALL.len() as isize;
        Self::ALL[(at + isize::from(dir.signum())).rem_euclid(n) as usize]
    }
}

/// BEHAVIOUR-01: how long the pointer rests before a surface comes out, and
/// how long it may be away before it goes back in, ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub dwell_ms: u32,
    pub grace_ms: u32,
}

impl Timing {
    pub const fn new(dwell_ms: u32, grace_ms: u32) -> Self {
        Self { dwell_ms, grace_ms }
    }

    fn is_valid(&self) -> bool {
        self.dwell_ms <= 2000 && self.grace_ms <= 2000
    }
}

pub const DWELL_PRESETS: [u32; 9] = [0, 60, 120, 180, 250, 350, 500, 750, 1000];
pub const GRACE_PRESETS: [u32; 7] = [0, 120, 200, 280, 400, 600, 1000];
pub const AIM_SLACK_PRESETS: [u32; 9] = [0, 4, 8, 12, 16, 24, 32, 48, 64];
pub const AIM_SPEED_PRESETS: [u32; 7] = [50, 100, 150, 200, 300, 500, 800];
pub const AIM_WINDOW_PRESETS: [u32; 8] = [40, 60, 80, 100, 140, 200, 300, 400];
pub const LAUNCH_FEEDBACK_PRESETS: [u32; 7] = [0, 5, 10, 15, 20, 30, 60];
/// WS-04: how often a window's live thumbnail may be taken again, ms.
pub const THUMBNAIL_REFRESH_PRESETS: [u32; 6] = [250, 500, 1000, 2000, 5000, 10_000];

/// WS-04: live window thumbnails in the workspaces panel.
///
/// The operator's decisions (2026-10-20): on by default; a deny-list whose
/// first entries are the sensitive applications `gnos_thumbnails::policy`
/// names. The operator's changes to that list are kept apart from it
/// (`excluded_added`, `excluded_removed`), so an entry a later Gnosix adds to
/// the default list reaches them too, unless they removed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ThumbnailPrefs {
    pub live: bool,
    pub refresh_ms: u32,
    pub excluded_added: BTreeSet<String>,
    pub excluded_removed: BTreeSet<String>,
}

impl Default for ThumbnailPrefs {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ThumbnailPrefs {
    pub const DEFAULT: Self = Self {
        live: true,
        refresh_ms: 1000,
        excluded_added: BTreeSet::new(),
        excluded_removed: BTreeSet::new(),
    };

    fn is_valid(&self) -> bool {
        let entry_ok =
            |e: &String| !e.trim().is_empty() && e.len() <= 128 && !e.chars().any(char::is_control);
        (250..=10_000).contains(&self.refresh_ms)
            && self.excluded_added.len() <= 128
            && self.excluded_removed.len() <= 128
            && self.excluded_added.iter().all(entry_ok)
            && self.excluded_removed.iter().all(entry_ok)
    }

    pub fn refresh(&self) -> Duration {
        Duration::from_millis(self.refresh_ms.into())
    }

    /// The list in force: the default one, less what the operator removed,
    /// with what they added.
    pub fn excluded(&self) -> Vec<String> {
        let mut list: Vec<String> = gnos_thumbnails::policy::DEFAULT_EXCLUDED
            .iter()
            .filter(|e| !self.excluded_removed.contains(**e))
            .map(|e| (*e).to_owned())
            .collect();
        for e in &self.excluded_added {
            if !list.contains(e) {
                list.push(e.clone());
            }
        }
        list
    }

    /// The entry that keeps a window of this class from being captured.
    pub fn excluded_by(&self, class: &str, initial_class: &str) -> Option<String> {
        gnos_thumbnails::policy::excluded_by(&self.excluded(), class, initial_class)
            .map(str::to_owned)
    }

    /// Exclude `entry` (a class or a pattern) or, when it is in force, let
    /// it go.
    pub fn toggle(&mut self, entry: &str) {
        let default = gnos_thumbnails::policy::DEFAULT_EXCLUDED.contains(&entry);
        let in_force = self.excluded().iter().any(|e| e == entry);
        match (in_force, default) {
            (true, true) => {
                self.excluded_removed.insert(entry.to_owned());
            }
            (true, false) => {
                self.excluded_added.remove(entry);
            }
            (false, true) => {
                self.excluded_removed.remove(entry);
            }
            (false, false) => {
                self.excluded_added.insert(entry.to_owned());
            }
        }
    }
}

/// BEHAVIOUR-01: how the shell's surfaces answer the pointer (Settings ›
/// Shell › Behaviour). The defaults are what the shell did before the
/// operator could choose. Session is not here: it opens on a click only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BehaviourPrefs {
    pub workspaces: Timing,
    pub dashboard: Timing,
    /// The status popouts (audio, network, Bluetooth, battery) open when the
    /// pointer rests on their icon (else not at all: a click on the icons is
    /// Utilities').
    pub status_on_hover: bool,
    pub status: Timing,
    /// A tray item's menu opens when the pointer rests on it (else on a
    /// click).
    pub tray_on_hover: bool,
    pub tray: Timing,
    /// Sliding along the Bar from an open surface opens the neighbour at
    /// once (no delay).
    pub warm_handoff: bool,
    /// Surfaces read where the pointer is heading.
    pub aim: bool,
    pub aim_slack_px: u32,
    pub aim_min_speed: u32,
    pub aim_window_ms: u32,
    /// CURSOR-01B: how long, at most, the pointer says an application is
    /// starting (0: never), s.
    pub launch_feedback_s: u32,
}

impl Default for BehaviourPrefs {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl BehaviourPrefs {
    pub const DEFAULT: Self = Self {
        workspaces: Timing::new(0, 280),
        dashboard: Timing::new(0, 0),
        status_on_hover: true,
        status: Timing::new(0, 0),
        tray_on_hover: true,
        tray: Timing::new(180, 240),
        warm_handoff: true,
        aim: true,
        aim_slack_px: 12,
        aim_min_speed: 150,
        aim_window_ms: 100,
        launch_feedback_s: 15,
    };

    fn is_valid(&self) -> bool {
        [self.workspaces, self.dashboard, self.status, self.tray]
            .iter()
            .all(Timing::is_valid)
            && self.aim_slack_px <= 128
            && (10..=5000).contains(&self.aim_min_speed)
            && (20..=1000).contains(&self.aim_window_ms)
            && self.launch_feedback_s <= 120
    }

    /// The timing of one of the modules the page lists (0 workspaces, 1
    /// dashboard, 2 status popouts, 3 tray menus).
    pub fn timing_mut(&mut self, module: u8) -> Option<&mut Timing> {
        match module {
            0 => Some(&mut self.workspaces),
            1 => Some(&mut self.dashboard),
            2 => Some(&mut self.status),
            3 => Some(&mut self.tray),
            _ => None,
        }
    }

    pub fn timing(&self, module: u8) -> Option<Timing> {
        match module {
            0 => Some(self.workspaces),
            1 => Some(self.dashboard),
            2 => Some(self.status),
            3 => Some(self.tray),
            _ => None,
        }
    }
}

/// WS-03: how the workspaces panel shows the workspaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacesView {
    /// Each workspace's miniature, its windows where they stand, in a grid.
    #[default]
    Map,
    /// The miniatures side by side, in one row.
    Strip,
    /// Each workspace and its windows, as a list.
    List,
}

impl WorkspacesView {
    pub const ALL: [Self; 3] = [Self::Map, Self::Strip, Self::List];

    pub fn label(self) -> &'static str {
        match self {
            Self::Map => "Map",     // i18n
            Self::Strip => "Strip", // i18n
            Self::List => "List",   // i18n
        }
    }

    pub fn step(self, dir: i8) -> Self {
        let at = Self::ALL.iter().position(|v| *v == self).unwrap_or(0) as isize;
        let n = Self::ALL.len() as isize;
        Self::ALL[(at + isize::from(dir.signum())).rem_euclid(n) as usize]
    }
}

/// WS-03: a miniature's height, px.
pub const WORKSPACE_TILE_PRESETS: [u32; 5] = [80, 100, 120, 160, 200];

/// WS-02: what brings the workspaces panel out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelOpening {
    /// The pointer resting on the module.
    #[default]
    Hover,
    /// A click on the workspace already shown (a click on another one
    /// still goes there).
    Click,
    /// Never: the module only switches workspaces.
    Never,
}

impl PanelOpening {
    pub const ALL: [Self; 3] = [Self::Hover, Self::Click, Self::Never];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hover => "When the pointer rests on the module", // i18n
            Self::Click => "On a click on the current workspace",  // i18n
            Self::Never => "Never",                                // i18n
        }
    }

    pub fn step(self, dir: i8) -> Self {
        let at = Self::ALL.iter().position(|o| *o == self).unwrap_or(0) as isize;
        let n = Self::ALL.len() as isize;
        Self::ALL[(at + isize::from(dir.signum())).rem_euclid(n) as usize]
    }
}

/// SET-02B: the shell's tint, as its two palette roles (surface, container).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tint {
    #[default]
    Neutral,
    Slate,
    Warm,
    Violet,
    Black,
}

impl Tint {
    pub const ALL: [Self; 5] = [
        Self::Neutral,
        Self::Slate,
        Self::Warm,
        Self::Violet,
        Self::Black,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral", // i18n
            Self::Slate => "Slate",     // i18n
            Self::Warm => "Warm",       // i18n
            Self::Violet => "Violet",   // i18n
            Self::Black => "Black",     // i18n
        }
    }

    /// (surface, surface container), opaque.
    pub fn palette(self) -> ([u8; 3], [u8; 3]) {
        match self {
            Self::Neutral => ([0x13, 0x13, 0x17], [0x20, 0x1f, 0x23]),
            Self::Slate => ([0x12, 0x16, 0x1c], [0x1e, 0x24, 0x2c]),
            Self::Warm => ([0x18, 0x14, 0x12], [0x26, 0x20, 0x1d]),
            Self::Violet => ([0x16, 0x12, 0x1d], [0x23, 0x1e, 0x2d]),
            Self::Black => ([0x08, 0x08, 0x0a], [0x14, 0x14, 0x17]),
        }
    }

    pub fn step(self, dir: i8) -> Self {
        let at = Self::ALL.iter().position(|t| *t == self).unwrap_or(0) as isize;
        let n = Self::ALL.len() as isize;
        Self::ALL[(at + isize::from(dir.signum())).rem_euclid(n) as usize]
    }
}

/// SET-02B: the shell's material: the Bar, the frame and every surface
/// fused to them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MaterialPrefs {
    /// Translucent (the observed reference) or opaque.
    pub translucent: bool,
    /// The shell's opacity when translucent, in percent.
    pub shell_opacity_pct: u32,
    /// The module containers' opacity over it, in percent.
    pub layer_opacity_pct: u32,
    pub tint: Tint,
    /// Ask the compositor to blur what lies behind the shell.
    pub blur: bool,
    /// How the blur is bounded to what the shell paints.
    pub blur_mask: BlurMask,
    /// A surface's own opacity, by slug (absent: the shell's).
    pub surface_opacity_pct: BTreeMap<String, u32>,
}

/// SHELL-FIELD-01: how the blur behind the shell is bounded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlurMask {
    /// The compositor masks the blur with the alpha the shell paints, pixel
    /// for pixel and in the same frame (Hyprland's `ignore_alpha` on the
    /// scene's layer, asserted by Gnosix).
    #[default]
    Pixels,
    /// Gnosix sends rectangles inside each surface's shape (the blur
    /// protocol): a staircase within 2 px of the curved edges.
    Regions,
}

impl BlurMask {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pixels => "Painted pixels", // i18n
            Self::Regions => "Rectangles",    // i18n
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Pixels => Self::Regions,
            Self::Regions => Self::Pixels,
        }
    }
}

impl Default for MaterialPrefs {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl MaterialPrefs {
    /// The observed reference: translucent, 85 % shell, 40 % layers.
    pub const DEFAULT: Self = Self {
        translucent: true,
        shell_opacity_pct: 85,
        layer_opacity_pct: 40,
        tint: Tint::Neutral,
        blur: true,
        blur_mask: BlurMask::Pixels,
        surface_opacity_pct: BTreeMap::new(),
    };

    pub fn is_valid(&self) -> bool {
        (20..=100).contains(&self.shell_opacity_pct)
            && self.layer_opacity_pct <= 100
            && self.surface_opacity_pct.len() <= MATERIAL_SURFACES.len()
            && self.surface_opacity_pct.iter().all(|(k, v)| {
                MATERIAL_SURFACES.iter().any(|(slug, _)| slug == k) && (20..=100).contains(v)
            })
    }

    /// A surface's opacity (0–1), when it differs from the shell's.
    pub fn surface_alpha(&self, slug: &str) -> Option<f32> {
        if !self.translucent {
            return None;
        }
        self.surface_opacity_pct
            .get(slug)
            .map(|pct| *pct as f32 / 100.0)
    }

    /// The next step of a surface's opacity: the shell's, then each preset.
    pub fn step_surface(&mut self, slug: &str, dir: i8) {
        let current = self.surface_opacity_pct.get(slug).copied();
        let next = match (current, dir.signum()) {
            (None, 1) => Some(SHELL_OPACITY_PRESETS[0]),
            (None, _) => None,
            (Some(p), -1) if p <= SHELL_OPACITY_PRESETS[0] => None,
            (Some(p), _) => Some(step_preset(&SHELL_OPACITY_PRESETS, p, dir)),
        };
        match next {
            Some(p) => self.surface_opacity_pct.insert(slug.to_owned(), p),
            None => self.surface_opacity_pct.remove(slug),
        };
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SurfacePrefs {
    /// The OSD shows volume, microphone and brightness changes.
    pub osd_shown: bool,
    /// How long it stays after the last change.
    pub osd_dwell_ms: u32,
    /// How long log out, reboot and power off are held.
    pub session_hold_ms: u32,
    /// Hovering the clock opens the dashboard (else a click does).
    pub dashboard_on_hover: bool,
    pub desktop_clock: bool,
    /// The desktop visualiser; off also stops its audio capture.
    pub desktop_visualiser: bool,
    /// The Utilities tiles hidden, by name (`wifi`, `bluetooth`, `mic`,
    /// `dnd`, `awake`).
    pub hidden_tiles: BTreeSet<String>,
    /// SET-02B: the shell's material.
    pub material: MaterialPrefs,
    /// SET-02C: the Bar on a short edge.
    pub bar_fit: BarFitPrefs,
    /// SET-02D: the dashboard's width, in pixels.
    pub dashboard_width: u32,
    /// The dashboard's tabs hidden, by slug (`dash`, `media`, `resources`);
    /// one always stays.
    pub dashboard_hidden_tabs: BTreeSet<String>,
    /// The desktop clock module's size, in percent, and its date line.
    pub clock_scale_pct: u32,
    pub clock_date: bool,
    /// Seconds on the desktop clock (the scene then wakes once a second).
    pub clock_seconds: bool,
    /// The desktop visualiser module's size, in percent.
    pub visualiser_scale_pct: u32,
    /// WS-02: what brings the workspaces panel out, and how many windows
    /// each workspace lists there.
    pub workspaces_open: PanelOpening,
    pub workspaces_windows: u32,
    /// WS-03: how the panel shows them, and a miniature's height.
    pub workspaces_view: WorkspacesView,
    pub workspaces_tile_px: u32,
    /// FOCUS-01D: when a surface opened by the pointer meets one opened
    /// deliberately.
    pub surfaces_meeting: SurfacesMeeting,
    /// BEHAVIOUR-01: how the surfaces answer the pointer.
    pub behaviour: BehaviourPrefs,
    /// WS-04: live window thumbnails in the workspaces panel.
    pub thumbnails: ThumbnailPrefs,
    /// SET-03: advanced values the operator moved off their default, by
    /// `TUNABLES` slug.
    pub advanced: BTreeMap<String, u32>,
}

impl Default for SurfacePrefs {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl SurfacePrefs {
    /// The values the surfaces used to hard-code.
    pub const DEFAULT: Self = Self {
        osd_shown: true,
        osd_dwell_ms: 1600,
        session_hold_ms: 1000,
        dashboard_on_hover: true,
        desktop_clock: true,
        desktop_visualiser: true,
        hidden_tiles: BTreeSet::new(),
        material: MaterialPrefs::DEFAULT,
        bar_fit: BarFitPrefs::DEFAULT,
        dashboard_width: 520,
        dashboard_hidden_tabs: BTreeSet::new(),
        clock_scale_pct: 100,
        clock_date: true,
        clock_seconds: false,
        visualiser_scale_pct: 100,
        workspaces_open: PanelOpening::Hover,
        workspaces_windows: 6,
        workspaces_view: WorkspacesView::Map,
        workspaces_tile_px: 120,
        surfaces_meeting: SurfacesMeeting::OneAtATime,
        behaviour: BehaviourPrefs::DEFAULT,
        thumbnails: ThumbnailPrefs::DEFAULT,
        advanced: BTreeMap::new(),
    };

    pub fn is_valid(&self) -> bool {
        (300..=10_000).contains(&self.osd_dwell_ms)
            && (200..=5_000).contains(&self.session_hold_ms)
            && self.hidden_tiles.len() <= 16
            && self.hidden_tiles.iter().all(|t| t.len() <= 32)
            && self.material.is_valid()
            && self.bar_fit.is_valid()
            && (400..=800).contains(&self.dashboard_width)
            && self.dashboard_hidden_tabs.len() < 3
            && self
                .dashboard_hidden_tabs
                .iter()
                .all(|t| ["dash", "media", "resources"].contains(&t.as_str()))
            && (50..=200).contains(&self.clock_scale_pct)
            && (50..=200).contains(&self.visualiser_scale_pct)
            && (1..=24).contains(&self.workspaces_windows)
            && (40..=400).contains(&self.workspaces_tile_px)
            && self.behaviour.is_valid()
            && self.thumbnails.is_valid()
            && self
                .advanced
                .iter()
                .all(|(slug, v)| tunable(slug).is_some_and(|t| (t.min..=t.max).contains(v)))
    }

    /// SET-03: an advanced value, the operator's or its default.
    pub fn advanced(&self, slug: &str) -> u32 {
        self.advanced
            .get(slug)
            .copied()
            .or_else(|| tunable(slug).map(|t| t.default))
            .unwrap_or_default()
    }

    /// SET-03: step an advanced value; back on its default it is forgotten.
    pub fn step_advanced(&mut self, slug: &str, dir: i8) {
        let Some(t) = tunable(slug) else {
            return;
        };
        let now = self.advanced(slug);
        let next = if t.unit == Unit::Toggle {
            u32::from(now == 0)
        } else if dir < 0 {
            now.saturating_sub(t.step).max(t.min)
        } else {
            (now + t.step).min(t.max)
        };
        if next == t.default {
            self.advanced.remove(slug);
        } else {
            self.advanced.insert(slug.to_owned(), next);
        }
    }

    pub fn osd_dwell(&self) -> Duration {
        Duration::from_millis(self.osd_dwell_ms.into())
    }

    pub fn session_hold(&self) -> Duration {
        Duration::from_millis(self.session_hold_ms.into())
    }

    pub fn tile_shown(&self, slug: &str) -> bool {
        !self.hidden_tiles.contains(slug)
    }
}

/// SET-03: how an advanced value reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Ms,
    Px,
    Pct,
    /// 0 off, 1 on.
    Toggle,
    /// A plain number of things.
    Count,
}

/// SET-03: a value the operator's review asked to be an advanced setting,
/// with its default (what the shell hard-coded), bounds and step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tunable {
    pub slug: &'static str,
    pub label: &'static str,
    pub unit: Unit,
    pub default: u32,
    pub min: u32,
    pub max: u32,
    pub step: u32,
}

const fn tune(
    slug: &'static str,
    label: &'static str,
    unit: Unit,
    default: u32,
    min: u32,
    max: u32,
    step: u32,
) -> Tunable {
    Tunable {
        slug,
        label,
        unit,
        default,
        min,
        max,
        step,
    }
}

/// SET-03: the advanced values, in the order Settings › Advanced lists them.
#[rustfmt::skip]
pub const TUNABLES: &[Tunable] = &[
    tune(
        "lift_press_ms",
        "Hold to lift a Bar module", // i18n
        Unit::Ms, 350, 100, 1500, 50
    ),
    tune(
        "hide_reach_px",
        "Reach before a carried module hides", // i18n
        Unit::Px, 48, 0, 240, 8
    ),
    tune(
        "reflow_ms",
        "Bar reflow", // i18n
        Unit::Ms, 300, 0, 1500, 50
    ),
    tune(
        "retract_split_pct",
        "Share of a travel surfaces spend retracting", // i18n
        Unit::Pct, 40, 10, 50, 5
    ),
    tune(
        "sheet_dwell_ms",
        "Hover before the Deck's sheet opens", // i18n
        Unit::Ms, 450, 0, 2000, 50
    ),
    tune(
        "sheet_row_px",
        "Deck sheet row height", // i18n
        Unit::Px, 30, 24, 48, 2
    ),
    tune(
        "hanging_name_near",
        "Hanging sheet: name on the Deck's side", // i18n
        Unit::Toggle, 0, 0, 1, 1
    ),
    tune(
        "one_row_heading",
        "Heading over a one-row category", // i18n
        Unit::Toggle, 1, 0, 1, 1
    ),
    tune(
        "toast_deck_gap_px",
        "Gap between toasts and the Deck", // i18n
        Unit::Px, 8, 0, 64, 4
    ),
    tune(
        "icon_weight_pct",
        "Icon stroke weight", // i18n
        Unit::Pct, 100, 50, 200, 10
    ),
    tune(
        "session_logo",
        "Gnosix logo on the Session button", // i18n
        Unit::Toggle, 1, 0, 1, 1
    ),
    tune(
        "icon_draw_on",
        "Icons draw on as their surface opens", // i18n
        Unit::Toggle, 1, 0, 1, 1
    ),
    tune(
        "shell_field",
        "The shell as one shape", // i18n
        Unit::Toggle, 1, 0, 1, 1
    ),
    tune(
        "joint_fillet_px",
        "Fillet where a surface meets the Bar", // i18n
        Unit::Px, 30, 0, 80, 2
    ),
    tune(
        "wallpaper_folder_cap",
        "Most pictures read from the wallpaper folder", // i18n
        Unit::Count, 4096, 256, 65536, 256
    ),
];

pub fn tunable(slug: &str) -> Option<&'static Tunable> {
    TUNABLES.iter().find(|t| t.slug == slug)
}

/// SET-03: an advanced value now.
pub fn advanced(slug: &str) -> u32 {
    current().advanced(slug)
}

/// SET-03: an advanced duration now.
pub fn advanced_ms(slug: &str) -> Duration {
    Duration::from_millis(advanced(slug).into())
}

/// The preset one step from `current` (the nearest when off a preset).
pub fn step_preset(presets: &[u32], current: u32, dir: i8) -> u32 {
    let at = presets
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| p.abs_diff(current))
        .map_or(0, |(i, _)| i);
    let next = (at as isize + isize::from(dir.signum())).clamp(0, presets.len() as isize - 1);
    presets[next as usize]
}

pub fn path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))
        .map(|c| c.join("gnosix").join("surfaces.json"))
}

fn read() -> SurfacePrefs {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<SurfacePrefs>(&t).ok())
        .map(|mut p| {
            // An advanced value a later shell moved elsewhere (BEHAVIOUR-01:
            // the aim and the tray's timings) is let go, not a reason to
            // throw every setting away.
            p.advanced.retain(|slug, _| tunable(slug).is_some());
            p
        })
        .filter(SurfacePrefs::is_valid)
        .unwrap_or_default()
}

struct Cache {
    prefs: SurfacePrefs,
    modified: Option<SystemTime>,
    checked: Option<Instant>,
}

static CACHE: RwLock<Option<Cache>> = RwLock::new(None);

/// The settings now (the defaults in tests).
pub fn current() -> SurfacePrefs {
    if cfg!(test) {
        return SurfacePrefs::default();
    }
    if let Ok(cache) = CACHE.read()
        && let Some(c) = cache.as_ref()
        && c.checked
            .is_some_and(|t| t.elapsed() < Duration::from_millis(500))
    {
        return c.prefs.clone();
    }
    let modified = path().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let mut cache = CACHE.write().unwrap_or_else(|e| e.into_inner());
    let fresh = match cache.as_ref() {
        Some(c) if c.modified == modified => c.prefs.clone(),
        _ => read(),
    };
    *cache = Some(Cache {
        prefs: fresh.clone(),
        modified,
        checked: Some(Instant::now()),
    });
    fresh
}

/// One per process. Every output's scene observes it, so a setting changed
/// from one output's Settings reaches every output at once, even one that
/// is not redrawing (LIVE-02: nothing redraws at rest).
pub struct SettingsChanged {
    pub generation: u64,
}

struct SettingsChangedGlobal(gpui::Entity<SettingsChanged>);
impl gpui::Global for SettingsChangedGlobal {}

impl SettingsChanged {
    pub fn shared(cx: &mut gpui::App) -> gpui::Entity<Self> {
        if let Some(global) = cx.try_global::<SettingsChangedGlobal>() {
            return global.0.clone();
        }
        let entity = gpui::AppContext::new(cx, |_| Self { generation: 0 });
        cx.set_global(SettingsChangedGlobal(entity.clone()));
        entity
    }

    /// Tell every output that a setting changed.
    pub fn announce(cx: &mut gpui::App) {
        Self::shared(cx).update(cx, |this, cx| {
            this.generation += 1;
            cx.notify();
        });
    }
}

/// Store the settings (atomically) and use them at once.
pub fn save(prefs: &SurfacePrefs) -> std::io::Result<()> {
    if !prefs.is_valid() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid settings",
        ));
    }
    let path = path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config directory"))?;
    let dir = path.parent().expect("a file in a directory");
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".surfaces.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(prefs)?)?;
    std::fs::rename(&tmp, &path)?;
    if let Ok(mut cache) = CACHE.write() {
        *cache = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn thumbnails_are_on_and_the_sensitive_list_is_in_force_by_default() {
        let t = SurfacePrefs::default().thumbnails;
        assert!(t.live);
        assert_eq!(t.refresh_ms, 1000);
        assert!(t.excluded_by("org.keepassxc.KeePassXC", "").is_some());
        assert!(t.excluded_by("pinentry-qt", "").is_some());
        assert!(t.excluded_by("kitty", "kitty").is_none());
    }

    #[test]
    fn the_operator_edits_the_list_without_freezing_the_default_one() {
        let mut t = ThumbnailPrefs::default();
        // Letting a default entry go is remembered as such.
        t.toggle("veracrypt");
        assert!(t.excluded_removed.contains("veracrypt"));
        assert!(t.excluded_by("veracrypt", "").is_none());
        // And taking it back forgets the removal.
        t.toggle("veracrypt");
        assert!(t.excluded_removed.is_empty());
        assert!(t.excluded_by("veracrypt", "").is_some());
        // An application of their own is added, then let go.
        t.toggle("org.example.Bank");
        assert_eq!(
            t.excluded_by("org.example.bank", ""),
            Some("org.example.Bank".to_owned())
        );
        t.toggle("org.example.Bank");
        assert!(t.excluded_added.is_empty());
        // Only the changes are stored: every default entry stays in force.
        assert_eq!(
            t.excluded().len(),
            gnos_thumbnails::policy::DEFAULT_EXCLUDED.len()
        );
    }

    #[test]
    fn thumbnail_settings_out_of_bounds_are_refused() {
        let mut p = SurfacePrefs::default();
        p.thumbnails.refresh_ms = 100;
        assert!(!p.is_valid());
        let mut p = SurfacePrefs::default();
        p.thumbnails.excluded_added.insert("bad\nentry".to_owned());
        assert!(!p.is_valid());
        let mut p = SurfacePrefs::default();
        p.thumbnails.excluded_added.insert("  ".to_owned());
        assert!(!p.is_valid());
    }

    use super::*;

    #[test]
    fn advanced_values_step_within_bounds_and_forget_their_default() {
        let mut p = SurfacePrefs::default();
        for t in TUNABLES {
            assert!((t.min..=t.max).contains(&t.default), "{}", t.slug);
            assert_eq!(p.advanced(t.slug), t.default);
        }
        p.step_advanced("lift_press_ms", 1);
        assert_eq!(p.advanced("lift_press_ms"), 400);
        assert!(p.is_valid());
        p.step_advanced("lift_press_ms", -1);
        assert!(p.advanced.is_empty(), "back on its default, forgotten");
        for _ in 0..100 {
            p.step_advanced("reflow_ms", -1);
        }
        assert_eq!(p.advanced("reflow_ms"), 0, "down to its bound");
        p.step_advanced("one_row_heading", 1);
        assert_eq!(p.advanced("one_row_heading"), 0, "a toggle flips");
        p.step_advanced("no_such_value", 1);
        assert!(!p.advanced.contains_key("no_such_value"));
        let mut bad = SurfacePrefs::default();
        bad.advanced.insert("sheet_row_px".into(), 99);
        assert!(!bad.is_valid(), "out of bounds is refused");
        bad.advanced.clear();
        bad.advanced.insert("unknown".into(), 1);
        assert!(!bad.is_valid(), "an unknown value is refused");
    }

    #[test]
    fn the_defaults_are_the_former_hard_coded_values_and_valid() {
        let p = SurfacePrefs::default();
        assert!(p.is_valid());
        assert_eq!(p.osd_dwell(), Duration::from_millis(1600));
        assert_eq!(p.session_hold(), Duration::from_millis(1000));
        assert!(p.dashboard_on_hover && p.desktop_clock && p.desktop_visualiser && p.osd_shown);
        assert_eq!(
            (p.workspaces_open, p.workspaces_windows as usize),
            (PanelOpening::Hover, crate::workspace_panel::MAX_WINDOWS)
        );
        assert_eq!(
            p.surfaces_meeting,
            SurfacesMeeting::OneAtATime,
            "one at a time"
        );
        assert!(p.tile_shown("wifi"));
        assert_eq!(current(), p, "tests never read the operator's file");
    }

    #[test]
    fn behaviour_defaults_are_what_the_shell_did_before() {
        let b = SurfacePrefs::default().behaviour;
        assert_eq!(
            (b.workspaces, b.dashboard, b.status, b.tray),
            (
                Timing::new(0, 280),
                Timing::new(0, 0),
                Timing::new(0, 0),
                Timing::new(180, 240)
            )
        );
        assert!(b.status_on_hover && b.tray_on_hover && b.warm_handoff && b.aim);
        assert_eq!(
            (b.aim_slack_px, b.aim_min_speed, b.aim_window_ms),
            (12, 150, 100)
        );
    }

    #[test]
    fn an_advanced_value_moved_elsewhere_does_not_throw_the_settings_away() {
        // A file from before BEHAVIOUR-01, with the aim in "advanced".
        let mut p: SurfacePrefs = serde_json::from_str(
            r#"{"osd_dwell_ms": 2500, "advanced": {"aim_slack_px": 20, "sheet_dwell_ms": 300}}"#,
        )
        .unwrap();
        assert!(!p.is_valid(), "as read, the unknown slug would be refused");
        p.advanced.retain(|slug, _| tunable(slug).is_some());
        assert!(p.is_valid());
        assert_eq!(p.osd_dwell_ms, 2500, "the operator's other values kept");
        assert_eq!(p.advanced("sheet_dwell_ms"), 300);
    }

    #[test]
    fn steps_move_through_presets_and_bounds_are_enforced() {
        assert_eq!(step_preset(&OSD_DWELL_PRESETS, 1600, 1), 2500);
        assert_eq!(step_preset(&OSD_DWELL_PRESETS, 800, -1), 800);
        assert_eq!(step_preset(&HOLD_PRESETS, 1000, -1), 700);
        assert_eq!(step_preset(&HOLD_PRESETS, 1100, 1), 1500);
        let bad = SurfacePrefs {
            osd_dwell_ms: 100,
            ..SurfacePrefs::default()
        };
        assert!(!bad.is_valid());
        let bad = SurfacePrefs {
            session_hold_ms: 9000,
            ..SurfacePrefs::default()
        };
        assert!(!bad.is_valid());
        assert!(serde_json::from_str::<SurfacePrefs>(r#"{"x": 1}"#).is_err());
        let partial: SurfacePrefs = serde_json::from_str(r#"{"osd_shown": false}"#).unwrap();
        assert!(!partial.osd_shown && partial.osd_dwell_ms == 1600);
    }

    #[test]
    fn the_dashboard_and_desktop_modules_keep_their_sizes_by_default() {
        let p = SurfacePrefs::default();
        assert_eq!(p.dashboard_width, 520);
        assert!(p.dashboard_hidden_tabs.is_empty() && p.clock_date);
        assert_eq!((p.clock_scale_pct, p.visualiser_scale_pct), (100, 100));
        let all_hidden = SurfacePrefs {
            dashboard_hidden_tabs: ["dash", "media", "resources"].map(String::from).into(),
            ..SurfacePrefs::default()
        };
        assert!(!all_hidden.is_valid(), "one tab always stays");
        let wide = SurfacePrefs {
            dashboard_width: 900,
            ..SurfacePrefs::default()
        };
        assert!(!wide.is_valid());
    }

    #[test]
    fn the_bar_gives_way_in_an_order_the_operator_moves() {
        use crate::reference_bar::IdleBarEntry as E;
        let mut f = BarFitPrefs::default();
        assert!(f.hide_what_does_not_fit && f.is_valid());
        assert_eq!(f.give_way_entries()[..2], [E::ActiveWindow, E::Tray]);
        f.move_module("clock", -1);
        assert_eq!(
            f.order()[..4],
            ["active_window", "tray", "clock", "workspaces"]
        );
        f.move_module("active_window", -1);
        assert_eq!(f.order()[0], "active_window", "the first stays first");
        assert!(f.is_valid());
        let dup = BarFitPrefs {
            give_way: vec!["tray".into(), "tray".into()],
            ..BarFitPrefs::default()
        };
        assert!(!dup.is_valid());
        let partial: BarFitPrefs = serde_json::from_str(r#"{"give_way": ["power"]}"#).unwrap();
        assert_eq!(partial.order()[0], "power");
        assert_eq!(partial.order().len(), 7);
    }

    #[test]
    fn the_material_defaults_to_the_observed_reference_and_steps_per_surface() {
        let mut m = MaterialPrefs::default();
        assert!(m.translucent && m.blur && m.is_valid());
        assert_eq!((m.shell_opacity_pct, m.layer_opacity_pct), (85, 40));
        assert_eq!(m.tint.palette().0, [0x13, 0x13, 0x17]);
        assert_eq!(m.surface_alpha("utilities"), None, "the shell's by default");
        m.step_surface("utilities", 1);
        assert_eq!(m.surface_alpha("utilities"), Some(0.4));
        m.step_surface("utilities", 1);
        assert_eq!(m.surface_alpha("utilities"), Some(0.55));
        m.step_surface("utilities", -1);
        m.step_surface("utilities", -1);
        assert_eq!(m.surface_alpha("utilities"), None, "back to the shell's");
        m.step_surface("osd", 1);
        m.translucent = false;
        assert_eq!(m.surface_alpha("osd"), None, "opaque everywhere");
        assert_eq!(Tint::Black.step(1), Tint::Neutral);
        let bad = MaterialPrefs {
            surface_opacity_pct: [("nowhere".to_owned(), 50)].into(),
            ..MaterialPrefs::default()
        };
        assert!(!bad.is_valid());
        let partial: SurfacePrefs =
            serde_json::from_str(r#"{"material": {"tint": "warm"}}"#).unwrap();
        assert_eq!(partial.material.tint, Tint::Warm);
        assert_eq!(partial.material.shell_opacity_pct, 85);
    }
}
