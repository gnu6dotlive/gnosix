//! The rendered tray menu surface: presentation model, binding identity, and
//! lifetime.
//!
//! A rendered row means "the exact semantic row I rendered from this exact
//! menu generation", never "click whatever item currently has this integer
//! id". Every actionable row carries an immutable [`TrayMenuEventBinding`];
//! invoking a row whose menu generation has since moved on is refused.
//!
//! A menu belongs to the interaction instance that opened it. Opening it from
//! one output renders one surface attached to that output; another output never
//! receives a duplicate of the same request.

use crate::product_tray::TraySnapshot;
use crate::product_tray_action::{ExactTrayItemRef, TrayMenuEvent, TrayMenuEventBinding};
use crate::product_tray_menu::{
    TrayMenuIcon, TrayMenuItemType, TrayMenuSnapshot, TrayMenuToggleState, TrayMenuToggleType,
};

/// Why a previously open surface is no longer allowed to act.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenuDismissReason {
    /// The user dismissed it explicitly.
    Explicit,
    /// The tray item disappeared, so no row could remain valid.
    ItemDisappeared,
    /// The bus owner behind the item changed.
    OwnerChanged,
    /// The exporter stopped offering a menu.
    MenuUnavailable,
    /// A future interaction moved ownership to another output.
    OutputChanged,
    /// Keyboard focus or a policy rule required dismissal.
    FocusLost,
}

impl TrayMenuDismissReason {
    pub fn receipt(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::ItemDisappeared => "item_disappeared",
            Self::OwnerChanged => "owner_changed",
            Self::MenuUnavailable => "menu_unavailable",
            Self::OutputChanged => "output_changed",
            Self::FocusLost => "focus_lost",
        }
    }
}

/// One rendered row, with its role and its immutable binding when actionable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuRowPresentation {
    pub role: TrayMenuRowRole,
    pub label: Option<String>,
    pub enabled: bool,
    pub visible: bool,
    pub toggle_type: TrayMenuToggleType,
    pub toggle_state: TrayMenuToggleState,
    pub icon: TrayMenuIcon,
    pub has_submenu: bool,
    /// Depth inside the menu tree, for indentation.
    pub depth: usize,
    /// The binding a click on this row would dispatch. Present only when the
    /// row can actually carry an action.
    pub binding: Option<TrayMenuEventBinding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenuRowRole {
    Standard,
    Separator,
    Submenu,
}

impl TrayMenuRowPresentation {
    /// A row dispatches only when it is presentable, enabled, not a separator,
    /// not a submenu parent, and carries a binding.
    pub fn is_actionable(&self) -> bool {
        self.role == TrayMenuRowRole::Standard
            && self.enabled
            && self.visible
            && self.binding.is_some()
    }
}

/// The full rendered surface for one open menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuSurfacePresentation {
    /// Which tray item this surface belongs to.
    pub item: ExactTrayItemRef,
    /// Menu generation this surface was rendered from.
    pub revision: u32,
    /// Rows in layout order, flattened with their depth.
    pub rows: Vec<TrayMenuRowPresentation>,
}

impl TrayMenuSurfacePresentation {
    /// Projects a typed menu snapshot into rendered rows. Rows that are not
    /// visible produce no layout and consume no input, so they do not appear.
    pub fn from_snapshot(item: ExactTrayItemRef, menu: &TrayMenuSnapshot) -> Self {
        let mut rows = Vec::new();
        collect(&menu.root, item.clone(), menu.revision, 0, &mut rows);
        Self {
            item,
            revision: menu.revision,
            rows,
        }
    }

    pub fn actionable_rows(&self) -> impl Iterator<Item = &TrayMenuRowPresentation> {
        self.rows.iter().filter(|row| row.is_actionable())
    }

    /// Finds the row a binding belongs to, so a later invoke can be matched to
    /// the row that still represents it.
    pub fn row_for_binding(
        &self,
        binding: &TrayMenuEventBinding,
    ) -> Option<&TrayMenuRowPresentation> {
        self.rows
            .iter()
            .find(|row| row.binding.as_ref() == Some(binding))
    }
}

