//! UXE-05B: Settings pages, derived only from what the scene observes.
//!
//! A page either shows observed state (read-only in this build), or says it
//! is under construction and names the power it is waiting for (D02). No
//! line here can change anything: writes belong to the profile authority
//! (R12) and arrive as their own slice.

use crate::bar_status_popouts::{InfoPopout, InfoRowKind};
use crate::product_preferences::{
    BarEdgePreference, EffectiveProductPreferences, PreferenceValueSource, ProductColorScheme,
    ProductPreferenceProjectionState,
};
use gnos_audio::{AudioEndpointKind, AudioProviderState, AudioSnapshot};
use gnos_bluetooth::BluetoothSnapshot;
use gnos_i18n::{t, tf};
use gnos_network::NetworkSnapshot;
use gnos_power::PowerSnapshot;
use in_gnu_os_settings_read_model::{BarModule, BarModuleLayout, BarPreferenceOverrides};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PageId {
    Appearance,
    WallpaperAndStyle,
    Display,
    Network,
    Bluetooth,
    Audio,
    Power,
    Updates,
    Plugins,
    Bar,
    /// DECK-01E: the Deck's wave (D09).
    Deck,
    Apps,
    Notifications,
    LanguageAndRegion,
    /// LOCK-01: the lock screen's settings.
    Lock,
    /// SET-02A: the surfaces' own settings.
    Surfaces,
    /// BEHAVIOUR-01: how the surfaces answer the pointer.
    Behaviour,
    /// SET-03: the values the review asked to be advanced settings.
    Advanced,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Appearance,
    Connectivity,
    System,
    Shell,
    About,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => t("Appearance"),
            Self::Connectivity => t("Connectivity"),
            Self::System => t("System"),
            Self::Shell => t("Shell"),
            Self::About => t("About"),
        }
    }
}

/// Catalogue order, grouped like the reference's navigation pane (R01).
pub const PAGES: [(PageId, Category); 19] = [
    (PageId::Appearance, Category::Appearance),
    (PageId::WallpaperAndStyle, Category::Appearance),
    (PageId::Display, Category::Connectivity),
    (PageId::Network, Category::Connectivity),
    (PageId::Bluetooth, Category::Connectivity),
    (PageId::Audio, Category::Connectivity),
    (PageId::Power, Category::System),
    (PageId::Lock, Category::System),
    (PageId::Updates, Category::System),
    (PageId::Plugins, Category::System),
    (PageId::Bar, Category::Shell),
    (PageId::Deck, Category::Shell),
    (PageId::Surfaces, Category::Shell),
    (PageId::Behaviour, Category::Shell),
    (PageId::Apps, Category::Shell),
    (PageId::Notifications, Category::Shell),
    (PageId::LanguageAndRegion, Category::Shell),
    (PageId::Advanced, Category::Shell),
    (PageId::About, Category::About),
];

impl PageId {
    pub fn title(self) -> &'static str {
        match self {
            Self::Appearance => t("Appearance"),
            Self::WallpaperAndStyle => t("Wallpaper & style"),
            Self::Display => t("Display"),
            Self::Network => t("Network"),
            Self::Bluetooth => t("Connected devices"),
            Self::Audio => t("Audio"),
            Self::Power => t("Power"),
            Self::Updates => t("Updates"),
            Self::Plugins => t("Plugins"),
            Self::Bar => t("Bar"),
            Self::Deck => t("Deck"),
            Self::Apps => t("Apps"),
            Self::Notifications => t("Notifications"),
            Self::LanguageAndRegion => t("Language & region"),
            Self::Lock => t("Lock screen"),
            Self::Surfaces => t("Surfaces"),
            Self::Behaviour => t("Behaviour"),
            Self::Advanced => t("Advanced"),
            Self::About => t("About"),
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::WallpaperAndStyle => "wallpaper",
            Self::Display => "display",
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Audio => "audio",
            Self::Power => "power",
            Self::Updates => "updates",
            Self::Plugins => "plugins",
            Self::Bar => "bar",
            Self::Deck => "deck",
            Self::Apps => "apps",
            Self::Notifications => "notifications",
            Self::LanguageAndRegion => "language",
            Self::Lock => "lock",
            Self::Surfaces => "surfaces",
            Self::Behaviour => "behaviour",
            Self::Advanced => "advanced",
            Self::About => "about",
        }
    }

    /// The power a page waits for; `None` when it already shows something
    /// real.
    pub fn missing_power(self) -> Option<&'static str> {
        match self {
            Self::Updates => Some(t("No package-update authority yet.")),
            // SET-02/SET-03: the surfaces and their advanced values belong
            // to the shell's surfaces, which this build does not carry.
            #[cfg(not(feature = "component-lab"))]
            Self::Surfaces => Some(t("This build has no surface settings yet.")),
            #[cfg(not(feature = "component-lab"))]
            Self::Behaviour => Some(t("This build has no surface settings yet.")),
            #[cfg(not(feature = "component-lab"))]
            Self::Advanced => Some(t("This build has no advanced settings yet.")),
            _ => None,
        }
    }
}

/// One edit a Settings control can ask for. It names a field, never a
/// value: the next value is derived from the current projection, so a
/// control can only step through the typed domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    CycleEdge,
    CyclePersistent,
    CycleShowOnHover,
    CycleTrayCompact,
    CycleWorkspaceDisplay,
    WorkspaceCountDown,
    WorkspaceCountUp,
    /// BAR-01E (B03): the bounded shell preferences.
    Step(ShellStep, i8),
    CycleClock(ClockField),
    ToggleStatus(StatusField),
    /// A module's group: hidden, start, centre, end, hidden.
    ModuleGroup(BarModule),
    /// A module one place earlier (-1) or later (+1) in its group.
    ModuleOrder(BarModule, i8),
    /// SETTINGS-02: a live system control (a power, not a preference).
    Live(LiveControl),
}

/// SET-02A: one of the surfaces' settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceSetting {
    OsdShown,
    OsdDwell,
    SessionHold,
    DashboardOnHover,
    DesktopClock,
    DesktopVisualiser,
    /// A Utilities tile, by its index in the tiles' order.
    Tile(u8),
    /// SET-02B: the shell's material.
    Translucent,
    ShellOpacity,
    LayerOpacity,
    Tint,
    Blur,
    BlurMask,
    /// A surface's own opacity, by its index in `MATERIAL_SURFACES`.
    SurfaceOpacity(u8),
    /// SET-02D: the dashboard's width, a tab (by its index in
    /// `DashTab::ALL`) shown or hidden, and the desktop modules' sizes.
    DashboardWidth,
    DashboardTab(u8),
    ClockScale,
    ClockDate,
    ClockSeconds,
    VisualiserScale,
    /// SET-02C: hide what does not fit on a short edge.
    BarFitHide,
    /// SET-03: an advanced value, by its index in `TUNABLES`.
    Advanced(u8),
    /// A Bar module, by its index in `BAR_MODULES`, moved earlier or later
    /// in the order modules give way.
    BarGiveWay(u8),
    /// WS-02: what brings the workspaces panel out, and the windows each
    /// workspace lists there.
    WorkspacesOpen,
    WorkspacesWindows,
    /// WS-03: the panel's view, a miniature's height.
    WorkspacesView,
    WorkspacesTile,
    /// FOCUS-01D: when surfaces meet.
    SurfacesMeeting,
    /// BEHAVIOUR-01: the warm Bar, the aim and its reading, the status
    /// popouts' and tray menus' opening, and a module's delay and grace (by
    /// its index: 0 workspaces, 1 dashboard, 2 status popouts, 3 tray menus).
    WarmHandoff,
    Aim,
    AimSlack,
    AimSpeed,
    AimWindow,
    StatusOnHover,
    TrayOnHover,
    Dwell(u8),
    Grace(u8),
    /// CURSOR-01B: how long the pointer says an application is starting.
    LaunchFeedback,
    /// WS-04: live thumbnails on or off, how often they refresh, and an
    /// application shown or not (by its index in `LiveFacts::thumbnail_apps`).
    ThumbnailsLive,
    ThumbnailRefresh,
    ThumbnailApp(u8),
}

/// SETTINGS-02: what a live control changes. Each is carried out by the
/// power the Bar's surfaces already use, and its result is observed, never
/// assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveControl {
    Wifi,
    Bluetooth,
    DoNotDisturb,
    KeepAwake,
    OutputMute,
    Microphone,
    /// One volume step down (-1) or up (+1) on the default output.
    OutputVolume(i8),
    /// One brightness step down (-1) or up (+1) on a target.
    Brightness(u8, i8),
    /// The next power profile.
    PowerProfile,
    /// Connect to the Wi-Fi network at this index of `LiveFacts::wifi`.
    WifiConnect(u8),
    /// SET-02A: one surface setting, one step in a direction (a switch
    /// ignores the direction).
    Surface(SurfaceSetting, i8),
    /// WALL-01: whether Gnosix draws the wallpaper.
    WallpaperEnabled,
    /// WALL-01: the previous (-1) or next (1) picture in the folder.
    WallpaperPicture(i8),
    /// WALL-01: the next way the picture fills the screen.
    WallpaperFit,
    /// I18N-01: the next language (the system's, then each one shipped).
    Language,
    /// APPS-01: the next application for the role at this index of
    /// `DefaultRole::ALL`.
    DefaultApp(u8),
    /// SETTINGS-03B: the toast life, one step in a direction.
    NotificationToast(i8),
    /// SETTINGS-03B: critical notifications through do not disturb.
    NotificationCritical,
    /// SETTINGS-03B: the next rule for the application at this index of
    /// `LiveFacts::notification_apps`.
    NotificationApp(u8),
    /// DISPLAY-01: one output's setting, one step in a direction.
    Display(u8, gnos_utilities::DisplaySetting, i8),
    /// Keep the display change just made (and write it for Hyprland).
    DisplayKeep,
    /// Put the outputs back as they were before the change.
    DisplayRevert,
    /// A lock screen setting, one step in a direction.
    Lock(gnos_lock::LockSetting, i8),
    /// Connect (or disconnect) the paired Bluetooth device at this index of
    /// `LiveFacts::bluetooth`.
    BluetoothDevice(u8),
    /// Ask for the passphrase of the secured network at this index.
    WifiPassword(u8),
    /// Make this device the default for its kind.
    DefaultDevice(gnos_audio::AudioEndpointId, gnos_audio::AudioEndpointKind),
}

/// SETTINGS-02: the live facts the pages show beside the observed services.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveFacts {
    pub do_not_disturb: bool,
    pub keep_awake: bool,
    /// Each brightness target: its label and level in percent (`None`
    /// while it is read or changes).
    pub brightness: Vec<(String, Option<u8>)>,
    /// The displays and backlights are being read.
    pub reading_brightness: bool,
    /// The Wi-Fi networks in range, as last read.
    pub wifi: Vec<gnos_network::WifiNetwork>,
    pub reading_wifi: bool,
    /// SET-02A: the surfaces' settings as stored.
    #[cfg(feature = "component-lab")]
    pub surfaces: crate::surface_prefs::SurfacePrefs,
    /// WS-04: the applications the thumbnails list concerns (those with a
    /// window now, then the rest of the list in force), each with the entry
    /// that keeps it from being captured.
    #[cfg(feature = "component-lab")]
    pub thumbnail_apps: Vec<(String, Option<String>)>,
    /// WALL-01: whether Gnosix draws it, the picture's name, how it fits,
    /// how many pictures the folder offers, and the folder.
    pub wallpaper: Option<(bool, String, &'static str, String, String)>,
    /// I18N-01: the stored choice (`None` is the system's) and the active
    /// language.
    pub language_choice: Option<String>,
    pub language_active: String,
    /// APPS-01: each role's default application (its name) and how many
    /// installed applications can take it.
    pub default_apps: Vec<(Option<String>, usize)>,
    /// SETTINGS-03B: the notification preferences, the applications seen
    /// (or with a rule), and who serves notifications when Gnosix does not.
    pub notification_prefs: gnos_notifications::prefs::NotificationPrefs,
    pub notification_apps: Vec<String>,
    pub notification_service: Option<String>,
    /// DISPLAY-01: the outputs as last read, and seconds left before an
    /// unconfirmed change reverts.
    pub displays: Vec<gnos_utilities::DisplayOutput>,
    pub display_revert_in: Option<u32>,
    /// The lock screen's settings, as stored.
    pub lock: gnos_lock::LockConfig,
    /// The paired Bluetooth devices: alias and whether connected.
    pub bluetooth: Vec<(String, bool)>,
    /// A text prompt in progress: what it asks for, and the text as typed
    /// (the page shows it masked).
    pub prompt: Option<(String, usize)>,
}

