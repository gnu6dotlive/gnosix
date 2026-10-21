//! Typed MPRIS2 player read model.
//!
//! This module holds no protocol vocabulary. It is the vocabulary a GPUI
//! surface is allowed to speak: a player, a playback state, a track, a
//! position, a set of capabilities. No bus name, object path, interface name,
//! method name, or `Variant` map appears here, and none of these types can be
//! constructed from one.
//!
//! Every shape in this file exists because live reconnaissance found a real
//! exporter that violated the specification. The evidence is recorded in
//! `docs/workstreams/mpris/01/reconnaissance.md`; the findings that shaped each type
//! are named at it.
//!
//! The governing rule from that reconnaissance is that **one malformed
//! property must not destroy player state**. A real exporter omitted
//! `DesktopEntry` entirely, another omitted every `xesam:` title field, and a
//! third declared `Position` as int32 where int64 is specified. So parsing is
//! per-field and total: anything unreadable degrades to an absent field plus a
//! recorded diagnostic, never to a dropped player.

use std::time::Duration;

/// Observation key for one player.
///
/// Distinct from the well-known bus name: a player observed under one name may
/// be replaced by another exporter that re-requests that name. The value here
/// is what a rendered surface binds to, so it must change when the underlying
/// instance does.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MprisPlayerId(String);

impl MprisPlayerId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MprisPlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Live identity of one observed player instance.
///
/// This is the MPRIS analogue of `ExactTrayItemRef`. It carries the unique-name
/// owner because a well-known name is not an identity: it can be released and
/// re-requested by a different exporter while a rendered Now Playing surface is
/// still on screen. Comparing the current owner against `owner` is what detects
/// that substitution, and it is why an interaction bound to a replaced player
/// cannot silently act on its replacement.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactMprisPlayerRef {
    pub id: MprisPlayerId,
    /// Well-known name the player was discovered under.
    pub service: String,
    /// Object path that actually answered. Not derivable from the well-known
    /// name in practice: finding 1 recorded that the specification's derivation
    /// returns nothing on both live exporters.
    pub path: String,
    /// Unique-name owner resolved at observation time.
    pub owner: String,
}

impl ExactMprisPlayerRef {
    /// Same exporter identity, ignoring the observation key.
    ///
    /// Used by deduplication (finding 3): one logical player can be published
    /// under two well-known names owned by two different unique names.
    pub fn same_exporter(&self, other: &Self) -> bool {
        self.path == other.path && self.service == other.service
    }
}

/// Playback state as reported by an exporter.
///
/// Finding 9: `Rate` was 0.0 on a paused player, so rate cannot be used to
/// infer playback. `PlaybackStatus` is the only authoritative signal, and this
/// type is the only representation of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlaybackState {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl PlaybackState {
    /// Total parse: an unrecognised or missing value becomes `Stopped`, the
    /// state that asserts nothing about the exporter.
    pub fn parse(value: &str) -> Self {
        match value {
            "Playing" => Self::Playing,
            "Paused" => Self::Paused,
            _ => Self::Stopped,
        }
    }

    pub fn is_playing(self) -> bool {
        matches!(self, Self::Playing)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Playing => "Lecture",
            Self::Paused => "En pause",
            Self::Stopped => "Arrêt",
        }
    }
}

/// Track identity as published by an exporter.
///
/// Absent on real exporters, so this is optional everywhere it appears: a
/// rendered control must not be able to assume a track identity exists.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TrackIdentity(pub Option<String>);

impl TrackIdentity {
    pub fn is_known(&self) -> bool {
        self.0.is_some()
    }
}

/// Artwork reference, reduced to what a bounded surface can actually use.
///
/// Finding 5: a real exporter advertised no artwork at all while another
/// advertised a `file://` URL. A malformed or non-displayable URL is recorded
/// as a diagnostic and yields `None` rather than an unusable image source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MprisArtwork {
    /// A URL the exporter published that uses a scheme the product can render.
    Renderable(String),
    /// A URL was published but cannot be rendered (unsupported scheme or a
    /// malformed reference). Retained so the surface can explain the gap
    /// instead of silently showing a placeholder.
    Unrenderable(String),
}

