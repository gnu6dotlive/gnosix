//! MPRIS2 projection and transport action.
//!
//! This is the only module that speaks MPRIS vocabulary. It discovers players
//! on the session bus, projects their state into [`crate::product_mpris`] types,
//! and dispatches the closed set of transport operations. Nothing here is
//! reachable from a widget except through [`MprisIntent`].
//!
//! Authority boundary: the set of reachable operations is closed. There is no
//! arbitrary method name, no arbitrary property write, no generic D-Bus
//! caller, and no way to construct an intent from a bus string.
//!
//! ## Why discovery probes rather than derives
//!
//! The MPRIS2 specification derives an exporter's object path from its
//! well-known name. On the operator's live session that derivation answered
//! nothing for every exporter present, while both answered at the fixed path
//! `/org/mpris/MediaPlayer2`. So [`discover_players`] probes candidates with a
//! real property read. This is recorded as finding 1 in
//! `docs/workstreams/mpris/01/reconnaissance.md`.

use std::collections::HashMap;
use std::time::Duration;

use zbus::zvariant::OwnedValue;

use crate::product_mpris::{
    ExactMprisPlayerRef, MprisArtwork, MprisOperation, MprisPlayerId, MprisPlayerSnapshot,
    MprisSnapshot, PlaybackCapabilities, PlaybackPosition, PlaybackState, ProjectionDiagnostic,
    TrackIdentity, TrackMetadata,
};

pub const MPRIS_INTERFACE: &str = "org.mpris.MediaPlayer2";
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const MPRIS_IFACE_NAME: &str = MPRIS_INTERFACE;

/// Protocol member names, named once so they cannot drift across call sites.
mod member {
    pub const PLAY_PAUSE: &str = "PlayPause";
    pub const NEXT: &str = "Next";
    pub const PREVIOUS: &str = "Previous";
    pub const SEEK: &str = "Seek";
}

/// The path both live exporters actually answered on.
///
/// Recorded rather than derived. It is the first candidate probed, but the
/// specification-derived path is also probed, because a conforming exporter is
/// entitled to use that one and a host that only knew this path would miss it.
const CANONICAL_PATH: &str = "/org/mpris/MediaPlayer2";

/// Maximum candidate object paths probed for one well-known name.
///
/// Bounded so a pathological bus cannot turn discovery into an unbounded scan.
/// Three candidates is what finding 1 required and is what finding 2's
/// chromium instance needs.
const MAX_PATH_CANDIDATES: usize = 3;

/// A typed transport intent bound to one exact observed player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MprisIntent {
    pub player: ExactMprisPlayerRef,
    pub operation: MprisOperation,
}

impl MprisIntent {
    pub fn play_pause(player: ExactMprisPlayerRef) -> Self {
        Self {
            player,
            operation: MprisOperation::PlayPause,
        }
    }

    pub fn next(player: ExactMprisPlayerRef) -> Self {
        Self {
            player,
            operation: MprisOperation::Next,
        }
    }

    pub fn previous(player: ExactMprisPlayerRef) -> Self {
        Self {
            player,
            operation: MprisOperation::Previous,
        }
    }
}

/// Why a transport action could not be completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MprisStaleReason {
    /// The player is no longer present in the current snapshot.
    PlayerAbsent,
    /// The well-known name now resolves to a different unique-name owner.
    ///
    /// A well-known name is not an identity. When it is released and
    /// re-requested by a different exporter, an action bound to the previous
    /// owner must not silently act on the replacement.
    OwnerReplaced,
    /// The player is present but does not advertise this capability.
    CapabilityMissing,
    /// The player exists but refuses all control.
    ControlRefused,
}