fn collect(
    node: &crate::product_tray_menu::TrayMenuItem,
    item: ExactTrayItemRef,
    revision: u32,
    depth: usize,
    rows: &mut Vec<TrayMenuRowPresentation>,
) {
    for child in &node.children {
        if !child.visible {
            // Hidden: no layout, no input.
            continue;
        }
        let role = match child.item_type {
            TrayMenuItemType::Separator => TrayMenuRowRole::Separator,
            TrayMenuItemType::Standard => {
                if matches!(
                    child.children_display,
                    crate::product_tray_menu::TrayMenuChildrenDisplay::Submenu
                ) && !child.children.is_empty()
                {
                    TrayMenuRowRole::Submenu
                } else {
                    TrayMenuRowRole::Standard
                }
            }
        };
        let binding = match role {
            TrayMenuRowRole::Standard if child.enabled => Some(TrayMenuEventBinding {
                item: item.clone(),
                menu_id: child.id,
                revision,
                event: TrayMenuEvent::Clicked,
                rendered_label: child.label.clone(),
            }),
            _ => None,
        };
        rows.push(TrayMenuRowPresentation {
            role,
            label: child.label.clone(),
            enabled: child.enabled,
            visible: child.visible,
            toggle_type: child.toggle_type,
            toggle_state: child.toggle_state,
            icon: child.icon.clone(),
            has_submenu: !child.children.is_empty(),
            depth,
            binding,
        });
        collect(child, item.clone(), revision, depth + 1, rows);
    }
}

/// The live state of the one menu surface.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuSurfaceState {
    #[default]
    Closed,
    Open {
        /// The output that owns this surface. Opening from another output is a
        /// different interaction instance.
        output: String,
        /// The item the menu belongs to.
        item: ExactTrayItemRef,
        /// The presentation rendered from the current revision.
        presentation: Box<TrayMenuSurfacePresentation>,
    },
}

