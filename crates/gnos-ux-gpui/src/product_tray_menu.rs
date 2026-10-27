//! Typed `com.canonical.dbusmenu` read model.
//!
//! Raw `a{sv}` never reaches GPUI. Vendor and unknown properties are either
//! projected into a typed field or deliberately ignored, and a single
//! malformed entry is dropped rather than poisoning the whole menu.
//!
//! The layout `revision` is carried explicitly. A menu action is only valid
//! against the revision it was rendered from, which is what keeps a stale
//! click from landing on a semantically different entry.

use crate::product_tray_action::{StaleReason, TrayMenuEvent, TrayMenuEventBinding};

/// Largest inline icon payload the read model will describe.
///
/// Real exporters ship raw PNG in `icon-data`; the bytes stay in the provider
/// and only the measured size is projected.
pub const MAX_INLINE_ICON_BYTES: usize = 256 * 1024;

/// Menu-level status, as published by the exporter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuStatus {
    #[default]
    Normal,
    Notice,
}

impl TrayMenuStatus {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "notice" => Self::Notice,
            _ => Self::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuTextDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

impl TrayMenuTextDirection {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "rtl" => Self::RightToLeft,
            _ => Self::LeftToRight,
        }
    }
}

/// How a menu entry presents itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuItemType {
    #[default]
    Standard,
    Separator,
}

impl TrayMenuItemType {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "separator" => Self::Separator,
            _ => Self::Standard,
        }
    }

    pub fn is_separator(self) -> bool {
        matches!(self, Self::Separator)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuToggleType {
    #[default]
    NotCheckable,
    Checkable,
    Radio,
}

impl TrayMenuToggleType {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "checkmark" => Self::Checkable,
            "radio" => Self::Radio,
            _ => Self::NotCheckable,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuToggleState {
    #[default]
    Unchecked,
    Checked,
    Inconsistent,
}

impl TrayMenuToggleState {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "checked" => Self::Checked,
            "inconsistent" | "indeterminate" => Self::Inconsistent,
            _ => Self::Unchecked,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuChildrenDisplay {
    #[default]
    Submenu,
    Inline,
    None,
}

impl TrayMenuChildrenDisplay {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "inline" => Self::Inline,
            "none" => Self::None,
            _ => Self::Submenu,
        }
    }
}

/// Typed icon for a menu entry.
///
/// Inline image data is reported by size only. The bytes never cross into
/// product presentation, and an oversized payload is dropped rather than
/// carried.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum TrayMenuIcon {
    Named(String),
    #[default]
    None,
    InlineData {
        byte_len: usize,
    },
}

impl TrayMenuIcon {
    pub fn inline_data(byte_len: usize) -> Self {
        if byte_len <= MAX_INLINE_ICON_BYTES {
            Self::InlineData { byte_len }
        } else {
            Self::None
        }
    }
}

/// One menu entry, fully typed.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TrayMenuItem {
    /// Identifier meaningful only within one layout revision.
    pub id: i32,
    pub label: Option<String>,
    pub enabled: bool,
    pub visible: bool,
    pub icon: TrayMenuIcon,
    pub item_type: TrayMenuItemType,
    pub toggle_type: TrayMenuToggleType,
    pub toggle_state: TrayMenuToggleState,
    pub children_display: TrayMenuChildrenDisplay,
    pub children: Vec<TrayMenuItem>,
}

impl TrayMenuItem {
    /// An entry is presentable only when it is visible.
    pub fn is_presentable(&self) -> bool {
        self.visible
    }

    /// An entry is actionable only when it is visible, enabled, not a
    /// separator, and not a submenu parent.
    pub fn is_actionable(&self) -> bool {
        self.visible && self.enabled && !self.item_type.is_separator() && self.children.is_empty()
    }

    /// Depth-first search for an entry id within this subtree.
    pub fn find(&self, id: i32) -> Option<&TrayMenuItem> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    /// Flattens presentable entries in layout order.
    pub fn presentable_leaves(&self) -> Vec<&TrayMenuItem> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves<'a>(&'a self, out: &mut Vec<&'a TrayMenuItem>) {
        if self.children.is_empty() {
            if self.is_presentable() {
                out.push(self);
            }
            return;
        }
        for child in &self.children {
            child.collect_leaves(out);
        }
        if self.is_presentable() {
            out.push(self);
        }
    }
}

/// A projected menu at one layout revision.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TrayMenuSnapshot {
    pub revision: u32,
    pub status: TrayMenuStatus,
    pub version: u32,
    pub text_direction: TrayMenuTextDirection,
    pub icon_theme_path: Option<String>,
    pub root: TrayMenuItem,
}

impl TrayMenuSnapshot {
    /// Root of the projected menu. Menu id `0` is the protocol root.
    pub fn root(&self) -> &TrayMenuItem {
        &self.root
    }