/// The normalised outcome of a transport action.
///
/// DONOR-15's lesson is encoded in the separation between
/// [`MprisTransportOutcome::Delivered`] and the local failure variants: a local
/// deserialization failure after a successful call must never be reported as a
/// remote refusal. Delivery is therefore decided by the actual method return
/// contract, and a local error afterwards is recorded as such.
#[derive(Clone, Debug, PartialEq)]
pub enum MprisTransportOutcome {
    /// The exporter accepted the call. Its method return contract is empty, so
    /// acceptance is the whole of the available evidence.
    Delivered { operation: MprisOperation },
    /// Refused locally before any call was made.
    RefusedStale {
        operation: MprisOperation,
        reason: MprisStaleReason,
    },
    /// The operation is not offered by this product surface.
    Unsupported { operation: MprisOperation },
    /// The player could not be reached at all.
    Unavailable {
        operation: MprisOperation,
        error: String,
    },
    /// The call was made and the exporter or bus reported failure.
    ///
    /// This is a *remote* failure: the exporter answered with an error, or the
    /// bus reported the call as rejected.
    TransportFailure {
        operation: MprisOperation,
        error: String,
    },
    /// The call was delivered, but interpreting the result failed locally.
    ///
    /// Kept distinct from `TransportFailure` precisely because DONOR-15 found
    /// that collapsing the two lets a local error masquerade as a remote
    /// refusal. A caller can see that delivery happened.
    DeliveredWithLocalError {
        operation: MprisOperation,
        error: String,
    },
}

impl MprisTransportOutcome {
    pub fn is_delivered(&self) -> bool {
        matches!(
            self,
            Self::Delivered { .. } | Self::DeliveredWithLocalError { .. }
        )
    }

    pub fn is_refused_stale(&self) -> bool {
        matches!(self, Self::RefusedStale { .. })
    }

    pub fn operation(&self) -> MprisOperation {
        match self {
            Self::Delivered { operation }
            | Self::RefusedStale { operation, .. }
            | Self::Unsupported { operation }
            | Self::Unavailable { operation, .. }
            | Self::TransportFailure { operation, .. }
            | Self::DeliveredWithLocalError { operation, .. } => *operation,
        }
    }

    /// Product-facing tone, matching the tray controller's vocabulary.
    pub fn tone(&self) -> &'static str {
        match self {
            Self::Delivered { .. } => "Success",
            Self::DeliveredWithLocalError { .. } => "Warning",
            Self::RefusedStale { .. } => "Warning",
            Self::Unsupported { .. } => "Neutral",
            Self::Unavailable { .. } => "Warning",
            Self::TransportFailure { .. } => "Warning",
        }
    }
}

/// Metadata keys outside the specification that must be tolerated.
///
/// Finding 5: both live exporters published vendor keys (`vlc:*`), which the
/// specification does not define. They are ignored and recorded.
const KNOWN_METADATA_KEYS: &[&str] = &[
    "xesam:title",
    "xesam:artist",
    "xesam:album",
    "xesam:url",
    "xesam:useCount",
    "xesam:playCount",
    "xesam:autoRating",
    "xesam:rating",
    "xesam:comment",
    "xesam:mimeTypes",
    "xesam:contentClass",
    "xesam:fullLength",
    "mpris:length",
    "mpris:artUrl",
    "mpris:trackid",
];

/// Derives the object path the specification defines, kept only as a probe
/// candidate. See the module docs for why it is not used on its own.
fn spec_derived_path(service: &str) -> String {
    format!("/{}", service.replace('.', "/"))
}

/// Candidate object paths for a well-known name, most likely first.
fn path_candidates(service: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push = |path: String| {
        if !candidates.contains(&path) && candidates.len() < MAX_PATH_CANDIDATES {
            candidates.push(path);
        }
    };
    push(CANONICAL_PATH.to_owned());
    push(spec_derived_path(service));

    // Chromium-style nesting: the base identity with the instance appended.
    if let Some(identity) = service.strip_prefix("org.mpris.MediaPlayer2.") {
        push(format!(
            "/org/mpris/MediaPlayer2/{}",
            identity.replace('.', "/")
        ));
    }
    candidates
}

/// Builds a properties proxy for a validated destination and path.
async fn properties_proxy<'a>(
    connection: &'a zbus::Connection,
    service: &str,
    path: &str,
) -> Result<zbus::Proxy<'a>, zbus::Error> {
    zbus::Proxy::new(
        connection,
        service.to_owned(),
        path.to_owned(),
        PROPERTIES_INTERFACE.to_owned(),
    )
    .await
}

/// Reads a player's properties, or `None` if it does not answer here.
async fn read_properties(
    connection: &zbus::Connection,
    service: &str,
    path: &str,
) -> Option<HashMap<String, OwnedValue>> {
    let proxy = properties_proxy(connection, service, path).await.ok()?;
    let all: HashMap<String, OwnedValue> = proxy.call("GetAll", &(PLAYER_INTERFACE,)).await.ok()?;
    if all.is_empty() {
        return None;
    }
    Some(all)
}

