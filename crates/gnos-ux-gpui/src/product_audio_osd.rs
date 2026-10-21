//! Observed-audio OSD presentation.
//!
//! AUDIO-01 Phase 5 adds presentation only. This module deliberately contains
//! no PipeWire client, no `AudioIntent`, and no endpoint identity. It cannot
//! mutate anything, because it holds nothing that could: it receives accepted
//! observed snapshots and renders what they say.
//!
//! The audio control loop is unchanged and remains:
//!
//! ```text
//! intent -> revalidation -> PipeWire mutation -> PipeWire observation
//!        -> AudioSession -> ProductRuntime
//! ```
//!
//! Phase 5 only adds the last edge of a presentation path:
//!
//! ```text
//! accepted observed transition -> transient presentation state -> GPUI surface
//! ```
//!
//! Two consequences are load-bearing:
//!
//! - The OSD is a consumer of observed truth, never a consequence of the Bar
//!   button. An external/native change observed through the same qualified
//!   PipeWire substrate presents exactly like a Gnosix-issued one, so a future
//!   keybinding or any other typed surface needs no OSD work at all.
//! - Durable audio truth and ephemeral OSD lifetime stay separate concepts. The
//!   dismissal timer can only make the surface disappear; it can never write
//!   product audio state.

use crate::product_design::{
    ProductDesignContext, ProductMotion, ProductPalette, ProductRadius, ProductSpacing,
    ProductTextStyle, panel, text,
};
use gnos_audio::{AudioEndpointId, AudioEndpointKind, AudioProviderState, AudioSnapshot};
use gpui::{AppContext, Context, Entity, div, prelude::*, px};
use std::time::{Duration, Instant};

/// How long the newest observed state stays presented without a further
/// accepted change.
///
/// This is presentation lifetime, not audio state. It is intentionally not
/// reduced by the reduced-motion preference: reduced motion changes how the
/// OSD appears and disappears, never how long the truth it reports is current.
pub const AUDIO_OSD_DWELL: Duration = Duration::from_millis(1600);

/// The observed output facts a presentation may report.
///
/// This is a copy of already-accepted observed truth, retained only for as long
/// as the surface needs it to render. It is not an audio authority and is not
/// consulted by any mutation path.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioOsdTruth {
    /// The exact observed endpoint, for receipts and identity visibility.
    pub endpoint: AudioEndpointId,
    /// Observed display label.
    pub label: String,
    /// Observed user-facing level on WirePlumber's cubic mixer scale.
    ///
    /// `None` means the endpoint published no readable volume. It is never
    /// silently rendered as zero.
    pub level: Option<f32>,
    /// Observed mute state.
    pub muted: bool,
    /// The observation generation this truth came from.
    pub generation: u64,
}

impl AudioOsdTruth {
    /// Observed level as a whole percentage, or `None` when unobserved.
    pub fn percent(&self) -> Option<i32> {
        self.level
            .map(|level| (level * 100.0).round().clamp(0.0, 9999.0) as i32)
    }

    /// The exact endpoint whose observed state this truth records.
    pub const fn endpoint_id(&self) -> AudioEndpointId {
        self.endpoint
    }
}

/// Which presentable field moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioOsdCause {
    Level,
    Mute,
    LevelAndMute,
}

impl AudioOsdCause {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Level => "level",
            Self::Mute => "mute",
            Self::LevelAndMute => "level_and_mute",
        }
    }
}

/// One semantic transition worth presenting.
///
/// Produced only from the difference between two accepted observations. A
/// snapshot that repeats, or that differs only in fields the OSD does not
/// report, produces nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioOsdEvent {
    pub truth: AudioOsdTruth,
    pub cause: AudioOsdCause,
}

/// Extracts the observed, presentable output facts from a snapshot.
///
/// `None` means "no presentable truth right now": provider unavailable, the
/// effective output role unresolved, or the resolved endpoint gone or no longer
/// an output. Every one of those is a reason to present nothing, never a reason
/// to invent a value.
fn observed_output(snapshot: &AudioSnapshot) -> Option<AudioOsdTruth> {
    if snapshot.provider != AudioProviderState::Available {
        return None;
    }
    let id = snapshot.defaults.output.effective.resolved?;
    let endpoint = snapshot.endpoints.get(&id)?;
    if endpoint.kind != AudioEndpointKind::Output {
        return None;
    }
    Some(AudioOsdTruth {
        endpoint: id,
        label: endpoint.display_name.clone(),
        level: endpoint.volume.cubic_user_level(),
        muted: endpoint.muted,
        generation: snapshot.generation,
    })
}

