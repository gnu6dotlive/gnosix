//! FOCUS-01: the shell's attention, one state machine for every transient
//! surface.
//!
//! Each surface that comes out of the shell (a Bar popout, the workspaces
//! panel, a tray menu, Session…) is an *attendant*: it is declared once with
//! its trigger, and placed each frame with its source (the module it comes
//! out of) and its body (where it stands). The conductor alone decides which
//! attendants are out, from one stream of inputs: where the pointer is and
//! which source it designates, the pointer leaving the shell, clicks, a press
//! elsewhere, Escape, a choice made, the keyboard's focus.
//!
//! The surfaces keep their content and their motion; they no longer carry a
//! lifecycle of their own. Every change is a [`Transition`] with its reason,
//! so one receipt explains why anything opened or closed.
//!
//! Time is injected: the conductor is tested without a compositor.
//!
//! ```compile_fail
//! // Session's trigger is click-only by construction: nothing outside this
//! // module can build one, so a setting cannot forge or rewrite it.
//! use gnos_ux_surface::grammar::attention::{Locked, Trigger};
//! let _ = Trigger::ClickOnly(Locked(()));
//! ```

use std::time::{Duration, Instant};

/// An axis-aligned area in scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Area {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn contains(&self, (x, y): (f32, f32)) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    /// The span between a source and the body it extrudes, across the gap
    /// that separates them: the pointer crossing it keeps the surface.
    fn bridge(source: Area, body: Area) -> Option<Area> {
        let cross = |a0: f32, a1: f32, b0: f32, b1: f32| {
            let (lo, hi) = (a0.max(b0), a1.min(b1));
            if hi > lo { (lo, hi) } else { (a0, a1) }
        };
        if source.bottom <= body.top || body.bottom <= source.top {
            let (top, bottom) = if source.bottom <= body.top {
                (source.bottom, body.top)
            } else {
                (body.bottom, source.top)
            };
            let (left, right) = cross(source.left, source.right, body.left, body.right);
            Some(Area::new(left, top, right, bottom))
        } else if source.right <= body.left || body.right <= source.left {
            let (left, right) = if source.right <= body.left {
                (source.right, body.left)
            } else {
                (body.right, source.left)
            };
            let (top, bottom) = cross(source.top, source.bottom, body.top, body.bottom);
            Some(Area::new(left, top, right, bottom))
        } else {
            None
        }
    }
}

/// FOCUS-01D: what a surface opened by the pointer does when one opened
/// deliberately (a click, a command) is out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Meeting {
    /// It does not come out over it (the default: one surface at a time).
    #[default]
    OneAtATime,
    /// Both stay out; the deliberate one gives way into its edge by what
    /// the other occupies of it (the caller applies the geometry).
    MakeRoom,
    /// Both stay out; the one opened by the pointer stands above, with its
    /// own outline.
    Layer,
}

/// Where the pointer was at the start of the heading's window, where it is,
/// and its velocity, px/s.
type Heading = ((f32, f32), (f32, f32), (f32, f32));

/// FOCUS-01B: how the pointer's heading is read (the operator's settings).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    /// How far outside a body a heading still counts as aiming at it, px.
    pub slack: f32,
    /// Below this speed, px/s, the pointer is resting, heading nowhere.
    pub min_speed: f32,
    /// How far back the heading is read, and how long an aim holds a
    /// surface without a further move.
    pub window: Duration,
}

impl Area {
    /// Distance from `p` to the area (0 inside).
    fn distance(&self, (x, y): (f32, f32)) -> f32 {
        let dx = (self.left - x).max(0.0).max(x - self.right);
        let dy = (self.top - y).max(0.0).max(y - self.bottom);
        dx.hypot(dy)
    }

    fn grown(&self, by: f32) -> Area {
        Area::new(
            self.left - by,
            self.top - by,
            self.right + by,
            self.bottom + by,
        )
    }

    /// Whether the ray from `p` along `v` enters the area (slab method).
    fn hit_by(&self, p: (f32, f32), v: (f32, f32)) -> bool {
        let mut near = 0.0_f32;
        let mut far = f32::INFINITY;
        for (o, d, lo, hi) in [
            (p.0, v.0, self.left, self.right),
            (p.1, v.1, self.top, self.bottom),
        ] {
            if d.abs() < f32::EPSILON {
                if o < lo || o >= hi {
                    return false;
                }
                continue;
            }
            let (a, b) = ((lo - o) / d, (hi - o) / d);
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
        near <= far
    }
}

/// How long the pointer rests before a hover surface comes out, and how long
/// it may be away before it goes back in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoverTiming {
    pub dwell: Duration,
    pub grace: Duration,
}

/// The proof a trigger is click-only. Only this module builds one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locked(());

/// What brings an attendant out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// The pointer resting on its source.
    Hover(HoverTiming),
    /// A click on its source; the operator may change it to hover.
    Click,
    /// A click on its source, and nothing else, ever (Session: its actions
    /// shut the hardware down).
    ClickOnly(Locked),
    /// It does not come out.
    Off,
}

/// What an operator's setting may ask of a trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerPreference {
    Hover(HoverTiming),
    Click,
    Off,
}

impl Trigger {
    pub const fn click_only() -> Self {
        Self::ClickOnly(Locked(()))
    }

    /// The trigger an operator's preference gives; a click-only trigger
    /// stays click-only.
    pub fn preferred(self, preference: TriggerPreference) -> Self {
        match (self, preference) {
            (Self::ClickOnly(_), _) => self,
            (_, TriggerPreference::Hover(timing)) => Self::Hover(timing),
            (_, TriggerPreference::Click) => Self::Click,
            (_, TriggerPreference::Off) => Self::Off,
        }
    }

    fn hover(self) -> Option<HoverTiming> {
        match self {
            Self::Hover(timing) => Some(timing),
            _ => None,
        }
    }
}

/// Where an attendant is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Rest,
    /// The pointer rests on the source; it comes out at the deadline.
    Arming,
    /// Out because the pointer is (or was just) on it.
    Open,
    /// Out, the pointer away; it goes back in at the deadline.
    Lingering,
    /// Out by a click or the keyboard: the pointer does not move it.
    Held,
    /// Asked out, waiting for the stage: another surface (not of its kin)
    /// is still visible, going back in. It comes out when that surface
    /// reports it is in, never on a timer.
    Queued,
}

