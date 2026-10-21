//! ICONS-01: Tabler icons, stroked natively.
//!
//! An icon is its Tabler path data (`gnos-icons`) traced with GPUI's path
//! builder on a square of its size: outline icons stroked at Tabler's weight
//! (2 on the 24 grid), filled ones filled, in the caller's colour. No font
//! and no raster are involved, so an icon scales, takes any weight and can
//! be drawn on progressively (`IconSpec::drawn`), the base of its motion.
//!
//! Surfaces still name some icons by their former Material Symbols names
//! (they are also semantic keys in the Bar's model); `tabler_name` maps
//! those, and passes a Tabler name through.

use gnos_icons::{GRID, STROKE, Segment, Style};
use gpui::{Bounds, Div, ParentElement, PathBuilder, Pixels, Styled, canvas, div, point, px, rgba};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Former Material Symbols names still used as keys, and their Tabler icon.
const FROM_MATERIAL: &[(&str, &str)] = &[
    ("apps", "grid-dots"),
    ("archive", "archive"),
    ("bedtime", "moon"),
    ("build", "tool"),
    ("chat", "message"),
    ("code", "code"),
    ("construction", "tools"),
    ("content_paste", "clipboard"),
    ("edit_note", "notes"),
    ("files", "folder"),
    ("graphic_eq", "wave-sine"),
    ("grid_view", "layout-grid"),
    ("host", "server"),
    ("lock", "lock"),
    ("logout", "logout"),
    ("mic", "microphone"),
    ("monitor_heart", "heart-rate-monitor"),
    ("more_horiz", "dots"),
    ("music_note", "music"),
    ("music_video", "movie"),
    ("photo_library", "photo"),
    ("power_settings_new", "power"),
    ("print", "printer"),
    ("restart_alt", "refresh"),
    ("schedule", "clock"),
    ("security", "shield-lock"),
    ("settings", "settings"),
    ("sports_esports", "device-gamepad-2"),
    ("terminal", "terminal-2"),
    ("tune", "adjustments-horizontal"),
    ("tv", "device-tv"),
    ("video_settings", "video"),
    ("videocam", "video"),
    ("web", "world"),
    ("web_asset", "app-window"),
    ("widgets", "stack-2"),
];

/// The Tabler icon a key names: a former Material name mapped, or a Tabler
/// name as is.
pub fn tabler_name(key: &str) -> &str {
    FROM_MATERIAL
        .iter()
        .find(|(m, _)| *m == key)
        .map_or(key, |(_, t)| t)
}

/// Freedesktop icon names (as tray menus and applications give them) whose
/// Tabler icon has another name. Matched by prefix, after `-symbolic` is
/// dropped; a name Tabler also has passes through.
const FROM_FREEDESKTOP: &[(&str, &str)] = &[
    ("application-exit", "logout"),
    ("audio-headphones", "headphones"),
    ("audio-headset", "headphones"),
    ("audio-input-microphone", "microphone"),
    ("audio-volume-muted", "volume-off"),
    ("audio-", "volume"),
    ("bluetooth-active", "bluetooth-connected"),
    ("bluetooth-disabled", "bluetooth-off"),
    ("blueman", "bluetooth"),
    ("camera", "camera"),
    ("computer", "device-desktop"),
    ("configure", "settings"),
    ("dialog-information", "info-circle"),
    ("dialog-warning", "alert-triangle"),
    ("document-send", "send"),
    ("document-open", "folder"),
    ("edit-copy", "copy"),
    ("edit-delete", "trash"),
    ("edit-find", "search"),
    ("folder", "folder"),
    ("go-down", "arrow-down"),
    ("go-next", "arrow-right"),
    ("go-previous", "arrow-left"),
    ("go-up", "arrow-up"),
    ("help", "help"),
    ("input-keyboard", "keyboard"),
    ("list-add", "plus"),
    ("list-remove", "minus"),
    ("mail", "mail"),
    ("media-playback-pause", "player-pause"),
    ("media-playback-start", "player-play"),
    ("media-skip-backward", "player-skip-back"),
    ("media-skip-forward", "player-skip-forward"),
    ("network-wired", "network"),
    ("network-wireless", "wifi"),
    ("phone", "device-mobile"),
    ("preferences", "settings"),
    ("process-stop", "x"),
    ("system-lock-screen", "lock"),
    ("system-shutdown", "power"),
    ("user", "user"),
    ("video-display", "device-desktop"),
    ("view-refresh", "refresh"),
    ("window-close", "x"),
];