/// A bounded value a stepper moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellStep {
    Thickness,
    Density,
    TravelTempo,
    SpringTempo,
    TitleMax,
    DeckMagnify,
    DeckReach,
    DeckTempo,
    DeckShoulder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockField {
    Hour24,
    Date,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusField {
    Lock,
    Network,
    Bluetooth,
    Battery,
}

impl ShellStep {
    fn slug(self) -> &'static str {
        match self {
            Self::Thickness => "thickness",
            Self::Density => "density",
            Self::TravelTempo => "travel_tempo",
            Self::SpringTempo => "spring_tempo",
            Self::TitleMax => "title_max",
            Self::DeckMagnify => "deck_magnify",
            Self::DeckReach => "deck_reach",
            Self::DeckTempo => "deck_tempo",
            Self::DeckShoulder => "deck_shoulder",
        }
    }
}

pub fn module_slug(module: BarModule) -> &'static str {
    match module {
        BarModule::Logo => "logo",
        BarModule::Workspaces => "workspaces",
        BarModule::ActiveWindow => "active_window",
        BarModule::Tray => "tray",
        BarModule::Clock => "clock",
        BarModule::StatusIcons => "status_icons",
        BarModule::Power => "power",
    }
}

pub fn module_label(module: BarModule) -> &'static str {
    match module {
        BarModule::Logo => t("Logo"),
        BarModule::Workspaces => t("Workspaces"),
        BarModule::ActiveWindow => t("Active window"),
        BarModule::Tray => t("Tray"),
        BarModule::Clock => t("Clock"),
        BarModule::StatusIcons => t("Status icons"),
        BarModule::Power => t("Power"),
    }
}

/// The product's own layout when the profile names none: the reference's.
pub fn default_module_layout() -> BarModuleLayout {
    use BarModule as M;
    BarModuleLayout::from_groups([
        &[M::Logo, M::Workspaces],
        &[M::ActiveWindow],
        &[M::Tray, M::Clock, M::StatusIcons, M::Power],
    ])
    .expect("the default layout lists each module once")
}

/// The product's own defaults for the bounded shell values.
pub mod shell_defaults {
    pub const THICKNESS: i64 = 60;
    pub const DENSITY: i64 = 100;
    pub const TEMPO: i64 = 100;
    pub const TITLE_MAX: i64 = 240;
    pub const DECK_MAGNIFY: i64 = 150;
    pub const DECK_REACH: i64 = 25;
    pub const DECK_TEMPO: i64 = 100;
    pub const DECK_SHOULDER: i64 = 24;
}

impl SettingsAction {
    pub fn slug(self) -> String {
        match self {
            Self::CycleEdge => "edge".into(),
            Self::CyclePersistent => "persistent".into(),
            Self::CycleShowOnHover => "show_on_hover".into(),
            Self::CycleTrayCompact => "tray_compact".into(),
            Self::CycleWorkspaceDisplay => "workspace_display".into(),
            Self::WorkspaceCountDown => "workspace_count_down".into(),
            Self::WorkspaceCountUp => "workspace_count_up".into(),
            Self::Step(step, d) => {
                format!("{}_{}", step.slug(), if d < 0 { "down" } else { "up" })
            }
            Self::CycleClock(f) => format!("clock_{f:?}").to_lowercase(),
            Self::ToggleStatus(f) => format!("status_{f:?}").to_lowercase(),
            Self::ModuleGroup(m) => format!("module_group_{}", module_slug(m)),
            Self::ModuleOrder(m, d) => format!(
                "module_{}_{}",
                module_slug(m),
                if d < 0 { "earlier" } else { "later" }
            ),
            Self::Live(control) => format!("live_{control:?}").to_lowercase(),
        }
    }
}

/// The typed request an action becomes, for the profile authority (R12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BarEdit {
    Edge(BarEdgePreference),
    Overrides(BarPreferenceOverrides),
}

/// The reference's default number of workspaces shown: the starting point
/// when the profile has no explicit count.
pub const DEFAULT_WORKSPACES_SHOWN: i64 = 5;

fn cycle_bool(value: Option<bool>) -> Option<bool> {
    match value {
        None => Some(true),
        Some(true) => Some(false),
        Some(false) => None,
    }
}

/// The next state for `action`, from the projection the user is looking at.
/// `None` when the step would leave the domain (a count outside 1..=20).
pub fn bar_edit(
    action: SettingsAction,
    edge: Option<BarEdgePreference>,
    current: &BarPreferenceOverrides,
) -> Option<BarEdit> {
    use in_gnu_os_settings_read_model::{WorkspaceDisplayCount, WorkspaceDisplayMode};
    let mut next = current.clone();
    match action {
        SettingsAction::Live(_) => return None,
        SettingsAction::CycleEdge => {
            return Some(BarEdit::Edge(match edge {
                Some(BarEdgePreference::Top) | None => BarEdgePreference::Right,
                Some(BarEdgePreference::Right) => BarEdgePreference::Bottom,
                Some(BarEdgePreference::Bottom) => BarEdgePreference::Left,
                Some(BarEdgePreference::Left) => BarEdgePreference::Top,
            }));
        }
        SettingsAction::CyclePersistent => next.persistent = cycle_bool(current.persistent),
        SettingsAction::CycleShowOnHover => next.show_on_hover = cycle_bool(current.show_on_hover),
        SettingsAction::CycleTrayCompact => next.tray_compact = cycle_bool(current.tray_compact),
        SettingsAction::CycleWorkspaceDisplay => {
            next.workspace_display = match current.workspace_display {
                None => Some(WorkspaceDisplayMode::Shapes),
                Some(WorkspaceDisplayMode::Shapes) => Some(WorkspaceDisplayMode::Text),
                Some(WorkspaceDisplayMode::Text) => Some(WorkspaceDisplayMode::Icons),
                Some(WorkspaceDisplayMode::Icons) => None,
            };
        }
        SettingsAction::WorkspaceCountDown | SettingsAction::WorkspaceCountUp => {
            let now = current
                .workspace_count
                .map_or(DEFAULT_WORKSPACES_SHOWN, |c| i64::from(c.get()));
            let delta = if action == SettingsAction::WorkspaceCountUp {
                1
            } else {
                -1
            };
            next.workspace_count = Some(WorkspaceDisplayCount::try_from(now + delta).ok()?);
        }
        SettingsAction::Step(step, direction) => shell_step(&mut next, step, direction)?,
        SettingsAction::CycleClock(field) => {
            let mut clock = current.clock.unwrap_or_default();
            match field {
                ClockField::Hour24 => clock.hour24 = !clock.hour24,
                ClockField::Date => clock.date = !clock.date,
            }
            next.clock = Some(clock);
        }
        SettingsAction::ToggleStatus(field) => {
            let mut icons = current.status_icons.unwrap_or_default();
            match field {
                StatusField::Lock => icons.lock = !icons.lock,
                StatusField::Network => icons.network = !icons.network,
                StatusField::Bluetooth => icons.bluetooth = !icons.bluetooth,
                StatusField::Battery => icons.battery = !icons.battery,
            }
            next.status_icons = Some(icons);
        }
        SettingsAction::ModuleGroup(module) => {
            let mut groups = current
                .modules
                .unwrap_or_else(default_module_layout)
                .groups();
            let from = groups.iter().position(|g| g.contains(&module));
            if let Some(g) = from {
                groups[g].retain(|m| *m != module);
            }
            // Hidden -> start -> centre -> end -> hidden.
            match from {
                None => groups[0].push(module),
                Some(2) => {}
                Some(g) => groups[g + 1].push(module),
            }
            next.modules =
                Some(BarModuleLayout::from_groups([&groups[0], &groups[1], &groups[2]]).ok()?);
        }
        SettingsAction::ModuleOrder(module, direction) => {
            let mut groups = current
                .modules
                .unwrap_or_else(default_module_layout)
                .groups();
            let g = groups.iter().position(|g| g.contains(&module))?;
            let i = groups[g].iter().position(|m| *m == module)?;
            let j = i.checked_add_signed(isize::from(direction))?;
            if j >= groups[g].len() {
                return None;
            }
            groups[g].swap(i, j);
            next.modules =
                Some(BarModuleLayout::from_groups([&groups[0], &groups[1], &groups[2]]).ok()?);
        }
    }
    Some(BarEdit::Overrides(next))
}

