use std::{
    env,
    hint::black_box,
    time::{Duration, Instant},
};

use gnos_ux_surface::grammar::{
    GeometryRect, Point2, SurfaceViewport,
    reactive_envelope::{
        ReactiveAttachment, ReactiveEnvelopeFrame, ReactiveEnvelopeInput, ReactiveEnvelopeSolver,
        ReactiveEnvelopeTuning, ReactiveGrowthDirection,
    },
};

const DT: f32 = 1.0 / 60.0;
const WARMUPS: usize = 2;
const REPETITIONS: usize = 5;

#[derive(Clone, Copy)]
struct Scene {
    parent: GeometryRect,
    available: GeometryRect,
    viewport: SurfaceViewport,
    neighbor: Option<GeometryRect>,
}

#[derive(Clone, Copy)]
struct Command {
    scene: Scene,
    target_open: bool,
    pointer_x: Option<f32>,
    reduced_motion: bool,
}

struct Trace {
    id: &'static str,
    commands: Vec<Command>,
}

#[derive(Default)]
struct Metrics {
    max_abs_center_delta: f32,
    max_abs_velocity_delta: f32,
    max_abs_contour_delta: f32,
    max_abs_neck_delta: f32,
    near_displacement: f32,
    far_displacement: f32,
    first_near_response_frame: Option<usize>,
    first_far_response_frame: Option<usize>,
    max_shoulder_child_offset: f32,
    reversal_frame: Option<usize>,
    reversal_velocity_before: Option<f32>,
    reversal_velocity_after: Option<f32>,
    reversal_zero_crossing_frame: Option<usize>,
    reversal_settled_frame: Option<usize>,
    max_reversal_overshoot: f32,
    first_left_pressure_frame: Option<usize>,
    first_right_pressure_frame: Option<usize>,
    first_neighbor_pressure_frame: Option<usize>,
    max_constraint_pressure: f32,
    max_neighbor_pressure: f32,
    max_neighbor_compression: f32,
    max_bilateral_confinement: f32,
    max_hard_bound_penetration: f32,
    attachment_transitions: Vec<(usize, ReactiveAttachment)>,
    settled_frame: Option<usize>,
    settle_events: Vec<(usize, bool)>,
    open_endpoint: Option<ReactiveEnvelopeFrame>,
    close_endpoint: Option<ReactiveEnvelopeFrame>,
    min_neighbor_clearance: Option<f32>,
    step_nanos: Vec<u128>,
}

