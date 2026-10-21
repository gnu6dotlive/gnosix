//! RAIL-01: what a click or the wheel on the native rail does.
//!
//! The router (`bar_pointer_routing`) has always resolved a click on the
//! rail to a typed intent, but the scene carried out only three: the status
//! icons (Utilities), the clock (the dashboard) and the power button (the
//! session panel). A click on the tray, a workspace or the logo did nothing,
//! nor did the wheel over the workspaces or the Bar's lower half. Here each
//! intent reaches the authority that already carries it out elsewhere:
//! - a tray item, through the tray interaction sink: a primary click
//!   activates it (or opens its menu, when the item says it is a menu); a
//!   secondary click opens its menu, or asks the item for its secondary
//!   action when it exports none. The menu opens beside the rail, at the
//!   item; a press outside it closes it.
//! - a workspace, through the R13 activation path; the wheel over the
//!   workspaces steps to the previous or next observed one;
//! - the logo: the AppStack in the Deck;
//! - the wheel in the Bar's lower half: this output's brightness, 5 % a
//!   notch (a display on this connector, else a backlight, else the first).
//!
//! Each intent is receipted (`GNOS_UX_RAIL_INTENT`).

use super::*;
use crate::bar_pointer_routing::{BarPointerIntent, StepDirection};
use crate::product_tray_action::{
    ExactTrayItemRef, PrimaryTrayInteraction, TrayIntent, TrayInteractionAnchor,
    primary_interaction_policy,
};

/// The brightness a wheel notch changes, in percent.
const BRIGHTNESS_STEP: u8 = 5;

impl SceneHost {
    /// A click (press and release on one module that did not lift it).
    pub(super) fn rail_click(
        &mut self,
        x: f32,
        y: f32,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(intent) = self.bar_popout.click(x, y, secondary) else {
            return false;
        };
        self.rail_intent(intent, (x, y), window, cx)
    }

    /// Carries out one rail intent; returns whether anything was done.
    pub(super) fn rail_intent(
        &mut self,
        intent: BarPointerIntent,
        at: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let done = match intent {
            BarPointerIntent::TrayActivate { index, secondary } => {
                self.rail_tray(index, secondary, at, cx)
            }
            BarPointerIntent::FocusWorkspace(index) => self.rail_workspace(index, cx),
            // WS-02: a click on the workspace already shown (the donor's
            // special-workspace toggle, which Gnosix does not carry out)
            // brings the workspaces panel out, or back in, in click mode.
            BarPointerIntent::ToggleSpecialWorkspace
                if crate::surface_prefs::current().workspaces_open
                    == crate::surface_prefs::PanelOpening::Click =>
            {
                self.attention_toggle(crate::attention::Attendant::Workspaces, cx);
                true
            }
            BarPointerIntent::WorkspaceStep(step) => {
                let targets = self.product_runtime.workspace_manager_targets();
                let active = targets
                    .iter()
                    .position(|w| self.product_runtime.workspace_manager_is_active(w));
                match active {
                    Some(active) => {
                        let next = active as isize + isize::from(step);
                        (0..targets.len() as isize).contains(&next)
                            && self.rail_workspace(next as usize, cx)
                    }
                    None => false,
                }
            }
            BarPointerIntent::ToggleLauncher => {
                let outcome = self.stack.toggle();
                self.apply_stack_outcome(outcome, window, cx);
                true
            }
            BarPointerIntent::BrightnessStep(direction) => self.rail_brightness(direction, cx),
            _ => false,
        };
        println!("GNOS_UX_RAIL_INTENT intent={intent:?} done={done}");
        done
    }

    /// The tray's item `index` among those the rail presents (not passive).
    pub(super) fn rail_tray_item(
        &self,
        index: usize,
    ) -> Option<crate::product_tray::TrayItemSnapshot> {
        let crate::product_tray::TraySnapshot::Active { items } = self.product_runtime.tray()
        else {
            return None;
        };
        items
            .iter()
            .filter(|item| item.status != crate::product_tray::TrayStatus::Passive)
            .nth(index)
            .cloned()
    }

    /// The place among the rail's items of the item a menu is open for.
    pub(super) fn rail_tray_index_of(&self, open: &ExactTrayItemRef) -> Option<usize> {
        let crate::product_tray::TraySnapshot::Active { items } = self.product_runtime.tray()
        else {
            return None;
        };
        items
            .iter()
            .filter(|item| item.status != crate::product_tray::TrayStatus::Passive)
            .position(|item| ExactTrayItemRef::from_observed(item, item.owner.clone()) == *open)
    }

