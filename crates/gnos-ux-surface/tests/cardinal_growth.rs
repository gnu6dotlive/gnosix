use gnos_ux_surface::{
    envelope::SurfaceEdge,
    grammar::{GeometryRect, SurfaceViewport, reactive_envelope::*},
};
#[test]
fn a_single_solver_extrudes_into_all_four_inward_half_planes() {
    let viewport = SurfaceViewport::try_new(800., 800.).unwrap();
    for (edge, parent, available) in [
        (
            SurfaceEdge::Top,
            [100., 20., 700., 76.],
            [0., 76., 800., 800.],
        ),
        (
            SurfaceEdge::Bottom,
            [100., 724., 700., 780.],
            [0., 0., 800., 724.],
        ),
        (
            SurfaceEdge::Left,
            [20., 100., 76., 700.],
            [76., 0., 800., 800.],
        ),
        (
            SurfaceEdge::Right,
            [724., 100., 780., 700.],
            [0., 0., 724., 800.],
        ),
    ] {
        let rect = |v: [f32; 4]| GeometryRect::try_new(v[0], v[1], v[2], v[3], 24.).unwrap();
        let parent = rect(parent);
        let available = rect(available);
        let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
        let frame = solver
            .step_from_edge(
                ReactiveEnvelopeInput {
                    attachment_intent: ReactiveAttachmentIntent::Fused,
                    parent,
                    available,
                    viewport,
                    target_open: true,
                    pointer: None,
                    neighbor: None,
                    reduced_motion: true,
                    growth: ReactiveGrowthDirection::Downward,
                },
                edge,
                1. / 60.,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .unwrap();
        match edge {
            SurfaceEdge::Top => assert!(frame.geometry.top() >= parent.bottom()),
            SurfaceEdge::Bottom => assert!(frame.geometry.bottom() <= parent.top()),
            SurfaceEdge::Left => assert!(frame.geometry.left() >= parent.right()),
            SurfaceEdge::Right => assert!(frame.geometry.right() <= parent.left()),
        }
        assert!(frame.geometry.width() > 1. && frame.geometry.height() > 1.);
        assert!(available.contains(frame.geometry));
        assert!(!frame.shape_input_regions().is_empty());
    }
}

#[test]
fn cardinal_local_frame_preserves_existing_motion_and_close_endpoints() {
    let viewport = SurfaceViewport::try_new(1000., 800.).unwrap();
    let swapped_viewport = SurfaceViewport::try_new(800., 1000.).unwrap();
    let parent = GeometryRect::try_new(100., 20., 700., 76., 24.).unwrap();
    let available = GeometryRect::try_new(0., 76., 1000., 800., 0.).unwrap();
    let swap = |r: GeometryRect| {
        GeometryRect::try_new(r.top(), r.left(), r.bottom(), r.right(), r.radius()).unwrap()
    };
    let mut vertical = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
    let mut horizontal = ReactiveEnvelopeSolver::new(swap(parent), swapped_viewport).unwrap();
    let input = |target_open| ReactiveEnvelopeInput {
        attachment_intent: ReactiveAttachmentIntent::Fused,
        parent,
        available,
        viewport,
        target_open,
        pointer: None,
        neighbor: None,
        reduced_motion: false,
        growth: ReactiveGrowthDirection::Downward,
    };
    for target in [true, false] {
        for _ in 0..120 {
            let a = vertical
                .step(
                    input(target),
                    1. / 60.,
                    ReactiveEnvelopeTuning::candidate_default(),
                )
                .unwrap();
            let mut local = input(target);
            local.parent = swap(parent);
            local.available = swap(available);
            local.viewport = swapped_viewport;
            let b = horizontal
                .step_from_edge(
                    local,
                    SurfaceEdge::Left,
                    1. / 60.,
                    ReactiveEnvelopeTuning::candidate_default(),
                )
                .unwrap();
            assert_eq!(b.geometry, swap(a.geometry));
            assert_eq!(b.paint_bounds, swap(a.paint_bounds));
            assert_eq!(b.input_bounds, swap(a.input_bounds));
            assert_eq!(b.openness, a.openness);
            assert_eq!(b.connector_presence, a.connector_presence);
            assert_eq!(
                b.top_contour,
                a.top_contour
                    .map(|p| gnos_ux_surface::grammar::Point2::new(p.y(), p.x()))
            );
            for region in b.shape_input_regions() {
                assert!(b.paint_bounds.contains(region));
            }
        }
    }
}
