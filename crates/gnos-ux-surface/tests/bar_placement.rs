use gnos_ux_surface::{
    envelope::SurfaceEdge,
    grammar::{SurfaceViewport, placement::*},
};
#[test]
fn cardinal_endpoints_preserve_physical_alignment() {
    let viewport = SurfaceViewport::try_new(800.0, 600.0).unwrap();
    for (edge, expected) in [
        (SurfaceEdge::Top, [300., 10., 500., 40.]),
        (SurfaceEdge::Right, [760., 200., 790., 400.]),
        (SurfaceEdge::Bottom, [300., 560., 500., 590.]),
        (SurfaceEdge::Left, [10., 200., 40., 400.]),
    ] {
        let resolved = BarPlacement {
            edge,
            along_edge_alignment: AlongEdgeAlignment::Center,
            thickness: 30.,
            span: 200.,
            inset: 10.,
            reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
            sensing_policy: BarSensingPolicy::EdgeStrip { thickness: 5. },
        }
        .resolve(viewport)
        .unwrap();
        let rect = resolved.allocation();
        assert_eq!(
            [rect.left(), rect.top(), rect.right(), rect.bottom()],
            expected
        );
    }
}

fn intent(edge: SurfaceEdge) -> BarPlacement {
    BarPlacement {
        edge,
        along_edge_alignment: AlongEdgeAlignment::Center,
        thickness: 30.,
        span: 200.,
        inset: 10.,
        reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
        sensing_policy: BarSensingPolicy::None,
    }
}
const EDGES: [SurfaceEdge; 4] = [
    SurfaceEdge::Top,
    SurfaceEdge::Right,
    SurfaceEdge::Bottom,
    SurfaceEdge::Left,
];
#[test]
fn bases_and_inward_popup_directions_are_cardinal() {
    for (edge, tangent, normal, orientation) in [
        (
            SurfaceEdge::Top,
            (1., 0.),
            (0., 1.),
            BarOrientation::Horizontal,
        ),
        (
            SurfaceEdge::Right,
            (0., 1.),
            (-1., 0.),
            BarOrientation::Vertical,
        ),
        (
            SurfaceEdge::Bottom,
            (-1., 0.),
            (0., -1.),
            BarOrientation::Horizontal,
        ),
        (
            SurfaceEdge::Left,
            (0., -1.),
            (1., 0.),
            BarOrientation::Vertical,
        ),
    ] {
        let basis = EdgeBasis::for_edge(edge);
        assert_eq!((basis.tangent.x(), basis.tangent.y()), tangent);
        assert_eq!((basis.inward_normal.x(), basis.inward_normal.y()), normal);
        assert_eq!(
            basis.tangent.x() * basis.inward_normal.x()
                + basis.tangent.y() * basis.inward_normal.y(),
            0.
        );
        let resolved = intent(edge)
            .resolve(SurfaceViewport::try_new(800., 600.).unwrap())
            .unwrap();
        assert_eq!(resolved.orientation(), orientation);
        assert_eq!(resolved.inward_direction(), basis.inward_normal);
    }
}
#[test]
fn physical_start_and_end_do_not_follow_signed_tangents() {
    for edge in EDGES {
        for (alignment, origin) in [
            (AlongEdgeAlignment::Start, 0.),
            (AlongEdgeAlignment::End, 400.),
        ] {
            let resolved = BarPlacement {
                along_edge_alignment: alignment,
                ..intent(edge)
            }
            .resolve(SurfaceViewport::try_new(600., 600.).unwrap())
            .unwrap();
            let r = resolved.allocation();
            assert_eq!(
                if resolved.orientation() == BarOrientation::Horizontal {
                    r.left()
                } else {
                    r.top()
                },
                origin
            );
        }
    }
}
#[test]
fn viewport_alignment_matrix_is_bounded_and_deterministic() {
    for (w, h) in [
        (1920., 1080.),
        (1080., 1920.),
        (1200., 720.),
        (720., 720.),
        (900., 560.),
    ] {
        let viewport = SurfaceViewport::try_new(w, h).unwrap();
        for edge in EDGES {
            for alignment in [
                AlongEdgeAlignment::Start,
                AlongEdgeAlignment::Center,
                AlongEdgeAlignment::End,
            ] {
                let placement = BarPlacement {
                    along_edge_alignment: alignment,
                    ..intent(edge)
                };
                let a = placement.resolve(viewport).unwrap();
                assert_eq!(a, placement.resolve(viewport).unwrap());
                let r = a.allocation();
                assert!(r.left() >= 0. && r.top() >= 0. && r.right() <= w && r.bottom() <= h);
                assert_eq!(
                    if a.orientation() == BarOrientation::Horizontal {
                        (r.width(), r.height())
                    } else {
                        (r.height(), r.width())
                    },
                    (200., 30.)
                );
                let along = if a.orientation() == BarOrientation::Horizontal {
                    w
                } else {
                    h
                };
                let origin = if a.orientation() == BarOrientation::Horizontal {
                    r.left()
                } else {
                    r.top()
                };
                match alignment {
                    AlongEdgeAlignment::Start => assert_eq!(origin, 0.),
                    AlongEdgeAlignment::Center => assert_eq!(origin + 100., along / 2.),
                    AlongEdgeAlignment::End => assert_eq!(origin + 200., along),
                }
                let reservation = a.reservation();
                let sides = [
                    reservation.top,
                    reservation.right,
                    reservation.bottom,
                    reservation.left,
                ];
                for (index, candidate) in EDGES.into_iter().enumerate() {
                    assert_eq!(sides[index], if edge == candidate { 40. } else { 0. });
                }
            }
        }
    }
}
#[test]
fn malformed_semantic_input_is_rejected_without_clamping() {
    let viewport = SurfaceViewport::try_new(800., 600.).unwrap();
    let base = intent(SurfaceEdge::Top);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0., -1.] {
        assert!(
            BarPlacement {
                thickness: bad,
                ..base
            }
            .resolve(viewport)
            .is_err()
        );
        assert!(
            BarPlacement { span: bad, ..base }
                .resolve(viewport)
                .is_err()
        );
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., 580.] {
        assert!(
            BarPlacement { inset: bad, ..base }
                .resolve(viewport)
                .is_err()
        );
    }
    assert!(
        BarPlacement { span: 801., ..base }
            .resolve(viewport)
            .is_err()
    );
    assert!(
        BarPlacement {
            thickness: 601.,
            ..base
        }
        .resolve(viewport)
        .is_err()
    );
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0., -1., 601.] {
        assert!(
            BarPlacement {
                sensing_policy: BarSensingPolicy::EdgeStrip { thickness: bad },
                ..base
            }
            .resolve(viewport)
            .is_err()
        );
    }
}
#[test]
fn sensing_is_disjoint_and_uses_exact_resolved_span() {
    use gnos_ux_surface::pointer_regions::project_resolved_bar_pointer_regions;
    let viewport = SurfaceViewport::try_new(800., 600.).unwrap();
    for edge in EDGES {
        for (inset, thickness) in [(0., 5.), (10., 5.), (10., 20.)] {
            let a = BarPlacement {
                inset,
                sensing_policy: BarSensingPolicy::EdgeStrip { thickness },
                ..intent(edge)
            }
            .resolve(viewport)
            .unwrap();
            let regions = project_resolved_bar_pointer_regions(viewport, a).unwrap();
            assert_eq!(regions.regions().len(), if inset == 0. { 1 } else { 2 });
            assert_eq!(
                regions
                    .regions()
                    .iter()
                    .find(|r| r.id().as_str() == "bar.input.surface")
                    .unwrap()
                    .bounds(),
                a.allocation()
            );
            if let Some(r) = a.sensing_region() {
                assert_eq!(
                    if a.orientation() == BarOrientation::Horizontal {
                        r.width()
                    } else {
                        r.height()
                    },
                    200.
                );
                assert_eq!(
                    if a.orientation() == BarOrientation::Horizontal {
                        r.height()
                    } else {
                        r.width()
                    },
                    thickness.min(inset)
                );
                match edge {
                    SurfaceEdge::Top => assert_eq!(r.top(), 0.),
                    SurfaceEdge::Right => assert_eq!(r.right(), 800.),
                    SurfaceEdge::Bottom => assert_eq!(r.bottom(), 600.),
                    SurfaceEdge::Left => assert_eq!(r.left(), 0.),
                }
            }
        }
        let overlay = BarPlacement {
            reservation_policy: BarReservationPolicy::Overlay,
            ..intent(edge)
        }
        .resolve(viewport)
        .unwrap();
        assert_eq!(
            overlay.reservation(),
            gnos_ux_surface::scene::SceneReservationIntent::NONE
        );
        assert_eq!(overlay.sensing_region(), None);
    }
}
