//! MPRIS session state and the player observation loop.
//!
//! Holds the observed player snapshot, the live bus connection, and the
//! observation loop that keeps the snapshot current. Mirrors the tray session
//! split so the Now Playing surface consumes typed state without owning
//! transport.
//!
//! ## Why the observation loop polls
//!
//! Section 8 of the mission asks how live exporters actually report position.
//! They do not: reconnaissance observed zero `PropertiesChanged` signals and
//! zero position-bearing signals across three live names over 16 seconds, while
//! `Position` advanced on 12 of 21 consecutive re-reads. So progress is only
//! observable by re-reading a property, and this loop does that on a bounded
//! cadence.
//!
//! ## Why the cadence is slow
//!
//! Because a position read is a bus round trip per player per tick, and no
//! exporter signals movement, polling faster buys nothing the exporter told us.
//! The interval is deliberately coarse, and the loop stops entirely when no
//! player is present.

use gpui::{AppContext, Context, Entity, SharedString};

use crate::product_mpris::{MprisPlayerSnapshot, MprisPresentation, MprisSnapshot};
use crate::product_mpris_transport::{MprisIntent, MprisTransportOutcome};

/// Interval between bounded re-reads of the observed players.
///
/// Chosen from the reconnaissance evidence rather than for responsiveness: no
/// live exporter signals position, so a faster tick would multiply bus traffic
/// without adding information.
const OBSERVATION_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// MPRIS session state, published to the product layer.
pub struct MprisSession {
    snapshot: MprisSnapshot,
    connection: Option<zbus::Connection>,
    last_outcome: Option<MprisTransportOutcome>,
    /// Whether the observation loop is running.
    observing: bool,
}

/// Process-global handle to the single MPRIS session.
///
/// Mirrors the tray session split: one session per process, so two outputs
/// rendering the same product state cannot each own an observation loop or
/// dispatch independently.
#[derive(Clone)]
struct MprisGlobal(Entity<MprisSession>);
impl gpui::Global for MprisGlobal {}

impl MprisSession {
    /// The shared session, created on first use.
    ///
    /// Callable from an `AsyncApp` as well as an `App`: an async context may
    /// update the global registry but cannot construct entities directly, so
    /// the session is created eagerly by the scene runtime and resolved here.
    pub fn shared(cx: &mut gpui::App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<MprisGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self {
            snapshot: MprisSnapshot::default(),
            connection: None,
            last_outcome: None,
            observing: false,
        });
        cx.set_global(MprisGlobal(entity.clone()));
        entity
    }

    /// The shared session from an async context.
    ///
    /// Panics if no session exists yet, because an async context can read the
    /// global registry but cannot create an entity in it. The scene runtime
    /// publishes the session at startup, so every async caller has one.
    pub fn shared_async(cx: &mut gpui::AsyncApp) -> Entity<Self> {
        cx.try_read_global(|global: &MprisGlobal, _app| global.0.clone())
            .expect("the scene runtime publishes the MPRIS session at startup")
    }

    /// The observed player snapshot.
    pub fn snapshot(&self) -> &MprisSnapshot {
        &self.snapshot
    }

    /// What a Now Playing surface should present, after duplicate folding.
    pub fn presentation(&self) -> MprisPresentation {
        MprisPresentation::select(self.snapshot.players())
    }

    /// The live bus connection, once discovery has established one.
    pub fn connection(&self) -> Option<&zbus::Connection> {
        self.connection.as_ref()
    }

    pub fn last_outcome(&self) -> Option<&MprisTransportOutcome> {
        self.last_outcome.as_ref()
    }

    pub fn is_observing(&self) -> bool {
        self.observing
    }

    pub fn set_snapshot(&mut self, snapshot: MprisSnapshot, cx: &mut Context<Self>) {
        // Only notify on a real change, so an unchanged poll does not repaint.
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot.clone();
        // Prime the relay hub at observation time, so a forwarded intent is
        // never answered "not ready" merely because the owner has not rendered
        // or clicked since observing the player.
        if let Some(connection) = &self.connection {
            crate::product_mpris_relay::publish(connection.clone(), snapshot);
        }
        cx.notify();
    }

    pub fn set_connection(&mut self, connection: zbus::Connection, cx: &mut Context<Self>) {
        self.connection = Some(connection);
        cx.notify();
    }

    pub fn set_observing(&mut self, observing: bool, cx: &mut Context<Self>) {
        if self.observing == observing {
            return;
        }
        self.observing = observing;
        cx.notify();
    }

    pub fn set_last_outcome(
        &mut self,
        outcome: Option<MprisTransportOutcome>,
        cx: &mut Context<Self>,
    ) {
        self.last_outcome = outcome;
        cx.notify();
    }

    /// Receipt line for the qualification log.
    pub fn receipt(&self) -> SharedString {
        let presentation = self.presentation();
        let players = self.snapshot.len();
        let detail = match &presentation {
            MprisPresentation::Unavailable => "effective=none".to_owned(),
            MprisPresentation::Single(player) => {
                format!("effective={} state={:?}", player.label, player.playback)
            }
            MprisPresentation::Multiple {
                effective,
                others,
                folded_duplicates,
            } => format!(
                "effective={} state={:?} others={} folded_duplicates={}",
                effective.label,
                effective.playback,
                others.len(),
                folded_duplicates.len()
            ),
        };
        SharedString::from(format!("GNOS_UX_MPRIS_RECEIPT players={players} {detail}"))
    }
}

