//! UXE-02C: notification centre, headless.
//!
//! A full-height drawer on the frame's right edge (N12). It opens by pressing
//! on the right border and dragging left past `dragThreshold`. It closes by
//! dragging right from a free area, or by a press outside it.
//!
//! The history is grouped by application, newest first (N15):
//! - a collapsed group previews its newest three entries;
//! - an expanded group lists every entry with its actions.
//!
//! Gestures: a group swipe closes the whole application; once expanded, an
//! entry swipe closes that entry. A vertical drag expands or collapses a
//! group, or scrolls inside an expanded one. Middle press closes and right
//! press toggles (N16). The list is a `blobin-ui-core` tree; scrolling is a
//! translation applied outside it, so it never invalidates a gesture.

use crate::notification_toasts::{ButtonKind, ButtonPaint, TextMeasure, ToastOutcome, age_label};
use blobin_ui_core::gestures::{
    AxisLock, CancelReason, DragAxis, GestureAction, GestureCoordinator,
};
use blobin_ui_core::input::{EventKind, InputEvent, InputReceipt, InputRouter, Key, Point, Units};
use blobin_ui_core::layout::{
    self, Axis, Insets, LayoutSnapshot, LayoutStyles, Length, Rect, Size, Style, Viewport,
};
use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiTree};
use gnos_i18n::t;
use gnos_notifications::{Action, Command, NotificationId, NotificationSnapshot, Urgency};
use gnos_ux_surface::grammar::{ExtrusionMotion, TemporalResponseMode};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// Reference `sizes.sidebar.width`.
pub const CENTRE_WIDTH: f32 = 430.0;
/// Reference `sidebar.dragThreshold`.
const OPEN_DRAG: f32 = 80.0;
/// Reference `notifs.groupPreviewNum`.
const PREVIEW_ENTRIES: usize = 3;
const CLEAR_THRESHOLD: f32 = 0.3;
const EXPAND_THRESHOLD: f32 = 20.0;
const AXIS_SLOP: f32 = 6.0;
const PANEL_PADDING: f32 = 16.0;
/// Reference `padding.medium`: dock margin, group and entry padding.
pub const PAD: f32 = 12.0;
pub const IMAGE_SIZE: f32 = 42.0;
pub const GROUP_RADIUS: f32 = 16.0;
pub const ENTRY_RADIUS: f32 = 12.0;
const GROUP_GAP: f32 = 8.0;
const ENTRY_GAP: f32 = 4.0;
pub const TITLE_H: f32 = 24.0;
pub const HEADER_H: f32 = 21.0;
pub const PILL_W: f32 = 44.0;
pub const LINE: f32 = 20.0;
pub const SUMMARY_PX: f32 = 14.0;
pub const BODY_PX: f32 = 12.0;
pub const BODY_LINE: f32 = 17.0;
pub const ACTION_H: f32 = 28.0;
pub const AGE_W: f32 = 36.0;
pub const CLEAR_SIZE: f32 = 48.0;
pub const DND_W: f32 = 56.0;
const WHEEL_STEP: f32 = 48.0;
const MAX_LINES: usize = 12;
const MOTION_TIME_CONSTANT_S: f32 = 0.08;
const SETTLE_PX: f32 = 0.25;
const POINTER: u64 = 1;
const PRIMARY: u8 = 1;
const SURFACE: SurfaceId = SurfaceId(4);

