//! Cardinal presentation of the existing Bar content. No settings or state owner.
use crate::product_scene::ProductSemanticState;
use gnos_ux_surface::{
    envelope::SurfaceEdge,
    grammar::{
        AlongEdgeAlignment, BarOrientation, BarPlacement, BarReservationPolicy, BarSensingPolicy,
        ResolvedBarPlacement, ResponsiveRadiusToken, SurfaceViewport,
    },
};
use gpui::{Div, div, prelude::*, px, white};
use std::time::Duration;

const QUALIFICATION_LEG_DURATION: Duration = Duration::from_millis(900);
pub const PRODUCT_BAR_INSET: f32 = 16.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BarTransitionQualification {
    Path {
        edges: Vec<SurfaceEdge>,
        leg_duration: Duration,
    },
    Retarget {
        source: SurfaceEdge,
        via: SurfaceEdge,
        target: SurfaceEdge,
        leg_duration: Duration,
    },
}

impl BarTransitionQualification {
    /// Production Settings consumes the same qualified relocation plan and
    /// duration. This constructor exposes no development selector or geometry.
    pub fn between_edges(source: SurfaceEdge, target: SurfaceEdge) -> Self {
        Self::Path {
            edges: vec![source, target],
            leg_duration: QUALIFICATION_LEG_DURATION,
        }
    }

    pub fn final_target(&self) -> SurfaceEdge {
        match self {
            Self::Path { edges, .. } => *edges.last().expect("validated transition path"),
            Self::Retarget { target, .. } => *target,
        }
    }

    pub fn total_duration(&self) -> Duration {
        match self {
            Self::Path {
                edges,
                leg_duration,
            } => *leg_duration * (edges.len() as u32 - 1),
            Self::Retarget { leg_duration, .. } => *leg_duration + *leg_duration / 2,
        }
    }

    pub fn target_at(&self, elapsed: Duration) -> SurfaceEdge {
        match self {
            Self::Path {
                edges,
                leg_duration,
            } => {
                let leg = (elapsed.as_millis() / leg_duration.as_millis()) as usize;
                edges[leg.min(edges.len() - 1)]
            }
            Self::Retarget {
                via,
                target,
                leg_duration,
                ..
            } => {
                if elapsed < *leg_duration / 2 {
                    *via
                } else {
                    *target
                }
            }
        }
    }
}

pub fn presentation_placement(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    font_px: f32,
) -> Option<BarPlacement> {
    presentation_placement_with_sensing(viewport, edge, font_px, BarSensingPolicy::None)
}

pub fn presentation_placement_with_sensing(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    font_px: f32,
    sensing_policy: BarSensingPolicy,
) -> Option<BarPlacement> {
    presentation_placement_with_policies(
        viewport,
        edge,
        font_px,
        BarReservationPolicy::ReserveOccupiedExtent,
        sensing_policy,
    )
}

pub fn presentation_placement_with_policies(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    font_px: f32,
    reservation_policy: BarReservationPolicy,
    sensing_policy: BarSensingPolicy,
) -> Option<BarPlacement> {
    if !font_px.is_finite() || font_px <= 0. {
        return None;
    }
    let radius = ResponsiveRadiusToken::candidate_default()
        .resolve(viewport)
        .ok()?;
    let vertical = matches!(edge, SurfaceEdge::Left | SurfaceEdge::Right);
    // Twelve upright glyph cells plus the existing 20px inset on each side.
    // This is a presentation capacity, not a geometry/persistence parameter.
    let thickness = if vertical {
        (font_px * 12. * 0.6 + 40.).max(radius * 2.)
    } else {
        56.0_f32.max(radius * 2.)
    };
    let along = if vertical {
        viewport.height
    } else {
        viewport.width
    };
    Some(BarPlacement {
        edge,
        along_edge_alignment: AlongEdgeAlignment::Center,
        thickness,
        span: (along - 32.).min(760.),
        inset: PRODUCT_BAR_INSET,
        reservation_policy,
        sensing_policy,
    })
}

pub fn content(state: &ProductSemanticState, placement: ResolvedBarPlacement, font_px: f32) -> Div {
    let base = div()
        .flex()
        .items_center()
        .px(px(20.))
        .text_color(white())
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(font_px));
    if placement.orientation() == BarOrientation::Horizontal {
        base.child(state.bar_label())
    } else {
        base.flex_col().justify_center().gap(px(12.)).children(
            state
                .bar_groups()
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    div()
                        .id(("bar-group", i))
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label)
                }),
        )
    }
}