/// The semantic delta between two accepted observations.
///
/// Deliberately compares only the two fields Phase 5 presents. A default-role
/// change, a route metadata refresh, or a device description edit that leaves
/// volume and mute alone produces no event.
pub fn audio_osd_delta(previous: &AudioSnapshot, current: &AudioSnapshot) -> Option<AudioOsdEvent> {
    let before = observed_output(previous)?;
    let after = observed_output(current)?;
    let cause = match (after.level != before.level, after.muted != before.muted) {
        (false, false) => return None,
        (true, false) => AudioOsdCause::Level,
        (false, true) => AudioOsdCause::Mute,
        (true, true) => AudioOsdCause::LevelAndMute,
    };
    Some(AudioOsdEvent {
        cause,
        truth: after,
    })
}

/// Deterministic output ownership for one OSD event.
///
/// The OSD is a session-level presentation, so exactly one output may host it.
/// Placement is resolved from facts this product already owns: the reconciled
/// output order, and an optional non-authoritative presentation context
/// contributed by an interaction that already happened.
///
/// The resolved value is a placement hint. It is not endpoint identity, is not
/// part of any intent, affects no mutation, and confers no capability.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioOsdPlacement {
    /// Session outputs in reconciled order; the first is the primary.
    outputs: Vec<String>,
}

impl AudioOsdPlacement {
    pub fn new(outputs: Vec<String>) -> Self {
        Self { outputs }
    }

    /// The deterministic primary output for this session.
    pub fn primary(&self) -> Option<&str> {
        self.outputs.first().map(String::as_str)
    }

    /// Resolves the single output that may present this OSD.
    ///
    /// A usable interaction context wins, because the user acted there. Anything
    /// else falls back to the reconciled primary. An unresolvable placement
    /// fails closed: the caller presents nothing rather than duplicating a
    /// global audio event across outputs.
    pub fn resolve(&self, context: Option<&str>) -> Option<String> {
        if self.outputs.is_empty() {
            return None;
        }
        if let Some(context) = context
            && self.outputs.iter().any(|output| output == context)
        {
            return Some(context.to_owned());
        }
        self.primary().map(str::to_owned)
    }
}

/// Ephemeral presentation phase.
///
/// Distinct from both audio truth and wall-clock intent: a phase never decides
/// whether a level is current, and an animation finishing never decides
/// whether the OSD is correct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioOsdPhase {
    Hidden,
    Appearing,
    Shown,
    Exiting,
}

impl AudioOsdPhase {
    /// A phase still being painted, and therefore still needs a frame.
    pub const fn is_transitional(self) -> bool {
        matches!(self, Self::Appearing | Self::Exiting)
    }
}

/// The transient OSD lifetime and coalescing state machine.
///
/// Pure by construction: no GPUI handle, no provider client, no timer of its
/// own. Callers drive it with an explicit `Instant` and explicit durations,
/// which is what makes both the source tests and the headless live
/// qualification able to exercise production behaviour without a display.
#[derive(Clone, Debug)]
pub struct AudioOsdPresentationState {
    /// Last accepted observation, retained only to compute the next delta.
    ///
    /// This is a transition cursor, not a second truth: it is never projected
    /// into product state and never read by a mutation path.
    cursor: Option<AudioSnapshot>,
    placement: AudioOsdPlacement,
    /// One-shot, non-authoritative presentation context from an interaction.
    context: Option<String>,
    phase: AudioOsdPhase,
    truth: Option<AudioOsdTruth>,
    owner: Option<String>,
    transition_started_at: Option<Instant>,
    dismiss_at: Option<Instant>,
    /// Bumped by every accepted transition. A timer armed for an older
    /// sequence is stale and must not act, which is how coalescing extends the
    /// dismissal without cancelling a running timer.
    sequence: u64,
    dwell: Duration,
    appear: Duration,
    exit: Duration,
}

impl AudioOsdPresentationState {
    pub fn new(
        placement: AudioOsdPlacement,
        dwell: Duration,
        appear: Duration,
        exit: Duration,
    ) -> Self {
        Self {
            cursor: None,
            placement,
            context: None,
            phase: AudioOsdPhase::Hidden,
            truth: None,
            owner: None,
            transition_started_at: None,
            dismiss_at: None,
            sequence: 0,
            dwell,
            appear,
            exit,
        }
    }

