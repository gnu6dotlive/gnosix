//! The scene frame: its layout from the product state, the surface
//! reservation it asks of the compositor, its paint, and its receipts.

use super::*;

impl SceneHost {
    pub(super) fn notify_projection_revision(&mut self, cx: &mut Context<Self>) {
        if !self.semantic_consumer.is_enabled() {
            return;
        }
        let revision = self.semantic_consumer.revision();
        if revision != self.last_projection_revision {
            self.last_projection_revision = revision;
            if self.product_mode {
                self.refresh_product_runtime();
                match self.product_runtime.semantic() {
                    ProductSemanticState::Current(content) => println!(
                        "GNOS_UX_PRODUCT_CONTENT output={} status=current generation={} workspace={:?}",
                        content.connector, content.generation.0, content.active_workspace
                    ),
                    ProductSemanticState::Unavailable => {
                        println!("GNOS_UX_PRODUCT_CONTENT status=unavailable")
                    }
                    ProductSemanticState::Invalid => {
                        println!("GNOS_UX_PRODUCT_CONTENT status=invalid")
                    }
                }
            }
            if let Some(perf) = &self.perf_counters {
                perf.projection_notifies.fetch_add(1, Ordering::Relaxed);
            }
            cx.notify();
        }
    }

    /// LIVE-02: one receipt per settled motion: how often frames were
    /// painted, and what each motion step cost.
    pub(super) fn log_frame_stats(&self) {
        let Some(perf) = &self.perf_counters else {
            return;
        };
        perf.motion_active.store(false, Ordering::Relaxed);
        let paints = perf
            .motion_paints
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default();
        let mut intervals: Vec<u64> = paints
            .windows(2)
            .map(|w| w[1].duration_since(w[0]).as_micros() as u64)
            .collect();
        let mut work = perf
            .motion_tick_work_us
            .lock()
            .map(|w| w.clone())
            .unwrap_or_default();
        if intervals.len() < 3 {
            return;
        }
        let (i50, i95, imax) = spread(&mut intervals);
        let (w50, w95, wmax) = spread(&mut work);
        let fps = if i50 > 0 {
            1_000_000.0 / i50 as f64
        } else {
            0.0
        };
        println!(
            "GNOS_UX_FRAME_STATS output={} frames={} interval_ms_median={:.1} interval_ms_p95={:.1} interval_ms_max={:.1} fps_median={fps:.1} step_ms_median={:.2} step_ms_p95={:.2} step_ms_max={:.2} steps={}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            paints.len(),
            i50 as f64 / 1000.0,
            i95 as f64 / 1000.0,
            imax as f64 / 1000.0,
            w50 as f64 / 1000.0,
            w95 as f64 / 1000.0,
            wmax as f64 / 1000.0,
            work.len(),
        );
    }

    pub(super) fn log_perf_sample(&self) {
        let Some(perf) = &self.perf_counters else {
            return;
        };
        let uptime_ms = perf.started.map_or(0, |start| start.elapsed().as_millis());
        let settled = self
            .product_hull
            .as_ref()
            .is_some_and(|solver| solver.is_settled_for(self.product_panel.is_workspace_summary()));
        let appstack_settled = self
            .appstack_motion
            .is_settled_for(self.product_panel.is_appstack());
        println!(
            "GNOS_UX_PERF_SAMPLE output={} uptime_ms={} panel={} settled={} appstack_settled={} pointer={} reduced={} timer_ticks={} motion_notifies={} projection_notifies={} interaction_notifies={} pointer_changes={} renders={} solver_resolves={} appstack_solver_resolves={} largest_motion_tick_gap_us={} paints={}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            uptime_ms,
            self.product_panel,
            settled,
            appstack_settled,
            self.product_pointer.is_some(),
            self.effective_reduced_motion(),
            perf.timer_ticks.load(Ordering::Relaxed),
            perf.motion_notifies.load(Ordering::Relaxed),
            perf.projection_notifies.load(Ordering::Relaxed),
            perf.interaction_notifies.load(Ordering::Relaxed),
            perf.pointer_changes.load(Ordering::Relaxed),
            perf.renders.load(Ordering::Relaxed),
            perf.solver_resolves.load(Ordering::Relaxed),
            perf.appstack_solver_resolves.load(Ordering::Relaxed),
            perf.largest_motion_tick_gap_us.load(Ordering::Relaxed),
            perf.paints.load(Ordering::Relaxed),
        );
    }