/// Development-only endpoint selection. Production builds reject the channel.
pub fn qualification_edge(args: &[String]) -> Result<Option<SurfaceEdge>, String> {
    if args.iter().any(|arg| arg == "--qualify-bar-edge") {
        return Err("qualify-bar-edge requires =top|right|bottom|left".into());
    }
    let values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-edge="))
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Ok(None);
    }
    if !cfg!(feature = "component-lab") {
        return Err("qualify-bar-edge requires component-lab development build".into());
    }
    if values.len() != 1 || !args.iter().any(|arg| arg == "--product-scene") {
        return Err("one qualify-bar-edge is required in product-scene mode".into());
    }
    match values[0] {
        "top" => Ok(Some(SurfaceEdge::Top)),
        "right" => Ok(Some(SurfaceEdge::Right)),
        "bottom" => Ok(Some(SurfaceEdge::Bottom)),
        "left" => Ok(Some(SurfaceEdge::Left)),
        _ => Err("invalid qualify-bar-edge; expected top|right|bottom|left".into()),
    }
}

/// Development-only timed relocation paths and a live mid-flight retarget.
pub fn qualification_transition(
    args: &[String],
) -> Result<Option<BarTransitionQualification>, String> {
    let path_values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-transition="))
        .collect::<Vec<_>>();
    let retarget_values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-retarget="))
        .collect::<Vec<_>>();
    let duration_values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-duration-ms="))
        .collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--qualify-bar-duration-ms") {
        return Err("qualify-bar-duration-ms requires an integer value".into());
    }
    if args
        .iter()
        .any(|arg| arg == "--qualify-bar-transition" || arg == "--qualify-bar-retarget")
    {
        return Err("bar transition qualification requires an =edge:path value".into());
    }
    if path_values.is_empty() && retarget_values.is_empty() {
        if !duration_values.is_empty() {
            return Err("qualify-bar-duration-ms requires a transition qualification".into());
        }
        return Ok(None);
    }
    if !cfg!(feature = "component-lab") || !args.iter().any(|arg| arg == "--product-scene") {
        return Err("qualify-bar-transition requires component-lab product-scene mode".into());
    }
    if path_values.len() + retarget_values.len() != 1 {
        return Err("select exactly one bar transition qualification mode".into());
    }
    if duration_values.len() > 1 {
        return Err("only one qualify-bar-duration-ms is accepted".into());
    }
    let leg_duration: Duration = duration_values.first().map_or(
        Ok::<Duration, &'static str>(QUALIFICATION_LEG_DURATION),
        |value| {
            let millis = value
                .parse::<u64>()
                .map_err(|_| "qualify-bar-duration-ms must be an integer")?;
            if !(900..=10_000).contains(&millis) {
                return Err("qualify-bar-duration-ms must be between 900 and 10000");
            }
            Ok(Duration::from_millis(millis))
        },
    )?;
    let parse = |value: &str| match value {
        "top" => Some(SurfaceEdge::Top),
        "right" => Some(SurfaceEdge::Right),
        "bottom" => Some(SurfaceEdge::Bottom),
        "left" => Some(SurfaceEdge::Left),
        _ => None,
    };
    if let Some(value) = path_values.first() {
        let edges = value
            .split(':')
            .map(|edge| parse(edge).ok_or("invalid transition path edge"))
            .collect::<Result<Vec<_>, _>>()?;
        if !(2..=8).contains(&edges.len()) || edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(
                "transition path requires 2..=8 edges with no repeated adjacent edge".into(),
            );
        }
        Ok(Some(BarTransitionQualification::Path {
            edges,
            leg_duration,
        }))
    } else {
        let mut edges = retarget_values[0]
            .split(':')
            .map(|edge| parse(edge).ok_or("invalid retarget edge"))
            .collect::<Result<Vec<_>, _>>()?;
        if edges.len() != 3 || edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("retarget requires exactly three distinct path edges".into());
        }
        let target = edges.pop().expect("length checked");
        let via = edges.pop().expect("length checked");
        let source = edges.pop().expect("length checked");
        Ok(Some(BarTransitionQualification::Retarget {
            source,
            via,
            target,
            leg_duration,
        }))
    }
}

