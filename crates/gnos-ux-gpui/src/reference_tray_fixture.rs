//! Deterministic component-lab Tray fixture for PARITY-01G.
//!
//! This constructs raw SNI snapshots and deliberately routes them through the
//! production presentation layer. It owns no provider/runtime authority.
use std::path::Path;

use crate::{
    product_tray::{
        TrayCategory, TrayIconFacts, TrayIconSlot, TrayItemId, TrayItemSnapshot, TrayPixmap,
        TraySnapshot, TrayStatus,
    },
    product_tray_presentation::present_tray,
    reference_bar::TrayVisual,
};

fn item(id: &str, status: TrayStatus, icons: TrayIconFacts) -> TrayItemSnapshot {
    TrayItemSnapshot {
        id: TrayItemId::new(id),
        service: format!("org.gnosix.fixture.{id}"),
        path: format!("/org/gnosix/fixture/{id}"),
        title: id.to_owned(),
        status,
        category: TrayCategory::ApplicationStatus,
        icons,
        tooltip: None,
        menu_path: None,
        item_is_menu: false,
        owner: ":fixture".into(),
    }
}

fn solid_pixmap(side: i32, argb: [u8; 4]) -> TrayPixmap {
    let mut bytes = Vec::with_capacity((side * side * 4) as usize);
    for _ in 0..side * side {
        bytes.extend_from_slice(&argb);
    }
    TrayPixmap {
        width: side,
        height: side,
        argb_bytes: bytes,
    }
}

/// Six items exercise the controlled 01G rendering branches in one Bar frame.
pub fn controlled_tray(root: &Path) -> Vec<TrayVisual> {
    let private_path = root.join("item-icons").to_string_lossy().into_owned();
    let named = item(
        "01-named",
        TrayStatus::Active,
        TrayIconFacts::named("fixture-normal"),
    );
    let attention = item(
        "02-attention",
        TrayStatus::NeedsAttention,
        TrayIconFacts::named("fixture-normal")
            .with_attention(TrayIconSlot::named("fixture-attention")),
    );
    let attention_fallback = item(
        "03-attention-pixmap",
        TrayStatus::NeedsAttention,
        TrayIconFacts {
            icon: TrayIconSlot {
                name: Some("fixture-normal".into()),
                pixmaps: vec![solid_pixmap(24, [0xff, 0xc0, 0x26, 0xd3])],
            },
            ..TrayIconFacts::default()
        },
    );
    let overlay = item(
        "04-overlay",
        TrayStatus::Active,
        TrayIconFacts::named("fixture-normal").with_overlay(TrayIconSlot::named("fixture-overlay")),
    );
    let private = item(
        "05-private",
        TrayStatus::Active,
        TrayIconFacts::named("fixture-private").with_theme_path(private_path),
    );
    let missing = item(
        "06-missing",
        TrayStatus::Active,
        TrayIconFacts::named("fixture-does-not-exist"),
    );

    let snapshot = TraySnapshot::from_items(vec![
        named,
        attention,
        attention_fallback,
        overlay,
        private,
        missing,
    ]);
    present_tray(&snapshot, &[])
        .expect("controlled tray provider is active")
        .into_iter()
        .map(TrayVisual::Native)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray_icon::{MissingReason, RealizedTrayIcon, realize_tray_icon};
    use gnosix_icon_theme::IconThemeSearch;
    fn fixture_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tray-native")
    }

    #[test]
    fn controlled_fixture_exercises_six_distinct_tray_slots() {
        let root = fixture_root();
        let visuals = controlled_tray(&root);
        assert_eq!(visuals.len(), 6);
        let search = IconThemeSearch {
            roots: vec![root.join("icons")],
            fallback_dirs: Vec::new(),
            theme: "GnosixTrayFixture".into(),
        };
        let realised = visuals
            .iter()
            .map(|visual| match visual {
                TrayVisual::Native(item) => realize_tray_icon(&item.source, &search, 24),
                TrayVisual::CapturedUnresolved { .. } => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert!(matches!(realised[0], RealizedTrayIcon::Image { .. }));
        assert!(matches!(realised[1], RealizedTrayIcon::Image { .. }));
        assert!(matches!(realised[2], RealizedTrayIcon::Image { .. }));
        assert!(matches!(
            realised[3],
            RealizedTrayIcon::Image {
                overlay: Some(_),
                ..
            }
        ));
        assert!(matches!(realised[4], RealizedTrayIcon::Image { .. }));
        assert!(matches!(
            realised[5],
            RealizedTrayIcon::Missing(MissingReason::NotFound(_))
        ));
    }
}

pub fn describe_realized(
    visuals: &[TrayVisual],
    realized: &[crate::product_tray_icon::RealizedTrayIcon],
) -> Vec<serde_json::Value> {
    use crate::product_tray_icon::{RealizedTrayIcon, TrayImage};
    visuals
        .iter()
        .zip(realized)
        .map(|(visual, icon)| {
            let id = match visual {
                TrayVisual::Native(item) => item.id.as_str(),
                TrayVisual::CapturedUnresolved { .. } => "captured-unresolved",
            };
            let layer = |image: &TrayImage| match image {
                TrayImage::File { path, format } => serde_json::json!({
                    "kind": "file",
                    "path": path,
                    "format": format!("{format:?}")
                }),
                TrayImage::Pixels(_) => serde_json::json!({"kind": "pixels"}),
            };
            let realization = match icon {
                RealizedTrayIcon::Image { base, overlay } => serde_json::json!({
                    "kind": "image",
                    "base": layer(base),
                    "overlay": overlay.as_ref().map(layer)
                }),
                RealizedTrayIcon::Missing(reason) => serde_json::json!({
                    "kind": "missing",
                    "reason": format!("{reason:?}")
                }),
            };
            serde_json::json!({
                "id": id,
                "realization": realization
            })
        })
        .collect()
}

#[cfg(test)]
mod svg_renderer_tests {
    use std::{fs, sync::Arc};

    #[test]
    fn fixture_svg_rasterises_to_its_native_colour_in_gpui() {
        let path = fixture_root().join("icons/GnosixTrayFixture/24/fixture-normal.svg");
        let bytes = fs::read(path).unwrap();
        let renderer = gpui::SvgRenderer::new(Arc::new(()));
        let image = renderer.render_single_frame(&bytes, 1.0).unwrap();
        let pixels = image.as_bytes(0).unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [0x35, 0x3b, 0xd8, 0xff]),
            "fixture red must survive GPUI SVG rasterisation"
        );
    }

    fn fixture_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tray-native")
    }
}