/// One bounded step; `None` past the range's end.
fn shell_step(next: &mut BarPreferenceOverrides, step: ShellStep, direction: i8) -> Option<()> {
    use in_gnu_os_settings_read_model::{
        BarDensityPercent, BarThickness, DeckMagnifyPercent, DeckReachTenths, DeckShoulderPx,
        DeckTuning, MotionTempoPercent, TitleMaxWidth,
    };
    use shell_defaults as d;
    let delta = i64::from(direction.signum());
    let stepped = |now: i64, by: i64| now + delta * by;
    let deck = next.deck.map_or(
        (
            d::DECK_MAGNIFY,
            d::DECK_REACH,
            d::DECK_TEMPO,
            d::DECK_SHOULDER,
        ),
        |t| {
            (
                i64::from(t.magnify.get()),
                i64::from(t.reach.get()),
                i64::from(t.tempo.get()),
                i64::from(t.shoulder.get()),
            )
        },
    );
    let set_deck = |next: &mut BarPreferenceOverrides, (m, r, t, s): (i64, i64, i64, i64)| {
        next.deck = Some(DeckTuning {
            magnify: DeckMagnifyPercent::try_from(m).ok()?,
            reach: DeckReachTenths::try_from(r).ok()?,
            tempo: MotionTempoPercent::try_from(t).ok()?,
            shoulder: DeckShoulderPx::try_from(s).ok()?,
        });
        Some(())
    };
    match step {
        ShellStep::Thickness => {
            let now = next.thickness.map_or(d::THICKNESS, |v| i64::from(v.get()));
            next.thickness = Some(BarThickness::try_from(stepped(now, 4)).ok()?);
        }
        ShellStep::Density => {
            let now = next.density.map_or(d::DENSITY, |v| i64::from(v.get()));
            next.density = Some(BarDensityPercent::try_from(stepped(now, 5)).ok()?);
        }
        ShellStep::TravelTempo => {
            let now = next.travel_tempo.map_or(d::TEMPO, |v| i64::from(v.get()));
            next.travel_tempo = Some(MotionTempoPercent::try_from(stepped(now, 25)).ok()?);
        }
        ShellStep::SpringTempo => {
            let now = next.spring_tempo.map_or(d::TEMPO, |v| i64::from(v.get()));
            next.spring_tempo = Some(MotionTempoPercent::try_from(stepped(now, 25)).ok()?);
        }
        ShellStep::TitleMax => {
            let now = next.title_max.map_or(d::TITLE_MAX, |v| i64::from(v.get()));
            next.title_max = Some(TitleMaxWidth::try_from(stepped(now, 20)).ok()?);
        }
        ShellStep::DeckMagnify => set_deck(next, (stepped(deck.0, 10), deck.1, deck.2, deck.3))?,
        ShellStep::DeckReach => set_deck(next, (deck.0, stepped(deck.1, 5), deck.2, deck.3))?,
        ShellStep::DeckTempo => set_deck(next, (deck.0, deck.1, stepped(deck.2, 25), deck.3))?,
        ShellStep::DeckShoulder => set_deck(next, (deck.0, deck.1, deck.2, stepped(deck.3, 4)))?,
    }
    Some(())
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsLine {
    Header(String),
    Caption(String),
    Value {
        label: String,
        value: String,
    },
    /// An observed on/off fact; never actionable.
    State {
        label: String,
        on: Option<bool>,
    },
    /// A row shared with the read-only Bar popouts.
    Info {
        kind: InfoRowKind,
        label: String,
    },
    UnderConstruction {
        reason: String,
    },
    /// A value one click steps to its next state.
    Choice {
        label: String,
        value: String,
        action: SettingsAction,
    },
    /// A count with step-down and step-up controls.
    Stepper {
        label: String,
        value: String,
        down: SettingsAction,
        up: SettingsAction,
    },
    /// MODS-01B: a module of the catalogue: its Tabler icon, its name, what
    /// it is and where it can stand, its place, and (when the profile can be
    /// written) `[place, earlier, later]` actions.
    Module {
        icon: String,
        label: String,
        detail: String,
        place: String,
        actions: Option<[SettingsAction; 3]>,
    },
}

impl SettingsLine {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Header(_) => "header",
            Self::Caption(_) => "caption",
            Self::Value { .. } => "value",
            Self::State { .. } => "state",
            Self::Info { .. } => "info",
            Self::UnderConstruction { .. } => "under_construction",
            Self::Choice { .. } => "choice",
            Self::Stepper { .. } => "stepper",
            Self::Module { .. } => "module",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Header(text) | Self::Caption(text) => text,
            Self::Value { label, .. }
            | Self::State { label, .. }
            | Self::Info { label, .. }
            | Self::Choice { label, .. }
            | Self::Stepper { label, .. }
            | Self::Module { label, .. } => label,
            Self::UnderConstruction { reason } => reason,
        }
    }
}

/// Everything a page may read.
pub struct SettingsFacts<'a> {
    pub preferences: &'a EffectiveProductPreferences,
    pub network: &'a NetworkSnapshot,
    pub bluetooth: &'a BluetoothSnapshot,
    pub power: PowerSnapshot,
    pub audio: &'a AudioSnapshot,
    /// Whether this build carries the profile client (R12) that can write.
    pub profile_writes: bool,
    /// The profile session's state, in builds that carry the client.
    pub profile_status: Option<&'a str>,
    /// A current projection exists and no request is in flight.
    pub profile_ready: bool,
    /// SETTINGS-02: facts the live controls show.
    pub live: &'a LiveFacts,
}

fn source(source: Option<PreferenceValueSource>) -> &'static str {
    match source {
        Some(PreferenceValueSource::ProductDefault) => "default",
        Some(PreferenceValueSource::SystemObservation) => "from the system",
        Some(PreferenceValueSource::DurableProfile) => "from your profile",
        None => "unavailable",
    }
}

fn projection(state: ProductPreferenceProjectionState) -> &'static str {
    match state {
        ProductPreferenceProjectionState::AuthorityUnavailable => "profile authority unavailable",
        ProductPreferenceProjectionState::NoDurableBinding => "no profile bound",
        ProductPreferenceProjectionState::ProfileDocumentAbsent => "no profile document",
        ProductPreferenceProjectionState::NoProfileSelection => "no profile selected",
        ProductPreferenceProjectionState::Selected => "profile selected",
        ProductPreferenceProjectionState::RecoveryRequired => "profile needs recovery",
        ProductPreferenceProjectionState::Unavailable => "unavailable",
    }
}

fn info_lines(popout: InfoPopout) -> Vec<SettingsLine> {
    popout
        .rows
        .into_iter()
        // Settings is where that row leads; it does not list it.
        .filter(|row| row.kind != InfoRowKind::OpenSettings)
        .map(|row| match row.kind {
            InfoRowKind::Header => SettingsLine::Header(row.label),
            InfoRowKind::Caption => SettingsLine::Caption(row.label),
            InfoRowKind::State { on } => SettingsLine::State {
                label: row.label,
                on,
            },
            kind => SettingsLine::Info {
                kind,
                label: row.label,
            },
        })
        .collect()
}

fn live_choice(label: &str, value: String, control: LiveControl) -> SettingsLine {
    SettingsLine::Choice {
        label: label.to_owned(),
        value,
        action: SettingsAction::Live(control),
    }
}

fn on_off_live(value: Option<bool>) -> String {
    match value {
        Some(true) => t("On").into(),
        Some(false) => t("Off").into(),
        None => t("Unknown").into(),
    }
}

/// SETTINGS-02: a popout's rows, with its first on/off fact (the radio)
/// as a live control.
fn live_info_lines(popout: InfoPopout, label: &str, control: LiveControl) -> Vec<SettingsLine> {
    let mut controlled = false;
    info_lines(popout)
        .into_iter()
        .map(|line| match line {
            SettingsLine::State { on, .. } if !controlled => {
                controlled = true;
                live_choice(label, on_off_live(on), control)
            }
            line => line,
        })
        .collect()
}

/// SETTINGS-02C: the networks in range. A secured network with no saved
/// connection asks for its passphrase.
fn wifi_lines(live: &LiveFacts) -> Vec<SettingsLine> {
    let mut lines = vec![SettingsLine::Header(t("Networks in range").into())];
    if let Some((what, typed)) = &live.prompt {
        lines.push(SettingsLine::Value {
            label: what.clone(),
            value: format!("{}|", "•".repeat(*typed)),
        });
        lines.push(SettingsLine::Caption(
            t("Type the passphrase. Enter connects, Escape cancels.").into(),
        ));
    }
    if live.wifi.is_empty() {
        lines.push(SettingsLine::Caption(
            if live.reading_wifi {
                t("Looking for networks…")
            } else {
                t("No network in range.")
            }
            .into(),
        ));
    }
    for (index, network) in live.wifi.iter().enumerate() {
        let label = format!(
            "{}{}  {} %",
            network.ssid,
            if network.secured { " (secured)" } else { "" },
            network.strength
        );
        let index = u8::try_from(index).ok();
        lines.push(match index {
            _ if network.active => SettingsLine::Value {
                label,
                value: t("Connected").into(),
            },
            Some(index) if network.can_connect_without_passphrase() => {
                live_choice(&label, t("Connect").into(), LiveControl::WifiConnect(index))
            }
            Some(index) => live_choice(
                &label,
                t("Enter password").into(),
                LiveControl::WifiPassword(index),
            ),
            None => SettingsLine::Value {
                label,
                value: t("Password needed").into(),
            },
        });
    }
    lines
}

/// SETTINGS-02: the battery or profile, the profile as a live control, and
/// keeping the session awake.
fn power_lines(facts: &SettingsFacts<'_>) -> Vec<SettingsLine> {
    let mut lines = info_lines(InfoPopout::battery(facts.power));
    lines.push(SettingsLine::Header(t("Power").into()));
    if let PowerSnapshot::Profile(profile) = facts.power {
        lines.push(live_choice(
            t("Power profile"),
            match profile {
                gnos_power::PowerProfile::PowerSaver => t("Power saver"),
                gnos_power::PowerProfile::Balanced => t("Balanced"),
                gnos_power::PowerProfile::Performance => t("Performance"),
            }
            .into(),
            LiveControl::PowerProfile,
        ));
    }
    lines.push(live_choice(
        t("Keep awake"),
        on_off_live(Some(facts.live.keep_awake)),
        LiveControl::KeepAwake,
    ));
    lines
}

#[cfg(feature = "component-lab")]
/// SET-03: every advanced value, each with its unit; a toggle is a choice,
/// the others step within their bounds.
fn advanced_lines(p: &crate::surface_prefs::SurfacePrefs) -> Vec<SettingsLine> {
    use crate::surface_prefs::{TUNABLES, Unit};
    let mut lines = vec![
        SettingsLine::Header(t("Advanced").into()),
        SettingsLine::Caption(t("Values the shell used to fix; each one returns to its default when stepped back onto it.").into()),
    ];
    for (i, tune) in TUNABLES.iter().enumerate() {
        let v = p.advanced(tune.slug);
        let s = SurfaceSetting::Advanced(i as u8);
        let label = t(tune.label);
        let value = match tune.unit {
            Unit::Ms => format!("{v} ms"),
            Unit::Px => format!("{v} px"),
            Unit::Pct => format!("{v} %"),
            Unit::Count => v.to_string(),
            Unit::Toggle => if v == 1 { t("On") } else { t("Off") }.to_owned(),
        };
        lines.push(if tune.unit == Unit::Toggle {
            live_choice(label, value, LiveControl::Surface(s, 1))
        } else {
            SettingsLine::Stepper {
                label: label.to_owned(),
                value,
                down: SettingsAction::Live(LiveControl::Surface(s, -1)),
                up: SettingsAction::Live(LiveControl::Surface(s, 1)),
            }
        });
    }
    lines
}

