//! UXE-01B: GPUI view of the Bar-attached popout (audio; UXE-04 status rows).
//!
//! Paints exactly what `BarPopoutHost` resolved: the connective hull (masked
//! to the right of the rail, so the translucent rail material is never painted
//! twice) and the content rows at the rectangles the router hit-tests. Motion
//! is geometric only: the content slides with the revealed edge, no fade.

use crate::bar_audio_popout::{AudioRow, AudioRowKind};
use crate::bar_popout_host::PopoutFrame;
use crate::product_status_glyph::{StatusGlyph, VolumeGlyph, status_glyph_paths_drawn};
use crate::reference_bar::{
    COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT, COLOR_PRIMARY,
    COLOR_SURFACE_CONTAINER_HIGHEST,
};
use gpui::{
    Bounds, ContentMask, Div, IntoElement, ParentElement, Pixels, Rgba, Styled, Window, canvas,
    div, point, px, rgba, size,
};

const TEXT_SMALL: f32 = 12.0;
const TEXT_BODY: f32 = 13.0;
const RADIO: f32 = 14.0;
const TRACK: f32 = 6.0;
const GLYPH: f32 = 18.0;

/// Hull for the revealed popout, clipped to the scene area beside the rail
/// (the rail itself is excluded, whichever side it is on).
pub fn paint_popout_hull(
    scene: Bounds<Pixels>,
    frame: &PopoutFrame,
    colour: Rgba,
    window: &mut Window,
) {
    // SHELL-FIELD-01B: the shell's field paints this hull with the frame.
    #[cfg(feature = "component-lab")]
    if crate::shell_field_paint::paints(colour) {
        return;
    }
    paint_own_hull(scene, frame, colour, window);
}

/// The surface's own hull, whether or not the shell's field paints its
/// colour (FOCUS-01D: a surface layered above another stands out of the
/// field, with its own outline).
pub fn paint_own_hull(
    scene: Bounds<Pixels>,
    frame: &PopoutFrame,
    colour: Rgba,
    window: &mut Window,
) {
    use crate::rail_topology::RailSide;
    let (w, h) = (scene.size.width, scene.size.height);
    let bar = px(crate::rail_topology::rail_width());
    let (x, y, mw, mh) = match frame.side {
        RailSide::Left => (bar, px(0.0), w - bar, h),
        RailSide::Right => (px(0.0), px(0.0), w - bar, h),
        RailSide::Top => (px(0.0), bar, w, h - bar),
        RailSide::Bottom => (px(0.0), px(0.0), w, h - bar),
    };
    let mask = ContentMask {
        bounds: Bounds::new(point(scene.origin.x + x, scene.origin.y + y), size(mw, mh)),
    };
    window.with_content_mask(Some(mask), |window| match frame.hull {
        Some(hull) => window.paint_path(
            crate::product_hull::connective_hull_path(scene, hull),
            colour,
        ),
        None => window.paint_path(
            crate::product_hull::rounded_rect_path(scene, frame.visible),
            colour,
        ),
    });
}

/// FOCUS-01B: one content on the popout's surface: its body's size, its
/// offset along the Bar from the body's leading edge (0 but during a
/// handoff), and what it shows.
pub struct PopoutPart {
    pub size: (f32, f32),
    pub offset: f32,
    pub rows: Vec<AudioRow>,
    pub info: Option<crate::bar_status_popouts::InfoPopout>,
}

/// Where a content of `size` stands inside the revealed part of `frame`,
/// `offset` along the Bar: hung from the joint (the edge the surface grows
/// out of), less what is still inside the Bar while it extrudes, so it
/// slides out with the surface; and laid from the body's leading edge.
pub fn content_origin(frame: &PopoutFrame, size: (f32, f32), offset: f32) -> (f32, f32) {
    let (rect, visible) = (frame.rect, frame.visible);
    use crate::rail_topology::RailSide;
    let across = match frame.side {
        RailSide::Left => -(rect.width() - visible.width()),
        RailSide::Right => rect.width() - size.0,
        RailSide::Top => -(rect.height() - visible.height()),
        RailSide::Bottom => rect.height() - size.1,
    };
    if frame.side.is_horizontal() {
        (rect.left() - visible.left() + offset, across)
    } else {
        (across, rect.top() - visible.top() + offset)
    }
}

/// Full-scene layer: hull canvas plus the clipped content(s).
pub fn popout_layer(
    viewport: (f32, f32),
    frame: PopoutFrame,
    parts: Vec<PopoutPart>,
    preview: Option<f32>,
    material: Rgba,
    own_hull: bool,
) -> Div {
    let scene = Bounds::new(
        point(px(0.0), px(0.0)),
        size(px(viewport.0), px(viewport.1)),
    );
    let hull = canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            if own_hull {
                paint_own_hull(scene, &frame, material, window)
            } else {
                paint_popout_hull(scene, &frame, material, window)
            }
        },
    )
    .absolute()
    .left(px(0.0))
    .top(px(0.0))
    .w(px(viewport.0))
    .h(px(viewport.1));
    let contents = parts.into_iter().map(|part| {
        let (x, y) = content_origin(&frame, part.size, part.offset);
        let rows = part.rows;
        let info = part.info;
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(part.size.0))
            .h(px(part.size.1))
            .children(crate::icons::drawing(
                crate::icons::draw_on(frame.openness),
                || {
                    rows.into_iter()
                        .map(|row| row_element(row, preview))
                        .chain(
                            info.into_iter()
                                .flat_map(|info| info.rows)
                                .map(crate::bar_status_popout_view::info_row_element),
                        )
                        .collect::<Vec<_>>()
                },
            ))
    });
    let clip = div()
        .absolute()
        .left(px(frame.visible.left()))
        .top(px(frame.visible.top()))
        .w(px(frame.visible.width()))
        .h(px(frame.visible.height()))
        .overflow_hidden()
        .children(contents);
    // Zero-size anchor: the layer itself owns no hitbox over the scene.
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .child(hull)
        .child(clip)
}

