//! UXE-01B: scene-side owner of the Bar-attached popout.
//!
//! The scene forwards raw pointer, wheel and key events here and paints what
//! `frame()` and `audio_rows()` return. This module owns: rebuilding the Bar
//! router when the presentation changes, which popout is shown, the slide-out
//! motion, world geometry, and the connective hull joining the popout to the
//! rail. It never dispatches: audio effects are returned as `AudioIntent`s for
//! the scene's existing AUDIO-01 sink.
//!
//! Audio is interactive (UXE-01). Network, Bluetooth and battery are
//! read-only (UXE-04, `bar_status_popouts`). Other popouts the router asks
//! for (lock, tray menu, active window) are treated as "nothing shown".

use crate::bar_audio_popout::{
    AUDIO_POPOUT_WIDTH, AudioPopoutEffect, AudioPopoutModel, AudioPopoutSurface, AudioRow,
    PendingCapability, popout_top,
};
use crate::bar_pointer_routing::{
    BarPointerConfig, BarPointerIntent, BarPointerRouter, BarPopout, PopoutChange, PopoutContext,
    StatusEntryKind,
};
use crate::bar_status_popouts::InfoPopout;
use crate::product_audio_action::AudioIntent;
use crate::rail_topology::RailSide;
use crate::reference_bar::{self, BORDER_ROUNDING, BORDER_THICKNESS, BarPresentation};
use gnos_audio::AudioSnapshot;
use gnos_ux_surface::grammar::connective_hull::ConnectiveHullGeometry;
use gnos_ux_surface::grammar::handoff::{ContentStrip, MorphFrame};
use gnos_ux_surface::grammar::{
    ExtrusionMotion, GeometryRect, TemporalResponseMode, fused_hull, revealed,
};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoutFrame {
    /// Full-size popout rectangle in scene coordinates.
    pub rect: GeometryRect,
    /// Currently revealed part (slides out of the rail).
    pub visible: GeometryRect,
    pub openness: f32,
    /// Rail + popout as one outline; `None` only if the joint is refused.
    pub hull: Option<ConnectiveHullGeometry>,
    /// TOPO-01A: the rail's side; the popout grows away from it.
    pub side: RailSide,
}

#[derive(Debug, Default, PartialEq)]
pub struct HostOutcome {
    /// Something visible changed; the scene should repaint.
    pub changed: bool,
    /// The event belonged to the popout and must not reach other surfaces.
    pub consumed: bool,
    pub intents: Vec<AudioIntent>,
    pub pending: Vec<PendingCapability>,
    /// "Open settings" was pressed in the popout of this status entry.
    pub open_settings: Option<StatusEntryKind>,
    /// SURF-03: a press (`true`) or release in the dashboard, in its
    /// coordinates, for the scene's dashboard panel.
    pub dashboard_press: Option<(f32, f32, bool)>,
    /// RAIL-01: a wheel step the scene carries out (workspace, brightness).
    pub bar_intent: Option<BarPointerIntent>,
}

pub struct BarPopoutHost {
    config: BarPointerConfig,
    router: Option<BarPointerRouter>,
    router_key: Option<String>,
    audio_model: Option<AudioPopoutModel>,
    audio: Option<AudioPopoutSurface>,
    audio_generation: Option<u64>,
    shown: Option<f32>,
    /// Which status entry the shown popout belongs to.
    shown_kind: StatusEntryKind,
    network: Option<InfoPopout>,
    bluetooth: Option<InfoPopout>,
    battery: Option<InfoPopout>,
    target_open: bool,
    /// The popout's extrusion out of the rail (envelope grammar).
    motion: ExtrusionMotion,
    preview: Option<f32>,
    last_tick: Option<Instant>,
    epoch: Instant,
    viewport: (f32, f32),
    reported_anchor: Option<Option<f32>>,
    reported_status_anchors: Option<[Option<f32>; 3]>,
    side: RailSide,
    /// SURF-03: the shown popout is the dashboard (from the clock).
    dashboard: bool,
    /// FOCUS-01: the popout the pointer designates on the rail, and where
    /// along the rail it was designated. Whether it comes out is the
    /// shell's attention's (`crate::attention`).
    designated: Option<BarPopout>,
    designated_at: Option<(BarPopout, f32)>,
    /// FOCUS-01B: the body changing shape from one popout to the next, and
    /// their contents passing along it (`grammar::handoff`). Both start
    /// when the popout comes out and end when it is back in.
    morph: Option<MorphFrame>,
    strip: Option<ContentStrip<BarPopout>>,
    /// FOCUS-01D: how deep it may come out while a surface stands with it.
    give_way: Option<f32>,
    dashboard_size: (f32, f32),
}

impl Default for BarPopoutHost {
    fn default() -> Self {
        Self::new()
    }
}

impl BarPopoutHost {
    pub fn new() -> Self {
        Self {
            config: BarPointerConfig {
                dashboard_popout: true,
                ..BarPointerConfig::default()
            },
            router: None,
            router_key: None,
            audio_model: None,
            audio: None,
            audio_generation: None,
            shown: None,
            shown_kind: StatusEntryKind::Audio,
            network: None,
            bluetooth: None,
            battery: None,
            target_open: false,
            motion: ExtrusionMotion::closed(),
            preview: None,
            last_tick: None,
            epoch: Instant::now(),
            viewport: (0.0, 0.0),
            reported_anchor: None,
            reported_status_anchors: None,
            side: RailSide::Left,
            dashboard: false,
            designated: None,
            designated_at: None,
            morph: None,
            strip: None,
            give_way: None,
            dashboard_size: (
                crate::dashboard_panel::dashboard_width(),
                crate::dashboard_panel::DASHBOARD_HEIGHT,
            ),
        }
    }

