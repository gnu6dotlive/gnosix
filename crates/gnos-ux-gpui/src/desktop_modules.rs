//! SURF-04 (W01–W03): the desktop modules' places, pure.
//!
//! Two modules stand on the desktop (the background layer, under the
//! windows): the clock and the audio visualiser. Each has a place, kept as
//! the fraction of the output where its top-left corner stands, so it
//! survives a resolution change; it is always clamped wholly inside the
//! output. A press on a module and a move of more than `DRAG_SLOP` drag it;
//! the release reports its new place once (the scene writes it to the
//! profile). A press that does not move is no drag.

use std::collections::BTreeMap;

pub const CLOCK_SIZE: (f32, f32) = (380.0, 150.0);
pub const VISUALISER_HEIGHT: f32 = 120.0;
/// The dictation element's own size: the mic patch, without the surface it
/// grows into.
pub const DICTATION_SIZE: (f32, f32) = (32.0, 32.0);
const VISUALISER_MAX_WIDTH: f32 = 720.0;
const DRAG_SLOP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleKind {
    Clock,
    Visualiser,
    /// The microphone: what it is doing, and what it is hearing.
    Dictation,
}

impl ModuleKind {
    pub const ALL: [ModuleKind; 3] = [
        ModuleKind::Clock,
        ModuleKind::Visualiser,
        ModuleKind::Dictation,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Clock => "clock",
            Self::Visualiser => "visualiser",
            Self::Dictation => "dictation",
        }
    }
}

/// Where a module's top-left stands, as fractions of the output (0..=1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModulePlace {
    pub x: f32,
    pub y: f32,
}

impl ModulePlace {
    pub fn clamped(self) -> Self {
        Self {
            x: self.x.clamp(0.0, 1.0),
            y: self.y.clamp(0.0, 1.0),
        }
    }
}

/// The default places: the clock low on the left, the visualiser along the
/// bottom, centred.
pub fn default_place(kind: ModuleKind, viewport: (f32, f32)) -> ModulePlace {
    match kind {
        ModuleKind::Clock => ModulePlace { x: 0.06, y: 0.62 },
        ModuleKind::Dictation => ModulePlace { x: 0.04, y: 0.86 },
        ModuleKind::Visualiser => {
            let (w, _) = module_size(kind, viewport);
            ModulePlace {
                x: if viewport.0 > 0.0 {
                    ((viewport.0 - w) / 2.0 / viewport.0).max(0.0)
                } else {
                    0.0
                },
                y: 0.80,
            }
        }
    }
}

pub fn module_size(kind: ModuleKind, viewport: (f32, f32)) -> (f32, f32) {
    // SET-02D: the operator's sizes, kept inside the output.
    #[cfg(feature = "component-lab")]
    let (clock_pct, visualiser_pct) = {
        let prefs = crate::surface_prefs::current();
        (prefs.clock_scale_pct, prefs.visualiser_scale_pct)
    };
    #[cfg(not(feature = "component-lab"))]
    let (clock_pct, visualiser_pct) = (100, 100);
    let (w, h) = match kind {
        ModuleKind::Clock => {
            let s = clock_pct as f32 / 100.0;
            (CLOCK_SIZE.0 * s, CLOCK_SIZE.1 * s)
        }
        ModuleKind::Dictation => (DICTATION_SIZE.0, DICTATION_SIZE.1),
        ModuleKind::Visualiser => {
            let s = visualiser_pct as f32 / 100.0;
            (
                ((viewport.0 * 0.5).clamp(200.0, VISUALISER_MAX_WIDTH) * s),
                VISUALISER_HEIGHT * s,
            )
        }
    };
    (w.min(viewport.0.max(1.0)), h.min(viewport.1.max(1.0)))
}

