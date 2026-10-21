//! BAR-01I (B08): the Bar's modules reordered by dragging the Bar itself.
//!
//! - **A long press lifts.** A press resting on a module for the lift hold
//!   (SET-03 `lift_press_ms`, 350 ms by default) lifts it;
//!   a press that travels first, or a release before, is the Bar's as
//!   always (hover, popouts and wheel are untouched).
//! - **The Bar answers.** While a module is carried, the Bar lays out the
//!   order it would have: the slot the module would take stays open, the
//!   others slide (the settings reflow), and the module follows the pointer
//!   as a ghost. Off the Bar the slot closes: dropped there it is hidden.
//! - **The drop is a profile edit.** The new order goes to the profile
//!   authority (R12) as the `modules` override; until the profile reflects
//!   it, the Bar keeps showing it, so nothing snaps back and forth.

use super::*;
use crate::bar_strip::{ModulePlacement, insertion, moved};
use crate::reference_bar::{BarModuleOrder, IdleBarEntry};

/// How long a press rests before it lifts a module.
/// A press that travels further before the lift is not a lift.
const SLOP: f32 = 6.0;
/// How long the Bar waits for the profile to reflect a drop.
const PENDING: Duration = Duration::from_secs(3);

#[derive(Clone, Debug)]
pub(super) struct BarPress {
    entry: IdleBarEntry,
    x: f32,
    y: f32,
    token: u64,
}

#[derive(Clone, Debug)]
pub(super) struct BarDrag {
    pub(super) entry: IdleBarEntry,
    pub(super) pointer: (f32, f32),
    /// The module's footprint at rest.
    size: (f32, f32),
    /// Where it would land: (group, index), or None (hidden).
    pub(super) target: Option<(usize, usize)>,
    /// The order before the lift.
    from: BarModuleOrder,
}

fn group_name(target: Option<(usize, usize)>) -> String {
    match target {
        Some((g, i)) => format!("{}:{i}", ["start", "centre", "end"][g.min(2)]),
        None => "hidden".to_owned(),
    }
}

/// The order the Bar shows now: a carried module's preview, a dropped
/// order the profile has not reflected yet, or the profile's. A free
/// function over the fields it needs, so the scene can call it while it
/// holds its reference projection.
pub(super) fn effective_order(
    drag: &Option<BarDrag>,
    pending: &mut Option<(BarModuleOrder, Instant)>,
    profile: &BarModuleOrder,
) -> BarModuleOrder {
    if let Some(drag) = drag {
        return moved(&drag.from, drag.entry, drag.target);
    }
    if let Some((order, since)) = pending {
        if order == profile || since.elapsed() > PENDING {
            if order != profile {
                println!("GNOS_UX_BAR_DRAG state=reverted reason=profile_unchanged");
            }
            *pending = None;
        } else {
            return order.clone();
        }
    }
    profile.clone()
}

impl SceneHost {
    /// SURF-01B: whether `(x, y)` is on the status icons at rest.
    pub(super) fn on_status_icons(&self, x: f32, y: f32) -> bool {
        self.bar_module_at(x, y)
            .is_some_and(|m| m.entry == IdleBarEntry::StatusIcons)
    }

    pub(super) fn on_overflow_button(&self, x: f32, y: f32) -> bool {
        self.bar_module_at(x, y)
            .is_some_and(|m| m.entry == IdleBarEntry::Overflow)
    }

    pub(super) fn on_power_button(&self, x: f32, y: f32) -> bool {
        self.bar_module_at(x, y)
            .is_some_and(|m| m.entry == IdleBarEntry::Power)
    }

    /// The module at rest under `(x, y)`, on the rail's current side.
    pub(super) fn bar_module_at(&self, x: f32, y: f32) -> Option<ModulePlacement> {
        let (side, placements) = self.bar_rest_previous.as_ref()?;
        (*side == self.rail_side())
            .then(|| {
                placements
                    .iter()
                    .rev()
                    .find(|m| m.clip.contains(x, y))
                    .copied()
            })
            .flatten()
    }