/// Runs one bounded discovery pass and publishes the result.
async fn observe_once(session: &Entity<MprisSession>, cx: &mut gpui::AsyncApp) {
    let connection = session.read_with(&*cx, |this, _cx| this.connection().cloned());
    let Some(connection) = connection else {
        return;
    };

    let snapshot = crate::product_mpris_transport::discover_players(&connection).await;
    let players = snapshot.len();
    let observed: Vec<MprisPlayerSnapshot> = snapshot.players().to_vec();

    session.update(cx, |this, cx| {
        this.set_snapshot(snapshot, cx);
        this.set_observing(!observed.is_empty(), cx);
    });

    let folded = MprisPresentation::select(&observed);
    println!(
        "GNOS_UX_MPRIS_OBSERVED players={players} effective={} folded={}",
        folded
            .effective()
            .map(|p| p.label.as_str())
            .unwrap_or("<none>"),
        match &folded {
            MprisPresentation::Multiple {
                folded_duplicates, ..
            } => folded_duplicates.len(),
            _ => 0,
        }
    );
}

/// The well-known name a qualification intends to exercise.
///
/// Recorded from the qualification request rather than discovered, because a
/// live session may hold several players. Without an explicit target the
/// deterministic selection policy would drive whichever player sorts first,
/// which on a real machine is a bystander rather than the exporter under test.
static QUALIFICATION_TARGET: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Records which player a qualification should exercise.
pub fn set_qualification_target(service: impl Into<String>) {
    let mut guard = QUALIFICATION_TARGET
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(service.into());
}