/// Why an attendant changed phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The pointer arrived on the source.
    Hover,
    /// The dwell elapsed.
    Dwell,
    /// The pointer came back within the grace.
    Return,
    /// The pointer went away (the grace starts, or it was zero).
    Away,
    /// The grace elapsed.
    Leave,
    Click,
    /// A press outside it.
    Press,
    Escape,
    /// A choice made in it: it stays in until the pointer leaves.
    Choice,
    /// Another attendant came out in its place.
    Displaced,
    /// The keyboard's focus.
    Key,
    /// Its trigger changed, or the caller closed it (a travel, a lock).
    Closed,
    /// The pointer's heading: toward the surface it holds (the grace is
    /// renewed as long as it heads there), away from it at speed it goes in
    /// at once.
    Aim,
    /// One of its kin came out in its place: the surface they share morphs
    /// from one to the other instead of going in and out.
    Handoff,
    /// The surface it waited for is in: its turn.
    Stage,
    /// A surface opened deliberately (a click, a command) is out: a hover
    /// does not take its place.
    Blocked,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Dwell => "dwell",
            Self::Return => "return",
            Self::Away => "away",
            Self::Leave => "leave",
            Self::Click => "click",
            Self::Press => "press",
            Self::Escape => "escape",
            Self::Choice => "choice",
            Self::Displaced => "displaced",
            Self::Key => "key",
            Self::Closed => "closed",
            Self::Aim => "aim",
            Self::Handoff => "handoff",
            Self::Stage => "stage",
            Self::Blocked => "blocked",
        }
    }
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::Arming => "arming",
            Self::Open => "open",
            Self::Lingering => "lingering",
            Self::Held => "held",
            Self::Queued => "queued",
        }
    }

    /// Whether the surface is (going) out.
    pub fn is_out(self) -> bool {
        matches!(self, Self::Open | Self::Lingering | Self::Held)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition<K> {
    pub key: K,
    pub from: Phase,
    pub to: Phase,
    pub reason: Reason,
}

#[derive(Clone, Debug)]
struct Slot<K> {
    key: K,
    trigger: Trigger,
    source: Option<Area>,
    body: Option<Area>,
    phase: Phase,
    deadline: Option<Instant>,
    /// A choice was made: no hover opening until the pointer has left.
    dismissed: bool,
    /// When it came out, to close the most recent first.
    order: u64,
    /// Attendants of one kin share a surface: they hand off to each other.
    kin: Option<u16>,
    /// Whether its surface is on screen at all (reported by the caller from
    /// its motion): the stage is clear only when no other one is.
    visible: bool,
    /// What it comes out as once the stage is clear.
    queued_to: Phase,
}

/// The conductor: every attendant, and the pointer as last seen.
#[derive(Clone, Debug)]
pub struct Conductor<K> {
    slots: Vec<Slot<K>>,
    at: Option<(f32, f32)>,
    over: Option<K>,
    order: u64,
    /// FOCUS-01B: the pointer's recent path, to read its heading.
    aim: Option<Aim>,
    trail: Vec<((f32, f32), Instant)>,
    meeting: Meeting,
    /// The warm Bar: from an open surface, its kin opens with no dwell.
    warm_handoff: bool,
}

impl<K> Default for Conductor<K> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            at: None,
            over: None,
            order: 0,
            aim: None,
            trail: Vec::new(),
            meeting: Meeting::OneAtATime,
            warm_handoff: true,
        }
    }
}

impl<K: Copy + Eq> Conductor<K> {
    pub fn new() -> Self {
        Self::default()
    }

    fn slot(&self, key: K) -> Option<&Slot<K>> {
        self.slots.iter().find(|s| s.key == key)
    }

    fn index(&self, key: K) -> Option<usize> {
        self.slots.iter().position(|s| s.key == key)
    }