    /// A primary press. Returns whether it is the Bar's (a module, waiting
    /// to lift); the press still reaches the Bar's other handlers.
    pub(super) fn bar_module_down(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        if self.bar_travelling || self.settings.is_open() || self.bar_drag.is_some() {
            return false;
        }
        let Some(module) = self.bar_module_at(x, y) else {
            return false;
        };
        self.bar_press_tokens += 1;
        let token = self.bar_press_tokens;
        self.bar_press = Some(BarPress {
            entry: module.entry,
            x,
            y,
            token,
        });
        cx.spawn(async move |this, cx| {
            // SET-03: the hold is the operator's.
            let lift = crate::surface_prefs::advanced_ms("lift_press_ms");
            cx.background_executor().timer(lift).await;
            let _ = this.update(cx, |view, cx| view.bar_long_press(token, cx));
        })
        .detach();
        true
    }

    fn bar_long_press(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some(press) = self.bar_press.take().filter(|p| p.token == token) else {
            return;
        };
        let Some(module) = self.bar_module_at(press.x, press.y) else {
            return;
        };
        let from = self.bar_rest_order.clone().unwrap_or_default();
        let target =
            from.0.iter().enumerate().find_map(|(g, group)| {
                group.iter().position(|e| *e == press.entry).map(|i| (g, i))
            });
        println!(
            "GNOS_UX_BAR_DRAG state=lift module={:?} at={}",
            press.entry,
            group_name(target)
        );
        self.bar_drag = Some(BarDrag {
            entry: press.entry,
            pointer: (press.x, press.y),
            size: (module.rect.width, module.rect.height),
            target,
            from,
        });
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// The pointer moved: a waiting press that travels is no lift; a
    /// carried module follows and aims.
    pub(super) fn bar_module_moved(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if let Some(press) = &self.bar_press
            && (x - press.x).hypot(y - press.y) > SLOP
        {
            self.bar_press = None;
        }
        let Some((entry, from)) = self.bar_drag.as_ref().map(|d| (d.entry, d.from.clone())) else {
            return;
        };
        let side = self.rail_side();
        let viewport = self.bar_viewport;
        let (rx, ry, rw, rh) = side.rail_rect(viewport);
        // Off the Bar (past a reach around the rail): hidden.
        // SET-03: the operator's reach.
        let reach = crate::surface_prefs::advanced("hide_reach_px") as f32;
        let on_bar =
            x >= rx - reach && x <= rx + rw + reach && y >= ry - reach && y <= ry + rh + reach;
        // Still over its own place at rest: it stays where it was. (The
        // others close the gap once it is lifted, so measured against them
        // the pointer would fall into the next slot.)
        let own = self
            .bar_rest_previous
            .as_ref()
            .filter(|(s, _)| *s == side)
            .and_then(|(_, placements)| placements.iter().find(|m| m.entry == entry))
            .is_some_and(|m| {
                if side.is_horizontal() {
                    x >= m.rect.x && x < m.rect.x + m.rect.width
                } else {
                    y >= m.rect.y && y < m.rect.y + m.rect.height
                }
            });
        let home = from
            .0
            .iter()
            .enumerate()
            .find_map(|(g, group)| group.iter().position(|e| *e == entry).map(|i| (g, i)));
        let target = on_bar.then(|| {
            if own && let Some(home) = home {
                return home;
            }
            let without = moved(&from, entry, None);
            let others = self
                .bar_rest_others
                .as_ref()
                .map(|(_, p)| p.clone())
                .unwrap_or_default();
            let horizontal = side.is_horizontal();
            let (along, bar) = if horizontal {
                (x, (rx, rx + rw))
            } else {
                (y, (ry, ry + rh))
            };
            insertion(&others, &without, horizontal, along, bar)
        });
        let drag = self.bar_drag.as_mut().expect("carried");
        drag.pointer = (x, y);
        if target != drag.target {
            drag.target = target;
            println!(
                "GNOS_UX_BAR_DRAG state=target module={:?} to={} pointer={x:.1},{y:.1}",
                drag.entry,
                group_name(target)
            );
        }
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// The release. Returns whether it ended a carry.
    pub(super) fn bar_module_up(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let press = self.bar_press.take();
        let Some(drag) = self.bar_drag.take() else {
            // SURF-01B (U03): a click (no lift, no travel) on the status
            // icons opens or closes Utilities.
            // SURF-02B (S01): a click on the power button opens or closes
            // the session panel.
            match press.as_ref().map(|p| p.entry) {
                Some(IdleBarEntry::StatusIcons) if !self.settings.is_open() => {
                    self.toggle_utilities(cx);
                    return true;
                }
                Some(IdleBarEntry::Power) if !self.settings.is_open() => {
                    self.toggle_session(cx);
                    return true;
                }
                // BAR-02A: a click on the "…" opens or closes the overflow panel.
                Some(IdleBarEntry::Overflow)
                    if !self.settings.is_open() && !self.bar_travelling =>
                {
                    self.toggle_overflow(cx);
                    return true;
                }
                // SURF-03B (H04): a click on the clock pins the dashboard
                // (or, pinned, closes it).
                Some(IdleBarEntry::Clock) if !self.settings.is_open() && !self.bar_travelling => {
                    self.attention_toggle(crate::attention::Attendant::Dashboard, cx);
                    self.report_dashboard("pin");
                    return true;
                }
                // RAIL-01: every other module's click (tray, workspaces, logo).
                Some(_) if !self.settings.is_open() && !self.bar_travelling => {
                    if let Some(press) = press {
                        return self.rail_click(press.x, press.y, false, window, cx);
                    }
                }
                _ => {}
            }
            return false;
        };
        let order = moved(&drag.from, drag.entry, drag.target);
        let outcome = if order == drag.from {
            "UNCHANGED"
        } else {
            self.write_module_order(&order, cx)
        };
        println!(
            "GNOS_UX_BAR_DRAG state=drop module={:?} to={} outcome={outcome}",
            drag.entry,
            group_name(drag.target)
        );
        if outcome == "SENT" {
            self.bar_order_pending = Some((order, Instant::now()));
        }
        self.start_motion_driver(cx);
        cx.notify();
        true
    }

    /// Escape: a carried module goes back where it was.
    pub(super) fn bar_drag_escape(&mut self, cx: &mut Context<Self>) -> bool {
        self.bar_press = None;
        let Some(drag) = self.bar_drag.take() else {
            return false;
        };
        println!(
            "GNOS_UX_BAR_DRAG state=drop module={:?} to=cancel outcome=CANCELLED",
            drag.entry
        );
        self.start_motion_driver(cx);
        cx.notify();
        true
    }

    /// The dragged module's ghost, centred on the pointer, in the form of
    /// the rail's side.
    pub(super) fn bar_ghost(&self) -> Option<ModulePlacement> {
        let drag = self.bar_drag.as_ref()?;
        let (w, h) = drag.size;
        let rect = blobin_ui_core::layout::Rect::new(
            drag.pointer.0 - w / 2.0,
            drag.pointer.1 - h / 2.0,
            w,
            h,
        );
        Some(ModulePlacement {
            entry: drag.entry,
            rect,
            clip: rect,
            horizontal: self.rail_side().is_horizontal(),
        })
    }

    /// Send the new module order to the profile authority.
    pub(super) fn write_module_order(
        &mut self,
        order: &BarModuleOrder,
        cx: &mut Context<Self>,
    ) -> &'static str {
        #[cfg(feature = "r12-production-client")]
        {
            use in_gnu_os_settings_read_model::{BarModule as M, BarModuleLayout};
            let module = |e: &IdleBarEntry| match e {
                IdleBarEntry::Logo => Some(M::Logo),
                IdleBarEntry::Workspaces => Some(M::Workspaces),
                IdleBarEntry::ActiveWindow => Some(M::ActiveWindow),
                IdleBarEntry::Tray => Some(M::Tray),
                IdleBarEntry::Clock => Some(M::Clock),
                IdleBarEntry::StatusIcons => Some(M::StatusIcons),
                IdleBarEntry::Power => Some(M::Power),
                IdleBarEntry::Spacer | IdleBarEntry::Overflow => None,
            };
            let groups: [Vec<M>; 3] = order
                .0
                .clone()
                .map(|g| g.iter().filter_map(module).collect());
            let Ok(layout) = BarModuleLayout::from_groups([&groups[0], &groups[1], &groups[2]])
            else {
                return "INVALID";
            };
            let mut overrides = self.product_runtime.preferences().bar_overrides.clone();
            overrides.modules = Some(layout);
            let session = crate::product_profile::ProfileSession::shared(cx);
            session.update(cx, |session, cx| session.select_preferences(overrides, cx));
            "SENT"
        }
        #[cfg(not(feature = "r12-production-client"))]
        {
            let _ = (order, cx);
            "NO_CLIENT"
        }
    }
}