/// A module's rectangle in output pixels: its place, clamped inside.
pub fn module_rect(
    kind: ModuleKind,
    place: ModulePlace,
    viewport: (f32, f32),
) -> (f32, f32, f32, f32) {
    let (w, h) = module_size(kind, viewport);
    let x = (place.x * viewport.0).clamp(0.0, (viewport.0 - w).max(0.0));
    let y = (place.y * viewport.1).clamp(0.0, (viewport.1 - h).max(0.0));
    (x, y, w, h)
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    kind: ModuleKind,
    /// Pointer minus the module's top-left at the press.
    grab: (f32, f32),
    start: (f32, f32),
    moving: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopOutcome {
    pub consumed: bool,
    pub changed: bool,
    /// A module was dropped at a new place.
    pub placed: Option<(ModuleKind, ModulePlace)>,
}

#[derive(Clone, Debug, Default)]
pub struct DesktopModules {
    viewport: (f32, f32),
    /// Places the profile holds (or a drop made); others are default.
    places: BTreeMap<ModuleKind, ModulePlace>,
    /// A drop's place, shown until the profile reflects it.
    dropped: BTreeMap<ModuleKind, ModulePlace>,
    drag: Option<Drag>,
    dragging_place: Option<ModulePlace>,
}

impl DesktopModules {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_viewport(&mut self, viewport: (f32, f32)) -> bool {
        let changed = self.viewport != viewport;
        self.viewport = viewport;
        changed
    }

    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    /// The profile's places. A drop shows until the profile carries it.
    pub fn set_places(&mut self, places: BTreeMap<ModuleKind, ModulePlace>) -> bool {
        self.dropped
            .retain(|kind, place| places.get(kind) != Some(place));
        let changed = self.places != places;
        self.places = places;
        changed
    }

    /// Forget a drop the profile did not take (refused or timed out).
    pub fn revert_drop(&mut self, kind: ModuleKind) -> bool {
        self.dropped.remove(&kind).is_some()
    }

    pub fn place(&self, kind: ModuleKind) -> ModulePlace {
        if let Some(drag) = self.drag
            && drag.kind == kind
            && let Some(place) = self.dragging_place
        {
            return place;
        }
        self.dropped
            .get(&kind)
            .or_else(|| self.places.get(&kind))
            .copied()
            .unwrap_or_else(|| default_place(kind, self.viewport))
    }

    pub fn rect(&self, kind: ModuleKind) -> (f32, f32, f32, f32) {
        module_rect(kind, self.place(kind), self.viewport)
    }

    pub fn dragging(&self) -> Option<ModuleKind> {
        self.drag.filter(|d| d.moving).map(|d| d.kind)
    }

    /// The input region: the modules only.
    pub fn input_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        if self.drag.is_some() {
            // While a drag is under way the whole output takes the pointer.
            return vec![(0.0, 0.0, self.viewport.0, self.viewport.1)];
        }
        ModuleKind::ALL.iter().map(|k| self.rect(*k)).collect()
    }

    fn at(&self, x: f32, y: f32) -> Option<ModuleKind> {
        // The visualiser is drawn last, so it is the one on top.
        ModuleKind::ALL.iter().rev().copied().find(|k| {
            let (rx, ry, rw, rh) = self.rect(*k);
            x >= rx && x < rx + rw && y >= ry && y < ry + rh
        })
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) -> DesktopOutcome {
        let mut out = DesktopOutcome::default();
        if let Some(kind) = self.at(x, y) {
            let (rx, ry, _, _) = self.rect(kind);
            self.drag = Some(Drag {
                kind,
                grab: (x - rx, y - ry),
                start: (x, y),
                moving: false,
            });
            out.consumed = true;
        }
        out
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> DesktopOutcome {
        let mut out = DesktopOutcome::default();
        let Some(mut drag) = self.drag else {
            return out;
        };
        out.consumed = true;
        if !drag.moving && (x - drag.start.0).hypot(y - drag.start.1) < DRAG_SLOP {
            return out;
        }
        drag.moving = true;
        self.drag = Some(drag);
        let (w, h) = module_size(drag.kind, self.viewport);
        let (vw, vh) = self.viewport;
        let left = (x - drag.grab.0).clamp(0.0, (vw - w).max(0.0));
        let top = (y - drag.grab.1).clamp(0.0, (vh - h).max(0.0));
        let place = ModulePlace {
            x: if vw > 0.0 { left / vw } else { 0.0 },
            y: if vh > 0.0 { top / vh } else { 0.0 },
        };
        if self.dragging_place != Some(place) {
            self.dragging_place = Some(place);
            out.changed = true;
        }
        out
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> DesktopOutcome {
        let mut out = self.pointer_moved(x, y);
        let Some(drag) = self.drag.take() else {
            return out;
        };
        out.consumed = true;
        if drag.moving
            && let Some(place) = self.dragging_place.take()
        {
            self.dropped.insert(drag.kind, place);
            out.placed = Some((drag.kind, place));
            out.changed = true;
        }
        self.dragging_place = None;
        out
    }

    /// Escape or the pointer leaving: a drag under way goes back.
    pub fn cancel(&mut self) -> bool {
        let was = self.drag.take().is_some_and(|d| d.moving);
        self.dragging_place = None;
        was
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: (f32, f32) = (1920.0, 1080.0);

    #[test]
    fn modules_stand_inside_the_output_at_their_places() {
        let mut d = DesktopModules::new();
        d.set_viewport(VIEW);
        for kind in ModuleKind::ALL {
            let (x, y, w, h) = d.rect(kind);
            assert!(
                x >= 0.0 && y >= 0.0 && x + w <= VIEW.0 && y + h <= VIEW.1,
                "{kind:?}"
            );
        }
        // A place beyond the edge is clamped inside.
        let rect = module_rect(ModuleKind::Clock, ModulePlace { x: 0.99, y: 0.99 }, VIEW);
        assert_eq!(
            rect,
            (
                VIEW.0 - CLOCK_SIZE.0,
                VIEW.1 - CLOCK_SIZE.1,
                CLOCK_SIZE.0,
                CLOCK_SIZE.1
            )
        );
        // Fractions survive a resolution change.
        let place = ModulePlace { x: 0.25, y: 0.5 };
        assert_eq!(
            module_rect(ModuleKind::Clock, place, (1000.0, 800.0)).0,
            250.0
        );
        assert_eq!(
            module_rect(ModuleKind::Clock, place, (2000.0, 1600.0)).0,
            500.0
        );
    }

    #[test]
    fn a_drag_moves_a_module_and_reports_its_place_once() {
        let mut d = DesktopModules::new();
        d.set_viewport(VIEW);
        let (x, y, _, _) = d.rect(ModuleKind::Clock);
        assert!(d.pointer_down(x + 10.0, y + 10.0).consumed);
        // A click without a move is no drag.
        let up = d.pointer_up(x + 11.0, y + 10.0);
        assert!(up.placed.is_none());
        assert!(d.pointer_down(x + 10.0, y + 10.0).consumed);
        assert!(d.pointer_moved(x + 210.0, y - 90.0).changed);
        assert_eq!(d.dragging(), Some(ModuleKind::Clock));
        assert_eq!(d.input_rects(), vec![(0.0, 0.0, VIEW.0, VIEW.1)]);
        let up = d.pointer_up(x + 410.0, y - 190.0);
        let (kind, place) = up.placed.expect("dropped");
        assert_eq!(kind, ModuleKind::Clock);
        assert_eq!(d.rect(ModuleKind::Clock).0, x + 400.0);
        assert!((place.x - (x + 400.0) / VIEW.0).abs() < 1e-4);
        // Shown until the profile carries it, then the profile's.
        let mut places = BTreeMap::new();
        places.insert(ModuleKind::Clock, place);
        d.set_places(places);
        assert_eq!(d.place(ModuleKind::Clock), place);
    }

    #[test]
    fn a_refused_drop_goes_back_and_a_cancel_drops_nothing() {
        let mut d = DesktopModules::new();
        d.set_viewport(VIEW);
        let before = d.rect(ModuleKind::Visualiser);
        d.pointer_down(before.0 + 5.0, before.1 + 5.0);
        d.pointer_moved(before.0 + 105.0, before.1 - 200.0);
        d.pointer_up(before.0 + 105.0, before.1 - 200.0);
        assert_ne!(d.rect(ModuleKind::Visualiser), before);
        assert!(d.revert_drop(ModuleKind::Visualiser));
        assert_eq!(d.rect(ModuleKind::Visualiser), before);
        d.pointer_down(before.0 + 5.0, before.1 + 5.0);
        d.pointer_moved(before.0 + 305.0, before.1 - 300.0);
        assert!(d.cancel());
        assert_eq!(d.rect(ModuleKind::Visualiser), before);
    }
}
