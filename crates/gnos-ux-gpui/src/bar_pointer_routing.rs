//! PARITY-02A: Bar pointer routing through `blobin-ui-core`.
//!
//! The reference Bar column (entries, flexible spacers, workspace, tray and
//! status sub-items) is expressed as a headless `blobin-ui-core` tree. blob.in's
//! router owns hit testing, hover transitions, scroll routing and click
//! activation; this module owns only the mapping from routed targets to typed
//! Bar intents. Behaviour follows the pinned reference observations recorded in
//! `docs/program/gnos-ux-1.0/PARITY_02_BAR_INTERACTION.md` (shell `1c4f92a`).
//!
//! Intents carry no authority. Workspace, volume, brightness, launcher and
//! session effects are resolved by their owners; this module never dispatches.

use crate::reference_bar::{
    self, BarPresentation, EXTRA_SMALL_SPACING, IdleBarEntry, LARGE_VERTICAL_PADDING,
    STATUS_ENTRY_GAP, STATUS_PADDING, StatusVisual, TRAY_ITEM_SIZE, TRAY_ITEM_SPACING,
    TRAY_PADDING,
};
use blobin_ui_core::input::{
    EventKind, InputEvent, InputReceipt, InputRouter, Point, ScrollDelta, ScrollSource, Units,
};
use blobin_ui_core::layout::{
    self, Insets, LayoutSnapshot, LayoutStyles, Length, Size, Style, Viewport,
};
use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiTree};
use std::collections::BTreeMap;

/// Full rail width; entries occupy the centred `BAR_INNER_WIDTH` column.
pub const BAR_WIDTH: f32 = 60.0;
/// The tray hover index divides by the tray's own spacing token, not the
/// item column's spacing (reference quirk kept on purpose).
const TRAY_HOVER_SPACING: f32 = EXTRA_SMALL_SPACING;
const POINTER: u64 = 1;
const PRIMARY: u8 = 1;
const SECONDARY: u8 = 3;

/// Reference defaults; the observed host runs with all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarPointerConfig {
    pub status_popouts: bool,
    pub tray_popouts: bool,
    pub active_window_popout: bool,
    pub active_window_show_on_hover: bool,
    pub scroll_workspaces: bool,
    pub scroll_volume: bool,
    pub scroll_brightness: bool,
    /// SURF-03: hovering the clock shows the dashboard. Off by default; the
    /// native rail's popout host turns it on.
    pub dashboard_popout: bool,
}