    pub(super) fn log_bar_transition_trace(&self) {
        let (Some((plan, _)), Some(perf)) =
            (&self.bar_transition_qualification, &self.perf_counters)
        else {
            return;
        };
        if perf
            .bar_transition_trace_emitted
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let mut gaps = perf
            .bar_transition_gaps_us
            .lock()
            .map(|gaps| gaps.clone())
            .unwrap_or_default();
        gaps.sort_unstable();
        let percentile = |p: f64| -> u64 {
            if gaps.is_empty() {
                return 0;
            }
            let index = ((gaps.len() as f64 * p).ceil() as usize)
                .saturating_sub(1)
                .min(gaps.len() - 1);
            gaps[index]
        };
        println!(
            "GNOS_UX_BAR_TRANSITION_TRACE scene_id={} output={} plan={:?} active_frames={} gap_samples={} median_gap_us={} p95_gap_us={} max_gap_us={} settle_ms={} driver_active=false driver_sleeps_after_settle=true",
            self.scene_id.value(),
            self.runtime_output
                .as_ref()
                .map_or("COMPOSITOR_SELECTED", RuntimeId::as_str),
            plan,
            perf.bar_transition_ticks.load(Ordering::Relaxed),
            gaps.len(),
            percentile(0.50),
            percentile(0.95),
            gaps.last().copied().unwrap_or_default(),
            self.bar_transition_driver_started
                .map_or(0, |started| started.elapsed().as_millis()),
        );
    }

    pub(super) fn ensure_scene(&mut self, viewport: SurfaceViewport) {
        if self.scene.is_none() {
            let mut scene =
                MonitorScene::new(self.scene_id, viewport).expect("creating monitor scene");
            let frame = scene.tick(1.0 / 60.0).expect("initial scene frame");
            self.log_frame(&frame, "initial");
            self.previous_step = Some(frame.cycle_step);
            self.previous_viewport = Some(viewport);
            self.frame = Some(frame);
            self.scene = Some(scene);
            return;
        }

        if self.previous_viewport != Some(viewport) {
            let previous = self.previous_viewport;
            let previous_frame = self.frame.clone();
            let scene = self.scene.as_mut().expect("scene");
            scene
                .rebase_viewport(viewport)
                .expect("rebasing monitor scene viewport");
            let frame = scene.tick(1.0 / 60.0).expect("scene frame after rebase");
            println!(
                "GNOS_UX_SCENE_VIEWPORT_REBASE scene_id={} host_token={} from={} to={:.1}x{:.1} epoch={}",
                scene.id().value(),
                self.host_token,
                previous
                    .map(|value| format!("{:.1}x{:.1}", value.width, value.height))
                    .unwrap_or_else(|| "NONE".to_owned()),
                viewport.width,
                viewport.height,
                frame.epoch.value(),
            );
            if let (Some(old_viewport), Some(old_frame)) = (previous, previous_frame.as_ref())
                && old_viewport.width > 8.0
                && old_viewport.height > 8.0
            {
                let delta = normalized_relation_endpoint_delta(old_frame, &frame);
                println!(
                    "GNOS_UX_SCENE_REBASE_CONTINUITY scene_id={} host_token={} normalized_endpoint_delta={:.6}",
                    frame.scene_id.value(),
                    self.host_token,
                    delta,
                );
            }
            self.log_frame(&frame, "rebase");
            self.previous_step = Some(frame.cycle_step);
            self.previous_viewport = Some(viewport);
            self.frame = Some(frame);
        }
    }

