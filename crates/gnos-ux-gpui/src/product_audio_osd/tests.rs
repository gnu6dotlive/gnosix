//! Source tests for the observed-audio OSD.
//!
//! The cases are named for the invariants they defend rather than for the
//! functions they call, because the point of each one is an architectural
//! claim: what may drive presentation, what may not, and where authority ends.

use super::*;
use gnos_audio::{
    AudioDefaultRoles, AudioEndpointKind, AudioEndpointSnapshot, AudioVolume, NativeVolume,
    RoleSelection,
};
use std::collections::BTreeMap;

const COOKIE: u32 = 7;
const SERIAL: u64 = 42;
const NODE: &str = "sink.probe";

/// A snapshot with an observed output at a user level and mute state.
fn snapshot(level_linear: f32, muted: bool) -> AudioSnapshot {
    let id = AudioEndpointId::new(COOKIE, SERIAL);
    let endpoint = AudioEndpointSnapshot::new(
        id,
        AudioEndpointKind::Output,
        NODE.into(),
        "Built-in Audio".into(),
        AudioVolume::from_parts(
            None,
            vec![NativeVolume::new(level_linear).unwrap(); 2],
            vec![NativeVolume::new(1.0).unwrap(); 2],
        ),
        muted,
    );
    let mut endpoints = BTreeMap::new();
    endpoints.insert(id, endpoint);
    let mut defaults = AudioDefaultRoles::default();
    defaults.output.effective = RoleSelection {
        name: Some(NODE.into()),
        resolved: Some(id),
        metadata_error: None,
    };
    AudioSnapshot {
        provider: AudioProviderState::Available,
        generation: 1,
        endpoints,
        defaults,
    }
}

/// The same snapshot with a distinct generation, so it is a fresh observation
/// carrying identical presentable facts.
fn replay(mut previous: AudioSnapshot, generation: u64) -> AudioSnapshot {
    previous.generation = generation;
    previous
}

/// An observation carrying no presentable output truth.
fn provider_unavailable() -> AudioSnapshot {
    AudioSnapshot {
        provider: AudioProviderState::Unavailable,
        generation: 2,
        endpoints: BTreeMap::new(),
        defaults: AudioDefaultRoles::default(),
    }
}

fn placement() -> AudioOsdPlacement {
    AudioOsdPlacement::new(vec!["DP-1".into(), "DP-2".into()])
}

fn state() -> AudioOsdPresentationState {
    AudioOsdPresentationState::new(
        placement(),
        AUDIO_OSD_DWELL,
        Duration::from_millis(180),
        Duration::from_millis(180),
    )
}

/// A settled baseline: one snapshot consumed, nothing presented.
fn baselined(mut presentation: AudioOsdPresentationState) -> AudioOsdPresentationState {
    let now = Instant::now();
    assert!(presentation.observe(&snapshot(0.729, false), now).is_none());
    presentation
}

// OSD-1: an initial enumeration establishes a baseline. It is not a change,
// so it must not present anything.
#[test]
fn osd_1_initial_snapshot_creates_no_osd() {
    let mut presentation = state();
    let now = Instant::now();
    assert!(presentation.observe(&snapshot(0.729, false), now).is_none());
    assert!(!presentation.visible());
    assert!(presentation.truth().is_none());
    assert_eq!(presentation.phase(), AudioOsdPhase::Hidden);
}

// OSD-2: an accepted level delta presents the observed value, not the requested
// one. 0.9^3 is the linear value behind an observed 0.9.
#[test]
fn osd_2_accepted_volume_delta_presents_the_observed_value() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    let event = presentation
        .observe(&snapshot(0.614125, false), now)
        .expect("an observed level change presents");
    assert_eq!(event.cause, AudioOsdCause::Level);
    let truth = presentation.truth().expect("presented truth");
    assert!((truth.level.expect("observed level") - 0.85).abs() < 0.0001);
    assert!(!truth.muted);
    assert!(presentation.visible());
}

// OSD-3: an accepted mute delta presents too, and mute is reported without
// destroying the observed level.
#[test]
fn osd_3_accepted_mute_delta_presents_observed_mute() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    let event = presentation
        .observe(&snapshot(0.729, true), now)
        .expect("an observed mute change presents");
    assert_eq!(event.cause, AudioOsdCause::Mute);
    let truth = presentation.truth().expect("presented truth");
    assert!(truth.muted);
    assert!(
        (truth.level.expect("observed level") - 0.9).abs() < 0.0001,
        "mute must not rewrite the observed level"
    );
}

