//! FOCUS-01: the shell's attention in the scene.
//!
//! The conductor (`crate::attention`) decides when each surface out of the
//! shell comes out and goes back in; here it is fed (the pointer, its
//! leaving the shell, clicks, presses, commands, Escape, choices, and each
//! surface's visibility from its motion) and its changes are carried out.
//!
//! - The workspaces panel and the Bar popouts are carried out at once.
//! - The panels opened by a click or a command (Session, Utilities, the
//!   overflow, Settings) need the window to open (keyboard focus, reading
//!   logind…): their changes wait for the next frame, `attention_apply`.
//! - A panel that closes by itself (its ✕, a press on it, its own Escape) is
//!   reconciled: the conductor hears it went in.
//! - Every step of the motion reports which surfaces are still on screen:
//!   that report, not a timer, lets the next surface out (FOCUS-01B).
//!
//! Receipt: `GNOS_UX_ATTENTION`.

use super::*;
use crate::attention::{Area, Attendant};
use crate::reference_bar::IdleBarEntry;
use gnos_ux_surface::grammar::attention::Transition;

/// What waits for the window.
#[derive(Default)]
pub(super) struct AttentionState {
    effects: Vec<(Attendant, bool)>,
    /// The page Settings opens on (a popout's "Open settings").
    settings_page: Option<crate::settings_pages::PageId>,
    /// Where a click asked a tray menu out (else it comes out at its item).
    tray_at: Option<(usize, (f32, f32))>,
    /// FOCUS-01B: the aim verdict last reported for each surface out.
    aim_reported: Vec<(Attendant, Option<bool>)>,
    /// FOCUS-01D (layer): the pointer-opened surfaces standing above a
    /// deliberate one, with their own outline.
    layered: Vec<Attendant>,
    /// FOCUS-01D (make room): the depth each deliberate surface gives way to,
    /// as last reported.
    give_way_reported: Vec<(Attendant, Option<f32>)>,
}

fn area_of(f: &crate::bar_popout_host::PopoutFrame) -> Area {
    let v = f.visible;
    Area::new(v.left(), v.top(), v.right(), v.bottom())
}

impl SceneHost {
    /// A module's footprint on the Bar at rest, as an attendant's source.
    fn module_area(&self, entry: IdleBarEntry) -> Option<Area> {
        let (side, placements) = self.bar_rest_previous.as_ref()?;
        if *side != self.rail_side() {
            return None;
        }
        placements.iter().find(|m| m.entry == entry).map(|m| {
            let r = m.clip;
            Area::new(r.x, r.y, r.x + r.width, r.y + r.height)
        })
    }

    fn panel_open(&self, key: Attendant) -> bool {
        match key {
            Attendant::Session => self.session.is_open(),
            Attendant::Utilities => self.utilities.is_open(),
            Attendant::Overflow => self.overflow.is_open(),
            Attendant::Settings => self.settings.is_open(),
            _ => false,
        }
    }

    /// Whether any part of the attendant's surface is on screen.
    fn on_screen(&self, key: Attendant) -> bool {
        match key {
            Attendant::Workspaces => self.workspaces.frame().is_some(),
            Attendant::Dashboard | Attendant::Status(_) => {
                self.bar_popout.frame().is_some()
                    && self
                        .bar_popout
                        .shown_popout()
                        .and_then(Attendant::of_popout)
                        == Some(key)
            }
            Attendant::Session => self.session.frame().is_some(),
            Attendant::Utilities => self.utilities.frame().is_some(),
            Attendant::Overflow => self.overflow.frame().is_some(),
            Attendant::Settings => self.settings.frame().is_some(),
            Attendant::Tray(index) => {
                self.tray_menu.frame().is_some() && self.tray_menu.key() == Some(index)
            }
        }
    }

    /// FOCUS-01D: the body an attendant's surface has fully out, and the edge
    /// it comes out of.
    fn surface_body(
        &self,
        key: Attendant,
    ) -> Option<(
        gnos_ux_surface::grammar::GeometryRect,
        gnos_ux_surface::envelope::SurfaceEdge,
    )> {
        match key {
            Attendant::Workspaces => self.workspaces.body(),
            Attendant::Dashboard | Attendant::Status(_) => self.bar_popout.body(),
            Attendant::Session => self.session.body(),
            Attendant::Utilities => self.utilities.body(),
            Attendant::Overflow => self.overflow.body(),
            Attendant::Settings => self.settings.body(),
            Attendant::Tray(_) => self.tray_menu.body(),
        }
    }