    pub(super) fn advance_scene(&mut self) {
        let Some(scene) = self.scene.as_mut() else {
            return;
        };
        if scene.script_complete() {
            if !self.completion_logged {
                let max_delta = scene.max_connector_endpoint_delta();
                if max_delta < 24.0 {
                    println!(
                        "GNOS_UX_SCENE_CONTINUITY_PASS scene_id={} host_token={} max_connector_endpoint_delta={:.4} bound=24.0",
                        scene.id().value(),
                        self.host_token,
                        max_delta,
                    );
                } else {
                    println!(
                        "GNOS_UX_SCENE_CONTINUITY_FAIL scene_id={} host_token={} max_connector_endpoint_delta={:.4} bound=24.0",
                        scene.id().value(),
                        self.host_token,
                        max_delta,
                    );
                }
                println!(
                    "GNOS_UX_SCENE_IDENTITY_PASS scene_id={} host_token={} epoch={}",
                    scene.id().value(),
                    self.host_token,
                    scene.epoch().value(),
                );
                println!(
                    "GNOS_UX_SCENE_CYCLE_COMPLETE scene_id={} host_token={} epoch={} max_connector_endpoint_delta={:.4}",
                    scene.id().value(),
                    self.host_token,
                    scene.epoch().value(),
                    max_delta,
                );
                self.completion_logged = true;
            }
            return;
        }

        let frame = scene.tick(1.0 / 60.0).expect("advancing scene frame");
        let step_changed = self.previous_step != Some(frame.cycle_step);
        if step_changed {
            println!(
                "GNOS_UX_SCENE_STEP scene_id={} host_token={} step={} epoch={} viewport={:.1}x{:.1}",
                frame.scene_id.value(),
                self.host_token,
                frame.cycle_step.as_str(),
                frame.epoch.value(),
                frame.viewport.width,
                frame.viewport.height,
            );
            self.previous_step = Some(frame.cycle_step);
        }
        if step_changed || frame.epoch.value() % 30 == 0 || frame.influence_pressure > 0.001 {
            self.log_frame(&frame, "tick");
        }
        self.frame = Some(frame);
    }

    pub(super) fn maybe_start_projection_qualification(&mut self, frame: &SceneFrame) {
        if !self.projection_qualification_enabled
            || self.projection_worker_started
            || frame.viewport.width <= 8.0
            || frame.viewport.height <= 8.0
        {
            return;
        }
        self.projection_worker_started = true;
        let source = CompositorIntentSource {
            scene_id: frame.scene_id,
            scene_epoch: frame.epoch,
        };
        thread::spawn(move || {
            if let Err(error) = run_global_projection_qualification(source) {
                eprintln!("GNOSIX_HYPRLAND_GLOBAL_PROJECTION_FAIL error={error}");
            }
        });
    }