    fn micros(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    /// Re-derives routing and audio content from current observations.
    /// `active_window_extent` is the rendered module's length along the rail.
    pub fn sync(
        &mut self,
        presentation: &BarPresentation,
        viewport: (f32, f32),
        active_window_extent: f32,
        audio: &AudioSnapshot,
    ) {
        let length = self.side.length(viewport);
        let key = format!(
            "{:?}|{}|{:?}|{}|{:?}|{}",
            reference_bar::reference_module_layout(presentation).entries,
            reference_bar::describe_clock_and_status(presentation),
            presentation
                .workspaces
                .iter()
                .map(|w| w.active)
                .collect::<Vec<_>>(),
            length,
            self.side,
            active_window_extent,
        );
        if self.router_key.as_deref() != Some(key.as_str()) || self.viewport != viewport {
            self.router = BarPointerRouter::new_on(
                presentation,
                self.config,
                length,
                length,
                active_window_extent,
                self.side.is_horizontal(),
            )
            .ok();
            self.router_key = Some(key);
            self.viewport = viewport;
        }
        let model = AudioPopoutModel::from_snapshot(audio);
        // Rebuilding mid-drag would cancel the gesture the user is making.
        if self.audio_generation != Some(audio.generation) && self.preview.is_none() {
            self.audio = AudioPopoutSurface::new(model.clone()).ok();
            self.audio_generation = Some(audio.generation);
        }
        self.audio_model = Some(model);
    }

    /// UXE-04: re-derives the read-only popouts. Returns whether the shown
    /// one changed.
    pub fn sync_status(
        &mut self,
        network: &gnos_network::NetworkSnapshot,
        bluetooth: &gnos_bluetooth::BluetoothSnapshot,
        power: gnos_power::PowerSnapshot,
    ) -> bool {
        let before = self.info();
        self.network = Some(InfoPopout::network(network));
        self.bluetooth = Some(InfoPopout::bluetooth(bluetooth));
        self.battery = Some(InfoPopout::battery(power));
        before != self.info()
    }

    /// The shown status entry, while a popout is open or sliding.
    pub fn shown_kind(&self) -> Option<StatusEntryKind> {
        self.shown
            .filter(|_| !self.dashboard)
            .map(|_| self.shown_kind)
    }

    /// Whether `(x, y)` is on the revealed popout.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.local(x, y).is_some()
    }

    /// SURF-03: whether the shown (or sliding) popout is the dashboard.
    pub fn dashboard_shown(&self) -> bool {
        self.dashboard && self.shown.is_some()
    }

    pub fn dashboard_open(&self) -> bool {
        self.dashboard && self.target_open
    }

    /// FOCUS-01: the popout the pointer designates on the rail, if any.
    pub fn designated(&self) -> Option<BarPopout> {
        self.designated
    }

    /// FOCUS-01: the popout that is out (or sliding), if any.
    pub fn shown_popout(&self) -> Option<BarPopout> {
        self.shown.map(|_| {
            if self.dashboard {
                BarPopout::Dashboard
            } else {
                BarPopout::Status(self.shown_kind)
            }
        })
    }

    /// FOCUS-01: bring `popout` out, at the place the pointer designated it,
    /// else where the rail has its module, else at `fallback` (BAR-02A: the
    /// clock hidden in the "…"). Content swaps in place when another popout
    /// is out. Returns whether anything changed; a popout without content
    /// (the lock, tray menus, the active window) does not come out.
    pub fn show(&mut self, popout: BarPopout, fallback: Option<f32>) -> bool {
        let designated = self
            .designated_at
            .filter(|(p, _)| *p == popout)
            .map(|(_, at)| at);
        let routed = match popout {
            BarPopout::Dashboard => self.router.as_ref().and_then(|r| r.clock_centre()),
            BarPopout::Status(kind) => self.status_anchor(kind),
            _ => None,
        };
        let Some(anchor) = designated.or(routed).or(fallback) else {
            return false;
        };
        let (dashboard, kind) = match popout {
            BarPopout::Dashboard => (true, self.shown_kind),
            BarPopout::Status(
                kind @ (StatusEntryKind::Audio
                | StatusEntryKind::Network
                | StatusEntryKind::Bluetooth
                | StatusEntryKind::BatteryOrProfile),
            ) => (false, kind),
            _ => return false,
        };
        let changed = !self.target_open
            || self.shown != Some(anchor)
            || self.dashboard != dashboard
            || (!dashboard && self.shown_kind != kind);
        // FOCUS-01B: on screen, another popout hands off to this one: its
        // content passes along the Bar from the side its module lies on.
        let previous = self.shown_popout().zip(self.shown);
        let on_screen = self.motion.is_visible();
        self.shown = Some(anchor);
        self.dashboard = dashboard;
        self.shown_kind = kind;
        self.target_open = true;
        let extent = self.extent_of(popout).unwrap_or(0.0);
        // While any of the body is on screen it is a handoff: it never
        // starts afresh over a surface still visible.
        let path = match (previous, on_screen) {
            (Some((was, at)), true) => {
                if was != popout {
                    let was_extent = self.extent_of(was).unwrap_or(0.0);
                    let strip = self
                        .strip
                        .get_or_insert_with(|| ContentStrip::start(was, was_extent));
                    strip.show(popout, extent, anchor > at);
                    "handoff"
                } else {
                    "same"
                }
            }
            _ => {
                self.strip = Some(ContentStrip::start(popout, extent));
                self.morph = None;
                "fresh"
            }
        };
        if changed {
            println!(
                "GNOS_UX_POPOUT_HANDOFF path={path} popout={popout:?} on_screen={on_screen} phase={:.3} morph={}",
                self.motion.phase(),
                self.morph.is_some()
            );
        }
        changed
    }

    /// A popout's body `(width, height)`, from its content.
    fn size_of(&self, popout: BarPopout) -> Option<(f32, f32)> {
        match popout {
            BarPopout::Dashboard => Some(self.dashboard_size),
            BarPopout::Status(StatusEntryKind::Audio) => {
                Some((AUDIO_POPOUT_WIDTH, self.audio.as_ref()?.height()))
            }
            BarPopout::Status(kind) => self.info_of(kind).map(|i| (i.width, i.height)),
            _ => None,
        }
    }

    /// Its extent along the Bar.
    fn extent_of(&self, popout: BarPopout) -> Option<f32> {
        let (w, h) = self.size_of(popout)?;
        Some(if self.side.is_horizontal() { w } else { h })
    }

    /// FOCUS-01B: each content on the surface and its offset along the Bar
    /// from the body's leading edge (one, at 0, but during a handoff).
    pub fn contents(&self) -> Vec<(BarPopout, f32)> {
        match (&self.strip, self.shown_popout()) {
            (Some(strip), _) => strip.offsets(),
            (None, Some(p)) => vec![(p, 0.0)],
            _ => Vec::new(),
        }
    }

    /// Each content on the surface with its offset and its body's size.
    pub fn content_parts(&self) -> Vec<(BarPopout, f32, (f32, f32))> {
        self.contents()
            .into_iter()
            .filter_map(|(p, offset)| Some((p, offset, self.size_of(p)?)))
            .collect()
    }

