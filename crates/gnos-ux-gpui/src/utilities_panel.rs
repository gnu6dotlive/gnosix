//! SURF-01B (U01–U03): the Utilities panel, out of the Bar's status icons.
//!
//! A click on the status icons extrudes the panel from the Bar, like a
//! popout, on whichever edge the Bar stands. It holds the quick toggles (Wi-Fi,
//! Bluetooth, microphone, Do not disturb, Keep awake) and one brightness
//! slider per display. It is pure: the scene gives it the observed state
//! (`UtilitiesModel`) and turns its effects into the typed powers.
//!
//! - A toggle shows the observed state only: on, off, or unavailable (no
//!   provider). While a request is out it shows "pending"; a refusal shows
//!   its reason until the next request.
//! - A slider shows the observed level; dragging it previews, releasing it
//!   asks once.
//! - A press outside closes the panel, as does Escape.

use crate::bar_audio_popout::popout_top;
use crate::bar_popout_host::PopoutFrame;
use crate::rail_topology::RailSide;
use crate::reference_bar::{BORDER_ROUNDING, BORDER_THICKNESS};
use gnos_i18n::t;
use gnos_ux_surface::grammar::{ExtrusionMotion, GeometryRect, TemporalResponseMode, revealed};
use std::time::Instant;

pub const PANEL_WIDTH: f32 = 336.0;
const PAD: f32 = 14.0;
const HEADER: f32 = 20.0;
const GAP: f32 = 10.0;
const TOGGLE: f32 = 56.0;
const TOGGLE_HEIGHT: f32 = 66.0;
const TOGGLE_GAP: f32 = 6.0;
const ROW: f32 = 40.0;
const LABEL_WIDTH: f32 = 104.0;
/// How fast the shown height follows the content (per second; about
/// 250 ms to settle), and when it counts as arrived.
const HEIGHT_RATE: f32 = 18.0;
const HEIGHT_EPSILON: f32 = 0.25;