/// BEHAVIOUR-01: how the surfaces answer the pointer, in one place: for
/// all of them, then module by module. Session is shown, not offered: it
/// opens on a click only.
#[cfg(feature = "component-lab")]
fn behaviour_lines(
    p: &crate::surface_prefs::SurfacePrefs,
    thumbnail_apps: &[(String, Option<String>)],
) -> Vec<SettingsLine> {
    use SurfaceSetting as S;
    let b = &p.behaviour;
    let switch = |label: &'static str, on: bool, s: S| {
        live_choice(t(label), on_off(Some(on)), LiveControl::Surface(s, 1))
    };
    let step = |label: &'static str, value: String, s: S| SettingsLine::Stepper {
        label: t(label).to_owned(),
        value,
        down: SettingsAction::Live(LiveControl::Surface(s, -1)),
        up: SettingsAction::Live(LiveControl::Surface(s, 1)),
    };
    let ms = |v: u32| format!("{v} ms");
    let timing = |module: u8| {
        let t = b
            .timing(module)
            .unwrap_or(crate::surface_prefs::Timing::new(0, 0));
        [
            step("Delay before it opens", ms(t.dwell_ms), S::Dwell(module)), // i18n
            step("Grace before it goes in", ms(t.grace_ms), S::Grace(module)), // i18n
        ]
    };
    let mut lines = vec![
        SettingsLine::Header(t("All surfaces").into()),
        step(
            "A surface the pointer opens over one you opened", // i18n
            t(p.surfaces_meeting.label()).to_owned(),
            S::SurfacesMeeting,
        ),
        switch(
            "Sliding along the Bar opens the neighbour at once", // i18n
            b.warm_handoff,
            S::WarmHandoff,
        ),
        switch(
            "Surfaces read where the pointer is heading", // i18n
            b.aim,
            S::Aim,
        ),
        step(
            "Busy pointer while an application starts, at most", // i18n
            if b.launch_feedback_s == 0 {
                t("Off").to_owned()
            } else {
                format!("{} s", b.launch_feedback_s)
            },
            S::LaunchFeedback,
        ),
    ];
    if b.aim {
        lines.extend([
            step(
                "Margin around a surface the pointer is heading to", // i18n
                format!("{} px", b.aim_slack_px),
                S::AimSlack,
            ),
            step(
                "Speed from which the pointer is heading somewhere", // i18n
                format!("{} px/s", b.aim_min_speed),
                S::AimSpeed,
            ),
            step(
                "How far back the pointer's heading is read", // i18n
                ms(b.aim_window_ms),
                S::AimWindow,
            ),
        ]);
    }
    lines.extend([
        SettingsLine::Header(t("Workspaces panel").into()),
        step(
            "Opens", // i18n
            t(p.workspaces_open.label()).to_owned(),
            S::WorkspacesOpen,
        ),
    ]);
    if p.workspaces_open == crate::surface_prefs::PanelOpening::Hover {
        lines.extend(timing(0));
    }
    // WS-04: live thumbnails, and the applications never captured.
    lines.push(switch(
        "Live window thumbnails", // i18n
        p.thumbnails.live,
        S::ThumbnailsLive,
    ));
    if p.thumbnails.live {
        let r = p.thumbnails.refresh_ms;
        lines.push(step(
            "Thumbnails refresh every", // i18n
            if r < 1000 {
                format!("{r} ms")
            } else {
                format!("{} s", r / 1000)
            },
            S::ThumbnailRefresh,
        ));
        lines.push(SettingsLine::Caption(
            t("Taken only while the panel is out, kept in memory only, never while the session is locked.")
                .into(),
        ));
        lines.push(SettingsLine::Header(
            t("Applications shown as thumbnails").into(),
        ));
        for (index, (app, excluded_by)) in thumbnail_apps.iter().enumerate() {
            let Ok(index) = u8::try_from(index) else {
                break;
            };
            let value = match excluded_by {
                None => t("Shown").to_owned(),
                Some(entry) if entry == app => t("Not shown").to_owned(),
                Some(entry) => tf("Not shown ({entry})", &[("entry", entry)]),
            };
            lines.push(live_choice(
                app,
                value,
                LiveControl::Surface(S::ThumbnailApp(index), 1),
            ));
        }
        lines.push(SettingsLine::Caption(
            t("A window not shown keeps its application's icon. Password managers, authenticators and password prompts are not shown unless you say so.")
                .into(),
        ));
    }
    lines.extend([
        SettingsLine::Header(t("Dashboard").into()),
        switch(
            "Opens when the pointer rests on the clock", // i18n
            p.dashboard_on_hover,
            S::DashboardOnHover,
        ),
    ]);
    if p.dashboard_on_hover {
        lines.extend(timing(1));
    }
    lines.extend([
        SettingsLine::Header(t("Status popouts").into()),
        switch(
            "Open when the pointer rests on their icon", // i18n
            b.status_on_hover,
            S::StatusOnHover,
        ),
    ]);
    if b.status_on_hover {
        lines.extend(timing(2));
    }
    lines.extend([
        SettingsLine::Header(t("Tray menus").into()),
        switch(
            "Open when the pointer rests on their icon", // i18n
            b.tray_on_hover,
            S::TrayOnHover,
        ),
    ]);
    if b.tray_on_hover {
        lines.extend(timing(3));
    }
    lines.extend([
        SettingsLine::Header(t("Session").into()),
        SettingsLine::Value {
            label: t("Opens").to_owned(),
            value: t("On a click only").to_owned(),
        },
        SettingsLine::Caption(
            t("Its actions lock, suspend or turn the computer off: it never opens by passing over it.")
                .into(),
        ),
    ]);
    lines
}

/// SET-02A: what each surface shows (how each answers the pointer is
/// BEHAVIOUR-01's page).
#[cfg(feature = "component-lab")]
fn surface_lines(p: &crate::surface_prefs::SurfacePrefs) -> Vec<SettingsLine> {
    use SurfaceSetting as S;
    let on_off = |on: bool| if on { t("On") } else { t("Off") }.to_owned();
    let secs = |ms: u32| format!("{:.1} s", f64::from(ms) / 1000.0);
    let switch = |label: &'static str, on: bool, s: S| {
        live_choice(t(label), on_off(on), LiveControl::Surface(s, 1))
    };
    let step = |label: &'static str, value: String, s: S| SettingsLine::Stepper {
        label: t(label).to_owned(),
        value,
        down: SettingsAction::Live(LiveControl::Surface(s, -1)),
        up: SettingsAction::Live(LiveControl::Surface(s, 1)),
    };
    let pct = |p: u32| format!("{p} %");
    let m = &p.material;
    let mut lines = vec![
        SettingsLine::Header(t("Material").into()),
        switch("Translucent", m.translucent, S::Translucent), // i18n
        step("Shell opacity", pct(m.shell_opacity_pct), S::ShellOpacity), // i18n
        step("Module layers", pct(m.layer_opacity_pct), S::LayerOpacity), // i18n
        step("Tint", t(m.tint.label()).to_owned(), S::Tint),  // i18n
        switch("Blur what lies behind", m.blur, S::Blur),     // i18n
        live_choice(
            t("Blur mask"),
            t(m.blur_mask.label()).to_owned(),
            LiveControl::Surface(S::BlurMask, 1),
        ),
        SettingsLine::Header(t("Opacity per surface").into()),
    ];
    for (i, (slug, label)) in crate::surface_prefs::MATERIAL_SURFACES.iter().enumerate() {
        if let Ok(i) = u8::try_from(i) {
            let value = m
                .surface_opacity_pct
                .get(*slug)
                .map_or_else(|| t("Same as the shell").to_owned(), |v| pct(*v));
            lines.push(step(label, value, S::SurfaceOpacity(i)));
        }
    }
    lines.push(SettingsLine::Caption(
        t("Blur needs a compositor that offers it; opaque ignores the opacities.").into(),
    ));
    lines.extend([
        SettingsLine::Header(t("On-screen display").into()),
        switch(
            "Show volume and brightness changes",
            p.osd_shown,
            S::OsdShown,
        ),
        step("Stays for", secs(p.osd_dwell_ms), S::OsdDwell), // i18n
        SettingsLine::Header(t("Session").into()),
        step(
            "Hold to log out, reboot or power off",
            secs(p.session_hold_ms),
            S::SessionHold,
        ),
        SettingsLine::Header(t("Dashboard").into()),
        step(
            "Width", // i18n
            format!("{} px", p.dashboard_width),
            S::DashboardWidth,
        ),
    ]);
    for (i, tab) in crate::dashboard_panel::DashTab::ALL.iter().enumerate() {
        if let Ok(i) = u8::try_from(i) {
            lines.push(live_choice(
                tab.label(),
                on_off(!p.dashboard_hidden_tabs.contains(tab.slug())),
                LiveControl::Surface(S::DashboardTab(i), 1),
            ));
        }
    }
    lines.extend([
        SettingsLine::Header(t("Workspaces panel").into()),
        step(
            "View", // i18n
            t(p.workspaces_view.label()).to_owned(),
            S::WorkspacesView,
        ),
        step(
            "Miniature height", // i18n
            format!("{} px", p.workspaces_tile_px),
            S::WorkspacesTile,
        ),
        step(
            "Windows listed per workspace", // i18n
            p.workspaces_windows.to_string(),
            S::WorkspacesWindows,
        ),
        SettingsLine::Header(t("Desktop").into()),
        switch("Clock", p.desktop_clock, S::DesktopClock),
        step("Clock size", pct(p.clock_scale_pct), S::ClockScale), // i18n
        switch("Date under the clock", p.clock_date, S::ClockDate), // i18n
        switch("Seconds on the clock", p.clock_seconds, S::ClockSeconds), // i18n // i18n
        switch(
            "Audio visualiser",
            p.desktop_visualiser,
            S::DesktopVisualiser,
        ),
        step(
            "Visualiser size", // i18n
            pct(p.visualiser_scale_pct),
            S::VisualiserScale,
        ),
        SettingsLine::Header(t("Utilities tiles").into()),
    ]);
    let tiles_at = lines.len();
    let fit = &p.bar_fit;
    let mut fit_lines = vec![
        SettingsLine::Header(t("When the Bar is too short").into()),
        switch(
            "Hide the modules that do not fit", // i18n
            fit.hide_what_does_not_fit,
            S::BarFitHide,
        ),
        SettingsLine::Caption(t("Modules give way in this order:").into()),
    ];
    for (n, slug) in fit.order().into_iter().enumerate() {
        let Some(i) = crate::surface_prefs::BAR_MODULES
            .iter()
            .position(|(s, _)| *s == slug)
            .and_then(|i| u8::try_from(i).ok())
        else {
            continue;
        };
        let label = crate::surface_prefs::BAR_MODULES[usize::from(i)].1;
        fit_lines.push(step(label, format!("{}", n + 1), S::BarGiveWay(i)));
    }
    // Before the tiles: the Bar's section reads with the material.
    lines.splice(tiles_at - 1..tiles_at - 1, fit_lines);
    for (i, kind) in crate::utilities_panel::ToggleKind::ALL.iter().enumerate() {
        if let Ok(i) = u8::try_from(i) {
            lines.push(live_choice(
                kind.label(),
                on_off(p.tile_shown(kind.slug())),
                LiveControl::Surface(S::Tile(i), 1),
            ));
        }
    }
    lines.push(SettingsLine::Caption(
        t("Turning the visualiser off also stops its audio capture.").into(),
    ));
    lines
}

/// WALL-01: the wallpaper Gnosix draws on each output's desktop layer.
fn wallpaper_lines(live: &LiveFacts) -> Vec<SettingsLine> {
    let Some((enabled, name, fit, count, folder)) = &live.wallpaper else {
        return vec![SettingsLine::Caption(
            t("Reading the wallpaper settings…").into(),
        )];
    };
    let mut lines = vec![
        SettingsLine::Header(t("Wallpaper").into()),
        live_choice(
            t("Gnosix draws the wallpaper"),
            if *enabled { t("On") } else { t("Off") }.into(),
            LiveControl::WallpaperEnabled,
        ),
        SettingsLine::Stepper {
            label: t("Picture").into(),
            value: name.clone(),
            down: SettingsAction::Live(LiveControl::WallpaperPicture(-1)),
            up: SettingsAction::Live(LiveControl::WallpaperPicture(1)),
        },
        live_choice(t("Fit"), t(fit).into(), LiveControl::WallpaperFit),
        SettingsLine::Caption(tf(
            "{count} pictures in {folder}",
            &[("count", count), ("folder", folder)],
        )),
    ];
    if !enabled {
        lines.push(SettingsLine::Caption(
            t("While it is off, another program may draw the wallpaper.").into(),
        ));
    }
    lines
}

/// I18N-01: the shell's language.
fn language_lines(live: &LiveFacts) -> Vec<SettingsLine> {
    let name = |code: &str| {
        gnos_i18n::LANGUAGES
            .iter()
            .find(|(c, _)| *c == code)
            .map_or(code.to_owned(), |(_, n)| (*n).to_owned())
    };
    let value = match &live.language_choice {
        Some(code) => name(code),
        None => tf(
            "System ({language})",
            &[("language", &name(&live.language_active))],
        ),
    };
    vec![
        SettingsLine::Header(t("Language").into()),
        live_choice(t("Shell language"), value, LiveControl::Language),
        SettingsLine::Caption(
            t("Applies at once; a few texts follow at their next change.").into(),
        ),
        SettingsLine::Caption(t("Dates and numbers follow the system's region.").into()),
    ]
}