pub fn workspace_content(
    state: &ProductSemanticState,
    placement: ResolvedBarPlacement,
    font_px: f32,
) -> Div {
    let vertical = placement.orientation() == BarOrientation::Vertical;
    let detail = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .when(!vertical, |d| d.whitespace_nowrap().text_ellipsis())
        .child(state.workspace_detail());
    let close = div()
        .flex_shrink_0()
        .when(!vertical, |d| {
            d.ml(px(crate::product_typography::WORKSPACE_CLOSE_GAP_PX))
        })
        .child(crate::product_typography::WORKSPACE_CLOSE_LABEL);
    div()
        .flex()
        .items_center()
        .px(px(20.))
        .text_color(white())
        .overflow_hidden()
        .text_size(px(font_px))
        .when(vertical, |d| {
            d.flex_col()
                .items_start()
                .justify_center()
                .py(px(20.))
                .gap(px(16.))
        })
        .child(detail)
        .child(close)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_is_bounded_and_development_only() {
        assert_eq!(qualification_edge(&[]), Ok(None));
        for edge in ["top", "right", "bottom", "left"] {
            let args = vec![
                "--product-scene".into(),
                format!("--qualify-bar-edge={edge}"),
            ];
            assert_eq!(
                qualification_edge(&args).is_ok(),
                cfg!(feature = "component-lab")
            );
        }
        for args in [
            vec!["--qualify-bar-edge"],
            vec!["--product-scene", "--qualify-bar-edge=diagonal"],
            vec!["--qualify-bar-edge=top"],
            vec![
                "--product-scene",
                "--qualify-bar-edge=top",
                "--qualify-bar-edge=left",
            ],
        ] {
            assert!(
                qualification_edge(&args.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }

    #[test]
    #[cfg(feature = "component-lab")]
    fn transition_selector_is_bounded_and_development_only() {
        let args = vec![
            "--product-scene".into(),
            "--qualify-bar-transition=top:right".into(),
        ];
        assert_eq!(
            qualification_transition(&args).is_ok(),
            cfg!(feature = "component-lab")
        );
        for args in [
            vec!["--qualify-bar-transition"],
            vec!["--product-scene", "--qualify-bar-transition=top:diagonal"],
            vec!["--product-scene", "--qualify-bar-transition=top:top"],
            vec![
                "--product-scene",
                "--qualify-bar-transition=top:right",
                "--qualify-bar-retarget=top:left:right",
            ],
            vec!["--product-scene", "--qualify-bar-duration-ms=3000"],
            vec![
                "--product-scene",
                "--qualify-bar-transition=top:right",
                "--qualify-bar-duration-ms=899",
            ],
        ] {
            assert!(
                qualification_transition(&args.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }

    #[test]
    #[cfg(feature = "component-lab")]
    fn transition_selector_supports_perimeter_path_and_live_retarget() {
        let path_args = vec![
            "--product-scene".into(),
            "--qualify-bar-transition=top:right:bottom:left".into(),
        ];
        let path = qualification_transition(&path_args)
            .unwrap()
            .expect("path selector");
        assert_eq!(path.total_duration(), Duration::from_millis(2700));
        assert_eq!(
            path.target_at(Duration::from_millis(1800)),
            SurfaceEdge::Bottom
        );
        assert_eq!(
            path.target_at(Duration::from_millis(2700)),
            SurfaceEdge::Left
        );

        let retarget_args = vec![
            "--product-scene".into(),
            "--qualify-bar-retarget=top:left:right".into(),
        ];
        let retarget = qualification_transition(&retarget_args)
            .unwrap()
            .expect("retarget selector");
        assert_eq!(retarget.total_duration(), Duration::from_millis(1350));
        assert_eq!(
            retarget.target_at(Duration::from_millis(449)),
            SurfaceEdge::Left
        );
        assert_eq!(
            retarget.target_at(Duration::from_millis(450)),
            SurfaceEdge::Right
        );

        let slow_path = qualification_transition(&[
            "--product-scene".into(),
            "--qualify-bar-transition=top:right:bottom:left:top".into(),
            "--qualify-bar-duration-ms=3000".into(),
        ])
        .unwrap()
        .expect("duration override");
        assert_eq!(slow_path.total_duration(), Duration::from_secs(12));
        assert_eq!(
            slow_path.target_at(Duration::from_secs(6)),
            SurfaceEdge::Bottom
        );
    }
}