    /// The offset of the shown popout's content along the Bar.
    fn shown_offset(&self) -> f32 {
        self.shown_popout()
            .and_then(|p| self.strip.as_ref()?.offset_of(p))
            .unwrap_or(0.0)
    }

    /// A read-only popout's content, whichever is shown.
    pub fn info_of(&self, kind: StatusEntryKind) -> Option<InfoPopout> {
        match kind {
            StatusEntryKind::Network => self.network.clone(),
            StatusEntryKind::Bluetooth => self.bluetooth.clone(),
            StatusEntryKind::BatteryOrProfile => self.battery.clone(),
            StatusEntryKind::Audio | StatusEntryKind::Lock => None,
        }
    }

    /// The audio popout's rows, whichever is shown.
    pub fn audio_rows_any(&self) -> Vec<AudioRow> {
        self.audio
            .as_ref()
            .map(AudioPopoutSurface::rows)
            .unwrap_or_default()
    }

    /// FOCUS-01: put the popout back into the rail.
    pub fn hide(&mut self) -> bool {
        std::mem::replace(&mut self.target_open, false)
    }

    /// Content of the shown read-only popout; None for audio.
    pub fn info(&self) -> Option<InfoPopout> {
        self.shown?;
        if self.dashboard {
            return None;
        }
        match self.shown_kind {
            StatusEntryKind::Network => self.network.clone(),
            StatusEntryKind::Bluetooth => self.bluetooth.clone(),
            StatusEntryKind::BatteryOrProfile => self.battery.clone(),
            StatusEntryKind::Audio | StatusEntryKind::Lock => None,
        }
    }

    fn is_audio(&self) -> bool {
        !self.dashboard && self.shown_kind == StatusEntryKind::Audio
    }

    /// The router's context: what the pointer designates (FOCUS-01: a
    /// popout may be designated before, or without, coming out).
    fn context(&self, in_panel: bool) -> PopoutContext {
        PopoutContext {
            current: self.designated,
            tray_submenu_open: false,
            pointer_in_popout_panel: in_panel,
        }
    }

    /// What the router says the pointer designates on the rail.
    fn designate(&mut self, change: PopoutChange) {
        match change {
            PopoutChange::Show { popout, anchor_y } => {
                self.designated = Some(popout);
                self.designated_at = Some((popout, anchor_y));
            }
            PopoutChange::Hide => self.designated = None,
        }
    }

    fn apply_audio(&mut self, effects: Vec<AudioPopoutEffect>, outcome: &mut HostOutcome) {
        for effect in effects {
            match effect {
                AudioPopoutEffect::PreviewLevel(level) => {
                    self.preview = Some(level);
                    outcome.changed = true;
                }
                AudioPopoutEffect::ClearPreview => {
                    self.preview = None;
                    outcome.changed = true;
                }
                AudioPopoutEffect::Intent(intent) => outcome.intents.push(intent),
                // Settings exists now (UXE-05B): opening it needs no authority.
                AudioPopoutEffect::Pending(PendingCapability::OpenAudioSettings) => {
                    outcome.open_settings = Some(StatusEntryKind::Audio);
                }
            }
        }
    }

    /// TOPO-01A: the rail's side. Returns whether it changed.
    pub fn set_side(&mut self, side: RailSide) -> bool {
        let changed = self.side != side;
        self.side = side;
        changed
    }

    /// A scene point in rail space `(across, along)`, where the router was
    /// built.
    fn to_rail(&self, x: f32, y: f32) -> (f32, f32) {
        self.side.to_rail(x, y, self.viewport)
    }