/// APPS-01: one choice per role; a click moves to the next installed
/// application that can take it.
fn default_app_lines(live: &LiveFacts) -> Vec<SettingsLine> {
    use gnosix_application_catalog::defaults::DefaultRole;
    let mut lines = vec![SettingsLine::Header(t("Default applications").into())];
    for (index, role) in DefaultRole::ALL.iter().enumerate() {
        let (current, choices) = live.default_apps.get(index).cloned().unwrap_or((None, 0));
        let value = current.unwrap_or_else(|| t("None").into());
        let Ok(index) = u8::try_from(index) else {
            break;
        };
        lines.push(if choices > 0 {
            live_choice(t(role.label()), value, LiveControl::DefaultApp(index))
        } else {
            SettingsLine::Value {
                label: t(role.label()).to_owned(),
                value: tf("{value} (no other installed)", &[("value", &value)]),
            }
        });
    }
    lines.push(SettingsLine::Caption(
        t("Written to mimeapps.list: every application that opens files and links follows it.")
            .into(),
    ));
    lines
}

/// SETTINGS-03B: do not disturb, the toast life, critical notifications,
/// and one rule per application.
fn notification_lines(live: &LiveFacts) -> Vec<SettingsLine> {
    let prefs = &live.notification_prefs;
    let mut lines = vec![SettingsLine::Header(t("Notifications").into())];
    if let Some(owner) = &live.notification_service {
        lines.push(SettingsLine::Caption(tf(
            "Another program serves notifications ({owner}): these settings apply once Gnosix does.",
            &[("owner", owner)],
        )));
    }
    lines.push(live_choice(
        t("Do not disturb"),
        on_off_live(Some(live.do_not_disturb)),
        LiveControl::DoNotDisturb,
    ));
    lines.push(SettingsLine::Stepper {
        label: t("Toasts stay").into(),
        value: format!("{} s", prefs.toast_secs),
        down: SettingsAction::Live(LiveControl::NotificationToast(-1)),
        up: SettingsAction::Live(LiveControl::NotificationToast(1)),
    });
    lines.push(live_choice(
        t("Urgent ones through do not disturb"),
        if prefs.critical_through_dnd {
            t("On")
        } else {
            t("Off")
        }
        .into(),
        LiveControl::NotificationCritical,
    ));
    lines.push(SettingsLine::Header(t("Applications").into()));
    if live.notification_apps.is_empty() {
        lines.push(SettingsLine::Caption(
            t("Applications appear here once they have sent a notification.").into(),
        ));
    }
    for (index, app) in live.notification_apps.iter().enumerate() {
        if let Ok(index) = u8::try_from(index) {
            lines.push(live_choice(
                app,
                prefs.rule(app).label().into(),
                LiveControl::NotificationApp(index),
            ));
        }
    }
    lines.push(SettingsLine::Caption(
        t("Silent: no toast, kept in the centre. Off: not kept at all.").into(),
    ));
    lines
}

/// A delay, or "Off".
fn delay_label(secs: Option<u32>) -> String {
    match secs {
        None => t("Off").to_owned(),
        Some(_) => gnos_lock::minutes_label(secs),
    }
}

/// LOCK-01: every lock screen setting. They apply from the next lock (the
/// idle delay and lock-before-sleep apply at once).
fn lock_lines(config: &gnos_lock::LockConfig) -> Vec<SettingsLine> {
    use gnos_lock::LockSetting as S;
    let step = |label: &str, value: String, setting: S| SettingsLine::Stepper {
        label: label.to_owned(),
        value,
        down: SettingsAction::Live(LiveControl::Lock(setting, -1)),
        up: SettingsAction::Live(LiveControl::Lock(setting, 1)),
    };
    let toggle = |label: &str, on: bool, setting: S| {
        live_choice(
            label,
            if on { t("On") } else { t("Off") }.into(),
            LiveControl::Lock(setting, 1),
        )
    };
    vec![
        SettingsLine::Header(t("When it locks").into()),
        step(
            t("Lock after being idle"),
            delay_label(config.idle_lock_secs),
            S::IdleLock,
        ),
        toggle(
            t("Lock before sleep"),
            config.lock_before_sleep,
            S::LockBeforeSleep,
        ),
        step(
            t("Screens off after locking"),
            delay_label(config.screens_off_secs),
            S::ScreensOff,
        ),
        SettingsLine::Header(t("What it shows").into()),
        toggle(t("Clock"), config.show_clock, S::ShowClock),
        toggle(t("Date"), config.show_date, S::ShowDate),
        live_choice(
            t("Clock cycle"),
            match config.clock_24h {
                None => t("System"),
                Some(false) => "12 hour",
                Some(true) => "24 hour",
            }
            .into(),
            LiveControl::Lock(S::ClockCycle, 1),
        ),
        toggle(t("User name"), config.show_user, S::ShowUser),
        toggle(
            t("One dot per typed character"),
            config.show_typed_length,
            S::ShowTypedLength,
        ),
        SettingsLine::Header(t("Wrong passwords").into()),
        step(
            t("Attempts before waiting"),
            config.attempts_before_wait.to_string(),
            S::Attempts,
        ),
        step(t("Wait"), delay_label(Some(config.wait_secs)), S::Wait),
        SettingsLine::Caption(
            t("The message shown under the clock is set in the lock.json file for now.").into(),
        ),
    ]
}

/// SETTINGS-02: each display's and backlight's brightness, stepped 5 %.
fn display_lines(live: &LiveFacts) -> Vec<SettingsLine> {
    use gnos_utilities::{DisplaySetting as D, Side, rate_label, rotation_label, side_of};
    let mut lines = Vec::new();
    if let Some(secs) = live.display_revert_in {
        lines.push(SettingsLine::Header(
            t("Keep these display settings?").into(),
        ));
        lines.push(live_choice(
            t("Keep"),
            t("Keep").into(),
            LiveControl::DisplayKeep,
        ));
        lines.push(live_choice(
            t("Revert"),
            format!("in {secs} s"),
            LiveControl::DisplayRevert,
        ));
    }
    for (index, output) in live.displays.iter().enumerate() {
        let Ok(i) = u8::try_from(index) else {
            break;
        };
        let step = |label: &str, value: String, setting: D| SettingsLine::Stepper {
            label: label.to_owned(),
            value,
            down: SettingsAction::Live(LiveControl::Display(i, setting, -1)),
            up: SettingsAction::Live(LiveControl::Display(i, setting, 1)),
        };
        lines.push(SettingsLine::Header(if output.description.is_empty() {
            output.name.clone()
        } else {
            format!("{} · {}", output.name, output.description)
        }));
        lines.push(step(
            t("Resolution"),
            format!("{} × {}", output.mode.width, output.mode.height),
            D::Size,
        ));
        lines.push(step(
            t("Refresh rate"),
            rate_label(output.mode.refresh_mhz),
            D::Rate,
        ));
        lines.push(step(
            t("Scale"),
            format!("{:.0} %", output.scale * 100.0),
            D::Scale,
        ));
        lines.push(live_choice(
            t("Rotation"),
            rotation_label(output.transform),
            LiveControl::Display(i, D::Rotation, 1),
        ));
        if index > 0
            && let Some(first) = live.displays.first()
        {
            let side: Side = side_of(output, first);
            lines.push(live_choice(
                t("Position"),
                format!("{} {}", t(side.label()), first.name),
                LiveControl::Display(i, D::Position, 1),
            ));
        }
    }
    if !live.displays.is_empty() {
        lines.push(SettingsLine::Caption(
            t("A change applies at once and goes back by itself unless you keep it.").into(),
        ));
    }
    lines.push(SettingsLine::Header(t("Brightness").into()));
    if live.brightness.is_empty() {
        lines.push(SettingsLine::Caption(
            if live.reading_brightness {
                t("Reading the displays…")
            } else {
                t("No display reports its brightness (DDC/CI or a backlight).")
            }
            .into(),
        ));
    }
    for (index, (label, level)) in live.brightness.iter().enumerate() {
        let Ok(index) = u8::try_from(index) else {
            break;
        };
        lines.push(SettingsLine::Stepper {
            label: label.clone(),
            value: level.map_or("…".into(), |l| format!("{l} %")),
            down: SettingsAction::Live(LiveControl::Brightness(index, -1)),
            up: SettingsAction::Live(LiveControl::Brightness(index, 1)),
        });
    }
    lines
}

fn on_off(value: Option<bool>) -> String {
    match value {
        Some(true) => t("On").into(),
        Some(false) => t("Off").into(),
        None => t("Default").into(),
    }
}

/// MODS-01: the modules the hull booted from their manifests, and what the
/// mods folder holds (listed, never run).
fn plugin_lines(registry: &'static gnos_modules::Registry) -> Vec<SettingsLine> {
    let place = |p: gnos_modules::Place| match p {
        gnos_modules::Place::Bar => t("Bar"),
        gnos_modules::Place::WorkspaceManager => t("Workspace manager"),
        gnos_modules::Place::Desktop => t("Desktop"),
    };
    let not_run = |r: &gnos_modules::Refusal| match r {
        gnos_modules::Refusal::NotNative => {
            t("Not run: third-party modules do not run yet").to_owned()
        }
        other => tf("Not run: {reason}", &[("reason", &other.to_string())]),
    };
    let mut lines = vec![
        SettingsLine::Header(t("Modules").into()),
        SettingsLine::Caption(t("Booted by the hull from their manifests.").into()),
    ];
    for m in &registry.booted {
        let places: Vec<&str> = m.places.iter().map(|p| place(*p)).collect();
        lines.push(SettingsLine::Value {
            label: t(m.name.as_str()).to_owned(),
            value: format!("{} · {}", places.join(", "), t("Native")),
        });
    }
    for (origin, refusal) in &registry.refused {
        lines.push(SettingsLine::Value {
            label: origin.label(),
            value: not_run(refusal),
        });
    }
    lines.push(SettingsLine::Header(t("Mods").into()));
    if let Some(dir) = gnos_modules::mods_dir() {
        lines.push(SettingsLine::Value {
            label: t("Folder").into(),
            value: dir.display().to_string(),
        });
    }
    if registry.found.is_empty() {
        lines.push(SettingsLine::Caption(
            t("No mods found. Third-party modules do not run yet.").into(),
        ));
    }
    for found in &registry.found {
        lines.push(SettingsLine::Value {
            label: found
                .manifest
                .as_ref()
                .map_or_else(|| found.origin.label(), |m| m.name.clone()),
            value: not_run(&found.refusal),
        });
    }
    lines
}