/// Reads one property of the base interface, tolerating its absence.
async fn read_base_property(
    connection: &zbus::Connection,
    service: &str,
    path: &str,
    name: &str,
) -> Option<String> {
    let proxy = properties_proxy(connection, service, path).await.ok()?;
    let value: String = proxy.call("Get", &(MPRIS_IFACE_NAME, name)).await.ok()?;
    Some(value)
}

fn string_field(props: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    let value = props.get(name)?;
    <String>::try_from(&**value)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Reads an integer across every width an exporter may have used.
///
/// Finding 8: a live exporter declared `Position` as int32 where the
/// specification requires int64. Refusing to read it would have made that
/// player unusable, so all widths are accepted and the value is normalised.
fn integer_field(props: &HashMap<String, OwnedValue>, name: &str) -> Option<i64> {
    let value = props.get(name)?;
    if let Ok(v) = i64::try_from(&**value) {
        return Some(v);
    }
    if let Ok(v) = i32::try_from(&**value) {
        return Some(i64::from(v));
    }
    if let Ok(v) = u32::try_from(&**value) {
        return Some(i64::from(v));
    }
    if let Ok(v) = u64::try_from(&**value) {
        return i64::try_from(v).ok();
    }
    if let Ok(v) = i16::try_from(&**value) {
        return Some(i64::from(v));
    }
    <u16>::try_from(&**value).ok().map(i64::from)
}

fn double_field(props: &HashMap<String, OwnedValue>, name: &str) -> Option<f64> {
    let value = props.get(name)?;
    if let Ok(v) = f64::try_from(&**value) {
        return Some(v);
    }
    integer_field(props, name).map(|v| v as f64)
}

fn bool_field(props: &HashMap<String, OwnedValue>, name: &str) -> Option<bool> {
    let value = props.get(name)?;
    <bool>::try_from(&**value).ok()
}

fn string_list_field(props: &HashMap<String, OwnedValue>, name: &str) -> Vec<String> {
    let Some(value) = props.get(name) else {
        return Vec::new();
    };
    let Ok(array) = <zbus::zvariant::Array<'_>>::try_from(&**value) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for index in 0..array.len() {
        if let Ok(Some(entry)) = array.get::<String>(index) {
            let trimmed = entry.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_owned());
            }
        }
    }
    out
}

/// Extracts the `a{sv}` metadata dictionary from a raw property value.
///
/// The dictionary is walked through zvariant's array form because that is the
/// only representation `a{sv}` decodes from, and a value of any other type is
/// reported as undecodable rather than coerced.
fn decode_metadata_value(value: &OwnedValue) -> Result<HashMap<String, OwnedValue>, ()> {
    // SURF-03C: MPRIS declares Metadata as a{sv}, a dictionary; each value
    // arrives wrapped in its variant. (An array of pairs is still accepted.)
    if let zbus::zvariant::Value::Dict(dict) = &**value {
        let mut fields = HashMap::new();
        for (key, entry) in dict.iter() {
            let Ok(key) = <&str>::try_from(key) else {
                continue;
            };
            let entry = match entry {
                zbus::zvariant::Value::Value(inner) => inner.as_ref(),
                other => other,
            };
            let Ok(owned) = entry.try_clone().map(OwnedValue::try_from) else {
                continue;
            };
            let Ok(owned) = owned else {
                continue;
            };
            fields.insert(key.to_owned(), owned);
        }
        return if fields.is_empty() {
            Err(())
        } else {
            Ok(fields)
        };
    }
    let array = <zbus::zvariant::Array<'_>>::try_from(&**value).map_err(|_| ())?;
    let mut fields = HashMap::new();
    for index in 0..array.len() {
        // Each entry is a (key, value) struct.
        let Ok(Some(entry)) = array.get::<(String, OwnedValue)>(index) else {
            continue;
        };
        fields.insert(entry.0, entry.1);
    }
    if fields.is_empty() {
        return Err(());
    }
    Ok(fields)
}

