//! UXE-02B: notification toasts, headless.
//!
//! The toast column is a drawer attached to the top-right corner of the shell
//! frame (N09), newest card first. The interactive body is a `blobin-ui-core`
//! tree:
//! - the router owns hit testing, hover and activation;
//! - the gesture coordinator owns card drags, split by `AxisLock` into a
//!   horizontal swipe that hides the toast (N07) and a vertical drag that
//!   expands or collapses it, applied on release.
//!
//! Hover and press hold the toast timer, leaving releases it (N04). Every
//! effect is a typed `gnos_notifications::Command` that the service
//! revalidates. Motion is geometric only: cards slide in from the frame edge
//! and slide out, and the drawer height follows its content.

use blobin_ui_core::gestures::{
    AxisLock, CancelReason, DragAxis, GestureAction, GestureCoordinator,
};
use blobin_ui_core::input::{EventKind, InputEvent, InputReceipt, InputRouter, Key, Point, Units};
use blobin_ui_core::layout::{
    self, Axis, Insets, LayoutSnapshot, LayoutStyles, Length, Rect, Size, Style, Viewport,
};
use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiTree};
use gnos_notifications::{
    Action, Command, Notification, NotificationId, NotificationSnapshot, Urgency,
};
use gnos_ux_surface::grammar::{Follow, TemporalResponseMode};
use std::collections::BTreeMap;
use std::time::Instant;

/// Reference `sizes.notifs.width`.
pub const PANEL_WIDTH: f32 = 430.0;
/// Reference `padding.large`, the drawer's inner margin.
const PANEL_PADDING: f32 = 16.0;
/// Reference `padding.medium`, a card's inner margin.
pub const CARD_PADDING: f32 = 12.0;
/// Reference `rounding.large`.
pub const CARD_RADIUS: f32 = 16.0;
/// Reference `spacing.medium`, between cards and beside the image.
const CARD_GAP: f32 = 12.0;
/// Reference `sizes.notifs.image`.
pub const IMAGE_SIZE: f32 = 42.0;
/// Reference `notifs.clearThreshold`.
const CLEAR_THRESHOLD: f32 = 0.3;
/// Reference `notifs.expandThreshold`.
const EXPAND_THRESHOLD: f32 = 20.0;
/// Gnosix: travel before a drag commits to an axis.
const AXIS_SLOP: f32 = 6.0;
const EXTRA_SMALL: f32 = 4.0;
const SMALL: f32 = 8.0;
pub const SUMMARY_PX: f32 = 14.0;
pub const SUMMARY_LINE: f32 = 20.0;
pub const BODY_PX: f32 = 12.0;
pub const BODY_LINE: f32 = 17.0;
pub const APP_LINE: f32 = 16.0;
pub const ACTION_ROW: f32 = 32.0;
pub const EXPAND_BUTTON: f32 = 24.0;
/// Bodies longer than this many lines are clipped when expanded.
const MAX_BODY_LINES: usize = 12;
const MAX_SUMMARY_LINES: usize = 3;
const MOTION_TIME_CONSTANT_S: f32 = 0.08;
const SETTLE_PX: f32 = 0.25;
const POINTER: u64 = 1;
const PRIMARY: u8 = 1;

/// Left edge of a card's text column, card-local.
pub const TEXT_X: f32 = CARD_PADDING + IMAGE_SIZE + CARD_GAP;

/// Width of a card's text column: right of the image, left of the expand
/// button. Measurement and paint both use it.
pub fn text_width(card_width: f32) -> f32 {
    (card_width - TEXT_X - CARD_PADDING - EXPAND_BUTTON - SMALL).max(0.0)
}

/// Wrapped line counts. The scene supplies GPUI's line wrapper, so painted
/// text and hit-tested card heights agree; tests use `EstimatedText`.
pub trait TextMeasure {
    fn lines(&mut self, text: &str, font_px: f32, width: f32) -> usize;
}

/// Character-count estimate (0.55 em advance) for headless use.
pub struct EstimatedText;

impl TextMeasure for EstimatedText {
    fn lines(&mut self, text: &str, font_px: f32, width: f32) -> usize {
        let per_line = ((width / (font_px * 0.55)).floor() as usize).max(1);
        text.split('\n')
            .map(|paragraph| paragraph.chars().count().div_ceil(per_line).max(1))
            .sum()
    }
}

/// Reference age string: `now`, then minutes, hours, days.
pub fn age_label(age_ms: u64) -> String {
    let minutes = age_ms / 60_000;
    let (hours, days) = (minutes / 60, minutes / 60 / 24);
    match () {
        _ if minutes < 1 => "now".to_owned(),
        _ if days > 0 => format!("{days}d"),
        _ if hours > 0 => format!("{hours}h"),
        _ => format!("{minutes}m"),
    }
}

/// What one card shows; taken from the service snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct ToastCard {
    pub id: NotificationId,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub actions: Vec<Action>,
    pub age: String,
    pub progress: Option<u8>,
    /// Stand-in for an image until images are rendered (G04).
    pub badge: char,
}