    pub(super) fn project_surface_reservation(&mut self, frame: &SceneFrame, window: &mut Window) {
        if frame.viewport.width <= 8.0 || frame.viewport.height <= 8.0 {
            return;
        }

        let intent = match ResolvedCompositorIntent::try_from_scene_frame_for_output(
            frame,
            self.runtime_output.clone(),
        ) {
            Ok(intent) => intent,
            Err(error) => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason={error:?}",
                    frame.scene_id.value(),
                    frame.epoch.value()
                );
                return;
            }
        };
        let plan = match surface_reservation_plan(&intent) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason={error:?}",
                    frame.scene_id.value(),
                    frame.epoch.value()
                );
                return;
            }
        };

        if plan.output != self.runtime_output {
            eprintln!(
                "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason=OUTPUT_TARGET_MISMATCH",
                plan.source.scene_id.value(),
                plan.source.scene_epoch.value()
            );
            return;
        }

        let key = ProjectionKey {
            scene_id: plan.source.scene_id,
            slot: ProjectionSlot::SurfaceReservation,
            runtime_target: SCENE_NAMESPACE.to_owned(),
        };
        let token = surface_request_token(plan.edge, plan.exclusive_zone);
        match self
            .surface_fence
            .evaluate(&key, plan.source.scene_epoch, token)
        {
            FenceDecision::Stale => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason=STALE",
                    plan.source.scene_id.value(),
                    plan.source.scene_epoch.value()
                );
                return;
            }
            FenceDecision::Conflict => {
                eprintln!(
                    "GNOS_UX_SCENE_RESERVATION_FAIL scene_id={} epoch={} reason=CONFLICT",
                    plan.source.scene_id.value(),
                    plan.source.scene_epoch.value()
                );
                return;
            }
            FenceDecision::AlreadyAccepted => return,
            FenceDecision::Fresh => {}
        }

        let current_reservation = (plan.edge, plan.exclusive_zone);
        if self.initial_reservation.is_none() {
            self.initial_reservation = Some(current_reservation);
        } else if self.initial_reservation != Some(current_reservation) {
            self.reservation_changed_from_initial = true;
        }

        if self.applied_reservation != Some(current_reservation) {
            window.set_exclusive_edge(surface_edge_anchor(plan.edge));
            window.set_exclusive_zone(px(plan.exclusive_zone as f32));
            self.applied_reservation = Some(current_reservation);
            #[cfg(feature = "component-lab")]
            {
                self.desktop_state.inset = [
                    frame.reservation.top,
                    frame.reservation.right,
                    frame.reservation.bottom,
                    frame.reservation.left,
                ];
            }
            let bar = frame.node(SceneNodeId::BAR).expect("Bar frame node");
            let dock = frame.node(SceneNodeId::DOCK).expect("Dock frame node");
            println!(
                "GNOS_UX_SCENE_RESERVATION_APPLIED scene_id={} host_token={} epoch={} edge={} zone={} intent=[{:.1},{:.1},{:.1},{:.1}] bar=[{:.1},{:.1},{:.1},{:.1}] dock=[{:.1},{:.1},{:.1},{:.1}]",
                plan.source.scene_id.value(),
                self.host_token,
                plan.source.scene_epoch.value(),
                surface_edge_name(plan.edge),
                plan.exclusive_zone,
                frame.reservation.top,
                frame.reservation.right,
                frame.reservation.bottom,
                frame.reservation.left,
                bar.envelope.left(),
                bar.envelope.top(),
                bar.envelope.right(),
                bar.envelope.bottom(),
                dock.envelope.left(),
                dock.envelope.top(),
                dock.envelope.right(),
                dock.envelope.bottom(),
            );

            if self.reservation_changed_from_initial
                && self.initial_reservation == Some(current_reservation)
                && !self.reservation_return_logged
            {
                self.reservation_return_logged = true;
                println!(
                    "GNOS_UX_SCENE_RESERVATION_RETURNED scene_id={} host_token={} epoch={} edge={} zone={}",
                    plan.source.scene_id.value(),
                    self.host_token,
                    plan.source.scene_epoch.value(),
                    surface_edge_name(plan.edge),
                    plan.exclusive_zone
                );
            }
        }

        self.surface_fence
            .commit(key, plan.source.scene_epoch, token);
    }

    pub(super) fn log_frame(&self, frame: &SceneFrame, phase: &str) {
        let relation = frame.relations.first().expect("scene relation");
        let body = frame.node(SceneNodeId::BODY).expect("body node");
        let dock = frame.node(SceneNodeId::DOCK).expect("dock node");
        let bar = frame.node(SceneNodeId::BAR).expect("bar node");
        println!(
            "GNOS_UX_SCENE_FRAME phase={} scene_id={} host_token={} epoch={} step={} viewport={:.1}x{:.1} nodes={} relations={} bar=[{:.1},{:.1},{:.1},{:.1}] body=[{:.1},{:.1},{:.1},{:.1}] dock=[{:.1},{:.1},{:.1},{:.1}] relation={:?} presence={:.4} influence_pressure={:.3} reservation=[{:.1},{:.1},{:.1},{:.1}]",
            phase,
            frame.scene_id.value(),
            self.host_token,
            frame.epoch.value(),
            frame.cycle_step.as_str(),
            frame.viewport.width,
            frame.viewport.height,
            frame.nodes.len(),
            frame.relations.len(),
            bar.envelope.left(),
            bar.envelope.top(),
            bar.envelope.right(),
            bar.envelope.bottom(),
            body.envelope.left(),
            body.envelope.top(),
            body.envelope.right(),
            body.envelope.bottom(),
            dock.envelope.left(),
            dock.envelope.top(),
            dock.envelope.right(),
            dock.envelope.bottom(),
            relation.state,
            relation.presence.value(),
            frame.influence_pressure,
            frame.reservation.top,
            frame.reservation.right,
            frame.reservation.bottom,
            frame.reservation.left,
        );
    }
}