/// Narrows a raw metadata dictionary to the keys this product understands.
///
/// One malformed entry must not destroy the projection, so each key is decoded
/// independently and an undecodable value becomes a diagnostic.
fn decode_metadata(
    raw: &HashMap<String, OwnedValue>,
    diagnostics: &mut Vec<ProjectionDiagnostic>,
) -> TrackMetadata {
    let mut track = TrackMetadata::default();
    let mut vendor_keys: Vec<String> = Vec::new();

    for (key, value) in raw {
        if !KNOWN_METADATA_KEYS.contains(&key.as_str()) {
            vendor_keys.push(key.clone());
            continue;
        }
        match key.as_str() {
            "xesam:title" => {
                track.title = <String>::try_from(&**value)
                    .ok()
                    .map(|s| s.trim().to_owned());
                if track.title.as_deref().is_some_and(str::is_empty) {
                    track.title = None;
                }
            }
            "xesam:album" => {
                track.album = <String>::try_from(&**value)
                    .ok()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty());
            }
            "xesam:artist" => track.artists = string_list_field(raw, "xesam:artist"),
            "xesam:url" => {
                track.url = <String>::try_from(&**value)
                    .ok()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty());
            }
            "mpris:trackid" => {
                track.identity = TrackIdentity(
                    <String>::try_from(&**value)
                        .ok()
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty()),
                );
            }
            "mpris:length" => {
                track.length = integer_field(raw, "mpris:length")
                    .and_then(|nanos| u64::try_from(nanos).ok())
                    .map(Duration::from_nanos)
                    .filter(|d| !d.is_zero());
                if track.length.is_none() {
                    diagnostics.push(ProjectionDiagnostic::LengthUnreadable);
                }
            }
            "mpris:artUrl" => {
                track.artwork = match <String>::try_from(&**value) {
                    Ok(url) => {
                        let trimmed = url.trim().to_owned();
                        if trimmed.is_empty() {
                            None
                        } else if trimmed.starts_with("file://")
                            || trimmed.starts_with("http://")
                            || trimmed.starts_with("https://")
                        {
                            Some(MprisArtwork::Renderable(trimmed))
                        } else {
                            diagnostics
                                .push(ProjectionDiagnostic::ArtworkUnrenderable(trimmed.clone()));
                            Some(MprisArtwork::Unrenderable(trimmed))
                        }
                    }
                    Err(_) => {
                        diagnostics.push(ProjectionDiagnostic::MetadataFieldUnreadable(
                            "mpris:artUrl".to_owned(),
                        ));
                        None
                    }
                };
            }
            _ => {}
        }
    }

    vendor_keys.sort();
    if !vendor_keys.is_empty() {
        diagnostics.push(ProjectionDiagnostic::VendorKeysIgnored(vendor_keys));
    }
    track
}

/// Projects one exporter's properties into a player snapshot.
async fn project_player(
    connection: &zbus::Connection,
    service: &str,
    path: &str,
    owner: &str,
    props: HashMap<String, OwnedValue>,
    now: std::time::Instant,
) -> MprisPlayerSnapshot {
    let mut diagnostics = Vec::new();

    let playback = match string_field(&props, "PlaybackStatus") {
        Some(value) => PlaybackState::parse(&value),
        None => {
            diagnostics.push(ProjectionDiagnostic::PlaybackStatusUnreadable);
            PlaybackState::default()
        }
    };

    let track = match props.get("Metadata").map(decode_metadata_value) {
        Some(Ok(fields)) => decode_metadata(&fields, &mut diagnostics),
        Some(Err(())) => {
            // A malformed metadata value costs the track fields, never the
            // player.
            diagnostics.push(ProjectionDiagnostic::MetadataUnreadable);
            TrackMetadata::default()
        }
        None => {
            diagnostics.push(ProjectionDiagnostic::MetadataUnreadable);
            TrackMetadata::default()
        }
    };

    let position = match integer_field(&props, "Position") {
        Some(nanos) => {
            let nanos = u64::try_from(nanos).unwrap_or(0);
            PlaybackPosition::observed(Duration::from_nanos(nanos), now)
        }
        None => {
            diagnostics.push(ProjectionDiagnostic::PositionUnreadable);
            PlaybackPosition::default()
        }
    };

    let capabilities = PlaybackCapabilities {
        can_control: bool_field(&props, "CanControl").unwrap_or(false),
        can_play: bool_field(&props, "CanPlay").unwrap_or(false),
        can_pause: bool_field(&props, "CanPause").unwrap_or(false),
        can_go_next: bool_field(&props, "CanGoNext").unwrap_or(false),
        can_go_previous: bool_field(&props, "CanGoPrevious").unwrap_or(false),
        can_seek: bool_field(&props, "CanSeek").unwrap_or(false),
    };

    let label = read_base_property(connection, service, path, "Identity")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| service.to_owned());

    let application_key = read_base_property(connection, service, path, "DesktopEntry")
        .await
        .filter(|value| !value.trim().is_empty());
    if application_key.is_none() {
        // Finding 4: observed on Chromium, which answers Identity but not
        // DesktopEntry.
        diagnostics.push(ProjectionDiagnostic::DesktopEntryAbsent);
    }

    MprisPlayerSnapshot {
        identity: ExactMprisPlayerRef {
            // Keyed on the unique-name owner so a re-requested well-known name
            // yields a different instance, which is what makes a stale binding
            // detectable.
            id: MprisPlayerId::new(format!("{service}@{owner}")),
            service: service.to_owned(),
            path: path.to_owned(),
            owner: owner.to_owned(),
        },
        label,
        application_key,
        playback,
        track,
        position,
        // Recorded, never used to infer playback: finding 9 observed 0.0 on a
        // paused player.
        rate: double_field(&props, "Rate").unwrap_or(1.0),
        volume: double_field(&props, "Volume")
            .unwrap_or(1.0)
            .clamp(0.0, 1.0),
        capabilities,
        diagnostics,
    }
}

