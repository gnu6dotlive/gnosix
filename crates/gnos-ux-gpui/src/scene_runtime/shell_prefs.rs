//! BAR-01D: the durable shell preferences (B03) as the scene applies them.
//!
//! - **Rail metrics.** The profile's thickness, density and title width are
//!   eased with the envelope's `Follow` and set as the current
//!   `rail_topology::RailMetrics` every frame, so a change reflows the Bar,
//!   its frame insets and its popouts continuously.
//! - **Motion speed.** The surfaces' springs run on a motion clock: real time
//!   scaled by the profile's speed. Reduced motion still snaps.
//! - **The Deck.** Its wave takes the profile's tuning (D09).

use super::*;
use gnos_ux_surface::grammar::{Follow, TemporalResponseMode};

const SETTLE_PX: f32 = 0.25;

pub(super) fn default_metrics_follow() -> [Follow; 3] {
    let d = crate::rail_topology::RailMetrics::DEFAULT;
    [
        Follow::at(d.thickness),
        Follow::at(crate::reference_bar::MEDIUM_SPACING * d.gap_scale),
        Follow::at(d.title_width),
    ]
}

impl SceneHost {
    /// The profile's shell overrides, or none.
    fn shell_overrides(&self) -> in_gnu_os_settings_read_model::BarPreferenceOverrides {
        self.product_runtime.preferences().bar_overrides.clone()
    }

    /// The metrics the profile asks for: thickness, gap and title width, in
    /// pixels.
    fn rail_metrics_target(&self) -> [f32; 3] {
        let o = self.shell_overrides();
        let d = crate::rail_topology::RailMetrics::DEFAULT;
        [
            o.thickness.map_or(d.thickness, |v| f32::from(v.get())),
            crate::reference_bar::MEDIUM_SPACING
                * o.density
                    .map_or(d.gap_scale, |v| f32::from(v.get()) / 100.0),
            o.title_max.map_or(d.title_width, |v| f32::from(v.get())),
        ]
    }

    /// Whether the metrics still move toward the profile's.
    pub(super) fn rail_metrics_moving(&self) -> bool {
        self.rail_metrics_follow
            .iter()
            .zip(self.rail_metrics_target())
            .any(|(f, t)| !f.is_settled(t, SETTLE_PX))
    }

    fn publish_rail_metrics(&self) {
        let [thickness, gap, title] = self.rail_metrics_follow.each_ref().map(|f| f.value);
        crate::rail_topology::set_metrics(crate::rail_topology::RailMetrics {
            thickness,
            gap_scale: gap / crate::reference_bar::MEDIUM_SPACING,
            title_width: title,
        });
    }