#[derive(Clone, Debug, PartialEq)]
pub struct CentreEntry {
    pub id: NotificationId,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub actions: Vec<Action>,
    pub age: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CentreGroup {
    pub app_name: String,
    /// Newest first.
    pub entries: Vec<CentreEntry>,
    /// Highest urgency among the entries.
    pub urgency: Urgency,
    pub badge: char,
}

/// Groups by application in newest-first order of first appearance.
pub fn group_snapshot(snapshot: &NotificationSnapshot, now_ms: u64) -> Vec<CentreGroup> {
    let mut groups: Vec<CentreGroup> = Vec::new();
    for n in &snapshot.notifications {
        let entry = CentreEntry {
            id: n.id,
            summary: n.summary.clone(),
            body: n.body.clone(),
            urgency: n.urgency,
            actions: n.actions.clone(),
            age: age_label(now_ms.saturating_sub(n.received_ms)),
        };
        match groups.iter_mut().find(|g| g.app_name == n.app_name) {
            Some(group) => group.entries.push(entry),
            None => groups.push(CentreGroup {
                app_name: n.app_name.clone(),
                entries: vec![entry],
                urgency: Urgency::Low,
                badge: n
                    .app_name
                    .chars()
                    .chain(n.summary.chars())
                    .find(|c| c.is_alphanumeric())
                    .map_or('•', |c| c.to_uppercase().next().unwrap_or(c)),
            }),
        }
    }
    for group in &mut groups {
        let rank = |u: Urgency| match u {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        };
        group.urgency = group
            .entries
            .iter()
            .map(|e| e.urgency)
            .max_by_key(|u| rank(*u))
            .unwrap_or_default();
    }
    groups
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntryPaint {
    pub entry: CentreEntry,
    pub rect: Rect,
    pub expanded: bool,
    pub summary_lines: usize,
    pub body_lines: usize,
    pub buttons: Vec<ButtonPaint>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupPaint {
    pub group: CentreGroup,
    pub expanded: bool,
    pub rect: Rect,
    pub pill: Rect,
    pub entries: Vec<EntryPaint>,
    pub leaving: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CentreFrame {
    /// Displayed drawer, screen coordinates (slides from the right edge).
    pub panel: Rect,
    /// Paint nothing right of this x: the frame's inner right edge.
    pub clip_right: f32,
    /// Paint nothing left of this x (the frame's inner left edge when the
    /// drawer is on the left).
    pub clip_left: f32,
    /// TOPO-01A: the drawer sits on the left edge.
    pub on_left: bool,
    pub container: Rect,
    pub title: Rect,
    pub count: usize,
    pub do_not_disturb: bool,
    pub dnd: Rect,
    pub clear: Option<Rect>,
    pub list: Rect,
    pub groups: Vec<GroupPaint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Middle,
    Secondary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Control {
    Group(String),
    Pill(String),
    Entry(NotificationId),
    Close(NotificationId),
    Action(NotificationId, String),
}

#[derive(Clone, Debug)]
struct EntryLayout {
    /// Group-local top at rest.
    y: f32,
    height: f32,
    summary_lines: usize,
    body_lines: usize,
}

#[derive(Debug)]
struct Group {
    model: CentreGroup,
    height: f32,
    entries: Vec<EntryLayout>,
    /// List-local top: displayed and at rest.
    y: f32,
    target_y: f32,
    x: f32,
    drag_x: f32,
    exit: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Geometry {
    viewport: (f32, f32),
    border: f32,
    /// TOPO-01A: the drawer is on the left edge (the rail took the right).
    on_left: bool,
    /// TOPO-01C: the frame's top and bottom insets; a horizontal Bar makes
    /// one of them the Bar's thickness, and the drawer meets it.
    top: f32,
    bottom: f32,
}

impl Geometry {
    fn inset(self) -> f32 {
        (PANEL_PADDING - self.border).clamp(0.0, PANEL_PADDING)
    }
    fn panel_x(self) -> f32 {
        if self.on_left {
            self.border
        } else {
            self.viewport.0 - self.border - CENTRE_WIDTH
        }
    }
    fn panel_height(self) -> f32 {
        (self.viewport.1 - self.top - self.bottom).max(0.0)
    }
    /// Container, panel-local.
    fn container(self) -> Rect {
        let inset = self.inset();
        Rect::new(
            PANEL_PADDING,
            inset,
            CENTRE_WIDTH - PANEL_PADDING - inset,
            (self.panel_height() - inset - PANEL_PADDING).max(0.0),
        )
    }
    fn dock(self) -> Rect {
        let c = self.container();
        Rect::new(
            c.x + PAD,
            c.y + PAD,
            c.width - 2.0 * PAD,
            (c.height - 2.0 * PAD).max(0.0),
        )
    }
    fn list(self) -> Rect {
        let d = self.dock();
        let top = d.y + TITLE_H + PAD;
        Rect::new(d.x, top, d.width, (d.y + d.height - top).max(0.0))
    }
    /// Text column inside a group: right of the badge.
    fn column_width(self) -> f32 {
        self.dock().width - 3.0 * PAD - IMAGE_SIZE
    }
}

struct Surface {
    tree: UiTree,
    styles: LayoutStyles,
    layout: LayoutSnapshot,
    viewport: Viewport,
    controls: BTreeMap<NodeId, Control>,
}

#[derive(Debug)]
enum DragTarget {
    Group(String),
    Entry(NotificationId),
}

#[derive(Debug)]
struct Drag {
    target: DragTarget,
    lock: AxisLock,
    scroll_origin: f32,
}

pub struct CentreHost {
    geometry: Option<Geometry>,
    revision: Option<u64>,
    now_ms: u64,
    open: bool,
    /// Displayed slide: 0 open, width + border closed.
    /// The drawer's extrusion out of the right border (envelope grammar).
    motion: ExtrusionMotion,
    groups: Vec<Group>,
    leaving: Vec<Group>,
    expanded: BTreeSet<String>,
    entry_drag_x: Option<(NotificationId, f32)>,
    scroll: f32,
    do_not_disturb: bool,
    /// The side requested for the next sync (TOPO-01A).
    pending_on_left: bool,
    /// Top and bottom insets requested for the next sync (TOPO-01C).
    pending_insets: Option<(f32, f32)>,
    surface: Option<Surface>,
    rebuild: bool,
    remeasure: bool,
    router: InputRouter,
    gestures: GestureCoordinator,
    drag: Option<Drag>,
    /// Border press waiting to become an opening drag: (x, y).
    opener: Option<(f32, f32)>,
    /// Free-area press inside the open centre: x.
    closer: Option<f32>,
    serial: u64,
    epoch: Instant,
    last_micros: u64,
    last_tick: Option<Instant>,
}

impl Default for CentreHost {
    fn default() -> Self {
        Self::new()
    }
}

impl CentreHost {
    pub fn new() -> Self {
        Self {
            geometry: None,
            revision: None,
            now_ms: 0,
            open: false,
            motion: ExtrusionMotion::closed(),
            groups: Vec::new(),
            leaving: Vec::new(),
            expanded: BTreeSet::new(),
            entry_drag_x: None,
            scroll: 0.0,
            do_not_disturb: false,
            pending_on_left: false,
            pending_insets: None,
            surface: None,
            rebuild: false,
            remeasure: false,
            router: InputRouter::new(),
            gestures: GestureCoordinator::new(),
            drag: None,
            opener: None,
            closer: None,
            serial: 0,
            epoch: Instant::now(),
            last_micros: 0,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn closed_offset(&self) -> f32 {
        CENTRE_WIDTH + self.geometry.map_or(0.0, |g| g.border)
    }

    /// Displayed slide toward the drawer's edge: the unrevealed part.
    fn offset(&self) -> f32 {
        let slide = (1.0 - self.motion.phase()) * self.closed_offset();
        if self.on_left() { -slide } else { slide }
    }

    fn on_left(&self) -> bool {
        self.geometry.is_some_and(|g| g.on_left)
    }

    /// TOPO-01A: put the drawer on the left edge when the rail takes the
    /// right. Takes effect at the next sync.
    pub fn set_on_left(&mut self, on_left: bool) {
        self.pending_on_left = on_left;
    }

    /// TOPO-01C: the frame's top and bottom insets when a horizontal Bar
    /// takes one of those edges; `None` is the border on both. Takes effect
    /// at the next sync.
    pub fn set_vertical_insets(&mut self, insets: Option<(f32, f32)>) {
        self.pending_insets = insets;
    }

    fn shown(&self) -> bool {
        self.motion.is_visible() && self.offset().abs() < self.closed_offset() - SETTLE_PX
    }

    fn is_expanded(&self, app: &str) -> bool {
        self.expanded.contains(app)
    }

    fn measure_group(
        group: &mut Group,
        expanded: bool,
        geometry: Geometry,
        measure: &mut dyn TextMeasure,
    ) {
        let column = geometry.column_width();
        let mut y = HEADER_H + if expanded { ENTRY_GAP } else { 0.0 };
        group.entries.clear();
        let shown = if expanded {
            group.model.entries.len()
        } else {
            group.model.entries.len().min(PREVIEW_ENTRIES)
        };
        for entry in group.model.entries.iter().take(shown) {
            let (height, summary_lines, body_lines) = if expanded {
                let inner = column - 2.0 * PAD;
                let summary_lines = measure
                    .lines(&entry.summary, SUMMARY_PX, (inner - AGE_W - 8.0).max(1.0))
                    .clamp(1, MAX_LINES);
                let body_lines = if entry.body.is_empty() {
                    0
                } else {
                    measure
                        .lines(&entry.body, BODY_PX, inner.max(1.0))
                        .clamp(1, MAX_LINES)
                };
                let body_block = if body_lines > 0 {
                    ENTRY_GAP + BODY_LINE * body_lines as f32
                } else {
                    0.0
                };
                (
                    2.0 * PAD + LINE * summary_lines as f32 + body_block + PAD + ACTION_H,
                    summary_lines,
                    body_lines,
                )
            } else {
                (LINE, 1, 1)
            };
            group.entries.push(EntryLayout {
                y: PAD + y,
                height,
                summary_lines,
                body_lines,
            });
            y += height + ENTRY_GAP;
        }
        let column_height = y - ENTRY_GAP;
        group.height = 2.0 * PAD + column_height.max(IMAGE_SIZE);
    }

    fn content_height(&self) -> f32 {
        if self.groups.is_empty() {
            return 0.0;
        }
        self.groups.iter().map(|g| g.height).sum::<f32>()
            + GROUP_GAP * (self.groups.len() - 1) as f32
    }

    fn max_scroll(&self) -> f32 {
        let list = self.geometry.map_or(0.0, |g| g.list().height);
        (self.content_height() - list).max(0.0)
    }

    fn place_groups(&mut self) {
        let mut y = 0.0;
        for group in &mut self.groups {
            group.target_y = y;
            if group.y.is_nan() {
                group.y = y;
            }
            y += group.height + GROUP_GAP;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    fn measure_all(&mut self, measure: &mut dyn TextMeasure) {
        let Some(geometry) = self.geometry else {
            return;
        };
        let expanded = self.expanded.clone();
        for group in &mut self.groups {
            let is_expanded = expanded.contains(&group.model.app_name);
            Self::measure_group(group, is_expanded, geometry, measure);
        }
        self.place_groups();
        self.rebuild = true;
        self.rebuild_if_idle();
    }

    /// Applies the latest service snapshot. Returns whether anything visible
    /// changed; a no-op when revision, geometry and layout are unchanged.
    pub fn sync(
        &mut self,
        update: Option<(&NotificationSnapshot, u64)>,
        viewport: (f32, f32),
        border: f32,
        measure: &mut dyn TextMeasure,
    ) -> bool {
        let (top, bottom) = self.pending_insets.unwrap_or((border, border));
        let geometry = Geometry {
            viewport,
            border,
            on_left: self.pending_on_left,
            top,
            bottom,
        };
        let geometry_changed = self.geometry != Some(geometry);
        let revision = update.map(|(s, _)| s.revision);
        if !geometry_changed && revision == self.revision {
            if !std::mem::take(&mut self.remeasure) {
                return false;
            }
            self.measure_all(measure);
            return true;
        }
        self.geometry = Some(geometry);
        self.revision = revision;
        self.do_not_disturb = update.is_some_and(|(s, _)| s.do_not_disturb);
        let models = update.map_or_else(Vec::new, |(s, now)| {
            self.now_ms = now;
            group_snapshot(s, now)
        });
        let mut previous = std::mem::take(&mut self.groups);
        for model in models {
            let group = match previous
                .iter()
                .position(|g| g.model.app_name == model.app_name)
            {
                Some(index) => {
                    let mut group = previous.remove(index);
                    group.model = model;
                    group
                }
                None => Group {
                    model,
                    height: 0.0,
                    entries: Vec::new(),
                    y: f32::NAN,
                    target_y: 0.0,
                    x: 0.0,
                    drag_x: 0.0,
                    exit: None,
                },
            };
            self.groups.push(group);
        }
        for mut group in previous {
            group.x += group.drag_x;
            group.drag_x = 0.0;
            if group.exit.is_none() {
                group.exit = Some(if group.x < 0.0 { -1.0 } else { 1.0 });
            }
            if matches!(&self.drag, Some(Drag { target: DragTarget::Group(app), .. }) if *app == group.model.app_name)
            {
                self.drag = None;
                self.gestures
                    .cancel_surface(SURFACE, CancelReason::SurfaceClosed);
            }
            self.expanded.remove(&group.model.app_name);
            self.leaving.push(group);
        }
        if let Some((id, _)) = self.entry_drag_x
            && !self
                .groups
                .iter()
                .any(|g| g.model.entries.iter().any(|e| e.id == id))
        {
            self.entry_drag_x = None;
        }
        self.remeasure = false;
        self.measure_all(measure);
        true
    }

    fn rebuild_if_idle(&mut self) {
        if !self.rebuild || self.drag.is_some() {
            return;
        }
        self.rebuild = false;
        self.surface = self.build_surface();
    }

    fn build_surface(&self) -> Option<Surface> {
        let geometry = self.geometry?;
        if self.groups.is_empty() {
            return None;
        }
        let width = geometry.dock().width;
        let mut tree = UiTree::new();
        let mut placed: Vec<(NodeId, Style)> = Vec::new();
        let mut controls = BTreeMap::new();
        let grow = Style {
            flex_grow: 1.0,
            ..Style::default()
        };
        tree.transact(|tx| {
            tx.open_surface(SURFACE)?;
            let root = tx.append_root(SURFACE, NodeProperties::new(Role::Container, "centre"))?;
            placed.push((
                root,
                Style {
                    gap: GROUP_GAP,
                    ..Style::fill()
                },
            ));
            for group in &self.groups {
                let app = group.model.app_name.clone();
                let expanded = self.is_expanded(&app);
                let node = tx.append_child(
                    root,
                    NodeProperties::new(Role::Custom("group".into()), app.clone()),
                )?;
                placed.push((
                    node,
                    Style {
                        height: Length::Px(group.height),
                        padding: Insets::uniform(PAD),
                        flex_shrink: 0.0,
                        ..Style::default()
                    },
                ));
                controls.insert(node, Control::Group(app.clone()));
                let header =
                    tx.append_child(node, NodeProperties::new(Role::Container, "header"))?;
                placed.push((
                    header,
                    Style {
                        height: Length::Px(HEADER_H),
                        axis: Axis::Row,
                        padding: Insets {
                            left: IMAGE_SIZE + PAD,
                            ..Insets::default()
                        },
                        flex_shrink: 0.0,
                        ..Style::default()
                    },
                ));
                let spacer = tx.append_child(header, NodeProperties::new(Role::Container, ""))?;
                placed.push((spacer, grow.clone()));
                let pill = tx.append_child(
                    header,
                    NodeProperties::new(
                        Role::Button,
                        if expanded { t("Collapse") } else { t("Expand") },
                    ),
                )?;
                placed.push((
                    pill,
                    Style {
                        width: Length::Px(PILL_W),
                        flex_shrink: 0.0,
                        ..Style::default()
                    },
                ));
                controls.insert(pill, Control::Pill(app.clone()));
                if !expanded {
                    continue;
                }
                let list =
                    tx.append_child(node, NodeProperties::new(Role::Container, "entries"))?;
                placed.push((
                    list,
                    Style {
                        gap: ENTRY_GAP,
                        padding: Insets {
                            left: IMAGE_SIZE + PAD,
                            top: ENTRY_GAP,
                            ..Insets::default()
                        },
                        flex_grow: 1.0,
                        ..Style::default()
                    },
                ));
                for (entry, layout) in group.model.entries.iter().zip(&group.entries) {
                    let node = tx.append_child(
                        list,
                        NodeProperties::new(Role::Custom("entry".into()), entry.summary.clone()),
                    )?;
                    placed.push((
                        node,
                        Style {
                            height: Length::Px(layout.height),
                            padding: Insets::uniform(PAD),
                            flex_shrink: 0.0,
                            ..Style::default()
                        },
                    ));
                    controls.insert(node, Control::Entry(entry.id));
                    let filler = tx.append_child(node, NodeProperties::new(Role::Container, ""))?;
                    placed.push((filler, grow.clone()));
                    let row =
                        tx.append_child(node, NodeProperties::new(Role::Container, "actions"))?;
                    placed.push((
                        row,
                        Style {
                            height: Length::Px(ACTION_H),
                            axis: Axis::Row,
                            gap: 8.0,
                            flex_shrink: 0.0,
                            ..Style::default()
                        },
                    ));
                    let close =
                        tx.append_child(row, NodeProperties::new(Role::Button, t("Close")))?;
                    placed.push((close, grow.clone()));
                    controls.insert(close, Control::Close(entry.id));
                    for action in &entry.actions {
                        let button = tx.append_child(
                            row,
                            NodeProperties::new(Role::Button, action.label.clone()),
                        )?;
                        placed.push((button, grow.clone()));
                        controls.insert(button, Control::Action(entry.id, action.key.clone()));
                    }
                }
            }
            Ok(())
        })
        .ok()?;
        let mut styles = LayoutStyles::new();
        let snapshot = tree.snapshot();
        for (id, style) in placed {
            styles.set(&snapshot, id, style).ok()?;
        }
        let viewport = Viewport::new(Size::new(width, self.content_height().max(1.0)), 1.0, 1);
        let layout = layout::compute(&snapshot, &styles, SURFACE, viewport).ok()?;
        Some(Surface {
            tree,
            styles,
            layout,
            viewport,
            controls,
        })
    }

    fn micros(&mut self) -> u64 {
        let now = u64::try_from(self.epoch.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.last_micros = now.max(self.last_micros + 1);
        self.last_micros
    }

    /// Screen -> displayed panel-local.
    fn panel_local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let g = self.geometry?;
        Some((x - g.panel_x() - self.offset(), y - g.top))
    }

    fn in_panel(&self, x: f32, y: f32) -> bool {
        let Some(g) = self.geometry else {
            return false;
        };
        self.panel_local(x, y).is_some_and(|(lx, ly)| {
            (0.0..=CENTRE_WIDTH).contains(&lx) && (0.0..=g.panel_height()).contains(&ly)
        })
    }

    /// Screen -> list content coordinates, when inside the visible list.
    fn list_local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let list = self.geometry?.list();
        let (lx, ly) = self.panel_local(x, y)?;
        let inside = (list.x..=list.x + list.width).contains(&lx)
            && (list.y..=list.y + list.height).contains(&ly);
        inside.then_some((lx - list.x, ly - list.y + self.scroll))
    }

    fn list_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let list = self.geometry?.list();
        let (lx, ly) = self.panel_local(x, y)?;
        Some((lx - list.x, ly - list.y + self.scroll))
    }

    fn route(&mut self, kind: EventKind) -> Option<(InputReceipt, Vec<GestureAction>)> {
        let t = self.micros();
        self.serial += 1;
        let surface = self.surface.as_ref()?;
        let event = InputEvent::for_layout(&surface.layout, self.serial, t, kind);
        let snapshot = surface.tree.snapshot();
        let receipt = self
            .router
            .dispatch(
                &snapshot,
                &surface.styles,
                &surface.layout,
                surface.viewport,
                event.clone(),
            )
            .ok()?;
        let actions = self
            .gestures
            .observe(
                &snapshot,
                &surface.styles,
                &surface.layout,
                surface.viewport,
                &event,
                &receipt,
            )
            .unwrap_or_default();
        Some((receipt, actions))
    }

    /// CURSOR-01: what the pointer is over in the centre (`None`: off it),
    /// from the layout's own hit test (no event is sent).
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        if !self.in_panel(x, y) {
            return None;
        }
        if self.chrome_hit(x, y).is_some() {
            return Some(crate::cursor::Affordance::Press);
        }
        let press = self.list_point(x, y).is_some_and(|(lx, ly)| {
            self.surface.as_ref().is_some_and(|surface| {
                InputRouter::hit_test(
                    &surface.tree.snapshot(),
                    &surface.styles,
                    &surface.layout,
                    surface.viewport,
                    Point::new(lx, ly),
                )
                .ok()
                .flatten()
                .and_then(|r| self.control(r.target))
                .is_some()
            })
        });
        Some(if press {
            crate::cursor::Affordance::Press
        } else {
            crate::cursor::Affordance::Inert
        })
    }

    fn control(&self, node: NodeId) -> Option<Control> {
        self.surface.as_ref()?.controls.get(&node).cloned()
    }

    fn group_ids(&self, app: &str) -> Vec<NotificationId> {
        self.groups
            .iter()
            .find(|g| g.model.app_name == app)
            .map(|g| g.model.entries.iter().map(|e| e.id).collect())
            .unwrap_or_default()
    }

    fn app_of(&self, id: NotificationId) -> Option<String> {
        self.groups
            .iter()
            .find(|g| g.model.entries.iter().any(|e| e.id == id))
            .map(|g| g.model.app_name.clone())
    }

    fn set_expanded(&mut self, app: &str, expanded: bool) -> bool {
        let changed = if expanded {
            self.expanded.insert(app.to_owned())
        } else {
            self.expanded.remove(app)
        };
        if changed {
            self.remeasure = true;
        }
        changed
    }

    fn dismiss_group(&mut self, app: &str, direction: f32, outcome: &mut ToastOutcome) {
        let ids = self.group_ids(app);
        if let Some(group) = self.groups.iter_mut().find(|g| g.model.app_name == app) {
            group.exit = Some(direction);
        }
        outcome
            .commands
            .extend(ids.into_iter().map(Command::Dismiss));
        outcome.changed = true;
    }

    pub fn open(&mut self) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if !self.open {
            self.open = true;
            outcome.commands.push(Command::SetCentreOpen(true));
            outcome.changed = true;
        }
        outcome
    }

    pub fn close(&mut self) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if self.open {
            self.open = false;
            self.drag = None;
            self.closer = None;
            self.gestures
                .cancel_surface(SURFACE, CancelReason::SurfaceClosed);
            outcome.commands.push(Command::SetCentreOpen(false));
            outcome.changed = true;
        }
        outcome
    }

    /// The drawer's border strip, where an opening drag starts.
    fn on_edge(&self, x: f32) -> bool {
        self.geometry.is_some_and(|g| {
            if g.on_left {
                x >= 0.0 && x <= g.border.max(2.0)
            } else {
                x >= g.viewport.0 - g.border.max(2.0) && x <= g.viewport.0
            }
        })
    }

    /// Travel away from the drawer's edge (the opening direction).
    fn inward(&self, from: f32, to: f32) -> f32 {
        if self.on_left() { to - from } else { from - to }
    }

    fn apply_gestures(&mut self, actions: Vec<GestureAction>, outcome: &mut ToastOutcome) {
        let list_width = self.geometry.map_or(CENTRE_WIDTH, |g| g.dock().width);
        for action in actions {
            let target = match self.control(action.target()) {
                Some(Control::Group(app)) => DragTarget::Group(app),
                Some(Control::Entry(id)) => DragTarget::Entry(id),
                _ => continue,
            };
            match action {
                GestureAction::Begin { .. } => {
                    self.drag = Some(Drag {
                        target,
                        lock: AxisLock::new(AXIS_SLOP),
                        scroll_origin: self.scroll,
                    });
                }
                GestureAction::Preview { delta, .. } => {
                    let Some(drag) = self.drag.as_mut() else {
                        continue;
                    };
                    let projected = drag.lock.project(delta);
                    let axis = drag.lock.locked();
                    let origin = drag.scroll_origin;
                    let scrolls = matches!(&drag.target, DragTarget::Entry(_))
                        || matches!(&drag.target, DragTarget::Group(app) if self.expanded.contains(app));
                    match (axis, &drag.target) {
                        (Some(DragAxis::Horizontal), DragTarget::Group(app)) => {
                            let app = app.clone();
                            if let Some(g) =
                                self.groups.iter_mut().find(|g| g.model.app_name == app)
                            {
                                g.drag_x = projected.x;
                            }
                        }
                        (Some(DragAxis::Horizontal), DragTarget::Entry(id)) => {
                            self.entry_drag_x = Some((*id, projected.x));
                        }
                        (Some(DragAxis::Vertical), _) if scrolls => {
                            self.scroll = (origin - projected.y).clamp(0.0, self.max_scroll());
                        }
                        _ => continue,
                    }
                    outcome.changed = true;
                }
                GestureAction::Commit { delta, .. } => {
                    let Some(mut drag) = self.drag.take() else {
                        continue;
                    };
                    let projected = drag.lock.project(delta);
                    let axis = drag.lock.locked();
                    outcome.changed = true;
                    let past = projected.x.abs() >= CLEAR_THRESHOLD * list_width;
                    match (axis, drag.target) {
                        (Some(DragAxis::Horizontal), DragTarget::Group(app)) => {
                            if let Some(g) =
                                self.groups.iter_mut().find(|g| g.model.app_name == app)
                            {
                                g.x += g.drag_x;
                                g.drag_x = 0.0;
                            }
                            if past {
                                self.dismiss_group(&app, projected.x.signum(), outcome);
                            }
                        }
                        (Some(DragAxis::Horizontal), DragTarget::Entry(id)) => {
                            self.entry_drag_x = None;
                            if past {
                                outcome.commands.push(Command::Dismiss(id));
                            }
                        }
                        (Some(DragAxis::Vertical), DragTarget::Group(app))
                            if !self.expanded.contains(&app)
                                && projected.y.abs() > EXPAND_THRESHOLD =>
                        {
                            self.set_expanded(&app, projected.y > 0.0);
                        }
                        _ => {}
                    }
                }
                GestureAction::Cancel { reason, .. } => {
                    self.drag = None;
                    if reason != CancelReason::NoTravel {
                        for g in &mut self.groups {
                            g.x += g.drag_x;
                            g.drag_x = 0.0;
                        }
                        self.entry_drag_x = None;
                        outcome.changed = true;
                    }
                }
            }
        }
    }

    /// Header and footer controls live outside the list tree.
    fn chrome_hit(&self, x: f32, y: f32) -> Option<&'static str> {
        let frame = self.frame()?;
        let contains = |r: Rect| x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height;
        if contains(frame.dnd) {
            return Some("dnd");
        }
        if frame.clear.is_some_and(contains) {
            return Some("clear");
        }
        None
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if let Some((start_x, _)) = self.opener {
            outcome.consumed = true;
            if self.inward(start_x, x) > OPEN_DRAG {
                self.opener = None;
                return self.open();
            }
            return outcome;
        }
        if !self.open {
            return outcome;
        }
        outcome.consumed = true;
        if let Some(start_x) = self.closer {
            if -self.inward(start_x, x) > OPEN_DRAG {
                return self.close();
            }
            return outcome;
        }
        if let Some((lx, ly)) = self.list_point(x, y)
            && (self.drag.is_some() || self.list_local(x, y).is_some())
            && let Some((_, actions)) = self.route(EventKind::PointerMove {
                pointer: POINTER,
                position: Point::new(lx, ly),
                units: Units::Logical,
            })
        {
            self.apply_gestures(actions, &mut outcome);
        }
        outcome
    }

    pub fn pointer_down(&mut self, x: f32, y: f32, button: PointerButton) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if !self.open {
            if button == PointerButton::Primary && self.on_edge(x) {
                self.opener = Some((x, y));
                outcome.consumed = true;
            }
            return outcome;
        }
        outcome.consumed = true;
        if !self.in_panel(x, y) {
            // A press outside the open centre closes it (and is consumed).
            return self.close_consumed();
        }
        if button == PointerButton::Primary && self.chrome_hit(x, y).is_some() {
            return outcome;
        }
        let Some((lx, ly)) = self.list_local(x, y) else {
            if button == PointerButton::Primary {
                self.closer = Some(x);
            }
            return outcome;
        };
        let raw_button = match button {
            PointerButton::Primary => PRIMARY,
            PointerButton::Middle => 2,
            PointerButton::Secondary => 3,
        };
        // No list tree (an empty centre) is a free area too.
        let (hit, actions) = match self.route(EventKind::PointerDown {
            pointer: POINTER,
            button: raw_button,
            position: Point::new(lx, ly),
            units: Units::Logical,
        }) {
            Some((receipt, actions)) => (
                receipt.route.as_ref().and_then(|r| self.control(r.target)),
                actions,
            ),
            None => (None, Vec::new()),
        };
        match (button, hit) {
            (PointerButton::Primary, None) => self.closer = Some(x),
            (PointerButton::Primary, Some(_)) => self.apply_gestures(actions, &mut outcome),
            (PointerButton::Middle, Some(Control::Group(app) | Control::Pill(app))) => {
                self.dismiss_group(&app, 1.0, &mut outcome);
            }
            (
                PointerButton::Middle,
                Some(Control::Entry(id) | Control::Close(id) | Control::Action(id, _)),
            ) => {
                outcome.commands.push(Command::Dismiss(id));
            }
            (PointerButton::Secondary, Some(control)) => {
                let app = match control {
                    Control::Group(app) | Control::Pill(app) => Some(app),
                    Control::Entry(id) | Control::Close(id) | Control::Action(id, _) => {
                        self.app_of(id)
                    }
                };
                if let Some(app) = app {
                    let expand = !self.is_expanded(&app);
                    self.set_expanded(&app, expand);
                    outcome.changed = true;
                }
            }
            _ => {}
        }
        outcome
    }

    fn close_consumed(&mut self) -> ToastOutcome {
        let mut outcome = self.close();
        outcome.consumed = true;
        outcome
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, button: PointerButton) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if self.opener.take().is_some() {
            outcome.consumed = true;
            return outcome;
        }
        if !self.open {
            return outcome;
        }
        outcome.consumed = true;
        if self.closer.take().is_some() {
            return outcome;
        }
        if button == PointerButton::Primary
            && self.drag.is_none()
            && let Some(hit) = self.chrome_hit(x, y)
        {
            match hit {
                "dnd" => outcome
                    .commands
                    .push(Command::SetDoNotDisturb(!self.do_not_disturb)),
                _ => {
                    for g in &mut self.groups {
                        g.exit = Some(1.0);
                    }
                    outcome.commands.push(Command::ClearAll);
                }
            }
            outcome.changed = true;
            return outcome;
        }
        let captured = self.router.capture(SURFACE, POINTER).is_some();
        let Some((lx, ly)) = self.list_point(x, y) else {
            return outcome;
        };
        if !captured && self.list_local(x, y).is_none() {
            return outcome;
        }
        let raw_button = match button {
            PointerButton::Primary => PRIMARY,
            PointerButton::Middle => 2,
            PointerButton::Secondary => 3,
        };
        let Some((receipt, actions)) = self.route(EventKind::PointerUp {
            pointer: POINTER,
            button: raw_button,
            position: Point::new(lx, ly),
            units: Units::Logical,
        }) else {
            return outcome;
        };
        if button != PointerButton::Primary {
            return outcome;
        }
        self.apply_gestures(actions, &mut outcome);
        match receipt.activated.and_then(|node| self.control(node)) {
            Some(Control::Pill(app)) => {
                let expand = !self.is_expanded(&app);
                self.set_expanded(&app, expand);
                outcome.changed = true;
            }
            Some(Control::Close(id)) => outcome.commands.push(Command::Dismiss(id)),
            Some(Control::Action(id, key)) => outcome.commands.push(Command::Invoke(id, key)),
            _ => {}
        }
        if self.rebuild {
            self.rebuild_if_idle();
            outcome.changed = true;
        }
        outcome
    }

    /// Wheel over the list scrolls it (Wayland sign: positive is down).
    pub fn wheel(&mut self, x: f32, y: f32, delta_y: f32) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if !self.open || !self.in_panel(x, y) {
            return outcome;
        }
        outcome.consumed = true;
        let next = (self.scroll + delta_y.signum() * WHEEL_STEP).clamp(0.0, self.max_scroll());
        if next != self.scroll {
            self.scroll = next;
            outcome.changed = true;
        }
        outcome
    }

    pub fn escape(&mut self) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if self.drag.is_some() {
            if let Some((_, actions)) = self.route(EventKind::KeyDown {
                key: Key::Escape,
                shift: false,
                repeat: false,
            }) {
                self.apply_gestures(actions, &mut outcome);
            }
            self.drag = None;
            for g in &mut self.groups {
                g.x += g.drag_x;
                g.drag_x = 0.0;
            }
            self.entry_drag_x = None;
            outcome.changed = true;
            outcome.consumed = true;
            return outcome;
        }
        if self.open {
            return self.close_consumed();
        }
        outcome
    }

