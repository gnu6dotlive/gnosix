//! Wire form for routing a projected tray menu across processes.
//!
//! The product menu model is a recursive tree, and a D-Bus type signature
//! cannot express a recursive struct. The wire form is therefore a flat,
//! pre-order row list; each row names its parent by id, and the receiver
//! rebuilds the exact tree. Only typed, bounded fields cross the bus — no raw
//! D-Bus values, no inline image bytes (size only, as in the product model).

use crate::product_tray_menu::{
    TrayMenuChildrenDisplay, TrayMenuIcon, TrayMenuItem, TrayMenuItemType, TrayMenuSnapshot,
    TrayMenuStatus, TrayMenuTextDirection, TrayMenuToggleState, TrayMenuToggleType,
};

/// Parent id used by the root row, which has no parent.
const NO_PARENT: i32 = -1;

/// One flattened menu entry.
#[derive(
    Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, zbus::zvariant::Type,
)]
pub struct RelayMenuRow {
    pub id: i32,
    pub parent_id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub icon_named: String,
    pub icon_inline_len: u32,
    pub item_type: String,
    pub toggle_type: String,
    pub toggle_state: String,
    pub children_display: String,
}

/// A whole projected menu, flattened.
#[derive(
    Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, zbus::zvariant::Type,
)]
pub struct RelayMenu {
    pub revision: u32,
    pub status: String,
    pub version: u32,
    pub text_direction: String,
    pub icon_theme_path: String,
    pub rows: Vec<RelayMenuRow>,
}

impl RelayMenu {
    /// An empty menu marker: nothing to render. Used when the executor refused
    /// or projected no menu.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn from_snapshot(menu: &TrayMenuSnapshot) -> Self {
        let mut rows = Vec::new();
        flatten(&menu.root, NO_PARENT, &mut rows);
        Self {
            revision: menu.revision,
            status: status_token(menu.status).to_owned(),
            version: menu.version,
            text_direction: direction_token(menu.text_direction).to_owned(),
            icon_theme_path: menu.icon_theme_path.clone().unwrap_or_default(),
            rows,
        }
    }

    pub fn to_snapshot(&self) -> TrayMenuSnapshot {
        use std::collections::HashMap;
        let mut children: HashMap<i32, Vec<&RelayMenuRow>> = HashMap::new();
        let mut root = None;
        for row in &self.rows {
            if row.parent_id == NO_PARENT {
                root = Some(row);
            } else {
                children.entry(row.parent_id).or_default().push(row);
            }
        }
        let root = root.map(|row| rebuild(row, &children)).unwrap_or_default();
        TrayMenuSnapshot {
            revision: self.revision,
            status: TrayMenuStatus::parse(&self.status),
            version: self.version,
            text_direction: TrayMenuTextDirection::parse(&self.text_direction),
            icon_theme_path: (!self.icon_theme_path.is_empty())
                .then(|| self.icon_theme_path.clone()),
            root,
        }
    }
}

fn flatten(item: &TrayMenuItem, parent_id: i32, rows: &mut Vec<RelayMenuRow>) {
    let (icon_named, icon_inline_len) = match &item.icon {
        TrayMenuIcon::Named(name) => (name.clone(), 0),
        TrayMenuIcon::InlineData { byte_len } => {
            (String::new(), u32::try_from(*byte_len).unwrap_or(u32::MAX))
        }
        TrayMenuIcon::None => (String::new(), 0),
    };
    rows.push(RelayMenuRow {
        id: item.id,
        parent_id,
        label: item.label.clone().unwrap_or_default(),
        enabled: item.enabled,
        visible: item.visible,
        icon_named,
        icon_inline_len,
        item_type: item_type_token(item.item_type).to_owned(),
        toggle_type: toggle_type_token(item.toggle_type).to_owned(),
        toggle_state: toggle_state_token(item.toggle_state).to_owned(),
        children_display: children_display_token(item.children_display).to_owned(),
    });
    for child in &item.children {
        flatten(child, item.id, rows);
    }
}