    /// CURSOR-01: what the pointer is over on the popout (`None`: off it):
    /// an audio control or "Open settings" it can press. The read-only
    /// switches do not react, so they keep the arrow. On the dashboard,
    /// [`Self::dashboard_point`] gives the point for its own buttons.
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        let (lx, ly) = self.local(x, y)?;
        let inside = |rx: f32, ry: f32, rw: f32, rh: f32| {
            lx >= rx && lx < rx + rw && ly >= ry && ly < ry + rh
        };
        let press = if self.dashboard {
            false
        } else if self.is_audio() {
            self.audio_rows().iter().any(|r| {
                inside(r.x, r.y, r.width, r.height)
                    && !matches!(
                        r.kind,
                        crate::bar_audio_popout::AudioRowKind::Header
                            | crate::bar_audio_popout::AudioRowKind::VolumeLabel
                    )
            })
        } else {
            self.info().is_some_and(|info| {
                info.rows.iter().any(|r| {
                    r.kind == crate::bar_status_popouts::InfoRowKind::OpenSettings
                        && inside(r.x, r.y, r.width, r.height)
                })
            })
        };
        Some(if press {
            crate::cursor::Affordance::Press
        } else {
            crate::cursor::Affordance::Inert
        })
    }

    /// CURSOR-01: a scene point in the dashboard's coordinates, when the
    /// dashboard is shown there.
    pub fn dashboard_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        self.local(x, y).filter(|_| self.dashboard)
    }

    /// A scene point in the shown content's coordinates (FOCUS-01B: the
    /// content may still be passing along the Bar).
    fn local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let frame = self.frame()?;
        let offset = self.shown_offset();
        let (dx, dy) = if self.side.is_horizontal() {
            (offset, 0.0)
        } else {
            (0.0, offset)
        };
        (frame.openness > 0.5 && frame.visible.contains_point(x, y))
            .then(|| (x - frame.rect.left() - dx, y - frame.rect.top() - dy))
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> HostOutcome {
        let mut outcome = HostOutcome::default();
        let t = self.micros();
        let local = self.local(x, y);
        if local.is_some() {
            outcome.consumed = true;
        }
        if let (Some((lx, ly)), true, Some(audio)) = (local, self.is_audio(), self.audio.as_mut())
            && let Ok(effects) = audio.pointer_move(lx, ly, t)
        {
            self.apply_audio(effects, &mut outcome);
        }
        let context = self.context(local.is_some());
        let (across, along) = self.to_rail(x, y);
        if let Some(router) = self.router.as_mut()
            && let Ok(intents) = router.pointer_moved(across, along, t, context)
        {
            for intent in intents {
                if let BarPointerIntent::Popout(change) = intent {
                    self.designate(change);
                }
            }
        }
        // Off the rail it designates nothing (the popout's own body keeps
        // it, in the shell's attention).
        if across >= crate::rail_topology::rail_width() {
            self.designated = None;
        }
        outcome
    }

    pub fn pointer_left(&mut self) -> HostOutcome {
        let t = self.micros();
        let context = self.context(false);
        if let Some(router) = self.router.as_mut() {
            let _ = router.pointer_left(t, context);
        }
        self.designated = None;
        HostOutcome::default()
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) -> HostOutcome {
        self.popout_pointer(x, y, true)
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> HostOutcome {
        self.popout_pointer(x, y, false)
    }

    fn popout_pointer(&mut self, x: f32, y: f32, down: bool) -> HostOutcome {
        let mut outcome = HostOutcome::default();
        let t = self.micros();
        // A drag that leaves the panel keeps its capture until release.
        let local = self.local(x, y).or_else(|| {
            (!down && self.preview.is_some())
                .then(|| self.frame().map(|f| (x - f.rect.left(), y - f.rect.top())))
                .flatten()
        });
        let Some((lx, ly)) = local else {
            return outcome;
        };
        // A read-only popout takes the press so nothing beneath reacts.
        outcome.consumed = true;
        if self.dashboard {
            outcome.dashboard_press = Some((lx, ly, down));
            return outcome;
        }
        if !self.is_audio() {
            // A release on the "Open settings" row opens Settings there.
            if !down
                && let Some(info) = self.info()
                && info.rows.iter().any(|r| {
                    r.kind == crate::bar_status_popouts::InfoRowKind::OpenSettings
                        && lx >= r.x
                        && lx < r.x + r.width
                        && ly >= r.y
                        && ly < r.y + r.height
                })
            {
                outcome.open_settings = Some(self.shown_kind);
            }
            return outcome;
        }
        let Some(audio) = self.audio.as_mut() else {
            return outcome;
        };
        let effects = if down {
            audio.pointer_down(lx, ly, t)
        } else {
            audio.pointer_up(lx, ly, t)
        };
        if let Ok(effects) = effects {
            self.apply_audio(effects, &mut outcome);
        }
        outcome
    }

    /// `delta_y` uses the Wayland axis sign (positive = downward).
    pub fn wheel(&mut self, x: f32, y: f32, delta_y: f32) -> HostOutcome {
        let mut outcome = HostOutcome::default();
        let t = self.micros();
        if let Some((lx, ly)) = self.local(x, y) {
            outcome.consumed = true;
            if self.is_audio()
                && let Some(audio) = self.audio.as_mut()
                && let Ok(effects) = audio.wheel(lx, ly, delta_y, t)
            {
                self.apply_audio(effects, &mut outcome);
            }
            return outcome;
        }
        let (x, y) = self.to_rail(x, y);
        if x >= crate::rail_topology::rail_width() {
            return outcome;
        }
        let Some(router) = self.router.as_mut() else {
            return outcome;
        };
        match router.wheel(x, y, delta_y, t) {
            Ok(Some(BarPointerIntent::VolumeStep(direction))) => {
                if let Some(model) = &self.audio_model {
                    outcome.consumed = true;
                    outcome.intents.extend(model.step(direction));
                }
            }
            // RAIL-01: workspaces and brightness are the scene's to carry out.
            Ok(Some(
                intent @ (BarPointerIntent::WorkspaceStep(_) | BarPointerIntent::BrightnessStep(_)),
            )) => {
                outcome.consumed = true;
                outcome.bar_intent = Some(intent);
            }
            _ => {}
        }
        outcome
    }

    /// RAIL-01: one click on the rail (a press and release on the same
    /// module that did not lift it), as the router's intent.
    pub fn click(&mut self, x: f32, y: f32, secondary: bool) -> Option<BarPointerIntent> {
        let t = self.micros();
        let context = self.context(false);
        let (x, y) = self.to_rail(x, y);
        self.router
            .as_mut()?
            .click(x, y, secondary, t, context)
            .ok()
            .flatten()
    }

    /// BAR-01B: the rail starts to travel: whatever popout is open
    /// retracts into the rail with its closing motion, at once.
    pub fn retract(&mut self) -> HostOutcome {
        let mut outcome = self.escape();
        self.pointer_left();
        outcome.changed |= self.hide();
        outcome
    }

    pub fn escape(&mut self) -> HostOutcome {
        let mut outcome = HostOutcome::default();
        let t = self.micros();
        if let Some(audio) = self.audio.as_mut()
            && let Ok(effects) = audio.escape(t)
        {
            self.apply_audio(effects, &mut outcome);
        }
        outcome
    }

    /// Advances the extrusion; returns whether another frame is needed.
    pub fn tick(&mut self, now: Instant, reduced_motion: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |previous| {
                now.duration_since(previous).as_secs_f32()
            })
            .clamp(0.0, 0.1);
        self.last_tick = Some(now);
        self.motion.set_open(self.target_open);
        let mode = if reduced_motion {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        if self.motion.step(dt, mode).is_err() {
            self.motion.snap();
        }
        // FOCUS-01B: the body takes the shown popout's shape, from where it
        // stands; its contents pass along at the same tempo.
        if let Some(target) = self.target_rect() {
            let target = [target.left(), target.top(), target.right(), target.bottom()];
            match &mut self.morph {
                Some(morph) => {
                    morph.retarget(target);
                    morph.step(dt, mode);
                }
                None => self.morph = Some(MorphFrame::at(target)),
            }
        }
        let window = self
            .morph
            .map(|m| {
                let [l, t, r, b] = m.rect();
                if self.side.is_horizontal() {
                    r - l
                } else {
                    b - t
                }
            })
            .unwrap_or(0.0);
        if let Some(strip) = &mut self.strip {
            strip.step(dt, mode, window);
        }
        if !self.motion.is_visible() && !self.target_open {
            self.shown = None;
            self.dashboard = false;
            self.morph = None;
            self.strip = None;
        }
        // Still: the clock stops with it, so the next movement starts with
        // one frame's step, not the time it stood still (FOCUS-01B: a popout
        // still for two seconds went in within one 100 ms step, and the
        // surface waiting for it came out over it).
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    /// Centre of the audio status icon along the rail, in scene space (y on
    /// a vertical rail, x on a horizontal one), if routed.
    pub fn audio_anchor(&self) -> Option<f32> {
        self.status_anchor(StatusEntryKind::Audio)
    }

    /// Network, Bluetooth and battery anchors when any changed since the last
    /// report (for receipts).
    pub fn take_status_anchor_report(&mut self) -> Option<[Option<f32>; 3]> {
        let anchors = [
            self.status_anchor(StatusEntryKind::Network),
            self.status_anchor(StatusEntryKind::Bluetooth),
            self.status_anchor(StatusEntryKind::BatteryOrProfile),
        ];
        (self.reported_status_anchors != Some(anchors)).then(|| {
            self.reported_status_anchors = Some(anchors);
            anchors
        })
    }

    /// FOCUS-01C: the centre of tray item `index` along the rail, in scene
    /// space, if routed.
    pub fn tray_anchor(&self, index: usize) -> Option<f32> {
        let (a, b) = self
            .router
            .as_ref()?
            .vertical_extent(crate::bar_pointer_routing::BarTarget::TrayItem(index))?;
        Some((a + b) / 2.0)
    }

    /// The output's size, as last synced.
    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    /// Centre of a status entry along the rail, in scene space, if routed.
    pub fn status_anchor(&self, kind: StatusEntryKind) -> Option<f32> {
        let (top, bottom) = self
            .router
            .as_ref()?
            .vertical_extent(crate::bar_pointer_routing::BarTarget::StatusEntry(kind))?;
        Some((top + bottom) / 2.0)
    }

    /// The audio anchor when it changed since the last report (for receipts).
    pub fn take_anchor_report(&mut self) -> Option<Option<f32>> {
        let anchor = self.audio_anchor();
        (self.reported_anchor != Some(anchor)).then(|| {
            self.reported_anchor = Some(anchor);
            anchor
        })
    }

    /// Scene-space slider rectangle `(x, y, w, h)` while the popout is shown.
    pub fn slider_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let frame = self.frame()?;
        let row = self.audio_rows().into_iter().find(|row| {
            matches!(
                row.kind,
                crate::bar_audio_popout::AudioRowKind::Slider { .. }
            )
        })?;
        Some((
            frame.rect.left() + row.x,
            frame.rect.top() + row.y,
            row.width,
            row.height,
        ))
    }

    /// FOCUS-01B: no handoff under way (the body and its contents still).
    pub fn handoff_settled(&self) -> bool {
        self.morph.is_none_or(|m| m.is_settled())
            && self.strip.as_ref().is_none_or(|s| s.is_settled())
    }

    pub fn needs_frame(&self) -> bool {
        self.motion.is_open() != self.target_open
            || self.motion.needs_frame()
            || self.morph.is_some_and(|m| !m.is_settled())
            || self.strip.as_ref().is_some_and(|s| !s.is_settled())
    }

    pub fn is_open(&self) -> bool {
        self.target_open
    }

    pub fn preview(&self) -> Option<f32> {
        self.preview
    }

    pub fn audio_rows(&self) -> Vec<AudioRow> {
        if !self.is_audio() {
            return Vec::new();
        }
        self.audio
            .as_ref()
            .map(AudioPopoutSurface::rows)
            .unwrap_or_default()
    }

    /// The body the shown popout's content asks for, at its anchor.
    fn target_rect(&self) -> Option<GeometryRect> {
        let anchor = self.shown?;
        let (popout_width, h) = self.size_of(self.shown_popout()?)?;
        let (width, height) = self.viewport;
        let rect = if self.side.is_horizontal() {
            // TOPO-01C: centred on the anchor along a horizontal rail, and
            // hanging below a top rail or standing on a bottom one.
            let area = width - 2.0 * BORDER_THICKNESS;
            let left = BORDER_THICKNESS + popout_top(anchor, popout_width, area, BORDER_THICKNESS);
            let left = left.clamp(
                BORDER_THICKNESS,
                (width - BORDER_THICKNESS - popout_width).max(BORDER_THICKNESS),
            );
            let rail = crate::rail_topology::rail_width();
            let top = if self.side == RailSide::Top {
                rail
            } else {
                height - rail - h
            };
            GeometryRect::try_new(left, top, left + popout_width, top + h, 0.0).ok()?
        } else {
            let area = height - 2.0 * BORDER_THICKNESS;
            let top = BORDER_THICKNESS + popout_top(anchor, h, area, BORDER_THICKNESS);
            let top = top.clamp(
                BORDER_THICKNESS,
                (height - BORDER_THICKNESS - h).max(BORDER_THICKNESS),
            );
            let x = self
                .side
                .mirror_span(crate::rail_topology::rail_width(), popout_width, width);
            GeometryRect::try_new(x, top, x + popout_width, top + h, 0.0).ok()?
        };
        Some(rect)
    }

    /// FOCUS-01D: the body it has fully out, and the edge it comes out of.
    pub fn body(&self) -> Option<(GeometryRect, gnos_ux_surface::envelope::SurfaceEdge)> {
        Some((self.target_rect()?, self.side.edge()))
    }

    /// FOCUS-01D: how deep it may come out (`None`: fully). Returns whether
    /// that changed.
    pub fn set_give_way(&mut self, limit: Option<f32>) -> bool {
        let changed = self.give_way != limit;
        self.give_way = limit;
        changed
    }

    pub fn frame(&self) -> Option<PopoutFrame> {
        if self.motion.phase() <= 0.0 {
            return None;
        }
        // FOCUS-01B: where the body stands while it changes shape.
        let rect = match self.morph {
            Some(m) => {
                let [l, t, r, b] = m.rect();
                GeometryRect::try_new(l, t, r, b, 0.0).ok()?
            }
            None => self.target_rect()?,
        };
        // FOCUS-01D: it stands back into the Bar by what a surface standing
        // with it occupies of its depth (make room).
        let depth = if self.side.is_horizontal() {
            rect.height()
        } else {
            rect.width()
        };
        let openness =
            gnos_ux_surface::grammar::within_depth(self.motion.phase(), depth, self.give_way);
        if openness <= 0.0 {
            return None;
        }
        let h = rect.height();
        let rounded = GeometryRect::try_new(
            rect.left(),
            rect.top(),
            rect.right(),
            rect.bottom(),
            BORDER_ROUNDING.min(h / 2.0),
        )
        .ok()?;
        let visible = revealed(rounded, self.side.edge(), openness).or_else(|| {
            // A hairline on the rail's inner edge.
            let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
            let (l, t, r, b) = match self.side {
                RailSide::Left => (l, t, l + 0.5, b),
                RailSide::Right => (r - 0.5, t, r, b),
                RailSide::Top => (l, t, r, t + 0.5),
                RailSide::Bottom => (l, b - 0.5, r, b),
            };
            GeometryRect::try_new(l, t, r, b, 0.0).ok()
        })?;
        Some(PopoutFrame {
            rect,
            visible,
            openness,
            hull: popout_hull(visible, self.viewport, self.side),
            side: self.side,
        })
    }
}

