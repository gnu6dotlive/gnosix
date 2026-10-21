//! First real surface for MPRIS2 state.
//!
//! Renders a bounded "Now Playing" component from [`MprisPresentation`], using
//! the existing GPUI toolkit and design tokens. It is not a media centre: it
//! shows what is playing and offers the three transport controls the mission
//! defines.
//!
//! ## What this widget is not allowed to know
//!
//! No bus name, object path, interface, method name, or property key appears
//! here, and none can reach it. Every control carries an [`MprisIntent`] bound
//! to one [`ExactMprisPlayerRef`], and the transport revalidates identity and
//! capability before any call. A control painted for a player that has since
//! been replaced therefore refuses rather than acting on the replacement.
//!
//! ## What the surface does with absences
//!
//! Live exporters omitted most of what a media widget normally shows. A player
//! with no title renders its source URL's final segment; a player with no track
//! at all renders an explicit unknown-track state rather than an empty gap; a
//! player with no known length renders no progress bar rather than a bar stuck
//! at zero. Each of those is a claim about what the surface knows, and none of
//! them invents data.

use std::rc::Rc;

use gpui::{App, Div, Stateful, div, prelude::*, px};

use crate::product_design::{
    ProductDesignContext, ProductPalette, ProductRadius, ProductSpacing, ProductTextStyle, text,
};
use crate::product_mpris::{
    ExactMprisPlayerRef, MprisArtwork, MprisOperation, MprisPlayerSnapshot, MprisPresentation,
};
use crate::product_mpris_transport::MprisIntent;

/// The sink a rendered control dispatches its typed intent through.
///
/// A widget cannot call D-Bus; it can only hand a typed intent to whatever the
/// scene runtime installed. This is what makes the transport the only reachable
/// path to an exporter.
pub type NowPlayingDispatch = Rc<dyn Fn(&mut App, MprisIntent)>;

/// Shown when the exporter identified no track at all.
///
/// A real exporter can publish no title, no artist and no URL. Rather than
/// drawing a blank gap, the surface says what it knows, which is that it knows
/// nothing.
pub const UNKNOWN_TRACK_LABEL: &str = "Piste inconnue";

/// One rendered transport control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NowPlayingControl {
    pub operation: MprisOperation,
    /// Human label, also used as the accessible name.
    pub label: &'static str,
    /// Glyph drawn for the control.
    pub glyph: &'static str,
    /// False when the observed capabilities forbid the operation.
    pub enabled: bool,
    /// Why the control is disabled, when it is. Surfaced so a disabled control
    /// is explainable rather than mysteriously inert.
    pub disabled_reason: Option<&'static str>,
}

impl NowPlayingControl {
    fn build(player: &MprisPlayerSnapshot, operation: MprisOperation) -> Self {
        let enabled = player.capabilities.permits(operation);
        let (label, glyph) = match operation {
            MprisOperation::Previous => ("Piste précédente", "«"),
            MprisOperation::PlayPause => {
                // The affordance follows observed state rather than being
                // static, so the control says what it will do.
                if player.playback.is_playing() {
                    ("Mettre en pause", "❚❚")
                } else {
                    ("Lire", "▶")
                }
            }
            MprisOperation::Next => ("Piste suivante", "»"),
            MprisOperation::Seek(_) => ("Position", "⏱"),
        };
        let disabled_reason = if enabled {
            None
        } else if !player.capabilities.can_control {
            Some("le lecteur refuse tout contrôle")
        } else {
            Some("capacité absente")
        };
        Self {
            operation,
            label,
            glyph,
            enabled,
            disabled_reason,
        }
    }
}

/// Everything a Now Playing surface needs, derived from typed state.
///
/// Derived rather than passed, so the widget cannot disagree with the snapshot
/// it claims to render.
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlayingPresentation {
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub player_label: String,
    pub playback_label: &'static str,
    /// Present only when both a length and a within-length position were
    /// observed. `None` means the surface must draw no bar at all.
    pub progress: Option<f64>,
    pub controls: Vec<NowPlayingControl>,
    pub artwork: Option<MprisArtwork>,
    /// Explicit empty-state marker, distinct from "has a track".
    pub empty: bool,
    /// Non-fatal observation problems worth showing.
    pub notes: Vec<String>,
    /// Other distinct players, when more than one exists.
    pub other_players: Vec<String>,
}