fn rebuild(
    row: &RelayMenuRow,
    children: &std::collections::HashMap<i32, Vec<&RelayMenuRow>>,
) -> TrayMenuItem {
    let icon = if !row.icon_named.is_empty() {
        TrayMenuIcon::Named(row.icon_named.clone())
    } else if row.icon_inline_len > 0 {
        TrayMenuIcon::InlineData {
            byte_len: row.icon_inline_len as usize,
        }
    } else {
        TrayMenuIcon::None
    };
    TrayMenuItem {
        id: row.id,
        label: (!row.label.is_empty()).then(|| row.label.clone()),
        enabled: row.enabled,
        visible: row.visible,
        icon,
        item_type: TrayMenuItemType::parse(&row.item_type),
        toggle_type: TrayMenuToggleType::parse(&row.toggle_type),
        toggle_state: TrayMenuToggleState::parse(&row.toggle_state),
        children_display: TrayMenuChildrenDisplay::parse(&row.children_display),
        children: children
            .get(&row.id)
            .map(|rows| rows.iter().map(|row| rebuild(row, children)).collect())
            .unwrap_or_default(),
    }
}

fn status_token(status: TrayMenuStatus) -> &'static str {
    match status {
        TrayMenuStatus::Normal => "normal",
        TrayMenuStatus::Notice => "notice",
    }
}

fn direction_token(direction: TrayMenuTextDirection) -> &'static str {
    match direction {
        TrayMenuTextDirection::LeftToRight => "ltr",
        TrayMenuTextDirection::RightToLeft => "rtl",
    }
}

fn item_type_token(item_type: TrayMenuItemType) -> &'static str {
    match item_type {
        TrayMenuItemType::Standard => "standard",
        TrayMenuItemType::Separator => "separator",
    }
}

fn toggle_type_token(toggle_type: TrayMenuToggleType) -> &'static str {
    match toggle_type {
        TrayMenuToggleType::NotCheckable => "not_checkable",
        TrayMenuToggleType::Checkable => "checkmark",
        TrayMenuToggleType::Radio => "radio",
    }
}

fn toggle_state_token(state: TrayMenuToggleState) -> &'static str {
    match state {
        TrayMenuToggleState::Unchecked => "unchecked",
        TrayMenuToggleState::Checked => "checked",
        TrayMenuToggleState::Inconsistent => "inconsistent",
    }
}

fn children_display_token(display: TrayMenuChildrenDisplay) -> &'static str {
    match display {
        TrayMenuChildrenDisplay::Submenu => "submenu",
        TrayMenuChildrenDisplay::Inline => "inline",
        TrayMenuChildrenDisplay::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray_menu::{TrayMenuItem, TrayMenuSnapshot};

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

    #[test]
    fn a_nested_menu_round_trips_exactly() {
        let mut disabled = entry(5, Some("Off"));
        disabled.enabled = false;
        let mut submenu = entry(3, Some("Store"));
        submenu.children = vec![entry(4, Some("Wishlists"))];
        let menu = TrayMenuSnapshot {
            revision: 42,
            status: TrayMenuStatus::Notice,
            version: 3,
            text_direction: TrayMenuTextDirection::LeftToRight,
            icon_theme_path: Some("/icons".to_owned()),
            root: TrayMenuItem {
                children: vec![entry(1, Some("Top")), submenu, disabled],
                ..entry(0, None)
            },
        };
        let round_tripped = RelayMenu::from_snapshot(&menu).to_snapshot();
        assert_eq!(round_tripped, menu);
        assert_eq!(
            round_tripped
                .actionable_items()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![1, 4],
            "actionability and order survive the wire form"
        );
    }

    #[test]
    fn inline_icon_bytes_cross_as_a_size_only() {
        let mut item = entry(1, Some("Icon"));
        item.icon = TrayMenuIcon::InlineData { byte_len: 4096 };
        let menu = TrayMenuSnapshot {
            revision: 1,
            root: TrayMenuItem {
                children: vec![item],
                ..entry(0, None)
            },
            ..Default::default()
        };
        let round_tripped = RelayMenu::from_snapshot(&menu).to_snapshot();
        assert_eq!(
            round_tripped.find_item(1).unwrap().icon,
            TrayMenuIcon::InlineData { byte_len: 4096 }
        );
    }

    #[test]
    fn an_empty_menu_is_marked_empty() {
        assert!(RelayMenu::default().is_empty());
    }
}
