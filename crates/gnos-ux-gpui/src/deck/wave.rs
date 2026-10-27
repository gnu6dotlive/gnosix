//! DECK-01B: the magnification wave (D06).
//!
//! The pointer raises a size wave over the appcards. Each card grows by a
//! cosine bell of its rest distance to the pointer, and the Deck widens to
//! hold the grown cards. The point under the pointer stays under it: the
//! layout is shifted so the rest position the pointer stands on maps to
//! itself, so cards slide under a moving pointer instead of running away
//! from it.
//!
//! The envelope deforms with the wave rather than letting cards stick out of
//! it. Its far edge (away from the border the Deck stands on) follows the
//! bell, so the hump keeps the rest padding above every card and meets the
//! flat edge tangentially, the concave joint of a hull. The hovered card's
//! name extrudes as a flat-topped tab out of the hump, blended into it by a
//! smooth maximum; the tab grows in height, never in opacity.
//!
//! Everything here is pure geometry over rest coordinates; `DeckWave` holds
//! the animated amounts and the scene feeds it pointer positions.

use super::{CARD, DeckCard, DeckSpacing, GAP, HEIGHT, PAD, Slot, slot_width, slots};
use blobin_ui_core::layout::Rect;
use gnos_ux_surface::grammar::{Follow, HullCommand, Point2, TemporalResponseMode};
use std::time::Instant;

/// The wave's amount is animated in pixels of this nominal lift, so the
/// spring's settle tolerance means the same at every magnification.
const AMOUNT_PX: f32 = 24.0;
/// The label tab's height above the hump.
pub const TAB: f32 = 26.0;
/// Label text size, and the tab's padding around the text.
pub const LABEL_PX: f32 = 13.0;
pub const LABEL_PAD: f32 = 12.0;
/// Points per quarter arc of the tab's corners and fillets.
const ARC_STEPS: usize = 10;
/// Profile sampling step, in rest pixels.
const STEP: f32 = 2.0;
/// Quarter-circle cubic constant.
const KAPPA: f32 = 0.552_284_8;
const SETTLE_PX: f32 = 0.5;

/// The wave's user-facing values (DECK-01, D09), each bounded to a range
/// that keeps the Deck coherent. Build it with `bounded`; a value that is
/// not finite takes its default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveTuning {
    magnify: f32,
    reach_slots: f32,
    tempo: f32,
    shoulder: f32,
}

impl WaveTuning {
    /// Peak scale of the card under the pointer; 1 turns the wave off.
    pub const MAGNIFY: (f32, f32) = (1.0, 2.5);
    /// How many slots the wave reaches on each side of the pointer.
    pub const REACH_SLOTS: (f32, f32) = (1.0, 6.0);
    /// The springs' speed; reduced motion snaps whatever it is.
    pub const TEMPO: (f32, f32) = (0.25, 2.5);
    /// Width of the tab's shoulders, in pixels: its softness, the radius of
    /// the fillets at its feet.
    pub const SHOULDER: (f32, f32) = (4.0, 64.0);

    pub fn bounded(magnify: f32, reach_slots: f32, tempo: f32, shoulder: f32) -> Self {
        let d = Self::default();
        let fit = |v: f32, (lo, hi): (f32, f32), default: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        Self {
            magnify: fit(magnify, Self::MAGNIFY, d.magnify),
            reach_slots: fit(reach_slots, Self::REACH_SLOTS, d.reach_slots),
            tempo: fit(tempo, Self::TEMPO, d.tempo),
            shoulder: fit(shoulder, Self::SHOULDER, d.shoulder),
        }
    }

    pub fn magnify(&self) -> f32 {
        self.magnify
    }

    pub fn tempo(&self) -> f32 {
        self.tempo
    }

    /// Rest distance over which the wave falls back to scale 1.
    pub fn reach(&self) -> f32 {
        self.reach_slots * (CARD + GAP)
    }