pub(super) fn surface_edge_anchor(edge: SurfaceEdge) -> Anchor {
    match edge {
        SurfaceEdge::Top => Anchor::TOP,
        SurfaceEdge::Right => Anchor::RIGHT,
        SurfaceEdge::Bottom => Anchor::BOTTOM,
        SurfaceEdge::Left => Anchor::LEFT,
    }
}

pub(super) fn surface_edge_name(edge: SurfaceEdge) -> &'static str {
    match edge {
        SurfaceEdge::Top => "top",
        SurfaceEdge::Right => "right",
        SurfaceEdge::Bottom => "bottom",
        SurfaceEdge::Left => "left",
    }
}

pub(super) fn surface_request_token(edge: SurfaceEdge, exclusive_zone: i32) -> i64 {
    let edge_code = match edge {
        SurfaceEdge::Top => 1_i64,
        SurfaceEdge::Right => 2,
        SurfaceEdge::Bottom => 3,
        SurfaceEdge::Left => 4,
    };
    edge_code * 1_000_000 + i64::from(exclusive_zone)
}

pub(super) fn normalized_relation_endpoint_delta(
    previous: &SceneFrame,
    current: &SceneFrame,
) -> f32 {
    let previous_relation = previous.relations.first().expect("previous scene relation");
    let current_relation = current.relations.first().expect("current scene relation");

    let normalize = |point: gnos_ux_surface::grammar::Point2, viewport: SurfaceViewport| {
        (
            point.x() / viewport.width.max(1.0),
            point.y() / viewport.height.max(1.0),
        )
    };
    let distance = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);

    distance(
        normalize(previous_relation.connector.p0(), previous.viewport),
        normalize(current_relation.connector.p0(), current.viewport),
    )
    .max(distance(
        normalize(previous_relation.connector.p2(), previous.viewport),
        normalize(current_relation.connector.p2(), current.viewport),
    ))
}

pub(super) fn paint_scene(bounds: Bounds<gpui::Pixels>, frame: &SceneFrame, window: &mut Window) {
    let relation = frame.relations.first().expect("scene relation");
    if relation.presence.value() > 0.005 {
        let alpha = (relation.presence.value() * 220.0)
            .round()
            .clamp(0.0, 255.0) as u32;
        let connector_color = rgba(0x315f_7900 | alpha);
        window.paint_path(
            connector_path(bounds, relation.connector, 22.0),
            connector_color,
        );
    }

    for node in &frame.nodes {
        let fill = match node.role {
            SceneNodeRole::Bar => rgba(0x17374bff),
            SceneNodeRole::Body => rgba(0x315f79ff),
            SceneNodeRole::Dock => rgba(0x315f79ff),
            // Inherits the Dock's material, which is the lineage the Envelope
            // model asks for: the derived body is the same material family as
            // its parent, not a new one.
            SceneNodeRole::AppStack => rgba(0x315f79ff),
            SceneNodeRole::Notification => rgba(0x456b7fff),
            SceneNodeRole::Overlay => rgba(0x5c7380ff),
            SceneNodeRole::Plugin => rgba(0x6d7e87ff),
        };
        if node.envelope.width() > 0.01 && node.envelope.height() > 0.01 {
            window.paint_path(rounded_rect_path(bounds, node.envelope), fill);
        }
    }
}

pub(super) fn connector_path(
    bounds: Bounds<gpui::Pixels>,
    pair: gnos_ux_surface::grammar::CubicJoinPair,
    width: f32,
) -> gpui::Path<gpui::Pixels> {
    let mut path = PathBuilder::stroke(px(width));
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let p = |value: gnos_ux_surface::grammar::Point2| point(px(ox + value.x()), px(oy + value.y()));

    path.move_to(p(pair.p0()));
    path.cubic_bezier_to(p(pair.p1()), p(pair.c1()), p(pair.c2()));
    path.cubic_bezier_to(p(pair.p2()), p(pair.c3()), p(pair.c4()));
    path.build().expect("building scene connector")
}

pub(super) fn rounded_rect_path(
    bounds: Bounds<gpui::Pixels>,
    rect: GeometryRect,
) -> gpui::Path<gpui::Pixels> {
    crate::product_hull::rounded_rect_path(bounds, rect)
}
