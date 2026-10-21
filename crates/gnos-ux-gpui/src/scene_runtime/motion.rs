//! The scene's motion: the shared motion driver, the system's colour
//! scheme and motion preference, and each motion step.

use super::*;

#[derive(Default)]
pub(super) struct ProductMotionDriver {
    pub(super) active: bool,
    pub(super) first_frame_pending: bool,
}

impl ProductMotionDriver {
    pub(super) fn arm(&mut self, reduced_motion: bool) -> bool {
        if self.active || reduced_motion {
            return false;
        }
        self.active = true;
        self.first_frame_pending = true;
        true
    }

    pub(super) fn stop(&mut self) {
        self.active = false;
        self.first_frame_pending = false;
    }

    pub(super) fn tick(&mut self, needs_frame: bool) -> bool {
        if !self.active {
            return false;
        }
        if self.first_frame_pending {
            self.first_frame_pending = false;
            return true;
        }
        if needs_frame {
            return true;
        }
        self.active = false;
        false
    }
}

impl SceneHost {
    pub(super) fn refresh_effective_product_preferences(&mut self) -> bool {
        let effective = resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            self.system_product_preferences,
        );
        #[cfg(feature = "r12-production-client")]
        let effective = crate::product_preferences::preserve_durable_bar_projection(
            effective,
            self.product_runtime.preferences(),
        );
        self.product_runtime.set_preferences(effective)
    }

    pub(super) fn set_system_color_scheme(
        &mut self,
        scheme: Option<ProductColorScheme>,
        cx: &mut Context<Self>,
    ) {
        self.system_product_preferences.color_scheme = scheme;
        let changed = self.refresh_effective_product_preferences();
        let effective = self.product_runtime.preferences();
        println!(
            "GNOS_UX_EFFECTIVE_PREFERENCES output={} field=color_scheme value={:?} source={:?} projection_state={:?}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            effective.color_scheme,
            effective.color_scheme_source,
            effective.projection_state,
        );
        if changed {
            cx.notify();
        }
    }

    pub(super) fn set_system_motion_preference(
        &mut self,
        preference: MotionPreference,
        cx: &mut Context<Self>,
    ) {
        let motion_changed = self.motion_preference.update_system(preference);
        self.system_product_preferences.reduced_motion = preference.as_system_observation();
        let preferences_changed = self.refresh_effective_product_preferences();
        let effective = self.effective_reduced_motion();
        println!(
            "GNOS_UX_MOTION_PREFERENCE output={} source=portal value={} effective={} cli_override={}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            preference.as_str(),
            if effective { "reduce" } else { "no_preference" },
            self.motion_preference.cli_override_enabled(),
        );
        if motion_changed && effective {
            self.motion_driver.stop();
        }
        let product_preferences = self.product_runtime.preferences();
        println!(
            "GNOS_UX_EFFECTIVE_PREFERENCES output={} field=reduced_motion value={} source={:?} projection_state={:?}",
            self.runtime_output
                .as_ref()
                .map_or("UNKNOWN", RuntimeId::as_str),
            product_preferences.reduced_motion,
            product_preferences.reduced_motion_source,
            product_preferences.projection_state,
        );
        if motion_changed || preferences_changed {
            cx.notify();
        }
    }

    pub(super) fn start_motion_driver(&mut self, cx: &mut Context<Self>) {
        if !self.product_mode || !self.motion_driver.arm(self.effective_reduced_motion()) {
            return;
        }
        let first_bar_transition_driver = self.bar_transition_driver_started.is_none()
            && self
                .bar_transition_qualification
                .as_ref()
                .is_some_and(|(plan, started)| started.elapsed() < plan.total_duration());
        if first_bar_transition_driver {
            self.bar_transition_driver_started = Some(Instant::now());
        }
        self.motion_tick_previous = None;
        if let Some(perf) = &self.perf_counters {
            perf.motion_active.store(true, Ordering::Relaxed);
            if let Ok(mut paints) = perf.motion_paints.lock() {
                paints.clear();
            }
            if let Ok(mut work) = perf.motion_tick_work_us.lock() {
                work.clear();
            }
            perf.largest_motion_tick_gap_us.store(0, Ordering::Relaxed);
            if first_bar_transition_driver {
                perf.bar_transition_ticks.store(0, Ordering::Relaxed);
                perf.bar_transition_trace_emitted
                    .store(false, Ordering::Relaxed);
                if let Ok(mut gaps) = perf.bar_transition_gaps_us.lock() {
                    gaps.clear();
                }
            }
        }
        // LIVE-02: the motion steps on fixed 4 ms deadlines (250 a second),
        // so every frame of any output up to 240 Hz shows a fresh step. The
        // former timer slept 16 ms after each step: ~60 steps a second, less
        // the step's own time. (Frame callbacks were tried: the 165 Hz output
        // then delivered one every 42 ms.) Notifications between two frames
        // coalesce, so a faster step adds no render.
        cx.spawn(async move |this, cx| {
            let mut deadline = Instant::now();
            loop {
                deadline += MOTION_STEP;
                let now = Instant::now();
                if deadline > now {
                    cx.background_executor().timer(deadline - now).await;
                } else {
                    // Behind (a long step or a busy thread): never burst.
                    deadline = now;
                }
                let Ok(continue_motion) = this.update(cx, |view, cx| {
                    let continue_motion = view.motion_frame();
                    cx.notify();
                    continue_motion
                }) else {
                    break;
                };
                if !continue_motion {
                    break;
                }
            }
        })
        .detach();
    }

    /// LIVE-02: one motion step; returns whether anything still moves.
    pub(super) fn motion_frame(&mut self) -> bool {
        if !self.motion_driver.active {
            return false;
        }
        let now = Instant::now();
        if let Some(perf) = &self.perf_counters {
            perf.timer_ticks.fetch_add(1, Ordering::Relaxed);
            if self.bar_transition_qualification.is_some() && !self.effective_reduced_motion() {
                perf.bar_transition_ticks.fetch_add(1, Ordering::Relaxed);
                if let Some(gap) = self
                    .motion_tick_previous
                    .map(|previous| now.duration_since(previous).as_micros() as u64)
                    && let Ok(mut gaps) = perf.bar_transition_gaps_us.lock()
                {
                    gaps.push(gap);
                }
            }
        }
        if let Some(previous) = self.motion_tick_previous.replace(now)
            && let Some(perf) = &self.perf_counters
        {
            let gap_us = now.duration_since(previous).as_micros() as u64;
            perf.largest_motion_tick_gap_us
                .fetch_max(gap_us, Ordering::Relaxed);
        }
        let work_started = Instant::now();
        let needs_frame = self.advance_product_motion();
        let continue_motion = self.motion_driver.tick(needs_frame);
        if let Some(perf) = &self.perf_counters
            && let Ok(mut work) = perf.motion_tick_work_us.lock()
        {
            work.push(work_started.elapsed().as_micros() as u64);
        }
        if continue_motion {
            if let Some(perf) = &self.perf_counters {
                perf.motion_notifies.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.log_frame_stats();
            self.log_bar_transition_trace();
        }
        continue_motion
    }

    pub(super) fn advance_product_motion(&mut self) -> bool {
        #[cfg(feature = "r12-production-client")]
        if self.consume_profile_edge() {
            return true;
        }
        self.refresh_product_runtime();
        // Re-constrain on every frame. The product's capability to show a
        // surface can be lost at any time - a projection goes stale, semantics
        // become unavailable - and a panel that outlives its capability would be
        // showing something the product can no longer honour.
        self.product_panel = self.product_panel.constrained_by(
            self.product_runtime.semantic().can_open_workspace(),
            self.product_runtime.semantic().can_open_appstack(),
        );
        // Every transient surface advances on every frame, before the solver
        // check: behind a short-circuit, an unsettled Dock hull would stall a
        // surface that is still sliding. BAR-01C: they also advance while
        // the rail travels (a popout retracting, toasts sliding, the Deck).
        // BAR-01D: the surfaces' springs run on the motion clock, which the
        // profile's motion speed scales; the Deck keeps its own speed.
        let motion_now = self.motion_now();
        let surfaces_moving = self.tick_bar_popout(motion_now)
            | self.tick_notification_toasts(motion_now)
            | self.tick_stack(motion_now)
            | self.tick_settings(motion_now)
            | self.tick_rail_metrics()
            | self.tick_toast_base()
            | self.relayout_active()
            | self.tick_deck();
        if !self.effective_reduced_motion()
            && let Some((plan, started)) = &self.bar_transition_qualification
        {
            if started.elapsed() < plan.total_duration() {
                return true;
            }
            if !self.bar_transition_completion_notified {
                // One final notification renders the exact target endpoint and
                // releases the source input/reservation generation.
                self.bar_transition_completion_notified = true;
                return true;
            }
        }
        // One scene driver owns every reactive surface with unsettled motion.
        // Each solver retains its own target and lineage; the driver only asks
        // whether either one still needs a render step.
        let solver_moving = !self.effective_reduced_motion()
            && (self
                .bar_transition_qualification
                .as_ref()
                .is_some_and(|(plan, started)| started.elapsed() < plan.total_duration())
                || crate::product_scene::reactive_motion_needs_frame(
                    self.product_hull.as_ref(),
                    self.product_panel.is_workspace_summary(),
                    false,
                )
                || !self
                    .appstack_motion
                    .is_settled_for(self.product_panel.is_appstack()));
        solver_moving || surfaces_moving
    }
}
