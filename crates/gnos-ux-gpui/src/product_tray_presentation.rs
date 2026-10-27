//! Typed Tray presentation: what a Tray surface shows for each item.
//!
//! Provider facts (`TraySnapshot`) are filtered, ordered and resolved to one
//! icon *source* per item here, so a renderer never interprets raw SNI. The icon
//! source is application-owned imagery (a themed name, pixmaps, or a base with an
//! overlay); it is not a Gnosix `IconRole` and is never recoloured.
//!
//! Observed behaviour reproduced (reference shell, Quickshell 0.3.1
//! `status_notifier/item.cpp`, `Tray.qml`): passive and hidden items are omitted;
//! `NeedsAttention` with an attention icon shows it, and without one falls to
//! the *pixmaps* of the base icon; otherwise a named icon without an overlay is
//! resolved by name and anything else is composed from the base and its overlay.
//! Two donor quirks are deliberately not reproduced and are named in the tests.

use crate::product_tray::{
    TrayIconFacts, TrayIconSlot, TrayItemId, TrayItemSnapshot, TrayPixmap, TraySnapshot, TrayStatus,
};
use crate::product_tray_action::TrayOperation;

/// Where an item's imagery comes from. Resolution to pixels is a later stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayIconSource {
    /// A themed icon name plus the item's SNI IconThemePath fallback.
    Named {
        name: String,
        theme_path: Option<String>,
    },
    /// Provider pixmaps; the closest size is chosen when the size is known.
    Pixmaps(Vec<TrayPixmap>),
    /// The overlay is painted over the base, scaled to the base.
    Composite {
        base: Box<TrayIconSource>,
        overlay: Box<TrayIconSource>,
    },
    /// Nothing to draw. The item still occupies its slot.
    None,
}

fn slot_source(slot: &TrayIconSlot, theme_path: &Option<String>) -> TrayIconSource {
    match (&slot.name, slot.pixmaps.is_empty()) {
        (Some(name), _) => TrayIconSource::Named {
            name: name.clone(),
            theme_path: theme_path.clone(),
        },
        (None, false) => TrayIconSource::Pixmaps(slot.pixmaps.clone()),
        (None, true) => TrayIconSource::None,
    }
}

/// Select the icon source for one item from its facts and status.
pub fn select_icon_source(facts: &TrayIconFacts, status: TrayStatus) -> TrayIconSource {
    let theme = &facts.theme_path;
    if status == TrayStatus::NeedsAttention {
        // Observed: the attention icon by name, else its pixmaps, else the base
        // icon's *pixmaps* (never its name). The overlay is not applied.
        return match &facts.attention.name {
            Some(name) => TrayIconSource::Named {
                name: name.clone(),
                theme_path: theme.clone(),
            },
            None if !facts.attention.pixmaps.is_empty() => {
                TrayIconSource::Pixmaps(facts.attention.pixmaps.clone())
            }
            None if !facts.icon.pixmaps.is_empty() => {
                TrayIconSource::Pixmaps(facts.icon.pixmaps.clone())
            }
            None => TrayIconSource::None,
        };
    }
    let base = slot_source(&facts.icon, theme);
    if facts.overlay.is_empty() {
        return base;
    }
    // An overlay is painted over the base image; with no base there is nothing
    // to paint on.
    match base {
        TrayIconSource::None => TrayIconSource::None,
        base => TrayIconSource::Composite {
            base: Box::new(base),
            overlay: Box::new(slot_source(&facts.overlay, theme)),
        },
    }
}