pub fn page_lines(page: PageId, facts: &SettingsFacts<'_>) -> Vec<SettingsLine> {
    if let Some(reason) = page.missing_power() {
        return vec![SettingsLine::UnderConstruction {
            reason: reason.to_owned(),
        }];
    }
    let p = facts.preferences;
    let value = |label: &str, value: String| SettingsLine::Value {
        label: label.to_owned(),
        value,
    };
    match page {
        PageId::Appearance => vec![
            SettingsLine::Header(t("Appearance").into()),
            value(
                t("Colour scheme"),
                format!(
                    "{} ({})",
                    match p.color_scheme {
                        ProductColorScheme::Light => t("Light"),
                        ProductColorScheme::Dark => t("Dark"),
                    },
                    source(Some(p.color_scheme_source))
                ),
            ),
            value(
                t("Reduced motion"),
                format!(
                    "{} ({})",
                    if p.reduced_motion { t("On") } else { t("Off") },
                    source(Some(p.reduced_motion_source))
                ),
            ),
            value(
                t("Text scale"),
                format!(
                    "{}% ({})",
                    p.text_scale_basis_points / 100,
                    source(Some(p.text_scale_source))
                ),
            ),
            SettingsLine::Caption(t("These follow your system settings.").into()),
        ],
        PageId::Bar => {
            let o = &p.bar_overrides;
            let edge = format!(
                "{} ({})",
                match p.bar_edge {
                    Some(BarEdgePreference::Top) => t("Top"),
                    Some(BarEdgePreference::Right) => t("Right"),
                    Some(BarEdgePreference::Bottom) => t("Bottom"),
                    Some(BarEdgePreference::Left) => t("Left"),
                    None => t("Unknown"),
                },
                source(p.bar_edge_source)
            );
            let count = o
                .workspace_count
                .map_or(t("Default").into(), |c| c.get().to_string());
            let display = o
                .workspace_display
                .map_or(t("Default").into(), |m| format!("{m:?}"));
            let mut lines = vec![SettingsLine::Header(t("Bar").into())];
            if facts.profile_writes && facts.profile_ready {
                let choice = |label: &str, value: String, action| SettingsLine::Choice {
                    label: label.to_owned(),
                    value,
                    action,
                };
                lines.extend([
                    choice(t("Edge"), edge, SettingsAction::CycleEdge),
                    choice(
                        t("Always visible"),
                        on_off(o.persistent),
                        SettingsAction::CyclePersistent,
                    ),
                    choice(
                        t("Show on hover"),
                        on_off(o.show_on_hover),
                        SettingsAction::CycleShowOnHover,
                    ),
                    SettingsLine::Stepper {
                        label: t("Workspaces shown").into(),
                        value: count,
                        down: SettingsAction::WorkspaceCountDown,
                        up: SettingsAction::WorkspaceCountUp,
                    },
                    choice(
                        t("Workspace display"),
                        display,
                        SettingsAction::CycleWorkspaceDisplay,
                    ),
                    choice(
                        t("Compact tray"),
                        on_off(o.tray_compact),
                        SettingsAction::CycleTrayCompact,
                    ),
                ]);
                lines.extend(shell_lines(o, true));
            } else {
                lines.extend([
                    value(t("Edge"), edge),
                    value(t("Always visible"), on_off(o.persistent)),
                    value(t("Show on hover"), on_off(o.show_on_hover)),
                    value(t("Workspaces shown"), count),
                    value(t("Workspace display"), display),
                    value(t("Compact tray"), on_off(o.tray_compact)),
                ]);
                lines.extend(shell_lines(o, false));
            }
            lines.push(value(
                t("Profile"),
                facts
                    .profile_status
                    .map_or_else(|| projection(p.projection_state).to_owned(), str::to_owned),
            ));
            lines.push(SettingsLine::Caption(if !facts.profile_writes {
                t("Read-only here: this build has no profile client to write.").into()
            } else if facts.profile_ready {
                t("A click steps a value; each change goes to your profile authority.").into()
            } else {
                t("Waiting for the profile authority; values are read-only meanwhile.").into()
            }));
            lines
        }
        PageId::Deck => {
            let writable = facts.profile_writes && facts.profile_ready;
            let mut lines = vec![SettingsLine::Header(t("Deck").into())];
            lines.extend(deck_lines(&p.bar_overrides, writable));
            lines.push(value(
                t("Profile"),
                facts
                    .profile_status
                    .map_or_else(|| projection(p.projection_state).to_owned(), str::to_owned),
            ));
            lines.push(SettingsLine::Caption(
                t("The Deck's magnification wave; each value stays within a range that keeps it coherent.")
                    .into(),
            ));
            lines
        }
        PageId::Network => {
            let mut lines = live_info_lines(
                InfoPopout::network(facts.network),
                t("Wi-Fi"),
                LiveControl::Wifi,
            );
            lines.extend(wifi_lines(facts.live));
            lines
        }
        PageId::Bluetooth => {
            let mut lines = live_info_lines(
                InfoPopout::bluetooth(facts.bluetooth),
                t("Bluetooth"),
                LiveControl::Bluetooth,
            );
            lines.push(SettingsLine::Header(t("Paired devices").into()));
            if facts.live.bluetooth.is_empty() {
                lines.push(SettingsLine::Caption(t("No paired device.").into()));
            }
            for (index, (alias, connected)) in facts.live.bluetooth.iter().enumerate() {
                if let Ok(index) = u8::try_from(index) {
                    lines.push(live_choice(
                        alias,
                        if *connected {
                            t("Disconnect")
                        } else {
                            t("Connect")
                        }
                        .into(),
                        LiveControl::BluetoothDevice(index),
                    ));
                }
            }
            lines.push(SettingsLine::Caption(
                t("Pairing a new device is not yet offered (it needs a pairing agent).").into(),
            ));
            lines
        }
        PageId::Power => power_lines(facts),
        PageId::Audio => audio_lines(facts.audio),
        PageId::Display => display_lines(facts.live),
        PageId::Lock => lock_lines(&facts.live.lock),
        PageId::Notifications => notification_lines(facts.live),
        PageId::Apps => default_app_lines(facts.live),
        PageId::LanguageAndRegion => language_lines(facts.live),
        PageId::WallpaperAndStyle => wallpaper_lines(facts.live),
        #[cfg(feature = "component-lab")]
        PageId::Surfaces => surface_lines(&facts.live.surfaces),
        #[cfg(not(feature = "component-lab"))]
        PageId::Surfaces => unreachable!("answered by missing_power"),
        #[cfg(feature = "component-lab")]
        PageId::Behaviour => behaviour_lines(&facts.live.surfaces, &facts.live.thumbnail_apps),
        #[cfg(not(feature = "component-lab"))]
        PageId::Behaviour => unreachable!("answered by missing_power"),
        #[cfg(feature = "component-lab")]
        PageId::Advanced => advanced_lines(&facts.live.surfaces),
        #[cfg(not(feature = "component-lab"))]
        PageId::Advanced => unreachable!("answered by missing_power"),
        PageId::Plugins => plugin_lines(crate::modules::registry()),
        PageId::About => vec![
            SettingsLine::Header(t("Gnosix").into()),
            value(
                t("Shell"),
                format!("gnos-ux-gpui {}", env!("CARGO_PKG_VERSION")),
            ),
            value(t("Profile"), projection(p.projection_state).to_owned()),
            SettingsLine::Header(t("Observed services").into()),
            value(
                t("Network"),
                if facts.network.provider == gnos_network::NetworkProviderState::Available {
                    t("NetworkManager").into()
                } else {
                    t("Unavailable").into()
                },
            ),
            value(
                t("Bluetooth"),
                if facts.bluetooth.provider == gnos_bluetooth::BluetoothProviderState::Available {
                    t("BlueZ").into()
                } else {
                    t("Unavailable").into()
                },
            ),
            value(
                t("Power"),
                if facts.power == PowerSnapshot::Unavailable {
                    t("Unavailable").into()
                } else {
                    t("UPower").into()
                },
            ),
            value(
                t("Audio"),
                if facts.audio.provider == AudioProviderState::Available {
                    t("PipeWire").into()
                } else {
                    t("Unavailable").into()
                },
            ),
        ],
        _ => Vec::new(),
    }
}

fn audio_lines(audio: &AudioSnapshot) -> Vec<SettingsLine> {
    if audio.provider != AudioProviderState::Available {
        return vec![
            SettingsLine::Header(t("Audio").into()),
            SettingsLine::Caption(t("Audio status unavailable").into()),
        ];
    }
    let model = crate::bar_audio_popout::AudioPopoutModel::from_snapshot(audio);
    let mut lines = vec![SettingsLine::Header(t("Volume").into())];
    if let Some(output) = model.output {
        lines.push(SettingsLine::Stepper {
            label: t("Output").into(),
            value: output
                .level
                .map_or("…".into(), |l| format!("{:.0} %", l * 100.0)),
            down: SettingsAction::Live(LiveControl::OutputVolume(-1)),
            up: SettingsAction::Live(LiveControl::OutputVolume(1)),
        });
        lines.push(live_choice(
            t("Output muted"),
            if output.muted { t("Yes") } else { t("No") }.into(),
            LiveControl::OutputMute,
        ));
    }
    if let Some(input) = model.input {
        lines.push(live_choice(
            t("Microphone"),
            if input.muted { t("Muted") } else { t("On") }.into(),
            LiveControl::Microphone,
        ));
    }
    for (kind, header, default) in [
        (
            AudioEndpointKind::Output,
            t("Output devices"),
            audio.defaults.output.effective.resolved,
        ),
        (
            AudioEndpointKind::Input,
            t("Input devices"),
            audio.defaults.input.effective.resolved,
        ),
    ] {
        lines.push(SettingsLine::Header(header.into()));
        let mut endpoints: Vec<_> = audio
            .endpoints
            .values()
            .filter(|e| e.kind == kind)
            .collect();
        endpoints.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        if endpoints.is_empty() {
            lines.push(SettingsLine::Caption(t("None").into()));
        }
        for endpoint in endpoints {
            let is_default = Some(endpoint.id) == default;
            lines.push(live_choice(
                &endpoint.display_name,
                if is_default { t("Default") } else { t("Use") }.into(),
                LiveControl::DefaultDevice(endpoint.id, kind),
            ));
        }
    }
    lines.push(SettingsLine::Caption(
        t("Choose a device to make it the default.").into(),
    ));
    lines
}

/// BAR-01E: a bounded value as a stepper when the profile can be written,
/// as a value otherwise.
fn bounded_line(label: &str, value: String, step: ShellStep, writable: bool) -> SettingsLine {
    if writable {
        SettingsLine::Stepper {
            label: label.to_owned(),
            value,
            down: SettingsAction::Step(step, -1),
            up: SettingsAction::Step(step, 1),
        }
    } else {
        SettingsLine::Value {
            label: label.to_owned(),
            value,
        }
    }
}

fn toggle_line(label: &str, on: bool, action: SettingsAction, writable: bool) -> SettingsLine {
    let value = if on { t("On") } else { t("Off") }.to_owned();
    if writable {
        SettingsLine::Choice {
            label: label.to_owned(),
            value,
            action,
        }
    } else {
        SettingsLine::Value {
            label: label.to_owned(),
            value,
        }
    }
}

fn tempo(percent: i64) -> String {
    format!("x{:.2}", percent as f32 / 100.0)
}

/// The Bar page's shell preferences (B03): thickness, density, motion,
/// clock, title, status icons and the modules.
fn shell_lines(o: &BarPreferenceOverrides, writable: bool) -> Vec<SettingsLine> {
    use shell_defaults as d;
    let n = |v: Option<i64>, default: i64| v.unwrap_or(default);
    let clock = o.clock.unwrap_or_default();
    let icons = o.status_icons.unwrap_or_default();
    let mut lines = vec![
        SettingsLine::Header(t("Size and motion").into()),
        bounded_line(
            t("Thickness"),
            format!(
                "{} px",
                n(o.thickness.map(|v| i64::from(v.get())), d::THICKNESS)
            ),
            ShellStep::Thickness,
            writable,
        ),
        bounded_line(
            t("Density"),
            format!("{} %", n(o.density.map(|v| i64::from(v.get())), d::DENSITY)),
            ShellStep::Density,
            writable,
        ),
        bounded_line(
            t("Travel speed"),
            tempo(n(o.travel_tempo.map(|v| i64::from(v.get())), d::TEMPO)),
            ShellStep::TravelTempo,
            writable,
        ),
        bounded_line(
            t("Motion speed"),
            tempo(n(o.spring_tempo.map(|v| i64::from(v.get())), d::TEMPO)),
            ShellStep::SpringTempo,
            writable,
        ),
        SettingsLine::Header(t("Clock and status").into()),
        toggle_line(
            "24-hour clock",
            clock.hour24,
            SettingsAction::CycleClock(ClockField::Hour24),
            writable,
        ),
        toggle_line(
            t("Date"),
            clock.date,
            SettingsAction::CycleClock(ClockField::Date),
            writable,
        ),
        bounded_line(
            t("Title width"),
            format!(
                "{} px",
                n(o.title_max.map(|v| i64::from(v.get())), d::TITLE_MAX)
            ),
            ShellStep::TitleMax,
            writable,
        ),
    ];
    for (label, on, field) in [
        (t("Keyboard locks"), icons.lock, StatusField::Lock),
        (t("Network"), icons.network, StatusField::Network),
        (t("Bluetooth"), icons.bluetooth, StatusField::Bluetooth),
        (t("Battery"), icons.battery, StatusField::Battery),
    ] {
        lines.push(toggle_line(
            label,
            on,
            SettingsAction::ToggleStatus(field),
            writable,
        ));
    }
    lines.extend(module_catalogue(
        &o.modules.unwrap_or_else(default_module_layout),
        writable,
    ));
    lines
}