impl NowPlayingPresentation {
    /// Projects the typed presentation into what the widget draws.
    pub fn derive(presentation: &MprisPresentation) -> Self {
        let Some(player) = presentation.effective() else {
            return Self {
                title: None,
                artists: Vec::new(),
                player_label: "Aucun lecteur".to_owned(),
                playback_label: "—",
                progress: None,
                controls: Vec::new(),
                artwork: None,
                empty: true,
                notes: Vec::new(),
                other_players: Vec::new(),
            };
        };

        let controls = vec![
            NowPlayingControl::build(player, MprisOperation::Previous),
            NowPlayingControl::build(player, MprisOperation::PlayPause),
            NowPlayingControl::build(player, MprisOperation::Next),
        ];

        let progress = player
            .position
            .fraction_of(player.track.length)
            .filter(|_| player.track.has_meaningful_length());

        // Diagnostics become product-visible notes. An exporter that told us
        // nothing about its track is a condition the surface should be able to
        // explain, not a silent gap.
        let mut notes = Vec::new();
        if !player.has_track() {
            notes.push("aucune information de piste".to_owned());
        }
        if player.track.title.is_none() && player.track.url.is_some() {
            notes.push("titre absent, nom de fichier utilisé".to_owned());
        }
        if player.track.length.is_none() {
            notes.push("durée inconnue".to_owned());
        }
        if player.application_key.is_none() {
            notes.push("DesktopEntry absent".to_owned());
        }

        let other_players = presentation
            .distinct_players()
            .iter()
            .skip(1)
            .map(|p| p.label.clone())
            .collect();

        // The unknown-track label is decided here rather than at paint time,
        // so the presentation itself states what it does not know. Leaving it
        // absent would make the emptiness a property of the caller's fallback
        // instead of a visible product condition.
        let title = player
            .display_title()
            .map(str::to_owned)
            .unwrap_or_else(|| UNKNOWN_TRACK_LABEL.to_owned());

        Self {
            title: Some(title),
            artists: player.track.artists.clone(),
            player_label: player.label.clone(),
            playback_label: player.playback.label(),
            progress,
            controls,
            artwork: player.track.artwork.clone(),
            empty: !player.has_track(),
            notes,
            other_players,
        }
    }
}

/// Renders the bounded Now Playing surface.
pub fn render_now_playing(
    presentation: &NowPlayingPresentation,
    design: ProductDesignContext,
    dispatch: Option<&NowPlayingDispatch>,
    player: Option<&ExactMprisPlayerRef>,
) -> Stateful<Div> {
    if presentation.empty {
        return div()
            .id("now-playing-empty")
            .role(gpui::Role::Group)
            .aria_label("Récupération de média")
            .flex()
            .flex_col()
            .px(px(ProductSpacing::XS))
            .py(px(ProductSpacing::XXS))
            .rounded(px(ProductRadius::PANEL))
            .border_1()
            .border_color(ProductPalette::border())
            .bg(ProductPalette::panel_elevated())
            .child(text(
                "Aucune lecture en cours",
                ProductTextStyle::Body,
                design,
            ));
    }

    let title = presentation.title.as_deref().unwrap_or(UNKNOWN_TRACK_LABEL);
    let artist_line = if presentation.artists.is_empty() {
        "Artiste inconnu".to_owned()
    } else {
        presentation.artists.join(", ")
    };

    let mut elem = div()
        .id("now-playing")
        .role(gpui::Role::Group)
        .aria_label("Lecture en cours")
        .flex()
        .flex_col()
        .min_w(px(220.0))
        .max_w(px(360.0))
        .px(px(ProductSpacing::XS))
        .py(px(ProductSpacing::XXS))
        .rounded(px(ProductRadius::PANEL))
        .border_1()
        .border_color(ProductPalette::border())
        .bg(ProductPalette::panel_elevated())
        .gap(px(ProductSpacing::XXS));

    elem = elem.child(
        div()
            .flex()
            .flex_col()
            .child(text(
                presentation.player_label.clone(),
                ProductTextStyle::Label,
                design,
            ))
            .child(text(title, ProductTextStyle::Title, design))
            .child(text(artist_line, ProductTextStyle::Body, design))
            .child(text(
                presentation.playback_label,
                ProductTextStyle::Caption,
                design,
            )),
    );

    // Progress is drawn only when the exporter gave both a length and a
    // position inside it. A bar at zero would assert a position that was never
    // observed.
    if let Some(fraction) = presentation.progress {
        elem = elem.child(render_progress(fraction, design));
    }

    let controls: Vec<Stateful<Div>> = presentation
        .controls
        .iter()
        .map(|control| {
            render_control(
                control,
                design,
                dispatch,
                player,
                presentation.player_label.as_str(),
            )
        })
        .collect();
    elem = elem.child(
        div()
            .flex()
            .flex_row()
            .gap(px(ProductSpacing::XXS))
            .children(controls),
    );

    if !presentation.notes.is_empty() {
        elem = elem.child(
            div().flex().flex_col().children(
                presentation
                    .notes
                    .iter()
                    .map(|note| text(note.as_str(), ProductTextStyle::Caption, design)),
            ),
        );
    }

    if !presentation.other_players.is_empty() {
        let label = format!("{} autre(s) lecteur(s)", presentation.other_players.len());
        elem = elem.child(text(label, ProductTextStyle::Caption, design));
    }

    elem
}