    /// Declare an attendant, or change its trigger. A click-only attendant
    /// keeps its trigger whatever is asked; a trigger that no longer allows
    /// the surface to be out closes it.
    pub fn declare(&mut self, key: K, trigger: Trigger) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        let Some(i) = self.index(key) else {
            self.slots.push(Slot {
                key,
                trigger,
                source: None,
                body: None,
                phase: Phase::Rest,
                deadline: None,
                dismissed: false,
                order: 0,
                kin: None,
                visible: false,
                queued_to: Phase::Rest,
            });
            return out;
        };
        let slot = &mut self.slots[i];
        if matches!(slot.trigger, Trigger::ClickOnly(_)) || slot.trigger == trigger {
            return out;
        }
        slot.trigger = trigger;
        let keep = match trigger {
            Trigger::Off => false,
            Trigger::Hover(_) => true,
            // A hover opening does not survive the change to a click.
            Trigger::Click | Trigger::ClickOnly(_) => slot.phase == Phase::Held,
        };
        if !keep && slot.phase != Phase::Rest {
            self.set(i, Phase::Rest, Reason::Closed, None, &mut out);
        }
        out
    }

    /// Its kin: attendants of one kin share a surface and hand off to each
    /// other (a morph); the others wait for the stage.
    pub fn set_kin(&mut self, key: K, kin: Option<u16>) {
        if let Some(i) = self.index(key) {
            self.slots[i].kin = kin;
        }
    }

    fn kin(&self, i: usize, j: usize) -> bool {
        self.slots[i].kin.is_some() && self.slots[i].kin == self.slots[j].kin
    }

    /// Whether its surface is on screen (any part of it), from its motion.
    /// The last one going in clears the stage for the one waiting.
    pub fn visible(&mut self, key: K, visible: bool) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        if let Some(i) = self.index(key) {
            self.slots[i].visible = visible;
        }
        self.promote(&mut out);
        out
    }

    /// FOCUS-01B: read the pointer's heading (`None`: no prediction).
    pub fn set_aim(&mut self, aim: Option<Aim>) {
        self.aim = aim;
        if aim.is_none() {
            self.trail.clear();
        }
    }

    /// Where the pointer was at the start of the window, where it is, and
    /// its velocity, px/s, when it moves fast enough to be heading
    /// somewhere.
    fn heading(&self) -> Option<Heading> {
        let aim = self.aim?;
        let (first, last) = (self.trail.first()?, self.trail.last()?);
        let dt = last.1.duration_since(first.1).as_secs_f32();
        if dt <= 0.0 {
            return None;
        }
        let v = ((last.0.0 - first.0.0) / dt, (last.0.1 - first.0.1) / dt);
        (v.0.hypot(v.1) >= aim.min_speed).then_some((first.0, last.0, v))
    }

    /// Whether the pointer heads into `i`'s body (its path enters it, or it
    /// draws nearer), or away from it (it draws away by more than the
    /// slack). `None` when it cannot be told: no prediction, the pointer
    /// resting, no body, or a path along the body neither in nor away.
    fn aims_at(&self, i: usize) -> Option<bool> {
        let aim = self.aim?;
        let body = self.slots[i].body?;
        let (from, p, v) = self.heading()?;
        let (then, now) = (body.distance(from), body.distance(p));
        if body.grown(aim.slack).hit_by(p, v) || now < then - f32::EPSILON {
            Some(true)
        } else if now > then + aim.slack {
            Some(false)
        } else {
            None
        }
    }

    /// The pointer's speed, px/s, when it is heading somewhere (receipts).
    pub fn heading_speed(&self) -> Option<f32> {
        self.heading().map(|(_, _, v)| v.0.hypot(v.1))
    }

    /// Whether the pointer heads into `key`'s body, away from it, or cannot
    /// be told (receipts).
    pub fn aim_verdict(&self, key: K) -> Option<bool> {
        self.aims_at(self.index(key)?)
    }

    /// Whether the pointer is on its way into the body of one of `i`'s kin
    /// that is out: crossing `i`'s source on the way is not asking for it.
    fn passing_to_kin(&self, i: usize) -> bool {
        (0..self.slots.len()).any(|j| {
            j != i
                && self.kin(i, j)
                && self.slots[j].phase.is_out()
                && self.aims_at(j) == Some(true)
        })
    }

    /// Whether sliding from an open surface to its kin opens it with no
    /// dwell (the warm Bar; on by default).
    pub fn set_warm_handoff(&mut self, on: bool) {
        self.warm_handoff = on;
    }

    /// Whether one of `i`'s kin is out by the pointer.
    fn warm(&self, i: usize) -> bool {
        self.warm_handoff
            && (0..self.slots.len()).any(|j| {
                j != i
                    && self.kin(i, j)
                    && matches!(self.slots[j].phase, Phase::Open | Phase::Lingering)
            })
    }

    /// Whether its surface was last reported on screen.
    pub fn is_visible(&self, key: K) -> bool {
        self.slot(key).is_some_and(|s| s.visible)
    }

    /// FOCUS-01D: what a pointer-opened surface does when a deliberate one is
    /// out.
    pub fn set_meeting(&mut self, meeting: Meeting) {
        self.meeting = meeting;
    }

    pub fn meeting(&self) -> Meeting {
        self.meeting
    }

    /// Whether a surface asked out as `to` stands with `j` (out
    /// deliberately) rather than waiting for it or taking its place.
    fn coexists(&self, to: Phase, j: usize) -> bool {
        self.meeting != Meeting::OneAtATime
            && to == Phase::Open
            && self.slots[j].phase == Phase::Held
    }

    /// The deliberate surfaces out and the pointer-opened ones standing
    /// with them (FOCUS-01D: make room, layer).
    pub fn meetings(&self) -> Vec<(K, K)> {
        let held = self.slots.iter().filter(|s| s.phase == Phase::Held);
        held.flat_map(|d| {
            self.slots
                .iter()
                .filter(|h| matches!(h.phase, Phase::Open | Phase::Lingering))
                .filter(move |h| !(h.kin.is_some() && h.kin == d.kin))
                .map(move |h| (d.key, h.key))
        })
        .collect()
    }

    /// Whether another surface, not of `i`'s kin, is still on stage for one
    /// asked out as `to`.
    fn stage_busy(&self, i: usize, to: Phase) -> bool {
        (0..self.slots.len()).any(|j| {
            j != i
                && !self.kin(i, j)
                && !self.coexists(to, j)
                && (self.slots[j].visible || self.slots[j].phase.is_out())
        })
    }

    fn promote(&mut self, out: &mut Vec<Transition<K>>) {
        for i in 0..self.slots.len() {
            let to = self.slots[i].queued_to;
            if self.slots[i].phase == Phase::Queued && !self.stage_busy(i, to) {
                let to = self.slots[i].queued_to;
                self.set(i, to, Reason::Stage, None, out);
            }
        }
    }

    pub fn trigger(&self, key: K) -> Option<Trigger> {
        self.slot(key).map(|s| s.trigger)
    }

    /// Where its source and its body stand now (scene coordinates).
    pub fn place(&mut self, key: K, source: Option<Area>, body: Option<Area>) {
        if let Some(i) = self.index(key) {
            self.slots[i].source = source;
            self.slots[i].body = body;
        }
    }

    pub fn phase(&self, key: K) -> Phase {
        self.slot(key).map_or(Phase::Rest, |s| s.phase)
    }

    pub fn is_out(&self, key: K) -> bool {
        self.phase(key).is_out()
    }

    /// Whether it is held out (by a click or the keyboard).
    pub fn is_held(&self, key: K) -> bool {
        self.phase(key) == Phase::Held
    }

    /// The attendants out, most recent last.
    pub fn out(&self) -> Vec<K> {
        let mut out: Vec<&Slot<K>> = self.slots.iter().filter(|s| s.phase.is_out()).collect();
        out.sort_by_key(|s| s.order);
        out.into_iter().map(|s| s.key).collect()
    }

    /// The next instant a timer runs out, for the caller's frame driver.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.slots.iter().filter_map(|s| s.deadline).min()
    }

    fn set(
        &mut self,
        i: usize,
        to: Phase,
        reason: Reason,
        deadline: Option<Instant>,
        out: &mut Vec<Transition<K>>,
    ) {
        let slot = &mut self.slots[i];
        let from = slot.phase;
        slot.deadline = deadline;
        if from == to {
            return;
        }
        if !from.is_out() && to.is_out() {
            self.order += 1;
            slot.order = self.order;
        }
        slot.phase = to;
        out.push(Transition {
            key: slot.key,
            from,
            to,
            reason,
        });
    }

    /// Whether the pointer is on the attendant: its source (as designated
    /// by the caller, who hit-tests the Bar), its body, or the gap between.
    fn in_zone(&self, i: usize) -> bool {
        let slot = &self.slots[i];
        if self.over == Some(slot.key) {
            return true;
        }
        let Some(at) = self.at else {
            return false;
        };
        slot.source.is_some_and(|s| s.contains(at))
            || slot.body.is_some_and(|b| {
                b.contains(at)
                    || slot
                        .source
                        .and_then(|s| Area::bridge(s, b))
                        .is_some_and(|g| g.contains(at))
            })
    }

    /// Bring one attendant out (`Open` by the pointer, `Held` by a click,
    /// a command or the keyboard). One surface at a time:
    /// - a hover does not take the place of a surface opened deliberately;
    /// - its kin out hand off to it (their shared surface morphs);
    /// - any other surface out goes back in, and it waits, queued, until
    ///   every other surface has reported it is off screen.
    fn open(&mut self, i: usize, to: Phase, reason: Reason, out: &mut Vec<Transition<K>>) {
        let deliberate = |s: &Slot<K>| {
            s.phase == Phase::Held || (s.phase == Phase::Queued && s.queued_to == Phase::Held)
        };
        if to != Phase::Held
            && self.meeting == Meeting::OneAtATime
            && (0..self.slots.len()).any(|j| j != i && deliberate(&self.slots[j]))
        {
            self.set(i, Phase::Rest, Reason::Blocked, None, out);
            return;
        }
        for j in 0..self.slots.len() {
            if j == i || self.coexists(to, j) {
                continue;
            }
            let phase = self.slots[j].phase;
            if phase == Phase::Queued || phase == Phase::Arming {
                self.set(j, Phase::Rest, Reason::Displaced, None, out);
            } else if phase.is_out() {
                let why = if self.kin(i, j) {
                    Reason::Handoff
                } else {
                    Reason::Displaced
                };
                self.set(j, Phase::Rest, why, None, out);
            }
        }
        if self.stage_busy(i, to) {
            self.slots[i].queued_to = to;
            self.set(i, Phase::Queued, reason, None, out);
        } else {
            self.set(i, to, reason, None, out);
        }
    }

    /// Open on a command (Settings from a popout's "Open settings", a
    /// keyboard shortcut): held, after whatever is out has gone in.
    pub fn request(&mut self, key: K) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        if let Some(i) = self.index(key)
            && self.slots[i].trigger != Trigger::Off
            && self.slots[i].phase != Phase::Held
        {
            self.open(i, Phase::Held, Reason::Click, &mut out);
        }
        out
    }

    /// The pointer moved. `at` is `None` when it left the shell; `over` is
    /// the source it designates, if any.
    pub fn pointer(
        &mut self,
        at: Option<(f32, f32)>,
        over: Option<K>,
        now: Instant,
    ) -> Vec<Transition<K>> {
        self.at = at;
        self.over = over.filter(|_| at.is_some());
        // The heading is read from the moves inside the shell; leaving it,
        // the last moves still tell where the pointer was going (the scene
        // hears nothing more until it comes back).
        match (at, self.aim) {
            (Some(p), Some(aim)) => {
                self.trail.push((p, now));
                self.trail
                    .retain(|(_, t)| now.duration_since(*t) <= aim.window);
            }
            (Some(_), None) => self.trail.clear(),
            (None, _) => {}
        }
        let mut out = Vec::new();
        // The surface the pointer designates is decided first: what it
        // displaces then goes straight in rather than lingering.
        let first = self.over.and_then(|k| self.index(k));
        let order: Vec<usize> = first
            .into_iter()
            .chain((0..self.slots.len()).filter(|i| Some(*i) != first))
            .collect();
        for i in order {
            let inside = self.in_zone(i);
            let on_source = self.over == Some(self.slots[i].key);
            if !inside {
                self.slots[i].dismissed = false;
            }
            let (trigger, phase, dismissed) = {
                let s = &self.slots[i];
                (s.trigger, s.phase, s.dismissed)
            };
            let Some(timing) = trigger.hover() else {
                continue;
            };
            match phase {
                // On its way into a kin's surface, crossing this source is
                // not asking for it.
                Phase::Rest | Phase::Arming if on_source && self.passing_to_kin(i) => {}
                Phase::Rest | Phase::Arming if on_source && !dismissed && self.warm(i) => {
                    // The warm Bar: one of its kin is out, so the pointer
                    // sliding over is no passing-through; no dwell.
                    self.open(i, Phase::Open, Reason::Hover, &mut out);
                }
                Phase::Rest if on_source && !dismissed => {
                    if timing.dwell.is_zero() {
                        self.open(i, Phase::Open, Reason::Hover, &mut out);
                    } else {
                        self.set(
                            i,
                            Phase::Arming,
                            Reason::Hover,
                            Some(now + timing.dwell),
                            &mut out,
                        );
                    }
                }
                Phase::Arming if !on_source => {
                    self.set(i, Phase::Rest, Reason::Away, None, &mut out);
                }
                // Asked by the pointer, it stops waiting once the pointer
                // has gone.
                Phase::Queued if self.slots[i].queued_to == Phase::Open && !inside => {
                    self.set(i, Phase::Rest, Reason::Away, None, &mut out);
                }
                Phase::Open | Phase::Lingering if !inside => match self.aims_at(i) {
                    // Heading into it: held, the hold renewed by each move
                    // that keeps heading there (resting, it runs out).
                    Some(true) => {
                        let hold = timing
                            .grace
                            .max(self.aim.map_or(timing.grace, |a| a.window));
                        self.set(i, Phase::Lingering, Reason::Aim, Some(now + hold), &mut out);
                    }
                    // Heading elsewhere at speed: no reason to wait.
                    Some(false) => self.set(i, Phase::Rest, Reason::Aim, None, &mut out),
                    // Resting or unpredictable: the grace, once.
                    None if phase == Phase::Lingering => {}
                    None if timing.grace.is_zero() => {
                        self.set(i, Phase::Rest, Reason::Away, None, &mut out)
                    }
                    None => self.set(
                        i,
                        Phase::Lingering,
                        Reason::Away,
                        Some(now + timing.grace),
                        &mut out,
                    ),
                },
                Phase::Lingering if inside => {
                    self.set(i, Phase::Open, Reason::Return, None, &mut out);
                }
                _ => {}
            }
        }
        if at.is_none() {
            self.trail.clear();
        }
        out
    }

    /// Run the timers out to `now`.
    pub fn tick(&mut self, now: Instant) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        for i in 0..self.slots.len() {
            let (deadline, phase) = (self.slots[i].deadline, self.slots[i].phase);
            if deadline.is_none_or(|d| now < d) {
                continue;
            }
            match phase {
                Phase::Arming => self.open(i, Phase::Open, Reason::Dwell, &mut out),
                Phase::Lingering => self.set(i, Phase::Rest, Reason::Leave, None, &mut out),
                _ => self.slots[i].deadline = None,
            }
        }
        out
    }

    /// A click on its source: out and held, or, held, back in. Off does
    /// nothing.
    pub fn toggle(&mut self, key: K) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        let Some(i) = self.index(key) else {
            return out;
        };
        match (self.slots[i].trigger, self.slots[i].phase) {
            (Trigger::Off, _) => {}
            (_, Phase::Held | Phase::Queued) => {
                self.slots[i].dismissed = true;
                self.set(i, Phase::Rest, Reason::Click, None, &mut out);
            }
            _ => self.open(i, Phase::Held, Reason::Click, &mut out),
        }
        out
    }

    /// A press at `at`: whatever is out that it is not on goes back in.
    pub fn press(&mut self, at: (f32, f32)) -> Vec<Transition<K>> {
        self.at = Some(at);
        let mut out = Vec::new();
        for i in 0..self.slots.len() {
            let slot = &self.slots[i];
            let on = slot.source.is_some_and(|s| s.contains(at))
                || slot.body.is_some_and(|b| b.contains(at));
            if slot.phase.is_out() && !on {
                self.set(i, Phase::Rest, Reason::Press, None, &mut out);
            }
        }
        self.promote(&mut out);
        out
    }

    /// Escape: the most recent attendant out goes back in.
    pub fn escape(&mut self) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        // A change still waiting for the stage is called off first.
        let queued: Vec<usize> = (0..self.slots.len())
            .filter(|i| self.slots[*i].phase == Phase::Queued)
            .collect();
        if !queued.is_empty() {
            for i in queued {
                self.set(i, Phase::Rest, Reason::Escape, None, &mut out);
            }
            return out;
        }
        if let Some(key) = self.out().last().copied()
            && let Some(i) = self.index(key)
        {
            self.slots[i].dismissed = true;
            self.set(i, Phase::Rest, Reason::Escape, None, &mut out);
        }
        out
    }

    /// A choice was made in it: back in, and it stays in until the pointer
    /// has left it.
    pub fn dismiss(&mut self, key: K) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        if let Some(i) = self.index(key) {
            self.slots[i].dismissed = true;
            if self.slots[i].phase != Phase::Rest {
                self.set(i, Phase::Rest, Reason::Choice, None, &mut out);
            }
        }
        self.promote(&mut out);
        out
    }

    /// The caller puts it back in (a travel, a lock, its surface gone).
    pub fn close(&mut self, key: K) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        if let Some(i) = self.index(key)
            && self.slots[i].phase != Phase::Rest
        {
            self.set(i, Phase::Rest, Reason::Closed, None, &mut out);
        }
        self.promote(&mut out);
        out
    }

    /// The keyboard's focus moved to `key` (BAR-02B), or left the Bar. A
    /// hover attendant comes out at once and is held while focused.
    pub fn focus(&mut self, key: Option<K>) -> Vec<Transition<K>> {
        let mut out = Vec::new();
        for i in 0..self.slots.len() {
            let (focused, hover, phase) = {
                let s = &self.slots[i];
                (key == Some(s.key), s.trigger.hover().is_some(), s.phase)
            };
            if focused && hover && phase != Phase::Held {
                self.open(i, Phase::Held, Reason::Key, &mut out);
            } else if !focused && phase == Phase::Held && hover {
                self.set(i, Phase::Rest, Reason::Key, None, &mut out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum K {
        Workspaces,
        Dashboard,
        Audio,
        Session,
        Settings,
    }

    const MS: Duration = Duration::from_millis(1);

    fn hover(dwell: u64, grace: u64) -> Trigger {
        Trigger::Hover(HoverTiming {
            dwell: MS * dwell as u32,
            grace: MS * grace as u32,
        })
    }

    fn conductor() -> Conductor<K> {
        let mut c = Conductor::new();
        c.declare(K::Workspaces, hover(0, 280));
        c.declare(K::Dashboard, hover(120, 0));
        c.declare(K::Session, Trigger::click_only());
        c.declare(K::Audio, hover(0, 0));
        c.declare(K::Settings, Trigger::Click);
        // The dashboard and the status popouts share one surface.
        c.set_kin(K::Dashboard, Some(1));
        c.set_kin(K::Audio, Some(1));
        // The workspaces module on the Bar, its panel below with a gap.
        c.place(
            K::Workspaces,
            Some(Area::new(50.0, 10.0, 150.0, 50.0)),
            Some(Area::new(10.0, 60.0, 350.0, 300.0)),
        );
        c.place(
            K::Session,
            Some(Area::new(10.0, 10.0, 40.0, 50.0)),
            Some(Area::new(10.0, 60.0, 400.0, 160.0)),
        );
        c
    }

    fn reasons(t: &[Transition<K>]) -> Vec<(K, Phase, Reason)> {
        t.iter().map(|t| (t.key, t.to, t.reason)).collect()
    }

    fn aim() -> Aim {
        Aim {
            slack: 8.0,
            min_speed: 150.0,
            window: Duration::from_millis(80),
        }
    }

    /// The pointer from `a` to `b` in `steps` moves `ms` apart.
    fn path(
        c: &mut Conductor<K>,
        a: (f32, f32),
        b: (f32, f32),
        steps: u32,
        ms: u32,
        t0: Instant,
        over: impl Fn((f32, f32)) -> Option<K>,
    ) -> (Vec<Transition<K>>, Instant) {
        let mut all = Vec::new();
        let mut t = t0;
        for k in 1..=steps {
            let f = k as f32 / steps as f32;
            let p = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
            t = t0 + MS * (k * ms);
            all.extend(c.pointer(Some(p), over(p), t));
        }
        (all, t)
    }

    #[test]
    fn heading_into_the_panel_holds_it_past_its_grace() {
        let mut c = conductor();
        c.set_aim(Some(aim()));
        // A module near the output's edge: its panel stands off to the side.
        c.place(
            K::Workspaces,
            Some(Area::new(50.0, 10.0, 150.0, 50.0)),
            Some(Area::new(300.0, 60.0, 640.0, 300.0)),
        );
        let t0 = Instant::now();
        c.pointer(Some((140.0, 30.0)), Some(K::Workspaces), t0);
        // A long, unhurried diagonal toward the panel, outside every zone.
        let (t, end) = path(&mut c, (152.0, 31.0), (296.0, 64.0), 60, 16, t0, |_| None);
        assert!(
            t.iter().all(|t| t.to != Phase::Rest),
            "held while heading in: {t:?}"
        );
        assert!(end > t0 + MS * 280, "past the grace");
        assert_eq!(c.phase(K::Workspaces), Phase::Lingering);
        // In: open again.
        let t = c.pointer(Some((310.0, 70.0)), None, end + MS * 16);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Open, Reason::Return)]
        );
        // Resting outside, short of it: the hold runs out.
        c.pointer(Some((290.0, 58.0)), None, end + MS * 32);
        assert!(
            c.tick(end + Duration::from_secs(2))
                .iter()
                .any(|t| t.to == Phase::Rest)
        );
    }

    #[test]
    fn heading_away_at_speed_puts_it_back_at_once() {
        let mut c = conductor();
        c.set_aim(Some(aim()));
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        // Out of the module and away to the right, fast.
        let (t, _) = path(&mut c, (150.0, 30.0), (600.0, 20.0), 6, 8, t0, |_| None);
        assert!(
            t.iter()
                .any(|t| (t.key, t.to, t.reason) == (K::Workspaces, Phase::Rest, Reason::Aim)),
            "{t:?}"
        );
        // Along the panel's edge, neither in nor away: the grace, as before.
        let mut c = conductor();
        c.set_aim(Some(aim()));
        c.pointer(Some((140.0, 30.0)), Some(K::Workspaces), t0);
        let (t, _) = path(&mut c, (152.0, 30.0), (300.0, 30.0), 6, 16, t0, |_| None);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Lingering, Reason::Away)]
        );
    }

    #[test]
    fn leaving_the_shell_at_speed_away_is_read_from_the_last_moves() {
        let mut c = conductor();
        c.set_aim(Some(aim()));
        let t0 = Instant::now();
        c.pointer(Some((140.0, 30.0)), Some(K::Workspaces), t0);
        // Fast to the right, along the Bar, then out of the shell.
        let (_, end) = path(&mut c, (150.0, 30.0), (400.0, 20.0), 5, 8, t0, |_| None);
        let t = c.pointer(None, None, end + MS * 4);
        assert!(
            c.phase(K::Workspaces) == Phase::Rest,
            "the grace is not waited for: {t:?}"
        );
    }

    #[test]
    fn cutting_across_a_neighbour_on_the_way_to_the_popout_keeps_the_popout() {
        let mut c = conductor();
        c.set_aim(Some(aim()));
        c.declare(K::Dashboard, hover(0, 0));
        // Status icons on a top Bar: audio at 700, the dashboard's clock just
        // left of it at 660; the audio popout hangs below, wide to the left.
        c.place(
            K::Audio,
            Some(Area::new(690.0, 10.0, 710.0, 50.0)),
            Some(Area::new(500.0, 60.0, 720.0, 300.0)),
        );
        c.place(
            K::Dashboard,
            Some(Area::new(640.0, 10.0, 680.0, 50.0)),
            None,
        );
        let t0 = Instant::now();
        let t = c.pointer(Some((700.0, 30.0)), Some(K::Audio), t0);
        assert_eq!(reasons(&t), vec![(K::Audio, Phase::Open, Reason::Hover)]);
        // Diagonally down-left toward the popout, over the clock's module.
        let over = |p: (f32, f32)| {
            if (640.0..680.0).contains(&p.0) && p.1 < 50.0 {
                Some(K::Dashboard)
            } else if (690.0..710.0).contains(&p.0) && p.1 < 50.0 {
                Some(K::Audio)
            } else {
                None
            }
        };
        let (t, end) = path(&mut c, (700.0, 30.0), (600.0, 120.0), 10, 8, t0, over);
        assert!(
            t.iter().all(|t| t.key != K::Dashboard),
            "the neighbour was not asked for: {t:?}"
        );
        assert!(c.is_out(K::Audio));
        // Resting on the clock's module is asking for it: the warm handoff.
        let t = c.pointer(Some((660.0, 30.0)), Some(K::Dashboard), end + MS * 200);
        let t2 = c.pointer(Some((660.0, 30.0)), Some(K::Dashboard), end + MS * 400);
        assert!(
            t.iter()
                .chain(&t2)
                .any(|t| (t.key, t.to) == (K::Dashboard, Phase::Open)),
            "{t:?} {t2:?}"
        );
    }

    #[test]
    fn the_next_surface_waits_until_the_last_one_is_in() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        c.visible(K::Workspaces, true);
        // A click on the power button: the panel goes in, Session waits.
        let t = c.toggle(K::Session);
        assert_eq!(
            reasons(&t),
            vec![
                (K::Workspaces, Phase::Rest, Reason::Displaced),
                (K::Session, Phase::Queued, Reason::Click)
            ]
        );
        assert!(!c.is_out(K::Session));
        // Still retracting: still waiting, however long it takes.
        assert!(c.visible(K::Workspaces, true).is_empty());
        assert!(c.tick(t0 + Duration::from_secs(10)).is_empty());
        // In: Session's turn.
        let t = c.visible(K::Workspaces, false);
        assert_eq!(reasons(&t), vec![(K::Session, Phase::Held, Reason::Stage)]);
    }

    #[test]
    fn nothing_on_screen_nothing_to_wait_for() {
        let mut c = conductor();
        let t0 = Instant::now();
        // Out, but its motion never showed it: no wait.
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        let t = c.toggle(K::Session);
        assert_eq!(
            t.last().map(|t| (t.to, t.reason)),
            Some((Phase::Held, Reason::Click))
        );
    }

    #[test]
    fn kin_hand_off_without_waiting() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((700.0, 30.0)), Some(K::Audio), t0);
        c.visible(K::Audio, true);
        let t = c.pointer(Some((600.0, 30.0)), Some(K::Dashboard), t0);
        assert_eq!(
            reasons(&t),
            vec![
                (K::Audio, Phase::Rest, Reason::Handoff),
                (K::Dashboard, Phase::Open, Reason::Hover)
            ]
        );
    }

    #[test]
    fn without_the_warm_bar_a_neighbour_waits_for_its_dwell() {
        let mut c = conductor();
        c.set_warm_handoff(false);
        let t0 = Instant::now();
        c.pointer(Some((700.0, 30.0)), Some(K::Audio), t0);
        // The dashboard has a 120 ms dwell: it arms, it does not come out.
        let t = c.pointer(Some((600.0, 30.0)), Some(K::Dashboard), t0);
        assert!(t.contains(&Transition {
            key: K::Dashboard,
            from: Phase::Rest,
            to: Phase::Arming,
            reason: Reason::Hover
        }));
        assert!(!c.is_out(K::Dashboard));
    }

    #[test]
    fn a_hover_does_not_take_the_place_of_a_deliberate_surface() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.request(K::Settings);
        c.visible(K::Settings, true);
        // The pointer crosses the status icons and the workspaces module:
        // nothing comes out over Settings.
        assert!(
            c.pointer(Some((700.0, 30.0)), Some(K::Audio), t0)
                .is_empty()
        );
        assert!(
            c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0)
                .is_empty()
        );
        assert_eq!(c.out(), vec![K::Settings]);
        // A click does, after Settings is in.
        let t = c.toggle(K::Session);
        assert_eq!(
            reasons(&t),
            vec![
                (K::Settings, Phase::Rest, Reason::Displaced),
                (K::Session, Phase::Queued, Reason::Click)
            ]
        );
    }

    #[test]
    fn making_room_or_layering_lets_a_hover_stand_with_a_deliberate_surface() {
        for meeting in [Meeting::MakeRoom, Meeting::Layer] {
            let mut c = conductor();
            c.set_meeting(meeting);
            let t0 = Instant::now();
            c.request(K::Settings);
            c.visible(K::Settings, true);
            // The audio icon: it comes out at once, Settings stays held.
            let t = c.pointer(Some((700.0, 30.0)), Some(K::Audio), t0);
            assert_eq!(reasons(&t), vec![(K::Audio, Phase::Open, Reason::Hover)]);
            assert!(c.is_held(K::Settings));
            assert_eq!(c.meetings(), vec![(K::Settings, K::Audio)]);
            // A deliberate request still takes Settings' place, after it.
            let t = c.toggle(K::Session);
            assert!(t.contains(&Transition {
                key: K::Settings,
                from: Phase::Held,
                to: Phase::Rest,
                reason: Reason::Displaced
            }));
            assert_eq!(c.phase(K::Session), Phase::Queued);
        }
        // One at a time: the hover waits for no one, it simply does not
        // come out over Settings.
        let mut c = conductor();
        c.request(K::Settings);
        assert!(
            c.pointer(Some((700.0, 30.0)), Some(K::Audio), Instant::now())
                .is_empty()
        );
        assert!(c.meetings().is_empty());
    }

    #[test]
    fn open_settings_from_a_popout_waits_for_the_popout_to_go_in() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((700.0, 30.0)), Some(K::Audio), t0);
        c.visible(K::Audio, true);
        let t = c.request(K::Settings);
        assert_eq!(
            reasons(&t),
            vec![
                (K::Audio, Phase::Rest, Reason::Displaced),
                (K::Settings, Phase::Queued, Reason::Click)
            ]
        );
        // The pointer moving on does not reopen the popout meanwhile.
        assert!(
            c.pointer(Some((700.0, 30.0)), Some(K::Audio), t0)
                .is_empty()
        );
        let t = c.visible(K::Audio, false);
        assert_eq!(reasons(&t), vec![(K::Settings, Phase::Held, Reason::Stage)]);
    }

    #[test]
    fn a_queued_hover_is_called_off_when_the_pointer_goes_and_escape_calls_off_any() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        c.visible(K::Workspaces, true);
        c.declare(K::Dashboard, hover(0, 0));
        let t = c.pointer(Some((500.0, 30.0)), Some(K::Dashboard), t0);
        assert_eq!(
            reasons(&t),
            vec![
                (K::Workspaces, Phase::Rest, Reason::Displaced),
                (K::Dashboard, Phase::Queued, Reason::Hover)
            ]
        );
        let t = c.pointer(Some((900.0, 900.0)), None, t0);
        assert_eq!(reasons(&t), vec![(K::Dashboard, Phase::Rest, Reason::Away)]);
        assert!(c.visible(K::Workspaces, false).is_empty(), "nothing waits");
        // Escape calls off a change still waiting.
        c.visible(K::Workspaces, true);
        c.toggle(K::Session);
        let t = c.escape();
        assert_eq!(reasons(&t), vec![(K::Session, Phase::Rest, Reason::Escape)]);
        assert!(c.visible(K::Workspaces, false).is_empty());
    }

    #[test]
    fn hover_opens_and_the_grace_lets_the_pointer_cross_the_gap() {
        let mut c = conductor();
        let t0 = Instant::now();
        let t = c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Open, Reason::Hover)]
        );
        // Across the gap between the module and the panel: still in its zone.
        assert!(c.pointer(Some((100.0, 55.0)), None, t0).is_empty());
        assert!(
            c.pointer(Some((200.0, 200.0)), None, t0).is_empty(),
            "on the panel"
        );
        // Away: the grace starts, and coming back within it keeps it.
        let t = c.pointer(Some((900.0, 900.0)), None, t0);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Lingering, Reason::Away)]
        );
        assert!(c.tick(t0 + MS * 100).is_empty());
        let t = c.pointer(Some((200.0, 200.0)), None, t0 + MS * 150);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Open, Reason::Return)]
        );
        // Away for the whole grace: back in.
        c.pointer(Some((900.0, 900.0)), None, t0 + MS * 200);
        assert_eq!(c.next_deadline(), Some(t0 + MS * 480));
        let t = c.tick(t0 + MS * 480);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Rest, Reason::Leave)]
        );
    }

    #[test]
    fn leaving_the_shell_counts_as_leaving() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        // The pointer went to a window: the scene saw it leave.
        let t = c.pointer(None, None, t0);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Lingering, Reason::Away)]
        );
        assert_eq!(
            reasons(&c.tick(t0 + MS * 280)),
            vec![(K::Workspaces, Phase::Rest, Reason::Leave)]
        );
    }

    #[test]
    fn a_dwell_waits_and_passing_through_opens_nothing() {
        let mut c = conductor();
        let t0 = Instant::now();
        let t = c.pointer(Some((500.0, 30.0)), Some(K::Dashboard), t0);
        assert_eq!(
            reasons(&t),
            vec![(K::Dashboard, Phase::Arming, Reason::Hover)]
        );
        // Passing through: gone before the dwell.
        let t = c.pointer(Some((700.0, 30.0)), None, t0 + MS * 60);
        assert_eq!(reasons(&t), vec![(K::Dashboard, Phase::Rest, Reason::Away)]);
        assert!(c.tick(t0 + MS * 200).is_empty());
        // Resting: out at the dwell.
        c.pointer(Some((500.0, 30.0)), Some(K::Dashboard), t0 + MS * 300);
        assert!(c.tick(t0 + MS * 400).is_empty());
        assert_eq!(
            reasons(&c.tick(t0 + MS * 420)),
            vec![(K::Dashboard, Phase::Open, Reason::Dwell)]
        );
    }

    #[test]
    fn one_hover_surface_at_a_time() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        c.pointer(Some((500.0, 30.0)), Some(K::Dashboard), t0);
        let t = c.tick(t0 + MS * 120);
        assert_eq!(
            reasons(&t),
            vec![
                (K::Workspaces, Phase::Rest, Reason::Displaced),
                (K::Dashboard, Phase::Open, Reason::Dwell)
            ]
        );
    }

    #[test]
    fn session_opens_only_on_a_click_and_no_setting_changes_that() {
        let mut c = conductor();
        let t0 = Instant::now();
        assert!(
            c.pointer(Some((20.0, 30.0)), Some(K::Session), t0)
                .is_empty()
        );
        assert!(
            c.tick(t0 + Duration::from_secs(2)).is_empty(),
            "resting: nothing"
        );
        // A setting asking for hover is refused, at both doors.
        let asked = TriggerPreference::Hover(HoverTiming {
            dwell: Duration::ZERO,
            grace: Duration::ZERO,
        });
        assert_eq!(
            Trigger::click_only().preferred(asked),
            Trigger::click_only()
        );
        assert!(c.declare(K::Session, hover(0, 0)).is_empty());
        assert_eq!(c.trigger(K::Session), Some(Trigger::click_only()));
        assert!(
            c.pointer(Some((20.0, 30.0)), Some(K::Session), t0)
                .is_empty()
        );
        // A click holds it out; leaving does not put it back.
        let t = c.toggle(K::Session);
        assert_eq!(reasons(&t), vec![(K::Session, Phase::Held, Reason::Click)]);
        assert!(c.pointer(None, None, t0).is_empty());
        assert!(c.tick(t0 + Duration::from_secs(5)).is_empty());
        // A press on it keeps it; a press elsewhere puts it back.
        assert!(c.press((200.0, 100.0)).is_empty());
        let t = c.press((800.0, 800.0));
        assert_eq!(reasons(&t), vec![(K::Session, Phase::Rest, Reason::Press)]);
        // So for a hover surface still within its grace.
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        c.pointer(Some((800.0, 800.0)), None, t0);
        let t = c.press((800.0, 800.0));
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Rest, Reason::Press)]
        );
    }

    #[test]
    fn a_click_holds_a_hover_surface_and_a_second_click_puts_it_back() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        c.toggle(K::Workspaces);
        assert!(c.is_held(K::Workspaces));
        assert!(
            c.pointer(None, None, t0).is_empty(),
            "held: the pointer leaving is fine"
        );
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        let t = c.toggle(K::Workspaces);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Rest, Reason::Click)]
        );
        // Still on the module: it does not come straight back out…
        assert!(
            c.pointer(Some((101.0, 30.0)), Some(K::Workspaces), t0)
                .is_empty()
        );
        // …until the pointer has left and come back.
        c.pointer(Some((900.0, 900.0)), None, t0);
        let t = c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Open, Reason::Hover)]
        );
    }

    #[test]
    fn a_choice_and_escape_put_it_back_until_the_pointer_leaves() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        let t = c.dismiss(K::Workspaces);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Rest, Reason::Choice)]
        );
        assert!(
            c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0)
                .is_empty()
        );
        c.pointer(None, None, t0);
        let t = c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Open, Reason::Hover)]
        );
        // Escape puts back what is out (one surface at a time).
        c.toggle(K::Workspaces);
        let t = c.escape();
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Rest, Reason::Escape)]
        );
        assert!(c.escape().is_empty());
    }

    #[test]
    fn the_keyboard_holds_what_it_focuses() {
        let mut c = conductor();
        let t = c.focus(Some(K::Workspaces));
        assert_eq!(reasons(&t), vec![(K::Workspaces, Phase::Held, Reason::Key)]);
        // The keyboard moves on: the next one out, the previous one back in.
        c.focus(Some(K::Dashboard));
        assert_eq!(c.phase(K::Workspaces), Phase::Rest);
        assert_eq!(c.phase(K::Dashboard), Phase::Held);
        // Session never comes out under the keyboard's focus alone.
        c.focus(Some(K::Session));
        assert_eq!(c.phase(K::Session), Phase::Rest);
        assert_eq!(c.phase(K::Dashboard), Phase::Rest);
    }

    #[test]
    fn a_trigger_changed_to_off_or_click_closes_a_hover_opening() {
        let mut c = conductor();
        let t0 = Instant::now();
        c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0);
        let t = c.declare(K::Workspaces, Trigger::Click);
        assert_eq!(
            reasons(&t),
            vec![(K::Workspaces, Phase::Rest, Reason::Closed)]
        );
        assert!(
            c.pointer(Some((100.0, 30.0)), Some(K::Workspaces), t0)
                .is_empty()
        );
        c.declare(K::Workspaces, Trigger::Off);
        assert!(c.toggle(K::Workspaces).is_empty());
    }
}
