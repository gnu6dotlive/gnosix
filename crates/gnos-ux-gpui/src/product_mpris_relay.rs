//! Cross-process routing for Now Playing transport intents.
//!
//! Mirrors `product_tray_relay.rs` deliberately. The invariant is the same:
//!
//! ```text
//! VISIBLE + ENABLED  ->  LOCAL INTERACTION SINK EXISTS
//! ```
//!
//! The lease elects an executor, not an interactive output. Every enabled
//! surface installs a sink; a non-owning process relays the exact typed intent
//! to the lease owner, which performs its normal revalidation and dispatch. The
//! interface is Now Playing specific (`PlayPause`, `Next`, `Previous`), never a
//! generic dispatch. An ambiguous relay failure is reported and never retried.

use std::sync::{Arc, OnceLock};

use crate::product_mpris::{ExactMprisPlayerRef, MprisOperation, MprisSnapshot};
use crate::product_mpris_transport::{MprisIntent, MprisStaleReason, MprisTransportOutcome};

/// Object path the owner serves the relay on.
pub const RELAY_PATH: &str = "/org/gnosix/NowPlayingRelay";
/// Interface the owner serves the relay on.
pub const RELAY_INTERFACE: &str = "org.gnosix.NowPlayingRelay";

#[derive(Default)]
struct HubState {
    connection: Option<zbus::Connection>,
    snapshot: MprisSnapshot,
}

/// Process-local relay hub. Only meaningful in the process that owns the lease.
#[derive(Default)]
pub struct NowPlayingRelayHub {
    state: std::sync::Mutex<HubState>,
}

impl NowPlayingRelayHub {
    fn update(&self, connection: zbus::Connection, snapshot: MprisSnapshot) {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.connection = Some(connection);
        guard.snapshot = snapshot;
    }

    fn ready(&self) -> Option<(zbus::Connection, MprisSnapshot)> {
        let guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Some((guard.connection.clone()?, guard.snapshot.clone()))
    }
}

static HUB: OnceLock<Arc<NowPlayingRelayHub>> = OnceLock::new();

pub fn hub() -> Arc<NowPlayingRelayHub> {
    HUB.get_or_init(|| Arc::new(NowPlayingRelayHub::default()))
        .clone()
}

/// Mirrors the owner's live dispatch state into the hub.
pub fn publish(connection: zbus::Connection, snapshot: MprisSnapshot) {
    hub().update(connection, snapshot);
}

struct NowPlayingRelayServer {
    hub: Arc<NowPlayingRelayHub>,
}

#[zbus::interface(name = "org.gnosix.NowPlayingRelay")]
impl NowPlayingRelayServer {
    async fn play_pause(&self, id: String, service: String, path: String, owner: String) -> String {
        self.execute(operation_intent(
            MprisOperation::PlayPause,
            id,
            service,
            path,
            owner,
        ))
        .await
    }

    async fn next(&self, id: String, service: String, path: String, owner: String) -> String {
        self.execute(operation_intent(
            MprisOperation::Next,
            id,
            service,
            path,
            owner,
        ))
        .await
    }

    async fn previous(&self, id: String, service: String, path: String, owner: String) -> String {
        self.execute(operation_intent(
            MprisOperation::Previous,
            id,
            service,
            path,
            owner,
        ))
        .await
    }
}

fn operation_intent(
    operation: MprisOperation,
    id: String,
    service: String,
    path: String,
    owner: String,
) -> MprisIntent {
    MprisIntent {
        player: ExactMprisPlayerRef {
            id: crate::product_mpris::MprisPlayerId::new(id),
            service,
            path,
            owner,
        },
        operation,
    }
}

impl NowPlayingRelayServer {
    async fn execute(&self, intent: MprisIntent) -> String {
        let Some((connection, snapshot)) = self.hub.ready() else {
            return encode(&MprisTransportOutcome::Unavailable {
                operation: intent.operation,
                error: "owner hub not ready".to_owned(),
            });
        };
        let outcome =
            crate::product_mpris_transport::dispatch(&connection, &snapshot, &intent).await;
        println!(
            "GNOS_UX_MPRIS_RELAY_RECEIVED operation={} delivered={} outcome={:?}",
            intent.operation.name(),
            outcome.is_delivered(),
            outcome
        );
        encode(&outcome)
    }
}

/// Serves the Now Playing relay on the retained lease connection.
pub async fn serve(connection: &zbus::Connection) -> zbus::Result<()> {
    connection
        .object_server()
        .at(RELAY_PATH, NowPlayingRelayServer { hub: hub() })
        .await
        .map(|_| ())
}

/// Routes one typed intent to the lease owner.
///
/// Any failure to obtain a reply is `Unavailable` and is never retried: an
/// ambiguous relay failure may or may not have executed remotely.
pub async fn relay(
    connection: &zbus::Connection,
    owner_name: &str,
    source_output: &str,
    intent: &MprisIntent,
) -> MprisTransportOutcome {
    let proxy = match zbus::Proxy::new(connection, owner_name, RELAY_PATH, RELAY_INTERFACE).await {
        Ok(proxy) => proxy,
        Err(_) => return unavailable(intent.operation, "no relay owner"),
    };
    let operation = intent.operation;
    let player = &intent.player;
    let args = (
        player.id.as_str().to_owned(),
        player.service.clone(),
        player.path.clone(),
        player.owner.clone(),
    );
    let receipt: Option<String> = match operation {
        MprisOperation::PlayPause => proxy.call("PlayPause", &args).await.ok(),
        MprisOperation::Next => proxy.call("Next", &args).await.ok(),
        MprisOperation::Previous => proxy.call("Previous", &args).await.ok(),
        MprisOperation::Seek(_) => {
            return crate::product_mpris_transport::MprisTransportOutcome::Unsupported {
                operation,
            };
        }
    };
    match receipt {
        Some(token) => {
            let outcome = decode(&token, operation);
            println!(
                "GNOS_UX_MPRIS_RELAY target={owner_name} source_output={source_output} outcome={outcome:?}"
            );
            outcome
        }
        None => {
            println!(
                "GNOS_UX_MPRIS_RELAY target={owner_name} source_output={source_output} outcome=unavailable"
            );
            unavailable(operation, "relay reply not received")
        }
    }
}