    /// Depth-first lookup of an entry id within this revision.
    pub fn find_item(&self, id: i32) -> Option<&TrayMenuItem> {
        self.root.find(id)
    }

    pub fn item_count(&self) -> usize {
        self.presentable_items().len()
    }

    /// Presentable rows, excluding the protocol root, which is a container
    /// rather than something a user can point at.
    pub fn presentable_items(&self) -> Vec<&TrayMenuItem> {
        self.root
            .children
            .iter()
            .flat_map(|child| child.presentable_leaves())
            .collect()
    }

    pub fn actionable_items(&self) -> Vec<&TrayMenuItem> {
        self.presentable_items()
            .into_iter()
            .filter(|item| item.is_actionable())
            .collect()
    }

    /// Binds an entry to the exact item and revision it was rendered from.
    pub fn bind(
        &self,
        item: crate::product_tray_action::ExactTrayItemRef,
        menu_id: i32,
        event: TrayMenuEvent,
    ) -> Option<TrayMenuEventBinding> {
        self.find_item(menu_id).map(|entry| TrayMenuEventBinding {
            item,
            menu_id,
            revision: self.revision,
            event,
            rendered_label: entry.label.clone(),
        })
    }

    /// Re-checks that a previously bound action still denotes its target.
    pub fn revalidate(&self, binding: &TrayMenuEventBinding) -> Result<(), StaleReason> {
        binding.revalidate(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray::TrayItemId;
    use crate::product_tray_action::ExactTrayItemRef;

    fn entry(id: i32, label: Option<&str>) -> TrayMenuItem {
        TrayMenuItem {
            id,
            label: label.map(str::to_owned),
            enabled: true,
            visible: true,
            icon: TrayMenuIcon::Named("probe".to_owned()),
            item_type: TrayMenuItemType::Standard,
            toggle_type: TrayMenuToggleType::NotCheckable,
            toggle_state: TrayMenuToggleState::Unchecked,
            children_display: TrayMenuChildrenDisplay::Submenu,
            children: Vec::new(),
        }
    }

    fn separator(id: i32) -> TrayMenuItem {
        let mut item = entry(id, None);
        item.item_type = TrayMenuItemType::Separator;
        item
    }

    fn submenu(id: i32, label: &str, children: Vec<TrayMenuItem>) -> TrayMenuItem {
        let mut item = entry(id, Some(label));
        item.children = children;
        item
    }

    fn snapshot(revision: u32, root: TrayMenuItem) -> TrayMenuSnapshot {
        TrayMenuSnapshot {
            revision,
            status: TrayMenuStatus::Normal,
            version: 3,
            text_direction: TrayMenuTextDirection::LeftToRight,
            icon_theme_path: None,
            root,
        }
    }

    fn item_ref() -> ExactTrayItemRef {
        ExactTrayItemRef {
            id: TrayItemId::new("steam"),
            service: "org.kde.StatusNotifierItem-steam".to_owned(),
            path: "/StatusNotifierItem".to_owned(),
            owner: ":1.42".to_owned(),
        }
    }

    #[test]
    fn separators_are_not_actionable() {
        let with_separator = snapshot(
            1,
            TrayMenuItem {
                children: vec![separator(1)],
                ..entry(0, None)
            },
        );
        assert!(
            !with_separator.find_item(1).unwrap().is_actionable(),
            "a separator carries no action"
        );
    }

    #[test]
    fn the_protocol_root_is_a_container_and_not_a_presentable_row() {
        let menu = snapshot(
            1,
            TrayMenuItem {
                children: vec![entry(1, Some("Only"))],
                ..entry(0, None)
            },
        );
        assert!(
            menu.presentable_items().iter().all(|item| item.id != 0),
            "menu id 0 is the root container, never a row"
        );
        assert_eq!(menu.item_count(), 1);
    }

    #[test]
    fn disabled_entries_are_not_actionable() {
        let mut disabled = entry(7, Some("Off"));
        disabled.enabled = false;
        let menu = snapshot(
            4,
            TrayMenuItem {
                children: vec![disabled],
                ..entry(0, None)
            },
        );
        let target = menu.find_item(7).unwrap();
        assert!(target.is_presentable());
        assert!(
            !target.is_actionable(),
            "a disabled entry must not be actionable"
        );
        assert!(menu.actionable_items().is_empty());
    }

    #[test]
    fn a_submenu_parent_is_not_itself_an_action() {
        let menu = snapshot(
            9,
            TrayMenuItem {
                children: vec![submenu(3, "Store", vec![entry(4, Some("Wishlists"))])],
                ..entry(0, None)
            },
        );
        let parent = menu.find_item(3).unwrap();
        assert!(!parent.is_actionable(), "a submenu is opened, not clicked");
        let leaf = menu.find_item(4).unwrap();
        assert!(leaf.is_actionable());
    }

    #[test]
    fn nested_entries_are_reachable_and_ordered() {
        let menu = snapshot(
            12,
            TrayMenuItem {
                children: vec![
                    entry(1, Some("Top")),
                    separator(2),
                    submenu(3, "Store", vec![entry(4, Some("Wishlists"))]),
                ],
                ..entry(0, None)
            },
        );
        assert_eq!(menu.item_count(), 4);
        assert_eq!(
            menu.find_item(4).unwrap().label.as_deref(),
            Some("Wishlists")
        );
        let labels: Vec<_> = menu
            .presentable_items()
            .iter()
            .filter_map(|item| item.label.clone())
            .collect();
        assert_eq!(
            labels,
            vec!["Top".to_owned(), "Wishlists".to_owned(), "Store".to_owned()],
            "layout order is preserved for presentation"
        );
    }

    #[test]
    fn a_binding_is_valid_only_against_its_own_revision() {
        let first = snapshot(
            3536,
            TrayMenuItem {
                children: vec![entry(1, Some("Turn Bluetooth Off"))],
                ..entry(0, None)
            },
        );
        let binding = first
            .bind(item_ref(), 1, TrayMenuEvent::Clicked)
            .expect("entry exists in this revision");

        assert_eq!(first.revalidate(&binding), Ok(()));

        let second = snapshot(
            3537,
            TrayMenuItem {
                children: vec![entry(1, Some("Turn Bluetooth On"))],
                ..entry(0, None)
            },
        );
        assert_eq!(
            second.revalidate(&binding),
            Err(StaleReason::MenuRevisionChanged),
            "a click rendered before LayoutUpdated must not be delivered afterwards"
        );
    }

    #[test]
    fn a_repurposed_entry_in_the_same_revision_is_refused() {
        let binding = TrayMenuEventBinding {
            item: item_ref(),
            menu_id: 1,
            revision: 5,
            event: TrayMenuEvent::Clicked,
            rendered_label: Some("Original".to_owned()),
        };

        let repointed = snapshot(
            5,
            TrayMenuItem {
                children: vec![entry(1, Some("Something Else"))],
                ..entry(0, None)
            },
        );
        assert_eq!(
            repointed.revalidate(&binding),
            Err(StaleReason::MenuItemAbsent),
            "an entry that changed meaning must not absorb the click"
        );
    }

    #[test]
    fn an_entry_missing_from_the_current_revision_is_refused() {
        let menu = snapshot(5, entry(0, None));
        let binding = TrayMenuEventBinding {
            item: item_ref(),
            menu_id: 42,
            revision: 5,
            event: TrayMenuEvent::Clicked,
            rendered_label: None,
        };
        assert_eq!(menu.revalidate(&binding), Err(StaleReason::MenuItemAbsent));
    }

    #[test]
    fn inline_icon_bytes_are_never_carried_into_presentation() {
        let small = TrayMenuIcon::inline_data(4096);
        assert_eq!(small, TrayMenuIcon::InlineData { byte_len: 4096 });
        match small {
            TrayMenuIcon::InlineData { byte_len } => {
                assert_eq!(byte_len, 4096, "only the measured size is projected")
            }
            other => panic!("unexpected {other:?}"),
        }

        assert_eq!(
            TrayMenuIcon::inline_data(MAX_INLINE_ICON_BYTES + 1),
            TrayMenuIcon::None,
            "an oversized inline payload is dropped rather than carried"
        );
    }

    #[test]
    fn enum_parsing_is_total_and_defaults_are_gnosix_side() {
        assert_eq!(
            TrayMenuStatus::parse("anything-else"),
            TrayMenuStatus::Normal
        );
        assert_eq!(
            TrayMenuItemType::parse("anything-else"),
            TrayMenuItemType::Standard
        );
        assert_eq!(
            TrayMenuToggleType::parse("anything-else"),
            TrayMenuToggleType::NotCheckable
        );
        assert_eq!(
            TrayMenuToggleState::parse("anything-else"),
            TrayMenuToggleState::Unchecked
        );
        assert_eq!(
            TrayMenuTextDirection::parse("anything-else"),
            TrayMenuTextDirection::LeftToRight
        );

        assert_eq!(TrayMenuStatus::parse("notice"), TrayMenuStatus::Notice);
        assert_eq!(
            TrayMenuItemType::parse("separator"),
            TrayMenuItemType::Separator
        );
        assert_eq!(
            TrayMenuToggleType::parse("radio"),
            TrayMenuToggleType::Radio
        );
        assert_eq!(
            TrayMenuToggleState::parse("inconsistent"),
            TrayMenuToggleState::Inconsistent
        );
        assert_eq!(
            TrayMenuTextDirection::parse("rtl"),
            TrayMenuTextDirection::RightToLeft
        );
    }

    #[test]
    fn only_clicked_is_exposed_as_a_menu_event() {
        assert_eq!(TrayMenuEvent::Clicked.protocol_id(), "clicked");
    }
}
