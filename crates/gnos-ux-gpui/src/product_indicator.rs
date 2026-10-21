//! The recording indicator: a product surface fed by published state.
//!
//! The indicator is painted by the shell, inside the shell's own surface. An
//! application that knows it is recording publishes that fact; the shell
//! decides where it is shown. The application never opens a window of its own
//! on top of the scene, so the indicator inherits the scene's geometry,
//! typography and motion instead of competing with them.
//!
//! The indicator is anchored above the Dock. It is never drawn over the Dock:
//! the Dock is the surface the indicator belongs to, and an indicator that
//! overlaps it reads as a foreign element floating over the shell.

use gpui::{Div, SharedString, Styled, div, prelude::*, px};

use crate::product_design::{
    ProductDesignContext, ProductPalette, ProductRadius, ProductSpacing, ProductTextStyle, text,
};
use gnos_ux_surface::grammar::GeometryRect;

/// The height of the indicator pill.
pub const INDICATOR_HEIGHT: f32 = 40.0;

/// The clear space between the pill and the Dock it belongs to.
pub const INDICATOR_GAP: f32 = 12.0;

/// The width of the pill when it carries no live text.
pub const INDICATOR_COMPACT_WIDTH: f32 = 176.0;

/// The width of the pill when it carries the live transcript.
pub const INDICATOR_WIDE_WIDTH: f32 = 420.0;

/// The resolved geometry of the indicator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorGeometry {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl IndicatorGeometry {
    /// Resolve the pill above the Dock: centred on it, clear of it.
    pub fn above_dock(dock: GeometryRect, width: f32) -> Option<Self> {
        if !(dock.width() > 0.0 && dock.height() > 0.0) {
            return None;
        }
        let center = dock.left() + dock.width() * 0.5;
        let left = center - width * 0.5;
        let top = dock.top() - INDICATOR_GAP - INDICATOR_HEIGHT;
        if !left.is_finite() || !top.is_finite() {
            return None;
        }
        Some(Self {
            left,
            top,
            width,
            height: INDICATOR_HEIGHT,
        })
    }
}

/// What the indicator shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorPresentation {
    /// The state, in the shell's own words.
    pub label: SharedString,
    /// The time spent in the current state.
    pub elapsed: SharedString,
    /// The live transcript, empty when there is none.
    pub text: SharedString,
    /// Whether the state is the one that asks for attention.
    pub recording: bool,
}

impl IndicatorPresentation {
    /// The width this presentation needs.
    pub fn width(&self) -> f32 {
        if self.text.is_empty() {
            INDICATOR_COMPACT_WIDTH
        } else {
            INDICATOR_WIDE_WIDTH
        }
    }
}

