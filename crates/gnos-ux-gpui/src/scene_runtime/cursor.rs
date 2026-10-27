//! CURSOR-01: the pointer's shape in the scene.
//!
//! Each frame the scene asks what is under way (a window or a module or a
//! pin carried; an action waiting for its answer) and what the pointer is
//! over, the topmost surface first, and gives the root the one shape
//! `crate::cursor::cursor_style` names. The compositor draws it from the
//! operator's cursor theme. Surfaces drawn as elements of their own (the
//! tray menu's rows, Now Playing, the AppStack's search field) set theirs on
//! those elements. Receipt: `GNOS_UX_CURSOR`, on change.

use super::*;
use crate::cursor::{Affordance, Carry, CursorIntent};
use crate::reference_bar::IdleBarEntry;

impl SceneHost {
    /// The pointer moved to `at` on the scene, or (`None`) left it.
    pub(super) fn cursor_pointer(&mut self, at: Option<(f32, f32)>) {
        self.cursor_at = at;
    }

    /// What is carried, and whether the place under the pointer takes it.
    fn cursor_carry(&self, (x, y): (f32, f32)) -> Option<Carry> {
        if let Some(carry) = self.workspaces.carry_at(x, y) {
            return Some(carry);
        }
        if let Some(drag) = &self.deck_drag {
            return Some(Carry {
                drop: Some(drag.target() != crate::deck::drag::DropTarget::Cancel),
            });
        }
        self.bar_drag.as_ref().map(|_| Carry { drop: None })
    }

    /// What the pointer is over: the topmost surface that answers.
    fn cursor_over(&self, (x, y): (f32, f32)) -> Affordance {
        if self.settings.is_open()
            && let Some(a) = self.settings.affordance_at(x, y)
        {
            return a;
        }
        if let Some(a) = self.bar_popout.affordance_at(x, y) {
            // The dashboard's own buttons.
            if let Some((lx, ly)) = self.bar_popout.dashboard_point(x, y) {
                return if self.dashboard.presses_at(lx, ly) {
                    Affordance::Press
                } else {
                    Affordance::Inert
                };
            }
            return a;
        }
        let panels = [
            self.workspaces.affordance_at(x, y),
            self.session.affordance_at(x, y),
            self.utilities.affordance_at(x, y),
            self.overflow.affordance_at(x, y),
            self.notification_toasts.affordance_at(x, y),
            self.notification_centre.affordance_at(x, y),
            self.stack.affordance_at(x, y),
        ];
        if let Some(a) = panels.into_iter().flatten().next() {
            return a;
        }
        if self
            .deck_shape
            .as_ref()
            .is_some_and(|deck| deck.card_at(x, y).is_some())
        {
            return Affordance::Press;
        }
        match self.bar_module_at(x, y).map(|m| m.entry) {
            // A tray item whose press opens its menu says so.
            Some(IdleBarEntry::Tray) => match self.bar_popout.designated() {
                Some(crate::bar_pointer_routing::BarPopout::TrayMenu(index)) => {
                    match self.rail_tray_item(index) {
                        Some(item)
                            if crate::product_tray_action::primary_interaction_policy(
                                item.item_is_menu,
                                item.menu_path.is_some(),
                            )
                                == crate::product_tray_action::PrimaryTrayInteraction::OpenMenu =>
                        {
                            Affordance::Menu
                        }
                        Some(_) => Affordance::Press,
                        None => Affordance::Inert,
                    }
                }
                _ => Affordance::Inert,
            },
            Some(_) => Affordance::Press,
            None => Affordance::Inert,
        }
    }

    /// An action waiting for its answer, the interface still usable.
    fn cursor_busy(&self) -> bool {
        let tray_asked = self
            .rail_tray_menu_key
            .is_some_and(|k| self.attention.is_out(crate::attention::Attendant::Tray(k)))
            && !self.tray_menu.is_shown();
        // A pin waiting for its profile write (a build with the profile).
        #[cfg(feature = "r12-production-client")]
        let pins = self.deck_pins_pending.is_some();
        #[cfg(not(feature = "r12-production-client"))]
        let pins = false;
        self.utilities.busy() || self.session.busy() || pins || tray_asked || self.launching()
    }

    /// The shape for this frame; `GNOS_UX_CURSOR` when it changes.
    pub(super) fn cursor_style(&mut self) -> gpui::CursorStyle {
        let intent = match self.cursor_at {
            Some(at) => CursorIntent {
                over: self.cursor_over(at),
                carry: self.cursor_carry(at),
                busy: self.cursor_busy(),
            },
            None => CursorIntent::default(),
        };
        let style = crate::cursor::cursor_style(intent);
        if self.cursor_reported != Some(style) {
            println!(
                "GNOS_UX_CURSOR shape={} over={:?} carry={} busy={}",
                crate::cursor::css_name(style),
                intent.over,
                intent
                    .carry
                    .map_or("none".to_owned(), |c| format!("{:?}", c.drop)),
                intent.busy
            );
            self.cursor_reported = Some(style);
        }
        style
    }
}