    pub fn phase(&self) -> AudioOsdPhase {
        self.phase
    }

    /// Whether the OSD currently claims audio state as current.
    ///
    /// True from the moment an accepted event presents until its dwell window
    /// elapses. A retiring surface is deliberately *not* visible here: during
    /// the exit transition it is painted but no longer authoritative, which is
    /// what keeps a fading OSD from claiming to describe the current level.
    pub fn visible(&self) -> bool {
        matches!(self.phase, AudioOsdPhase::Appearing | AudioOsdPhase::Shown)
    }

    /// Whether a surface is still being painted, including an exit transition.
    ///
    /// Distinct from [`Self::visible`]: this is "is there pixels on screen",
    /// which is a rendering question and says nothing about currency.
    pub fn painted(&self) -> bool {
        matches!(
            self.phase,
            AudioOsdPhase::Appearing | AudioOsdPhase::Shown | AudioOsdPhase::Exiting
        )
    }

    pub fn dwell(&self) -> Duration {
        self.dwell
    }

    pub fn exit_duration(&self) -> Duration {
        self.exit
    }

    pub fn truth(&self) -> Option<&AudioOsdTruth> {
        self.truth.as_ref()
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn dismiss_at(&self) -> Option<Instant> {
        self.dismiss_at
    }

    /// The one output allowed to present, resolved when the event was accepted.
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    pub fn placement(&self) -> &AudioOsdPlacement {
        &self.placement
    }

    pub fn set_placement(&mut self, placement: AudioOsdPlacement) {
        self.placement = placement;
    }

    /// Records which output an already-issued interaction happened on.
    ///
    /// Presentation context only. It is consumed by the next accepted event and
    /// is never used to select an endpoint, form an intent, or authorise a
    /// mutation.
    pub fn note_interaction_context(&mut self, output: Option<&str>) {
        self.context = output.map(str::to_owned);
    }

    /// Consumes one accepted observation.
    ///
    /// Returns the semantic event when the observation changed a presentable
    /// field, and `None` for a baseline, an identical replay, an unrelated
    /// refresh, or a loss of presentable truth.
    pub fn observe(&mut self, snapshot: &AudioSnapshot, now: Instant) -> Option<AudioOsdEvent> {
        if observed_output(snapshot).is_none() {
            // Provider unavailable, role unresolved, or endpoint gone. Retire any
            // presented state immediately: a stale OSD must never outlive the
            // truth it reports. The next presentable observation re-establishes
            // a baseline instead of presenting a change.
            self.invalidate();
            self.cursor = None;
            return None;
        }
        match self.cursor.replace(snapshot.clone()) {
            // A first snapshot establishes the baseline. Enumeration is not a
            // change, so it must not flash an OSD.
            None => None,
            Some(previous) => {
                let event = audio_osd_delta(&previous, snapshot)?;
                self.present(&event, now);
                Some(event.clone())
            }
        }
    }

    /// Applies a semantic event. Newest accepted truth replaces what was
    /// presented and the dismissal window restarts, so a run of changes updates
    /// one surface instead of stacking several.
    fn present(&mut self, event: &AudioOsdEvent, now: Instant) {
        self.truth = Some(event.truth.clone());
        // One-shot: the next event consumes the context so a later, purely
        // external change falls back to the deterministic primary.
        self.owner = self.placement.resolve(self.context.as_deref());
        self.context = None;
        self.transition_started_at = Some(now);
        self.dismiss_at = Some(now + self.dwell);
        self.sequence = self.sequence.wrapping_add(1);
        self.phase = if self.appear.is_zero() {
            AudioOsdPhase::Shown
        } else {
            AudioOsdPhase::Appearing
        };
    }

    /// Hides the presentation once its dismissal window elapsed.
    ///
    /// Presentation-only: nothing here touches audio state. Returns whether
    /// this call changed the presentation.
    pub fn expire(&mut self, now: Instant) -> bool {
        if !self.visible() {
            return false;
        }
        let Some(dismiss_at) = self.dismiss_at else {
            return false;
        };
        if now < dismiss_at {
            return false;
        }
        self.transition_started_at = Some(now);
        self.sequence = self.sequence.wrapping_add(1);
        self.phase = if self.exit.is_zero() {
            self.clear_truth();
            AudioOsdPhase::Hidden
        } else {
            AudioOsdPhase::Exiting
        };
        true
    }

    /// Advances an in-flight appear/exit transition. Visual progress only.
    pub fn settle(&mut self, now: Instant) -> bool {
        let (started, phase) = match self.phase {
            AudioOsdPhase::Appearing => (self.transition_started_at, self.appear),
            AudioOsdPhase::Exiting => (self.transition_started_at, self.exit),
            _ => return false,
        };
        let Some(started) = started else {
            return false;
        };
        if phase.is_zero() || now < started + phase {
            return false;
        }
        if self.phase == AudioOsdPhase::Exiting {
            self.clear_truth();
        }
        self.phase = match self.phase {
            AudioOsdPhase::Appearing => AudioOsdPhase::Shown,
            _ => AudioOsdPhase::Hidden,
        };
        true
    }

    /// Retires presented state because the observation is no longer presentable.
    fn invalidate(&mut self) {
        if !self.painted() && self.truth.is_none() {
            return;
        }
        self.phase = AudioOsdPhase::Hidden;
        self.clear_truth();
        self.transition_started_at = None;
        self.dismiss_at = None;
        self.owner = None;
        self.sequence = self.sequence.wrapping_add(1);
    }

    fn clear_truth(&mut self) {
        self.truth = None;
        self.owner = None;
        self.dismiss_at = None;
    }
}

/// Process-global holder for the observed audio OSD.
///
/// Presentation only. It owns no provider connection, constructs no intent, and
/// has no mutation path; it exists to give the transient lifetime one owner
/// regardless of how many outputs the process paints, and to arm the explicit
/// timers that own appear, dwell, and exit.
pub struct AudioOsd {
    state: AudioOsdPresentationState,
}

#[derive(Clone)]
struct AudioOsdGlobal(Entity<AudioOsd>);
impl gpui::Global for AudioOsdGlobal {}

/// Which explicit timer fired.
#[derive(Clone, Copy)]
enum AudioOsdTimer {
    /// The dwell window elapsed; the surface may retire.
    Dismiss,
    /// An appear or exit transition reached its end; visual progress only.
    Settle,
}

impl AudioOsd {
    pub fn shared(cx: &mut gpui::App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<AudioOsdGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self {
            state: AudioOsdPresentationState::new(
                AudioOsdPlacement::default(),
                AUDIO_OSD_DWELL,
                Duration::ZERO,
                Duration::ZERO,
            ),
        });
        cx.set_global(AudioOsdGlobal(entity.clone()));
        entity
    }