/// Rail and revealed popout as one outline (flush, fused joint).
pub(crate) fn popout_hull(
    child: GeometryRect,
    (width, height): (f32, f32),
    side: RailSide,
) -> Option<ConnectiveHullGeometry> {
    let rail_radius = if side.is_horizontal() {
        BORDER_ROUNDING.min(child.left()).min(width - child.right())
    } else {
        BORDER_ROUNDING
            .min(child.top())
            .min(height - child.bottom())
    };
    let (x, y, w, h) = side.rail_rect((width, height));
    let parent = GeometryRect::try_new(x, y, x + w, y + h, rail_radius.max(0.0)).ok()?;
    fused_hull(parent, child, side.edge())
}

#[cfg(all(test, feature = "component-lab"))]
mod tests {
    use super::*;

    /// FOCUS-01: the host driven by the shell's conductor, as in the scene.
    fn attention() -> crate::attention::Attention {
        let mut att = crate::attention::Attention::new();
        crate::attention::declare(&mut att, &crate::surface_prefs::SurfacePrefs::default());
        att
    }

    fn attend(
        host: &mut BarPopoutHost,
        att: &mut crate::attention::Attention,
        at: Option<(f32, f32)>,
    ) -> bool {
        crate::attention::place_popouts(att, host, |_| None);
        let over = crate::attention::over(false, host.designated());
        let mut changed = false;
        for t in att.pointer(at, over, Instant::now()) {
            changed |= crate::attention::apply_to_host(host, &t, None);
        }
        changed
    }