/// The recorded qualification target, if any.
pub fn qualification_target() -> Option<String> {
    QUALIFICATION_TARGET
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Establishes the session bus connection and starts observation, if not already.
///
/// Unlike the tray, no watcher registration is involved: MPRIS players are
/// discovered by enumerating the bus, so there is nothing to register for and
/// nothing to be a secondary host alongside.
pub fn ensure_connection(this: &mut MprisSession, cx: &mut Context<MprisSession>) {
    if this.connection().is_some() {
        return;
    }
    let session = cx.entity();
    start_observation(&session, cx);
    cx.spawn(
        async move |_this, task_cx| match zbus::Connection::session().await {
            Ok(connection) => {
                task_cx.update(|cx| {
                    session.update(cx, |this, cx| {
                        this.set_connection(connection, cx);
                    });
                });
                println!("GNOS_UX_MPRIS_CONNECTED");
            }
            Err(error) => {
                println!("GNOS_UX_MPRIS_CONNECT_FAILED error={error}");
            }
        },
    )
    .detach();
}

/// Keeps the observed snapshot current.
///
/// The loop owns no policy: it re-reads on a bounded interval and stops once no
/// player is present, so an idle desktop issues no MPRIS traffic at all.
pub fn start_observation(session: &Entity<MprisSession>, cx: &mut gpui::Context<MprisSession>) {
    let session = session.clone();
    cx.spawn(async move |_this, task_cx| {
        loop {
            task_cx
                .background_executor()
                .timer(OBSERVATION_INTERVAL)
                .await;
            observe_once(&session, task_cx).await;
            let still_present =
                session.read_with(&*task_cx, |this, _cx| !this.snapshot().is_empty());
            if !still_present {
                session.update(task_cx, |this, cx| this.set_observing(false, cx));
                break;
            }
        }
    })
    .detach();
}

/// Dispatches one typed intent through the revalidating transport path.
pub async fn dispatch_intent(
    session: &Entity<MprisSession>,
    intent: MprisIntent,
    cx: &mut gpui::AsyncApp,
) -> MprisTransportOutcome {
    let (connection, snapshot) = session.read_with(&*cx, |this, _cx| {
        (this.connection().cloned(), this.snapshot().clone())
    });
    let Some(connection) = connection else {
        return MprisTransportOutcome::Unavailable {
            operation: intent.operation,
            error: "no bus connection".to_owned(),
        };
    };
    let outcome = crate::product_mpris_transport::dispatch(&connection, &snapshot, &intent).await;
    session.update(cx, |this, cx| {
        this.set_last_outcome(Some(outcome.clone()), cx);
    });
    println!(
        "GNOS_UX_MPRIS_INTENT operation={} outcome={:?} tone={}",
        intent.operation.name(),
        outcome,
        outcome.tone()
    );
    outcome
}

/// Whether a rendered control for `operation` should be interactive.
///
/// Rechecked at the moment of use rather than trusted from render time, so a
/// capability that changed since the surface was painted cannot receive a
/// click.
pub fn is_actionable(
    player: &MprisPlayerSnapshot,
    operation: crate::product_mpris::MprisOperation,
) -> bool {
    player.capabilities.permits(operation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_mpris::{
        ExactMprisPlayerRef, MprisPlayerId, PlaybackCapabilities, PlaybackState, TrackMetadata,
    };

    fn player(
        service: &str,
        owner: &str,
        capabilities: PlaybackCapabilities,
    ) -> MprisPlayerSnapshot {
        MprisPlayerSnapshot {
            identity: ExactMprisPlayerRef {
                id: MprisPlayerId::new(format!("{service}@{owner}")),
                service: service.to_owned(),
                path: "/org/mpris/MediaPlayer2".to_owned(),
                owner: owner.to_owned(),
            },
            label: "test".to_owned(),
            application_key: None,
            playback: PlaybackState::Playing,
            track: TrackMetadata::default(),
            position: Default::default(),
            rate: 1.0,
            volume: 1.0,
            capabilities,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn observation_interval_is_not_aggressive() {
        // No live exporter signals position, so a fast tick only multiplies bus
        // traffic. The bound is a deliberate product decision.
        assert!(
            OBSERVATION_INTERVAL >= std::time::Duration::from_secs(1),
            "polling must not be aggressive"
        );
    }

    #[test]
    fn actionability_follows_capabilities_at_use_time() {
        let disabled = player(
            "org.mpris.MediaPlayer2.x",
            ":1.1",
            PlaybackCapabilities::none(),
        );
        assert!(!is_actionable(
            &disabled,
            crate::product_mpris::MprisOperation::PlayPause
        ));
        let enabled = player(
            "org.mpris.MediaPlayer2.y",
            ":1.2",
            PlaybackCapabilities {
                can_control: true,
                can_play: true,
                can_pause: true,
                can_go_next: true,
                can_go_previous: true,
                can_seek: true,
            },
        );
        assert!(is_actionable(
            &enabled,
            crate::product_mpris::MprisOperation::Next
        ));
    }
}