/// Enumerates every MPRIS2 player currently on the bus.
///
/// Each well-known name is probed against candidate object paths with a real
/// property read, because introspection cannot be used for discovery (it
/// answers with an empty node for paths that do serve properties) and the
/// specification's path derivation found nothing live.
pub async fn discover_players(connection: &zbus::Connection) -> MprisSnapshot {
    let now = std::time::Instant::now();

    let services = match list_player_names(connection).await {
        Some(names) => names,
        None => return MprisSnapshot::default(),
    };

    // A qualification may name the player it intends to exercise. Restricting
    // the scan keeps the gate from reading bystander exporters, which both
    // wastes bus traffic and pollutes the journal the oracle judges.
    if let Some(target) = crate::product_mpris_session::qualification_target() {
        let names: Vec<String> = services
            .into_iter()
            .filter(|service| *service == target)
            .collect();
        if names.is_empty() {
            return MprisSnapshot::default();
        }
        return collect_players(connection, names, now).await;
    }

    collect_players(connection, services, now).await
}

/// Reads every named player, skipping any that cannot be reached.
async fn collect_players(
    connection: &zbus::Connection,
    services: Vec<String>,
    now: std::time::Instant,
) -> MprisSnapshot {
    let mut players = Vec::new();

    for service in services {
        let Some(owner) = crate::interaction_lease::unique_name_owner(connection, &service).await
        else {
            continue;
        };
        // The owner is resolved before the read and again implicitly by the
        // read itself; a player whose owner vanished mid-probe simply yields
        // nothing, which is the correct outcome for a player that disappeared.
        let mut found = None;
        for path in path_candidates(&service) {
            if let Some(props) = read_properties(connection, &service, &path).await {
                found = Some((path, props));
                break;
            }
        }
        let Some((path, props)) = found else {
            continue;
        };
        players.push(project_player(connection, &service, &path, &owner, props, now).await);
    }

    // Deterministic order so two observations of the same bus state compare
    // equal, which is what lets a stale-binding check mean anything.
    players.sort_by(|a, b| {
        (a.identity.service.as_str(), a.identity.owner.as_str())
            .cmp(&(b.identity.service.as_str(), b.identity.owner.as_str()))
    });
    MprisSnapshot::new(players)
}

/// Lists well-known MPRIS2 names from the bus.
async fn list_player_names(connection: &zbus::Connection) -> Option<Vec<String>> {
    let proxy = zbus::Proxy::new(
        connection,
        "org.freedesktop.DBus".to_owned(),
        "/org/freedesktop/DBus".to_owned(),
        "org.freedesktop.DBus".to_owned(),
    )
    .await
    .ok()?;
    let names: Vec<String> = proxy.call("ListNames", &()).await.ok()?;
    let mut players: Vec<String> = names
        .into_iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
        .collect();
    players.sort();
    Some(players)
}

/// Re-reads one known player's state, used by the observation loop.
///
/// Returns `None` when the player can no longer be reached, which is how
/// disappearance is detected.
pub async fn refresh_player(
    connection: &zbus::Connection,
    identity: &ExactMprisPlayerRef,
) -> Option<MprisPlayerSnapshot> {
    let owner = crate::interaction_lease::unique_name_owner(connection, &identity.service).await?;
    if owner != identity.owner {
        // The name was re-requested by a different exporter. Reporting this as
        // a different player is the whole point of carrying the owner.
        return None;
    }
    let props = read_properties(connection, &identity.service, &identity.path).await?;
    Some(
        project_player(
            connection,
            &identity.service,
            &identity.path,
            &owner,
            props,
            std::time::Instant::now(),
        )
        .await,
    )
}

