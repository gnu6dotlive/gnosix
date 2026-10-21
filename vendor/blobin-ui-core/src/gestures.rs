//! BLOB-AC-01: headless pointer gestures following accepted UI-G04 receipts.
//! No renderer ownership, canonical mutation, or upstream ArtCraft source code.
use std::collections::BTreeMap;

use crate::input::{EventKind, InputEvent, InputReceipt, Key, Point, Units};
use crate::layout::{LayoutSnapshot, LayoutStamp, LayoutStyles, Viewport};
use crate::{NodeId, SurfaceId, UiSnapshot};

/// Logical displacement from the pointer-down position (never accumulated).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Translation {
    pub x: f32,
    pub y: f32,
}
impl Translation {
    fn between(start: Point, end: Point) -> Option<Self> {
        let x = end.x - start.x;
        let y = end.y - start.y;
        (x.is_finite() && y.is_finite() && x.abs() <= 2_000_000.0 && y.abs() <= 2_000_000.0)
            .then_some(Self { x, y })
    }
    fn is_zero(self) -> bool {
        self.x == 0.0 && self.y == 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    NoTravel,
    PointerCancelled,
    Escape,
    FocusLost,
    CaptureLost,
    LayoutChanged,
    SurfaceClosed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GestureAction {
    Begin {
        surface: SurfaceId,
        pointer: u64,
        target: NodeId,
        origin: Point,
    },
    Preview {
        surface: SurfaceId,
        pointer: u64,
        target: NodeId,
        delta: Translation,
    },
    /// Commit is one semantic edit for the consumer to apply and record in its own history.
    Commit {
        surface: SurfaceId,
        pointer: u64,
        target: NodeId,
        delta: Translation,
    },
    Cancel {
        surface: SurfaceId,
        pointer: u64,
        target: NodeId,
        reason: CancelReason,
    },
}
impl GestureAction {
    pub fn target(&self) -> NodeId {
        match self {
            Self::Begin { target, .. }
            | Self::Preview { target, .. }
            | Self::Commit { target, .. }
            | Self::Cancel { target, .. } => *target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureError {
    MismatchedReceipt,
    StaleLayout,
    StaleEvent,
    ReplayedReceipt,
    InvalidPosition,
}
impl std::fmt::Display for GestureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for GestureError {}

#[derive(Clone, Copy)]
struct Active {
    stamp: LayoutStamp,
    target: NodeId,
    button: u8,
    origin: Point,
    last: Point,
}
#[derive(Clone, Copy)]
struct Ledger {
    epoch: u64,
    last_serial: u64,
}

/// Pair the exact G04 receipt with its accepted input event. This module does not
/// itself route input: the host MUST call InputRouter::dispatch first and hand
/// that successful receipt plus the same event to observe.
#[derive(Default)]
pub struct GestureCoordinator {
    active: BTreeMap<(SurfaceId, u64), Active>,
    receipts: BTreeMap<SurfaceId, Ledger>,
}
impl GestureCoordinator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
    /// Explicit surface teardown or host geometry invalidation. No canonical mutations.
    pub fn cancel_surface(
        &mut self,
        surface: SurfaceId,
        reason: CancelReason,
    ) -> Vec<GestureAction> {
        let keys: Vec<_> = self
            .active
            .keys()
            .filter(|(s, _)| *s == surface)
            .copied()
            .collect();
        keys.into_iter()
            .filter_map(|(s, pointer)| {
                self.active
                    .remove(&(s, pointer))
                    .map(|a| GestureAction::Cancel {
                        surface: s,
                        pointer,
                        target: a.target,
                        reason,
                    })
            })
            .collect()
    }
    pub fn close_surface(&mut self, surface: SurfaceId) -> Vec<GestureAction> {
        self.receipts.remove(&surface);
        self.cancel_surface(surface, CancelReason::SurfaceClosed)
    }

    pub fn observe(
        &mut self,
        tree: &UiSnapshot,
        styles: &LayoutStyles,
        layout: &LayoutSnapshot,
        viewport: Viewport,
        event: &InputEvent,
        receipt: &InputReceipt,
    ) -> Result<Vec<GestureAction>, GestureError> {
        if receipt.surface != event.surface || receipt.serial != event.serial {
            return Err(GestureError::MismatchedReceipt);
        }
        if !layout.is_current(tree, styles, viewport) {
            return Err(GestureError::StaleLayout);
        }
        if event.surface != layout.stamp.surface
            || event.surface_epoch != layout.stamp.surface_epoch
            || event.geometry_epoch != layout.stamp.geometry_epoch
            || event.scale_factor_bits != viewport.scale_factor.to_bits()
        {
            return Err(GestureError::StaleEvent);
        }
        if self.receipts.get(&event.surface).is_some_and(|l| {
            (l.epoch == event.surface_epoch && event.serial <= l.last_serial)
                || l.epoch > event.surface_epoch
        }) {
            return Err(GestureError::ReplayedReceipt);
        }

        // All rejection above is nonmutating. Position checks happen before consuming a receipt.
        let pos = match &event.kind {
            EventKind::PointerDown {
                position, units, ..
            }
            | EventKind::PointerMove {
                position, units, ..
            }
            | EventKind::PointerUp {
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
                    return Err(GestureError::InvalidPosition);
                }
                Some(p)
            }
            _ => None,
        };
        let mut actions = Vec::new();
        let previous_epoch = self.receipts.get(&event.surface).map(|l| l.epoch);
        if previous_epoch.is_some_and(|e| e != event.surface_epoch) {
            actions.extend(self.cancel_surface(event.surface, CancelReason::LayoutChanged));
        }
        self.receipts.insert(
            event.surface,
            Ledger {
                epoch: event.surface_epoch,
                last_serial: event.serial,
            },
        );

        // A current event cannot continue a drag against a different snapshot,
        // geometry epoch, style revision or disposed target.
        let invalid: Vec<_> = self
            .active
            .iter()
            .filter(|((surface, _), a)| {
                *surface == event.surface
                    && (a.stamp != layout.stamp
                        || tree.get(a.target).is_none()
                        || layout.get(a.target).is_none())
            })
            .map(|(key, _)| *key)
            .collect();
        for (surface, pointer) in invalid {
            if let Some(a) = self.active.remove(&(surface, pointer)) {
                actions.push(GestureAction::Cancel {
                    surface,
                    pointer,
                    target: a.target,
                    reason: CancelReason::LayoutChanged,
                });
            }
        }

        match &event.kind {
            EventKind::PointerDown {
                pointer, button, ..
            } if *button == 1 => {
                let key = (event.surface, *pointer);
                if let std::collections::btree_map::Entry::Vacant(entry) = self.active.entry(key) {
                    if let (Some(target), Some(route), Some(origin)) =
                        (receipt.pointer_capture, receipt.route.as_ref(), pos)
                    {
                        if route.target == target
                            && tree.get(target).is_some_and(|n| {
                                n.surface() == event.surface && n.properties().enabled
                            })
                            && layout.get(target).is_some()
                        {
                            entry.insert(Active {
                                stamp: layout.stamp,
                                target,
                                button: *button,
                                origin,
                                last: origin,
                            });
                            actions.push(GestureAction::Begin {
                                surface: event.surface,
                                pointer: *pointer,
                                target,
                                origin,
                            });
                        }
                    }
                }
            }
            EventKind::PointerMove { pointer, .. } => {
                let key = (event.surface, *pointer);
                if let Some(a) = self.active.get_mut(&key) {
                    if receipt.pointer_capture != Some(a.target)
                        || receipt.route.as_ref().map(|r| r.target) != Some(a.target)
                    {
                        let target = a.target;
                        self.active.remove(&key);
                        actions.push(GestureAction::Cancel {
                            surface: event.surface,
                            pointer: *pointer,
                            target,
                            reason: CancelReason::CaptureLost,
                        });
                    } else if let Some(p) = pos {
                        if p != a.last {
                            let Some(delta) = Translation::between(a.origin, p) else {
                                return Err(GestureError::InvalidPosition);
                            };
                            a.last = p;
                            actions.push(GestureAction::Preview {
                                surface: event.surface,
                                pointer: *pointer,
                                target: a.target,
                                delta,
                            });
                        }
                    }
                }
            }
            EventKind::PointerUp {
                pointer, button, ..
            } => {
                let key = (event.surface, *pointer);
                if let Some(a) = self.active.get(&key).copied() {
                    if *button == a.button {
                        self.active.remove(&key);
                        if receipt.route.as_ref().map(|r| r.target) != Some(a.target) {
                            actions.push(GestureAction::Cancel {
                                surface: event.surface,
                                pointer: *pointer,
                                target: a.target,
                                reason: CancelReason::CaptureLost,
                            });
                        } else if let Some(p) = pos {
                            let Some(delta) = Translation::between(a.origin, p) else {
                                return Err(GestureError::InvalidPosition);
                            };
                            if delta.is_zero() {
                                actions.push(GestureAction::Cancel {
                                    surface: event.surface,
                                    pointer: *pointer,
                                    target: a.target,
                                    reason: CancelReason::NoTravel,
                                });
                            } else {
                                actions.push(GestureAction::Commit {
                                    surface: event.surface,
                                    pointer: *pointer,
                                    target: a.target,
                                    delta,
                                });
                            }
                        }
                    }
                }
            }
            EventKind::PointerCancel { pointer } => {
                if let Some(a) = self.active.remove(&(event.surface, *pointer)) {
                    actions.push(GestureAction::Cancel {
                        surface: event.surface,
                        pointer: *pointer,
                        target: a.target,
                        reason: CancelReason::PointerCancelled,
                    });
                }
            }
            EventKind::KeyDown {
                key: Key::Escape, ..
            } => {
                actions.extend(self.cancel_surface(event.surface, CancelReason::Escape));
            }
            EventKind::WindowBlur => {
                actions.extend(self.cancel_surface(event.surface, CancelReason::FocusLost));
            }
            _ => {}
        }
        Ok(actions)
    }
}

/// Which axis a drag has committed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragAxis {
    Horizontal,
    Vertical,
}

/// Locks a drag to its dominant axis once travel exceeds `slop` (BLOB-AC-02).
///
/// Pure consumer-side policy over [`Translation`]s from one gesture: it never
/// routes input or alters the coordinator. Before the slop is exceeded the
/// projection is zero, so a jittery press neither swipes nor scrolls. Once
/// locked, the off-axis component is always zero for the rest of the gesture;
/// ties lock horizontally.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisLock {
    slop: f32,
    locked: Option<DragAxis>,
}
impl AxisLock {
    /// `slop` is a logical distance; non-finite or negative values become 0.
    pub fn new(slop: f32) -> Self {
        Self {
            slop: if slop.is_finite() { slop.max(0.0) } else { 0.0 },
            locked: None,
        }
    }
    pub fn locked(&self) -> Option<DragAxis> {
        self.locked
    }
    /// Projects a translation (relative to the gesture origin) onto the locked
    /// axis, locking on the first translation whose length exceeds the slop.
    pub fn project(&mut self, delta: Translation) -> Translation {
        if self.locked.is_none() && (delta.x.hypot(delta.y)) > self.slop {
            self.locked = Some(if delta.x.abs() >= delta.y.abs() {
                DragAxis::Horizontal
            } else {
                DragAxis::Vertical
            });
        }
        match self.locked {
            Some(DragAxis::Horizontal) => Translation { x: delta.x, y: 0.0 },
            Some(DragAxis::Vertical) => Translation { x: 0.0, y: delta.y },
            None => Translation { x: 0.0, y: 0.0 },
        }
    }
    /// Forgets the lock for the next gesture.
    pub fn reset(&mut self) {
        self.locked = None;
    }
}