fn placed(row: &AudioRow) -> Div {
    div()
        .absolute()
        .left(px(row.x))
        .top(px(row.y))
        .w(px(row.width))
        .h(px(row.height))
}

fn text(label: &str, colour: u32, size_px: f32) -> Div {
    div()
        .min_w_0()
        .flex_1()
        .text_size(px(size_px))
        .text_color(rgba(colour))
        .truncate()
        .child(label.to_owned())
}

fn glyph(glyph: StatusGlyph, colour: u32) -> impl IntoElement {
    let drawn = crate::icons::current_drawn();
    canvas(
        |bounds, _, _| bounds,
        move |bounds, _, window, _| {
            let ox = f32::from(bounds.origin.x) + (f32::from(bounds.size.width) - GLYPH) / 2.0;
            let oy = f32::from(bounds.origin.y) + (f32::from(bounds.size.height) - GLYPH) / 2.0;
            for path in status_glyph_paths_drawn(glyph, (ox, oy), GLYPH, drawn) {
                window.paint_path(path, rgba(colour));
            }
        },
    )
    .size_full()
}

fn row_element(row: AudioRow, preview: Option<f32>) -> Div {
    match row.kind {
        AudioRowKind::Header => placed(&row).flex().items_end().child(text(
            &row.label,
            COLOR_ON_SURFACE_VARIANT,
            TEXT_SMALL,
        )),
        AudioRowKind::VolumeLabel => {
            placed(&row)
                .flex()
                .items_center()
                .child(text(&row.label, COLOR_ON_SURFACE, TEXT_BODY))
        }
        AudioRowKind::OutputDevice { selected } | AudioRowKind::InputDevice { selected } => {
            // Only the current default is actionable today; switching devices
            // has no authority yet, so other rows are drawn subdued.
            let colour = if selected {
                COLOR_ON_SURFACE
            } else {
                COLOR_ON_SURFACE_VARIANT
            };
            let radio = div()
                .w(px(RADIO))
                .h(px(RADIO))
                .flex_shrink_0()
                .rounded_full()
                .border_2()
                .border_color(rgba(if selected {
                    COLOR_PRIMARY
                } else {
                    COLOR_OUTLINE_VARIANT
                }))
                .flex()
                .items_center()
                .justify_center()
                .when_selected(selected);
            placed(&row)
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(radio)
                .child(text(&row.label, colour, TEXT_BODY))
        }
        AudioRowKind::Slider { level, muted } => {
            let shown = preview.or(level).unwrap_or(0.0).clamp(0.0, 1.0);
            let fill = if muted {
                COLOR_OUTLINE_VARIANT
            } else {
                COLOR_PRIMARY
            };
            let track_top = (row.height - TRACK) / 2.0;
            placed(&row)
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(track_top))
                        .w(px(row.width))
                        .h(px(TRACK))
                        .rounded_full()
                        .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(track_top))
                        .w(px(row.width * shown))
                        .h(px(TRACK))
                        .rounded_full()
                        .bg(rgba(fill)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px((row.width * shown - 2.0).max(0.0)))
                        .top(px(row.height * 0.2))
                        .w(px(4.0))
                        .h(px(row.height * 0.6))
                        .rounded_full()
                        .bg(rgba(fill)),
                )
        }
        AudioRowKind::MuteOutput { muted } => placed(&row).child(glyph(
            StatusGlyph::Volume(if muted {
                VolumeGlyph::Muted
            } else {
                VolumeGlyph::High
            }),
            if muted {
                COLOR_ON_SURFACE_VARIANT
            } else {
                COLOR_ON_SURFACE
            },
        )),
        AudioRowKind::MuteInput { muted } => placed(&row).child(glyph(
            StatusGlyph::Volume(if muted {
                VolumeGlyph::Muted
            } else {
                VolumeGlyph::Low
            }),
            if muted {
                COLOR_ON_SURFACE_VARIANT
            } else {
                COLOR_ON_SURFACE
            },
        )),
        // No settings surface exists yet (Pending); drawn as an inert outline.
        AudioRowKind::OpenSettings => placed(&row)
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(rgba(COLOR_OUTLINE_VARIANT))
            .child(
                div()
                    .text_size(px(TEXT_BODY))
                    .text_color(rgba(COLOR_ON_SURFACE_VARIANT))
                    .child(row.label.clone()),
            ),
    }
}

trait RadioDot {
    fn when_selected(self, selected: bool) -> Self;
}

impl RadioDot for Div {
    fn when_selected(self, selected: bool) -> Self {
        if selected {
            self.child(
                div()
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded_full()
                    .bg(rgba(COLOR_PRIMARY)),
            )
        } else {
            self
        }
    }
}