    /// How far the hump rises at full magnification.
    pub fn lift(&self) -> f32 {
        CARD * (self.magnify - 1.0)
    }

    /// Cosine falloff: 1 at the pointer, 0 from the reach on, flat at both
    /// ends.
    pub fn bell(&self, distance: f32) -> f32 {
        let d = distance.abs();
        let reach = self.reach();
        if d >= reach {
            0.0
        } else {
            0.5 * (1.0 + (std::f32::consts::PI * d / reach).cos())
        }
    }

    /// DECK-03: the radius of the concave fillets where the tab's walls meet
    /// the hump.
    fn fillet(&self) -> f32 {
        self.shoulder
    }
}

impl Default for WaveTuning {
    /// The candidate values of DECK-01B.
    fn default() -> Self {
        Self {
            magnify: 1.5,
            reach_slots: 2.5,
            tempo: 1.0,
            shoulder: 24.0,
        }
    }
}

/// The animated state the shape is built from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaveInput {
    /// The pointer, in rest coordinates (from the rest Deck's left edge).
    pub pointer: f32,
    /// 0 at rest, 1 at full magnification.
    pub amount: f32,
    /// The label tab, when one shows or retracts.
    pub tab: Option<TabInput>,
    pub tuning: WaveTuning,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabInput {
    /// Centre and width of the tab, in scene coordinates.
    pub center: f32,
    pub width: f32,
    /// How far the tab stands out of the hump: up to `TAB` for a label, up
    /// to `height` for an action sheet (DECK-01D).
    pub extrusion: f32,
    /// The height its content is laid out for: `TAB`, or the sheet's.
    pub height: f32,
}