/// MODS-01B: the Bar's module catalogue, from the manifests the hull booted:
/// the modules on the Bar in its order, then those available. Each shows
/// its icon, name, what it is and where it can stand; its place cycles
/// start, centre, end and off the Bar, and two arrows move it earlier or
/// later. Only the modules the hull boots for the Bar are offered (the
/// profile may still name others, such as the donor's logo button).
fn module_catalogue(layout: &BarModuleLayout, writable: bool) -> Vec<SettingsLine> {
    let registry = crate::modules::registry();
    let manifest = |m: BarModule| {
        registry
            .module(module_slug(m))
            .filter(|man| man.places.contains(&gnos_modules::Place::Bar))
    };
    let place_name = |p: gnos_modules::Place| match p {
        gnos_modules::Place::Bar => t("Bar"),
        gnos_modules::Place::WorkspaceManager => t("Workspace manager"),
        gnos_modules::Place::Desktop => t("Desktop"),
    };
    let line = |m: BarModule, place: String| {
        let man = manifest(m)?;
        let places: Vec<&str> = man.places.iter().map(|p| place_name(*p)).collect();
        Some(SettingsLine::Module {
            icon: man.icon.clone(),
            label: t(man.name.as_str()).to_owned(),
            detail: format!("{} · {}", t(man.description.as_str()), places.join(", ")),
            place,
            actions: writable.then_some([
                SettingsAction::ModuleGroup(m),
                SettingsAction::ModuleOrder(m, -1),
                SettingsAction::ModuleOrder(m, 1),
            ]),
        })
    };
    let groups = layout.groups();
    let group_names = [t("Start"), t("Centre"), t("End")];
    let mut on_bar = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        for (i, m) in group.iter().filter(|m| manifest(**m).is_some()).enumerate() {
            on_bar.extend(line(*m, format!("{} {}", group_names[g], i + 1)));
        }
    }
    let available: Vec<SettingsLine> = BarModule::ALL
        .into_iter()
        .filter(|m| !groups.iter().any(|g| g.contains(m)))
        .filter_map(|m| line(m, t("Off the Bar").to_owned()))
        .collect();
    let mut lines = vec![SettingsLine::Header(t("Modules on the Bar").into())];
    if on_bar.is_empty() {
        lines.push(SettingsLine::Caption(
            t("No module stands on the Bar.").into(),
        ));
    }
    lines.extend(on_bar);
    if !available.is_empty() {
        lines.push(SettingsLine::Header(t("Available modules").into()));
        lines.extend(available);
    }
    lines
}

