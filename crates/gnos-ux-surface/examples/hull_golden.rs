//! Emit the Downward hull commands verbatim, for a byte-exact before/after golden.
//!
//! Temporary scaffold for the HULL-DIRECTIONAL-01 tranche. Its only output is the
//! exact command stream the pre-change Downward construction produces, captured
//! before any edit so the post-change path can be compared against it.

use gnos_ux_surface::grammar::connective_hull::{
    ConnectiveHullGeometry, ConnectiveHullInput, HullCommand,
};
use gnos_ux_surface::grammar::reactive_envelope::{
    ReactiveAttachment, ReactiveEnvelopeInput, ReactiveEnvelopeSolver, ReactiveEnvelopeTuning,
    ReactiveGrowthDirection,
};
use gnos_ux_surface::grammar::{GeometryRect, Point2, SurfaceViewport};

fn golden_input(attachment: ReactiveAttachment) -> ConnectiveHullInput {
    let parent = GeometryRect::try_new(260.0, 16.0, 1020.0, 72.0, 18.0).unwrap();
    let child = GeometryRect::try_new(393.0, 82.0, 887.0, 238.0, 18.0).unwrap();
    ConnectiveHullInput {
        parent,
        child,
        attachment,
        topology_phase: 1.0,
        attachment_phase: 1.0,
        release_phase: 0.0,
        reattach_phase: 1.0,
        neck_width: 228.0,
        mouth_width: 342.0,
        shoulder_run: 0.0,
        shoulder_center_x: 640.0,
        outer_contour: std::array::from_fn(|i| {
            Point2::new(
                child.left()
                    + child.radius()
                    + (child.width() - 2.0 * child.radius()) * i as f32 / 4.0,
                child.bottom(),
            )
        }),
        openness: 1.0,
        growth: ReactiveGrowthDirection::Downward,
    }
}

fn emit(label: &str, hull: ConnectiveHullGeometry) {
    let b = hull.bounds();
    println!(
        "{label} commands={} bounds={:?},{:?}..{:?},{:?}",
        hull.command_count(),
        b.left(),
        b.top(),
        b.right(),
        b.bottom()
    );
    for i in 0..hull.command_count() {
        match hull.command(i).expect("command") {
            HullCommand::Empty => println!("  {i} Empty"),
            HullCommand::MoveTo(p) => {
                println!("  {i} M {:.9e} {:.9e}", p.x(), p.y())
            }
            HullCommand::LineTo(p) => {
                println!("  {i} L {:.9e} {:.9e}", p.x(), p.y())
            }
            HullCommand::CubicTo {
                control1,
                control2,
                to,
            } => println!(
                "  {i} C {:.9e} {:.9e} {:.9e} {:.9e} {:.9e} {:.9e}",
                control1.x(),
                control1.y(),
                control2.x(),
                control2.y(),
                to.x(),
                to.y()
            ),
            HullCommand::Close => println!("  {i} Close"),
        }
    }
}

fn main() {
    // 1. The synthetic downward cases the crate's own tests use.
    for (label, attachment, reattach, release) in [
        ("connected-downward", ReactiveAttachment::Fused, 1.0, 0.0),
        ("bridged-downward", ReactiveAttachment::Bridged, 1.0, 0.0),
        (
            "bridged-reattaching-downward",
            ReactiveAttachment::Bridged,
            0.5,
            0.0,
        ),
        ("detached-downward", ReactiveAttachment::Detached, 0.0, 0.4),
        (
            "detached-reattaching-downward",
            ReactiveAttachment::Detached,
            0.4,
            0.0,
        ),
    ] {
        let mut i = golden_input(attachment);
        i.reattach_phase = reattach;
        i.release_phase = release;
        let hull = ConnectiveHullGeometry::resolve(i).expect("resolve");
        emit(label, hull);
    }

    // 2. The real production Downward path: the workspace summary's own solver
    //    output, so the golden covers the values R09/R13 actually qualified.
    let viewport = SurfaceViewport::try_new(1920.0, 1080.0).unwrap();
    let parent = GeometryRect::try_new(380.0, 8.0, 1540.0, 64.0, 18.0).unwrap();
    let available = GeometryRect::try_new(0.0, 72.0, 1920.0, 1040.0, 0.0).unwrap();
    let mut solver = ReactiveEnvelopeSolver::new(parent, viewport).unwrap();
    let mut frame = None;
    for _ in 0..400 {
        frame = Some(
            solver
                .step(
                    ReactiveEnvelopeInput {
                        attachment_intent: gnos_ux_surface::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                        parent,
                        available,
                        viewport,
                        target_open: true,
                        pointer: None,
                        neighbor: None,
                        reduced_motion: false,
                        growth: ReactiveGrowthDirection::Downward,
                    },
                    1.0 / 60.0,
                    ReactiveEnvelopeTuning::candidate_default(),
                )
                .expect("step"),
        );
    }
    let frame = frame.expect("steps");
    emit("production-workspace-summary-downward", frame.hull);

    println!(
        "  NOTE geometry={:?} attachment={:?} neck={:.9e} connector_w={:.9e}",
        frame.geometry, frame.attachment, frame.neck_width, frame.connector_width
    );
}