    fn drive(
        host: &mut BarPopoutHost,
        att: &mut crate::attention::Attention,
        x: f32,
        y: f32,
    ) -> HostOutcome {
        let mut outcome = host.pointer_moved(x, y);
        outcome.changed |= attend(host, att, Some((x, y)));
        outcome
    }

    fn leave(host: &mut BarPopoutHost, att: &mut crate::attention::Attention) -> HostOutcome {
        let mut outcome = host.pointer_left();
        outcome.changed |= attend(host, att, None);
        outcome
    }
    use crate::bar_pointer_routing::BAR_WIDTH;
    use crate::bar_pointer_routing::BarTarget;
    use crate::product_status_glyph::{StatusGlyph, VolumeGlyph};
    use crate::reference_bar::StatusVisual;
    use gnos_audio::{
        AudioDefaultRoles, AudioEndpointId, AudioEndpointKind, AudioEndpointSnapshot,
        AudioProviderState, AudioVolume, NativeVolume, RoleSelection,
    };
    use std::collections::BTreeMap;
    use std::time::Duration;

    const H: f32 = 1440.0;

    fn presentation() -> BarPresentation {
        let mut bar = BarPresentation::captured_oracle_2026_10_07();
        let status = bar.status.as_mut().expect("oracle status");
        status.entries.insert(
            1,
            StatusVisual::Audio {
                glyph: StatusGlyph::Volume(VolumeGlyph::High),
            },
        );
        bar
    }

    fn audio(muted: bool) -> AudioSnapshot {
        let id = AudioEndpointId::new(7, 1);
        let mut endpoints = BTreeMap::new();
        endpoints.insert(
            id,
            AudioEndpointSnapshot::new(
                id,
                AudioEndpointKind::Output,
                "node.1".into(),
                "Speakers".into(),
                AudioVolume::from_parts(
                    None,
                    vec![NativeVolume::new(0.125).unwrap(); 2],
                    Vec::new(),
                ),
                muted,
            ),
        );
        let mut defaults = AudioDefaultRoles::default();
        defaults.output.effective = RoleSelection {
            name: Some("node.1".into()),
            resolved: Some(id),
            metadata_error: None,
        };
        AudioSnapshot {
            provider: AudioProviderState::Available,
            generation: 1,
            endpoints,
            defaults,
        }
    }

    fn host() -> (BarPopoutHost, f32) {
        let mut host = BarPopoutHost::new();
        host.sync(&presentation(), (2560.0, H), 180.0, &audio(false));
        let router =
            BarPointerRouter::new(&presentation(), BarPointerConfig::default(), H, H, 180.0)
                .unwrap();
        let (top, bottom) = router
            .vertical_extent(BarTarget::StatusEntry(StatusEntryKind::Audio))
            .expect("audio status entry routed");
        (host, (top + bottom) / 2.0)
    }

    fn settle(host: &mut BarPopoutHost) {
        let start = Instant::now();
        for i in 1..=120 {
            if !host.tick(start + Duration::from_millis(16 * i), false) {
                return;
            }
        }
        panic!("slide did not settle");
    }

    #[test]
    fn hovering_the_audio_icon_opens_slides_out_and_joins_the_rail() {
        let (mut host, anchor) = host();
        let mut att = attention();
        assert!(host.frame().is_none());
        let moved = drive(&mut host, &mut att, 30.0, anchor);
        assert!(moved.changed && host.is_open());
        let start = Instant::now();
        host.tick(start, false);
        host.tick(start + Duration::from_millis(16), false);
        let partial = host.frame().expect("revealing");
        assert!(partial.openness > 0.0 && partial.openness < 1.0);
        assert!(partial.visible.width() < AUDIO_POPOUT_WIDTH);
        settle(&mut host);
        let open = host.frame().unwrap();
        assert_eq!(open.openness, 1.0);
        assert_eq!(open.visible.left(), BAR_WIDTH);
        assert_eq!(open.visible.width(), AUDIO_POPOUT_WIDTH);
        assert!(
            ((open.rect.top() + open.rect.bottom()) / 2.0 - anchor).abs() < 1.0
                || open.rect.bottom() >= H - BORDER_THICKNESS - 0.5
        );
        let hull = open.hull.expect("flush fused joint resolves");
        assert_eq!(hull.bounds().left(), 0.0);
        assert_eq!(hull.bounds().right(), BAR_WIDTH + AUDIO_POPOUT_WIDTH);
    }