    /// FOCUS-01D: the part of an attendant's surface on screen now.
    fn surface_visible(&self, key: Attendant) -> Option<gnos_ux_surface::grammar::GeometryRect> {
        match key {
            Attendant::Workspaces => self.workspaces.frame().map(|f| f.visible),
            Attendant::Dashboard | Attendant::Status(_) => self
                .bar_popout
                .frame()
                .filter(|_| {
                    self.bar_popout
                        .shown_popout()
                        .and_then(Attendant::of_popout)
                        == Some(key)
                })
                .map(|f| f.visible),
            Attendant::Session => self.session.frame().map(|f| f.visible),
            Attendant::Utilities => self.utilities.frame().map(|f| f.visible),
            Attendant::Overflow => self.overflow.frame().map(|f| f.visible),
            Attendant::Settings => self.settings.frame().and_then(|f| {
                let p = f.panel;
                gnos_ux_surface::grammar::GeometryRect::try_new(
                    p.x,
                    p.y.max(f.clip_top),
                    p.x + p.width,
                    (p.y + p.height).min(f.clip_bottom),
                    0.0,
                )
                .ok()
            }),
            Attendant::Tray(index) => self
                .tray_menu
                .frame()
                .filter(|_| self.tray_menu.key() == Some(index))
                .map(|f| f.visible),
        }
    }

    /// FOCUS-01D: cap how deep an attendant's surface may come out.
    fn set_surface_give_way(&mut self, key: Attendant, limit: Option<f32>) -> bool {
        match key {
            Attendant::Workspaces => self.workspaces.set_give_way(limit),
            Attendant::Dashboard | Attendant::Status(_) => self.bar_popout.set_give_way(limit),
            Attendant::Session => self.session.set_give_way(limit),
            Attendant::Utilities => self.utilities.set_give_way(limit),
            Attendant::Overflow => self.overflow.set_give_way(limit),
            Attendant::Settings => self.settings.set_give_way(limit),
            Attendant::Tray(_) => self.tray_menu.set_give_way(limit),
        }
    }