impl Default for BarPointerConfig {
    fn default() -> Self {
        Self {
            status_popouts: true,
            tray_popouts: true,
            active_window_popout: true,
            active_window_show_on_hover: true,
            scroll_workspaces: true,
            scroll_volume: true,
            scroll_brightness: true,
            dashboard_popout: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusEntryKind {
    Lock,
    Network,
    Bluetooth,
    BatteryOrProfile,
    Audio,
}

impl From<&StatusVisual> for StatusEntryKind {
    fn from(visual: &StatusVisual) -> Self {
        match visual {
            StatusVisual::Lock { .. } => Self::Lock,
            StatusVisual::Network { .. } => Self::Network,
            StatusVisual::Bluetooth { .. } => Self::Bluetooth,
            StatusVisual::BatteryOrProfile { .. } => Self::BatteryOrProfile,
            StatusVisual::Audio { .. } => Self::Audio,
        }
    }
}

/// What a routed node means to the Bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarTarget {
    Logo,
    Workspaces,
    Workspace(usize),
    Spacer,
    ActiveWindow,
    Tray,
    TrayItem(usize),
    Clock,
    StatusIcons,
    StatusEntry(StatusEntryKind),
    Power,
    /// BAR-02A: the "…" module.
    Overflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarPopout {
    Status(StatusEntryKind),
    TrayMenu(usize),
    ActiveWindow,
    /// SURF-03 (H03): the dashboard, out of the clock.
    Dashboard,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopoutChange {
    /// Open or retarget; `anchor_y` is the target's vertical centre in Bar space.
    Show {
        popout: BarPopout,
        anchor_y: f32,
    },
    Hide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepDirection {
    Increase,
    Decrease,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarPointerIntent {
    Popout(PopoutChange),
    /// Wheel over workspaces: `-1` previous, `+1` next. Resolution to an
    /// observed output-local workspace stays with the activation owner.
    WorkspaceStep(i8),
    VolumeStep(StepDirection),
    BrightnessStep(StepDirection),
    FocusWorkspace(usize),
    /// Click on the already active workspace.
    ToggleSpecialWorkspace,
    ToggleLauncher,
    ToggleSession,
    TrayActivate {
        index: usize,
        secondary: bool,
    },
}

/// Popout state the router needs from its owner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopoutContext {
    pub current: Option<BarPopout>,
    /// A tray menu with a pushed submenu survives leaving the Bar.
    pub tray_submenu_open: bool,
    /// Pointer is over the attached popout panel itself.
    pub pointer_in_popout_panel: bool,
}

#[derive(Debug)]
pub enum BarRoutingError {
    Tree(String),
    Layout(String),
    Input(String),
}

pub struct BarPointerRouter {
    config: BarPointerConfig,
    tree: UiTree,
    styles: LayoutStyles,
    layout: LayoutSnapshot,
    viewport: Viewport,
    router: InputRouter,
    targets: BTreeMap<NodeId, BarTarget>,
    active_workspace: Option<usize>,
    tray_count: usize,
    screen_height: f32,
    serial: u64,
}

struct NodeSpec {
    target: BarTarget,
    style: Style,
    children: Vec<NodeSpec>,
}

fn px_column(height: f32, padding: Insets, gap: f32) -> Style {
    Style {
        height: Length::Px(height),
        padding,
        gap,
        ..Style::default()
    }
}

fn leaf(target: BarTarget, height: f32) -> NodeSpec {
    NodeSpec {
        target,
        style: px_column(height, Insets::default(), 0.0),
        children: Vec::new(),
    }
}

fn vertical(top_bottom: f32, side: f32) -> Insets {
    Insets {
        left: side,
        right: side,
        top: top_bottom,
        bottom: top_bottom,
    }
}

impl BarPointerRouter {
    /// `active_window_height` is the rendered module's own height; every
    /// explicit height is snapped like the scene's layout quantiser.
    pub fn new(
        presentation: &BarPresentation,
        config: BarPointerConfig,
        bar_height: f32,
        screen_height: f32,
        active_window_height: f32,
    ) -> Result<Self, BarRoutingError> {
        Self::new_on(
            presentation,
            config,
            bar_height,
            screen_height,
            active_window_height,
            false,
        )
    }

    /// TOPO-01C: the same router for a horizontal Bar. Heights become
    /// lengths along the Bar; only the Clock's differs (`row_clock_length`).
    /// `active_window_length` is the rendered module's extent along the Bar.
    pub fn new_on(
        presentation: &BarPresentation,
        config: BarPointerConfig,
        bar_height: f32,
        screen_height: f32,
        active_window_height: f32,
        horizontal: bool,
    ) -> Result<Self, BarRoutingError> {
        let snap = reference_bar::layout_round_half_down;
        let module_layout = reference_bar::reference_module_layout(presentation);
        let mut entries = Vec::new();
        for (entry, height) in &module_layout.entries {
            entries.push(match entry {
                IdleBarEntry::Logo => leaf(BarTarget::Logo, snap(*height)),
                IdleBarEntry::Spacer => NodeSpec {
                    target: BarTarget::Spacer,
                    style: Style {
                        flex_grow: 1.0,
                        ..Style::default()
                    },
                    children: Vec::new(),
                },
                IdleBarEntry::ActiveWindow => {
                    leaf(BarTarget::ActiveWindow, snap(active_window_height))
                }
                IdleBarEntry::Clock => leaf(
                    BarTarget::Clock,
                    snap(match presentation.clock.as_ref() {
                        Some(clock) if horizontal => reference_bar::row_clock_length(clock),
                        _ => *height,
                    }),
                ),
                IdleBarEntry::Power => leaf(BarTarget::Power, snap(*height)),
                IdleBarEntry::Overflow => leaf(BarTarget::Overflow, snap(*height)),
                IdleBarEntry::Workspaces => NodeSpec {
                    target: BarTarget::Workspaces,
                    style: px_column(
                        snap(*height),
                        vertical(EXTRA_SMALL_SPACING, 0.0),
                        EXTRA_SMALL_SPACING,
                    ),
                    children: presentation
                        .workspaces
                        .iter()
                        .enumerate()
                        .map(|(index, workspace)| {
                            leaf(
                                BarTarget::Workspace(index),
                                reference_bar::workspace_preferred_height(workspace.window_count()),
                            )
                        })
                        .collect(),
                },
                IdleBarEntry::Tray => {
                    let count = presentation.tray.as_ref().map_or(0, Vec::len);
                    let side = (reference_bar::BAR_INNER_WIDTH - TRAY_ITEM_SIZE) / 2.0;
                    NodeSpec {
                        target: BarTarget::Tray,
                        style: px_column(
                            snap(*height),
                            vertical(TRAY_PADDING, side),
                            TRAY_ITEM_SPACING,
                        ),
                        children: (0..count)
                            .map(|index| leaf(BarTarget::TrayItem(index), TRAY_ITEM_SIZE))
                            .collect(),
                    }
                }
                IdleBarEntry::StatusIcons => NodeSpec {
                    target: BarTarget::StatusIcons,
                    style: px_column(
                        snap(*height),
                        vertical(STATUS_PADDING, 0.0),
                        STATUS_ENTRY_GAP,
                    ),
                    children: presentation
                        .status
                        .iter()
                        .flat_map(|status| status.entries.iter())
                        .filter(|visual| visual.is_present())
                        .map(|visual| {
                            leaf(
                                BarTarget::StatusEntry(visual.into()),
                                visual.source_height(),
                            )
                        })
                        .collect(),
                },
            });
        }

        let surface = SurfaceId(1);
        let mut tree = UiTree::new();
        let mut placed: Vec<(NodeId, Style)> = Vec::new();
        let mut targets = BTreeMap::new();
        tree.transact(|tx| {
            tx.open_surface(surface)?;
            let root = tx.append_root(surface, NodeProperties::new(Role::Container, "bar"))?;
            placed.push((
                root,
                Style {
                    padding: vertical(
                        LARGE_VERTICAL_PADDING,
                        (crate::rail_topology::rail_width() - reference_bar::BAR_INNER_WIDTH) / 2.0,
                    ),
                    gap: reference_bar::module_gap(),
                    ..Style::fill()
                },
            ));
            let mut stack: Vec<(NodeId, &NodeSpec)> =
                entries.iter().rev().map(|spec| (root, spec)).collect();
            while let Some((parent, spec)) = stack.pop() {
                let label = format!("{:?}", spec.target);
                let id = tx.append_child(
                    parent,
                    NodeProperties::new(Role::Custom(label.clone()), label),
                )?;
                placed.push((id, spec.style.clone()));
                targets.insert(id, spec.target);
                stack.extend(spec.children.iter().rev().map(|child| (id, child)));
            }
            Ok(())
        })
        .map_err(|e| BarRoutingError::Tree(format!("{e:?}")))?;

        let mut styles = LayoutStyles::new();
        let snapshot = tree.snapshot();
        for (id, style) in placed {
            styles
                .set(&snapshot, id, style)
                .map_err(|e| BarRoutingError::Layout(format!("{e:?}")))?;
        }
        let viewport = Viewport::new(
            Size::new(crate::rail_topology::rail_width(), bar_height),
            1.0,
            1,
        );
        let layout = layout::compute(&snapshot, &styles, surface, viewport)
            .map_err(|e| BarRoutingError::Layout(format!("{e:?}")))?;
        Ok(Self {
            config,
            tree,
            styles,
            layout,
            viewport,
            router: InputRouter::new(),
            targets,
            active_workspace: presentation.workspaces.iter().position(|w| w.active),
            tray_count: presentation.tray.as_ref().map_or(0, Vec::len),
            screen_height,
            serial: 0,
        })
    }

    /// Bar-space rectangle `(top, bottom)` of the first node routed as `target`.
    pub fn vertical_extent(&self, target: BarTarget) -> Option<(f32, f32)> {
        let (id, _) = self.targets.iter().find(|(_, t)| **t == target)?;
        let rect = self.layout.get(*id)?.rect;
        Some((rect.y, rect.y + rect.height))
    }

    fn send(
        &mut self,
        timestamp_micros: u64,
        kind: EventKind,
    ) -> Result<InputReceipt, BarRoutingError> {
        self.serial += 1;
        let event = InputEvent::for_layout(&self.layout, self.serial, timestamp_micros, kind);
        self.router
            .dispatch(
                &self.tree.snapshot(),
                &self.styles,
                &self.layout,
                self.viewport,
                event,
            )
            .map_err(|e| BarRoutingError::Input(format!("{e:?}")))
    }

    fn target_of(&self, receipt: &InputReceipt) -> Option<BarTarget> {
        receipt
            .route
            .as_ref()
            .and_then(|route| self.targets.get(&route.target).copied())
    }

    /// SURF-03: the clock's centre along the Bar, if it is routed.
    pub fn clock_centre(&self) -> Option<f32> {
        self.vertical_extent(BarTarget::Clock)
            .map(|(top, bottom)| (top + bottom) / 2.0)
    }

    fn centre_of(&self, target: BarTarget) -> f32 {
        self.vertical_extent(target)
            .map_or(0.0, |(top, bottom)| (top + bottom) / 2.0)
    }

    /// Reference tray hover index: uniform partition of the tray column,
    /// including its padding, rather than an item hit test.
    fn tray_hover_index(&self, y: f32) -> Option<usize> {
        let (top, _) = self.vertical_extent(BarTarget::Tray)?;
        let n = self.tray_count;
        if n == 0 {
            return None;
        }
        let column = n as f32 * TRAY_ITEM_SIZE + (n - 1) as f32 * TRAY_ITEM_SPACING;
        let index =
            ((y - top - TRAY_PADDING * 2.0 + TRAY_HOVER_SPACING) / column * n as f32).floor();
        (index >= 0.0 && (index as usize) < n).then_some(index as usize)
    }

    fn popout_for(
        &self,
        target: BarTarget,
        y: f32,
        context: PopoutContext,
    ) -> Option<Option<PopoutChange>> {
        match target {
            BarTarget::StatusEntry(kind) if self.config.status_popouts => {
                Some(Some(PopoutChange::Show {
                    popout: BarPopout::Status(kind),
                    anchor_y: self.centre_of(target),
                }))
            }
            BarTarget::Tray | BarTarget::TrayItem(_) if self.config.tray_popouts => {
                Some(Some(match self.tray_hover_index(y) {
                    Some(index) => PopoutChange::Show {
                        popout: BarPopout::TrayMenu(index),
                        anchor_y: self.centre_of(BarTarget::TrayItem(index)),
                    },
                    None => PopoutChange::Hide,
                }))
            }
            BarTarget::ActiveWindow
                if self.config.active_window_popout && self.config.active_window_show_on_hover =>
            {
                Some(Some(PopoutChange::Show {
                    popout: BarPopout::ActiveWindow,
                    anchor_y: self.centre_of(target),
                }))
            }
            // Click-driven ActiveWindow: hovering it dismisses any other popout.
            BarTarget::ActiveWindow
                if !self.config.active_window_show_on_hover
                    && context
                        .current
                        .is_some_and(|c| c != BarPopout::ActiveWindow) =>
            {
                Some(Some(PopoutChange::Hide))
            }
            BarTarget::Clock if self.config.dashboard_popout => Some(Some(PopoutChange::Show {
                popout: BarPopout::Dashboard,
                anchor_y: self.centre_of(target),
            })),
            // Entries without a popout leave the current one untouched; this
            // includes the status column's own padding between icons.
            _ => Some(None),
        }
    }

    /// Pointer motion anywhere on the output; `x`/`y` are Bar-space.
    pub fn pointer_moved(
        &mut self,
        x: f32,
        y: f32,
        timestamp_micros: u64,
        context: PopoutContext,
    ) -> Result<Vec<BarPointerIntent>, BarRoutingError> {
        if x < crate::rail_topology::rail_width() {
            let receipt = self.send(
                timestamp_micros,
                EventKind::PointerMove {
                    pointer: POINTER,
                    position: Point::new(crate::rail_topology::rail_width() / 2.0, y),
                    units: Units::Logical,
                },
            )?;
            let change = match self.target_of(&receipt) {
                // Gaps between entries are not children: the popout closes.
                None => Some(PopoutChange::Hide),
                Some(target) => self.popout_for(target, y, context).flatten(),
            };
            return Ok(change
                .filter(|change| !self.is_redundant(*change, context))
                .map(BarPointerIntent::Popout)
                .into_iter()
                .collect());
        }
        self.send(
            timestamp_micros,
            EventKind::PointerLeave { pointer: POINTER },
        )?;
        Ok(self.leave_intent(context).into_iter().collect())
    }

    /// The pointer left the shell's interaction surface entirely.
    pub fn pointer_left(
        &mut self,
        timestamp_micros: u64,
        context: PopoutContext,
    ) -> Result<Vec<BarPointerIntent>, BarRoutingError> {
        self.send(
            timestamp_micros,
            EventKind::PointerLeave { pointer: POINTER },
        )?;
        let mut context = context;
        context.pointer_in_popout_panel = false;
        Ok(self.leave_intent(context).into_iter().collect())
    }

    fn leave_intent(&self, context: PopoutContext) -> Option<BarPointerIntent> {
        let held_tray =
            matches!(context.current, Some(BarPopout::TrayMenu(_))) && context.tray_submenu_open;
        (context.current.is_some() && !held_tray && !context.pointer_in_popout_panel)
            .then_some(BarPointerIntent::Popout(PopoutChange::Hide))
    }

    fn is_redundant(&self, change: PopoutChange, context: PopoutContext) -> bool {
        matches!(change, PopoutChange::Hide) && context.current.is_none()
    }

    /// Wheel on the Bar. `delta_y` uses the Wayland axis sign (positive =
    /// downward, i.e. the wheel rolled toward the user).
    pub fn wheel(
        &mut self,
        x: f32,
        y: f32,
        delta_y: f32,
        timestamp_micros: u64,
    ) -> Result<Option<BarPointerIntent>, BarRoutingError> {
        if x >= crate::rail_topology::rail_width() {
            return Ok(None);
        }
        let receipt = self.send(
            timestamp_micros,
            EventKind::Scroll {
                pointer: POINTER,
                position: Point::new(crate::rail_topology::rail_width() / 2.0, y),
                units: Units::Logical,
                delta: ScrollDelta { x: 0.0, y: delta_y },
                source: ScrollSource::Wheel,
            },
        )?;
        // A zero vertical delta (horizontal-only scroll) is refused; the
        // reference would treat it as "next workspace".
        if delta_y == 0.0 {
            return Ok(None);
        }
        let up = delta_y < 0.0;
        let direction = if up {
            StepDirection::Increase
        } else {
            StepDirection::Decrease
        };
        let over_workspaces = matches!(
            self.target_of(&receipt),
            Some(BarTarget::Workspaces | BarTarget::Workspace(_))
        );
        // Reference precedence: workspaces, then upper-half volume, then
        // brightness anywhere else (including the upper half when volume is off).
        Ok(if over_workspaces && self.config.scroll_workspaces {
            Some(BarPointerIntent::WorkspaceStep(if up { -1 } else { 1 }))
        } else if y < self.screen_height / 2.0 && self.config.scroll_volume {
            Some(BarPointerIntent::VolumeStep(direction))
        } else if self.config.scroll_brightness {
            Some(BarPointerIntent::BrightnessStep(direction))
        } else {
            None
        })
    }

    /// One completed click (press + release at the same point). Click
    /// targets use the real `x`, unlike hover.
    pub fn click(
        &mut self,
        x: f32,
        y: f32,
        secondary: bool,
        timestamp_micros: u64,
        context: PopoutContext,
    ) -> Result<Option<BarPointerIntent>, BarRoutingError> {
        let button = if secondary { SECONDARY } else { PRIMARY };
        let position = Point::new(x, y);
        self.send(
            timestamp_micros,
            EventKind::PointerDown {
                pointer: POINTER,
                button,
                position,
                units: Units::Logical,
            },
        )?;
        let release = self.send(
            timestamp_micros,
            EventKind::PointerUp {
                pointer: POINTER,
                button,
                position,
                units: Units::Logical,
            },
        )?;
        let Some(target) = release
            .activated
            .and_then(|id| self.targets.get(&id).copied())
        else {
            return Ok(None);
        };
        Ok(match (target, secondary) {
            (BarTarget::Logo, false) => Some(BarPointerIntent::ToggleLauncher),
            (BarTarget::Power, false) => Some(BarPointerIntent::ToggleSession),
            (BarTarget::Workspace(index), false) => Some(if self.active_workspace == Some(index) {
                BarPointerIntent::ToggleSpecialWorkspace
            } else {
                BarPointerIntent::FocusWorkspace(index)
            }),
            (BarTarget::TrayItem(index), secondary) => {
                Some(BarPointerIntent::TrayActivate { index, secondary })
            }
            (BarTarget::ActiveWindow, false) if !self.config.active_window_show_on_hover => {
                Some(BarPointerIntent::Popout(if context.current.is_some() {
                    PopoutChange::Hide
                } else {
                    PopoutChange::Show {
                        popout: BarPopout::ActiveWindow,
                        anchor_y: self.centre_of(target),
                    }
                }))
            }
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f32 = 1440.0;
    const ACTIVE_WINDOW: f32 = 180.0;

    fn router(config: BarPointerConfig) -> BarPointerRouter {
        let bar = BarPresentation::captured_oracle_2026_10_07();
        BarPointerRouter::new(&bar, config, H, H, ACTIVE_WINDOW).expect("router")
    }

    fn centre(r: &BarPointerRouter, t: BarTarget) -> f32 {
        let (top, bottom) = r.vertical_extent(t).expect("target present");
        (top + bottom) / 2.0
    }

    /// Cross-check against EC-04A's qualified workspace hover bounds.
    #[cfg(feature = "component-lab")]
    #[test]
    fn blob_layout_reproduces_qualified_workspace_bounds() {
        let bar = BarPresentation::captured_oracle_2026_10_07();
        let r = router(BarPointerConfig::default());
        let qualified =
            crate::product_reference_consumer::workspace_hover_bounds(&bar).expect("workspaces");
        let (top, bottom) = r.vertical_extent(BarTarget::Workspaces).unwrap();
        assert_eq!(top, qualified.top);
        assert_eq!(bottom, qualified.bottom);
    }

    #[test]
    fn blob_layout_reproduces_qualified_active_window_top() {
        let bar = BarPresentation::captured_oracle_2026_10_07();
        let r = router(BarPointerConfig::default());
        let (aw_top, _) = r.vertical_extent(BarTarget::ActiveWindow).unwrap();
        assert_eq!(
            Some(reference_bar::layout_round_half_down(aw_top)),
            reference_bar::active_window_top(&bar, H, ACTIVE_WINDOW)
        );
    }

    #[test]
    fn hover_on_status_icon_shows_its_popout_and_gaps_hide() {
        let mut r = router(BarPointerConfig::default());
        let bar = BarPresentation::captured_oracle_2026_10_07();
        let first = bar
            .status
            .as_ref()
            .and_then(|s| s.entries.iter().find(|v| v.is_present()))
            .map(StatusEntryKind::from)
            .expect("oracle has a status entry");
        let y = centre(&r, BarTarget::StatusEntry(first));
        let intents = r
            .pointer_moved(30.0, y, 1, PopoutContext::default())
            .unwrap();
        assert!(matches!(
            intents.as_slice(),
            [BarPointerIntent::Popout(PopoutChange::Show { popout: BarPopout::Status(k), .. })] if *k == first
        ));
        // The vertical gap between Workspaces and the first spacer is not a child.
        let (_, ws_bottom) = r.vertical_extent(BarTarget::Workspaces).unwrap();
        let open = PopoutContext {
            current: Some(BarPopout::Status(first)),
            ..PopoutContext::default()
        };
        assert_eq!(
            r.pointer_moved(
                30.0,
                ws_bottom + reference_bar::MEDIUM_SPACING / 2.0,
                2,
                open
            )
            .unwrap(),
            vec![BarPointerIntent::Popout(PopoutChange::Hide)]
        );
    }

    #[test]
    fn entries_without_popout_keep_the_current_one() {
        let mut r = router(BarPointerConfig::default());
        let open = PopoutContext {
            current: Some(BarPopout::ActiveWindow),
            ..PopoutContext::default()
        };
        for target in [
            BarTarget::Clock,
            BarTarget::Power,
            BarTarget::Workspaces,
            BarTarget::Spacer,
        ] {
            let y = centre(&r, target);
            assert!(
                r.pointer_moved(30.0, y, 1, open).unwrap().is_empty(),
                "{target:?}"
            );
        }
    }

    #[test]
    fn hover_projects_x_to_the_rail_centre() {
        let mut r = router(BarPointerConfig::default());
        let y = centre(&r, BarTarget::ActiveWindow);
        // x = 2 is outside the 40 px entry column but inside the 60 px rail.
        let intents = r
            .pointer_moved(2.0, y, 1, PopoutContext::default())
            .unwrap();
        assert!(matches!(
            intents.as_slice(),
            [BarPointerIntent::Popout(PopoutChange::Show {
                popout: BarPopout::ActiveWindow,
                ..
            })]
        ));
    }

    #[test]
    fn tray_hover_uses_the_reference_partition_formula() {
        let mut r = router(BarPointerConfig::default());
        if r.tray_count == 0 {
            return;
        }
        let (top, _) = r.vertical_extent(BarTarget::Tray).unwrap();
        // Formula: floor((y - top - 2p + s) / column * n). With p = s = 4 the
        // first band starts 4 px below the tray top.
        assert_eq!(r.tray_hover_index(top + 4.0), Some(0));
        assert_eq!(r.tray_hover_index(top + 3.9), None);
        let intents = r
            .pointer_moved(30.0, top + 6.0, 1, PopoutContext::default())
            .unwrap();
        assert!(matches!(
            intents.as_slice(),
            [BarPointerIntent::Popout(PopoutChange::Show {
                popout: BarPopout::TrayMenu(0),
                ..
            })]
        ));
    }

    #[test]
    fn leaving_the_rail_hides_unless_held() {
        let mut r = router(BarPointerConfig::default());
        let open = PopoutContext {
            current: Some(BarPopout::ActiveWindow),
            ..PopoutContext::default()
        };
        assert_eq!(
            r.pointer_moved(200.0, 300.0, 1, open).unwrap(),
            vec![BarPointerIntent::Popout(PopoutChange::Hide)]
        );
        let over_panel = PopoutContext {
            pointer_in_popout_panel: true,
            ..open
        };
        assert!(
            r.pointer_moved(200.0, 300.0, 2, over_panel)
                .unwrap()
                .is_empty()
        );
        let submenu = PopoutContext {
            current: Some(BarPopout::TrayMenu(0)),
            tray_submenu_open: true,
            pointer_in_popout_panel: false,
        };
        assert!(r.pointer_left(3, submenu).unwrap().is_empty());
        assert!(
            r.pointer_left(4, PopoutContext::default())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn wheel_zones_follow_reference() {
        let mut r = router(BarPointerConfig::default());
        let ws = centre(&r, BarTarget::Workspaces);
        assert_eq!(
            r.wheel(30.0, ws, -15.0, 1).unwrap(),
            Some(BarPointerIntent::WorkspaceStep(-1))
        );
        assert_eq!(
            r.wheel(30.0, ws, 15.0, 2).unwrap(),
            Some(BarPointerIntent::WorkspaceStep(1))
        );
        let upper = centre(&r, BarTarget::ActiveWindow).min(H / 2.0 - 1.0);
        assert_eq!(
            r.wheel(30.0, upper, -15.0, 3).unwrap(),
            Some(BarPointerIntent::VolumeStep(StepDirection::Increase))
        );
        assert_eq!(
            r.wheel(30.0, H - 20.0, 15.0, 4).unwrap(),
            Some(BarPointerIntent::BrightnessStep(StepDirection::Decrease))
        );
        assert_eq!(r.wheel(70.0, ws, -15.0, 5).unwrap(), None);
        assert_eq!(r.wheel(30.0, ws, 0.0, 6).unwrap(), None);
    }

    #[test]
    fn disabled_scroll_actions_produce_nothing() {
        let mut r = router(BarPointerConfig {
            scroll_volume: false,
            scroll_brightness: false,
            scroll_workspaces: false,
            ..BarPointerConfig::default()
        });
        let ws = centre(&r, BarTarget::Workspaces);
        assert_eq!(r.wheel(30.0, ws, -15.0, 1).unwrap(), None);
        let mut volume_off = router(BarPointerConfig {
            scroll_volume: false,
            ..BarPointerConfig::default()
        });
        let (_, ws_bottom) = volume_off.vertical_extent(BarTarget::Workspaces).unwrap();
        assert!(ws_bottom + reference_bar::MEDIUM_SPACING / 2.0 < H / 2.0);
        assert_eq!(
            volume_off
                .wheel(
                    30.0,
                    ws_bottom + reference_bar::MEDIUM_SPACING / 2.0,
                    -15.0,
                    1
                )
                .unwrap(),
            Some(BarPointerIntent::BrightnessStep(StepDirection::Increase))
        );
        assert_eq!(r.wheel(30.0, 100.0, -15.0, 2).unwrap(), None);
        assert_eq!(r.wheel(30.0, H - 20.0, -15.0, 3).unwrap(), None);
    }

    #[test]
    fn clicks_map_to_typed_intents_with_real_x() {
        let mut r = router(BarPointerConfig::default());
        let none = PopoutContext::default();
        let power = centre(&r, BarTarget::Power);
        assert_eq!(
            r.click(30.0, power, false, 1, none).unwrap(),
            Some(BarPointerIntent::ToggleSession)
        );
        // Real x outside the entry column misses, unlike hover.
        assert_eq!(r.click(2.0, power, false, 2, none).unwrap(), None);
        if r.vertical_extent(BarTarget::Logo).is_some() {
            let logo = centre(&r, BarTarget::Logo);
            assert_eq!(
                r.click(30.0, logo, false, 3, none).unwrap(),
                Some(BarPointerIntent::ToggleLauncher)
            );
        }
        let active = r.active_workspace;
        for index in 0..3 {
            let Some(_) = r.vertical_extent(BarTarget::Workspace(index)) else {
                break;
            };
            let y = centre(&r, BarTarget::Workspace(index));
            let expected = if active == Some(index) {
                BarPointerIntent::ToggleSpecialWorkspace
            } else {
                BarPointerIntent::FocusWorkspace(index)
            };
            assert_eq!(
                r.click(30.0, y, false, 10 + index as u64, none).unwrap(),
                Some(expected)
            );
        }
        // ActiveWindow click is inert while the popout is hover-driven.
        let aw = centre(&r, BarTarget::ActiveWindow);
        assert_eq!(r.click(30.0, aw, false, 20, none).unwrap(), None);
    }

    #[test]
    fn tray_items_take_primary_and_secondary_clicks_at_their_own_width() {
        let mut r = router(BarPointerConfig::default());
        if r.tray_count == 0 {
            return;
        }
        let none = PopoutContext::default();
        let y = centre(&r, BarTarget::TrayItem(0));
        assert_eq!(
            r.click(30.0, y, false, 1, none).unwrap(),
            Some(BarPointerIntent::TrayActivate {
                index: 0,
                secondary: false
            })
        );
        assert_eq!(
            r.click(30.0, y, true, 2, none).unwrap(),
            Some(BarPointerIntent::TrayActivate {
                index: 0,
                secondary: true
            })
        );
        // 11 px is inside the 40 px column but outside the centred tray item.
        assert_eq!(r.click(11.0, y, false, 3, none).unwrap(), None);
    }

    #[test]
    fn active_window_click_toggles_only_when_hover_is_disabled() {
        let mut r = router(BarPointerConfig {
            active_window_show_on_hover: false,
            ..BarPointerConfig::default()
        });
        let aw = centre(&r, BarTarget::ActiveWindow);
        assert!(
            r.pointer_moved(30.0, aw, 1, PopoutContext::default())
                .unwrap()
                .is_empty()
        );
        let tray_open = PopoutContext {
            current: Some(BarPopout::TrayMenu(0)),
            ..PopoutContext::default()
        };
        assert_eq!(
            r.pointer_moved(30.0, aw, 2, tray_open).unwrap(),
            vec![BarPointerIntent::Popout(PopoutChange::Hide)]
        );
        assert!(matches!(
            r.click(30.0, aw, false, 3, PopoutContext::default())
                .unwrap(),
            Some(BarPointerIntent::Popout(PopoutChange::Show {
                popout: BarPopout::ActiveWindow,
                ..
            }))
        ));
        let open = PopoutContext {
            current: Some(BarPopout::ActiveWindow),
            ..PopoutContext::default()
        };
        assert_eq!(
            r.click(30.0, aw, false, 4, open).unwrap(),
            Some(BarPointerIntent::Popout(PopoutChange::Hide))
        );
    }
}