/// The Tabler icon for a freedesktop icon name, if there is one.
pub fn from_freedesktop(name: &str) -> Option<String> {
    let name = name.trim_end_matches("-symbolic");
    if gnos_icons::icon(Style::Outline, name).is_some() {
        return Some(name.to_owned());
    }
    FROM_FREEDESKTOP
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map(|(_, tabler)| (*tabler).to_owned())
}

/// How to draw one icon.
#[derive(Clone, Debug, PartialEq)]
pub struct IconSpec {
    pub name: String,
    pub style: Style,
    pub size: f32,
    pub color: u32,
    /// Stroke weight against Tabler's (1 = 2 units on the 24 grid).
    pub weight: f32,
    /// How much of the stroke is drawn, 0..=1 (1: all of it).
    pub drawn: f32,
}

impl IconSpec {
    pub fn new(name: impl Into<String>, size: f32, color: u32) -> Self {
        Self {
            name: name.into(),
            style: Style::Outline,
            size,
            color,
            weight: stroke_weight(),
            drawn: DRAWN.with(Cell::get),
        }
    }
}

thread_local! {
    /// How much of their stroke the icons built now draw (ICONS-01 motion).
    static DRAWN: Cell<f32> = const { Cell::new(1.0) };
}

/// Build with icons drawn on to `drawn` (0..=1): a surface coming out of the
/// Bar draws its icons as it opens.
pub fn drawing<R>(drawn: f32, build: impl FnOnce() -> R) -> R {
    let before = DRAWN.with(|d| d.replace(drawn.clamp(0.0, 1.0)));
    let built = build();
    DRAWN.with(|d| d.set(before));
    built
}

/// How much a surface's icons are drawn at `openness`: they start once the
/// surface is a third out and are whole when it is, unless the operator
/// turned the motion off (Settings › Advanced).
pub fn draw_on(openness: f32) -> f32 {
    #[cfg(feature = "component-lab")]
    if crate::surface_prefs::advanced("icon_draw_on") == 0 {
        return 1.0;
    }
    ((openness - 0.35) / 0.65).clamp(0.0, 1.0)
}

/// SET-03: the operator's stroke weight.
fn stroke_weight() -> f32 {
    #[cfg(feature = "component-lab")]
    return crate::surface_prefs::advanced("icon_weight_pct") as f32 / 100.0;
    #[cfg(not(feature = "component-lab"))]
    1.0
}

type Parsed = Rc<(Vec<Segment>, f32)>;

thread_local! {
    /// Parsed icons, by style and name (each parsed once per thread).
    static PARSED: RefCell<HashMap<(Style, String), Option<Parsed>>> = RefCell::new(HashMap::new());
}

fn parsed(style: Style, name: &str) -> Option<Parsed> {
    PARSED.with(|cache| {
        cache
            .borrow_mut()
            .entry((style, name.to_owned()))
            .or_insert_with(|| {
                let icon = gnos_icons::icon(style, tabler_name(name))?;
                let segments = icon.segments();
                let length = gnos_icons::length(&segments);
                Some(Rc::new((segments, length)))
            })
            .clone()
    })
}

/// Whether Tabler has an icon for this key.
pub fn exists(style: Style, name: &str) -> bool {
    parsed(style, name).is_some()
}

/// An icon's path with the grid's top-left at `origin` and its side `size`
/// pixels: stroked for an outline (dashed while it draws on), filled
/// otherwise. `None` when Tabler lacks it or nothing is drawn yet.
pub fn path(spec: &IconSpec, origin: (f32, f32)) -> Option<gpui::Path<Pixels>> {
    let icon = parsed(spec.style, &spec.name)?;
    let (segments, length) = (&icon.0, icon.1);
    let k = spec.size / GRID;
    let at = |p: gnos_icons::Point| point(px(origin.0 + p.x * k), px(origin.1 + p.y * k));
    let drawn = spec.drawn.clamp(0.0, 1.0);
    if drawn <= 0.0 {
        return None;
    }
    let mut builder = match spec.style {
        Style::Outline => {
            let builder = PathBuilder::stroke(px(STROKE * k * spec.weight.max(0.1)));
            if drawn < 1.0 {
                let on = length * k * drawn;
                builder.dash_array(&[px(on), px(length * k + 1.0)])
            } else {
                builder
            }
        }
        Style::Filled => PathBuilder::fill(),
    };
    for s in segments {
        match *s {
            Segment::MoveTo(p) => builder.move_to(at(p)),
            Segment::LineTo(p) => builder.line_to(at(p)),
            Segment::Quad { ctrl, to } => builder.curve_to(at(to), at(ctrl)),
            Segment::Cubic { ctrl1, ctrl2, to } => {
                builder.cubic_bezier_to(at(to), at(ctrl1), at(ctrl2))
            }
            Segment::Arc {
                radii,
                rotation,
                large_arc,
                sweep,
                to,
            } => builder.arc_to(
                point(px(radii.x * k), px(radii.y * k)),
                px(rotation),
                large_arc,
                sweep,
                at(to),
            ),
            Segment::Close => builder.close(),
        }
    }
    builder.build().ok()
}