/// The provider pixmap closest to a requested size: the smallest one that is at
/// least as large in both dimensions, else the largest.
pub fn closest_pixmap(size: (i32, i32), pixmaps: &[TrayPixmap]) -> Option<&TrayPixmap> {
    let adequate = |p: &TrayPixmap| p.width >= size.0 && p.height >= size.1;
    let mut chosen: Option<&TrayPixmap> = None;
    for candidate in pixmaps {
        let Some(current) = chosen else {
            chosen = Some(candidate);
            continue;
        };
        let smaller = candidate.width < current.width || candidate.height < current.height;
        // Keep a smaller adequate pixmap over a larger adequate one; when the
        // current choice is too small, take any pixmap that is not smaller.
        if (adequate(current) && adequate(candidate) && smaller) || (!adequate(current) && !smaller)
        {
            chosen = Some(candidate);
        }
    }
    chosen
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayItemPresentation {
    pub id: TrayItemId,
    pub title: String,
    pub status: TrayStatus,
    pub source: TrayIconSource,
    /// Reference-shell primary click: StatusNotifierItem Activate.
    pub primary: TrayOperation,
    /// Reference-shell secondary/right click: SecondaryActivate.
    pub secondary: TrayOperation,
}

/// Items to show, in the snapshot's deterministic order. `None` means the
/// provider is unavailable: absence stays absence, it is not an empty tray.
pub fn present_tray(
    snapshot: &TraySnapshot,
    hidden_ids: &[&str],
) -> Option<Vec<TrayItemPresentation>> {
    let TraySnapshot::Active { items } = snapshot else {
        return None;
    };
    Some(
        items
            .iter()
            .filter(|item| item.status != TrayStatus::Passive)
            .filter(|item| !hidden_ids.contains(&item.id.as_str()))
            .map(present_item)
            .collect(),
    )
}

fn present_item(item: &TrayItemSnapshot) -> TrayItemPresentation {
    TrayItemPresentation {
        id: item.id.clone(),
        title: item.title.clone(),
        status: item.status,
        source: select_icon_source(&item.icons, item.status),
        // TrayItem.qml accepts left/right only and dispatches these operations
        // unconditionally; ItemIsMenu does not rewrite the left click.
        primary: TrayOperation::Activate,
        secondary: TrayOperation::SecondaryActivate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray::TrayCategory;

    fn pixmap(side: i32) -> TrayPixmap {
        TrayPixmap {
            width: side,
            height: side,
            argb_bytes: vec![0xff; (side * side * 4) as usize],
        }
    }

    fn named(name: &str, theme: Option<&str>) -> TrayIconSource {
        TrayIconSource::Named {
            name: name.into(),
            theme_path: theme.map(str::to_owned),
        }
    }

    fn item(id: &str, status: TrayStatus, icons: TrayIconFacts) -> TrayItemSnapshot {
        TrayItemSnapshot {
            id: TrayItemId::new(id),
            service: format!("org.example.{id}"),
            path: format!("/org/example/{id}"),
            title: id.to_owned(),
            status,
            category: TrayCategory::ApplicationStatus,
            icons,
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.1".into(),
        }
    }

    #[test]
    fn a_named_icon_without_overlay_resolves_by_name_with_its_theme_path() {
        let facts = TrayIconFacts::named("steam").with_theme_path("/opt/steam/icons");
        assert_eq!(
            select_icon_source(&facts, TrayStatus::Active),
            named("steam", Some("/opt/steam/icons"))
        );
    }

    #[test]
    fn pixmaps_are_kept_whole_when_there_is_no_name() {
        let facts = TrayIconFacts {
            icon: TrayIconSlot::from_pixmaps(vec![pixmap(16), pixmap(48)]),
            ..TrayIconFacts::default()
        };
        assert_eq!(
            select_icon_source(&facts, TrayStatus::Active),
            TrayIconSource::Pixmaps(vec![pixmap(16), pixmap(48)])
        );
    }

    #[test]
    fn no_icon_facts_present_nothing_but_keep_the_item() {
        assert_eq!(
            select_icon_source(&TrayIconFacts::none(), TrayStatus::Active),
            TrayIconSource::None
        );
    }

    #[test]
    fn needs_attention_prefers_the_attention_icon() {
        let facts = TrayIconFacts::named("idle")
            .with_attention(TrayIconSlot::named("urgent"))
            .with_theme_path("/t");
        assert_eq!(
            select_icon_source(&facts, TrayStatus::NeedsAttention),
            named("urgent", Some("/t"))
        );
        // The same facts while active show the normal icon.
        assert_eq!(
            select_icon_source(&facts, TrayStatus::Active),
            named("idle", Some("/t"))
        );
    }

    #[test]
    fn attention_without_an_attention_icon_falls_to_base_pixmaps_never_the_base_name() {
        let with_pixmaps = TrayIconFacts {
            icon: TrayIconSlot {
                name: Some("idle".into()),
                pixmaps: vec![pixmap(24)],
            },
            ..TrayIconFacts::default()
        };
        assert_eq!(
            select_icon_source(&with_pixmaps, TrayStatus::NeedsAttention),
            TrayIconSource::Pixmaps(vec![pixmap(24)])
        );
        // A name-only base has no pixmap to fall back to: the observed result is an
        // empty image, not the named icon.
        assert_eq!(
            select_icon_source(&TrayIconFacts::named("idle"), TrayStatus::NeedsAttention),
            TrayIconSource::None
        );
    }

    #[test]
    fn attention_pixmaps_win_over_base_pixmaps() {
        let facts = TrayIconFacts {
            icon: TrayIconSlot::from_pixmaps(vec![pixmap(24)]),
            attention: TrayIconSlot::from_pixmaps(vec![pixmap(32)]),
            ..TrayIconFacts::default()
        };
        assert_eq!(
            select_icon_source(&facts, TrayStatus::NeedsAttention),
            TrayIconSource::Pixmaps(vec![pixmap(32)])
        );
    }

    #[test]
    fn an_overlay_forces_the_composite_path_over_the_base() {
        let facts = TrayIconFacts::named("mail")
            .with_overlay(TrayIconSlot::named("badge"))
            .with_theme_path("/t");
        assert_eq!(
            select_icon_source(&facts, TrayStatus::Active),
            TrayIconSource::Composite {
                base: Box::new(named("mail", Some("/t"))),
                overlay: Box::new(named("badge", Some("/t"))),
            }
        );
    }

    #[test]
    fn a_pixmap_only_overlay_is_composed_not_dropped() {
        // Donor quirk NOT reproduced: the reference shell ignores an overlay that
        // has pixmaps but no name when the base icon is named. Gnosix composes it.
        let facts =
            TrayIconFacts::named("mail").with_overlay(TrayIconSlot::from_pixmaps(vec![pixmap(8)]));
        assert_eq!(
            select_icon_source(&facts, TrayStatus::Active),
            TrayIconSource::Composite {
                base: Box::new(named("mail", None)),
                overlay: Box::new(TrayIconSource::Pixmaps(vec![pixmap(8)])),
            }
        );
    }

    #[test]
    fn the_overlay_is_not_applied_to_the_attention_icon() {
        let facts = TrayIconFacts::named("idle")
            .with_attention(TrayIconSlot::named("urgent"))
            .with_overlay(TrayIconSlot::named("badge"));
        assert_eq!(
            select_icon_source(&facts, TrayStatus::NeedsAttention),
            named("urgent", None)
        );
    }

    #[test]
    fn an_overlay_without_a_base_image_draws_nothing() {
        let facts = TrayIconFacts::none().with_overlay(TrayIconSlot::named("badge"));
        assert_eq!(
            select_icon_source(&facts, TrayStatus::Active),
            TrayIconSource::None
        );
    }

    #[test]
    fn the_item_theme_path_is_retained_for_every_named_source() {
        let facts = TrayIconFacts::named("a")
            .with_overlay(TrayIconSlot::named("b"))
            .with_theme_path("/per/item");
        match select_icon_source(&facts, TrayStatus::Active) {
            TrayIconSource::Composite { base, overlay } => {
                assert_eq!(*base, named("a", Some("/per/item")));
                assert_eq!(*overlay, named("b", Some("/per/item")));
            }
            other => panic!("expected a composite, got {other:?}"),
        }
    }

    #[test]
    fn closest_pixmap_prefers_the_smallest_adequate_size_else_the_largest() {
        let sizes = [pixmap(16), pixmap(48), pixmap(32), pixmap(64)];
        let pick = |wanted: i32| closest_pixmap((wanted, wanted), &sizes).map(|p| p.width);
        assert_eq!(pick(24), Some(32));
        assert_eq!(pick(32), Some(32));
        assert_eq!(pick(33), Some(48));
        assert_eq!(pick(8), Some(16));
        // Nothing is large enough: the largest is the closest.
        assert_eq!(pick(100), Some(64));
        assert_eq!(closest_pixmap((24, 24), &[]), None);
    }

    #[test]
    fn closest_pixmap_follows_the_observed_comparison_for_non_square_pixmaps() {
        // Observed: "smaller" means narrower OR shorter, so an 8x64 pixmap that is
        // not wide enough is kept over a later, adequate 24x24 one. Square
        // pixmaps are the normal SNI case; this edge is reproduced, not tuned.
        let tall = TrayPixmap {
            width: 8,
            height: 64,
            argb_bytes: vec![0; 8 * 64 * 4],
        };
        let pixmaps = [tall, pixmap(24)];
        let picked = closest_pixmap((24, 24), &pixmaps).unwrap();
        assert_eq!((picked.width, picked.height), (8, 64));
        // Reversed order: the adequate square is first and nothing displaces it,
        // because the tall one is also adequate in height but narrower.
        let pixmaps = [pixmap(24), pixmaps[0].clone()];
        let picked = closest_pixmap((24, 24), &pixmaps).unwrap();
        assert_eq!((picked.width, picked.height), (24, 24));
    }

    #[test]
    fn passive_and_hidden_items_are_omitted_and_order_is_the_snapshots() {
        let snapshot = TraySnapshot::from_items(vec![
            item("zeta", TrayStatus::Active, TrayIconFacts::named("z")),
            item("alpha", TrayStatus::Active, TrayIconFacts::named("a")),
            item("quiet", TrayStatus::Passive, TrayIconFacts::named("q")),
            item("secret", TrayStatus::Active, TrayIconFacts::named("s")),
        ]);
        let ids = |hidden: &[&str]| {
            let presented = present_tray(&snapshot, hidden).unwrap();
            presented
                .iter()
                .map(|i| i.id.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        // Deterministic identity order from the snapshot; passive never shown.
        assert_eq!(ids(&[]), ["alpha", "secret", "zeta"]);
        assert_eq!(ids(&["secret"]), ["alpha", "zeta"]);
    }

    #[test]
    fn an_empty_tray_is_present_and_an_unavailable_provider_is_absent() {
        assert_eq!(
            present_tray(&TraySnapshot::from_items(Vec::new()), &[]),
            Some(Vec::new())
        );
        assert_eq!(present_tray(&TraySnapshot::Unavailable, &[]), None);
    }

    #[test]
    fn interaction_intent_is_typed_provider_authority() {
        let mut menu = item("menu", TrayStatus::Active, TrayIconFacts::named("m"));
        menu.item_is_menu = true;
        menu.menu_path = Some("/MenuBar".into());
        let plain = item("plain", TrayStatus::Active, TrayIconFacts::named("p"));
        let presented = present_tray(&TraySnapshot::from_items(vec![menu, plain]), &[]).unwrap();
        let by_id = |id: &str| presented.iter().find(|i| i.id.as_str() == id).unwrap();
        assert_eq!(by_id("plain").primary, TrayOperation::Activate);
        assert_eq!(by_id("menu").primary, TrayOperation::Activate);
        // The reference shell maps the accepted right button to secondaryActivate().
        assert_eq!(by_id("plain").secondary, TrayOperation::SecondaryActivate);
        assert_eq!(by_id("menu").secondary, TrayOperation::SecondaryActivate);
    }

    #[test]
    fn presentation_carries_the_selected_source_per_item() {
        let snapshot = TraySnapshot::from_items(vec![item(
            "mail",
            TrayStatus::NeedsAttention,
            TrayIconFacts::named("idle").with_attention(TrayIconSlot::named("urgent")),
        )]);
        let presented = present_tray(&snapshot, &[]).unwrap();
        assert_eq!(presented[0].source, named("urgent", None));
    }
}