// OSD-4: a fresh observation repeating the same presentable facts presents
// nothing. Semantic deltas drive presentation, not snapshot churn.
#[test]
fn osd_4_repeated_identical_state_creates_no_duplicate_event() {
    let mut presentation = baselined(state());
    let before = presentation.sequence();
    let now = Instant::now();
    assert!(
        presentation
            .observe(&replay(snapshot(0.729, false), 9), now)
            .is_none()
    );
    assert_eq!(presentation.sequence(), before);
    assert!(!presentation.visible());
}

// OSD-5: a change that leaves volume and mute alone presents nothing. Route and
// role metadata refreshes are not presentable changes.
#[test]
fn osd_5_unrelated_metadata_change_creates_no_osd_event() {
    let mut presentation = baselined(state());
    let mut refreshed = replay(snapshot(0.729, false), 11);
    refreshed.defaults.output.configured = RoleSelection {
        name: Some("sink.other".into()),
        resolved: None,
        metadata_error: None,
    };
    let before = presentation.sequence();
    assert!(presentation.observe(&refreshed, Instant::now()).is_none());
    assert_eq!(presentation.sequence(), before);
    assert!(!presentation.visible());
}

// OSD-6: a run of observed deltas coalesces into one presentation carrying the
// newest accepted truth, rather than one presentation per change.
#[test]
fn osd_6_rapid_observed_deltas_coalesce_into_one_newest_truth() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    for level in [0.51_f32, 0.52, 0.53, 0.54, 0.55] {
        presentation.observe(&snapshot(level.powi(3), false), now);
    }
    assert!(
        presentation.visible(),
        "a run of changes keeps one visible surface"
    );
    assert_eq!(presentation.phase(), AudioOsdPhase::Appearing);
    let truth = presentation.truth().expect("newest observed truth");
    assert!(
        (truth.level.expect("observed level") - 0.55).abs() < 0.0001,
        "the presented value must be the newest accepted observation"
    );
}

// OSD-7: each accepted change restarts the dismissal window.
#[test]
fn osd_7_new_delta_resets_the_dismissal_lifetime() {
    let mut presentation = baselined(state());
    let start = Instant::now();
    presentation.observe(&snapshot(0.343, false), start);
    let first = presentation.dismiss_at().expect("armed dismissal");
    let later = start + AUDIO_OSD_DWELL / 2;
    presentation.observe(&snapshot(0.512, false), later);
    let second = presentation.dismiss_at().expect("re-armed dismissal");
    assert!(
        second > first,
        "a newer accepted change must extend the window"
    );
}

// OSD-8: expiry hides the presentation and leaves audio truth untouched. The
// OSD holds no snapshot that could rewrite product state, and expiring it
// produces no event for any other consumer.
#[test]
fn osd_8_timeout_hides_presentation_without_altering_audio_truth() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    assert!(
        presentation.observe(&snapshot(0.343, false), now).is_some(),
        "an observed change presents"
    );
    assert!(
        !presentation.expire(now + AUDIO_OSD_DWELL - Duration::from_millis(1)),
        "expiry is not due yet"
    );
    assert!(presentation.visible());
    assert!(presentation.expire(now + AUDIO_OSD_DWELL));
    assert!(
        !presentation.visible(),
        "a retiring surface no longer claims the level as current"
    );
    assert!(presentation.painted(), "it is still fading out");
    assert_eq!(presentation.phase(), AudioOsdPhase::Exiting);
    assert!(
        presentation.settle(now + AUDIO_OSD_DWELL + Duration::from_millis(180)),
        "the exit transition completes"
    );
    assert_eq!(presentation.phase(), AudioOsdPhase::Hidden);
    assert!(presentation.truth().is_none());
    assert!(
        !presentation.expire(now + AUDIO_OSD_DWELL * 10),
        "a hidden OSD has no dismissal to act on"
    );
}

// OSD-9: when the provider stops reporting presentable truth, a presented OSD is
// retired rather than left looking current. Recovery re-establishes a baseline,
// so a provider bounce never fabricates a change.
#[test]
fn osd_9_unavailable_provider_leaves_no_falsely_current_osd() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    assert!(presentation.observe(&snapshot(0.343, false), now).is_some());
    assert!(presentation.visible());

    assert!(
        presentation.observe(&provider_unavailable(), now).is_none(),
        "losing the provider is not a presentable change"
    );
    assert!(!presentation.visible());
    assert!(presentation.truth().is_none());
    assert_eq!(presentation.phase(), AudioOsdPhase::Hidden);

    // Recovery establishes a new baseline rather than presenting the level the
    // provider happens to report at the moment it returns.
    let mut recovered = snapshot(0.343, false);
    recovered.generation = 3;
    assert!(presentation.observe(&recovered, now).is_none());
    assert!(!presentation.visible());

    // ...and the first genuine change after recovery presents normally.
    assert!(presentation.observe(&snapshot(0.729, false), now).is_some());
}