/// Format a duration as `m:ss`.
pub fn format_elapsed(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The content of the pill.
pub fn indicator_content(
    presentation: &IndicatorPresentation,
    context: ProductDesignContext,
) -> Div {
    let tone = if presentation.recording {
        ProductPalette::accent()
    } else {
        ProductPalette::text_secondary()
    };
    div()
        .h(px(INDICATOR_HEIGHT))
        .px(px(ProductSpacing::SM))
        .flex()
        .items_center()
        .gap(px(ProductSpacing::XS))
        .rounded(px(ProductRadius::CONTROL))
        .border_1()
        .border_color(tone)
        .bg(ProductPalette::panel_elevated())
        .child(
            div()
                .w(px(10.0))
                .h(px(10.0))
                .flex_shrink_0()
                .rounded(px(5.0))
                .bg(tone),
        )
        .child(text(presentation.label.clone(), ProductTextStyle::Label, context).flex_shrink_0())
        .child(
            text(
                presentation.elapsed.clone(),
                ProductTextStyle::Caption,
                context,
            )
            .flex_shrink_0()
            .text_color(ProductPalette::text_muted()),
        )
        .when(!presentation.text.is_empty(), |pill| {
            pill.child(
                text(presentation.text.clone(), ProductTextStyle::Body, context)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
            )
        })
}

#[cfg(feature = "indicator-production-client")]
mod production {
    use super::{IndicatorPresentation, format_elapsed};
    use gnos_ux_indicator_client::{IndicatorSnapshot, IndicatorState};
    use gpui::SharedString;

    /// Turn a published snapshot into what the desktop element shows.
    #[cfg(feature = "component-lab")]
    pub fn desktop_view_from_snapshot(
        snapshot: &IndicatorSnapshot,
    ) -> crate::desktop_layer::DictationView {
        crate::desktop_layer::DictationView {
            active: snapshot.is_active(),
            recording: matches!(snapshot.state, IndicatorState::Recording),
            elapsed_seconds: snapshot.elapsed_seconds,
            text: snapshot.text.clone(),
            tentative: snapshot.tentative.clone(),
        }
    }

    /// Turn a published snapshot into what the product shows.
    pub fn presentation_from_snapshot(snapshot: &IndicatorSnapshot) -> IndicatorPresentation {
        IndicatorPresentation {
            label: match snapshot.state {
                IndicatorState::Recording => SharedString::from("En écoute"),
                IndicatorState::Transcribing => SharedString::from("Transcription…"),
                IndicatorState::Idle => SharedString::from(snapshot.title.clone()),
            },
            elapsed: SharedString::from(format_elapsed(snapshot.elapsed_seconds)),
            text: SharedString::from(snapshot.text.clone()),
            recording: matches!(snapshot.state, IndicatorState::Recording),
        }
    }
}

#[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
pub use production::desktop_view_from_snapshot;
#[cfg(feature = "indicator-production-client")]
pub use production::presentation_from_snapshot;

#[cfg(feature = "indicator-production-client")]
pub use gnos_ux_indicator_client::toggle_publisher;

#[cfg(feature = "indicator-production-client")]
/// The process-wide registry of published indicators.
///
/// One shell process serves every output, so the registry is a process fact
/// rather than a per-output one: each output paints the same state above its
/// own Dock.
pub fn registry() -> &'static gnos_ux_indicator_client::IndicatorRegistry {
    static REGISTRY: std::sync::OnceLock<gnos_ux_indicator_client::IndicatorRegistry> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let registry = gnos_ux_indicator_client::IndicatorRegistry::new();
        gnos_ux_indicator_client::observe(registry.clone());
        registry
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_indicator_sits_above_the_dock_and_never_on_it() {
        let dock = GeometryRect::try_new(100.0, 900.0, 500.0, 964.0, 16.0).expect("valid dock");
        let geometry = IndicatorGeometry::above_dock(dock, INDICATOR_COMPACT_WIDTH)
            .expect("indicator above the dock");
        assert_eq!(geometry.top + geometry.height + INDICATOR_GAP, dock.top());
        assert_eq!(
            geometry.left + geometry.width * 0.5,
            dock.left() + dock.width() * 0.5
        );
    }

    #[test]
    fn a_degenerate_dock_resolves_no_indicator() {
        let dock = GeometryRect::try_new(0.0, 0.0, 0.0, 0.0, 0.0).expect("valid empty dock");
        assert!(IndicatorGeometry::above_dock(dock, INDICATOR_COMPACT_WIDTH).is_none());
    }

    #[test]
    fn elapsed_time_is_readable() {
        assert_eq!(format_elapsed(0), "0:00");
        assert_eq!(format_elapsed(4), "0:04");
        assert_eq!(format_elapsed(65), "1:05");
    }

    #[test]
    fn a_presentation_without_text_stays_compact() {
        let presentation = IndicatorPresentation {
            label: SharedString::from("En écoute"),
            elapsed: SharedString::from("0:04"),
            text: SharedString::from(""),
            recording: true,
        };
        assert_eq!(presentation.width(), INDICATOR_COMPACT_WIDTH);
    }

    #[test]
    fn a_presentation_with_text_widens() {
        let presentation = IndicatorPresentation {
            label: SharedString::from("En écoute"),
            elapsed: SharedString::from("0:04"),
            text: SharedString::from("bonjour le monde"),
            recording: true,
        };
        assert_eq!(presentation.width(), INDICATOR_WIDE_WIDTH);
    }
}

#[cfg(feature = "component-lab")]
/// Where the element stands, and where its surface is anchored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DictationLayout {
    /// The mic patch: x, y, w, h.
    pub patch: (f32, f32, f32, f32),
    /// The surface's lower-left corner, and its width: x, y, width.
    pub surface_anchor: (f32, f32, f32),
}

/// Resolve the element's geometry.
///
/// The patch stands where the desktop module stands. The surface is anchored
/// by its lower edge, just above the patch: it is the patch's extrusion, and
/// it grows upwards from it.
#[cfg(feature = "component-lab")]
#[allow(clippy::too_many_arguments)]
pub fn dictation_layout(
    place: crate::desktop_modules::ModulePlace,
    free: (f32, f32),
    inset: [f32; 4],
) -> DictationLayout {
    let (x, y, w, h) = crate::desktop_modules::module_rect(
        crate::desktop_modules::ModuleKind::Dictation,
        place,
        free,
    );
    let patch = (x + inset[3], y + inset[0], w, h);
    DictationLayout {
        patch,
        surface_anchor: (
            patch.0,
            patch.1 - crate::desktop_layer::DICTATION_SURFACE_GAP,
            crate::desktop_layer::DICTATION_SURFACE_WIDTH,
        ),
    }
}
/// The element's motion, in the shell's own vocabulary.
///
/// The extrusion is the surface's openness (0 inside the patch, 1 fully out);
/// the two followers are the lengths that track their content — the surface's
/// height as the lines arrive, and the number of characters the typewriter has
/// written so far.
#[cfg(feature = "component-lab")]
#[derive(Clone, Copy, Debug)]
pub struct DictationMotion {
    pub extrusion: gnos_ux_surface::grammar::ExtrusionMotion,
    pub height: gnos_ux_surface::grammar::Follow,
    pub reveal: gnos_ux_surface::grammar::Follow,
}

#[cfg(feature = "component-lab")]
impl Default for DictationMotion {
    fn default() -> Self {
        Self {
            extrusion: gnos_ux_surface::grammar::ExtrusionMotion::closed(),
            height: gnos_ux_surface::grammar::Follow::at(0.0),
            reveal: gnos_ux_surface::grammar::Follow::at(0.0),
        }
    }
}