    pub fn state(&self) -> &AudioOsdPresentationState {
        &self.state
    }

    pub fn set_placement(&mut self, placement: AudioOsdPlacement) {
        self.state.set_placement(placement);
    }

    pub fn note_interaction_context(&mut self, output: Option<&str>) {
        self.state.note_interaction_context(output);
    }

    /// Consumes one accepted observation and, if it is a presentable change,
    /// presents it and arms the presentation timers.
    pub fn observe(
        &mut self,
        snapshot: &AudioSnapshot,
        design: ProductDesignContext,
        cx: &mut Context<Self>,
    ) -> Option<AudioOsdEvent> {
        let event = self.state.observe(snapshot, Instant::now())?;
        let appear = ProductMotion::standard(design);
        let sequence = self.state.sequence();
        self.arm(appear, sequence, AudioOsdTimer::Settle, cx);
        // A newer accepted event re-arms this timer with a fresh sequence, which
        // is how a run of changes extends one window instead of stacking.
        let remaining = self
            .state
            .dismiss_at()
            .map(|at| at.saturating_duration_since(Instant::now()))
            .unwrap_or_default();
        let sequence = self.state.sequence();
        self.arm(remaining, sequence, AudioOsdTimer::Dismiss, cx);
        Some(event)
    }

    /// Arms one explicit presentation timer.
    ///
    /// A timer whose sequence is stale does nothing, so a newer accepted event
    /// both replaces the presented truth and extends the window without
    /// needing cancellation.
    fn arm(&self, delay: Duration, sequence: u64, timer: AudioOsdTimer, cx: &mut Context<Self>) {
        let entity = cx.entity();
        cx.spawn(async move |_this, task_cx| {
            task_cx.background_executor().timer(delay).await;
            let now = Instant::now();
            entity.update(&mut *task_cx, |osd, cx| {
                if osd.state.sequence() != sequence {
                    return;
                }
                let changed = match timer {
                    AudioOsdTimer::Dismiss => {
                        let expired = osd.state.expire(now);
                        if expired {
                            let exit = osd.state.exit_duration();
                            let sequence = osd.state.sequence();
                            osd.arm(exit, sequence, AudioOsdTimer::Settle, cx);
                        }
                        expired
                    }
                    AudioOsdTimer::Settle => osd.state.settle(now),
                };
                if changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

/// The rendered facts for one presented OSD, or `None` when nothing is shown.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioOsdPresentation {
    pub glyph: &'static str,
    pub percent: Option<String>,
    pub state_word: Option<&'static str>,
    pub label: String,
    pub aria_label: String,
    /// Filled fraction of the level track, clamped to `0.0..=1.0`.
    pub fraction: f32,
    pub muted: bool,
}

impl AudioOsdPresentation {
    pub fn derive(state: &AudioOsdPresentationState) -> Option<Self> {
        let truth = state.truth()?;
        let percent = truth.percent();
        let fraction = truth
            .level
            .map(|level| level.clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let glyph = if truth.muted {
            "🔇"
        } else if truth.level.is_some_and(|level| level < 0.01) {
            "🔈"
        } else if truth.level.is_some_and(|level| level < 0.5) {
            "🔉"
        } else {
            "🔊"
        };
        let percent_text = percent.map(|percent| format!("{percent}%"));
        // Mute is stated in words as well as by glyph, so it never depends on
        // colour or on recognising an icon.
        let state_word = truth.muted.then_some("muet");
        let aria_label = match (&percent_text, state_word) {
            (Some(percent), Some(_)) => {
                format!("Audio · {} · {percent} · muet", truth.label)
            }
            (Some(percent), None) => format!("Audio · {} · {percent}", truth.label),
            (None, Some(_)) => format!("Audio · {} · niveau inconnu · muet", truth.label),
            (None, None) => format!("Audio · {} · niveau inconnu", truth.label),
        };
        Some(Self {
            glyph,
            percent: percent_text,
            state_word,
            label: truth.label.clone(),
            aria_label,
            fraction,
            muted: truth.muted,
        })
    }
}

/// Renders the transient observed-audio OSD.
///
/// Deliberately non-interactive: a plain `Div` with no element id, no hover or
/// active styles, and no focus handle, so the surface never claims input
/// geometry merely because it paints pixels. There is no slider, no click to
/// mute, and no drag; the OSD reports observed state and owns no mutation.
pub fn render_audio_osd(
    state: AudioOsdPresentationState,
    design: ProductDesignContext,
) -> Option<gpui::Stateful<gpui::Div>> {
    let presentation = AudioOsdPresentation::derive(&state)?;
    // The id is derived from the owning output, so it is unique per surface even
    // if several outputs are being rendered in one process.
    let element_id = format!("audio-osd-{}", state.owner().unwrap_or("unowned"));
    let track_width = design.scaled_px(96.0);
    let track = div()
        .relative()
        .w(px(track_width))
        .h(px(design.scaled_px(6.0)))
        .rounded(px(ProductRadius::BADGE))
        .bg(ProductPalette::control())
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(track_width * presentation.fraction))
                .rounded(px(ProductRadius::BADGE))
                .bg(if presentation.muted {
                    ProductPalette::text_muted()
                } else {
                    ProductPalette::accent()
                }),
        );
    let readout = div()
        .flex()
        .flex_col()
        .gap(px(ProductSpacing::XXS))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(ProductSpacing::XXS))
                .child(text(
                    presentation.percent.clone().unwrap_or_else(|| "—".into()),
                    ProductTextStyle::Label,
                    design,
                ))
                .when_some(presentation.state_word.map(str::to_owned), |row, word| {
                    row.child(text(word, ProductTextStyle::Caption, design))
                }),
        )
        .child(track);
    Some(
        panel()
            // The id exists only so the surface can carry a role and an
            // accessible name. No `on_*` handler, hover, active or focus style is
            // attached, so the OSD creates no hit region and claims no input.
            .id(element_id)
            .flex()
            .items_center()
            .gap(px(ProductSpacing::SM))
            .px(px(ProductSpacing::MD))
            .py(px(ProductSpacing::SM))
            .role(gpui::Role::Status)
            .aria_label(presentation.aria_label)
            .child(text(presentation.glyph, ProductTextStyle::Title, design))
            .child(readout)
            .child(text(presentation.label, ProductTextStyle::Caption, design)),
    )
}

#[cfg(test)]
mod tests;