    /// Advance the rail metrics; true while they move.
    pub(super) fn tick_rail_metrics(&mut self) -> bool {
        let now = Instant::now();
        let dt = self
            .rail_metrics_last
            .map_or(1.0 / 60.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .clamp(1.0e-4, 0.1);
        self.rail_metrics_last = Some(now);
        let mode = if self.effective_reduced_motion() {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        let tempo = self.motion_tempo();
        let target = self.rail_metrics_target();
        let was_moving = self.rail_metrics_moving();
        for (follow, target) in self.rail_metrics_follow.iter_mut().zip(target) {
            follow.step_at_tempo(target, tempo, dt, mode, SETTLE_PX);
        }
        self.publish_rail_metrics();
        let moving = self.rail_metrics_moving();
        if was_moving {
            self.rail_metrics_frames += 1;
        }
        if !moving {
            self.rail_metrics_last = None;
            if was_moving {
                let m = crate::rail_topology::metrics();
                println!(
                    "GNOS_UX_BAR_METRICS state=settled thickness={:.1} gap={:.1} title={:.1} frames={}",
                    m.thickness,
                    m.gap_scale * crate::reference_bar::MEDIUM_SPACING,
                    m.title_width,
                    self.rail_metrics_frames
                );
                self.rail_metrics_frames = 0;
            }
        }
        moving
    }

    /// Before a frame is built: publish the current metrics, and wake the
    /// driver when the profile asks for new ones.
    pub(super) fn sync_shell_preferences(&mut self, cx: &mut Context<Self>) {
        self.publish_rail_metrics();
        if self.rail_metrics_moving() {
            self.start_motion_driver(cx);
        }
        #[cfg(feature = "component-lab")]
        {
            let tuning = self.shell_overrides().deck.map_or_else(
                crate::deck::wave::WaveTuning::default,
                |t| {
                    crate::deck::wave::WaveTuning::bounded(
                        f32::from(t.magnify.get()) / 100.0,
                        f32::from(t.reach.get()) / 10.0,
                        f32::from(t.tempo.get()) / 100.0,
                        f32::from(t.shoulder.get()),
                    )
                },
            );
            if self.deck_wave.tuning() != tuning {
                println!(
                    "GNOS_UX_DECK_TUNING magnify={:.2} reach={:.2} tempo={:.2}",
                    tuning.magnify(),
                    tuning.reach() / (crate::deck::CARD + crate::deck::GAP),
                    tuning.tempo()
                );
                self.deck_wave.set_tuning(tuning);
            }
        }
        // DECK-01F (B10): the durable profile owns the Deck's pins.
        #[cfg(all(feature = "component-lab", feature = "r12-production-client"))]
        self.sync_deck_pins(cx);
    }

    /// BAR-01H (B07): how far the toast drawer must stand off its edge: on
    /// the Deck, when the Bar is horizontal (they share an edge) and their
    /// horizontal extents meet; else 0.
    #[cfg(feature = "component-lab")]
    pub(super) fn toast_base_target(
        &self,
        side: crate::rail_topology::RailSide,
        viewport: (f32, f32),
    ) -> f32 {
        // SET-03: the operator's gap.
        let gap = crate::surface_prefs::advanced("toast_deck_gap_px") as f32;
        let border = crate::reference_bar::BORDER_THICKNESS;
        if !side.is_horizontal() {
            return 0.0;
        }
        let deck_left = self.deck_rest_left;
        let deck_right = deck_left + crate::deck::deck_width(&self.deck_cards);
        let panel_right = viewport.0 - border;
        let panel_left = panel_right - crate::notification_toasts::PANEL_WIDTH;
        if deck_right <= panel_left || deck_left >= panel_right {
            return 0.0;
        }
        // The Deck stands 16 px off its edge (TOPO-01C2b).
        let deck_reach = 16.0 + crate::deck::HEIGHT;
        deck_reach - border + gap
    }

    /// Advance the toast drawer's base; true while it moves.
    pub(super) fn tick_toast_base(&mut self) -> bool {
        let target = self.toast_base_target_value;
        if self.toast_base.is_settled(target, SETTLE_PX) {
            self.toast_base_last = None;
            if self.toast_base.value != target {
                self.toast_base = Follow::at(target);
            }
            return false;
        }
        let now = Instant::now();
        let dt = self
            .toast_base_last
            .map_or(1.0 / 60.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .clamp(1.0e-4, 0.1);
        self.toast_base_last = Some(now);
        let mode = if self.effective_reduced_motion() {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        let tempo = self.motion_tempo();
        self.toast_base
            .step_at_tempo(target, tempo, dt, mode, SETTLE_PX);
        true
    }

    /// A settings reflow of the Bar is under way (it needs frames).
    pub(super) fn relayout_active(&self) -> bool {
        #[cfg(feature = "component-lab")]
        {
            self.bar_relayout.is_some()
        }
        #[cfg(not(feature = "component-lab"))]
        {
            false
        }
    }

    /// The profile's motion speed, as a factor.
    pub(super) fn motion_tempo(&self) -> f32 {
        self.shell_overrides()
            .spring_tempo
            .map_or(1.0, |v| f32::from(v.get()) / 100.0)
    }

    /// The profile's travel speed, as a factor (the R12 edge edit).
    #[cfg(feature = "r12-production-client")]
    pub(super) fn travel_tempo(&self) -> f32 {
        self.shell_overrides()
            .travel_tempo
            .map_or(1.0, |v| f32::from(v.get()) / 100.0)
    }

    /// The motion clock: real time scaled by the motion speed; monotonic.
    pub(super) fn motion_now(&mut self) -> Instant {
        let real = Instant::now();
        let tempo = self.motion_tempo();
        if let Some(last) = self.motion_real_last {
            self.motion_virtual += real.saturating_duration_since(last).mul_f32(tempo);
        } else {
            self.motion_virtual = self.motion_virtual.max(real);
        }
        self.motion_real_last = Some(real);
        self.motion_virtual
    }
}