/// Operations this product surface actually offers.
///
/// Seek is excluded because no surface in this vertical exercises it. The
/// mission scopes Seek to surfaces that use it, and offering a transport action
/// the product cannot present would be a capability the user can trigger but
/// never see. Excluding it here means a Seek intent is refused as
/// [`MprisTransportOutcome::Unsupported`] before any connection work, which is
/// also what makes the refusal testable against a journal that would otherwise
/// record the call.
fn is_offered(operation: MprisOperation) -> bool {
    matches!(
        operation,
        MprisOperation::PlayPause | MprisOperation::Next | MprisOperation::Previous
    )
}

/// Dispatches one typed intent, revalidating identity and capability first.
///
/// No call is made unless the operation is one this product offers, the
/// intent's exact player is still the observed player, and the player still
/// advertises the capability the operation needs. That is what makes a stale
/// rendered control refuse rather than retarget.
pub async fn dispatch(
    connection: &zbus::Connection,
    snapshot: &MprisSnapshot,
    intent: &MprisIntent,
) -> MprisTransportOutcome {
    let operation = intent.operation;

    if !is_offered(operation) {
        return MprisTransportOutcome::Unsupported { operation };
    }

    let Some(player) = snapshot.find(&intent.player.id) else {
        return MprisTransportOutcome::RefusedStale {
            operation,
            reason: MprisStaleReason::PlayerAbsent,
        };
    };

    if player.identity.owner != intent.player.owner {
        return MprisTransportOutcome::RefusedStale {
            operation,
            reason: MprisStaleReason::OwnerReplaced,
        };
    }

    if !player.capabilities.can_control {
        return MprisTransportOutcome::RefusedStale {
            operation,
            reason: MprisStaleReason::ControlRefused,
        };
    }

    if !player.capabilities.permits(operation) {
        return MprisTransportOutcome::RefusedStale {
            operation,
            reason: MprisStaleReason::CapabilityMissing,
        };
    }

    let proxy = match zbus::Proxy::new(
        connection,
        player.identity.service.clone(),
        player.identity.path.clone(),
        PLAYER_INTERFACE.to_owned(),
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(error) => {
            return MprisTransportOutcome::Unavailable {
                operation,
                error: error.to_string(),
            };
        }
    };

    let call = match operation {
        MprisOperation::PlayPause => proxy.call::<_, _, ()>(member::PLAY_PAUSE, &()).await,
        MprisOperation::Next => proxy.call::<_, _, ()>(member::NEXT, &()).await,
        MprisOperation::Previous => proxy.call::<_, _, ()>(member::PREVIOUS, &()).await,
        MprisOperation::Seek(offset) => {
            let micros = i64::try_from(offset.as_micros()).unwrap_or(i64::MAX);
            proxy.call::<_, _, ()>(member::SEEK, &(micros,)).await
        }
    };

    match call {
        Ok(()) => {
            // The method's return contract is empty, so a successful return is
            // the full extent of the evidence available that the exporter
            // accepted the call. Nothing further is inferred from a local
            // read-back, which is what DONOR-15 showed can lie.
            MprisTransportOutcome::Delivered { operation }
        }
        // A remote error name is the exporter's own answer. The detail string
        // is optional in D-Bus, so the name is the reliable part and a missing
        // detail must not be read as success.
        Err(zbus::Error::MethodError(name, detail, _)) => MprisTransportOutcome::TransportFailure {
            operation,
            error: match detail {
                Some(detail) => format!("{name}: {detail}"),
                None => name.to_string(),
            },
        },
        Err(error) => MprisTransportOutcome::Unavailable {
            operation,
            error: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_path_is_probed_before_the_derived_one() {
        // Finding 1: live exporters answered only at the canonical path.
        let candidates = path_candidates("org.mpris.MediaPlayer2.vlc");
        assert_eq!(candidates[0], CANONICAL_PATH);
        assert!(
            candidates.contains(&"/org/mpris/MediaPlayer2/vlc".to_owned()),
            "a conforming exporter's derived path must still be probed"
        );
    }

    #[test]
    fn candidate_paths_are_bounded_and_distinct() {
        let candidates = path_candidates("org.mpris.MediaPlayer2.chromium.instance3953");
        assert!(candidates.len() <= MAX_PATH_CANDIDATES);
        let mut sorted = candidates.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            candidates.len(),
            "candidates must be distinct"
        );
    }

    #[test]
    fn chromium_style_nesting_is_probed() {
        let candidates = path_candidates("org.mpris.MediaPlayer2.chromium.instance3953");
        assert!(
            candidates
                .iter()
                .any(|c| c.starts_with("/org/mpris/MediaPlayer2/chromium"))
        );
    }

    #[test]
    fn outcome_keeps_delivery_distinct_from_local_error() {
        // DONOR-15: collapsing these let a local failure read as a remote
        // refusal. A delivered call with a local error is still delivered.
        let delivered_with_local_error = MprisTransportOutcome::DeliveredWithLocalError {
            operation: MprisOperation::Next,
            error: "deserialization".to_owned(),
        };
        assert!(delivered_with_local_error.is_delivered());
        assert!(
            !MprisTransportOutcome::TransportFailure {
                operation: MprisOperation::Next,
                error: "remote said no".to_owned(),
            }
            .is_delivered()
        );
        assert!(
            MprisTransportOutcome::RefusedStale {
                operation: MprisOperation::Next,
                reason: MprisStaleReason::OwnerReplaced,
            }
            .is_refused_stale()
        );
    }

    #[test]
    fn every_outcome_reports_its_operation() {
        for outcome in [
            MprisTransportOutcome::Delivered {
                operation: MprisOperation::PlayPause,
            },
            MprisTransportOutcome::RefusedStale {
                operation: MprisOperation::Next,
                reason: MprisStaleReason::PlayerAbsent,
            },
            MprisTransportOutcome::Unsupported {
                operation: MprisOperation::Previous,
            },
            MprisTransportOutcome::Unavailable {
                operation: MprisOperation::PlayPause,
                error: "x".to_owned(),
            },
            MprisTransportOutcome::TransportFailure {
                operation: MprisOperation::Next,
                error: "x".to_owned(),
            },
            MprisTransportOutcome::DeliveredWithLocalError {
                operation: MprisOperation::Previous,
                error: "x".to_owned(),
            },
        ] {
            assert!(matches!(
                outcome.operation(),
                MprisOperation::PlayPause | MprisOperation::Next | MprisOperation::Previous
            ));
        }
    }

    #[test]
    fn intents_carry_no_protocol_vocabulary() {
        let player = ExactMprisPlayerRef {
            id: MprisPlayerId::new("org.mpris.MediaPlayer2.vlc@:1.1"),
            service: "org.mpris.MediaPlayer2.vlc".to_owned(),
            path: CANONICAL_PATH.to_owned(),
            owner: ":1.1".to_owned(),
        };
        // The vocabulary is closed: three operations, constructible without a
        // method name.
        assert_eq!(
            MprisIntent::play_pause(player.clone()).operation,
            MprisOperation::PlayPause
        );
        assert_eq!(
            MprisIntent::next(player.clone()).operation,
            MprisOperation::Next
        );
        assert_eq!(
            MprisIntent::previous(player).operation,
            MprisOperation::Previous
        );
        assert_eq!(member::PLAY_PAUSE, "PlayPause");
    }

    #[test]
    fn spec_metadata_is_a_dictionary_and_decodes() {
        // MPRIS declares Metadata as a{sv}: a dictionary, as players send it.
        let mut fields = std::collections::HashMap::<&str, zbus::zvariant::Value>::new();
        fields.insert("xesam:title", zbus::zvariant::Value::from("A title"));
        fields.insert(
            "xesam:artist",
            zbus::zvariant::Value::from(vec!["One".to_owned(), "Two".to_owned()]),
        );
        fields.insert("mpris:length", zbus::zvariant::Value::from(240_000_000_i64));
        let value = OwnedValue::try_from(zbus::zvariant::Value::from(fields)).unwrap();
        let raw = decode_metadata_value(&value).expect("a{sv} decodes");
        let mut diagnostics = Vec::new();
        let track = decode_metadata(&raw, &mut diagnostics);
        assert_eq!(track.title.as_deref(), Some("A title"));
        assert_eq!(track.artists, ["One", "Two"]);
        assert!(track.length.is_some());
    }
}