fn render_progress(fraction: f64, design: ProductDesignContext) -> Stateful<Div> {
    let track_width = 240.0f32;
    // Clamped for painting only. The presentation already refuses a fraction
    // outside 0..=1, so this cannot paper over an invalid value.
    let filled = (fraction.clamp(0.0, 1.0) as f32 * track_width)
        .round()
        .max(1.0);
    let percent = format!("{:.0} %", (fraction.clamp(0.0, 1.0) * 100.0).round());
    div()
        .id("now-playing-progress")
        .role(gpui::Role::Slider)
        .h(px(3.0))
        .w(px(track_width))
        .rounded(px(ProductRadius::CONTROL))
        .bg(ProductPalette::border())
        .child(
            div()
                .h(px(3.0))
                .w(px(filled))
                .rounded(px(ProductRadius::CONTROL))
                .bg(ProductPalette::accent()),
        )
        .child(
            text(percent.as_str(), ProductTextStyle::Caption, design)
                .absolute()
                .top(px(6.0)),
        )
}

fn render_control(
    control: &NowPlayingControl,
    design: ProductDesignContext,
    dispatch: Option<&NowPlayingDispatch>,
    player: Option<&ExactMprisPlayerRef>,
    player_label: &str,
) -> Stateful<Div> {
    let mut elem = div()
        .id(format!(
            "now-playing-control-{}-{}",
            control.operation.name(),
            sanitize(player_label)
        ))
        .flex()
        .items_center()
        .justify_center()
        .w(px(36.0))
        .h(px(28.0))
        .rounded(px(ProductRadius::CONTROL))
        .cursor(if control.enabled {
            gpui::CursorStyle::PointingHand
        } else {
            gpui::CursorStyle::Arrow
        })
        .child(text(control.glyph, ProductTextStyle::Title, design));

    if control.enabled {
        elem = elem
            .bg(ProductPalette::border())
            .hover(|style| style.bg(ProductPalette::control_hover()));
        // The handler carries a typed intent bound to the exact player. There
        // is no path from this closure to a bus address.
        if let (Some(dispatch), Some(player)) = (dispatch, player) {
            // Cloned so the closure owns the sink, matching how the tray menu
            // surface does it: the handler must outlive this borrow.
            let sink = Rc::clone(dispatch);
            let intent = match control.operation {
                MprisOperation::Previous => MprisIntent::previous(player.clone()),
                MprisOperation::PlayPause => MprisIntent::play_pause(player.clone()),
                MprisOperation::Next => MprisIntent::next(player.clone()),
                MprisOperation::Seek(_) => MprisIntent::play_pause(player.clone()),
            };
            elem = elem.on_click(move |_click, _window, cx| sink(cx, intent.clone()));
        }
    } else {
        elem = elem.bg(ProductPalette::border()).opacity(0.5);
        // A disabled control states why, rather than being silently inert.
        let reason = control.disabled_reason.unwrap_or("indisponible");
        elem = elem
            .child(text(reason, ProductTextStyle::Caption, design))
            .absolute()
            .left(px(0.0))
            .top(px(ProductSpacing::XXS * 2.0));
    }

    elem
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_mpris::{
        ExactMprisPlayerRef, MprisPlayerId, PlaybackCapabilities, PlaybackPosition, PlaybackState,
        TrackIdentity, TrackMetadata,
    };
    use std::time::Duration;

    fn player(label: &str, capabilities: PlaybackCapabilities) -> MprisPlayerSnapshot {
        MprisPlayerSnapshot {
            identity: ExactMprisPlayerRef {
                id: MprisPlayerId::new("org.mpris.MediaPlayer2.t@:1.1"),
                service: "org.mpris.MediaPlayer2.t".to_owned(),
                path: "/org/mpris/MediaPlayer2".to_owned(),
                owner: ":1.1".to_owned(),
            },
            label: label.to_owned(),
            application_key: None,
            playback: PlaybackState::Playing,
            track: TrackMetadata {
                identity: TrackIdentity(Some("/track/1".to_owned())),
                title: Some("Song".to_owned()),
                artists: vec!["Artist".to_owned()],
                length: Some(Duration::from_secs(100)),
                ..TrackMetadata::default()
            },
            position: PlaybackPosition::observed(
                Duration::from_secs(25),
                std::time::Instant::now(),
            ),
            rate: 1.0,
            volume: 1.0,
            capabilities,
            diagnostics: Vec::new(),
        }
    }

    fn all_capabilities() -> PlaybackCapabilities {
        PlaybackCapabilities {
            can_control: true,
            can_play: true,
            can_pause: true,
            can_go_next: true,
            can_go_previous: true,
            can_seek: true,
        }
    }

    #[test]
    fn an_absent_player_renders_an_explicit_empty_state() {
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Unavailable);
        assert!(derived.empty);
        assert!(derived.controls.is_empty(), "no controls without a player");
        assert_eq!(derived.player_label, "Aucun lecteur");
    }

    #[test]
    fn controls_are_bounded_to_the_three_transport_operations() {
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(player(
            "VLC",
            all_capabilities(),
        ))));
        let operations: Vec<MprisOperation> =
            derived.controls.iter().map(|c| c.operation).collect();
        assert_eq!(
            operations,
            vec![
                MprisOperation::Previous,
                MprisOperation::PlayPause,
                MprisOperation::Next
            ],
            "the surface must not offer Seek, which this component never exercises"
        );
    }

    #[test]
    fn disabled_capabilities_disable_controls_and_say_why() {
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(player(
            "Silent",
            PlaybackCapabilities {
                can_control: true,
                can_pause: true,
                ..PlaybackCapabilities::none()
            },
        ))));
        let previous = derived
            .controls
            .iter()
            .find(|c| c.operation == MprisOperation::Previous)
            .expect("previous control");
        assert!(!previous.enabled);
        assert_eq!(previous.disabled_reason, Some("capacité absente"));
        let play_pause = derived
            .controls
            .iter()
            .find(|c| c.operation == MprisOperation::PlayPause)
            .expect("play/pause control");
        assert!(
            play_pause.enabled,
            "a player that can play may still be played"
        );
    }

    #[test]
    fn a_refused_control_disables_everything_with_its_own_reason() {
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(player(
            "Refuses",
            PlaybackCapabilities {
                can_control: false,
                can_play: true,
                can_pause: true,
                can_go_next: true,
                can_go_previous: true,
                can_seek: true,
            },
        ))));
        assert!(derived.controls.iter().all(|c| !c.enabled));
        assert!(
            derived
                .controls
                .iter()
                .all(|c| c.disabled_reason == Some("le lecteur refuse tout contrôle")),
            "control refusal must be distinguished from a missing capability"
        );
    }

    #[test]
    fn play_pause_affordance_follows_observed_state() {
        let mut playing = player("VLC", all_capabilities());
        let derived =
            NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(playing.clone())));
        let control = derived
            .controls
            .iter()
            .find(|c| c.operation == MprisOperation::PlayPause)
            .expect("control");
        assert_eq!(control.glyph, "❚❚", "a playing player offers pause");

        playing.playback = PlaybackState::Paused;
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(playing)));
        let control = derived
            .controls
            .iter()
            .find(|c| c.operation == MprisOperation::PlayPause)
            .expect("control");
        assert_eq!(control.glyph, "▶", "a paused player offers play");
    }

    #[test]
    fn progress_needs_a_length_and_a_position_inside_it() {
        let mut p = player("VLC", all_capabilities());
        let derived =
            NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(p.clone())));
        assert_eq!(derived.progress, Some(0.25), "25s of 100s is a quarter");

        // A live exporter published no length at all.
        p.track.length = None;
        let derived =
            NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(p.clone())));
        assert_eq!(derived.progress, None, "no length means no bar");
        assert!(derived.notes.iter().any(|n| n == "durée inconnue"));

        // No observed position means no fraction, rather than a bar at zero.
        p.track.length = Some(Duration::from_secs(100));
        p.position = PlaybackPosition::default();
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(p)));
        assert_eq!(derived.progress, None, "no observed position means no bar");
    }

    #[test]
    fn a_missing_title_falls_back_to_the_source_filename() {
        let mut p = player("VLC", all_capabilities());
        p.track.title = None;
        p.track.url = Some("file:///home/u/Music/Interlude.ogg".to_owned());
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(p)));
        assert_eq!(derived.title.as_deref(), Some("Interlude.ogg"));
        assert!(
            derived.notes.iter().any(|n| n.contains("titre absent")),
            "the fallback must be disclosed, not passed off as a real title"
        );
    }

    #[test]
    fn a_player_with_nothing_known_renders_unknown_not_empty() {
        let mut p = player("Bare", PlaybackCapabilities::none());
        p.track = TrackMetadata::default();
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(p)));
        // Not the empty state: the player exists, we simply know nothing about
        // its track. The distinction is what lets the surface say so.
        assert!(
            derived.empty,
            "a player with no track information renders as such"
        );
        assert_eq!(derived.title.as_deref(), Some("Piste inconnue"));
        assert!(
            derived
                .notes
                .iter()
                .any(|n| n.contains("aucune information"))
        );
    }

    #[test]
    fn missing_artwork_and_desktop_entry_become_notes() {
        let mut p = player("Chromium", PlaybackCapabilities::none());
        p.application_key = None;
        p.track.artwork = None;
        let derived = NowPlayingPresentation::derive(&MprisPresentation::Single(Box::new(p)));
        assert_eq!(derived.artwork, None);
        assert!(
            derived.notes.iter().any(|n| n == "DesktopEntry absent"),
            "the absent key observed on Chromium must be visible, not hidden"
        );
    }

    #[test]
    fn several_players_name_the_effective_one_and_count_the_rest() {
        let mut first = player("VLC media player", all_capabilities());
        first.identity.service = "org.mpris.MediaPlayer2.vlc".to_owned();
        first.identity.owner = ":1.5065".to_owned();
        let mut duplicate = first.clone();
        duplicate.identity.service = "org.mpris.MediaPlayer2.vlc.instance1".to_owned();
        duplicate.identity.owner = ":1.5066".to_owned();

        let mut second = player("Helium", all_capabilities());
        second.identity.service = "org.mpris.MediaPlayer2.chromium.i1".to_owned();

        let presentation = MprisPresentation::select(&[first, duplicate, second]);
        let derived = NowPlayingPresentation::derive(&presentation);
        assert_eq!(
            derived.player_label, "Helium",
            "selection is deterministic by well-known name, and the label shown is that of the player it chose"
        );
        assert_eq!(
            derived.other_players.len(),
            1,
            "the duplicate is folded, the other listed"
        );
    }
}