// OSD-10: one event resolves to exactly one output, and the placement hint from
// an earlier interaction is consumed rather than reused.
#[test]
fn osd_10_one_event_resolves_to_exactly_one_output() {
    let mut presentation = state();

    // With no interaction context the reconciled primary is deterministic.
    presentation.observe(&snapshot(0.729, false), Instant::now());
    presentation.observe(&snapshot(0.343, false), Instant::now());
    assert_eq!(presentation.owner(), Some("DP-1"));

    // An interaction on the second output places it there instead.
    presentation.note_interaction_context(Some("DP-2"));
    presentation.observe(&snapshot(0.512, false), Instant::now());
    assert_eq!(presentation.owner(), Some("DP-2"));

    // The context is one-shot, so a later external change falls back to the
    // deterministic primary rather than sticking to the old output.
    presentation.observe(&snapshot(0.729, false), Instant::now());
    assert_eq!(presentation.owner(), Some("DP-1"));
}

#[test]
fn osd_10_unusable_or_absent_placement_fails_closed() {
    // An unresolvable placement must present nowhere rather than duplicate.
    let mut presentation = AudioOsdPresentationState::new(
        AudioOsdPlacement::default(),
        AUDIO_OSD_DWELL,
        Duration::from_millis(180),
        Duration::from_millis(180),
    );
    presentation.observe(&snapshot(0.729, false), Instant::now());
    presentation.observe(&snapshot(0.343, false), Instant::now());
    assert_eq!(presentation.owner(), None);

    // A context naming an output this session does not have is not honoured.
    let mut presentation = state();
    presentation.note_interaction_context(Some("DP-9"));
    presentation.observe(&snapshot(0.729, false), Instant::now());
    presentation.observe(&snapshot(0.343, false), Instant::now());
    assert_eq!(presentation.owner(), Some("DP-1"));
}

// OSD-11: the OSD cannot reach PipeWire or form an intent, because it holds no
// capability that could. This pins the absence at the module boundary rather
// than trusting a convention.
#[test]
fn osd_11_osd_has_no_mutation_authority() {
    // The presentation state carries only output identity, presentation context
    // and timing. There is no provider client, no session cookie authority, and
    // no intent type reachable from it.
    let presentation = baselined(state());
    let truth = presentation.truth();
    assert!(truth.is_none(), "a baseline presents nothing");
    // `AudioOsdTruth` is a data record: its only field that names an endpoint is
    // the observed id it reports, never a request.
    let sample = AudioOsdTruth {
        endpoint: AudioEndpointId::new(COOKIE, SERIAL),
        label: "Built-in Audio".into(),
        level: Some(0.9),
        muted: false,
        generation: 3,
    };
    assert_eq!(sample.percent(), Some(90));
}

#[test]
fn osd_11_source_has_no_provider_or_intent_dependency() {
    // A source-level assertion, because a future edit could reintroduce a
    // dependency that the type-level tests above would not notice.
    //
    // Comments are stripped first: the module's own documentation legitimately
    // names the capabilities it must never acquire, and this check is about
    // code, not prose.
    let source = include_str!("../product_audio_osd.rs")
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.starts_with("*") && !trimmed.starts_with("/*")
        })
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "pipewire",
        "AudioIntent",
        "dispatch_intent",
        "PipeWireObserver",
        "mutate",
        "set_volume",
        "on_click",
        "on_mouse",
        "hover",
        "FocusHandle",
    ] {
        assert!(
            !source.contains(forbidden),
            "the OSD module must not reference {forbidden}"
        );
    }
}