/// The paths of an outline icon at Tabler's weight, for painters that fill
/// them in their own colour (the Bar's status glyphs and role icons).
pub fn paths(name: &str, origin: (f32, f32), size: f32) -> Vec<gpui::Path<Pixels>> {
    paths_drawn(name, origin, size, 1.0)
}

/// `paths`, drawn on to `drawn` (a painter that runs at paint time reads the
/// factor when its element is built: `current_drawn`).
pub fn paths_drawn(
    name: &str,
    origin: (f32, f32),
    size: f32,
    drawn: f32,
) -> Vec<gpui::Path<Pixels>> {
    let spec = IconSpec {
        drawn,
        ..IconSpec::new(name, size, 0)
    };
    path(&spec, origin).into_iter().collect()
}

/// The draw-on factor of icons built now (see `drawing`).
pub fn current_drawn() -> f32 {
    DRAWN.with(Cell::get)
}

/// Paint an icon into `bounds` (a square).
pub fn paint(window: &mut gpui::Window, bounds: Bounds<Pixels>, spec: &IconSpec) {
    let size = f32::from(bounds.size.width.min(bounds.size.height));
    let spec = IconSpec {
        size,
        ..spec.clone()
    };
    let origin = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    if let Some(path) = path(&spec, origin) {
        window.paint_path(path, rgba(spec.color));
    }
}

/// An icon as an element: a square of its size. A key Tabler lacks shows a
/// neutral mark instead of nothing.
pub fn icon_element(spec: IconSpec) -> Div {
    let size = spec.size;
    let square = div()
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center();
    if !exists(spec.style, &spec.name) {
        return square.child(
            div()
                .size(px(size * 0.36))
                .rounded(px(size * 0.18))
                .bg(rgba(spec.color)),
        );
    }
    square.child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| paint(window, bounds, &spec),
        )
        .w(px(size))
        .h(px(size)),
    )
}

/// The common case: an outline icon at Tabler's weight.
pub fn icon(name: impl Into<String>, size: f32, color: u32) -> Div {
    icon_element(IconSpec::new(name, size, color))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freedesktop_names_find_their_tabler_icon() {
        for (_, tabler) in FROM_FREEDESKTOP {
            assert!(exists(Style::Outline, tabler), "{tabler}");
        }
        assert_eq!(
            from_freedesktop("bluetooth-disabled-symbolic").as_deref(),
            Some("bluetooth-off")
        );
        assert_eq!(
            from_freedesktop("network-wireless-signal-good").as_deref(),
            Some("wifi")
        );
        assert_eq!(
            from_freedesktop("preferences-system").as_deref(),
            Some("settings")
        );
        assert_eq!(
            from_freedesktop("bluetooth").as_deref(),
            Some("bluetooth"),
            "Tabler's own"
        );
        assert_eq!(from_freedesktop("org.example.unknown-thing"), None);
    }

    #[test]
    fn icons_built_while_drawing_carry_it_and_it_is_restored() {
        let spec = drawing(0.4, || IconSpec::new("bell", 18.0, 0));
        assert!((spec.drawn - 0.4).abs() < 1e-6);
        assert_eq!(IconSpec::new("bell", 18.0, 0).drawn, 1.0, "restored");
        assert_eq!(draw_on(0.2), 0.0);
        assert!((draw_on(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_former_material_key_has_a_tabler_icon() {
        for (material, tabler) in FROM_MATERIAL {
            assert!(
                gnos_icons::icon(Style::Outline, tabler).is_some(),
                "{material} -> {tabler}"
            );
        }
        assert_eq!(tabler_name("power_settings_new"), "power");
        assert_eq!(tabler_name("bell"), "bell", "a Tabler name passes through");
        assert!(exists(Style::Outline, "terminal"));
    }
}