impl TrayMenuSurfaceState {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }

    pub fn output(&self) -> Option<&str> {
        match self {
            Self::Open { output, .. } => Some(output),
            Self::Closed => None,
        }
    }

    pub fn presentation(&self) -> Option<&TrayMenuSurfacePresentation> {
        match self {
            Self::Open { presentation, .. } => Some(presentation.as_ref()),
            Self::Closed => None,
        }
    }

    /// Opens a surface for one item on one output. Opening it again on another
    /// output is refused rather than duplicated.
    pub fn open(
        &mut self,
        output: impl Into<String>,
        item: ExactTrayItemRef,
        menu: &TrayMenuSnapshot,
    ) -> Result<(), OpenRefused> {
        if let Self::Open {
            output: current, ..
        } = self
        {
            return Err(OpenRefused::AlreadyOpen {
                owned_by: current.clone(),
            });
        }
        *self = Self::Open {
            output: output.into(),
            presentation: Box::new(TrayMenuSurfacePresentation::from_snapshot(
                item.clone(),
                menu,
            )),
            item,
        };
        Ok(())
    }

    /// Replaces the rendered presentation when the layout legitimately moves on.
    ///
    /// This is normal product behaviour, and the freshness invariant is kept
    /// because every action is revalidated before dispatch, not because the old
    /// rows were never replaced.
    pub fn revise(&mut self, menu: &TrayMenuSnapshot) -> bool {
        match self {
            Self::Open { presentation, .. } => {
                let next =
                    TrayMenuSurfacePresentation::from_snapshot(presentation.item.clone(), menu);
                if next == **presentation {
                    return false;
                }
                **presentation = next;
                true
            }
            Self::Closed => false,
        }
    }

    pub fn close(&mut self) -> TrayMenuDismissReason {
        let reason = match self {
            Self::Open { .. } => TrayMenuDismissReason::Explicit,
            Self::Closed => TrayMenuDismissReason::Explicit,
        };
        *self = Self::Closed;
        reason
    }

    /// The gate a stale cached binding must survive: if the item, the owner,
    /// or the menu has moved on, the surface must refuse to keep acting.
    pub fn validate_against(
        &mut self,
        snapshot: &TraySnapshot,
        current_owner: impl Fn(&str) -> Option<String>,
    ) -> Result<(), TrayMenuDismissReason> {
        let (item, _) = match self {
            Self::Open { item, .. } => (item.clone(), ()),
            Self::Closed => return Ok(()),
        };

        let still_present = snapshot.items().iter().any(|live| {
            live.id == item.id && live.service == item.service && live.path == item.path
        });
        if !still_present {
            *self = Self::Closed;
            return Err(TrayMenuDismissReason::ItemDisappeared);
        }
        match current_owner(&item.service) {
            Some(owner) if owner == item.owner => {}
            Some(_) => {
                *self = Self::Closed;
                return Err(TrayMenuDismissReason::OwnerChanged);
            }
            None => {
                *self = Self::Closed;
                return Err(TrayMenuDismissReason::ItemDisappeared);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenRefused {
    AlreadyOpen { owned_by: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray::{TrayCategory, TrayItemId, TrayItemSnapshot, TrayStatus};
    use crate::product_tray_menu::TrayMenuItem;

    fn item(id: &str, owner: &str, item_is_menu: bool) -> TrayItemSnapshot {
        TrayItemSnapshot {
            id: TrayItemId::new(id),
            service: format!("org.kde.StatusNotifierItem-{id}"),
            path: "/StatusNotifierItem".to_owned(),
            title: id.to_owned(),
            status: TrayStatus::Active,
            category: TrayCategory::ApplicationStatus,
            icons: crate::product_tray::TrayIconFacts::named("probe"),
            tooltip: None,
            menu_path: Some("/MenuBar".to_owned()),
            item_is_menu,
            owner: owner.to_owned(),
        }
    }

    fn row(id: i32, label: &str, enabled: bool) -> TrayMenuItem {
        TrayMenuItem {
            id,
            label: Some(label.to_owned()),
            enabled,
            visible: true,
            icon: TrayMenuIcon::None,
            item_type: TrayMenuItemType::Standard,
            toggle_type: TrayMenuToggleType::NotCheckable,
            toggle_state: TrayMenuToggleState::Unchecked,
            children_display: crate::product_tray_menu::TrayMenuChildrenDisplay::None,
            children: Vec::new(),
        }
    }

    fn menu(revision: u32, rows: Vec<TrayMenuItem>) -> TrayMenuSnapshot {
        TrayMenuSnapshot {
            revision,
            status: Default::default(),
            version: 3,
            text_direction: Default::default(),
            icon_theme_path: None,
            root: TrayMenuItem {
                children: rows,
                ..TrayMenuItem::default()
            },
        }
    }

    fn exact(item: &TrayItemSnapshot) -> ExactTrayItemRef {
        ExactTrayItemRef::from_observed(item, item.owner.clone())
    }

    #[test]
    fn a_rendered_row_binds_its_exact_item_id_and_revision() {
        let menu = menu(42, vec![row(7, "Quit", true)]);
        let surface =
            TrayMenuSurfacePresentation::from_snapshot(exact(&item("steam", ":1.9", true)), &menu);

        let row = surface
            .actionable_rows()
            .next()
            .expect("one actionable row");
        let binding = row.binding.as_ref().expect("binding present");
        assert_eq!(binding.menu_id, 7);
        assert_eq!(
            binding.revision, 42,
            "the row is bound to its generation, not to whatever exists now"
        );
        assert_eq!(binding.rendered_label.as_deref(), Some("Quit"));
        assert_eq!(binding.item.id.as_str(), "steam");
    }

    #[test]
    fn separators_and_disabled_rows_do_not_bind_and_cannot_dispatch() {
        let mut separator = row(8, "——", true);
        separator.item_type = TrayMenuItemType::Separator;
        let menu = menu(1, vec![separator, row(9, "Dim", false), row(2, "Go", true)]);
        let surface =
            TrayMenuSurfacePresentation::from_snapshot(exact(&item("app", ":1.1", true)), &menu);

        let rows: Vec<_> = surface.rows.iter().collect();
        assert_eq!(
            rows.len(),
            3,
            "hidden rows would be omitted, visible ones stay"
        );
        assert_eq!(rows[0].role, TrayMenuRowRole::Separator);
        assert!(!rows[0].is_actionable(), "a separator never dispatches");
        assert!(!rows[1].is_actionable(), "a disabled row never dispatches");
        assert!(rows[2].is_actionable());

        assert_eq!(surface.actionable_rows().count(), 1);
    }

    #[test]
    fn hidden_rows_produce_no_layout_and_no_binding() {
        let mut hidden = row(3, "Invisible", true);
        hidden.visible = false;
        let menu = menu(1, vec![hidden, row(4, "Seen", true)]);
        let surface =
            TrayMenuSurfacePresentation::from_snapshot(exact(&item("app", ":1.1", true)), &menu);

        assert_eq!(surface.rows.len(), 1, "hidden rows consume no layout");
        assert_eq!(surface.rows[0].label.as_deref(), Some("Seen"));
    }

    #[test]
    fn a_submenu_parent_is_visible_but_does_not_dispatch() {
        let mut parent = row(5, "Store", true);
        parent.children_display = crate::product_tray_menu::TrayMenuChildrenDisplay::Submenu;
        parent.children.push(row(6, "Wishlists", true));
        let menu = menu(1, vec![parent]);
        let surface =
            TrayMenuSurfacePresentation::from_snapshot(exact(&item("app", ":1.1", true)), &menu);

        let parent = surface
            .rows
            .iter()
            .find(|r| r.label.as_deref() == Some("Store"))
            .unwrap();
        assert_eq!(parent.role, TrayMenuRowRole::Submenu);
        assert!(
            !parent.is_actionable(),
            "a submenu parent opens the branch, it does not dispatch"
        );
        let leaf = surface
            .rows
            .iter()
            .find(|r| r.label.as_deref() == Some("Wishlists"))
            .unwrap();
        assert!(leaf.is_actionable());
    }

    #[test]
    fn a_menu_opens_on_one_output_and_refuses_a_second_open() {
        let mut state = TrayMenuSurfaceState::default();
        let menu = menu(1, vec![row(1, "Row", true)]);
        let app = item("app", ":1.1", true);

        assert!(state.open("DP-2", exact(&app), &menu).is_ok());
        let second = state.open("HDMI-A-1", exact(&app), &menu);
        assert_eq!(
            second,
            Err(OpenRefused::AlreadyOpen {
                owned_by: "DP-2".to_owned()
            }),
            "a second output must not receive a duplicate of the same request"
        );
        assert_eq!(state.output(), Some("DP-2"));
    }

    #[test]
    fn an_open_surface_revise_replaces_rows_without_reopening() {
        let mut state = TrayMenuSurfaceState::default();
        let app = item("app", ":1.1", true);
        state
            .open("DP-2", exact(&app), &menu(1, vec![row(1, "Old", true)]))
            .unwrap();

        let changed = state.revise(&menu(2, vec![row(1, "New", true), row(2, "More", true)]));
        assert!(changed, "a moved-on layout must repaint");
        let presentation = state.presentation().unwrap();
        assert_eq!(presentation.revision, 2);
        assert_eq!(presentation.rows.len(), 2);
        assert_eq!(
            state.output(),
            Some("DP-2"),
            "revising does not move the surface to another output"
        );

        // Revising to an identical layout is not a change.
        assert!(!state.revise(&menu(2, vec![row(1, "New", true), row(2, "More", true)])));
    }

    #[test]
    fn the_surface_closes_when_the_item_disappears() {
        let mut state = TrayMenuSurfaceState::default();
        let app = item("app", ":1.1", true);
        state
            .open("DP-2", exact(&app), &menu(1, vec![row(1, "Row", true)]))
            .unwrap();

        let snapshot = TraySnapshot::from_items(vec![]);
        let outcome = state.validate_against(&snapshot, |_| Some(":1.1".to_owned()));
        assert_eq!(outcome, Err(TrayMenuDismissReason::ItemDisappeared));
        assert!(
            !state.is_open(),
            "a surface must not retain actionable authority after its item disappears"
        );
    }

    #[test]
    fn the_surface_closes_when_the_owner_changes() {
        let mut state = TrayMenuSurfaceState::default();
        let app = item("app", ":1.1", true);
        state
            .open("DP-2", exact(&app), &menu(1, vec![row(1, "Row", true)]))
            .unwrap();

        let snapshot = TraySnapshot::from_items(vec![app.clone()]);
        let outcome = state.validate_against(&snapshot, |_| Some(":1.99".to_owned()));
        assert_eq!(outcome, Err(TrayMenuDismissReason::OwnerChanged));
        assert!(!state.is_open());
    }

    #[test]
    fn the_surface_survives_a_valid_revalidation() {
        let mut state = TrayMenuSurfaceState::default();
        let app = item("app", ":1.1", true);
        state
            .open("DP-2", exact(&app), &menu(1, vec![row(1, "Row", true)]))
            .unwrap();

        let snapshot = TraySnapshot::from_items(vec![app]);
        assert_eq!(
            state.validate_against(&snapshot, |_| Some(":1.1".to_owned())),
            Ok(())
        );
        assert!(state.is_open());
    }

    #[test]
    fn reopening_after_close_succeeds_on_a_new_instance() {
        let mut state = TrayMenuSurfaceState::default();
        let app = item("app", ":1.1", true);
        state
            .open("DP-2", exact(&app), &menu(1, vec![row(1, "Row", true)]))
            .unwrap();
        state.close();

        assert!(
            state
                .open("HDMI-A-1", exact(&app), &menu(1, vec![row(1, "Row", true)]))
                .is_ok()
        );
        assert_eq!(
            state.output(),
            Some("HDMI-A-1"),
            "after a clean close the menu may be re-opened on another output"
        );
    }
}