    /// FOCUS-01D, on each motion step: with "make room", each deliberate
    /// surface comes out no deeper than what leaves the pointer-opened ones
    /// standing with it clear (a constraint of geometry, followed frame by
    /// frame: they never overlap, and it comes back as they go); with
    /// "layer", those stand above it. Returns whether anything changed.
    fn arrange_meetings(&mut self) -> bool {
        use gnos_ux_surface::grammar::attention::Meeting;
        let meeting = self.attention.meeting();
        let meetings = self.attention.meetings();
        // One surface per kin: the popouts share a host, the tray a menu.
        let surface = |k: Attendant| match k {
            Attendant::Dashboard | Attendant::Status(_) => Attendant::Dashboard,
            Attendant::Tray(_) => Attendant::Tray(0),
            k => k,
        };
        let mut limits: Vec<(Attendant, Option<f32>)> = [
            Attendant::Workspaces,
            Attendant::Dashboard,
            Attendant::Session,
            Attendant::Utilities,
            Attendant::Overflow,
            Attendant::Settings,
            Attendant::Tray(0),
        ]
        .into_iter()
        .map(|k| (k, None))
        .collect();
        if meeting == Meeting::MakeRoom {
            for (d, h) in &meetings {
                let (Some((body, edge)), Some(other)) =
                    (self.surface_body(*d), self.surface_visible(*h))
                else {
                    continue;
                };
                if let Some(depth) = gnos_ux_surface::grammar::clear_depth(body, edge, other)
                    && let Some(slot) = limits.iter_mut().find(|(k, _)| *k == surface(*d))
                {
                    slot.1 = Some(slot.1.map_or(depth, |l: f32| l.min(depth)));
                }
            }
        }
        let mut changed = false;
        for (key, limit) in &limits {
            if self.set_surface_give_way(*key, *limit) {
                changed = true;
                let before = self
                    .attention_state
                    .give_way_reported
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, l)| *l);
                // A receipt when it starts or stops giving way, not each px.
                if before.map(|b| b.is_some()) != Some(limit.is_some()) {
                    println!(
                        "GNOS_UX_SURFACES_MEET mode=make_room surface={} gives_way={}",
                        key.slug(),
                        limit.map_or("none".to_owned(), |l| format!("{l:.0}"))
                    );
                }
            }
        }
        self.attention_state.give_way_reported = limits;
        let layered: Vec<Attendant> = if meeting == Meeting::Layer {
            meetings.iter().map(|(_, h)| *h).collect()
        } else {
            Vec::new()
        };
        if layered != self.attention_state.layered {
            println!(
                "GNOS_UX_SURFACES_MEET mode=layer above=[{}]",
                layered
                    .iter()
                    .map(|k| k.slug())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            self.attention_state.layered = layered;
            changed = true;
        }
        changed
    }

    /// FOCUS-01D (layer): whether this surface stands above a deliberate one,
    /// with its own outline (out of the shell's field).
    pub(super) fn layered(&self, key: Attendant) -> bool {
        self.attention_state.layered.iter().any(|k| match (k, key) {
            (
                Attendant::Dashboard | Attendant::Status(_),
                Attendant::Dashboard | Attendant::Status(_),
            ) => true,
            (Attendant::Tray(_), Attendant::Tray(_)) => true,
            (a, b) => *a == b,
        })
    }

    /// FOCUS-01C: a click asked tray item `index`'s menu out at `at`.
    pub(super) fn attention_state_tray_at(&mut self, index: usize, at: (f32, f32)) {
        self.attention_state.tray_at = Some((index, at));
    }

    /// Where tray item `index`'s menu comes out: where a click asked, else at
    /// its item on the rail.
    fn tray_point(&mut self, index: usize) -> Option<(f32, f32)> {
        if let Some((i, at)) = self.attention_state.tray_at.take()
            && i == index
        {
            return Some(at);
        }
        let along = self.bar_popout.tray_anchor(index)?;
        let (w, h) = self.bar_popout.viewport();
        let across = crate::rail_topology::rail_width() / 2.0;
        Some(match self.rail_side() {
            crate::rail_topology::RailSide::Top => (along, across),
            crate::rail_topology::RailSide::Bottom => (along, h - across),
            crate::rail_topology::RailSide::Left => (across, along),
            crate::rail_topology::RailSide::Right => (w - across, along),
        })
    }

    /// The attendants that exist now (the tray's items come and go).
    fn attendants(&self) -> Vec<Attendant> {
        Attendant::POPOUTS
            .into_iter()
            .chain(Attendant::PANELS)
            .chain([Attendant::Workspaces])
            .chain((0..self.rail_tray_count()).map(Attendant::Tray))
            .collect()
    }

    /// Triggers from the settings, where every attendant stands, and the
    /// panels that went in by themselves.
    pub(super) fn sync_attention(&mut self) -> Vec<Transition<Attendant>> {
        let prefs = crate::surface_prefs::current();
        let mut changes = crate::attention::declare(&mut self.attention, &prefs);
        self.attention.set_aim(crate::attention::aim(&prefs));
        self.attention
            .set_meeting(crate::attention::meeting(&prefs));
        self.attention
            .set_warm_handoff(prefs.behaviour.warm_handoff);
        let count = self.rail_tray_count();
        changes.extend(crate::attention::declare_tray(
            &mut self.attention,
            count,
            &prefs,
        ));
        let tray = self.module_area(IdleBarEntry::Tray);
        let tray_body = self.tray_menu.frame().as_ref().map(area_of);
        for index in 0..count {
            let body = tray_body.filter(|_| self.tray_menu.key() == Some(index));
            self.attention.place(Attendant::Tray(index), tray, body);
        }
        // A tray menu that went in by itself (a row chosen, a press outside,
        // its application): on that edge only, never while one is arming.
        if let Some(index) = self.tray_menu.take_went_in() {
            let key = Attendant::Tray(index);
            let pending = self.attention_state.effects.iter().any(|(k, _)| *k == key);
            if self.attention.is_out(key) && !pending {
                changes.extend(self.attention.close(key));
            }
        }
        let workspaces = self.module_area(IdleBarEntry::Workspaces);
        let body = self.workspaces.frame().as_ref().map(area_of);
        self.attention
            .place(Attendant::Workspaces, workspaces, body);
        let clock = self.module_area(IdleBarEntry::Clock);
        let status = self.module_area(IdleBarEntry::StatusIcons);
        crate::attention::place_popouts(&mut self.attention, &self.bar_popout, |a| match a {
            Attendant::Dashboard => clock,
            Attendant::Status(_) => status,
            _ => None,
        });
        let settings = self.settings.frame().map(|f| {
            let p = f.panel;
            Area::new(p.x, p.y, p.x + p.width, p.y + p.height)
        });
        let places = [
            (
                Attendant::Session,
                self.module_area(IdleBarEntry::Power),
                self.session.frame().as_ref().map(area_of),
            ),
            (
                Attendant::Utilities,
                status,
                self.utilities.frame().as_ref().map(area_of),
            ),
            (
                Attendant::Overflow,
                self.module_area(IdleBarEntry::Overflow),
                self.overflow.frame().as_ref().map(area_of),
            ),
            (Attendant::Settings, None, settings),
        ];
        for (key, source, body) in places {
            self.attention.place(key, source, body);
        }
        // A panel that went in by itself (its ✕, a press, its own Escape).
        for key in Attendant::PANELS {
            let pending = self.attention_state.effects.iter().any(|(k, _)| *k == key);
            if self.attention.is_out(key) && !self.panel_open(key) && !pending {
                changes.extend(self.attention.close(key));
            }
        }
        changes
    }

    /// The pointer moved on the scene, or (`None`) left it for a window.
    /// `on_workspaces`: it is on the workspaces module; `quiet`: it designates
    /// nothing (another surface has the pointer).
    pub(super) fn attention_pointer(
        &mut self,
        at: Option<(f32, f32)>,
        on_workspaces: bool,
        quiet: bool,
        cx: &mut Context<Self>,
    ) {
        let mut changes = self.sync_attention();
        let over = crate::attention::over(on_workspaces, self.bar_popout.designated())
            .filter(|_| at.is_some() && !quiet);
        changes.extend(self.attention.pointer(at, over, Instant::now()));
        self.report_aim();
        self.attend(changes, cx);
    }

    /// `GNOS_UX_ATTENTION_AIM`: for each surface out, whether the pointer
    /// heads into it (`true`), away (`false`) or cannot be told, on change.
    fn report_aim(&mut self) {
        let out = self.attention.out();
        let speed = self.attention.heading_speed();
        let mut seen = Vec::new();
        for key in out {
            let verdict = self.attention.aim_verdict(key);
            let before = self
                .attention_state
                .aim_reported
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| *v);
            if before != Some(verdict) {
                println!(
                    "GNOS_UX_ATTENTION_AIM surface={} verdict={} speed={}",
                    key.slug(),
                    verdict.map_or("unknown", |v| if v { "toward" } else { "away" }),
                    speed.map_or("rest".to_owned(), |v| format!("{v:.0}"))
                );
            }
            seen.push((key, verdict));
        }
        self.attention_state.aim_reported = seen;
    }

    /// A press: whatever is out that it is not on goes back in.
    pub(super) fn attention_press(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        let mut changes = self.sync_attention();
        changes.extend(self.attention.press((x, y)));
        self.attend(changes, cx);
    }

    /// A click that means this surface (the clock, the power button, the
    /// current workspace…): out and held, or back in.
    pub(super) fn attention_toggle(&mut self, key: Attendant, cx: &mut Context<Self>) {
        if self.bar_travelling {
            return;
        }
        let mut changes = self.sync_attention();
        changes.extend(self.attention.toggle(key));
        self.attend(changes, cx);
    }

    /// A command that opens this surface.
    pub(super) fn attention_request(&mut self, key: Attendant, cx: &mut Context<Self>) {
        let mut changes = self.sync_attention();
        changes.extend(self.attention.request(key));
        self.attend(changes, cx);
    }

    /// Settings on `page` (a popout's "Open settings"), once whatever is out
    /// has gone in.
    pub(super) fn request_settings(
        &mut self,
        page: crate::settings_pages::PageId,
        cx: &mut Context<Self>,
    ) {
        self.attention_state.settings_page = Some(page);
        self.attention_request(Attendant::Settings, cx);
    }

    /// Escape: a change still waiting is called off, else the surface out
    /// goes back in. Returns whether anything did.
    pub(super) fn attention_escape(&mut self, cx: &mut Context<Self>) -> bool {
        let changes = self.attention.escape();
        let any = !changes.is_empty();
        self.attend(changes, cx);
        any
    }

    /// A choice made in it: back in until the pointer leaves it.
    pub(super) fn attention_dismiss(&mut self, key: Attendant, cx: &mut Context<Self>) {
        let changes = self.attention.dismiss(key);
        self.attend(changes, cx);
    }

    /// Put these back in (a travel, a lock, the module lifted).
    pub(super) fn attention_close(&mut self, keys: &[Attendant], cx: &mut Context<Self>) {
        let changes = keys.iter().flat_map(|k| self.attention.close(*k)).collect();
        self.attend(changes, cx);
    }

    /// On each motion step: the timers, then which surfaces are still on
    /// screen (the last one going in lets the next one out). Returns whether
    /// the attention still needs steps.
    pub(super) fn attention_tick(&mut self, now: Instant) -> bool {
        // A panel that went in by itself (Escape, without a pointer move).
        let mut changes = self.sync_attention();
        changes.extend(self.attention.tick(now));
        for key in self.attendants() {
            let visible = self.on_screen(key);
            // The gate's evidence: each surface's coming on and going off
            // screen, as its motion reports it.
            if visible != self.attention.is_visible(key) {
                println!(
                    "GNOS_UX_ATTENTION_VISIBLE surface={} visible={visible}",
                    key.slug()
                );
            }
            changes.extend(self.attention.visible(key, visible));
        }
        self.carry_out(changes);
        let arranged = self.arrange_meetings();
        arranged
            || self.attention.next_deadline().is_some()
            || !self.attention_state.effects.is_empty()
    }

    /// The panels' changes, with the window (each frame).
    pub(super) fn attention_apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (key, out) in std::mem::take(&mut self.attention_state.effects) {
            match key {
                Attendant::Session => self.session_set_open(out, cx),
                Attendant::Utilities => self.utilities_set_open(out, cx),
                Attendant::Overflow => self.overflow_set_open(out, cx),
                Attendant::Tray(index) if out => {
                    if let Some(at) = self.tray_point(index) {
                        self.tray_open_menu(index, at, cx);
                    }
                }
                Attendant::Tray(index) => {
                    if self.rail_tray_menu_key == Some(index) {
                        self.rail_tray_menu_dismiss(cx);
                    }
                }
                Attendant::Settings => {
                    let outcome = if out {
                        match self.attention_state.settings_page.take() {
                            Some(page) => self.settings.open_page(page),
                            None => self.settings.open(),
                        }
                    } else {
                        self.settings.close()
                    };
                    self.apply_settings_outcome(outcome, window, cx);
                }
                _ => {}
            }
        }
    }

    fn attend(&mut self, changes: Vec<Transition<Attendant>>, cx: &mut Context<Self>) {
        let changed = self.carry_out(changes);
        if changed
            || self.attention.next_deadline().is_some()
            || !self.attention_state.effects.is_empty()
        {
            self.start_motion_driver(cx);
            cx.notify();
        }
    }

    /// Carry the conductor's changes out; returns whether any surface
    /// changed (or a panel's change waits for the window).
    fn carry_out(&mut self, changes: Vec<Transition<Attendant>>) -> bool {
        let mut changed = false;
        for t in &changes {
            println!("{}", crate::attention::receipt(t));
            let out = t.to.is_out();
            match t.key {
                Attendant::Workspaces => {
                    if self.workspaces.set_open(out) {
                        changed = true;
                        self.report_workspaces(if out { "opening" } else { "closing" });
                    }
                }
                Attendant::Dashboard | Attendant::Status(_) => {
                    // BAR-02A: the clock hidden in the "…" anchors the
                    // dashboard beside it.
                    let fallback = (t.key == Attendant::Dashboard)
                        .then(|| self.module_anchor(self.rail_side(), IdleBarEntry::Clock))
                        .flatten();
                    if crate::attention::apply_to_host(&mut self.bar_popout, t, fallback) {
                        changed = true;
                        if t.key == Attendant::Dashboard {
                            self.report_dashboard(if out { "opening" } else { "closing" });
                        } else {
                            println!(
                                "GNOS_UX_UXE01_POPOUT state={}",
                                if out { "opening" } else { "closing" }
                            );
                        }
                    }
                }
                // FOCUS-01C: a tray menu is asked of its application (with
                // the window); one handed off to a neighbour's is replaced
                // by it, not dismissed.
                Attendant::Tray(_) => {
                    // Only a change of side (lingering is still out), and a
                    // handoff is the next menu's, not a dismissal.
                    if t.from.is_out() != out
                        && (out || t.reason != crate::attention::Reason::Handoff)
                    {
                        self.attention_state.effects.retain(|(k, _)| *k != t.key);
                        self.attention_state.effects.push((t.key, out));
                        changed = true;
                    }
                }
                key => {
                    // Queued is not out yet: only a real change of side.
                    if self.panel_open(key) != out {
                        self.attention_state.effects.retain(|(k, _)| *k != key);
                        self.attention_state.effects.push((key, out));
                        changed = true;
                    }
                }
            }
        }
        changed
    }
}