    /// The number of items the rail presents.
    pub(super) fn rail_tray_count(&self) -> usize {
        match self.product_runtime.tray() {
            crate::product_tray::TraySnapshot::Active { items } => items
                .iter()
                .filter(|item| item.status != crate::product_tray::TrayStatus::Passive)
                .count(),
            _ => 0,
        }
    }

    pub(super) fn rail_tray(
        &mut self,
        index: usize,
        secondary: bool,
        at: (f32, f32),
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(item) = self.rail_tray_item(index) else {
            return false;
        };
        let target = ExactTrayItemRef::from_observed(&item, item.owner.clone());
        let has_menu = item.menu_path.is_some();
        let item_is_menu = item.item_is_menu;
        let Some(anchor) = TrayInteractionAnchor::from_local_point(self.output_origin, at) else {
            return false;
        };
        let open_menu = if secondary {
            has_menu
        } else {
            primary_interaction_policy(item_is_menu, has_menu) == PrimaryTrayInteraction::OpenMenu
        };
        // FOCUS-01C: a menu comes out through the shell's attention (held
        // by the click), after whatever else is out has gone in.
        if open_menu {
            self.attention_state_tray_at(index, at);
            self.attention_toggle(crate::attention::Attendant::Tray(index), cx);
            return true;
        }
        let intent = if secondary {
            TrayIntent::SecondaryActivate {
                item: target,
                anchor,
            }
        } else {
            TrayIntent::Activate {
                item: target,
                anchor,
            }
        };
        let Some(handler) = self.tray_interaction(cx) else {
            return false;
        };
        (handler.dispatch)(cx, intent);
        true
    }

    /// FOCUS-01C: ask item `index`'s application for its menu, to come out
    /// at `at` on the rail (the conductor's decision: a hover or a click).
    pub(super) fn tray_open_menu(
        &mut self,
        index: usize,
        at: (f32, f32),
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(item) = self.rail_tray_item(index) else {
            return false;
        };
        if item.menu_path.is_none() {
            return false;
        }
        let Some(anchor) = TrayInteractionAnchor::from_local_point(self.output_origin, at) else {
            return false;
        };
        let target = ExactTrayItemRef::from_observed(&item, item.owner.clone());
        self.rail_tray_menu_at = Some(at);
        self.rail_tray_menu_key = Some(index);
        // Already out (the pointer came back within the grace): asking
        // again would toggle it shut.
        let open = self.tray_controller.read(cx);
        if open.menu().is_some() && open.open_item() == Some(&target) {
            return true;
        }
        let intent = TrayIntent::OpenMenu {
            item: target,
            anchor,
        };
        let Some(handler) = self.tray_interaction(cx) else {
            return false;
        };
        println!("GNOS_UX_TRAY_MENU_REQUEST item={index}");
        (handler.dispatch)(cx, intent);
        true
    }

    /// A press outside an open tray menu closes it.
    pub(super) fn rail_tray_menu_dismiss(&mut self, cx: &mut Context<Self>) {
        // FOCUS-01C: the shell's attention hears it went in.
        if let Some(key) = self.rail_tray_menu_key {
            self.attention_close(&[crate::attention::Attendant::Tray(key)], cx);
        }
        self.rail_tray_menu_at = None;
        let cleared = self
            .tray_controller
            .update(cx, |controller, _| controller.clear_menu());
        if cleared {
            println!("GNOS_UX_TRAY_MENU_DISMISS reason=explicit");
            cx.notify();
        }
    }

    #[cfg(feature = "r13-production-client")]
    pub(super) fn rail_workspace(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(selected) = self
            .product_runtime
            .workspace_manager_targets()
            .get(index)
            .cloned()
        else {
            return false;
        };
        self.activate_workspace_intent(selected, cx);
        true
    }

    #[cfg(not(feature = "r13-production-client"))]
    pub(super) fn rail_workspace(&mut self, _index: usize, _cx: &mut Context<Self>) -> bool {
        false
    }

    fn rail_brightness(&mut self, direction: StepDirection, cx: &mut Context<Self>) -> bool {
        let output = self
            .runtime_output
            .as_ref()
            .map(|output| output.as_str().to_owned());
        let Some(index) = self.utilities_brightness_target(output.as_deref()) else {
            // Nothing read yet: read now; the next notch steps.
            self.utilities_detect_brightness(cx);
            return false;
        };
        let Some(current) = self.utilities_brightness_percent(index) else {
            return false;
        };
        let next = match direction {
            StepDirection::Increase => current.saturating_add(BRIGHTNESS_STEP).min(100),
            StepDirection::Decrease => current.saturating_sub(BRIGHTNESS_STEP),
        };
        if next == current {
            return false;
        }
        self.utilities_set_brightness(index, next, cx);
        true
    }
}