    fn status_host() -> (BarPopoutHost, f32, f32, f32) {
        let (mut host, _) = host();
        host.sync_status(
            &gnos_network::NetworkSnapshot {
                provider: gnos_network::NetworkProviderState::Unavailable,
                generation: 0,
                provider_owner: None,
                globals: None,
                devices: BTreeMap::new(),
                active_connections: BTreeMap::new(),
                settings_connections: BTreeMap::new(),
            },
            &gnos_bluetooth::BluetoothSnapshot {
                provider: gnos_bluetooth::BluetoothProviderState::Unavailable,
                generation: 0,
                provider_owner: None,
                adapters: BTreeMap::new(),
                devices: BTreeMap::new(),
            },
            gnos_power::PowerSnapshot::Profile(gnos_power::PowerProfile::Balanced),
        );
        let router =
            BarPointerRouter::new(&presentation(), BarPointerConfig::default(), H, H, 180.0)
                .unwrap();
        let centre = |kind| {
            let (top, bottom) = router
                .vertical_extent(BarTarget::StatusEntry(kind))
                .expect("status entry routed");
            (top + bottom) / 2.0
        };
        (
            host,
            centre(StatusEntryKind::Network),
            centre(StatusEntryKind::Bluetooth),
            centre(StatusEntryKind::BatteryOrProfile),
        )
    }

    #[test]
    fn status_icons_open_their_read_only_popouts_and_retarget_in_place() {
        let (mut host, network, bluetooth, battery) = status_host();
        let mut att = attention();
        drive(&mut host, &mut att, 30.0, network);
        settle(&mut host);
        assert_eq!(host.shown_kind(), Some(StatusEntryKind::Network));
        let frame = host.frame().expect("network popout");
        assert_eq!(
            frame.rect.width(),
            crate::bar_status_popouts::NETWORK_POPOUT_WIDTH
        );
        assert!(host.audio_rows().is_empty());
        assert_eq!(host.info().expect("rows").rows[0].label, "Network");
        drive(&mut host, &mut att, 30.0, bluetooth);
        assert_eq!(host.shown_kind(), Some(StatusEntryKind::Bluetooth));
        // The body takes the new shape by morphing (FOCUS-01B).
        settle(&mut host);
        assert_eq!(
            host.frame().expect("bluetooth popout").rect.width(),
            crate::bar_status_popouts::BLUETOOTH_POPOUT_WIDTH
        );
        drive(&mut host, &mut att, 30.0, battery);
        let info = host.info().expect("battery rows");
        assert_eq!(info.rows[1].label, "Power profile: Balanced");
        // Inside the panel the popout stays and takes presses without effects.
        let frame = host.frame().unwrap();
        let (x, y) = (frame.rect.left() + 40.0, frame.rect.top() + 20.0);
        assert!(drive(&mut host, &mut att, x, y).consumed);
        assert!(host.is_open());
        let down = host.pointer_down(x, y);
        let up = host.pointer_up(x, y);
        assert!(down.consumed && up.consumed);
        assert!(down.intents.is_empty() && up.intents.is_empty());
        assert!(host.wheel(x, y, 1.0).intents.is_empty());
        leave(&mut host, &mut att);
        assert!(!host.is_open());
    }

    #[test]
    fn a_neighbour_s_popout_takes_over_by_morphing_not_by_going_in() {
        let (mut host, network, bluetooth, _) = status_host();
        let mut att = attention();
        drive(&mut host, &mut att, 30.0, network);
        settle(&mut host);
        let before = host.frame().expect("network popout");
        assert_eq!(
            host.contents(),
            vec![(BarPopout::Status(StatusEntryKind::Network), 0.0)]
        );
        // Next icon: the surface stays out and changes shape.
        drive(&mut host, &mut att, 30.0, bluetooth);
        let mut t = Instant::now();
        let mut both = false;
        let mut previous_top = before.rect.top();
        let target_top = loop {
            t += std::time::Duration::from_millis(4);
            let moving = host.tick(t, false);
            let frame = host.frame().expect("never goes in");
            assert_eq!(frame.openness, 1.0, "no retraction on a handoff");
            let parts = host.contents();
            if parts.len() == 2 {
                both = true;
                // One rigid strip: the contents keep their spacing.
                let gap = (parts[1].1 - parts[0].1).abs();
                assert!(gap > 0.0);
            }
            // The body moves one way, toward the next icon.
            let top = frame.rect.top();
            assert!((top - previous_top) * (bluetooth - network) >= -1e-3);
            previous_top = top;
            if !moving {
                break top;
            }
        };
        assert!(both, "the two contents passed along together");
        assert_eq!(
            host.contents(),
            vec![(BarPopout::Status(StatusEntryKind::Bluetooth), 0.0)]
        );
        assert_ne!(target_top, before.rect.top());
    }

    #[test]
    fn after_standing_still_it_goes_in_from_where_it_stands_not_in_one_jump() {
        let (mut host, anchor) = host();
        let mut att = attention();
        drive(&mut host, &mut att, 30.0, anchor);
        let start = Instant::now();
        let mut t = start;
        while host.tick(t, false) {
            t += Duration::from_millis(16);
        }
        assert_eq!(host.frame().unwrap().openness, 1.0);
        // Two seconds still, then back in: the first step is one frame's.
        assert!(host.hide());
        t += Duration::from_secs(2);
        host.tick(t, false);
        t += Duration::from_millis(4);
        host.tick(t, false);
        let openness = host.frame().expect("still mostly out").openness;
        assert!(openness > 0.9, "jumped to {openness}");
    }