    fn exit_distance(&self) -> f32 {
        2.0 * self.geometry.map_or(CENTRE_WIDTH, |g| g.dock().width)
    }

    pub fn needs_frame(&self) -> bool {
        let unsettled = |a: f32, b: f32| (a - b).abs() > SETTLE_PX;
        let exit = self.exit_distance();
        let dragging = |app: &str| matches!(&self.drag, Some(Drag { target: DragTarget::Group(a), .. }) if a == app);
        self.motion.is_open() != self.open
            || self.motion.needs_frame()
            || !self.leaving.is_empty()
            || self.groups.iter().any(|g| {
                unsettled(g.y, g.target_y)
                    || (!dragging(&g.model.app_name)
                        && unsettled(g.x, g.exit.map_or(0.0, |d| d * exit)))
            })
    }

    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .min(0.1);
        self.last_tick = Some(now);
        let k = if reduced {
            1.0
        } else {
            1.0 - (-dt / MOTION_TIME_CONSTANT_S).exp()
        };
        let approach = |value: &mut f32, target: f32| {
            *value += (target - *value) * k;
            if (target - *value).abs() <= SETTLE_PX {
                *value = target;
            }
        };
        self.motion.set_open(self.open);
        let mode = if reduced {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        if self
            .motion
            .step(if dt > 0.0 { dt } else { 1.0 / 60.0 }, mode)
            .is_err()
        {
            self.motion.snap();
        }
        let exit = self.exit_distance();
        let dragged = match &self.drag {
            Some(Drag {
                target: DragTarget::Group(app),
                ..
            }) => Some(app.clone()),
            _ => None,
        };
        for g in &mut self.groups {
            approach(&mut g.y, g.target_y);
            if dragged.as_deref() != Some(g.model.app_name.as_str()) {
                approach(&mut g.x, g.exit.map_or(0.0, |d| d * exit));
            }
        }
        for g in &mut self.leaving {
            approach(&mut g.x, g.exit.unwrap_or(1.0) * exit);
        }
        self.leaving
            .retain(|g| (g.x - g.exit.unwrap_or(1.0) * exit).abs() > SETTLE_PX);
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    /// Input the scene must route here: the right border strip while closed;
    /// the whole output while open (a press outside closes, N12).
    pub fn input_rects(&self) -> Vec<Rect> {
        let Some(g) = self.geometry else {
            return Vec::new();
        };
        if self.open || self.drag.is_some() {
            return vec![Rect::new(0.0, 0.0, g.viewport.0, g.viewport.1)];
        }
        let strip = g.border.max(2.0);
        // The border strip beside the drawer; a horizontal Bar's span is the
        // Bar's, not the strip's.
        let strip_top = g.top - g.border;
        let mut rects = vec![Rect::new(
            if g.on_left { 0.0 } else { g.viewport.0 - strip },
            strip_top,
            strip,
            g.viewport.1 - strip_top - (g.bottom - g.border),
        )];
        if let Some(panel) = self.visible_panel() {
            rects.push(panel);
        }
        rects
    }

    fn visible_panel(&self) -> Option<Rect> {
        let g = self.geometry?;
        if !self.shown() {
            return None;
        }
        let x = g.panel_x() + self.offset();
        let (lo, hi) = if g.on_left {
            (g.border, x + CENTRE_WIDTH)
        } else {
            (x, g.viewport.0 - g.border)
        };
        Some(Rect::new(lo, g.top, (hi - lo).max(0.0), g.panel_height()))
    }

    fn paint_group(
        &self,
        g: &Group,
        leaving: bool,
        origin: (f32, f32),
        list_width: f32,
    ) -> GroupPaint {
        let expanded = !leaving && self.is_expanded(&g.model.app_name);
        let gx = origin.0 + g.x + g.drag_x;
        let gy = origin.1 + g.y - self.scroll;
        let rect = Rect::new(gx, gy, list_width, g.height);
        let pill = Rect::new(gx + list_width - PAD - PILL_W, gy + PAD, PILL_W, HEADER_H);
        let column_x = gx + 2.0 * PAD + IMAGE_SIZE;
        let column_w = list_width - 3.0 * PAD - IMAGE_SIZE;
        let surface = self.surface.as_ref();
        let entries = g
            .model
            .entries
            .iter()
            .zip(&g.entries)
            .map(|(entry, layout)| {
                let dx = match self.entry_drag_x {
                    Some((id, dx)) if id == entry.id => dx,
                    _ => 0.0,
                };
                let rect = Rect::new(column_x + dx, gy + layout.y, column_w, layout.height);
                let mut buttons = Vec::new();
                if expanded && let Some(surface) = surface {
                    for (node, control) in &surface.controls {
                        let kind = match control {
                            Control::Close(id) if *id == entry.id => ButtonKind::Close,
                            Control::Action(id, key) if *id == entry.id => ButtonKind::Action {
                                label: entry
                                    .actions
                                    .iter()
                                    .find(|a| a.key == *key)
                                    .map(|a| a.label.clone())
                                    .unwrap_or_default(),
                            },
                            _ => continue,
                        };
                        if let Some(at) = surface.layout.get(*node) {
                            // Router rectangles are at rest, unscrolled.
                            buttons.push(ButtonPaint {
                                kind,
                                rect: Rect::new(
                                    origin.0 + at.rect.x + g.x + g.drag_x + dx,
                                    origin.1 + at.rect.y - self.scroll + (g.y - g.target_y),
                                    at.rect.width,
                                    at.rect.height,
                                ),
                            });
                        }
                    }
                }
                EntryPaint {
                    entry: entry.clone(),
                    rect,
                    expanded,
                    summary_lines: layout.summary_lines,
                    body_lines: layout.body_lines,
                    buttons,
                }
            })
            .collect();
        GroupPaint {
            group: g.model.clone(),
            expanded,
            rect,
            pill,
            entries,
            leaving,
        }
    }

    /// The service's observed Do not disturb (SURF-01B shows it too).
    pub fn do_not_disturb(&self) -> bool {
        self.do_not_disturb
    }

    pub fn frame(&self) -> Option<CentreFrame> {
        let g = self.geometry?;
        if !self.shown() {
            return None;
        }
        let px = g.panel_x() + self.offset();
        let py = g.top;
        let shift = |r: Rect| Rect::new(px + r.x, py + r.y, r.width, r.height);
        let container = shift(g.container());
        let dock = shift(g.dock());
        let list = shift(g.list());
        let title = Rect::new(dock.x, dock.y, dock.width - DND_W - 8.0, TITLE_H);
        let dnd = Rect::new(dock.x + dock.width - DND_W, dock.y, DND_W, TITLE_H);
        let count = self.groups.iter().map(|g| g.model.entries.len()).sum();
        let clear = (count > 0).then(|| {
            Rect::new(
                dock.x + dock.width - PAD - CLEAR_SIZE,
                dock.y + dock.height - PAD - CLEAR_SIZE,
                CLEAR_SIZE,
                CLEAR_SIZE,
            )
        });
        let groups = self
            .groups
            .iter()
            .map(|grp| self.paint_group(grp, false, (list.x, list.y), list.width))
            .chain(
                self.leaving
                    .iter()
                    .map(|grp| self.paint_group(grp, true, (list.x, list.y), list.width)),
            )
            .collect();
        Some(CentreFrame {
            panel: Rect::new(px, py, CENTRE_WIDTH, g.panel_height()),
            clip_right: if g.on_left {
                g.viewport.0
            } else {
                g.viewport.0 - g.border
            },
            clip_left: if g.on_left { g.border } else { 0.0 },
            on_left: g.on_left,
            container,
            title,
            count,
            do_not_disturb: self.do_not_disturb,
            dnd,
            clear,
            list,
            groups,
        })
    }

    pub fn scroll(&self) -> f32 {
        self.scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification_toasts::EstimatedText;
    use gnos_notifications::CloseReason;
    use gnos_notifications::model::{ExpireTimeout, Hints, NotifyRequest, pair_actions};
    use gnos_notifications::store::{NotificationStore, StoreConfig};
    use std::time::Duration;

    const VIEW: (f32, f32) = (1280.0, 800.0);
    const BORDER: f32 = 10.0;

    struct Fixture {
        store: NotificationStore,
        host: CentreHost,
        now: Instant,
    }

    impl Fixture {
        fn new() -> Self {
            let mut f = Self {
                store: NotificationStore::new(StoreConfig::default()),
                host: CentreHost::new(),
                now: Instant::now(),
            };
            f.sync();
            f
        }

        fn notify(&mut self, app: &str, summary: &str, actions: &[&str]) -> NotificationId {
            let request = NotifyRequest {
                app_name: app.into(),
                replaces_id: 0,
                app_icon: String::new(),
                summary: summary.into(),
                body: format!("{summary} body"),
                actions: pair_actions(&actions.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()),
                hints: Hints::default(),
                expire_timeout: ExpireTimeout::ServerDefault,
            };
            let id = self.store.notify(request, 0).id;
            self.sync();
            id
        }

        fn sync(&mut self) -> bool {
            let snapshot = self.store.snapshot();
            self.host
                .sync(Some((&snapshot, 0)), VIEW, BORDER, &mut EstimatedText)
        }

        fn settle(&mut self) {
            self.host.tick(self.now, false);
            for _ in 0..300 {
                self.now += Duration::from_millis(16);
                if !self.host.tick(self.now, false) {
                    return;
                }
            }
            panic!("motion never settled");
        }

        fn apply(&mut self, outcome: &ToastOutcome) {
            for command in &outcome.commands {
                match command {
                    Command::Dismiss(id) => {
                        self.store.close(*id, CloseReason::Dismissed);
                    }
                    Command::Invoke(id, key) => {
                        self.store.invoke(*id, key);
                    }
                    Command::ClearAll => {
                        self.store.clear_all();
                    }
                    Command::SetDoNotDisturb(on) => {
                        self.store.set_do_not_disturb(*on);
                    }
                    Command::SetCentreOpen(open) => {
                        self.store.set_centre_open(*open);
                    }
                    _ => {}
                }
            }
            self.sync();
        }

        fn open(&mut self) {
            let edge = VIEW.0 - 4.0;
            assert!(
                self.host
                    .pointer_down(edge, 300.0, PointerButton::Primary)
                    .consumed
            );
            let outcome = self.host.pointer_moved(edge - 90.0, 300.0);
            assert!(outcome.commands.contains(&Command::SetCentreOpen(true)));
            self.apply(&outcome);
            self.host
                .pointer_up(edge - 90.0, 300.0, PointerButton::Primary);
            self.settle();
        }

        fn frame(&self) -> CentreFrame {
            self.host.frame().expect("centre visible")
        }

        fn click(&mut self, x: f32, y: f32) -> ToastOutcome {
            let mut a = self.host.pointer_down(x, y, PointerButton::Primary);
            let b = self.host.pointer_up(x, y, PointerButton::Primary);
            a.commands.extend(b.commands);
            a.changed |= b.changed;
            self.apply(&a);
            a
        }
    }

    fn centre(r: Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    #[test]
    fn a_horizontal_bar_shortens_the_centre_and_the_strip_to_meet_it() {
        let mut f = Fixture::new();
        f.notify("mail", "Hello", &[]);
        f.host.set_vertical_insets(Some((60.0, BORDER)));
        f.sync();
        let strip = f.host.input_rects()[0];
        assert_eq!((strip.y, strip.y + strip.height), (50.0, VIEW.1));
        f.host
            .pointer_down(VIEW.0 - 3.0, 400.0, PointerButton::Primary);
        f.host.pointer_moved(VIEW.0 - 60.0, 400.0);
        f.host.pointer_moved(VIEW.0 - 120.0, 400.0);
        f.host
            .pointer_up(VIEW.0 - 120.0, 400.0, PointerButton::Primary);
        f.settle();
        let frame = f.host.frame().expect("open");
        assert_eq!(frame.panel.y, 60.0, "the top meets the Bar's inner edge");
        assert_eq!(frame.panel.y + frame.panel.height, VIEW.1 - BORDER);
    }

    #[test]
    fn with_the_rail_on_the_right_the_centre_opens_from_the_left_border() {
        let mut f = Fixture::new();
        f.notify("mail", "Hello", &[]);
        f.host.set_on_left(true);
        f.sync();
        // A press on the right border no longer opens it; the left one does.
        assert!(
            !f.host
                .pointer_down(VIEW.0 - 3.0, 400.0, PointerButton::Primary)
                .consumed
        );
        f.host
            .pointer_up(VIEW.0 - 3.0, 400.0, PointerButton::Primary);
        assert!(
            f.host
                .pointer_down(3.0, 400.0, PointerButton::Primary)
                .consumed
        );
        f.host.pointer_moved(60.0, 400.0);
        let outcome = f.host.pointer_moved(120.0, 400.0);
        assert!(outcome.commands.contains(&Command::SetCentreOpen(true)));
        f.host.pointer_up(120.0, 400.0, PointerButton::Primary);
        f.settle();
        let frame = f.host.frame().expect("open");
        assert_eq!(frame.panel.x, BORDER);
        assert!(frame.on_left);
        assert_eq!(frame.clip_left, BORDER);
        // A drag left from a free area closes it.
        let free = (frame.list.x + 40.0, frame.list.y + frame.list.height - 20.0);
        f.host.pointer_down(free.0, free.1, PointerButton::Primary);
        let outcome = f.host.pointer_moved(free.0 - 100.0, free.1);
        assert!(outcome.commands.contains(&Command::SetCentreOpen(false)));
    }

    #[test]
    fn groups_by_app_newest_first_with_highest_urgency() {
        let mut store = NotificationStore::new(StoreConfig::default());
        let req = |app: &str, urgency| NotifyRequest {
            app_name: app.into(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: "s".into(),
            body: String::new(),
            actions: vec![],
            hints: Hints {
                urgency: Some(urgency),
                ..Hints::default()
            },
            expire_timeout: ExpireTimeout::ServerDefault,
        };
        store.notify(req("mail", Urgency::Normal), 0);
        store.notify(req("chat", Urgency::Low), 0);
        store.notify(req("mail", Urgency::Critical), 0);
        let groups = group_snapshot(&store.snapshot(), 0);
        let names: Vec<_> = groups.iter().map(|g| g.app_name.as_str()).collect();
        assert_eq!(names, ["mail", "chat"]);
        assert_eq!(groups[0].entries.len(), 2);
        assert_eq!(groups[0].urgency, Urgency::Critical);
        assert_eq!(groups[1].urgency, Urgency::Low);
    }

    #[test]
    fn edge_drag_opens_hides_toasts_and_drawer_meets_the_frame() {
        let mut f = Fixture::new();
        f.notify("mail", "hello", &[]);
        assert!(f.host.frame().is_none(), "closed centre paints nothing");
        assert_eq!(f.store.snapshot().toasts().count(), 1);
        // A short drag does not open.
        let edge = VIEW.0 - 4.0;
        f.host.pointer_down(edge, 300.0, PointerButton::Primary);
        assert!(f.host.pointer_moved(edge - 40.0, 300.0).commands.is_empty());
        f.host
            .pointer_up(edge - 40.0, 300.0, PointerButton::Primary);
        f.open();
        assert_eq!(f.store.snapshot().toasts().count(), 0, "N14");
        let frame = f.frame();
        assert_eq!(frame.panel.x, VIEW.0 - BORDER - CENTRE_WIDTH);
        assert_eq!(frame.panel.y, BORDER);
        assert_eq!(frame.panel.height, VIEW.1 - 2.0 * BORDER);
        assert_eq!(frame.container.x, frame.panel.x + 16.0);
        assert_eq!(frame.count, 1);
        assert!(frame.clear.is_some());
        // Open: input covers the output so a press outside closes it.
        assert_eq!(f.host.input_rects(), [Rect::new(0.0, 0.0, VIEW.0, VIEW.1)]);
    }

    #[test]
    fn press_outside_or_drag_right_closes() {
        let mut f = Fixture::new();
        f.notify("mail", "hello", &[]);
        f.open();
        let outcome = f.host.pointer_down(200.0, 300.0, PointerButton::Primary);
        assert!(outcome.consumed);
        assert_eq!(outcome.commands, [Command::SetCentreOpen(false)]);
        f.apply(&outcome);
        f.host.pointer_up(200.0, 300.0, PointerButton::Primary);
        f.settle();
        assert!(f.host.frame().is_none());
        f.open();
        // Drag right from the free title area.
        let title = f.frame().title;
        let (x, y) = centre(title);
        f.host.pointer_down(x, y, PointerButton::Primary);
        let outcome = f.host.pointer_moved(x + 90.0, y);
        assert_eq!(outcome.commands, [Command::SetCentreOpen(false)]);
        f.apply(&outcome);
        f.host.pointer_up(x + 90.0, y, PointerButton::Primary);
        // An empty centre has no list tree; its list area still closes.
        f.store.clear_all();
        f.sync();
        f.open();
        let (x, y) = centre(f.frame().list);
        f.host.pointer_down(x, y, PointerButton::Primary);
        let outcome = f.host.pointer_moved(x + 90.0, y);
        assert_eq!(outcome.commands, [Command::SetCentreOpen(false)]);
    }

    #[test]
    fn collapsed_group_previews_three_and_pill_expands_with_actions() {
        let mut f = Fixture::new();
        for n in 0..5 {
            f.notify("mail", &format!("m{n}"), &["open", "Open"]);
        }
        f.open();
        let group = f.frame().groups[0].clone();
        assert!(!group.expanded);
        assert_eq!(group.entries.len(), 3, "groupPreviewNum");
        assert_eq!(group.entries[0].entry.summary, "m4");
        let (x, y) = centre(group.pill);
        f.click(x, y);
        f.sync();
        f.settle();
        let group = f.frame().groups[0].clone();
        assert!(group.expanded);
        assert_eq!(group.entries.len(), 5);
        let open = group.entries[0]
            .buttons
            .iter()
            .find(|b| {
                b.kind
                    == ButtonKind::Action {
                        label: "Open".into(),
                    }
            })
            .unwrap()
            .rect;
        let id = group.entries[0].entry.id;
        let (x, y) = centre(open);
        let outcome = f.click(x, y);
        assert!(
            outcome
                .commands
                .contains(&Command::Invoke(id, "open".into())),
            "{outcome:?}"
        );
        assert!(f.store.snapshot().get(id).is_none());
    }

    #[test]
    fn group_swipe_closes_the_app_and_entry_swipe_closes_one() {
        let mut f = Fixture::new();
        f.notify("chat", "c1", &[]);
        f.notify("mail", "m1", &[]);
        f.notify("mail", "m2", &[]);
        f.open();
        let group = f.frame().groups[0].clone();
        assert_eq!(group.group.app_name, "mail");
        let (x, y) = (group.rect.x + 30.0, group.rect.y + group.rect.height - 6.0);
        f.host.pointer_down(x, y, PointerButton::Primary);
        f.host.pointer_moved(x + 40.0, y);
        assert_eq!(f.frame().groups[0].rect.x, group.rect.x + 40.0);
        f.host.pointer_moved(x + 160.0, y + 3.0);
        let outcome = f
            .host
            .pointer_up(x + 160.0, y + 3.0, PointerButton::Primary);
        f.apply(&outcome);
        let left: Vec<_> = f
            .store
            .snapshot()
            .notifications
            .iter()
            .map(|n| n.summary.clone())
            .collect();
        assert_eq!(left, ["c1"]);
        f.settle();
        assert_eq!(f.frame().groups.len(), 1);
        // Expand chat via right press, then swipe its entry left.
        f.notify("chat", "c2", &[]);
        f.settle();
        let group = f.frame().groups[0].clone();
        let (x, y) = centre(group.rect);
        f.host.pointer_down(x, y, PointerButton::Secondary);
        f.host.pointer_up(x, y, PointerButton::Secondary);
        f.sync();
        f.settle();
        let entry = f.frame().groups[0].entries[1].clone();
        assert!(entry.expanded);
        let (x, y) = (entry.rect.x + 100.0, entry.rect.y + 14.0);
        f.host.pointer_down(x, y, PointerButton::Primary);
        f.host.pointer_moved(x - 50.0, y);
        let outcome = f.host.pointer_up(x - 140.0, y, PointerButton::Primary);
        assert_eq!(outcome.commands, [Command::Dismiss(entry.entry.id)]);
    }

    #[test]
    fn middle_press_closes_group_and_vertical_drag_expands() {
        let mut f = Fixture::new();
        f.notify("chat", "c1", &[]);
        f.notify("mail", "m1", &[]);
        f.open();
        let mail = f.frame().groups[0].rect;
        let (x, y) = centre(mail);
        let outcome = f.host.pointer_down(x, y, PointerButton::Middle);
        f.host.pointer_up(x, y, PointerButton::Middle);
        f.apply(&outcome);
        assert_eq!(f.store.snapshot().notifications.len(), 1);
        f.settle();
        let chat = f.frame().groups[0].rect;
        let (x, y) = (chat.x + 30.0, chat.y + 20.0);
        f.host.pointer_down(x, y, PointerButton::Primary);
        f.host.pointer_moved(x + 2.0, y + 30.0);
        f.host.pointer_up(x + 2.0, y + 30.0, PointerButton::Primary);
        f.sync();
        assert!(f.frame().groups[0].expanded);
    }

    #[test]
    fn dnd_toggle_clear_all_and_empty_state() {
        let mut f = Fixture::new();
        f.notify("mail", "m1", &[]);
        f.notify("chat", "c1", &[]);
        f.open();
        let (x, y) = centre(f.frame().dnd);
        let outcome = f.click(x, y);
        assert_eq!(outcome.commands, [Command::SetDoNotDisturb(true)]);
        assert!(f.frame().do_not_disturb);
        let (x, y) = centre(f.frame().clear.unwrap());
        let outcome = f.click(x, y);
        assert_eq!(outcome.commands, [Command::ClearAll]);
        assert!(f.store.snapshot().notifications.is_empty());
        f.settle();
        let frame = f.frame();
        assert_eq!(frame.count, 0);
        assert!(frame.groups.is_empty() && frame.clear.is_none());
    }

    #[test]
    fn wheel_scrolls_a_long_history_within_bounds() {
        let mut f = Fixture::new();
        for n in 0..14 {
            f.notify(&format!("app{n}"), "s", &[]);
        }
        f.open();
        let list = f.frame().list;
        let (x, y) = centre(list);
        for _ in 0..100 {
            f.host.wheel(x, y, 1.0);
        }
        let max = f.host.scroll();
        assert!(max > 0.0);
        let last = f.frame().groups.last().unwrap().rect;
        assert!(
            (last.y + last.height - (list.y + list.height)).abs() < 0.5,
            "last group ends at the list bottom"
        );
        f.host.wheel(x, y, -1.0);
        assert_eq!(f.host.scroll(), max - WHEEL_STEP);
    }
}