/// A surface of `width` x `height` extruded from the Bar beside one of its
/// modules (`anchor`: the module's centre along the Bar), `openness` of the
/// way out: against the Bar, centred on the module, kept inside the output.
/// Shared by Utilities and the OSD (beside the status icons, SURF-01) and
/// the session panel (beside the power button, SURF-02).
pub(crate) fn beside_module(
    side: RailSide,
    viewport: (f32, f32),
    anchor: f32,
    panel_width: f32,
    h: f32,
    openness: f32,
) -> Option<PopoutFrame> {
    let (width, height) = viewport;
    let rail = crate::rail_topology::rail_width();
    let rect = if side.is_horizontal() {
        let area = width - 2.0 * BORDER_THICKNESS;
        let left = (BORDER_THICKNESS + popout_top(anchor, panel_width, area, BORDER_THICKNESS))
            .clamp(
                BORDER_THICKNESS,
                (width - BORDER_THICKNESS - panel_width).max(BORDER_THICKNESS),
            );
        let top = if side == RailSide::Top {
            rail
        } else {
            height - rail - h
        };
        GeometryRect::try_new(left, top, left + panel_width, top + h, 0.0).ok()?
    } else {
        let area = height - 2.0 * BORDER_THICKNESS;
        let top = (BORDER_THICKNESS + popout_top(anchor, h, area, BORDER_THICKNESS)).clamp(
            BORDER_THICKNESS,
            (height - BORDER_THICKNESS - h).max(BORDER_THICKNESS),
        );
        let x = side.mirror_span(rail, panel_width, width);
        GeometryRect::try_new(x, top, x + panel_width, top + h, 0.0).ok()?
    };
    let rounded = GeometryRect::try_new(
        rect.left(),
        rect.top(),
        rect.right(),
        rect.bottom(),
        BORDER_ROUNDING.min(h / 2.0),
    )
    .ok()?;
    let visible = revealed(rounded, side.edge(), openness)?;
    Some(PopoutFrame {
        rect,
        visible,
        openness,
        hull: crate::bar_popout_host::popout_hull(visible, viewport, side),
        side,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToggleKind {
    Wifi,
    Bluetooth,
    Microphone,
    DoNotDisturb,
    KeepAwake,
}

impl ToggleKind {
    pub const ALL: [ToggleKind; 5] = [
        ToggleKind::Wifi,
        ToggleKind::Bluetooth,
        ToggleKind::Microphone,
        ToggleKind::DoNotDisturb,
        ToggleKind::KeepAwake,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Wifi => t("Wi-Fi"),
            Self::Bluetooth => t("Bluetooth"),
            Self::Microphone => t("Micro"),
            Self::DoNotDisturb => t("Silence"),
            Self::KeepAwake => t("Awake"),
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Wifi => "wifi",
            Self::Bluetooth => "bluetooth",
            Self::Microphone => "mic",
            Self::DoNotDisturb => "dnd",
            Self::KeepAwake => "awake",
        }
    }
}

/// One toggle as observed.
#[derive(Clone, Debug, PartialEq)]
pub struct ToggleState {
    pub kind: ToggleKind,
    /// None: no provider to act on.
    pub on: Option<bool>,
    pub pending: bool,
    /// The last refusal or failure, until the next request.
    pub note: Option<String>,
}

/// One brightness target (a display over DDC/CI, or a backlight).
#[derive(Clone, Debug, PartialEq)]
pub struct BrightnessRow {
    pub label: String,
    /// None: not read yet, or unreadable.
    pub percent: Option<u8>,
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct UtilitiesModel {
    pub toggles: Vec<ToggleState>,
    pub brightness: Vec<BrightnessRow>,
    /// Displays are being detected (DDC/CI is slow).
    pub detecting: bool,
}

/// What the scene turns into a typed power.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtilitiesEffect {
    Toggle { kind: ToggleKind, enable: bool },
    Brightness { index: usize, percent: u8 },
}

#[derive(Debug, Default, PartialEq)]
pub struct UtilitiesOutcome {
    pub consumed: bool,
    pub changed: bool,
    pub effects: Vec<UtilitiesEffect>,
}

/// One painted part, in panel coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum UtilitiesRow {
    Header {
        text: &'static str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Toggle {
        state: ToggleState,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Label {
        text: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Slider {
        index: usize,
        percent: Option<u8>,
        preview: Option<u8>,
        pending: bool,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Caption {
        text: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

impl UtilitiesRow {
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Header {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Toggle {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Label {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Slider {
                x,
                y,
                width,
                height,
                ..
            }
            | Self::Caption {
                x,
                y,
                width,
                height,
                ..
            } => (*x, *y, *width, *height),
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        let (rx, ry, rw, rh) = self.rect();
        x >= rx && x < rx + rw && y >= ry && y < ry + rh
    }
}

#[derive(Clone, Copy, Debug)]
enum Press {
    Toggle(ToggleKind),
    Slider(usize),
}

pub struct UtilitiesPanel {
    open: bool,
    motion: ExtrusionMotion,
    /// FOCUS-01D: how deep it may come out while a surface stands with it.
    give_way: Option<f32>,
    /// Centre of the status icons along the rail.
    anchor: Option<f32>,
    side: RailSide,
    viewport: (f32, f32),
    model: UtilitiesModel,
    press: Option<Press>,
    preview: Option<(usize, u8)>,
    /// The height shown: it follows the content's height (rows arrive
    /// once the displays are detected) instead of jumping to it.
    shown_height: Option<f32>,
    last_tick: Option<Instant>,
}

impl Default for UtilitiesPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl UtilitiesPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            motion: ExtrusionMotion::closed(),
            give_way: None,
            anchor: None,
            side: RailSide::Left,
            viewport: (0.0, 0.0),
            model: UtilitiesModel::default(),
            press: None,
            preview: None,
            shown_height: None,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_visible(&self) -> bool {
        self.motion.is_visible()
    }

    pub fn model(&self) -> &UtilitiesModel {
        &self.model
    }

    /// Placement inputs. Returns whether anything visible changed.
    pub fn place(&mut self, side: RailSide, viewport: (f32, f32), anchor: Option<f32>) -> bool {
        let changed = self.side != side || self.viewport != viewport || self.anchor != anchor;
        self.side = side;
        self.viewport = viewport;
        self.anchor = anchor;
        changed
    }

    /// The observed state. A preview ends once the level it asked for (or
    /// any new level) is observed.
    pub fn set_model(&mut self, model: UtilitiesModel) -> bool {
        if model == self.model {
            return false;
        }
        if let Some((index, _)) = self.preview
            && self.press.is_none()
            && model.brightness.get(index).map(|r| (r.percent, r.pending))
                != self
                    .model
                    .brightness
                    .get(index)
                    .map(|r| (r.percent, r.pending))
        {
            self.preview = None;
        }
        self.model = model;
        true
    }

    pub fn toggle_open(&mut self) -> bool {
        if self.open {
            self.close()
        } else {
            self.open = true;
            self.motion.set_open(true);
            true
        }
    }

    pub fn close(&mut self) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        self.press = None;
        self.motion.set_open(false);
        true
    }

    pub fn needs_frame(&self) -> bool {
        self.motion.is_open() != self.open
            || self.motion.needs_frame()
            || self
                .shown_height
                .is_some_and(|h| (h - self.height()).abs() > HEIGHT_EPSILON)
    }

    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |p| now.duration_since(p).as_secs_f32())
            .clamp(0.0, 0.1);
        self.last_tick = Some(now);
        self.motion.set_open(self.open);
        let mode = if reduced {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        let was_hidden = self.motion.phase() <= 0.0;
        if self.motion.step(dt, mode).is_err() {
            self.motion.snap();
        }
        let target = self.height();
        self.shown_height = Some(match self.shown_height {
            // Hidden until now, or reduced motion: the content's height.
            Some(h) if !reduced && !was_hidden => {
                let next = h + (target - h) * (1.0 - (-HEIGHT_RATE * dt).exp());
                if (next - target).abs() <= HEIGHT_EPSILON {
                    target
                } else {
                    next
                }
            }
            _ => target,
        });
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    fn height(&self) -> f32 {
        let rows = self.model.brightness.len().max(1) as f32;
        PAD + HEADER + GAP + TOGGLE_HEIGHT + GAP + HEADER + GAP / 2.0 + rows * ROW + PAD
    }

    /// The parts, laid out in panel coordinates.
    pub fn rows(&self) -> Vec<UtilitiesRow> {
        let inner = PANEL_WIDTH - 2.0 * PAD;
        let mut rows = Vec::new();
        let mut y = PAD;
        rows.push(UtilitiesRow::Header {
            text: t("Quick toggles"),
            x: PAD,
            y,
            width: inner,
            height: HEADER,
        });
        y += HEADER + GAP;
        let n = self.model.toggles.len().max(1) as f32;
        let pitch = (inner - TOGGLE * n) / (n - 1.0).max(1.0);
        let pitch = pitch.max(TOGGLE_GAP);
        for (i, state) in self.model.toggles.iter().enumerate() {
            rows.push(UtilitiesRow::Toggle {
                state: state.clone(),
                x: PAD + i as f32 * (TOGGLE + pitch),
                y,
                width: TOGGLE,
                height: TOGGLE_HEIGHT,
            });
        }
        y += TOGGLE_HEIGHT + GAP;
        rows.push(UtilitiesRow::Header {
            text: t("Brightness"),
            x: PAD,
            y,
            width: inner,
            height: HEADER,
        });
        y += HEADER + GAP / 2.0;
        if self.model.brightness.is_empty() {
            rows.push(UtilitiesRow::Caption {
                text: if self.model.detecting {
                    t("Looking for displays…").to_owned()
                } else {
                    t("No display reports its brightness").to_owned()
                },
                x: PAD,
                y,
                width: inner,
                height: ROW,
            });
        }
        for (index, row) in self.model.brightness.iter().enumerate() {
            rows.push(UtilitiesRow::Label {
                text: row.label.clone(),
                x: PAD,
                y,
                width: LABEL_WIDTH,
                height: ROW,
            });
            rows.push(UtilitiesRow::Slider {
                index,
                percent: row.percent,
                preview: self.preview.filter(|(i, _)| *i == index).map(|(_, p)| p),
                pending: row.pending,
                x: PAD + LABEL_WIDTH + 8.0,
                y,
                width: inner - LABEL_WIDTH - 8.0,
                height: ROW,
            });
            y += ROW;
        }
        rows
    }

    pub fn frame(&self) -> Option<PopoutFrame> {
        let anchor = self.anchor?;
        let h = self.shown_height.unwrap_or_else(|| self.height());
        // FOCUS-01D: it stands back into the Bar by what a surface standing
        // with it occupies of its depth (make room).
        let depth = if self.side.is_horizontal() {
            h
        } else {
            PANEL_WIDTH
        };
        let openness =
            gnos_ux_surface::grammar::within_depth(self.motion.phase(), depth, self.give_way);
        if openness <= 0.0 {
            return None;
        }
        beside_module(self.side, self.viewport, anchor, PANEL_WIDTH, h, openness)
    }

    /// FOCUS-01D: the body it has fully out, and the edge it comes out of.
    pub fn body(
        &self,
    ) -> Option<(
        gnos_ux_surface::grammar::GeometryRect,
        gnos_ux_surface::envelope::SurfaceEdge,
    )> {
        let anchor = self.anchor?;
        let h = self.shown_height.unwrap_or_else(|| self.height());
        let full = beside_module(self.side, self.viewport, anchor, PANEL_WIDTH, h, 1.0)?;
        Some((full.rect, self.side.edge()))
    }

    /// FOCUS-01D: how deep it may come out (`None`: fully). Returns whether
    /// that changed.
    pub fn set_give_way(&mut self, limit: Option<f32>) -> bool {
        let changed = self.give_way != limit;
        self.give_way = limit;
        changed
    }

    /// CURSOR-01: what the pointer is over on the panel (`None`: off it): a
    /// switch or a slider it can press.
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        let (lx, ly) = self.local(x, y)?;
        let on = self.rows().into_iter().find(|r| r.contains(lx, ly));
        Some(match on {
            Some(UtilitiesRow::Toggle { state, .. }) if state.on.is_none() => {
                crate::cursor::Affordance::Disabled
            }
            Some(UtilitiesRow::Toggle { .. } | UtilitiesRow::Slider { .. }) => {
                crate::cursor::Affordance::Press
            }
            _ => crate::cursor::Affordance::Inert,
        })
    }

    /// CURSOR-01: a request out (a switch, a brightness).
    pub fn busy(&self) -> bool {
        self.model.toggles.iter().any(|t| t.pending)
            || self.model.brightness.iter().any(|b| b.pending)
    }

    fn local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let frame = self.frame()?;
        let v = frame.visible;
        (x >= v.left() && x < v.right() && y >= v.top() && y < v.bottom())
            .then(|| (x - frame.rect.left(), y - frame.rect.top()))
    }

    fn slider_percent(&self, index: usize, lx: f32) -> Option<u8> {
        self.rows().into_iter().find_map(|r| match r {
            UtilitiesRow::Slider {
                index: i, x, width, ..
            } if i == index => Some((((lx - x) / width).clamp(0.0, 1.0) * 100.0).round() as u8),
            _ => None,
        })
    }

    /// A primary press. Outside the panel (and not on the status icons,
    /// which the scene routes first) it closes it.
    pub fn pointer_down(&mut self, x: f32, y: f32) -> UtilitiesOutcome {
        let mut outcome = UtilitiesOutcome::default();
        if !self.open {
            return outcome;
        }
        let Some((lx, ly)) = self.local(x, y) else {
            outcome.changed = self.close();
            return outcome;
        };
        outcome.consumed = true;
        for row in self.rows() {
            if !row.contains(lx, ly) {
                continue;
            }
            match row {
                UtilitiesRow::Toggle { state, .. } => self.press = Some(Press::Toggle(state.kind)),
                UtilitiesRow::Slider { index, .. } => {
                    self.press = Some(Press::Slider(index));
                    if let Some(p) = self.slider_percent(index, lx) {
                        self.preview = Some((index, p));
                        outcome.changed = true;
                    }
                }
                _ => {}
            }
        }
        outcome
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> UtilitiesOutcome {
        let mut outcome = UtilitiesOutcome::default();
        if let Some(Press::Slider(index)) = self.press
            && let Some(frame) = self.frame()
            && let Some(p) = self.slider_percent(index, x - frame.rect.left())
        {
            outcome.consumed = true;
            if self.preview != Some((index, p)) {
                self.preview = Some((index, p));
                outcome.changed = true;
            }
        } else if self.local(x, y).is_some() {
            outcome.consumed = true;
        }
        let _ = y;
        outcome
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> UtilitiesOutcome {
        let mut outcome = UtilitiesOutcome::default();
        let Some(press) = self.press.take() else {
            outcome.consumed = self.local(x, y).is_some();
            return outcome;
        };
        outcome.consumed = true;
        match press {
            Press::Toggle(kind) => {
                let here = self
                    .local(x, y)
                    .and_then(|(lx, ly)| self.rows().into_iter().find(|r| r.contains(lx, ly)));
                if let Some(UtilitiesRow::Toggle { state, .. }) = here
                    && state.kind == kind
                    && !state.pending
                    && let Some(on) = state.on
                {
                    outcome
                        .effects
                        .push(UtilitiesEffect::Toggle { kind, enable: !on });
                }
            }
            Press::Slider(index) => {
                if let Some((i, percent)) = self.preview
                    && i == index
                {
                    let observed = self.model.brightness.get(index).and_then(|r| r.percent);
                    if observed != Some(percent) {
                        outcome
                            .effects
                            .push(UtilitiesEffect::Brightness { index, percent });
                    } else {
                        self.preview = None;
                    }
                    outcome.changed = true;
                }
            }
        }
        outcome
    }

    pub fn escape(&mut self) -> bool {
        self.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(brightness: usize) -> UtilitiesModel {
        UtilitiesModel {
            toggles: ToggleKind::ALL
                .iter()
                .map(|&kind| ToggleState {
                    kind,
                    on: if kind == ToggleKind::Bluetooth {
                        None
                    } else {
                        Some(true)
                    },
                    pending: kind == ToggleKind::KeepAwake,
                    note: None,
                })
                .collect(),
            brightness: (0..brightness)
                .map(|i| BrightnessRow {
                    label: format!("Display {i}"),
                    percent: Some(80),
                    pending: false,
                })
                .collect(),
            detecting: false,
        }
    }

    fn open_panel(side: RailSide, brightness: usize) -> UtilitiesPanel {
        let mut p = UtilitiesPanel::new();
        p.place(side, (1280.0, 800.0), Some(700.0));
        p.set_model(model(brightness));
        p.toggle_open();
        p.tick(Instant::now(), true);
        p
    }

    fn centre(p: &UtilitiesPanel, pick: impl Fn(&UtilitiesRow) -> bool) -> (f32, f32) {
        let f = p.frame().unwrap();
        let r = p.rows().into_iter().find(|r| pick(r)).unwrap();
        let (x, y, w, h) = r.rect();
        (f.rect.left() + x + w / 2.0, f.rect.top() + y + h / 2.0)
    }

    #[test]
    fn it_extrudes_from_the_rail_beside_the_status_icons_on_any_side() {
        for side in [
            RailSide::Left,
            RailSide::Right,
            RailSide::Top,
            RailSide::Bottom,
        ] {
            let p = open_panel(side, 2);
            let f = p.frame().unwrap();
            assert_eq!(f.openness, 1.0);
            assert!(f.hull.is_some(), "{side:?}: joined to the rail");
            let rail = crate::rail_topology::rail_width();
            match side {
                RailSide::Left => assert_eq!(f.rect.left(), rail),
                RailSide::Right => assert_eq!(f.rect.right(), 1280.0 - rail),
                RailSide::Top => assert_eq!(f.rect.top(), rail),
                RailSide::Bottom => assert_eq!(f.rect.bottom(), 800.0 - rail),
            }
            // Inside the output.
            assert!(f.rect.left() >= 0.0 && f.rect.right() <= 1280.0);
            assert!(f.rect.top() >= 0.0 && f.rect.bottom() <= 800.0);
        }
    }

    #[test]
    fn the_panel_grows_to_rows_that_arrive_without_a_jump() {
        let mut p = UtilitiesPanel::new();
        p.place(RailSide::Left, (1280.0, 800.0), Some(700.0));
        p.set_model(model(0));
        p.toggle_open();
        let mut now = Instant::now();
        for _ in 0..120 {
            now += std::time::Duration::from_millis(16);
            p.tick(now, false);
        }
        let before = p.frame().unwrap().rect.height();
        // The displays are detected: three slider rows arrive.
        p.set_model(model(3));
        let target = p.height();
        assert_eq!(target, before + 2.0 * ROW);
        let mut last = before;
        let mut frames = 0;
        while p.needs_frame() {
            now += std::time::Duration::from_millis(16);
            p.tick(now, false);
            let h = p.frame().unwrap().rect.height();
            assert!(
                h >= last && h - last < 0.3 * (target - before),
                "step {last} -> {h}"
            );
            last = h;
            frames += 1;
            assert!(frames < 60, "settles within a second");
        }
        assert_eq!(last, target);
        // Reduced motion: at once.
        p.set_model(model(1));
        p.tick(now + std::time::Duration::from_millis(16), true);
        assert_eq!(p.frame().unwrap().rect.height(), p.height());
    }

    #[test]
    fn toggles_ask_for_the_other_state_and_never_when_pending_or_unavailable() {
        let mut p = open_panel(RailSide::Left, 1);
        let at = |kind| move |r: &UtilitiesRow| matches!(r, UtilitiesRow::Toggle { state, .. } if state.kind == kind);
        let (x, y) = centre(&p, at(ToggleKind::Wifi));
        assert!(p.pointer_down(x, y).consumed);
        let out = p.pointer_up(x, y);
        assert_eq!(
            out.effects,
            vec![UtilitiesEffect::Toggle {
                kind: ToggleKind::Wifi,
                enable: false
            }]
        );
        for kind in [ToggleKind::Bluetooth, ToggleKind::KeepAwake] {
            let (x, y) = centre(&p, at(kind));
            p.pointer_down(x, y);
            assert!(p.pointer_up(x, y).effects.is_empty(), "{kind:?}");
        }
        // A press that leaves the toggle before release asks nothing.
        let (x, y) = centre(&p, at(ToggleKind::Microphone));
        p.pointer_down(x, y);
        assert!(p.pointer_up(x + 200.0, y + 200.0).effects.is_empty());
    }

    #[test]
    fn a_slider_previews_while_dragged_and_asks_once_on_release() {
        let mut p = open_panel(RailSide::Left, 2);
        let pick = |r: &UtilitiesRow| matches!(r, UtilitiesRow::Slider { index: 1, .. });
        let (x, y) = centre(&p, pick);
        p.pointer_down(x, y);
        let f = p.frame().unwrap();
        let slider = p.rows().into_iter().find(|r| pick(r)).unwrap();
        let (sx, _, sw, _) = slider.rect();
        let target = f.rect.left() + sx + sw * 0.3;
        assert!(p.pointer_moved(target, y).changed);
        let preview = p.rows().into_iter().find_map(|r| match r {
            UtilitiesRow::Slider {
                index: 1, preview, ..
            } => preview,
            _ => None,
        });
        assert_eq!(preview, Some(30));
        let out = p.pointer_up(target, y);
        assert_eq!(
            out.effects,
            vec![UtilitiesEffect::Brightness {
                index: 1,
                percent: 30
            }]
        );
        // The observed level ends the preview.
        let mut m = model(2);
        m.brightness[1].percent = Some(30);
        p.set_model(m);
        assert!(p.rows().iter().all(|r| !matches!(
            r,
            UtilitiesRow::Slider {
                preview: Some(_),
                ..
            }
        )));
    }

    #[test]
    fn a_press_outside_or_escape_closes_it() {
        let mut p = open_panel(RailSide::Left, 1);
        let out = p.pointer_down(1200.0, 100.0);
        assert!(!out.consumed && out.changed);
        assert!(!p.is_open());
        let mut p = open_panel(RailSide::Left, 1);
        assert!(p.escape());
        assert!(!p.is_open());
        assert!(
            p.rows()
                .iter()
                .any(|r| matches!(r, UtilitiesRow::Slider { .. }))
        );
        let empty = open_panel(RailSide::Left, 0);
        assert!(
            empty
                .rows()
                .iter()
                .any(|r| matches!(r, UtilitiesRow::Caption { .. }))
        );
    }
}