    #[test]
    fn a_horizontal_rail_routes_along_x_and_hangs_the_popout_off_its_edge() {
        const W: f32 = 2560.0;
        for side in [RailSide::Top, RailSide::Bottom] {
            let mut host = BarPopoutHost::new();
            let mut att = attention();
            host.set_side(side);
            host.sync(&presentation(), (W, H), 300.0, &audio(false));
            let router = BarPointerRouter::new_on(
                &presentation(),
                BarPointerConfig::default(),
                W,
                W,
                300.0,
                true,
            )
            .unwrap();
            let (start, end) = router
                .vertical_extent(BarTarget::StatusEntry(StatusEntryKind::Audio))
                .expect("audio entry routed along the rail");
            let anchor = (start + end) / 2.0;
            assert!(anchor > W / 2.0, "status icons sit at the end of the row");
            assert_eq!(host.audio_anchor(), Some(anchor));
            // Across the rail: 30 px in from the rail's outer edge.
            let y = if side == RailSide::Top {
                30.0
            } else {
                H - 30.0
            };
            assert!(drive(&mut host, &mut att, anchor, y).changed);
            settle(&mut host);
            let frame = host.frame().expect("audio popout");
            assert_eq!(frame.side, side);
            // Centred on the anchor, bounded by the frame's inner edge.
            assert!(frame.rect.left() <= anchor && anchor <= frame.rect.right());
            assert!(frame.rect.right() <= W - BORDER_THICKNESS);
            match side {
                RailSide::Top => assert_eq!(frame.rect.top(), BAR_WIDTH),
                _ => assert_eq!(frame.rect.bottom(), H - BAR_WIDTH),
            }
            assert!(frame.hull.is_some(), "joined to the rail");
            // The same x below the rail's inner edge is not the rail.
            let off = if side == RailSide::Top {
                200.0
            } else {
                H - 200.0
            };
            drive(&mut host, &mut att, 10.0, off);
            assert!(!host.is_open());
        }
    }

    #[test]
    fn moving_into_the_panel_keeps_it_and_leaving_closes_it() {
        let (mut host, anchor) = host();
        let mut att = attention();
        drive(&mut host, &mut att, 30.0, anchor);
        settle(&mut host);
        let frame = host.frame().unwrap();
        let inside = (frame.rect.left() + 100.0, frame.rect.top() + 20.0);
        let kept = drive(&mut host, &mut att, inside.0, inside.1);
        assert!(kept.consumed && host.is_open());
        let left = drive(&mut host, &mut att, frame.rect.right() + 200.0, inside.1);
        assert!(left.changed && !host.is_open());
        settle(&mut host);
        assert!(host.frame().is_none());
    }

    #[test]
    fn dragging_the_slider_in_the_scene_emits_one_volume_intent() {
        let (mut host, anchor) = host();
        let mut att = attention();
        drive(&mut host, &mut att, 30.0, anchor);
        settle(&mut host);
        let frame = host.frame().unwrap();
        let slider = host
            .audio_rows()
            .into_iter()
            .find(|r| matches!(r.kind, crate::bar_audio_popout::AudioRowKind::Slider { .. }))
            .unwrap();
        let y = frame.rect.top() + slider.y + slider.height / 2.0;
        let x0 = frame.rect.left() + slider.x + slider.width * 0.2;
        let x1 = frame.rect.left() + slider.x + slider.width * 0.8;
        assert!(host.pointer_down(x0, y).consumed);
        assert!(host.preview().is_some());
        drive(&mut host, &mut att, x1, y);
        let up = host.pointer_up(x1, y);
        assert_eq!(host.preview(), None);
        assert!(matches!(
            up.intents.as_slice(),
            [AudioIntent::SetOutputVolume { level, .. }] if (level.get() - 0.8).abs() < 1e-4
        ));
    }

    #[test]
    fn wheel_on_the_rail_steps_volume_workspaces_and_brightness() {
        let (mut host, anchor) = host();
        let router =
            BarPointerRouter::new(&presentation(), BarPointerConfig::default(), H, H, 180.0)
                .unwrap();
        let (ws_top, ws_bottom) = router.vertical_extent(BarTarget::Workspaces).unwrap();
        // Upper half, off the workspaces: reference volume zone.
        let upper = ws_bottom + 6.0;
        assert!(upper < H / 2.0);
        let up = host.wheel(30.0, upper, -15.0);
        assert!(up.consumed);
        assert!(matches!(
            up.intents.as_slice(),
            [AudioIntent::AdjustOutputVolume { delta, .. }] if delta.get() > 0.0
        ));
        // RAIL-01: the lower half is brightness, the workspaces step; both
        // are the scene's to carry out.
        assert!(anchor > H / 2.0);
        let lower = host.wheel(30.0, anchor, -15.0);
        assert!(lower.consumed && lower.intents.is_empty());
        assert_eq!(
            lower.bar_intent,
            Some(BarPointerIntent::BrightnessStep(
                crate::bar_pointer_routing::StepDirection::Increase
            ))
        );
        let ws = host.wheel(30.0, (ws_top + ws_bottom) / 2.0, -15.0);
        assert!(ws.consumed && ws.intents.is_empty());
        assert_eq!(ws.bar_intent, Some(BarPointerIntent::WorkspaceStep(-1)));
        assert!(!host.wheel(400.0, anchor, -15.0).consumed);
    }

    #[test]
    fn a_click_on_the_rail_resolves_to_the_module_s_intent() {
        let (mut host, _) = host();
        let router =
            BarPointerRouter::new(&presentation(), BarPointerConfig::default(), H, H, 180.0)
                .unwrap();
        let (ws_top, ws_bottom) = router.vertical_extent(BarTarget::Workspaces).unwrap();
        let intent = host.click(30.0, (ws_top + ws_bottom) / 2.0, false);
        assert!(matches!(
            intent,
            Some(BarPointerIntent::FocusWorkspace(_) | BarPointerIntent::ToggleSpecialWorkspace)
        ));
        // Off the rail, nothing.
        assert_eq!(host.click(400.0, (ws_top + ws_bottom) / 2.0, false), None);
    }

    #[test]
    fn reduced_motion_snaps_without_intermediate_frames() {
        let (mut host, anchor) = host();
        let mut att = attention();
        drive(&mut host, &mut att, 30.0, anchor);
        assert!(!host.tick(Instant::now(), true));
        assert_eq!(host.frame().unwrap().openness, 1.0);
    }

    #[test]
    fn audio_observation_change_does_not_cancel_a_drag() {
        let (mut host, anchor) = host();
        let mut att = attention();
        drive(&mut host, &mut att, 30.0, anchor);
        settle(&mut host);
        let frame = host.frame().unwrap();
        let slider = host
            .audio_rows()
            .into_iter()
            .find(|r| matches!(r.kind, crate::bar_audio_popout::AudioRowKind::Slider { .. }))
            .unwrap();
        let y = frame.rect.top() + slider.y + slider.height / 2.0;
        let x = frame.rect.left() + slider.x + 10.0;
        host.pointer_down(x, y);
        let mut next = audio(true);
        next.generation = 2;
        host.sync(&presentation(), (2560.0, H), 180.0, &next);
        assert!(
            host.pointer_up(x + 40.0, y)
                .intents
                .iter()
                .any(|i| matches!(i, AudioIntent::SetOutputVolume { .. }))
        );
    }
}