impl TabInput {
    pub fn label(center: f32, width: f32, extrusion: f32) -> Self {
        Self {
            center,
            width,
            extrusion,
            height: TAB,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckTab {
    /// Where the label text is laid out (`TAB` high, riding the tab's top).
    pub text_box: Rect,
    /// The sheet's body under the label, laid out at its full height and
    /// riding the tab's top with it (empty for a label).
    pub body: Rect,
    /// The part of the tab that has extruded; its content is clipped to it.
    pub clip: Rect,
}

/// The Deck as painted and hit-tested this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct DeckShape {
    /// The Deck at rest height and current width.
    pub base: Rect,
    pub cards: Vec<Rect>,
    pub scales: Vec<f32>,
    pub separators: Vec<Rect>,
    pub control: Rect,
    /// The envelope, in scene coordinates.
    pub outline: Vec<HullCommand>,
    /// Base, hump and tab.
    pub bounds: Rect,
    pub tab: Option<DeckTab>,
    /// The open slots of a drag (DECK-01D), where the card would land.
    pub gaps: Vec<Rect>,
    pub grows_up: bool,
}

impl DeckShape {
    /// The card under `(x, y)`: a point inside the shape whose x falls on a
    /// card or within half a gap of it.
    pub fn card_at(&self, x: f32, y: f32) -> Option<usize> {
        if !self.bounds.contains(x, y) {
            return None;
        }
        self.cards
            .iter()
            .position(|r| x >= r.x - GAP / 2.0 && x < r.x + r.width + GAP / 2.0)
    }

    /// The same shape moved by `dy` (BAR-01C: a Deck sliding into its edge
    /// and out of the new one).
    pub fn translated(mut self, dy: f32) -> Self {
        let mv = |r: &mut Rect| r.y += dy;
        mv(&mut self.base);
        mv(&mut self.control);
        mv(&mut self.bounds);
        self.cards.iter_mut().for_each(mv);
        self.separators.iter_mut().for_each(mv);
        self.gaps.iter_mut().for_each(mv);
        if let Some(tab) = &mut self.tab {
            mv(&mut tab.text_box);
            mv(&mut tab.body);
            mv(&mut tab.clip);
        }
        let p = |q: Point2| Point2::new(q.x(), q.y() + dy);
        for command in &mut self.outline {
            *command = match *command {
                HullCommand::MoveTo(a) => HullCommand::MoveTo(p(a)),
                HullCommand::LineTo(a) => HullCommand::LineTo(p(a)),
                HullCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => HullCommand::CubicTo {
                    control1: p(control1),
                    control2: p(control2),
                    to: p(to),
                },
                other => other,
            };
        }
        self
    }

    /// The centre the hovered card's label tab aims for.
    pub fn tab_target(&self, card: usize) -> Option<f32> {
        self.cards.get(card).map(|r| r.x + r.width / 2.0)
    }
}

/// The Deck's shape for these cards. `rest_left` is the rest Deck's left
/// edge, `base_top` the top of its rest-height body, `span` the horizontal
/// range it must stay within. A Deck that `grows_up` stands on the bottom
/// border and its hump rises; otherwise it hangs from the top and the hump
/// descends.
pub fn deck_shape(
    cards: &[DeckCard],
    rest_left: f32,
    base_top: f32,
    grows_up: bool,
    span: (f32, f32),
    wave: &WaveInput,
) -> DeckShape {
    deck_shape_spaced(
        cards,
        &DeckSpacing::default(),
        rest_left,
        base_top,
        grows_up,
        span,
        wave,
    )
}

/// `deck_shape` while a drag reshapes the row: slots keep their presence
/// (a card carried away, a gap opening) and the wave runs over what stands.
pub fn deck_shape_spaced(
    cards: &[DeckCard],
    spacing: &DeckSpacing,
    rest_left: f32,
    base_top: f32,
    grows_up: bool,
    span: (f32, f32),
    wave: &WaveInput,
) -> DeckShape {
    let amount = wave.amount.clamp(0.0, 1.0);
    let tuning = wave.tuning;
    let slots = slots(cards, spacing);
    let presence: Vec<f32> = slots.iter().map(|(_, p)| *p).collect();
    let slots: Vec<Slot> = slots.into_iter().map(|(s, _)| s).collect();
    // Rest starts and the widths that stand, then the grown widths and
    // starts.
    let mut rest = Vec::with_capacity(slots.len());
    let mut at = PAD;
    for (slot, p) in slots.iter().zip(&presence) {
        let w = slot_width(*slot) * p;
        rest.push((at, w));
        at += w + GAP * p;
    }
    let rest_width = at - GAP + PAD;
    let scales_at = |amount: f32| -> Vec<f32> {
        slots
            .iter()
            .zip(&rest)
            .map(|(slot, (x, w))| match slot {
                Slot::Card(_) | Slot::Gap => {
                    1.0 + (tuning.magnify - 1.0) * amount * tuning.bell(x + w / 2.0 - wave.pointer)
                }
                _ => 1.0,
            })
            .collect()
    };
    let growth = |scales: &[f32]| -> f32 {
        rest.iter()
            .zip(scales)
            .map(|((_, w), s)| w * (s - 1.0))
            .sum()
    };
    // Space aware: the growth is linear in the amount, so a span too narrow
    // for the full wave gets the largest wave that fits.
    let mut scales = scales_at(amount);
    let room = (span.1 - span.0 - rest_width).max(0.0);
    let grown_by = growth(&scales);
    let amount = if grown_by > room {
        let fitted = amount * room / grown_by;
        scales = scales_at(fitted);
        fitted
    } else {
        amount
    };
    let mut grown = Vec::with_capacity(slots.len());
    let mut at = PAD;
    for (((_, w), s), p) in rest.iter().zip(&scales).zip(&presence) {
        grown.push(at);
        at += w * s + GAP * p;
    }
    let width = at - GAP + PAD;
    // Rest x to unshifted grown x, piecewise linear.
    let forward = |x: f32| -> f32 {
        let mut previous_end = (PAD, PAD);
        for (i, (r0, w)) in rest.iter().enumerate() {
            if x < *r0 {
                return previous_end.1 + (x - previous_end.0);
            }
            if x <= r0 + w {
                return grown[i] + (x - r0) * scales[i];
            }
            previous_end = (r0 + w, grown[i] + w * scales[i]);
        }
        previous_end.1 + (x - previous_end.0)
    };
    let pointer = wave.pointer.clamp(0.0, rest_width);
    let mut left = rest_left
        + if amount > 0.0 {
            pointer - forward(pointer)
        } else {
            0.0
        };
    if left + width > span.1 {
        left = span.1 - width;
    }
    if left < span.0 {
        left = span.0;
    }
    let base = Rect::new(left, base_top, width, HEIGHT);
    let (top, bottom) = (base_top, base_top + HEIGHT);
    // Mirror for a Deck hanging from the top border.
    let flip = |y: f32| if grows_up { y } else { top + bottom - y };
    let flip_rect = |r: Rect| {
        if grows_up {
            r
        } else {
            Rect::new(r.x, top + bottom - r.y - r.height, r.width, r.height)
        }
    };

    let mut shape_cards = Vec::new();
    let mut card_scales = Vec::new();
    let mut gaps = Vec::new();
    let mut separators = Vec::new();
    let mut control = Rect::new(0.0, 0.0, 0.0, 0.0);
    for (i, slot) in slots.iter().enumerate() {
        let x = left + grown[i];
        match slot {
            Slot::Card(_) => {
                let size = CARD * scales[i] * presence[i];
                shape_cards.push(flip_rect(Rect::new(x, bottom - PAD - size, size, size)));
                card_scales.push(scales[i]);
            }
            Slot::Gap => {
                let size = CARD * scales[i] * presence[i];
                gaps.push(flip_rect(Rect::new(x, bottom - PAD - size, size, size)));
            }
            Slot::Separator => separators.push(Rect::new(
                x,
                top + PAD + CARD * 0.2,
                super::SEPARATOR,
                CARD * 0.6,
            )),
            Slot::Dictation => {}
            Slot::Control => control = Rect::new(x, top + PAD, super::CONTROL, CARD),
        }
    }

    // The far edge's profile: (scene x, rise) from the bell over rest x.
    let mut bell_profile = Vec::new();
    let mut x = 0.0;
    while x < rest_width {
        bell_profile.push((
            left + forward(x),
            tuning.lift() * amount * tuning.bell(x - wave.pointer),
        ));
        x += STEP;
    }
    bell_profile.push((
        left + width,
        tuning.lift() * amount * tuning.bell(rest_width - wave.pointer),
    ));
    let rise_at = |profile: &[(f32, f32)], at: f32| -> f32 {
        match profile.iter().position(|(px, _)| *px >= at) {
            None => profile.last().map_or(0.0, |p| p.1),
            Some(0) => profile[0].1,
            Some(i) => {
                let (x0, h0) = profile[i - 1];
                let (x1, h1) = profile[i];
                if x1 - x0 <= f32::EPSILON {
                    h1
                } else {
                    h0 + (h1 - h0) * (at - x0) / (x1 - x0)
                }
            }
        }
    };
    // The tab, blended in by how far it has extruded.
    let tab = wave.tab.filter(|t| t.extrusion > 0.0 && t.width > 0.0);
    let peak = tab.map_or(0.0, |t| rise_at(&bell_profile, t.center));
    let profile = match tab {
        Some(t) => tab_profile(&bell_profile, t, peak + t.extrusion, HEIGHT / 2.0, &tuning),
        None => bell_profile.clone(),
    };
    let tab_shape = tab.map(|t| {
        let crest = rise_at(&profile, t.center);
        let body_height = (t.height - TAB).max(0.0);
        // SET-03: a sheet hanging from the top may carry its name on the
        // Deck's side, its rows beyond it; both still ride the crest.
        let near = !grows_up && crate::surface_prefs::advanced("hanging_name_near") == 1;
        let (name_y, body_y) = if near {
            (top - crest + body_height, top - crest)
        } else {
            (top - crest, top - crest + TAB)
        };
        let text_box = Rect::new(t.center - t.width / 2.0, name_y, t.width, TAB);
        let body = Rect::new(text_box.x, body_y, t.width, body_height);
        let clip = Rect::new(text_box.x, top - crest, t.width, (crest - peak).max(0.0));
        DeckTab {
            text_box: flip_rect(text_box),
            body: flip_rect(body),
            clip: flip_rect(clip),
        }
    });

    // Outline, clockwise, built for a rising Deck and mirrored.
    let r = HEIGHT / 2.0;
    let (l, rt) = (left, left + width);
    let hl = rise_at(&profile, l + r);
    let hr = rise_at(&profile, rt - r);
    let p = |x: f32, y: f32| Point2::new(x, flip(y));
    let mut outline = vec![HullCommand::MoveTo(p(l, bottom - r))];
    outline.push(HullCommand::LineTo(p(l, top + r - hl)));
    outline.push(HullCommand::CubicTo {
        control1: p(l, top + r - hl - KAPPA * r),
        control2: p(l + r - KAPPA * r, top - hl),
        to: p(l + r, top - hl),
    });
    for &(sx, h) in &profile {
        if sx > l + r && sx < rt - r {
            outline.push(HullCommand::LineTo(p(sx, top - h)));
        }
    }
    outline.push(HullCommand::LineTo(p(rt - r, top - hr)));
    outline.push(HullCommand::CubicTo {
        control1: p(rt - r + KAPPA * r, top - hr),
        control2: p(rt, top + r - hr - KAPPA * r),
        to: p(rt, top + r - hr),
    });
    outline.push(HullCommand::LineTo(p(rt, bottom - r)));
    outline.push(HullCommand::CubicTo {
        control1: p(rt, bottom - r + KAPPA * r),
        control2: p(rt - r + KAPPA * r, bottom),
        to: p(rt - r, bottom),
    });
    outline.push(HullCommand::LineTo(p(l + r, bottom)));
    outline.push(HullCommand::CubicTo {
        control1: p(l + r - KAPPA * r, bottom),
        control2: p(l, bottom - r + KAPPA * r),
        to: p(l, bottom - r),
    });
    outline.push(HullCommand::Close);
    // When the flat run between the end arcs is shorter than the end rises
    // differ, the arcs alone still bound it; the profile's highest point
    // bounds the shape.
    let crest = profile.iter().map(|(_, h)| *h).fold(hl.max(hr), f32::max);
    let bounds = flip_rect(Rect::new(l, top - crest, width, HEIGHT + crest));
    DeckShape {
        base,
        cards: shape_cards,
        scales: card_scales,
        separators,
        control,
        outline,
        bounds,
        tab: tab_shape,
        gaps,
        grows_up,
    }
}

/// The wave's animated state, owned by the scene.
#[derive(Clone, Debug, Default)]
pub struct DeckWave {
    /// The pointer in rest coordinates while it is over the Deck.
    pointer: Option<f32>,
    /// The last pointer, which the wave keeps while it settles back.
    anchor: f32,
    lift: Follow,
    /// The card whose label shows, and its text.
    hovered: Option<usize>,
    label: Option<String>,
    tab_target: Option<(f32, f32)>,
    tab_center: Follow,
    tab_width: Follow,
    tab_extrusion: Follow,
    /// The action sheet's height while it is the target (DECK-01D), and
    /// the height its content keeps while it is out.
    sheet: Option<f32>,
    sheet_shown: Option<f32>,
    tuning: WaveTuning,
    last_tick: Option<Instant>,
}

impl DeckWave {
    /// The pointer moved over the Deck (`Some` rest x) or off it. Returns
    /// whether anything the shape depends on changed.
    pub fn pointer(&mut self, rest_x: Option<f32>) -> bool {
        let changed = self.pointer != rest_x;
        self.pointer = rest_x;
        match rest_x {
            Some(x) => self.anchor = x,
            None => self.sheet = None,
        }
        changed
    }

    /// The card under the pointer, and the label its tab shows.
    pub fn hover(&mut self, card: Option<(usize, String)>) -> bool {
        let next = card.as_ref().map(|(i, _)| *i);
        if next == self.hovered {
            return false;
        }
        self.hovered = next;
        // A sheet belongs to its card.
        self.sheet = None;
        if let Some((_, label)) = card {
            if self.tab_extrusion.value <= SETTLE_PX {
                // A tab that is not out yet starts where it will stand.
                self.tab_target = None;
            }
            self.label = Some(label);
        }
        true
    }

    pub fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn is_over(&self) -> bool {
        self.pointer.is_some()
    }

    /// Grow the hovered card's tab into an action sheet of this height
    /// (`Some`), or retract it to the label. Returns whether it changed.
    pub fn set_sheet(&mut self, height: Option<f32>) -> bool {
        let height = height.filter(|h| h.is_finite()).map(|h| h.max(TAB));
        if height == self.sheet {
            return false;
        }
        self.sheet = height;
        if let Some(h) = height {
            self.sheet_shown = Some(self.sheet_shown.map_or(h, |shown| shown.max(h)));
        }
        true
    }

    /// The sheet is the tab's target.
    pub fn sheet_open(&self) -> bool {
        self.sheet.is_some()
    }

    /// The sheet's content height while any of it is out.
    pub fn sheet_shown(&self) -> Option<f32> {
        self.sheet_shown
    }

    /// Where the hovered card's tab should stand (scene centre, width). The
    /// scene sets it from the shape it just built. A fresh tab starts where
    /// it stands; `snap` (reduced motion) puts every tab there at once.
    pub fn aim_tab(&mut self, center: f32, width: f32, snap: bool) {
        if snap || self.tab_target.is_none() {
            self.tab_center = Follow::at(center);
            self.tab_width = Follow::at(width);
        }
        self.tab_target = Some((center, width));
    }

    /// New values apply from the next frame; the springs keep their state.
    pub fn set_tuning(&mut self, tuning: WaveTuning) {
        self.tuning = tuning;
    }

    pub fn tuning(&self) -> WaveTuning {
        self.tuning
    }

    pub fn input(&self) -> WaveInput {
        let extrusion = self.tab_extrusion.value.max(0.0);
        let height = self.sheet_shown.unwrap_or(TAB);
        WaveInput {
            pointer: self.anchor,
            amount: (self.lift.value / AMOUNT_PX).clamp(0.0, 1.0),
            tab: (extrusion > 0.0 && self.label.is_some()).then_some(TabInput {
                center: self.tab_center.value,
                width: self.tab_width.value,
                extrusion: extrusion.min(height),
                height,
            }),
            tuning: self.tuning,
        }
    }

    fn lift_target(&self) -> f32 {
        if self.pointer.is_some() {
            AMOUNT_PX
        } else {
            0.0
        }
    }

    fn tab_extrusion_target(&self) -> f32 {
        if self.pointer.is_some() && self.hovered.is_some() {
            self.sheet.unwrap_or(TAB)
        } else {
            0.0
        }
    }

    pub fn needs_frame(&self) -> bool {
        let tab_moving = self.tab_target.is_some_and(|(c, w)| {
            !self.tab_center.is_settled(c, SETTLE_PX) || !self.tab_width.is_settled(w, SETTLE_PX)
        });
        !self.lift.is_settled(self.lift_target(), SETTLE_PX)
            || !self
                .tab_extrusion
                .is_settled(self.tab_extrusion_target(), SETTLE_PX)
            || (tab_moving && self.tab_extrusion.value > 0.0)
    }

    /// Advances every channel; true while any still moves.
    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .clamp(1.0e-4, 0.1);
        self.last_tick = Some(now);
        let mode = if reduced {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        let tempo = self.tuning.tempo;
        self.lift
            .step_at_tempo(self.lift_target(), tempo, dt, mode, SETTLE_PX);
        self.tab_extrusion
            .step_at_tempo(self.tab_extrusion_target(), tempo, dt, mode, SETTLE_PX);
        if let Some((center, width)) = self.tab_target {
            self.tab_center
                .step_at_tempo(center, tempo, dt, mode, SETTLE_PX);
            self.tab_width
                .step_at_tempo(width, tempo, dt, mode, SETTLE_PX);
        }
        if self.sheet.is_none() && self.tab_extrusion.value <= TAB + SETTLE_PX {
            self.sheet_shown = None;
        }
        if self.tab_extrusion.value <= 0.0 && self.hovered.is_none() {
            self.label = None;
            self.tab_target = None;
        }
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }
}

#[cfg(test)]
mod tests;

/// DECK-03: the far edge with the tab standing on it, as a real contour:
/// vertical walls rounded into its top by the Deck's own end radius, met
/// by concave fillets at the hump. While the tab is shorter than its
/// corner and fillet together, both shrink with it, so it extrudes in
/// height from nothing.
fn tab_profile(
    bell: &[(f32, f32)],
    tab: TabInput,
    top: f32,
    end_radius: f32,
    tuning: &WaveTuning,
) -> Vec<(f32, f32)> {
    let hump_at = |at: f32| -> f32 {
        match bell.iter().position(|(px, _)| *px >= at) {
            None => bell.last().map_or(0.0, |p| p.1),
            Some(0) => bell[0].1,
            Some(i) => {
                let (x0, h0) = bell[i - 1];
                let (x1, h1) = bell[i];
                if x1 - x0 <= f32::EPSILON {
                    h1
                } else {
                    h0 + (h1 - h0) * (at - x0) / (x1 - x0)
                }
            }
        }
    };
    let (e0, e1) = (tab.center - tab.width / 2.0, tab.center + tab.width / 2.0);
    let corner = end_radius.min(tab.width / 2.0);
    let fillet = tuning.fillet();
    // Each side's corner and fillet, shrunk to the wall they stand on.
    let radii = |edge: f32| -> (f32, f32) {
        let wall = (top - hump_at(edge)).max(0.0);
        let k = (wall / (corner + fillet)).min(1.0);
        (corner * k, fillet * k)
    };
    let ((c0, f0), (c1, f1)) = (radii(e0), radii(e1));
    let quarter = |i: usize| std::f32::consts::FRAC_PI_2 * i as f32 / ARC_STEPS as f32;
    let mut out = Vec::with_capacity(bell.len() + 4 * (ARC_STEPS + 1));
    out.extend(bell.iter().copied().filter(|(sx, _)| *sx < e0 - f0));
    // Left: fillet up the hump, wall, corner into the top.
    for i in 0..=ARC_STEPS {
        let a = quarter(i);
        let x = e0 - f0 + f0 * a.sin();
        out.push((x, hump_at(x) + f0 * (1.0 - a.cos())));
    }
    for i in 0..=ARC_STEPS {
        let a = quarter(i);
        out.push((e0 + c0 * (1.0 - a.cos()), top - c0 + c0 * a.sin()));
    }
    out.extend(
        bell.iter()
            .filter(|(sx, _)| *sx > e0 + c0 && *sx < e1 - c1)
            .map(|&(sx, h)| (sx, h.max(top))),
    );
    // Right, mirrored.
    for i in (0..=ARC_STEPS).rev() {
        let a = quarter(i);
        out.push((e1 - c1 * (1.0 - a.cos()), top - c1 + c1 * a.sin()));
    }
    for i in (0..=ARC_STEPS).rev() {
        let a = quarter(i);
        let x = e1 + f1 - f1 * a.sin();
        out.push((x, hump_at(x) + f1 * (1.0 - a.cos())));
    }
    out.extend(bell.iter().copied().filter(|(sx, _)| *sx > e1 + f1));
    out
}