fn rect(left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> GeometryRect {
    GeometryRect::try_new(left, top, right, bottom, radius).expect("valid trace rectangle")
}

fn scene(width: f32, height: f32) -> Scene {
    let viewport = SurfaceViewport::try_new(width, height).expect("valid viewport");
    let parent_width = width.min(760.0);
    let left = (width - parent_width) * 0.5;
    Scene {
        parent: rect(left, 16.0, left + parent_width, 72.0, 18.0),
        available: rect(0.0, 72.0, width, (height - 16.0).max(72.0), 0.0),
        viewport,
        neighbor: None,
    }
}

fn command(scene: Scene, open: bool, pointer_x: Option<f32>, reduced: bool) -> Command {
    Command {
        scene,
        target_open: open,
        pointer_x,
        reduced_motion: reduced,
    }
}

fn hold(trace: &mut Vec<Command>, command: Command, frames: usize) {
    trace.extend(std::iter::repeat_n(command, frames));
}

fn point_x(x: Option<f32>, y: f32) -> Option<Point2> {
    x.map(|x| Point2::new(x, y))
}

fn input(command: Command) -> ReactiveEnvelopeInput {
    ReactiveEnvelopeInput {
        attachment_intent:
            gnos_ux_surface::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
        parent: command.scene.parent,
        available: command.scene.available,
        viewport: command.scene.viewport,
        target_open: command.target_open,
        pointer: point_x(command.pointer_x, 100.0),
        neighbor: command.scene.neighbor,
        reduced_motion: command.reduced_motion,
        growth: ReactiveGrowthDirection::Downward,
    }
}

fn settle(
    solver: &mut ReactiveEnvelopeSolver,
    command: Command,
    tuning: ReactiveEnvelopeTuning,
) -> ReactiveEnvelopeFrame {
    for _ in 0..900 {
        let frame = solver
            .step(input(command), DT, tuning)
            .expect("endpoint trace frame resolves");
        if solver.is_settled_for(command.target_open) {
            return frame;
        }
    }
    panic!(
        "endpoint failed to settle: target_open={}",
        command.target_open
    );
}

fn same_endpoint(a: ReactiveEnvelopeFrame, b: ReactiveEnvelopeFrame) -> bool {
    a.geometry == b.geometry
        && a.paint_bounds == b.paint_bounds
        && a.input_bounds == b.input_bounds
        && a.top_contour == b.top_contour
        && a.attachment == b.attachment
        && a.attachment_phase == b.attachment_phase
        && a.release_phase == b.release_phase
        && a.reattach_phase == b.reattach_phase
        && a.connector_width == b.connector_width
        && a.neck_width == b.neck_width
}

fn endpoint_equivalence() -> Vec<(&'static str, bool)> {
    let wide = scene(1280.0, 800.0);
    let tuning = ReactiveEnvelopeTuning::candidate_default();
    let mut normal =
        ReactiveEnvelopeSolver::new(wide.parent, wide.viewport).expect("normal solver");
    let normal_open = settle(&mut normal, command(wide, true, None, false), tuning);
    let mut reduced =
        ReactiveEnvelopeSolver::new(wide.parent, wide.viewport).expect("reduced solver");
    let reduced_open = reduced
        .step(input(command(wide, true, None, true)), DT, tuning)
        .expect("reduced open");

    let normal_detached = settle(&mut normal, command(wide, true, Some(0.0), false), tuning);
    let mut reduced_displaced = ReactiveEnvelopeSolver::new(wide.parent, wide.viewport)
        .expect("reduced displacement solver");
    let _ = reduced_displaced
        .step(input(command(wide, true, None, true)), DT, tuning)
        .expect("reduced open seed");
    let reduced_detached = reduced_displaced
        .step(input(command(wide, true, Some(0.0), true)), DT, tuning)
        .expect("reduced displacement");

    let normal_reattached = settle(&mut normal, command(wide, true, None, false), tuning);
    let reduced_reattached = reduced_displaced
        .step(input(command(wide, true, None, true)), DT, tuning)
        .expect("reduced reattach");
    let normal_closed = settle(&mut normal, command(wide, false, None, false), tuning);
    let reduced_closed = reduced_displaced
        .step(input(command(wide, false, None, true)), DT, tuning)
        .expect("reduced close");

    vec![
        ("open", same_endpoint(normal_open, reduced_open)),
        (
            "displaced-detached",
            same_endpoint(normal_detached, reduced_detached),
        ),
        (
            "reattached",
            same_endpoint(normal_reattached, reduced_reattached),
        ),
        ("closed", same_endpoint(normal_closed, reduced_closed)),
    ]
}

fn traces() -> Vec<Trace> {
    let wide = scene(1280.0, 800.0);
    let mut traces = Vec::new();

    let mut t01 = Vec::new();
    hold(&mut t01, command(wide, false, None, false), 1);
    hold(&mut t01, command(wide, true, None, false), 300);
    traces.push(Trace {
        id: "T01-open-from-rest",
        commands: t01,
    });

    let mut t02 = Vec::new();
    hold(&mut t02, command(wide, true, None, false), 300);
    hold(&mut t02, command(wide, false, None, false), 300);
    traces.push(Trace {
        id: "T02-close-from-settled-open",
        commands: t02,
    });

    let mut t03 = Vec::new();
    hold(&mut t03, command(wide, true, None, false), 300);
    for (from, to, count) in [
        (640.0, 1020.0, 60),
        (1020.0, 640.0, 60),
        (640.0, 260.0, 60),
        (260.0, 640.0, 60),
    ] {
        for i in 1..=count {
            let x = from + (to - from) * i as f32 / count as f32;
            t03.push(command(wide, true, Some(x), false));
        }
    }
    hold(&mut t03, command(wide, true, None, false), 240);
    traces.push(Trace {
        id: "T03-slow-lateral-sweep",
        commands: t03,
    });

    let mut t04 = Vec::new();
    hold(&mut t04, command(wide, true, None, false), 300);
    for (from, to, count) in [
        (640.0, 1020.0, 12),
        (1020.0, 640.0, 12),
        (640.0, 260.0, 12),
        (260.0, 640.0, 12),
    ] {
        for i in 1..=count {
            let x = from + (to - from) * i as f32 / count as f32;
            t04.push(command(wide, true, Some(x), false));
        }
    }
    hold(&mut t04, command(wide, true, None, false), 240);
    traces.push(Trace {
        id: "T04-fast-lateral-sweep",
        commands: t04,
    });

    let mut t05 = Vec::new();
    hold(&mut t05, command(wide, true, None, false), 300);
    for i in 1..=18 {
        let x = 640.0 + (1020.0 - 640.0) * i as f32 / 18.0;
        t05.push(command(wide, true, Some(x), false));
    }
    for i in 1..=36 {
        let x = 1020.0 + (260.0 - 1020.0) * i as f32 / 36.0;
        t05.push(command(wide, true, Some(x), false));
    }
    for i in 1..=18 {
        let x = 260.0 + (640.0 - 260.0) * i as f32 / 18.0;
        t05.push(command(wide, true, Some(x), false));
    }
    hold(&mut t05, command(wide, true, None, false), 240);
    traces.push(Trace {
        id: "T05-reversal-before-settle",
        commands: t05,
    });

    // Centered on a wide output, the same fixed-gain input trace approaches
    // the left and right soft ranges without changing solver parameters.
    let edge_scene = wide;
    let mut t06 = Vec::new();
    hold(&mut t06, command(edge_scene, true, None, false), 300);
    for x in [0.0, 260.0, 640.0, 1020.0, 1280.0, 640.0] {
        hold(&mut t06, command(edge_scene, true, Some(x), false), 48);
    }
    hold(&mut t06, command(edge_scene, true, None, false), 240);
    traces.push(Trace {
        id: "T06-unilateral-edge-approach",
        commands: t06,
    });

    let mut confined = scene(1062.0, 943.0);
    confined.parent = rect(151.0, 16.0, 911.0, 72.0, 18.0);
    let mut t07 = Vec::new();
    hold(&mut t07, command(confined, true, None, false), 300);
    hold(&mut t07, command(confined, true, Some(911.0), false), 60);
    hold(&mut t07, command(confined, true, None, false), 240);
    traces.push(Trace {
        id: "T07-bilateral-confinement",
        commands: t07,
    });

    let mut short = scene(1280.0, 400.0);
    short.available = rect(0.0, 72.0, 1280.0, 380.0, 0.0);
    short.neighbor = Some(rect(380.0, 320.0, 900.0, 384.0, 20.0));
    let mut t08 = Vec::new();
    hold(&mut t08, command(short, true, None, false), 300);
    hold(&mut t08, command(short, false, None, false), 240);
    traces.push(Trace {
        id: "T08-dock-neighbor-compression",
        commands: t08,
    });

    let mut t09 = Vec::new();
    hold(&mut t09, command(wide, true, None, false), 300);
    let mut moved = wide;
    moved.parent = rect(300.0, 16.0, 1060.0, 72.0, 18.0);
    hold(&mut t09, command(moved, true, None, false), 180);
    hold(&mut t09, command(wide, true, None, false), 180);
    traces.push(Trace {
        id: "T09-parent-rebase",
        commands: t09,
    });

    let mut t10 = Vec::new();
    hold(&mut t10, command(wide, true, None, false), 300);
    hold(&mut t10, command(wide, true, Some(0.0), false), 120);
    hold(&mut t10, command(wide, true, None, false), 180);
    hold(&mut t10, command(wide, false, None, false), 240);
    traces.push(Trace {
        id: "T10-detach-reattach",
        commands: t10,
    });

    for (name, setup, endpoint) in [
        (
            "T11-reduced-open",
            Vec::new(),
            command(wide, true, None, true),
        ),
        (
            "T11-reduced-displaced",
            vec![command(wide, true, None, false); 300],
            command(wide, true, Some(0.0), true),
        ),
        (
            "T11-reduced-reattached",
            [
                vec![command(wide, true, None, false); 300],
                vec![command(wide, true, Some(0.0), false); 120],
            ]
            .concat(),
            command(wide, true, None, true),
        ),
        (
            "T11-reduced-closed",
            vec![command(wide, true, None, false); 300],
            command(wide, false, None, true),
        ),
    ] {
        let mut frames = setup;
        frames.push(endpoint);
        traces.push(Trace {
            id: name,
            commands: frames,
        });
    }

    traces
}

fn center(frame: &ReactiveEnvelopeFrame) -> (f32, f32) {
    (
        frame.geometry.left() + frame.geometry.width() * 0.5,
        frame.geometry.top() + frame.geometry.height() * 0.5,
    )
}

fn float_fields(frame: &ReactiveEnvelopeFrame, command: Command) -> Vec<f32> {
    let mut values = vec![
        DT,
        command.scene.viewport.width,
        command.scene.viewport.height,
        command.pointer_x.unwrap_or(0.0),
    ];
    values.extend([
        command.scene.parent.left(),
        command.scene.parent.top(),
        command.scene.parent.right(),
        command.scene.parent.bottom(),
        command.scene.available.left(),
        command.scene.available.top(),
        command.scene.available.right(),
        command.scene.available.bottom(),
    ]);
    if let Some(neighbor) = command.scene.neighbor {
        values.extend([
            neighbor.left(),
            neighbor.top(),
            neighbor.right(),
            neighbor.bottom(),
        ]);
    } else {
        values.extend([0.0; 4]);
    }
    values.extend([
        frame.geometry.left(),
        frame.geometry.top(),
        frame.geometry.right(),
        frame.geometry.bottom(),
        frame.geometry.radius(),
        frame.paint_bounds.left(),
        frame.paint_bounds.top(),
        frame.paint_bounds.right(),
        frame.paint_bounds.bottom(),
        frame.input_bounds.left(),
        frame.input_bounds.top(),
        frame.input_bounds.right(),
        frame.input_bounds.bottom(),
        frame.openness,
        frame.velocity_x,
        frame.velocity_y,
        frame.neck_width,
        frame.connector.p0().x(),
        frame.deformation,
        frame.constraint_pressure,
        frame.edge_response.left_pressure,
        frame.edge_response.right_pressure,
        frame.edge_response.directional_bias,
        frame.edge_response.bilateral_confinement,
        frame.neighbor_pressure,
        frame.neighbor_compression,
        frame.attachment_phase,
        frame.release_phase,
        frame.reattach_phase,
        frame.connector_width,
        frame.connector_presence,
    ]);
    for point in frame.top_contour {
        values.extend([point.x(), point.y()]);
    }
    values
}

fn hash_sample(
    mut hash: u64,
    floats: &[f32],
    booleans: &[bool],
    attachment: ReactiveAttachment,
) -> u64 {
    for value in floats {
        for byte in value.to_bits().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    for value in booleans.iter().copied().chain([match attachment {
        ReactiveAttachment::Fused => false,
        ReactiveAttachment::Bridged => true,
        ReactiveAttachment::Detached => true,
    }]) {
        hash ^= u64::from(value);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= match attachment {
        ReactiveAttachment::Fused => 0,
        ReactiveAttachment::Bridged => 1,
        ReactiveAttachment::Detached => 2,
    };
    hash.wrapping_mul(0x100000001b3)
}

fn f(value: f32) -> String {
    if value.is_finite() {
        format!("{value:.7}")
    } else {
        "null".to_owned()
    }
}

fn rect_json(rect: GeometryRect) -> String {
    format!(
        "[{}, {}, {}, {}, {}]",
        f(rect.left()),
        f(rect.top()),
        f(rect.right()),
        f(rect.bottom()),
        f(rect.radius())
    )
}

fn run(trace: &Trace, repetition: usize, emit: bool) -> (u64, Metrics, Duration) {
    let first = trace.commands[0].scene;
    let mut solver =
        ReactiveEnvelopeSolver::new(first.parent, first.viewport).expect("solver init");
    let tuning = ReactiveEnvelopeTuning::candidate_default();
    let mut metrics = Metrics::default();
    let mut hash = 0xcbf29ce484222325_u64;
    let mut previous: Option<ReactiveEnvelopeFrame> = None;
    let mut previous_attachment = ReactiveAttachment::Fused;
    let mut settled_open_baseline: Option<ReactiveEnvelopeFrame> = None;
    let mut previous_pointer_x: Option<f32> = None;
    let mut previous_pointer_direction = 0.0_f32;
    let mut reversal_old_direction: Option<f32> = None;
    let mut previous_body_velocity: Option<f32> = None;
    let mut active_target = trace.commands[0].target_open;
    let mut target_segment_start = 0usize;
    let mut target_segment_settled = false;
    let start = Instant::now();

    for (frame_index, cmd) in trace.commands.iter().copied().enumerate() {
        if cmd.target_open != active_target {
            active_target = cmd.target_open;
            target_segment_start = frame_index;
            target_segment_settled = false;
        }
        let step_start = Instant::now();
        let frame = solver
            .step(input(cmd), DT, tuning)
            .expect("trace frame resolves");
        let elapsed = step_start.elapsed().as_nanos();
        metrics.step_nanos.push(elapsed);
        if let Some(pointer_x) = cmd.pointer_x {
            let origin = previous_pointer_x
                .unwrap_or_else(|| (cmd.scene.parent.left() + cmd.scene.parent.right()) * 0.5);
            let pointer_delta = pointer_x - origin;
            if pointer_delta.abs() > 0.001 {
                let direction = pointer_delta.signum();
                if previous_pointer_direction != 0.0 && direction != previous_pointer_direction {
                    metrics.reversal_frame.get_or_insert(frame_index);
                    if let Some(prior) = previous {
                        metrics
                            .reversal_velocity_before
                            .get_or_insert(prior.velocity_x);
                    }
                    metrics
                        .reversal_velocity_after
                        .get_or_insert(frame.velocity_x);
                    reversal_old_direction = Some(previous_pointer_direction);
                }
                previous_pointer_direction = direction;
            }
        }
        previous_pointer_x = cmd.pointer_x;

        if frame.attachment != previous_attachment {
            metrics
                .attachment_transitions
                .push((frame_index, frame.attachment));
            previous_attachment = frame.attachment;
        }
        if frame.edge_response.left_pressure > 0.0 && metrics.first_left_pressure_frame.is_none() {
            metrics.first_left_pressure_frame = Some(frame_index);
        }
        if frame.edge_response.right_pressure > 0.0 && metrics.first_right_pressure_frame.is_none()
        {
            metrics.first_right_pressure_frame = Some(frame_index);
        }
        if frame.neighbor_pressure > 0.0 && metrics.first_neighbor_pressure_frame.is_none() {
            metrics.first_neighbor_pressure_frame = Some(frame_index);
        }
        metrics.max_constraint_pressure = metrics
            .max_constraint_pressure
            .max(frame.constraint_pressure);
        metrics.max_neighbor_pressure = metrics.max_neighbor_pressure.max(frame.neighbor_pressure);
        metrics.max_neighbor_compression = metrics
            .max_neighbor_compression
            .max(frame.neighbor_compression);
        metrics.max_bilateral_confinement = metrics
            .max_bilateral_confinement
            .max(frame.edge_response.bilateral_confinement);
        let hard_penetration = (cmd.scene.available.left() - frame.geometry.left())
            .max(0.0)
            .max((frame.geometry.right() - cmd.scene.available.right()).max(0.0));
        metrics.max_hard_bound_penetration =
            metrics.max_hard_bound_penetration.max(hard_penetration);
        if let Some(neighbor) = cmd.scene.neighbor {
            let clearance = neighbor.top() - frame.geometry.bottom();
            metrics.min_neighbor_clearance = Some(
                metrics
                    .min_neighbor_clearance
                    .map_or(clearance, |old| old.min(clearance)),
            );
        }
        let (cx, _) = center(&frame);
        if let (Some(old_direction), Some(previous_velocity)) =
            (reversal_old_direction, previous_body_velocity)
        {
            if metrics.reversal_zero_crossing_frame.is_none()
                && previous_velocity * frame.velocity_x <= 0.0
            {
                metrics.reversal_zero_crossing_frame = Some(frame_index);
            }
            let parent_center = (cmd.scene.parent.left() + cmd.scene.parent.right()) * 0.5;
            let requested_shift = cmd.pointer_x.map_or(0.0, |pointer_x| {
                let offset = pointer_x - parent_center;
                let active =
                    offset.signum() * (offset.abs() - cmd.scene.parent.width() * 0.03).max(0.0);
                (active * 0.12).clamp(
                    -cmd.scene.parent.width() * 0.07,
                    cmd.scene.parent.width() * 0.07,
                )
            });
            let target_center = parent_center
                + if cmd.target_open {
                    requested_shift
                } else {
                    0.0
                };
            metrics.max_reversal_overshoot = metrics
                .max_reversal_overshoot
                .max(old_direction * (cx - target_center));
            if solver.is_settled_for(cmd.target_open) {
                metrics.reversal_settled_frame.get_or_insert(frame_index);
            }
        }
        previous_body_velocity = Some(frame.velocity_x);
        metrics.max_shoulder_child_offset = metrics
            .max_shoulder_child_offset
            .max((frame.connector.p0().x() - cx).abs());
        if cmd.target_open && cmd.pointer_x.is_none() && solver.is_settled_for(true) {
            settled_open_baseline = Some(frame);
        }
        if let Some(x) = cmd.pointer_x
            && let Some(baseline) = settled_open_baseline
        {
            let nearest = (0..baseline.top_contour.len())
                .min_by(|a, b| {
                    (baseline.top_contour[*a].x() - x)
                        .abs()
                        .total_cmp(&(baseline.top_contour[*b].x() - x).abs())
                })
                .expect("contour sample exists");
            let farthest = (0..baseline.top_contour.len())
                .max_by(|a, b| {
                    (baseline.top_contour[*a].x() - x)
                        .abs()
                        .total_cmp(&(baseline.top_contour[*b].x() - x).abs())
                })
                .expect("contour sample exists");
            let near = (frame.top_contour[nearest].y() - baseline.top_contour[nearest].y()).abs();
            let far = (frame.top_contour[farthest].y() - baseline.top_contour[farthest].y()).abs();
            metrics.near_displacement = metrics.near_displacement.max(near);
            metrics.far_displacement = metrics.far_displacement.max(far);
            if near > 0.0001 && metrics.first_near_response_frame.is_none() {
                metrics.first_near_response_frame = Some(frame_index);
            }
            if far > 0.0001 && metrics.first_far_response_frame.is_none() {
                metrics.first_far_response_frame = Some(frame_index);
            }
        }
        if let Some(prior) = previous {
            let (before_x, _) = center(&prior);
            metrics.max_abs_center_delta = metrics.max_abs_center_delta.max((cx - before_x).abs());
            metrics.max_abs_velocity_delta = metrics.max_abs_velocity_delta.max(
                (frame.velocity_x - prior.velocity_x)
                    .abs()
                    .max((frame.velocity_y - prior.velocity_y).abs()),
            );
            metrics.max_abs_neck_delta = metrics
                .max_abs_neck_delta
                .max((frame.neck_width - prior.neck_width).abs());
            for i in 0..frame.top_contour.len() {
                metrics.max_abs_contour_delta = metrics
                    .max_abs_contour_delta
                    .max(frame.top_contour[i].distance_to(prior.top_contour[i]));
            }
        }
        if solver.is_settled_for(cmd.target_open)
            && !cmd.reduced_motion
            && metrics.settled_frame.is_none()
        {
            metrics.settled_frame = Some(frame_index);
        }
        if solver.is_settled_for(cmd.target_open) && !target_segment_settled {
            metrics.settle_events.push((
                frame_index.saturating_sub(target_segment_start),
                cmd.target_open,
            ));
            target_segment_settled = true;
        }
        if solver.is_settled_for(cmd.target_open) {
            if cmd.target_open {
                metrics.open_endpoint = Some(frame);
            } else {
                metrics.close_endpoint = Some(frame);
            }
        }

        let floats = float_fields(&frame, cmd);
        hash = hash_sample(
            hash,
            &floats,
            &[
                cmd.target_open,
                cmd.pointer_x.is_some(),
                cmd.reduced_motion,
                cmd.scene.neighbor.is_some(),
                solver.is_settled_for(cmd.target_open),
            ],
            frame.attachment,
        );
        if emit {
            let pointer = cmd.pointer_x.map_or_else(|| "null".to_owned(), f);
            println!(
                "{{\"kind\":\"sample\",\"scenario\":\"{}\",\"repetition\":{},\"frame_index\":{},\"dt\":{},\"target_open\":{},\"pointer_x\":{},\"pointer_y\":{},\"parent\":{},\"available\":{},\"neighbor\":{},\"child\":{},\"center\":[{},{}],\"velocity\":[{},{}],\"top_contour\":[[{},{}],[{},{}],[{},{}],[{},{}],[{},{}]],\"neck_width\":{},\"shoulder_center\":{},\"deformation\":{},\"constraint_pressure\":{},\"edge_left\":{},\"edge_right\":{},\"directional_bias\":{},\"bilateral_confinement\":{},\"neighbor_pressure\":{},\"neighbor_compression\":{},\"attachment\":\"{:?}\",\"attachment_phase\":{},\"release_phase\":{},\"reattach_phase\":{},\"connector_width\":{},\"connector_presence\":{},\"settled\":{},\"step_ns\":{}}}",
                trace.id,
                repetition,
                frame_index,
                f(DT),
                cmd.target_open,
                pointer,
                cmd.pointer_x
                    .map_or_else(|| "null".to_owned(), |_| f(100.0)),
                rect_json(cmd.scene.parent),
                rect_json(cmd.scene.available),
                cmd.scene
                    .neighbor
                    .map_or_else(|| "null".to_owned(), rect_json),
                rect_json(frame.geometry),
                f(cx),
                f(frame.geometry.top() + frame.geometry.height() * 0.5),
                f(frame.velocity_x),
                f(frame.velocity_y),
                f(frame.top_contour[0].x()),
                f(frame.top_contour[0].y()),
                f(frame.top_contour[1].x()),
                f(frame.top_contour[1].y()),
                f(frame.top_contour[2].x()),
                f(frame.top_contour[2].y()),
                f(frame.top_contour[3].x()),
                f(frame.top_contour[3].y()),
                f(frame.top_contour[4].x()),
                f(frame.top_contour[4].y()),
                f(frame.neck_width),
                f(frame.connector.p0().x()),
                f(frame.deformation),
                f(frame.constraint_pressure),
                f(frame.edge_response.left_pressure),
                f(frame.edge_response.right_pressure),
                f(frame.edge_response.directional_bias),
                f(frame.edge_response.bilateral_confinement),
                f(frame.neighbor_pressure),
                f(frame.neighbor_compression),
                frame.attachment,
                f(frame.attachment_phase),
                f(frame.release_phase),
                f(frame.reattach_phase),
                f(frame.connector_width),
                f(frame.connector_presence),
                solver.is_settled_for(cmd.target_open),
                elapsed
            );
        }
        previous = Some(frame);
        black_box(frame);
    }
    (hash, metrics, start.elapsed())
}

fn percentile(values: &mut [u128], p: f64) -> u128 {
    values.sort_unstable();
    let index = ((values.len() - 1) as f64 * p).round() as usize;
    values[index]
}

fn main() {
    let repetitions = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(REPETITIONS);
    let traces = traces();
    let tuning = ReactiveEnvelopeTuning::candidate_default();
    println!(
        "{{\"kind\":\"header\",\"harness\":\"gnos-ux-e11-native-baseline\",\"schema\":1,\"host_arch\":\"{}\",\"host_os\":\"{}\",\"dt\":{},\"warmup_runs\":{},\"measured_repetitions\":{},\"trace_count\":{},\"candidate_tuning\":{{\"response\":{},\"damping\":{},\"elasticity\":{},\"propagation\":{},\"shape_preservation\":{},\"edge_affinity\":{},\"bilateral_contour_gain\":{},\"neighbor_compression_gain\":{},\"fusion_hysteresis_radius_ratio\":{},\"detach_strain_ratio\":{},\"reattach_strain_ratio\":{}}}}}",
        env::consts::ARCH,
        env::consts::OS,
        f(DT),
        WARMUPS,
        repetitions,
        traces.len(),
        f(tuning.response),
        f(tuning.damping),
        f(tuning.elasticity),
        f(tuning.propagation),
        f(tuning.shape_preservation),
        f(tuning.edge_affinity),
        f(tuning.bilateral_contour_gain),
        f(tuning.neighbor_compression_gain),
        f(tuning.fusion_hysteresis_radius_ratio),
        f(tuning.detach_strain_ratio),
        f(tuning.reattach_strain_ratio)
    );
    let endpoint_checks = endpoint_equivalence();
    println!(
        "{{\"kind\":\"endpoint-equivalence\",\"normal_vs_reduced\":[{}]}}",
        endpoint_checks
            .iter()
            .map(|(name, pass)| format!("{{\"endpoint\":\"{name}\",\"exact_match\":{pass}}}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    for trace in traces {
        for warmup in 0..WARMUPS {
            let _ = run(&trace, usize::MAX - warmup, false);
        }
        let mut hashes = Vec::new();
        let mut timing = Vec::new();
        let mut final_metrics = Metrics::default();
        for repetition in 0..repetitions {
            let (hash, metrics, elapsed) = run(&trace, repetition, repetition == 0);
            hashes.push(hash);
            timing.extend(metrics.step_nanos.iter().copied());
            final_metrics = metrics;
            println!(
                "{{\"kind\":\"trace-repeat\",\"scenario\":\"{}\",\"repetition\":{},\"frames\":{},\"total_ns\":{},\"replay_hash\":\"{hash:016x}\",\"step_ns_samples\":[{}]}}",
                trace.id,
                repetition,
                trace.commands.len(),
                elapsed.as_nanos(),
                final_metrics
                    .step_nanos
                    .iter()
                    .map(u128::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        let deterministic = hashes.windows(2).all(|pair| pair[0] == pair[1]);
        let p50 = percentile(&mut timing.clone(), 0.50);
        let p95 = percentile(&mut timing.clone(), 0.95);
        let p99 = percentile(&mut timing.clone(), 0.99);
        let transitions = final_metrics
            .attachment_transitions
            .iter()
            .map(|(frame, attachment)| {
                format!("{{\"frame\":{frame},\"attachment\":\"{attachment:?}\"}}")
            })
            .collect::<Vec<_>>()
            .join(",");
        let settle_events = final_metrics
            .settle_events
            .iter()
            .map(|(frame, open)| {
                format!("{{\"frames_from_target_change\":{frame},\"target_open\":{open}}}")
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"kind\":\"summary\",\"scenario\":\"{}\",\"frames_per_run\":{},\"repetitions\":{},\"replay_hashes\":[{}],\"deterministic_same_build_host\":{},\"step_ns_p50\":{},\"step_ns_p95\":{},\"step_ns_p99\":{},\"max_center_delta\":{},\"max_velocity_delta\":{},\"max_contour_delta\":{},\"max_neck_delta\":{},\"max_shoulder_child_offset\":{},\"near_contour_displacement\":{},\"far_contour_displacement\":{},\"near_far_ratio\":{},\"first_near_response_frame\":{},\"first_far_response_frame\":{},\"reversal_frame\":{},\"reversal_velocity_before\":{},\"reversal_velocity_after\":{},\"reversal_zero_crossing_frame\":{},\"reversal_settled_frame\":{},\"max_reversal_overshoot\":{},\"max_constraint_pressure\":{},\"max_neighbor_pressure\":{},\"max_neighbor_compression\":{},\"max_bilateral_confinement\":{},\"max_hard_bound_penetration\":{},\"first_left_pressure_frame\":{},\"first_right_pressure_frame\":{},\"first_neighbor_pressure_frame\":{},\"settled_frame\":{},\"min_neighbor_clearance\":{},\"attachment_transitions\":[{}],\"settle_events\":[{}]}}",
            trace.id,
            trace.commands.len(),
            repetitions,
            hashes
                .iter()
                .map(|h| format!("\"{h:016x}\""))
                .collect::<Vec<_>>()
                .join(","),
            deterministic,
            p50,
            p95,
            p99,
            f(final_metrics.max_abs_center_delta),
            f(final_metrics.max_abs_velocity_delta),
            f(final_metrics.max_abs_contour_delta),
            f(final_metrics.max_abs_neck_delta),
            f(final_metrics.max_shoulder_child_offset),
            f(final_metrics.near_displacement),
            f(final_metrics.far_displacement),
            if final_metrics.far_displacement > 0.0 {
                f(final_metrics.near_displacement / final_metrics.far_displacement)
            } else {
                "null".to_owned()
            },
            final_metrics
                .first_near_response_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .first_far_response_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .reversal_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .reversal_velocity_before
                .map_or_else(|| "null".to_owned(), f),
            final_metrics
                .reversal_velocity_after
                .map_or_else(|| "null".to_owned(), f),
            final_metrics
                .reversal_zero_crossing_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .reversal_settled_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            f(final_metrics.max_reversal_overshoot),
            f(final_metrics.max_constraint_pressure),
            f(final_metrics.max_neighbor_pressure),
            f(final_metrics.max_neighbor_compression),
            f(final_metrics.max_bilateral_confinement),
            f(final_metrics.max_hard_bound_penetration),
            final_metrics
                .first_left_pressure_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .first_right_pressure_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .first_neighbor_pressure_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .settled_frame
                .map_or_else(|| "null".to_owned(), |v| v.to_string()),
            final_metrics
                .min_neighbor_clearance
                .map_or_else(|| "null".to_owned(), f),
            transitions,
            settle_events
        );
    }
}
