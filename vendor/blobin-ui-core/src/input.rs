//! UI-G04: headless input routing and focused event consumption.
use crate::layout::{LayoutSnapshot, LayoutStyles, Viewport};
use crate::{NodeId, Role, SurfaceId, UiSnapshot};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Logical,
    Physical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Tab,
    Enter,
    Space,
    Escape,
    Other(u32),
}
/// Wheel/touchpad scroll amount in logical pixels, Wayland `wl_pointer.axis`
/// sign convention: positive `y` scrolls toward the bottom of the content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollDelta {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollSource {
    Wheel,
    Finger,
    Continuous,
}
#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    PointerDown {
        pointer: u64,
        button: u8,
        position: Point,
        units: Units,
    },
    PointerMove {
        pointer: u64,
        position: Point,
        units: Units,
    },
    PointerUp {
        pointer: u64,
        button: u8,
        position: Point,
        units: Units,
    },
    PointerCancel {
        pointer: u64,
    },
    /// Routed like a move (capture first, then hit test); never focuses or activates.
    Scroll {
        pointer: u64,
        position: Point,
        units: Units,
        delta: ScrollDelta,
        source: ScrollSource,
    },
    /// The pointer left the surface: hover ends, captures are untouched.
    PointerLeave {
        pointer: u64,
    },
    KeyDown {
        key: Key,
        shift: bool,
        repeat: bool,
    },
    KeyUp {
        key: Key,
    },
    WindowBlur,
}
#[derive(Clone, Debug, PartialEq)]
pub struct InputEvent {
    pub surface: SurfaceId,
    pub surface_epoch: u64,
    pub geometry_epoch: u64,
    pub scale_factor_bits: u32,
    pub serial: u64,
    pub timestamp_micros: u64,
    pub kind: EventKind,
}
impl InputEvent {
    pub fn for_layout(
        layout: &LayoutSnapshot,
        serial: u64,
        timestamp_micros: u64,
        kind: EventKind,
    ) -> Self {
        Self {
            surface: layout.stamp.surface,
            surface_epoch: layout.stamp.surface_epoch,
            geometry_epoch: layout.stamp.geometry_epoch,
            scale_factor_bits: layout.viewport.scale_factor.to_bits(),
            serial,
            timestamp_micros,
            kind,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    StaleLayout,
    StaleEvent,
    NonMonotonicSerial,
    NonMonotonicTimestamp,
    InvalidPosition,
    InvalidScroll,
}
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for InputError {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRoute {
    pub target: NodeId,
    pub capture_path: Vec<NodeId>,
    pub bubble_path: Vec<NodeId>,
}
impl EventRoute {
    fn build(tree: &UiSnapshot, target: NodeId) -> Option<Self> {
        let mut parents = Vec::new();
        let mut cur = tree.get(target)?.parent();
        while let Some(parent) = cur {
            parents.push(parent);
            cur = tree.get(parent)?.parent();
        }
        let bubble_path = parents.clone();
        parents.reverse();
        Some(Self {
            target,
            capture_path: parents,
            bubble_path,
        })
    }
}

/// Emitted only when the node under a pointer changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoverTransition {
    pub pointer: u64,
    pub from: Option<NodeId>,
    pub to: Option<NodeId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputReceipt {
    pub surface: SurfaceId,
    pub serial: u64,
    pub route: Option<EventRoute>,
    pub focus: Option<NodeId>,
    pub activated: Option<NodeId>,
    pub pointer_capture: Option<NodeId>,
    pub hover: Option<HoverTransition>,
}
#[derive(Clone, Copy)]
struct PointerCapture {
    target: NodeId,
    button: u8,
}
#[derive(Default)]
struct SurfaceInput {
    epoch: u64,
    last_serial: u64,
    last_timestamp: u64,
    focus: Option<NodeId>,
    captures: BTreeMap<u64, PointerCapture>,
    hovers: BTreeMap<u64, NodeId>,
    keys: BTreeSet<Key>,
}
#[derive(Default)]
pub struct InputRouter {
    surfaces: BTreeMap<SurfaceId, SurfaceInput>,
}
impl InputRouter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn focus(&self, surface: SurfaceId) -> Option<NodeId> {
        self.surfaces.get(&surface).and_then(|s| s.focus)
    }
    pub fn capture(&self, surface: SurfaceId, pointer: u64) -> Option<NodeId> {
        self.surfaces
            .get(&surface)?
            .captures
            .get(&pointer)
            .map(|c| c.target)
    }
    /// Node geometrically under the pointer, independent of any capture.
    pub fn hover(&self, surface: SurfaceId, pointer: u64) -> Option<NodeId> {
        self.surfaces.get(&surface)?.hovers.get(&pointer).copied()
    }
    pub fn close_surface(&mut self, surface: SurfaceId) {
        self.surfaces.remove(&surface);
    }
    /// Topmost enabled Button/Custom node within ancestor clips.
    /// Hit testing is fail-closed on stale layout revisions and DPI/resize epochs.
    pub fn hit_test(
        tree: &UiSnapshot,
        styles: &LayoutStyles,
        layout: &LayoutSnapshot,
        viewport: Viewport,
        point: Point,
    ) -> Result<Option<EventRoute>, InputError> {
        if !layout.is_current(tree, styles, viewport) {
            return Err(InputError::StaleLayout);
        }
        Ok(Self::hit_test_unchecked(tree, layout, point))
    }

    fn hit_test_unchecked(
        tree: &UiSnapshot,
        layout: &LayoutSnapshot,
        point: Point,
    ) -> Option<EventRoute> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return None;
        }
        for id in layout.paint_order.iter().rev() {
            let Some(node) = tree.get(*id) else {
                continue;
            };
            if node.surface() != layout.stamp.surface
                || !interactive(&node.properties().role)
                || !node.properties().enabled
            {
                continue;
            }
            if layout
                .get(*id)
                .is_some_and(|b| b.visible.contains(point.x, point.y))
            {
                return EventRoute::build(tree, *id);
            }
        }
        None
    }
    /// Reject stale/duplicated events; one committed receipt per serial.
    pub fn dispatch(
        &mut self,
        tree: &UiSnapshot,
        styles: &LayoutStyles,
        layout: &LayoutSnapshot,
        viewport: Viewport,
        event: InputEvent,
    ) -> Result<InputReceipt, InputError> {
        if !layout.is_current(tree, styles, viewport) {
            return Err(InputError::StaleLayout);
        }
        if event.surface != layout.stamp.surface
            || event.surface_epoch != layout.stamp.surface_epoch
            || event.geometry_epoch != layout.stamp.geometry_epoch
            || event.scale_factor_bits != viewport.scale_factor.to_bits()
        {
            return Err(InputError::StaleEvent);
        }
        let pos = match &event.kind {
            EventKind::PointerDown {
                position, units, ..
            }
            | EventKind::PointerMove {
                position, units, ..
            }
            | EventKind::PointerUp {
                position, units, ..
            }
            | EventKind::Scroll {
                position, units, ..
            } => {
                let p = match units {
                    Units::Logical => *position,
                    Units::Physical => Point::new(
                        position.x / viewport.scale_factor,
                        position.y / viewport.scale_factor,
                    ),
                };
                if !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x.abs() > 1_000_000.0
                    || p.y.abs() > 1_000_000.0
                {
                    return Err(InputError::InvalidPosition);
                }
                Some(p)
            }
            _ => None,
        };
        if let EventKind::Scroll { delta, .. } = &event.kind {
            if !delta.x.is_finite()
                || !delta.y.is_finite()
                || delta.x.abs() > 1_000_000.0
                || delta.y.abs() > 1_000_000.0
            {
                return Err(InputError::InvalidScroll);
            }
        }

        let state = self.surfaces.entry(event.surface).or_default();
        if state.epoch > event.surface_epoch {
            return Err(InputError::StaleEvent);
        }
        if state.epoch != event.surface_epoch {
            *state = SurfaceInput {
                epoch: event.surface_epoch,
                ..SurfaceInput::default()
            };
        }
        if event.serial <= state.last_serial {
            return Err(InputError::NonMonotonicSerial);
        }
        if event.timestamp_micros < state.last_timestamp {
            return Err(InputError::NonMonotonicTimestamp);
        }
        let valid = |id: NodeId| {
            tree.get(id).is_some_and(|n| {
                n.surface() == event.surface
                    && n.properties().enabled
                    && interactive(&n.properties().role)
                    && layout.get(id).is_some()
            })
        };
        state.focus = state.focus.filter(|id| valid(*id));
        state.captures.retain(|_, c| valid(c.target));
        state.hovers.retain(|_, id| valid(*id));
        let mut route = None;
        let mut activated = None;
        let mut pointer_capture = None;
        match event.kind {
            EventKind::PointerDown {
                pointer, button, ..
            } => {
                if let Some(held) = state.captures.get(&pointer) {
                    route = EventRoute::build(tree, held.target);
                } else {
                    route = pos.and_then(|p| Self::hit_test_unchecked(tree, layout, p));
                    if let Some(ref hit) = route {
                        state.captures.insert(
                            pointer,
                            PointerCapture {
                                target: hit.target,
                                button,
                            },
                        );
                        state.focus = Some(hit.target);
                    } else {
                        state.focus = None;
                    }
                }
                pointer_capture = state.captures.get(&pointer).map(|c| c.target);
            }
            EventKind::PointerMove { pointer, .. } => {
                let captured = state.captures.get(&pointer).map(|c| c.target);
                route = if let Some(id) = captured {
                    EventRoute::build(tree, id)
                } else {
                    pos.and_then(|p| Self::hit_test_unchecked(tree, layout, p))
                };
                pointer_capture = captured;
            }
            EventKind::PointerUp {
                pointer, button, ..
            } => {
                if let Some(held) = state.captures.get(&pointer).copied() {
                    route = EventRoute::build(tree, held.target);
                    if held.button == button {
                        state.captures.remove(&pointer);
                        if let Some(p) = pos {
                            if Self::hit_test_unchecked(tree, layout, p)
                                .is_some_and(|h| h.target == held.target)
                            {
                                activated = Some(held.target);
                            }
                        }
                    } else {
                        pointer_capture = Some(held.target);
                    }
                } else {
                    route = pos.and_then(|p| Self::hit_test_unchecked(tree, layout, p));
                }
            }
            EventKind::PointerCancel { pointer } => {
                if let Some(held) = state.captures.remove(&pointer) {
                    route = EventRoute::build(tree, held.target);
                }
            }
            EventKind::Scroll { pointer, .. } => {
                let captured = state.captures.get(&pointer).map(|c| c.target);
                route = if let Some(id) = captured {
                    EventRoute::build(tree, id)
                } else {
                    pos.and_then(|p| Self::hit_test_unchecked(tree, layout, p))
                };
                pointer_capture = captured;
            }
            EventKind::PointerLeave { .. } => {}

            EventKind::KeyDown { key, shift, repeat } => {
                let fresh = state.keys.insert(key);
                match key {
                    Key::Tab if fresh && !repeat => {
                        let eligible: Vec<NodeId> = layout
                            .paint_order
                            .iter()
                            .copied()
                            .filter(|id| valid(*id))
                            .collect();
                        if !eligible.is_empty() {
                            let index = state
                                .focus
                                .and_then(|id| eligible.iter().position(|v| *v == id));
                            let next = match (index, shift) {
                                (Some(0), true) | (None, true) => eligible.len() - 1,
                                (Some(i), true) => i - 1,
                                (Some(i), false) => (i + 1) % eligible.len(),
                                (None, false) => 0,
                            };
                            state.focus = Some(eligible[next]);
                        }
                    }
                    Key::Enter | Key::Space if fresh && !repeat => {
                        activated = state.focus.filter(|id| valid(*id));
                    }
                    Key::Escape => state.captures.clear(),
                    _ => {}
                }
                route = state.focus.and_then(|id| EventRoute::build(tree, id));
            }
            EventKind::KeyUp { key } => {
                state.keys.remove(&key);
                route = state.focus.and_then(|id| EventRoute::build(tree, id));
            }
            EventKind::WindowBlur => {
                state.focus = None;
                state.keys.clear();
                state.captures.clear();
            }
        }
        let hover_pointer = match event.kind {
            EventKind::PointerDown { pointer, .. }
            | EventKind::PointerMove { pointer, .. }
            | EventKind::PointerUp { pointer, .. }
            | EventKind::Scroll { pointer, .. }
            | EventKind::PointerLeave { pointer } => Some(pointer),
            _ => None,
        };
        let hover = hover_pointer.and_then(|pointer| {
            let to = pos.and_then(|p| Self::hit_test_unchecked(tree, layout, p).map(|r| r.target));
            let from = match to {
                Some(id) => state.hovers.insert(pointer, id),
                None => state.hovers.remove(&pointer),
            };
            (from != to).then_some(HoverTransition { pointer, from, to })
        });
        state.last_serial = event.serial;
        state.last_timestamp = event.timestamp_micros;
        Ok(InputReceipt {
            surface: event.surface,
            serial: event.serial,
            route,
            focus: state.focus,
            activated,
            pointer_capture,
            hover,
        })
    }
}
fn interactive(role: &Role) -> bool {
    matches!(role, Role::Button | Role::Custom(_))
}