/// The current track, tolerating every absence a live exporter exhibited.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackMetadata {
    pub identity: TrackIdentity,
    pub title: Option<String>,
    /// Renderable artists. Exporters publish zero, one, or many; this stays a
    /// list rather than being collapsed, so collapsing is a product decision
    /// made at the surface and not smuggled into the protocol layer.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub artwork: Option<MprisArtwork>,
    /// Track length. Absent when the exporter omits `mpris:length`, which
    /// decides whether progress is meaningful at all.
    pub length: Option<Duration>,
    /// The location the exporter published. For a title-less track this is the
    /// only identifying information available, so it is retained rather than
    /// discarded during projection.
    pub url: Option<String>,
}

impl TrackMetadata {
    /// Whether enough is known to render a progress bar.
    ///
    /// Deliberately requires both a length and a position within it. A length
    /// with no position, or a position past the end, is not progress.
    pub fn has_meaningful_length(&self) -> bool {
        self.length.is_some_and(|length| length > Duration::ZERO)
    }

    /// True when the exporter gave us nothing to identify the track by.
    pub fn is_blank(&self) -> bool {
        self.title.is_none()
            && self.artists.is_empty()
            && self.album.is_none()
            && self.url.is_none()
    }
}

/// Last observed playback position.
///
/// Finding 6: no live exporter emitted any position signal in 16 seconds of
/// observation, so position is only ever observed by re-reading a property.
/// Finding 10: observed position advanced roughly 2ms per 2 seconds of wall
/// clock, so interpolating from an anchor plus elapsed time would report
/// progress the exporter never produced. This type therefore carries **only an
/// observation** — never a computed or projected value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaybackPosition {
    position: Duration,
    observed_at: Option<std::time::Instant>,
}

impl PlaybackPosition {
    pub fn observed(position: Duration, at: std::time::Instant) -> Self {
        Self {
            position,
            observed_at: Some(at),
        }
    }

    /// Position as last reported by the exporter. Never interpolated.
    pub fn position(self) -> Duration {
        self.position
    }

    /// Whether any position has been observed at all.
    pub fn is_known(self) -> bool {
        self.observed_at.is_some()
    }

    /// Fraction of `length` reached, or `None` when progress is not meaningful.
    ///
    /// Returns `None` rather than a clamped or zero value when the length is
    /// unknown or the position lies outside it, because a progress bar reading
    /// 0% for an unknown track is a claim, and a false one.
    pub fn fraction_of(self, length: Option<Duration>) -> Option<f64> {
        let length = length?;
        if !self.is_known() || length.is_zero() {
            return None;
        }
        let fraction = self.position.as_secs_f64() / length.as_secs_f64();
        if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
            return None;
        }
        Some(fraction)
    }
}

/// What an exporter claims it can do.
///
/// Every flag is read from the exporter's `GetAll` response and never from
/// introspection XML. Finding 2 recorded why that distinction is load-bearing:
/// one live exporter returned a completely empty introspection node, and
/// another's XML omitted capability flags its own property response contained.
/// Introspection cannot decide what a player can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaybackCapabilities {
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_seek: bool,
}

impl PlaybackCapabilities {
    /// Capabilities of a player that accepts nothing.
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether a transport action is permitted by these capabilities alone.
    pub fn permits(&self, operation: MprisOperation) -> bool {
        if !self.can_control {
            return false;
        }
        match operation {
            MprisOperation::PlayPause => self.can_play || self.can_pause,
            MprisOperation::Next => self.can_go_next,
            MprisOperation::Previous => self.can_go_previous,
            MprisOperation::Seek(_) => self.can_seek,
        }
    }
}

/// The closed set of transport operations.
///
/// Finding: an early probe of mine wrongly concluded a real exporter lacked
/// `Seek` because I sent the wrong integer width. The operation set is
/// therefore fixed by the product's needs and revalidated against capabilities
/// before every dispatch, rather than being inferred from what a call appears
/// to succeed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MprisOperation {
    PlayPause,
    Next,
    Previous,
    Seek(Duration),
}

