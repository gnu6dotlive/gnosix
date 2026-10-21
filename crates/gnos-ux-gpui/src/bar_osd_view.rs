//! SURF-01C: the OSD painted, on the popouts' material.
//!
//! The hull is the popout's (joined to the Bar) and the content slides with
//! the revealed edge: geometry only, no fade. A muted level keeps its
//! track, outlined, with "Muted" in place of the percentage.

use crate::bar_osd::{OSD_HEIGHT, OSD_WIDTH, OsdReading};
use crate::bar_popout_host::PopoutFrame;
use crate::bar_popout_view::paint_popout_hull;
use crate::notification_view::line;
use crate::reference_bar::{
    COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT, COLOR_PRIMARY,
    COLOR_SURFACE_CONTAINER_HIGHEST,
};
use gpui::{Bounds, Div, ParentElement, Rgba, Styled, canvas, div, point, px, rgba, size};

const PAD: f32 = 14.0;
const TRACK: f32 = 6.0;

fn content(reading: &OsdReading) -> Div {
    let inner = OSD_WIDTH - 2.0 * PAD;
    let value = match (reading.muted, reading.percent) {
        (true, _) => gnos_i18n::t("Muted").to_owned(),
        (false, Some(p)) => format!("{p}%"),
        (false, None) => "—".to_owned(),
    };
    let fill = reading
        .percent
        .map_or(0.0, |p| inner * f32::from(p.min(100)) / 100.0);
    let title = if reading.label.is_empty() {
        reading.kind.title().to_owned()
    } else {
        format!("{} · {}", reading.kind.title(), reading.label)
    };
    let header = div()
        .absolute()
        .left(px(PAD))
        .top(px(8.0))
        .w(px(inner))
        .h(px(18.0))
        .flex()
        .items_center()
        .justify_between()
        .overflow_hidden()
        .child(line(&title, COLOR_ON_SURFACE, 12.0, 16.0))
        .child(line(&value, COLOR_ON_SURFACE_VARIANT, 11.0, 16.0));
    let mut track = div()
        .absolute()
        .left(px(PAD))
        .top(px(OSD_HEIGHT - PAD - TRACK))
        .w(px(inner))
        .h(px(TRACK))
        .rounded(px(TRACK / 2.0))
        .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST));
    if reading.percent.is_some() {
        track = track.child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(fill))
                .h(px(TRACK))
                .rounded(px(TRACK / 2.0))
                .bg(rgba(if reading.muted {
                    COLOR_OUTLINE_VARIANT
                } else {
                    COLOR_PRIMARY
                })),
        );
    }
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .w(px(OSD_WIDTH))
        .h(px(OSD_HEIGHT))
        .child(header)
        .child(track)
}

/// The OSD's full-scene layer: hull, then the sliding content.
pub fn bar_osd_layer(
    viewport: (f32, f32),
    frame: PopoutFrame,
    reading: &OsdReading,
    material: Rgba,
) -> Div {
    let scene = Bounds::new(
        point(px(0.0), px(0.0)),
        size(px(viewport.0), px(viewport.1)),
    );
    let hull = canvas(
        |_, _, _| (),
        move |_, _, window, _| paint_popout_hull(scene, &frame, material, window),
    )
    .absolute()
    .left(px(0.0))
    .top(px(0.0))
    .w(px(viewport.0))
    .h(px(viewport.1));
    let (slide_x, slide_y) = match frame.side {
        crate::rail_topology::RailSide::Left => (frame.rect.width() - frame.visible.width(), 0.0),
        crate::rail_topology::RailSide::Top => (0.0, frame.rect.height() - frame.visible.height()),
        crate::rail_topology::RailSide::Right | crate::rail_topology::RailSide::Bottom => {
            (0.0, 0.0)
        }
    };
    let clip = div()
        .absolute()
        .left(px(frame.visible.left()))
        .top(px(frame.visible.top()))
        .w(px(frame.visible.width()))
        .h(px(frame.visible.height()))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(px(-slide_x))
                .top(px(-slide_y))
                .w(px(frame.rect.width()))
                .h(px(frame.rect.height()))
                .child(content(reading)),
        );
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .child(hull)
        .child(clip)
}