/// DECK-01E: the wave's four values (P06 ranges).
fn deck_lines(o: &BarPreferenceOverrides, writable: bool) -> Vec<SettingsLine> {
    use shell_defaults as d;
    let (m, r, speed, s) = o.deck.map_or(
        (
            d::DECK_MAGNIFY,
            d::DECK_REACH,
            d::DECK_TEMPO,
            d::DECK_SHOULDER,
        ),
        |wave| {
            (
                i64::from(wave.magnify.get()),
                i64::from(wave.reach.get()),
                i64::from(wave.tempo.get()),
                i64::from(wave.shoulder.get()),
            )
        },
    );
    vec![
        bounded_line(t("Peak"), tempo(m), ShellStep::DeckMagnify, writable),
        bounded_line(
            t("Reach"),
            tf("{n} slots", &[("n", &format!("{:.1}", r as f32 / 10.0))]),
            ShellStep::DeckReach,
            writable,
        ),
        bounded_line(t("Speed"), tempo(speed), ShellStep::DeckTempo, writable),
        bounded_line(
            t("Tab shoulders"),
            format!("{s} px"),
            ShellStep::DeckShoulder,
            writable,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    static LIVE: LiveFacts = LiveFacts {
        do_not_disturb: false,
        keep_awake: false,
        brightness: Vec::new(),
        reading_brightness: false,
        wifi: Vec::new(),
        reading_wifi: false,
        notification_prefs: gnos_notifications::prefs::NotificationPrefs {
            toast_secs: 5,
            critical_through_dnd: true,
            apps: std::collections::BTreeMap::new(),
        },
        notification_apps: Vec::new(),
        notification_service: None,
        default_apps: Vec::new(),
        language_choice: None,
        language_active: String::new(),
        wallpaper: None,
        #[cfg(feature = "component-lab")]
        surfaces: crate::surface_prefs::SurfacePrefs::DEFAULT,
        #[cfg(feature = "component-lab")]
        thumbnail_apps: Vec::new(),
        displays: Vec::new(),
        display_revert_in: None,
        lock: gnos_lock::LockConfig {
            idle_lock_secs: None,
            lock_before_sleep: false,
            screens_off_secs: None,
            show_clock: false,
            show_date: false,
            clock_24h: None,
            show_user: false,
            message: String::new(),
            attempts_before_wait: 1,
            wait_secs: 5,
            show_typed_length: false,
        },
        bluetooth: Vec::new(),
        prompt: None,
    };

    #[cfg(feature = "component-lab")]
    #[test]
    fn the_behaviour_page_lists_thumbnails_and_each_application_concerned() {
        let mut p = crate::surface_prefs::SurfacePrefs::default();
        let apps = vec![
            ("kitty".to_owned(), None),
            ("keepassxc".to_owned(), Some("keepassxc".to_owned())),
            ("pinentry-qt".to_owned(), Some("pinentry*".to_owned())),
        ];
        let choice = |lines: &[SettingsLine], label: &str| {
            lines.iter().find_map(|l| match l {
                SettingsLine::Choice {
                    label: l,
                    value,
                    action,
                } if l == label => Some((value.clone(), *action)),
                _ => None,
            })
        };
        let lines = behaviour_lines(&p, &apps);
        let (value, _) = choice(&lines, "Live window thumbnails").expect("switch");
        assert_eq!(value, "On");
        assert_eq!(
            choice(&lines, "kitty"),
            Some((
                "Shown".to_owned(),
                SettingsAction::Live(LiveControl::Surface(SurfaceSetting::ThumbnailApp(0), 1))
            ))
        );
        assert_eq!(choice(&lines, "keepassxc").unwrap().0, "Not shown");
        assert_eq!(
            choice(&lines, "pinentry-qt").unwrap().0,
            "Not shown (pinentry*)"
        );
        // Off, the page keeps the switch alone.
        p.thumbnails.live = false;
        let lines = behaviour_lines(&p, &apps);
        assert!(choice(&lines, "Live window thumbnails").is_some());
        assert!(choice(&lines, "kitty").is_none());
    }

    #[test]
    fn the_display_page_offers_each_output_and_asks_to_keep_a_change() {
        use gnos_utilities::{DisplayMode, DisplayOutput, DisplaySetting as D};
        let output = |name: &str, x: i32| DisplayOutput {
            name: name.into(),
            description: "Panel".into(),
            mode: DisplayMode {
                width: 1920,
                height: 1080,
                refresh_mhz: 60000,
            },
            modes: vec![DisplayMode {
                width: 1920,
                height: 1080,
                refresh_mhz: 60000,
            }],
            x,
            y: 0,
            scale: 1.0,
            transform: 0,
            disabled: false,
        };
        let mut live = LiveFacts {
            displays: vec![output("HDMI-A-1", 0), output("DP-2", 1920)],
            ..LiveFacts::default()
        };
        let lines = display_lines(&live);
        let actions: Vec<_> = lines
            .iter()
            .filter_map(|l| match l {
                SettingsLine::Stepper {
                    up: SettingsAction::Live(c),
                    ..
                }
                | SettingsLine::Choice {
                    action: SettingsAction::Live(c),
                    ..
                } => Some(*c),
                _ => None,
            })
            .collect();
        assert!(actions.contains(&LiveControl::Display(0, D::Size, 1)));
        assert!(actions.contains(&LiveControl::Display(1, D::Rate, 1)));
        assert!(actions.contains(&LiveControl::Display(0, D::Rotation, 1)));
        // Only the second output is placed against the first.
        assert!(!actions.contains(&LiveControl::Display(0, D::Position, 1)));
        assert!(actions.contains(&LiveControl::Display(1, D::Position, 1)));
        assert!(lines.iter().any(
            |l| matches!(l, SettingsLine::Choice { value, .. } if value == "Right of HDMI-A-1")
        ));
        assert!(!actions.contains(&LiveControl::DisplayKeep));
        live.display_revert_in = Some(12);
        let lines = display_lines(&live);
        assert!(
            matches!(&lines[0], SettingsLine::Header(h) if h == "Keep these display settings?")
        );
        assert!(lines.iter().any(|l| matches!(l, SettingsLine::Choice { value, action: SettingsAction::Live(LiveControl::DisplayRevert), .. } if value == "in 12 s")));
    }

    #[test]
    fn live_pages_offer_controls_for_the_powers_that_exist() {
        let (p, n, b, a) = empty();
        let f = facts(&p, &n, &b, &a);
        let live = |page| {
            page_lines(page, &f)
                .into_iter()
                .filter_map(|line| match line {
                    SettingsLine::Choice {
                        action: SettingsAction::Live(c),
                        ..
                    } => Some(c),
                    SettingsLine::Stepper {
                        up: SettingsAction::Live(c),
                        ..
                    } => Some(c),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            live(PageId::Notifications),
            [
                LiveControl::DoNotDisturb,
                LiveControl::NotificationToast(1),
                LiveControl::NotificationCritical
            ]
        );
        assert_eq!(
            live(PageId::Power),
            [LiveControl::PowerProfile, LiveControl::KeepAwake]
        );
        // Nothing read yet: the Display page says so and offers no stepper.
        assert!(live(PageId::Display).is_empty());
        let reading = LiveFacts {
            brightness: vec![("Private A".into(), Some(40)), ("eDP".into(), None)],
            ..LiveFacts::default()
        };
        let lines = display_lines(&reading);
        assert!(lines.iter().any(|l| matches!(l,
            SettingsLine::Stepper { value, up: SettingsAction::Live(LiveControl::Brightness(0, 1)), .. }
                if value == "40 %")));
        assert!(lines.iter().any(|l| matches!(l,
            SettingsLine::Stepper { value, down: SettingsAction::Live(LiveControl::Brightness(1, -1)), .. }
                if value == "…")));
        // A live control is never a profile write.
        assert_eq!(
            bar_edit(
                SettingsAction::Live(LiveControl::Wifi),
                None,
                &BarPreferenceOverrides::default()
            ),
            None
        );
    }

    fn facts<'a>(
        preferences: &'a EffectiveProductPreferences,
        network: &'a NetworkSnapshot,
        bluetooth: &'a BluetoothSnapshot,
        audio: &'a AudioSnapshot,
    ) -> SettingsFacts<'a> {
        SettingsFacts {
            preferences,
            network,
            bluetooth,
            power: PowerSnapshot::Profile(gnos_power::PowerProfile::Balanced),
            audio,
            profile_writes: false,
            profile_status: None,
            profile_ready: false,
            live: &LIVE,
        }
    }

    fn empty() -> (
        EffectiveProductPreferences,
        NetworkSnapshot,
        BluetoothSnapshot,
        AudioSnapshot,
    ) {
        (
            EffectiveProductPreferences::default(),
            NetworkSnapshot {
                provider: gnos_network::NetworkProviderState::Unavailable,
                generation: 0,
                provider_owner: None,
                globals: None,
                devices: Default::default(),
                active_connections: Default::default(),
                settings_connections: Default::default(),
            },
            BluetoothSnapshot {
                provider: gnos_bluetooth::BluetoothProviderState::Unavailable,
                generation: 0,
                provider_owner: None,
                adapters: Default::default(),
                devices: Default::default(),
            },
            AudioSnapshot {
                provider: AudioProviderState::Unavailable,
                generation: 0,
                endpoints: Default::default(),
                defaults: Default::default(),
            },
        )
    }

    #[test]
    fn every_page_has_content_and_powerless_pages_say_what_they_wait_for() {
        let (p, n, b, a) = empty();
        let f = facts(&p, &n, &b, &a);
        let mut construction = 0;
        for (page, _) in PAGES {
            let lines = page_lines(page, &f);
            assert!(!lines.is_empty(), "{page:?}");
            if let [SettingsLine::UnderConstruction { reason }] = lines.as_slice() {
                assert!(reason.contains("yet"), "{page:?}");
                construction += 1;
            }
        }
        // Updates only: Plugins lists the booted modules (MODS-01); a build
        // without the shell's surfaces has no Surfaces, Behaviour or
        // Advanced page.
        let expected = if cfg!(feature = "component-lab") {
            1
        } else {
            4
        };
        assert_eq!(construction, expected);
        let plugins = page_lines(PageId::Plugins, &f);
        assert!(plugins.contains(&SettingsLine::Value {
            label: "Clock".into(),
            value: "Bar, Desktop · Native".into()
        }));
    }

    #[test]
    fn the_bar_page_reads_the_projection_and_says_it_cannot_write() {
        let (p, n, b, a) = empty();
        let lines = page_lines(PageId::Bar, &facts(&p, &n, &b, &a));
        let labels: Vec<_> = lines.iter().map(|l| l.label().to_owned()).collect();
        assert_eq!(labels[1], "Edge");
        assert!(lines.contains(&SettingsLine::Value {
            label: "Profile".into(),
            value: "profile authority unavailable".into()
        }));
        assert!(labels.last().unwrap().contains("no profile client"));
    }

    #[test]
    fn shell_edits_step_within_their_ranges() {
        let none = BarPreferenceOverrides::default();
        let edit = |action, from: &BarPreferenceOverrides| match bar_edit(action, None, from) {
            Some(BarEdit::Overrides(o)) => Some(o),
            _ => None,
        };
        let up = edit(SettingsAction::Step(ShellStep::Thickness, 1), &none).unwrap();
        assert_eq!(up.thickness.unwrap().get(), 64);
        // SET-03: up to the widened bound.
        let mut top = up;
        while let Some(next) = edit(SettingsAction::Step(ShellStep::Thickness, 1), &top) {
            top = next;
        }
        assert_eq!(top.thickness.unwrap().get(), 120);
        assert!(
            edit(SettingsAction::Step(ShellStep::Thickness, 1), &top).is_none(),
            "the end"
        );
        let deck = edit(SettingsAction::Step(ShellStep::DeckMagnify, -1), &none).unwrap();
        let t = deck.deck.unwrap();
        assert_eq!(
            (
                t.magnify.get(),
                t.reach.get(),
                t.tempo.get(),
                t.shoulder.get()
            ),
            (140, 25, 100, 24)
        );
        let clock = edit(SettingsAction::CycleClock(ClockField::Date), &none).unwrap();
        assert_eq!(clock.clock.map(|c| (c.hour24, c.date)), Some((false, true)));
        let status = edit(SettingsAction::ToggleStatus(StatusField::Bluetooth), &none).unwrap();
        assert!(!status.status_icons.unwrap().bluetooth && status.status_icons.unwrap().battery);
    }

    #[test]
    fn the_module_catalogue_lists_the_bar_in_order_then_what_is_available() {
        use in_gnu_os_settings_read_model::BarModule as M;
        let layout = BarModuleLayout::from_groups([
            &[M::Logo, M::Workspaces, M::ActiveWindow],
            &[],
            &[M::StatusIcons, M::Power],
        ])
        .expect("valid layout");
        let lines = module_catalogue(&layout, true);
        let modules: Vec<(&str, &str)> = lines
            .iter()
            .filter_map(|l| match l {
                SettingsLine::Module { label, place, .. } => Some((label.as_str(), place.as_str())),
                _ => None,
            })
            .collect();
        // The donor's logo button is not booted: never offered.
        assert_eq!(
            modules,
            vec![
                ("Workspaces", "Start 1"),
                ("Active window", "Start 2"),
                ("Quick settings", "End 1"),
                ("Session", "End 2"),
                ("Tray", "Off the Bar"),
                ("Clock", "Off the Bar"),
            ]
        );
        let SettingsLine::Module {
            icon,
            detail,
            actions,
            ..
        } = &lines[1]
        else {
            panic!("a module line first");
        };
        assert_eq!(icon, "layout-grid");
        assert!(detail.ends_with("Bar, Workspace manager"), "{detail}");
        assert_eq!(
            *actions,
            Some([
                SettingsAction::ModuleGroup(M::Workspaces),
                SettingsAction::ModuleOrder(M::Workspaces, -1),
                SettingsAction::ModuleOrder(M::Workspaces, 1),
            ])
        );
        // Without a profile client, the place only.
        assert!(module_catalogue(&layout, false).iter().all(|l| !matches!(
            l,
            SettingsLine::Module {
                actions: Some(_),
                ..
            }
        )));
    }

    #[test]
    fn modules_move_between_groups_and_within_them() {
        use in_gnu_os_settings_read_model::BarModule as M;
        let none = BarPreferenceOverrides::default();
        let edit = |action, from: &BarPreferenceOverrides| match bar_edit(action, None, from) {
            Some(BarEdit::Overrides(o)) => Some(o),
            _ => None,
        };
        // The clock leaves the end group: hidden.
        let hidden = edit(SettingsAction::ModuleGroup(M::Clock), &none).unwrap();
        assert!(!hidden.modules.unwrap().groups()[2].contains(&M::Clock));
        assert!(
            hidden
                .modules
                .unwrap()
                .groups()
                .iter()
                .all(|g| !g.contains(&M::Clock))
        );
        // Hidden -> start, at its end.
        let start = edit(SettingsAction::ModuleGroup(M::Clock), &hidden).unwrap();
        assert_eq!(
            start.modules.unwrap().groups()[0],
            [M::Logo, M::Workspaces, M::Clock]
        );
        let earlier = edit(SettingsAction::ModuleOrder(M::Clock, -1), &start).unwrap();
        assert_eq!(
            earlier.modules.unwrap().groups()[0],
            [M::Logo, M::Clock, M::Workspaces]
        );
        assert!(
            edit(SettingsAction::ModuleOrder(M::Logo, -1), &earlier).is_none(),
            "first"
        );
    }

    #[test]
    fn the_deck_page_steps_the_wave_when_writable() {
        let (p, n, b, a) = empty();
        let mut f = facts(&p, &n, &b, &a);
        let read_only = page_lines(PageId::Deck, &f);
        assert!(read_only.contains(&SettingsLine::Value {
            label: "Peak".into(),
            value: "x1.50".into()
        }));
        f.profile_writes = true;
        f.profile_ready = true;
        let lines = page_lines(PageId::Deck, &f);
        assert_eq!(
            lines
                .iter()
                .filter(|l| matches!(l, SettingsLine::Stepper { .. }))
                .count(),
            4
        );
    }

    #[test]
    fn a_ready_profile_turns_bar_values_into_controls() {
        let (p, n, b, a) = empty();
        let mut f = facts(&p, &n, &b, &a);
        f.profile_writes = true;
        f.profile_ready = true;
        f.profile_status = Some("Current");
        let kinds: Vec<_> = page_lines(PageId::Bar, &f)
            .iter()
            .map(SettingsLine::name)
            .collect();
        assert_eq!(
            &kinds[..7],
            [
                "header", "choice", "choice", "choice", "stepper", "choice", "choice"
            ]
        );
        assert_eq!(&kinds[kinds.len() - 2..], ["value", "caption"]);
        // BAR-01E: the shell preferences follow as controls.
        let lines = page_lines(PageId::Bar, &f);
        for label in [
            "Thickness",
            "Density",
            "Travel speed",
            "Motion speed",
            "Title width",
        ] {
            assert!(
                lines
                    .iter()
                    .any(|l| matches!(l, SettingsLine::Stepper { label: x, .. } if x == label)),
                "{label}"
            );
        }
        // MODS-01B: the clock in the catalogue, at its place, movable.
        assert!(lines.iter().any(|l| matches!(l,
            SettingsLine::Module { label, place, actions: Some(_), .. }
                if label == "Clock" && place == "End 2")));
        f.profile_ready = false;
        let waiting = page_lines(PageId::Bar, &f);
        assert!(waiting.iter().all(|l| !matches!(
            l,
            SettingsLine::Module {
                actions: Some(_),
                ..
            }
        )));
        assert!(
            waiting
                .iter()
                .all(|l| !matches!(l, SettingsLine::Choice { .. }))
        );
        assert!(waiting.last().unwrap().label().contains("Waiting"));
    }

    #[test]
    fn actions_step_through_the_typed_domain_only() {
        use in_gnu_os_settings_read_model::{WorkspaceDisplayCount, WorkspaceDisplayMode};
        let none = BarPreferenceOverrides::default();
        assert_eq!(
            bar_edit(
                SettingsAction::CycleEdge,
                Some(BarEdgePreference::Left),
                &none
            ),
            Some(BarEdit::Edge(BarEdgePreference::Top))
        );
        let step = |action, from: &BarPreferenceOverrides| match bar_edit(action, None, from) {
            Some(BarEdit::Overrides(next)) => Some(next),
            _ => None,
        };
        let on = step(SettingsAction::CyclePersistent, &none).unwrap();
        assert_eq!(on.persistent, Some(true));
        let off = step(SettingsAction::CyclePersistent, &on).unwrap();
        assert_eq!(off.persistent, Some(false));
        assert_eq!(step(SettingsAction::CyclePersistent, &off).unwrap(), none);
        let six = step(SettingsAction::WorkspaceCountUp, &none).unwrap();
        assert_eq!(six.workspace_count.map(|c| c.get()), Some(6));
        let mut one = none.clone();
        one.workspace_count = Some(WorkspaceDisplayCount::try_from(1i64).unwrap());
        assert_eq!(step(SettingsAction::WorkspaceCountDown, &one), None);
        let mut icons = none.clone();
        icons.workspace_display = Some(WorkspaceDisplayMode::Icons);
        assert_eq!(
            step(SettingsAction::CycleWorkspaceDisplay, &icons)
                .unwrap()
                .workspace_display,
            None
        );
    }

    #[test]
    fn connectivity_pages_reuse_the_popout_rows() {
        let (p, n, b, a) = empty();
        let f = facts(&p, &n, &b, &a);
        assert_eq!(
            page_lines(PageId::Power, &f)[1],
            SettingsLine::Info {
                kind: InfoRowKind::Text,
                label: "Power profile: Balanced".into()
            }
        );
        assert_eq!(
            page_lines(PageId::Network, &f)[1].label(),
            "Network status unavailable"
        );
        assert_eq!(
            page_lines(PageId::Audio, &f)[1].label(),
            "Audio status unavailable"
        );
    }
}