fn unavailable(operation: MprisOperation, error: &str) -> MprisTransportOutcome {
    MprisTransportOutcome::Unavailable {
        operation,
        error: error.to_owned(),
    }
}

fn encode(outcome: &MprisTransportOutcome) -> String {
    match outcome {
        MprisTransportOutcome::Delivered { operation } => {
            format!("delivered|{}", operation.name())
        }
        MprisTransportOutcome::DeliveredWithLocalError { operation, .. } => {
            format!("delivered_local_error|{}", operation.name())
        }
        MprisTransportOutcome::RefusedStale { operation, reason } => {
            format!(
                "refused_stale|{}|{}",
                reason_token(*reason),
                operation.name()
            )
        }
        MprisTransportOutcome::Unsupported { operation } => {
            format!("unsupported|{}", operation.name())
        }
        MprisTransportOutcome::Unavailable { operation, .. } => {
            format!("unavailable|{}", operation.name())
        }
        MprisTransportOutcome::TransportFailure { operation, .. } => {
            format!("transport_failure|{}", operation.name())
        }
    }
}

fn decode(token: &str, fallback: MprisOperation) -> MprisTransportOutcome {
    let parts: Vec<&str> = token.split('|').collect();
    let op_at = |index: usize| {
        parts
            .get(index)
            .and_then(|name| operation_from_name(name))
            .unwrap_or(fallback)
    };
    match parts.first().copied() {
        Some("delivered") => MprisTransportOutcome::Delivered {
            operation: op_at(1),
        },
        Some("delivered_local_error") => MprisTransportOutcome::DeliveredWithLocalError {
            operation: op_at(1),
            error: "relayed outcome decoded locally".to_owned(),
        },
        Some("refused_stale") => MprisTransportOutcome::RefusedStale {
            reason: parts
                .get(1)
                .and_then(|token| reason_from_token(token))
                .unwrap_or(MprisStaleReason::PlayerAbsent),
            operation: op_at(2),
        },
        Some("unsupported") => MprisTransportOutcome::Unsupported {
            operation: op_at(1),
        },
        Some("transport_failure") => MprisTransportOutcome::TransportFailure {
            operation: op_at(1),
            error: "relayed transport failure".to_owned(),
        },
        Some("unavailable") => unavailable(op_at(1), "relayed unavailable"),
        _ => unavailable(fallback, "unrecognised relay receipt"),
    }
}

fn operation_from_name(name: &str) -> Option<MprisOperation> {
    Some(match name {
        "play_pause" => MprisOperation::PlayPause,
        "next" => MprisOperation::Next,
        "previous" => MprisOperation::Previous,
        _ => return None,
    })
}

fn reason_token(reason: MprisStaleReason) -> &'static str {
    match reason {
        MprisStaleReason::PlayerAbsent => "player_absent",
        MprisStaleReason::OwnerReplaced => "owner_replaced",
        MprisStaleReason::CapabilityMissing => "capability_missing",
        MprisStaleReason::ControlRefused => "control_refused",
    }
}

fn reason_from_token(token: &str) -> Option<MprisStaleReason> {
    Some(match token {
        "player_absent" => MprisStaleReason::PlayerAbsent,
        "owner_replaced" => MprisStaleReason::OwnerReplaced,
        "capability_missing" => MprisStaleReason::CapabilityMissing,
        "control_refused" => MprisStaleReason::ControlRefused,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_round_trips() {
        let outcome = MprisTransportOutcome::Delivered {
            operation: MprisOperation::PlayPause,
        };
        assert_eq!(
            decode(&encode(&outcome), MprisOperation::PlayPause),
            outcome
        );
    }

    #[test]
    fn refused_stale_round_trips_with_reason() {
        let outcome = MprisTransportOutcome::RefusedStale {
            operation: MprisOperation::Next,
            reason: MprisStaleReason::OwnerReplaced,
        };
        assert_eq!(decode(&encode(&outcome), MprisOperation::Next), outcome);
    }

    #[test]
    fn an_unrecognised_receipt_is_a_failure_not_a_success() {
        assert!(matches!(
            decode("garbage", MprisOperation::PlayPause),
            MprisTransportOutcome::Unavailable { .. }
        ));
    }

    #[test]
    fn delivered_with_local_error_stays_distinct_from_transport_failure() {
        // DONOR-15's lesson: a delivered call whose local decode failed must not
        // collapse into a remote refusal across the relay either.
        let outcome = MprisTransportOutcome::DeliveredWithLocalError {
            operation: MprisOperation::Previous,
            error: "local".to_owned(),
        };
        let decoded = decode(&encode(&outcome), MprisOperation::Previous);
        assert!(decoded.is_delivered());
        assert!(!matches!(
            decoded,
            MprisTransportOutcome::TransportFailure { .. }
        ));
    }
}