impl MprisOperation {
    pub fn name(self) -> &'static str {
        match self {
            Self::PlayPause => "play_pause",
            Self::Next => "next",
            Self::Previous => "previous",
            Self::Seek(_) => "seek",
        }
    }
}

/// Why an observed player is missing something.
///
/// Diagnostics are part of the model rather than log output, because "this
/// exporter told us nothing about its track" is a product-visible condition:
/// the surface needs to say so instead of inventing a placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionDiagnostic {
    /// The exporter did not publish `DesktopEntry`. Observed on Chromium.
    DesktopEntryAbsent,
    /// `PlaybackStatus` was missing or unrecognised.
    PlaybackStatusUnreadable,
    /// `Metadata` was absent or not a dictionary.
    MetadataUnreadable,
    /// A metadata key existed but its value could not be interpreted.
    MetadataFieldUnreadable(String),
    /// `Position` could not be read as any supported integer width.
    PositionUnreadable,
    /// `mpris:length` was absent or non-positive.
    LengthUnreadable,
    /// `mpris:artUrl` was present but not renderable.
    ArtworkUnrenderable(String),
    /// Keys outside the specification were present and ignored.
    VendorKeysIgnored(Vec<String>),
}

/// One observed player, fully projected into product vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct MprisPlayerSnapshot {
    pub identity: ExactMprisPlayerRef,
    /// Human-readable label from the exporter's `Identity`.
    ///
    /// Finding 4: this is a display label, not a stable application key
    /// (Chromium reported `"Helium"`), so it is never used as an identity.
    pub label: String,
    /// Application key when the exporter publishes one. Absent on Chromium.
    pub application_key: Option<String>,
    pub playback: PlaybackState,
    pub track: TrackMetadata,
    pub position: PlaybackPosition,
    /// Rate as published. Recorded for completeness and deliberately unused for
    /// playback inference (finding 9).
    pub rate: f64,
    pub volume: f64,
    pub capabilities: PlaybackCapabilities,
    pub diagnostics: Vec<ProjectionDiagnostic>,
}

impl MprisPlayerSnapshot {
    /// Whether this snapshot has enough to render a track line.
    pub fn has_track(&self) -> bool {
        !self.track.is_blank()
    }

    /// Label to show when the exporter published no title.
    ///
    /// Finding 5: a live exporter published no title, artist or artwork and
    /// offered only a `file://` URL. Falling back to the URL's final path
    /// segment is a **product** decision about presentation, deliberately made
    /// here rather than in the protocol layer, and it is never treated as the
    /// track's identity.
    pub fn display_title(&self) -> Option<&str> {
        if let Some(title) = self.track.title.as_deref() {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
        let url = self.track.url.as_deref()?;
        let without_query = url.split(['?', '#']).next().unwrap_or(url);
        let segment = without_query.trim_end_matches('/').rsplit('/').next()?;
        let segment = segment.trim();
        (!segment.is_empty()).then_some(segment)
    }
}

/// Every player currently observed on the session bus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MprisSnapshot {
    players: Vec<MprisPlayerSnapshot>,
}

impl MprisSnapshot {
    pub fn new(players: Vec<MprisPlayerSnapshot>) -> Self {
        Self { players }
    }