impl ToastCard {
    pub fn from_notification(n: &Notification, now_ms: u64) -> Self {
        let badge = n
            .app_name
            .chars()
            .chain(n.summary.chars())
            .find(|c| c.is_alphanumeric())
            .map_or('•', |c| c.to_uppercase().next().unwrap_or(c));
        Self {
            id: n.id,
            app_name: n.app_name.clone(),
            summary: n.summary.clone(),
            body: n.body.clone(),
            urgency: n.urgency,
            actions: n.actions.clone(),
            age: age_label(now_ms.saturating_sub(n.received_ms)),
            progress: n.progress,
            badge,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ButtonKind {
    Expand { expanded: bool },
    Close,
    Action { label: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ButtonPaint {
    pub kind: ButtonKind,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardPaint {
    pub card: ToastCard,
    pub expanded: bool,
    /// Displayed card, screen coordinates.
    pub rect: Rect,
    pub summary_lines: usize,
    pub body_lines: usize,
    pub buttons: Vec<ButtonPaint>,
    pub leaving: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToastFrame {
    /// Displayed drawer, screen coordinates; its top and right edges meet
    /// the frame's inner edges.
    pub panel: Rect,
    /// Cards are clipped to this rectangle.
    pub clip: Rect,
    pub cards: Vec<CardPaint>,
    /// TOPO-01A: the drawer sits in the top-left corner.
    pub on_left: bool,
    /// TOPO-01C: the drawer sits in a bottom corner (a top Bar took the
    /// top edge), flush with the bottom border.
    pub on_bottom: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct ToastOutcome {
    pub changed: bool,
    pub consumed: bool,
    pub commands: Vec<Command>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Card,
    Expand,
    Close,
    Action(usize),
}

#[derive(Debug)]
struct Card {
    model: ToastCard,
    expanded: bool,
    height: f32,
    summary_lines: usize,
    body_lines: usize,
    /// Displayed top, drawer-local.
    y: f32,
    target_y: f32,
    /// Displayed horizontal offset; 0 at rest.
    x: f32,
    /// Swipe preview on top of `x`.
    drag_x: f32,
    /// Set once a swipe committed: the card keeps sliding this way.
    exit: Option<f32>,
}

#[derive(Debug)]
struct Leaving {
    card: Card,
    direction: f32,
}

#[derive(Debug)]
struct Drag {
    card: NotificationId,
    lock: AxisLock,
}

struct Surface {
    tree: UiTree,
    styles: LayoutStyles,
    layout: LayoutSnapshot,
    viewport: Viewport,
    controls: BTreeMap<NodeId, (NotificationId, Control)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Geometry {
    viewport: (f32, f32),
    border: f32,
    /// TOPO-01A: the drawer sits in the top-left corner (the rail took the
    /// right edge).
    on_left: bool,
    /// TOPO-01C: the drawer sits in a bottom corner and grows upward.
    on_bottom: bool,
    /// Inset of the edge opposite the drawer's anchor edge (a Bar there is
    /// thicker than the border).
    far: f32,
    /// BAR-01H (B07): how far the drawer stands off its anchor edge, on the
    /// Deck that shares that edge on a narrow output.
    base: f32,
}

impl Geometry {
    fn inset(self) -> f32 {
        (PANEL_PADDING - self.border).clamp(0.0, PANEL_PADDING)
    }

    fn card_width(self) -> f32 {
        PANEL_WIDTH - PANEL_PADDING - self.inset()
    }

    fn text_width(self) -> f32 {
        text_width(self.card_width())
    }

    fn panel_x(self) -> f32 {
        if self.on_left {
            self.border
        } else {
            self.viewport.0 - self.border - PANEL_WIDTH
        }
    }

    /// Panel top for a displayed height: on the top border, or standing on
    /// the bottom border and growing upward.
    fn panel_y(self, height: f32) -> f32 {
        if self.on_bottom {
            self.viewport.1 - self.border - self.base - height
        } else {
            self.border + self.base
        }
    }

    /// Cards sit `inset` from the anchor edge and `PANEL_PADDING` from the
    /// free one; anchored at the bottom, that is a shift down.
    fn content_dy(self) -> f32 {
        if self.on_bottom {
            PANEL_PADDING - self.inset()
        } else {
            0.0
        }
    }

    /// Toward the drawer's edge: where cards enter from and leave to.
    fn outward(self) -> f32 {
        if self.on_left { -1.0 } else { 1.0 }
    }

    fn max_panel_height(self) -> f32 {
        (self.viewport.1 - self.border - self.far - self.base).max(0.0)
    }
}

pub struct ToastHost {
    geometry: Option<Geometry>,
    revision: Option<u64>,
    cards: Vec<Card>,
    leaving: Vec<Leaving>,
    /// Drawer height, extruding from the top border on the envelope's
    /// critically damped follower.
    panel_height: f32,
    panel_velocity: f32,
    surface: Option<Surface>,
    rebuild: bool,
    remeasure: bool,
    /// The side requested for the next sync (TOPO-01A).
    pending_on_left: bool,
    /// The bottom corner and far inset requested for the next sync (TOPO-01C).
    pending_on_bottom: Option<f32>,
    pending_base: f32,
    router: InputRouter,
    gestures: GestureCoordinator,
    hovered: Option<NotificationId>,
    pressed: Option<NotificationId>,
    drag: Option<Drag>,
    serial: u64,
    epoch: Instant,
    last_micros: u64,
    last_tick: Option<Instant>,
}

impl Default for ToastHost {
    fn default() -> Self {
        Self::new()
    }
}

const SURFACE: SurfaceId = SurfaceId(3);

impl ToastHost {
    pub fn new() -> Self {
        Self {
            geometry: None,
            revision: None,
            cards: Vec::new(),
            leaving: Vec::new(),
            panel_height: 0.0,
            panel_velocity: 0.0,
            surface: None,
            rebuild: false,
            remeasure: false,
            pending_on_left: false,
            pending_on_bottom: None,
            pending_base: 0.0,
            router: InputRouter::new(),
            gestures: GestureCoordinator::new(),
            hovered: None,
            pressed: None,
            drag: None,
            serial: 0,
            epoch: Instant::now(),
            last_micros: 0,
            last_tick: None,
        }
    }

    fn measure_card(card: &mut Card, geometry: Geometry, measure: &mut dyn TextMeasure) {
        let text = geometry.text_width();
        let inner = if card.expanded {
            card.summary_lines = measure
                .lines(&card.model.summary, SUMMARY_PX, text)
                .clamp(1, MAX_SUMMARY_LINES);
            card.body_lines = if card.model.body.is_empty() {
                0
            } else {
                measure
                    .lines(&card.model.body, BODY_PX, text)
                    .clamp(1, MAX_BODY_LINES)
            };
            APP_LINE
                + 2.0 * EXTRA_SMALL
                + SUMMARY_LINE * card.summary_lines as f32
                + BODY_LINE * card.body_lines as f32
                + SMALL
                + ACTION_ROW
        } else {
            card.summary_lines = 1;
            card.body_lines = 1;
            SUMMARY_LINE + BODY_LINE
        };
        card.height = 2.0 * CARD_PADDING + inner.max(IMAGE_SIZE);
    }

    fn content_height(&self) -> f32 {
        let Some(geometry) = self.geometry else {
            return 0.0;
        };
        if self.cards.is_empty() {
            return 0.0;
        }
        let cards: f32 = self.cards.iter().map(|c| c.height).sum();
        (cards + CARD_GAP * (self.cards.len() - 1) as f32 + PANEL_PADDING + geometry.inset())
            .min(geometry.max_panel_height())
    }

    fn place_cards(&mut self) {
        let Some(geometry) = self.geometry else {
            return;
        };
        let mut y = geometry.inset();
        for card in &mut self.cards {
            card.target_y = y;
            y += card.height + CARD_GAP;
        }
    }

    /// Applies the latest service snapshot. Returns whether anything visible
    /// changed. Cheap when the revision and geometry are unchanged.
    /// TOPO-01A: put the drawer in the top-left corner when the rail takes
    /// the right edge. Takes effect at the next sync.
    pub fn set_on_left(&mut self, on_left: bool) {
        self.pending_on_left = on_left;
    }

    /// TOPO-01C: stand the drawer in a bottom corner when a top Bar takes the
    /// top edge; `far_inset` is that Bar's thickness. `None` keeps it on the
    /// top border. Takes effect at the next sync.
    pub fn set_on_bottom(&mut self, far_inset: Option<f32>) {
        self.pending_on_bottom = far_inset;
    }

    /// BAR-01H (B07): stand the drawer this far off its anchor edge (on the
    /// Deck that shares it). Takes effect at the next sync.
    pub fn set_base(&mut self, base: f32) {
        self.pending_base = if base.is_finite() { base.max(0.0) } else { 0.0 };
    }

    /// Whether the drawer shows anything (a panel to stand on the Deck).
    pub fn has_panel(&self) -> bool {
        self.panel_rect().is_some()
    }

    pub fn sync(
        &mut self,
        update: Option<(&NotificationSnapshot, u64)>,
        viewport: (f32, f32),
        border: f32,
        measure: &mut dyn TextMeasure,
    ) -> bool {
        let geometry = Geometry {
            viewport,
            border,
            on_left: self.pending_on_left,
            on_bottom: self.pending_on_bottom.is_some(),
            far: self.pending_on_bottom.unwrap_or(border),
            base: self.pending_base,
        };
        let geometry_changed = self.geometry != Some(geometry);
        let revision = update.map(|(snapshot, _)| snapshot.revision);
        if !geometry_changed && revision == self.revision {
            if !std::mem::take(&mut self.remeasure) {
                return false;
            }
            self.measure_all(measure);
            return true;
        }
        self.geometry = Some(geometry);
        self.revision = revision;
        let toasts: Vec<ToastCard> = update.map_or_else(Vec::new, |(snapshot, now_ms)| {
            snapshot
                .toasts()
                .map(|n| ToastCard::from_notification(n, now_ms))
                .collect()
        });
        let mut previous: Vec<Card> = std::mem::take(&mut self.cards);
        for model in toasts {
            let card = match previous.iter().position(|c| c.model.id == model.id) {
                Some(index) => {
                    let mut card = previous.remove(index);
                    card.model = model;
                    card
                }
                None => Card {
                    model,
                    expanded: false,
                    height: 0.0,
                    summary_lines: 1,
                    body_lines: 1,
                    y: f32::NAN,
                    target_y: 0.0,
                    // Enters sliding in from the frame edge.
                    x: geometry.outward() * geometry.card_width(),
                    drag_x: 0.0,
                    exit: None,
                },
            };
            self.cards.push(card);
        }
        for mut card in previous {
            let shown = card.x + card.drag_x;
            card.x = shown;
            card.drag_x = 0.0;
            // Toward the edge unless the user swiped it the other way.
            let outward = geometry.outward();
            let direction = card.exit.unwrap_or(if shown * outward < 0.0 {
                -outward
            } else {
                outward
            });
            if self.drag.as_ref().is_some_and(|d| d.card == card.model.id) {
                self.drag = None;
                self.gestures
                    .cancel_surface(SURFACE, CancelReason::SurfaceClosed);
            }
            if self.hovered == Some(card.model.id) {
                self.hovered = None;
            }
            if self.pressed == Some(card.model.id) {
                self.pressed = None;
            }
            self.leaving.push(Leaving { card, direction });
        }
        self.remeasure = false;
        self.measure_all(measure);
        true
    }

    fn measure_all(&mut self, measure: &mut dyn TextMeasure) {
        let Some(geometry) = self.geometry else {
            return;
        };
        for card in &mut self.cards {
            Self::measure_card(card, geometry, measure);
        }
        self.place_cards();
        for card in &mut self.cards {
            if card.y.is_nan() {
                card.y = card.target_y;
            }
        }
        self.rebuild = true;
        self.rebuild_if_idle();
    }

    fn rebuild_if_idle(&mut self) {
        if !self.rebuild || self.drag.is_some() {
            return;
        }
        self.rebuild = false;
        self.surface = self.build_surface();
        // Hover is geometric; it is re-derived on the next pointer event.
    }

    fn build_surface(&self) -> Option<Surface> {
        let geometry = self.geometry?;
        if self.cards.is_empty() {
            return None;
        }
        let mut tree = UiTree::new();
        let mut placed: Vec<(NodeId, Style)> = Vec::new();
        let mut controls = BTreeMap::new();
        let fixed_row = |height: f32| Style {
            height: Length::Px(height),
            axis: Axis::Row,
            ..Style::default()
        };
        let grow = Style {
            flex_grow: 1.0,
            ..Style::default()
        };
        let square = |size: f32| Style {
            width: Length::Px(size),
            height: Length::Px(size),
            flex_shrink: 0.0,
            ..Style::default()
        };
        tree.transact(|tx| {
            tx.open_surface(SURFACE)?;
            let root = tx.append_root(SURFACE, NodeProperties::new(Role::Container, "toasts"))?;
            placed.push((
                root,
                Style {
                    padding: Insets {
                        left: PANEL_PADDING,
                        top: geometry.inset(),
                        right: geometry.inset(),
                        bottom: PANEL_PADDING,
                    },
                    gap: CARD_GAP,
                    clip_children: true,
                    ..Style::fill()
                },
            ));
            for card in &self.cards {
                let id = card.model.id;
                let node = tx.append_child(
                    root,
                    NodeProperties::new(Role::Custom("toast".into()), card.model.summary.clone()),
                )?;
                placed.push((
                    node,
                    Style {
                        height: Length::Px(card.height),
                        padding: Insets::uniform(CARD_PADDING),
                        flex_shrink: 0.0,
                        clip_children: true,
                        ..Style::default()
                    },
                ));
                controls.insert(node, (id, Control::Card));
                let header =
                    tx.append_child(node, NodeProperties::new(Role::Container, "header"))?;
                placed.push((header, fixed_row(EXPAND_BUTTON)));
                let spacer = tx.append_child(header, NodeProperties::new(Role::Container, ""))?;
                placed.push((spacer, grow.clone()));
                let expand = tx.append_child(
                    header,
                    NodeProperties::new(
                        Role::Button,
                        if card.expanded {
                            gnos_i18n::t("Collapse")
                        } else {
                            gnos_i18n::t("Expand")
                        },
                    ),
                )?;
                placed.push((expand, square(EXPAND_BUTTON)));
                controls.insert(expand, (id, Control::Expand));
                if card.expanded {
                    let filler = tx.append_child(node, NodeProperties::new(Role::Container, ""))?;
                    placed.push((filler, grow.clone()));
                    let row =
                        tx.append_child(node, NodeProperties::new(Role::Container, "actions"))?;
                    placed.push((
                        row,
                        Style {
                            padding: Insets {
                                left: IMAGE_SIZE + CARD_GAP,
                                top: 0.0,
                                right: EXPAND_BUTTON + SMALL,
                                bottom: 0.0,
                            },
                            gap: EXTRA_SMALL,
                            ..fixed_row(ACTION_ROW)
                        },
                    ));
                    let close = tx.append_child(
                        row,
                        NodeProperties::new(Role::Button, gnos_i18n::t("Close")),
                    )?;
                    // Alone, the close button fills the row (reference).
                    placed.push((
                        close,
                        if card.model.actions.is_empty() {
                            grow.clone()
                        } else {
                            square(ACTION_ROW)
                        },
                    ));
                    controls.insert(close, (id, Control::Close));
                    for (index, action) in card.model.actions.iter().enumerate() {
                        let button = tx.append_child(
                            row,
                            NodeProperties::new(Role::Button, action.label.clone()),
                        )?;
                        placed.push((button, grow.clone()));
                        controls.insert(button, (id, Control::Action(index)));
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
        let content: f32 = self.cards.iter().map(|c| c.height).sum::<f32>()
            + CARD_GAP * (self.cards.len() - 1) as f32
            + PANEL_PADDING
            + geometry.inset();
        let viewport = Viewport::new(Size::new(PANEL_WIDTH, content), 1.0, 1);
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

    /// Displayed panel origin.
    fn panel_origin(&self) -> Option<(f32, f32)> {
        let g = self.geometry?;
        Some((g.panel_x(), g.panel_y(self.panel_height)))
    }

    /// Origin of the card layout (the router's space).
    fn content_origin(&self) -> Option<(f32, f32)> {
        let (ox, oy) = self.panel_origin()?;
        Some((ox, oy + self.geometry?.content_dy()))
    }

    fn local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let (ox, oy) = self.content_origin()?;
        Some((x - ox, y - oy))
    }

    fn panel_contains(&self, x: f32, y: f32) -> bool {
        self.panel_origin().is_some_and(|(ox, oy)| {
            (0.0..=PANEL_WIDTH).contains(&(x - ox)) && (0.0..=self.panel_height).contains(&(y - oy))
        })
    }

    /// Routes one event; returns the receipt and any gesture actions.
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

    /// CURSOR-01: what the pointer is over among the toasts (`None`: off
    /// them), from the layout's own hit test (no event is sent): a button
    /// it can press; the card itself does nothing on a click.
    pub fn affordance_at(&self, x: f32, y: f32) -> Option<crate::cursor::Affordance> {
        if !self.panel_contains(x, y) {
            return None;
        }
        let (lx, ly) = self.local(x, y)?;
        let surface = self.surface.as_ref()?;
        let route = InputRouter::hit_test(
            &surface.tree.snapshot(),
            &surface.styles,
            &surface.layout,
            surface.viewport,
            Point::new(lx, ly),
        )
        .ok()
        .flatten();
        Some(match route.and_then(|r| self.control(r.target)) {
            Some((_, Control::Card)) | None => crate::cursor::Affordance::Inert,
            Some(_) => crate::cursor::Affordance::Press,
        })
    }

    fn control(&self, node: NodeId) -> Option<(NotificationId, Control)> {
        self.surface.as_ref()?.controls.get(&node).copied()
    }

    fn card_mut(&mut self, id: NotificationId) -> Option<&mut Card> {
        self.cards.iter_mut().find(|c| c.model.id == id)
    }

    fn set_hovered(&mut self, next: Option<NotificationId>, outcome: &mut ToastOutcome) {
        if self.hovered == next {
            return;
        }
        if let Some(previous) = self.hovered
            && self.pressed != Some(previous)
        {
            outcome.commands.push(Command::ReleaseToast(previous));
        }
        if let Some(next) = next {
            outcome.commands.push(Command::HoldToast(next));
        }
        self.hovered = next;
    }

    fn hovered_card(&self) -> Option<NotificationId> {
        let node = self.router.hover(SURFACE, POINTER)?;
        self.control(node).map(|(id, _)| id)
    }

    fn apply_gestures(&mut self, actions: Vec<GestureAction>, outcome: &mut ToastOutcome) {
        let card_width = self.geometry.map_or(PANEL_WIDTH, Geometry::card_width);
        for action in actions {
            let Some((id, Control::Card)) = self.control(action.target()) else {
                continue;
            };
            match action {
                GestureAction::Begin { .. } => {
                    self.drag = Some(Drag {
                        card: id,
                        lock: AxisLock::new(AXIS_SLOP),
                    });
                }
                GestureAction::Preview { delta, .. } => {
                    let Some(drag) = self.drag.as_mut().filter(|d| d.card == id) else {
                        continue;
                    };
                    let projected = drag.lock.project(delta);
                    if let Some(card) = self.card_mut(id)
                        && card.drag_x != projected.x
                    {
                        card.drag_x = projected.x;
                        outcome.changed = true;
                    }
                }
                GestureAction::Commit { delta, .. } => {
                    let Some(mut drag) = self.drag.take().filter(|d| d.card == id) else {
                        continue;
                    };
                    let projected = drag.lock.project(delta);
                    let axis = drag.lock.locked();
                    let Some(card) = self.card_mut(id) else {
                        continue;
                    };
                    card.x += card.drag_x;
                    card.drag_x = 0.0;
                    outcome.changed = true;
                    match axis {
                        Some(DragAxis::Horizontal)
                            if projected.x.abs() >= CLEAR_THRESHOLD * card_width =>
                        {
                            card.exit = Some(projected.x.signum());
                            outcome.commands.push(Command::HideToast(id));
                        }
                        Some(DragAxis::Vertical) if projected.y.abs() > EXPAND_THRESHOLD => {
                            let expand = projected.y > 0.0;
                            if card.expanded != expand {
                                card.expanded = expand;
                                self.relayout();
                            }
                        }
                        // Below threshold: springs back to rest.
                        _ => {}
                    }
                }
                GestureAction::Cancel { reason, .. } => {
                    if self.drag.as_ref().is_some_and(|d| d.card == id) {
                        self.drag = None;
                    }
                    if reason != CancelReason::NoTravel
                        && let Some(card) = self.card_mut(id)
                    {
                        card.x += card.drag_x;
                        card.drag_x = 0.0;
                        outcome.changed = true;
                    }
                }
            }
        }
    }

    /// Heights depend on text measurement, which only `sync` has; the next
    /// sync re-measures and rebuilds.
    fn relayout(&mut self) {
        self.remeasure = true;
    }

    fn finish_press(&mut self, x: f32, y: f32, outcome: &mut ToastOutcome) {
        if let Some(pressed) = self.pressed.take() {
            let still_over = self.hovered == Some(pressed) && self.panel_contains(x, y);
            if !still_over {
                outcome.commands.push(Command::ReleaseToast(pressed));
            }
        }
        if self.rebuild {
            self.rebuild_if_idle();
            outcome.changed = true;
        }
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        let dragging = self.drag.is_some();
        if !dragging && !self.panel_contains(x, y) {
            if self.hovered.is_some() {
                let _ = self.route(EventKind::PointerLeave { pointer: POINTER });
                self.set_hovered(None, &mut outcome);
            }
            return outcome;
        }
        let Some((lx, ly)) = self.local(x, y) else {
            return outcome;
        };
        let Some((_, actions)) = self.route(EventKind::PointerMove {
            pointer: POINTER,
            position: Point::new(lx, ly),
            units: Units::Logical,
        }) else {
            return outcome;
        };
        self.apply_gestures(actions, &mut outcome);
        let hovered = self.hovered_card();
        self.set_hovered(hovered, &mut outcome);
        outcome.consumed = true;
        outcome
    }

    pub fn pointer_left(&mut self) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if self.drag.is_none() && self.hovered.is_some() {
            let _ = self.route(EventKind::PointerLeave { pointer: POINTER });
            self.set_hovered(None, &mut outcome);
        }
        outcome
    }

    /// Primary press starts a drag or a button press; middle press closes the
    /// notification under the pointer (N07).
    pub fn pointer_down(&mut self, x: f32, y: f32, middle: bool) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if !self.panel_contains(x, y) {
            return outcome;
        }
        outcome.consumed = true;
        let Some((lx, ly)) = self.local(x, y) else {
            return outcome;
        };
        let Some((receipt, actions)) = self.route(EventKind::PointerDown {
            pointer: POINTER,
            button: if middle { 2 } else { PRIMARY },
            position: Point::new(lx, ly),
            units: Units::Logical,
        }) else {
            return outcome;
        };
        let Some((id, _)) = receipt
            .route
            .as_ref()
            .and_then(|route| self.control(route.target))
        else {
            return outcome;
        };
        if middle {
            outcome.commands.push(Command::Dismiss(id));
            return outcome;
        }
        self.pressed = Some(id);
        outcome.commands.push(Command::HoldToast(id));
        self.apply_gestures(actions, &mut outcome);
        outcome
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, middle: bool) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        let Some((lx, ly)) = self.local(x, y) else {
            return outcome;
        };
        let captured = self.router.capture(SURFACE, POINTER).is_some();
        if !captured && !self.panel_contains(x, y) {
            return outcome;
        }
        outcome.consumed = true;
        let Some((receipt, actions)) = self.route(EventKind::PointerUp {
            pointer: POINTER,
            button: if middle { 2 } else { PRIMARY },
            position: Point::new(lx, ly),
            units: Units::Logical,
        }) else {
            return outcome;
        };
        if middle {
            return outcome;
        }
        self.apply_gestures(actions, &mut outcome);
        if let Some((id, control)) = receipt.activated.and_then(|node| self.control(node)) {
            match control {
                Control::Expand => {
                    if let Some(card) = self.card_mut(id) {
                        card.expanded = !card.expanded;
                        outcome.changed = true;
                    }
                    self.relayout();
                }
                Control::Close => outcome.commands.push(Command::Dismiss(id)),
                Control::Action(index) => {
                    if let Some(action) = self
                        .cards
                        .iter()
                        .find(|c| c.model.id == id)
                        .and_then(|c| c.model.actions.get(index))
                    {
                        outcome
                            .commands
                            .push(Command::Invoke(id, action.key.clone()));
                    }
                }
                // Reference default: clicking the card does nothing.
                Control::Card => {}
            }
        }
        let hovered = self.hovered_card();
        self.set_hovered(hovered, &mut outcome);
        self.finish_press(x, y, &mut outcome);
        outcome
    }

    /// Escape abandons a drag; the card springs back.
    pub fn escape(&mut self) -> ToastOutcome {
        let mut outcome = ToastOutcome::default();
        if self.drag.is_none() {
            return outcome;
        }
        if let Some((_, actions)) = self.route(EventKind::KeyDown {
            key: Key::Escape,
            shift: false,
            repeat: false,
        }) {
            self.apply_gestures(actions, &mut outcome);
        }
        if self.drag.take().is_some() {
            for card in &mut self.cards {
                card.x += card.drag_x;
                card.drag_x = 0.0;
            }
            outcome.changed = true;
        }
        outcome.consumed = true;
        outcome
    }

    fn exit_distance(&self) -> f32 {
        2.0 * self.geometry.map_or(PANEL_WIDTH, Geometry::card_width)
    }

    pub fn needs_frame(&self) -> bool {
        let exit = self.exit_distance();
        let unsettled = |value: f32, target: f32| (value - target).abs() > SETTLE_PX;
        !self.leaving.is_empty()
            || unsettled(self.panel_height, self.content_height())
            || self.panel_velocity != 0.0
            || self.cards.iter().any(|c| {
                let dragging = self.drag.as_ref().is_some_and(|d| d.card == c.model.id);
                unsettled(c.y, c.target_y)
                    || (!dragging && unsettled(c.x, c.exit.map_or(0.0, |d| d * exit)))
            })
    }

    /// Advances every slide toward its target; true while anything moves.
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
        let exit = self.exit_distance();
        let target_height = self.content_height();
        let mut follow = Follow {
            value: self.panel_height,
            velocity: self.panel_velocity,
        };
        follow.step(
            target_height,
            if dt > 0.0 { dt } else { 1.0 / 60.0 },
            if reduced {
                TemporalResponseMode::Instant
            } else {
                TemporalResponseMode::Full
            },
            SETTLE_PX,
        );
        self.panel_height = follow.value;
        self.panel_velocity = follow.velocity;
        let dragged = self.drag.as_ref().map(|d| d.card);
        for card in &mut self.cards {
            approach(&mut card.y, card.target_y);
            if dragged != Some(card.model.id) {
                approach(&mut card.x, card.exit.map_or(0.0, |d| d * exit));
            }
        }
        for leaving in &mut self.leaving {
            approach(&mut leaving.card.x, leaving.direction * exit);
        }
        self.leaving
            .retain(|l| (l.card.x - l.direction * exit).abs() > SETTLE_PX);
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    pub fn has_toasts(&self) -> bool {
        !self.cards.is_empty()
    }

    /// Displayed drawer, screen coordinates; `None` when fully retracted.
    pub fn panel_rect(&self) -> Option<Rect> {
        if self.panel_height <= SETTLE_PX && self.cards.is_empty() && self.leaving.is_empty() {
            return None;
        }
        let (ox, oy) = self.panel_origin()?;
        Some(Rect::new(ox, oy, PANEL_WIDTH, self.panel_height))
    }

    fn card_paint(&self, card: &Card, leaving: bool) -> Option<CardPaint> {
        let geometry = self.geometry?;
        let (ox, oy) = self.content_origin()?;
        let left = ox + PANEL_PADDING + card.x + card.drag_x;
        let rect = Rect::new(left, oy + card.y, geometry.card_width(), card.height);
        let mut buttons = Vec::new();
        if !leaving && let Some(surface) = self.surface.as_ref() {
            // Router rectangles are at rest; shift them with the card.
            let (dx, dy) = (card.x + card.drag_x, card.y - card.target_y);
            for (node, (id, control)) in &surface.controls {
                if *id != card.model.id {
                    continue;
                }
                let kind = match control {
                    Control::Card => continue,
                    Control::Expand => ButtonKind::Expand {
                        expanded: card.expanded,
                    },
                    Control::Close => ButtonKind::Close,
                    Control::Action(index) => ButtonKind::Action {
                        label: card
                            .model
                            .actions
                            .get(*index)
                            .map(|a| a.label.clone())
                            .unwrap_or_default(),
                    },
                };
                let Some(at) = surface.layout.get(*node) else {
                    continue;
                };
                buttons.push(ButtonPaint {
                    kind,
                    rect: Rect::new(
                        ox + at.rect.x + dx,
                        oy + at.rect.y + dy,
                        at.rect.width,
                        at.rect.height,
                    ),
                });
            }
        }
        Some(CardPaint {
            card: card.model.clone(),
            expanded: card.expanded,
            rect,
            summary_lines: card.summary_lines,
            body_lines: card.body_lines,
            buttons,
            leaving,
        })
    }

    pub fn frame(&self) -> Option<ToastFrame> {
        let panel = self.panel_rect()?;
        let geometry = self.geometry?;
        let inset = geometry.inset();
        let top_pad = if geometry.on_bottom {
            PANEL_PADDING
        } else {
            inset
        };
        let clip = Rect::new(
            panel.x + PANEL_PADDING,
            panel.y + top_pad,
            (PANEL_WIDTH - PANEL_PADDING - inset).max(0.0),
            (panel.height - inset - PANEL_PADDING).max(0.0),
        );
        let cards = self
            .cards
            .iter()
            .filter_map(|c| self.card_paint(c, false))
            .chain(
                self.leaving
                    .iter()
                    .filter_map(|l| self.card_paint(&l.card, true)),
            )
            .collect();
        Some(ToastFrame {
            panel,
            clip,
            cards,
            on_left: geometry.on_left,
            on_bottom: geometry.on_bottom,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_notifications::model::{ExpireTimeout, Hints, NotifyRequest, pair_actions};
    use gnos_notifications::store::{NotificationStore, StoreConfig};
    use std::time::Duration;

    const VIEW: (f32, f32) = (1280.0, 800.0);
    const BORDER: f32 = 10.0;

    fn request(summary: &str, actions: &[&str]) -> NotifyRequest {
        NotifyRequest {
            app_name: "probe".into(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: summary.into(),
            body: "body text".into(),
            actions: pair_actions(&actions.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()),
            hints: Hints::default(),
            expire_timeout: ExpireTimeout::ServerDefault,
        }
    }

    struct Fixture {
        store: NotificationStore,
        host: ToastHost,
        now: Instant,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                store: NotificationStore::new(StoreConfig::default()),
                host: ToastHost::new(),
                now: Instant::now(),
            }
        }

        fn notify(&mut self, summary: &str, actions: &[&str]) -> NotificationId {
            let id = self.store.notify(request(summary, actions), 0).id;
            self.sync();
            id
        }

        fn sync(&mut self) -> bool {
            let snapshot = self.store.snapshot();
            self.host
                .sync(Some((&snapshot, 0)), VIEW, BORDER, &mut EstimatedText)
        }

        fn settle(&mut self) {
            for _ in 0..200 {
                self.now += Duration::from_millis(16);
                if !self.host.tick(self.now, false) {
                    return;
                }
            }
            panic!("motion never settled");
        }

        /// Applies the host's commands to the store, as the service would.
        fn apply(&mut self, outcome: &ToastOutcome) {
            for command in &outcome.commands {
                match command {
                    Command::HoldToast(id) => {
                        self.store.hold_toast(*id);
                    }
                    Command::ReleaseToast(id) => {
                        self.store.release_toast(*id, 0);
                    }
                    Command::HideToast(id) => {
                        self.store.hide_toast(*id);
                    }
                    Command::Dismiss(id) => {
                        self.store
                            .close(*id, gnos_notifications::CloseReason::Dismissed);
                    }
                    Command::Invoke(id, key) => {
                        self.store.invoke(*id, key);
                    }
                    _ => {}
                }
            }
            self.sync();
        }

        fn card(&self, index: usize) -> CardPaint {
            self.host.frame().unwrap().cards[index].clone()
        }

        fn button(&self, index: usize, pick: impl Fn(&ButtonKind) -> bool) -> Rect {
            self.card(index)
                .buttons
                .into_iter()
                .find(|b| pick(&b.kind))
                .expect("button painted")
                .rect
        }
    }

    fn centre(rect: Rect) -> (f32, f32) {
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    #[test]
    fn drawer_meets_the_frame_corner_and_stacks_newest_first() {
        let mut f = Fixture::new();
        f.notify("first", &[]);
        f.notify("second", &[]);
        f.settle();
        let frame = f.host.frame().unwrap();
        assert_eq!(
            (frame.panel.x, frame.panel.y),
            (1280.0 - 10.0 - 430.0, 10.0)
        );
        assert_eq!(frame.panel.x + frame.panel.width, 1280.0 - BORDER);
        let (a, b) = (&frame.cards[0], &frame.cards[1]);
        assert_eq!(a.card.summary, "second");
        assert_eq!(a.rect.width, 430.0 - 16.0 - 6.0);
        assert_eq!(a.rect.x, frame.panel.x + 16.0);
        assert_eq!(a.rect.y, frame.panel.y + 6.0);
        assert_eq!(b.rect.y, a.rect.y + a.rect.height + 12.0);
        assert_eq!(a.rect.height, 2.0 * 12.0 + 42.0);
        assert_eq!(frame.panel.height, 2.0 * a.rect.height + 12.0 + 16.0 + 6.0);
    }

    #[test]
    fn under_a_top_bar_the_drawer_stands_in_the_bottom_right_corner() {
        let mut f = Fixture::new();
        f.host.set_on_bottom(Some(60.0));
        let id = f.notify("first", &[]);
        f.notify("second", &[]);
        f.settle();
        let frame = f.host.frame().unwrap();
        assert!(frame.on_bottom && !frame.on_left);
        assert_eq!(frame.panel.x + frame.panel.width, 1280.0 - BORDER);
        assert_eq!(frame.panel.y + frame.panel.height, 800.0 - BORDER);
        let (a, b) = (&frame.cards[0], &frame.cards[1]);
        // Mirrored padding: 16 from the free top, 6 from the bottom border.
        assert_eq!(a.rect.y, frame.panel.y + 16.0);
        assert_eq!(
            b.rect.y + b.rect.height,
            frame.panel.y + frame.panel.height - 6.0
        );
        assert_eq!(frame.clip.y, frame.panel.y + 16.0);
        // Hit-testing follows the painted cards.
        let (x, y) = centre(b.rect);
        let outcome = f.host.pointer_moved(x, y);
        assert_eq!(outcome.commands, [Command::HoldToast(id)]);
    }

    #[test]
    fn on_a_narrow_output_the_drawer_stands_on_the_deck() {
        // BAR-01H (B07): under a top Bar, a base of 82 lifts the drawer off
        // the bottom border onto the Deck; its height shrinks to fit.
        let mut f = Fixture::new();
        f.host.set_on_bottom(Some(60.0));
        f.host.set_base(82.0);
        f.notify("first", &[]);
        f.settle();
        let frame = f.host.frame().unwrap();
        assert_eq!(frame.panel.y + frame.panel.height, 800.0 - BORDER - 82.0);
        // Over a bottom Bar it hangs below the Deck instead.
        let mut f = Fixture::new();
        f.host.set_base(82.0);
        f.notify("first", &[]);
        f.settle();
        assert_eq!(f.host.frame().unwrap().panel.y, BORDER + 82.0);
        f.host.set_base(f32::NAN);
        f.notify("second", &[]);
        f.settle();
        assert_eq!(
            f.host.frame().unwrap().panel.y,
            BORDER,
            "a bad base is none"
        );
    }

    #[test]
    fn a_new_card_slides_in_from_the_frame_edge() {
        let mut f = Fixture::new();
        f.notify("a", &[]);
        let start = f.card(0).rect.x;
        // The first tick only starts the clock.
        f.host.tick(f.now, false);
        f.now += Duration::from_millis(16);
        f.host.tick(f.now, false);
        let moving = f.card(0).rect.x;
        f.settle();
        let rest = f.card(0).rect.x;
        assert!(
            start > moving && moving > rest,
            "{start} > {moving} > {rest}"
        );
        assert_eq!(start - rest, 408.0);
    }

    #[test]
    fn hover_holds_and_leaving_releases_the_toast() {
        let mut f = Fixture::new();
        let id = f.notify("a", &[]);
        f.settle();
        let (x, y) = centre(f.card(0).rect);
        let outcome = f.host.pointer_moved(x, y);
        assert_eq!(outcome.commands, [Command::HoldToast(id)]);
        assert!(outcome.consumed);
        assert!(f.host.pointer_moved(x + 1.0, y).commands.is_empty());
        let outcome = f.host.pointer_moved(200.0, 400.0);
        assert_eq!(outcome.commands, [Command::ReleaseToast(id)]);
        assert!(!outcome.consumed);
    }

    #[test]
    fn swipe_past_threshold_hides_the_toast_and_slides_it_out() {
        let mut f = Fixture::new();
        let id = f.notify("a", &[]);
        f.settle();
        let rect = f.card(0).rect;
        let (x, y) = (rect.x + 40.0, rect.y + rect.height - 10.0);
        let o = f.host.pointer_moved(x, y);
        f.apply(&o);
        let o = f.host.pointer_down(x, y, false);
        f.apply(&o);
        f.host.pointer_moved(x + 30.0, y + 2.0);
        assert_eq!(
            f.card(0).rect.x,
            rect.x + 30.0,
            "previews on the locked axis"
        );
        f.host.pointer_moved(x + 140.0, y + 9.0);
        let outcome = f.host.pointer_up(x + 140.0, y + 9.0, false);
        assert!(
            outcome.commands.contains(&Command::HideToast(id)),
            "{:?}",
            outcome.commands
        );
        f.apply(&outcome);
        // The toast is hidden, the notification stays in history (N07).
        assert_eq!(f.store.snapshot().notifications.len(), 1);
        assert!(f.host.frame().unwrap().cards[0].leaving);
        f.settle();
        assert!(f.host.frame().is_none(), "drawer retracts once empty");
    }

    #[test]
    fn short_swipe_springs_back() {
        let mut f = Fixture::new();
        let id = f.notify("a", &[]);
        f.settle();
        let rect = f.card(0).rect;
        let (x, y) = (rect.x + 40.0, rect.y + rect.height - 10.0);
        f.host.pointer_down(x, y, false);
        f.host.pointer_moved(x - 60.0, y);
        let outcome = f.host.pointer_up(x - 60.0, y, false);
        assert!(!outcome.commands.contains(&Command::HideToast(id)));
        f.settle();
        assert_eq!(f.card(0).rect.x, rect.x);
    }

    #[test]
    fn vertical_drag_expands_and_collapses_on_release() {
        let mut f = Fixture::new();
        f.notify("a", &["open", "Open"]);
        f.settle();
        let rect = f.card(0).rect;
        let (x, y) = (rect.x + 40.0, rect.y + 20.0);
        f.host.pointer_down(x, y, false);
        f.host.pointer_moved(x + 3.0, y + 30.0);
        assert_eq!(f.card(0).rect.x, rect.x, "a vertical drag never swipes");
        let outcome = f.host.pointer_up(x + 3.0, y + 30.0, false);
        assert!(outcome.changed);
        f.sync();
        f.settle();
        let expanded = f.card(0);
        assert!(expanded.expanded);
        assert!(expanded.rect.height > rect.height);
        assert!(expanded.buttons.iter().any(|b| b.kind
            == ButtonKind::Action {
                label: "Open".into()
            }));
        let (x, y) = (expanded.rect.x + 40.0, expanded.rect.y + 30.0);
        f.host.pointer_down(x, y, false);
        f.host.pointer_moved(x, y - 25.0);
        f.host.pointer_up(x, y - 25.0, false);
        f.sync();
        assert!(!f.card(0).expanded);
    }

    #[test]
    fn buttons_expand_invoke_and_close() {
        let mut f = Fixture::new();
        let id = f.notify("a", &["open", "Open", "later", "Later"]);
        f.settle();
        let (x, y) = centre(f.button(0, |k| matches!(k, ButtonKind::Expand { .. })));
        f.host.pointer_down(x, y, false);
        f.host.pointer_up(x, y, false);
        f.sync();
        f.settle();
        let (x, y) = centre(f.button(0, |k| {
            *k == ButtonKind::Action {
                label: "Later".into(),
            }
        }));
        f.host.pointer_down(x, y, false);
        let outcome = f.host.pointer_up(x, y, false);
        assert!(
            outcome
                .commands
                .contains(&Command::Invoke(id, "later".into())),
            "{:?}",
            outcome.commands
        );
        let id2 = f.notify("b", &[]);
        f.settle();
        let (x, y) = centre(f.button(0, |k| matches!(k, ButtonKind::Expand { .. })));
        f.host.pointer_down(x, y, false);
        f.host.pointer_up(x, y, false);
        f.sync();
        f.settle();
        // Without actions the close button fills the row.
        let close = f.button(0, |k| *k == ButtonKind::Close);
        assert!(close.width > ACTION_ROW);
        let (x, y) = centre(close);
        f.host.pointer_down(x, y, false);
        let outcome = f.host.pointer_up(x, y, false);
        assert!(outcome.commands.contains(&Command::Dismiss(id2)));
    }

    #[test]
    fn middle_press_closes_and_escape_abandons_a_drag() {
        let mut f = Fixture::new();
        let id = f.notify("a", &[]);
        f.settle();
        let rect = f.card(0).rect;
        let (x, y) = (rect.x + 40.0, rect.y + rect.height - 10.0);
        assert_eq!(
            f.host.pointer_down(x, y, true).commands,
            [Command::Dismiss(id)]
        );
        f.host.pointer_up(x, y, true);
        f.host.pointer_down(x, y, false);
        f.host.pointer_moved(x + 200.0, y);
        let outcome = f.host.escape();
        assert!(outcome.changed && outcome.consumed);
        let outcome = f.host.pointer_up(x + 200.0, y, false);
        assert!(!outcome.commands.contains(&Command::HideToast(id)));
        f.settle();
        assert_eq!(f.card(0).rect.x, rect.x);
    }

    #[test]
    fn expiry_slides_out_right_and_others_close_the_gap() {
        let mut f = Fixture::new();
        f.notify("old", &[]);
        f.notify("new", &[]);
        f.settle();
        let first_top = f.card(0).rect.y;
        // Only "new" (the top card) expires.
        let new_id = f.store.snapshot().notifications[0].id;
        f.store.hide_toast(new_id);
        f.sync();
        let frame = f.host.frame().unwrap();
        let leaving = frame.cards.iter().find(|c| c.leaving).unwrap();
        assert_eq!(leaving.card.summary, "new");
        f.settle();
        let frame = f.host.frame().unwrap();
        assert_eq!(frame.cards.len(), 1);
        assert_eq!(frame.cards[0].rect.y, first_top);
    }

    #[test]
    fn reduced_motion_snaps_and_idle_needs_no_frame() {
        let mut f = Fixture::new();
        f.notify("a", &[]);
        assert!(f.host.needs_frame());
        assert!(!f.host.tick(f.now, true));
        assert!(!f.host.needs_frame());
        assert!(!f.sync(), "unchanged revision is a no-op");
    }

    #[test]
    fn age_labels() {
        assert_eq!(age_label(59_999), "now");
        assert_eq!(age_label(60_000), "1m");
        assert_eq!(age_label(3_600_000), "1h");
        assert_eq!(age_label(86_400_000 * 2), "2d");
    }
}