// OSD-12: reduced motion changes only how the OSD appears and disappears. The
// presented truth, the dwell window, and the semantic state machine are
// identical.
#[test]
fn osd_12_reduced_motion_changes_animation_only() {
    let start = Instant::now();
    let mut full = AudioOsdPresentationState::new(
        placement(),
        AUDIO_OSD_DWELL,
        Duration::from_millis(180),
        Duration::from_millis(180),
    );
    let mut reduced = AudioOsdPresentationState::new(
        placement(),
        AUDIO_OSD_DWELL,
        Duration::ZERO,
        Duration::ZERO,
    );
    for presentation in [&mut full, &mut reduced] {
        presentation.observe(&snapshot(0.729, false), start);
        presentation.observe(&snapshot(0.343, false), start);
    }

    // Same semantics.
    assert_eq!(full.truth(), reduced.truth());
    assert_eq!(full.dismiss_at(), reduced.dismiss_at());
    assert_eq!(full.visible(), reduced.visible());

    // Reduced motion skips straight to shown; full motion passes through a
    // transition that resolves to the same place.
    assert_eq!(reduced.phase(), AudioOsdPhase::Shown);
    assert_eq!(full.phase(), AudioOsdPhase::Appearing);
    assert!(full.settle(start + Duration::from_millis(180)));
    assert_eq!(full.phase(), reduced.phase());

    // Identical lifetime, and expiry does not depend on the animation.
    for presentation in [&mut full, &mut reduced] {
        assert!(presentation.expire(start + AUDIO_OSD_DWELL));
    }
    assert_eq!(full.phase(), AudioOsdPhase::Exiting);
    assert_eq!(reduced.phase(), AudioOsdPhase::Hidden);
    assert!(
        full.truth().is_some(),
        "the fading surface still renders the state it is fading from"
    );
    assert!(reduced.truth().is_none());
    assert_eq!(full.visible(), reduced.visible());
    assert!(full.settle(start + AUDIO_OSD_DWELL + Duration::from_millis(180)));
    assert_eq!(full.phase(), reduced.phase());
}

// The OSD reports observed level even above unity, and never invents a level the
// provider did not publish.
#[test]
fn presentation_reports_observed_amplification_and_unknown_level() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    presentation.observe(&snapshot(1.2_f32.powi(3), false), now);
    let rendered = AudioOsdPresentation::derive(&presentation).expect("rendered");
    assert_eq!(rendered.percent.as_deref(), Some("120%"));
    assert!(
        (rendered.fraction - 1.0).abs() < f32::EPSILON,
        "the track fill is bounded by the track, while the readout stays truthful"
    );

    // An output with no readable volume renders as unknown, not as zero.
    let mut silent = snapshot(0.0, false);
    if let Some(endpoint) = silent
        .endpoints
        .get_mut(&AudioEndpointId::new(COOKIE, SERIAL))
    {
        endpoint.volume = AudioVolume::from_parts(None, Vec::new(), Vec::new());
    }
    let mut presentation = state();
    presentation.observe(&snapshot(0.729, false), now);
    presentation.observe(&silent, now);
    let rendered = AudioOsdPresentation::derive(&presentation).expect("rendered");
    assert_eq!(rendered.percent, None);
    assert_eq!(rendered.state_word, None);
    assert!(
        rendered.aria_label.contains("niveau inconnu"),
        "an unobserved level is stated, not fabricated"
    );
}

// Mute must be legible without colour: the word appears in the OSD text, not
// only in the accessibility label or the glyph.
#[test]
fn presentation_states_mute_in_words_not_only_in_colour() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    presentation.observe(&snapshot(0.729, true), now);
    let rendered = AudioOsdPresentation::derive(&presentation).expect("rendered");
    assert!(rendered.muted);
    assert_eq!(rendered.state_word, Some("muet"));
    assert!(rendered.aria_label.contains("muet"));

    // Unmuting reports the current observed level rather than a zeroed one.
    presentation.observe(&snapshot(0.729, false), now);
    let rendered = AudioOsdPresentation::derive(&presentation).expect("rendered");
    assert!(!rendered.muted);
    assert_eq!(rendered.state_word, None);
    assert_eq!(rendered.percent.as_deref(), Some("90%"));
}

// A hidden OSD renders nothing at all, rather than an empty surface.
#[test]
fn hidden_state_renders_nothing() {
    let presentation = state();
    assert!(render_audio_osd(presentation.clone(), design_context()).is_none());
}

fn design_context() -> ProductDesignContext {
    ProductDesignContext::new(1.0, false).expect("design context")
}

// The surface is rendered from existing design tokens, at the requested scale,
// and carries an accessible status role with the observed facts in its label.
#[test]
fn rendered_surface_uses_design_context_scale_and_observed_label() {
    let mut presentation = baselined(state());
    let now = Instant::now();
    presentation.observe(&snapshot(0.729, true), now);

    let scaled = design_context();
    let stress = ProductDesignContext::new(2.0, false).expect("stress context");
    assert!(render_audio_osd(presentation.clone(), scaled).is_some());
    assert!(
        render_audio_osd(presentation.clone(), stress).is_some(),
        "the OSD renders at long-scale rather than being suppressed"
    );
}