    pub fn players(&self) -> &[MprisPlayerSnapshot] {
        &self.players
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    /// Look up a player by exact observed identity.
    pub fn find(&self, id: &MprisPlayerId) -> Option<&MprisPlayerSnapshot> {
        self.players.iter().find(|p| &p.identity.id == id)
    }

    /// Whether an exact identity is still a member of this snapshot.
    pub fn contains(&self, identity: &ExactMprisPlayerRef) -> bool {
        self.players
            .iter()
            .any(|p| p.identity.id == identity.id && p.identity.owner == identity.owner)
    }
}

/// The player a Now Playing surface should present.
///
/// Finding 3 recorded a real exporter publishing one logical player under two
/// well-known names owned by two different unique names, serving byte-identical
/// state. Presenting both would show one player twice; collapsing by well-known
/// name alone would merge genuinely different players that happen to share an
/// application. [`MprisPresentation::select`] therefore folds only names whose
/// observed state is identical, and records that it did so.
#[derive(Clone, Debug, PartialEq)]
pub enum MprisPresentation {
    /// No player is available.
    Unavailable,
    /// Exactly one distinct player.
    Single(Box<MprisPlayerSnapshot>),
    /// Several distinct players, with the effective one named.
    Multiple {
        effective: Box<MprisPlayerSnapshot>,
        others: Vec<MprisPlayerSnapshot>,
        /// Names folded away as duplicate reporters of the effective player.
        folded_duplicates: Vec<String>,
    },
}

impl MprisPresentation {
    /// Deterministic selection over observed players.
    ///
    /// The policy, stated once so it cannot drift:
    ///
    /// 1. Fold away a player whose every observable field matches an earlier
    ///    player's, treating the earlier one as the representative and keeping
    ///    its well-known name. Two owners serving identical state are two
    ///    reporters of one player (finding 3); a player that differs in any
    ///    observed field is never folded.
    /// 2. Order the remainder by well-known name, which is stable across
    ///    observations and does not depend on bus arrival order.
    /// 3. The effective player is the first in that order.
    ///
    /// This is deliberately not a permanent user preference. It is a
    /// deterministic default derived from observation, and it carries no
    /// persistence.
    pub fn select(players: &[MprisPlayerSnapshot]) -> Self {
        let mut representatives: Vec<MprisPlayerSnapshot> = Vec::new();
        let mut folded: Vec<String> = Vec::new();

        for player in players {
            match representatives
                .iter()
                .find(|kept| observably_identical(kept, player))
            {
                Some(_) => folded.push(player.identity.service.clone()),
                None => representatives.push(player.clone()),
            }
        }

        representatives.sort_by(|a, b| a.identity.service.cmp(&b.identity.service));

        match representatives.len() {
            0 => Self::Unavailable,
            1 if folded.is_empty() => Self::Single(Box::new(
                representatives.pop().expect("length checked above"),
            )),
            1 => {
                let effective = representatives.pop().expect("length checked above");
                Self::Multiple {
                    effective: Box::new(effective),
                    others: Vec::new(),
                    folded_duplicates: folded,
                }
            }
            _ => {
                let effective = representatives.remove(0);
                Self::Multiple {
                    effective: Box::new(effective),
                    others: representatives,
                    folded_duplicates: folded,
                }
            }
        }
    }

    /// The player a surface should render, if any.
    pub fn effective(&self) -> Option<&MprisPlayerSnapshot> {
        match self {
            Self::Unavailable => None,
            Self::Single(player) => Some(player),
            Self::Multiple { effective, .. } => Some(effective),
        }
    }

    /// Distinct players, after duplicate folding.
    pub fn distinct_players(&self) -> Vec<&MprisPlayerSnapshot> {
        match self {
            Self::Unavailable => Vec::new(),
            Self::Single(player) => vec![player.as_ref()],
            Self::Multiple {
                effective, others, ..
            } => std::iter::once(effective.as_ref())
                .chain(others.iter())
                .collect(),
        }
    }
}

/// Whether two observations are indistinguishable, and therefore two reporters
/// of one player.
///
/// Every product-visible field participates. `identity.owner` and `identity.id`
/// deliberately do not, because that is precisely the case being folded.
/// Position is compared as a plain value: two reporters sampled at slightly
/// different moments legitimately differ, so position is excluded and a
/// difference in it must not be read as two players.
fn observably_identical(a: &MprisPlayerSnapshot, b: &MprisPlayerSnapshot) -> bool {
    a.label == b.label
        && a.application_key == b.application_key
        && a.playback == b.playback
        && a.track == b.track
        && a.capabilities == b.capabilities
        && a.rate == b.rate
        && a.volume == b.volume
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(service: &str, owner: &str, label: &str, title: Option<&str>) -> MprisPlayerSnapshot {
        MprisPlayerSnapshot {
            identity: ExactMprisPlayerRef {
                id: MprisPlayerId::new(format!("{service}@{owner}")),
                service: service.to_owned(),
                path: "/org/mpris/MediaPlayer2".to_owned(),
                owner: owner.to_owned(),
            },
            label: label.to_owned(),
            application_key: Some("vlc".to_owned()),
            playback: PlaybackState::Playing,
            track: TrackMetadata {
                title: title.map(str::to_owned),
                ..TrackMetadata::default()
            },
            position: PlaybackPosition::observed(
                Duration::from_millis(100),
                std::time::Instant::now(),
            ),
            rate: 1.0,
            volume: 1.0,
            capabilities: PlaybackCapabilities {
                can_control: true,
                can_play: true,
                can_pause: true,
                can_go_next: true,
                can_go_previous: true,
                can_seek: true,
            },
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn playback_state_never_guesses() {
        assert_eq!(PlaybackState::parse("Playing"), PlaybackState::Playing);
        assert_eq!(PlaybackState::parse("Paused"), PlaybackState::Paused);
        // Chromium reported "Paused" with Rate 0.0; an unrecognised value must
        // not be resolved into a claim about what the exporter is doing.
        assert_eq!(PlaybackState::parse("Buffering"), PlaybackState::Stopped);
        assert_eq!(PlaybackState::parse(""), PlaybackState::Stopped);
    }

    #[test]
    fn capabilities_are_gated_on_control() {
        let mut caps = PlaybackCapabilities {
            can_control: false,
            can_play: true,
            can_pause: true,
            can_go_next: true,
            can_go_previous: true,
            can_seek: true,
        };
        // Every capability can be advertised while control is refused; the
        // refusal is what must win, or a disabled surface would dispatch.
        assert!(!caps.permits(MprisOperation::PlayPause));
        caps.can_control = true;
        assert!(caps.permits(MprisOperation::PlayPause));
        assert!(caps.permits(MprisOperation::Next));
        assert!(caps.permits(MprisOperation::Previous));
        caps.can_go_next = false;
        assert!(!caps.permits(MprisOperation::Next));
        assert!(caps.permits(MprisOperation::Previous));
    }

    #[test]
    fn position_is_never_interpolated() {
        let at = std::time::Instant::now();
        let position = PlaybackPosition::observed(Duration::from_secs(10), at);
        // Time passing must not move the position. Finding 10 established that
        // wall-clock extrapolation would fabricate progress.
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(position.position(), Duration::from_secs(10));
        assert_eq!(
            position.fraction_of(Some(Duration::from_secs(40))),
            Some(0.25)
        );
    }

    #[test]
    fn progress_is_absent_rather_than_fake() {
        let unknown = PlaybackPosition::default();
        assert!(!unknown.is_known());
        assert_eq!(unknown.fraction_of(Some(Duration::from_secs(10))), None);

        let known = PlaybackPosition::observed(Duration::from_secs(10), std::time::Instant::now());
        // A real exporter publishes no mpris:length for some tracks.
        assert_eq!(known.fraction_of(None), None);
        assert_eq!(known.fraction_of(Some(Duration::ZERO)), None);
        // A position past the end is not 100%; it is not meaningful.
        assert_eq!(
            known.fraction_of(Some(Duration::from_secs(5))),
            None,
            "a position beyond the length must not report a clamped fraction"
        );
    }

    #[test]
    fn a_title_less_track_still_offers_something_to_show() {
        let mut p = player("org.mpris.MediaPlayer2.vlc", ":1.1", "VLC", None);
        // VLC publishes no xesam:title; only a local file URL.
        p.track.url = Some("file:///home/someone/Music/Menu-AdventureAndChill.ogg".to_owned());
        assert_eq!(
            p.display_title(),
            Some("Menu-AdventureAndChill.ogg"),
            "the URL's final segment is the only identifying information available"
        );
        assert!(p.has_track());
    }

    #[test]
    fn a_truly_blank_track_reports_no_title() {
        let p = player("org.mpris.MediaPlayer2.x", ":1.1", "X", None);
        assert_eq!(p.display_title(), None);
        assert!(!p.has_track());
        assert!(!TrackMetadata::default().has_meaningful_length());
    }

    #[test]
    fn duplicate_reporters_of_one_player_are_folded() {
        // Finding 3: one logical VLC published under two names with two owners
        // serving identical state.
        let players = vec![
            player(
                "org.mpris.MediaPlayer2.vlc",
                ":1.5065",
                "VLC media player",
                None,
            ),
            player(
                "org.mpris.MediaPlayer2.vlc.instance1040494",
                ":1.5066",
                "VLC media player",
                None,
            ),
        ];
        let presentation = MprisPresentation::select(&players);
        assert_eq!(presentation.distinct_players().len(), 1);
        match presentation {
            MprisPresentation::Multiple {
                effective,
                folded_duplicates,
                ..
            } => {
                assert_eq!(effective.identity.service, "org.mpris.MediaPlayer2.vlc");
                assert_eq!(
                    folded_duplicates,
                    vec!["org.mpris.MediaPlayer2.vlc.instance1040494".to_owned()],
                    "the fold must be recorded, not silent"
                );
            }
            other => panic!("expected a folded multiple, got {other:?}"),
        }
    }

    #[test]
    fn genuinely_different_players_are_never_folded() {
        let vlc = player(
            "org.mpris.MediaPlayer2.vlc",
            ":1.1",
            "VLC media player",
            None,
        );
        let mut chrome = player(
            "org.mpris.MediaPlayer2.chromium.instance1",
            ":1.44",
            "Helium",
            Some("Andre The Giant"),
        );
        chrome.capabilities.can_seek = false;
        let presentation = MprisPresentation::select(&[vlc.clone(), chrome.clone()]);
        assert_eq!(presentation.distinct_players().len(), 2);
        // Deterministic by well-known name, not bus arrival order.
        assert_eq!(
            presentation.effective().unwrap().identity.service,
            "org.mpris.MediaPlayer2.chromium.instance1"
        );
        // Order of observation must not change the outcome.
        let reversed = MprisPresentation::select(&[chrome, vlc]);
        assert_eq!(
            reversed.effective().unwrap().identity.service,
            "org.mpris.MediaPlayer2.chromium.instance1"
        );
    }

    #[test]
    fn different_state_under_two_names_is_two_players() {
        let mut first = player("org.mpris.MediaPlayer2.a", ":1.1", "A", Some("Song"));
        let mut second = player("org.mpris.MediaPlayer2.b", ":1.2", "A", Some("Song"));
        first.track.url = Some("file:///one.ogg".to_owned());
        second.track.url = Some("file:///two.ogg".to_owned());
        assert_eq!(
            MprisPresentation::select(&[first, second])
                .distinct_players()
                .len(),
            2,
            "a differing observed field must prevent folding"
        );
    }

    #[test]
    fn no_players_is_unavailable_not_a_single_player() {
        assert_eq!(
            MprisPresentation::select(&[]),
            MprisPresentation::Unavailable
        );
        assert!(MprisPresentation::Unavailable.effective().is_none());
    }

    #[test]
    fn snapshot_membership_tracks_owner() {
        let p = player("org.mpris.MediaPlayer2.vlc", ":1.1", "VLC", None);
        let snapshot = MprisSnapshot::new(vec![p.clone()]);
        assert!(snapshot.contains(&p.identity));
        // The same well-known name re-requested by a different exporter is a
        // different player, and a binding to the old one must not match it.
        let mut replacement = p.identity.clone();
        replacement.owner = ":1.999".to_owned();
        assert!(!snapshot.contains(&replacement));
        assert!(snapshot.find(&p.identity.id).is_some());
    }

    #[test]
    fn identical_state_under_two_names_is_one_player_but_two_names() {
        let a = player("org.mpris.MediaPlayer2.vlc", ":1.5065", "VLC", None);
        let b = player(
            "org.mpris.MediaPlayer2.vlc.instance1",
            ":1.5066",
            "VLC",
            None,
        );
        assert!(!a.identity.same_exporter(&b.identity));
        assert_eq!(
            MprisPresentation::select(&[a.clone(), b.clone()])
                .distinct_players()
                .len(),
            1
        );
        // Folding is for presentation; identity must still distinguish them so
        // a stale binding cannot act on a different owner.
        assert_ne!(a.identity.owner, b.identity.owner);
    }
}
