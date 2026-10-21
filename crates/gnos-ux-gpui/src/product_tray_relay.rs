//! Cross-process routing for tray interaction intents.
//!
//! ## The invariant this encodes
//!
//! ```text
//! VISIBLE + ENABLED  ->  LOCAL INTERACTION SINK EXISTS
//! ```
//!
//! regardless of which process holds the execution lease. The lease elects an
//! **executor**, not an interactive output. Before this module the lease also
//! selected the one usable output, so a click on any other output was silently
//! dropped. Now every enabled surface installs a sink and emits the same typed
//! intent; ownership only decides how the intent reaches execution:
//!
//! ```text
//! click on output B -> local typed sink -> does this process own the lease?
//!     yes -> execute locally
//!     no  -> relay the exact typed intent to the lease owner
//!                -> owner revalidates and executes
//!                -> exactly one remote action
//! ```
//!
//! ## Coordination transport, not an authority API
//!
//! The owner exports a narrow, tray-specific interface on the same retained
//! connection that holds the lease name. The payload is tray vocabulary
//! (`Activate`, `SecondaryActivate`, `OpenMenu`, `Scroll`, `MenuEvent`), never a
//! generic `Dispatch(action, payload)`. The forwarding process confers no
//! authority: the owner performs exactly the same exact-instance revalidation,
//! freshness and capability checks it performs for a local click.
//!
//! ## Failure is terminal, never retried
//!
//! If the relay fails ambiguously -- the owner died before replying -- the
//! sender cannot know whether the remote action happened. It reports
//! `Unavailable` and does **not** redispatch. A new physical interaction may try
//! again. This is what protects the exactly-once property.

use std::sync::{Arc, OnceLock};

use crate::product_tray_action::{
    ExactTrayItemRef, StaleReason, TrayIntent, TrayIntentOutcome, TrayInteractionAnchor,
    TrayOperation, TrayScrollAxis, TrayScrollDelta,
};

/// Object path the owner serves the relay on.
pub const RELAY_PATH: &str = "/org/gnosix/TrayInteractionRelay";
/// Interface the owner serves the relay on. Vertical-specific by construction.
pub const RELAY_INTERFACE: &str = "org.gnosix.TrayInteractionRelay";

/// The owner's live coordinate state.
///
/// The relay runs on the zbus executor, outside the GPUI app, so it cannot read
/// the session entity directly. The scene host mirrors the small amount the
/// owner needs to revalidate and dispatch into this hub on every render.
#[derive(Default)]
struct HubState {
    connection: Option<zbus::Connection>,
    snapshot: crate::product_tray::TraySnapshot,
    open_menu: Option<crate::product_tray_menu::TrayMenuSnapshot>,
}

/// Process-local relay hub. Only meaningful in the process that owns the lease.
#[derive(Default)]
pub struct TrayRelayHub {
    state: std::sync::Mutex<HubState>,
}

impl TrayRelayHub {
    fn update(
        &self,
        connection: zbus::Connection,
        snapshot: crate::product_tray::TraySnapshot,
        open_menu: Option<crate::product_tray_menu::TrayMenuSnapshot>,
    ) {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.connection = Some(connection);
        guard.snapshot = snapshot;
        guard.open_menu = open_menu;
    }

    fn ready(
        &self,
    ) -> Option<(
        zbus::Connection,
        crate::product_tray::TraySnapshot,
        Option<crate::product_tray_menu::TrayMenuSnapshot>,
    )> {
        let guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Some((
            guard.connection.clone()?,
            guard.snapshot.clone(),
            guard.open_menu.clone(),
        ))
    }

    /// Records the menu the owner projected for a forwarded open, so a later
    /// forwarded `MenuEvent` revalidates against the same revision the clicked
    /// output is rendering.
    fn set_menu(&self, menu: Option<crate::product_tray_menu::TrayMenuSnapshot>) {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.open_menu = menu;
    }
}

static HUB: OnceLock<Arc<TrayRelayHub>> = OnceLock::new();

/// The process-local hub. Created on first use.
pub fn hub() -> Arc<TrayRelayHub> {
    HUB.get_or_init(|| Arc::new(TrayRelayHub::default()))
        .clone()
}

/// Mirrors the owner's live dispatch state into the hub.
pub fn publish(
    connection: zbus::Connection,
    snapshot: crate::product_tray::TraySnapshot,
    open_menu: Option<crate::product_tray_menu::TrayMenuSnapshot>,
) {
    hub().update(connection, snapshot, open_menu);
}

/// Mirrors snapshot and connection into the hub without touching the open-menu
/// projection, which the tray session does not own. Called at observation time
/// so a forwarded intent is never answered "not ready" merely because the owner
/// has not rendered since observing the item.
pub fn publish_snapshot(connection: zbus::Connection, snapshot: crate::product_tray::TraySnapshot) {
    let hub = hub();
    let mut guard = hub
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.connection = Some(connection);
    guard.snapshot = snapshot;
}

/// Owner reply to a forwarded `OpenMenu`: the bounded receipt plus the typed
/// menu projection the clicked output must render. The executor and the
/// presentation owner are different processes.
#[derive(
    Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, zbus::zvariant::Type,
)]
pub struct RelayOpenMenuReply {
    pub receipt: String,
    pub menu: crate::product_tray_menu_wire::RelayMenu,
}

impl RelayOpenMenuReply {
    fn refused(outcome: TrayIntentOutcome) -> Self {
        Self {
            receipt: encode(&outcome),
            menu: Default::default(),
        }
    }
}

/// The owner's relay server.
struct TrayRelayServer {
    hub: Arc<TrayRelayHub>,
}

#[zbus::interface(name = "org.gnosix.TrayInteractionRelay")]
impl TrayRelayServer {
    // D-Bus relay wire signature is externally constrained; grouping these fields
    // into a Rust struct would change the method ABI.
    #[allow(clippy::too_many_arguments)]
    async fn activate(
        &self,
        item_id: String,
        service: String,
        path: String,
        owner: String,
        screen_x: i32,
        screen_y: i32,
        source_output: String,
    ) -> String {
        self.execute(
            &TrayIntent::Activate {
                item: item_ref(item_id, service, path, owner),
                anchor: anchor(screen_x, screen_y),
            },
            &source_output,
        )
        .await
    }

    // D-Bus relay wire signature is externally constrained; grouping these fields
    // into a Rust struct would change the method ABI.
    #[allow(clippy::too_many_arguments)]
    async fn secondary_activate(
        &self,
        item_id: String,
        service: String,
        path: String,
        owner: String,
        screen_x: i32,
        screen_y: i32,
        source_output: String,
    ) -> String {
        self.execute(
            &TrayIntent::SecondaryActivate {
                item: item_ref(item_id, service, path, owner),
                anchor: anchor(screen_x, screen_y),
            },
            &source_output,
        )
        .await
    }

    // D-Bus relay wire signature is externally constrained; grouping these fields
    // into a Rust struct would change the method ABI.
    #[allow(clippy::too_many_arguments)]
    async fn open_menu(
        &self,
        item_id: String,
        service: String,
        path: String,
        owner: String,
        screen_x: i32,
        screen_y: i32,
        source_output: String,
    ) -> RelayOpenMenuReply {
        let item = item_ref(item_id, service, path, owner);
        let _ = anchor(screen_x, screen_y);
        let Some((connection, snapshot, _)) = self.hub.ready() else {
            return RelayOpenMenuReply::refused(TrayIntentOutcome::Unavailable);
        };
        // Revalidate membership and owner provenance before projecting, exactly
        // as a local open would.
        let observed = snapshot
            .items()
            .iter()
            .find(|observed| {
                observed.id == item.id
                    && observed.service == item.service
                    && observed.path == item.path
            })
            .cloned();
        let Some(observed) = observed else {
            return RelayOpenMenuReply::refused(TrayIntentOutcome::RefusedStale {
                reason: StaleReason::MembershipLost,
                operation: TrayOperation::ContextMenu,
            });
        };
        match crate::interaction_lease::unique_name_owner(&connection, &item.service).await {
            Some(current) if current == item.owner => {}
            Some(_) => {
                return RelayOpenMenuReply::refused(TrayIntentOutcome::RefusedStale {
                    reason: StaleReason::OwnerReplaced,
                    operation: TrayOperation::ContextMenu,
                });
            }
            None => {
                return RelayOpenMenuReply::refused(TrayIntentOutcome::RefusedStale {
                    reason: StaleReason::MembershipLost,
                    operation: TrayOperation::ContextMenu,
                });
            }
        }
        // The remote AboutToShow / GetLayout executes once, here, in the
        // executor. The resulting projection is returned to the interaction
        // source, which is where it renders.
        let (outcome, menu) =
            crate::product_tray_controller::read_exported_menu(&connection, &observed).await;
        if let Some(menu) = &menu {
            self.hub.set_menu(Some(menu.clone()));
            println!(
                "GNOS_UX_TRAY_RELAY_MENU source_output={source_output} revision={} rows={}",
                menu.revision,
                menu.item_count()
            );
        }
        RelayOpenMenuReply {
            receipt: encode(&outcome),
            menu: menu
                .map(|menu| crate::product_tray_menu_wire::RelayMenu::from_snapshot(&menu))
                .unwrap_or_default(),
        }
    }

    // D-Bus relay wire signature is externally constrained; grouping these fields
    // into a Rust struct would change the method ABI.
    #[allow(clippy::too_many_arguments)]
    async fn scroll(
        &self,
        item_id: String,
        service: String,
        path: String,
        owner: String,
        axis: String,
        delta: i32,
        source_output: String,
    ) -> String {
        let axis = if axis == "horizontal" {
            TrayScrollAxis::Horizontal
        } else {
            TrayScrollAxis::Vertical
        };
        self.execute(
            &TrayIntent::Scroll {
                item: item_ref(item_id, service, path, owner),
                axis,
                delta: TrayScrollDelta::new(delta),
            },
            &source_output,
        )
        .await
    }

    // D-Bus relay wire signature is externally constrained; grouping these fields
    // into a Rust struct would change the method ABI.
    #[allow(clippy::too_many_arguments)]
    async fn menu_event(
        &self,
        item_id: String,
        service: String,
        path: String,
        owner: String,
        menu_id: i32,
        revision: u32,
        rendered_label: String,
        screen_x: i32,
        screen_y: i32,
        source_output: String,
    ) -> String {
        self.execute(
            &TrayIntent::MenuEvent {
                binding: crate::product_tray_action::TrayMenuEventBinding {
                    item: item_ref(item_id, service, path, owner),
                    menu_id,
                    revision,
                    event: crate::product_tray_action::TrayMenuEvent::Clicked,
                    rendered_label: (!rendered_label.is_empty()).then_some(rendered_label),
                },
                anchor: anchor(screen_x, screen_y),
            },
            &source_output,
        )
        .await
    }

    /// Liveness probe so a sender can distinguish "owner reachable" from
    /// "no owner", without dispatching anything.
    async fn ping(&self) -> bool {
        true
    }
}

fn item_ref(item_id: String, service: String, path: String, owner: String) -> ExactTrayItemRef {
    ExactTrayItemRef {
        id: crate::product_tray::TrayItemId::new(item_id),
        service,
        path,
        owner,
    }
}

fn anchor(screen_x: i32, screen_y: i32) -> TrayInteractionAnchor {
    TrayInteractionAnchor {
        output_origin: (0, 0),
        local: (0.0, 0.0),
        screen: (screen_x, screen_y),
    }
}

impl TrayRelayServer {
    /// Performs the owner's normal dispatch path for a forwarded intent.
    async fn execute(&self, intent: &TrayIntent, source_output: &str) -> String {
        let Some((connection, snapshot, open_menu)) = self.hub.ready() else {
            return encode(&TrayIntentOutcome::Unavailable);
        };
        let outcome = crate::product_tray_controller::dispatch_intent(
            &connection,
            &snapshot,
            open_menu.as_ref(),
            intent,
        )
        .await;
        println!(
            "GNOS_UX_TRAY_RELAY_RECEIVED source_output={source_output} operation={} outcome={:?}",
            intent.operation().token(),
            outcome
        );
        encode(&outcome)
    }
}

/// Serves the relay on the retained lease connection.
///
/// Called once, by the process that wins the lease. The interface lives on the
/// object server of that connection, so it is reachable at the well-known owner
/// name for exactly as long as the lease is held.
pub async fn serve(connection: &zbus::Connection) -> zbus::Result<()> {
    connection
        .object_server()
        .at(RELAY_PATH, TrayRelayServer { hub: hub() })
        .await
        .map(|_| ())
}

/// Routes one typed intent to the lease owner and returns the owner's receipt,
/// mapped back into the product outcome vocabulary.
///
/// Any failure to obtain a reply is reported as `Unavailable`. It is never
/// retried: an ambiguous relay failure may or may not have executed remotely,
/// so redispatching would risk a second remote action.
pub async fn relay(
    connection: &zbus::Connection,
    owner_name: &str,
    source_output: &str,
    intent: &TrayIntent,
) -> TrayIntentOutcome {
    let proxy = match zbus::Proxy::new(connection, owner_name, RELAY_PATH, RELAY_INTERFACE).await {
        Ok(proxy) => proxy,
        Err(_) => return TrayIntentOutcome::Unavailable,
    };
    let operation = intent.operation();
    let receipt: Option<String> = match intent {
        TrayIntent::Activate { item, anchor } => proxy
            .call(
                "Activate",
                &(
                    item.id.as_str().to_owned(),
                    item.service.clone(),
                    item.path.clone(),
                    item.owner.clone(),
                    anchor.screen.0,
                    anchor.screen.1,
                    source_output.to_owned(),
                ),
            )
            .await
            .ok(),
        TrayIntent::SecondaryActivate { item, anchor } => proxy
            .call(
                "SecondaryActivate",
                &(
                    item.id.as_str().to_owned(),
                    item.service.clone(),
                    item.path.clone(),
                    item.owner.clone(),
                    anchor.screen.0,
                    anchor.screen.1,
                    source_output.to_owned(),
                ),
            )
            .await
            .ok(),
        TrayIntent::OpenMenu { item, anchor } => {
            let (outcome, _menu) =
                relay_open_menu(connection, owner_name, source_output, item, anchor).await;
            return outcome;
        }
        TrayIntent::Scroll { item, axis, delta } => proxy
            .call(
                "Scroll",
                &(
                    item.id.as_str().to_owned(),
                    item.service.clone(),
                    item.path.clone(),
                    item.owner.clone(),
                    match axis {
                        TrayScrollAxis::Horizontal => "horizontal".to_owned(),
                        TrayScrollAxis::Vertical => "vertical".to_owned(),
                    },
                    delta.get(),
                    source_output.to_owned(),
                ),
            )
            .await
            .ok(),
        TrayIntent::MenuEvent { binding, anchor } => proxy
            .call(
                "MenuEvent",
                &(
                    binding.item.id.as_str().to_owned(),
                    binding.item.service.clone(),
                    binding.item.path.clone(),
                    binding.item.owner.clone(),
                    binding.menu_id,
                    binding.revision,
                    binding.rendered_label.clone().unwrap_or_default(),
                    anchor.screen.0,
                    anchor.screen.1,
                    source_output.to_owned(),
                ),
            )
            .await
            .ok(),
    };
    let outcome = match receipt {
        Some(token) => decode(&token, operation),
        None => TrayIntentOutcome::Unavailable,
    };
    println!(
        "GNOS_UX_TRAY_RELAY target={owner_name} source_output={source_output} outcome={outcome:?}"
    );
    outcome
}

/// Routes one `OpenMenu` to the lease owner and returns the owner's projection.
///
/// The executor projects once (remote `AboutToShow` + `GetLayout`) and returns
/// the typed menu so the interaction source renders it on the output the user
/// actually clicked. Any failure is `Unavailable` and is never retried.
pub async fn relay_open_menu(
    connection: &zbus::Connection,
    owner_name: &str,
    source_output: &str,
    item: &ExactTrayItemRef,
    anchor: &TrayInteractionAnchor,
) -> (
    TrayIntentOutcome,
    Option<crate::product_tray_menu::TrayMenuSnapshot>,
) {
    let proxy = match zbus::Proxy::new(connection, owner_name, RELAY_PATH, RELAY_INTERFACE).await {
        Ok(proxy) => proxy,
        Err(_) => return (TrayIntentOutcome::Unavailable, None),
    };
    let reply: Option<RelayOpenMenuReply> = proxy
        .call(
            "OpenMenu",
            &(
                item.id.as_str().to_owned(),
                item.service.clone(),
                item.path.clone(),
                item.owner.clone(),
                anchor.screen.0,
                anchor.screen.1,
                source_output.to_owned(),
            ),
        )
        .await
        .ok();
    match reply {
        Some(reply) => {
            let outcome = decode(&reply.receipt, TrayOperation::ContextMenu);
            println!(
                "GNOS_UX_TRAY_RELAY target={owner_name} source_output={source_output} operation=context_menu outcome={outcome:?}"
            );
            let menu = (!reply.menu.is_empty()).then(|| reply.menu.to_snapshot());
            (outcome, menu)
        }
        None => (TrayIntentOutcome::Unavailable, None),
    }
}

/// Encodes a resolved outcome as a bounded coordination receipt token.
fn encode(outcome: &TrayIntentOutcome) -> String {
    match outcome {
        TrayIntentOutcome::Completed { operation } => {
            format!("completed|{}", operation.token())
        }
        TrayIntentOutcome::RefusedStale { reason, operation } => {
            format!(
                "refused_stale|{}|{}",
                reason_token(*reason),
                operation.token()
            )
        }
        TrayIntentOutcome::Unavailable => "unavailable".to_owned(),
        TrayIntentOutcome::Unsupported { operation } => {
            format!("unsupported|{}", operation.token())
        }
        TrayIntentOutcome::TransportFailure { operation } => {
            format!("transport_failure|{}", operation.token())
        }
    }
}

/// Decodes a receipt token. An unrecognised token is treated as an ambiguous
/// failure, never as success.
fn decode(token: &str, fallback: TrayOperation) -> TrayIntentOutcome {
    let mut parts = token.split('|');
    match parts.next() {
        Some("completed") => TrayIntentOutcome::Completed {
            operation: parts
                .next()
                .and_then(operation_from_token)
                .unwrap_or(fallback),
        },
        Some("refused_stale") => TrayIntentOutcome::RefusedStale {
            reason: parts
                .next()
                .and_then(reason_from_token)
                .unwrap_or(StaleReason::MembershipLost),
            operation: parts
                .next()
                .and_then(operation_from_token)
                .unwrap_or(fallback),
        },
        Some("unsupported") => TrayIntentOutcome::Unsupported {
            operation: parts
                .next()
                .and_then(operation_from_token)
                .unwrap_or(fallback),
        },
        Some("transport_failure") => TrayIntentOutcome::TransportFailure {
            operation: parts
                .next()
                .and_then(operation_from_token)
                .unwrap_or(fallback),
        },
        _ => TrayIntentOutcome::Unavailable,
    }
}

fn reason_token(reason: StaleReason) -> &'static str {
    match reason {
        StaleReason::MembershipLost => "membership_lost",
        StaleReason::OwnerReplaced => "owner_replaced",
        StaleReason::MenuRevisionChanged => "menu_revision_changed",
        StaleReason::MenuItemAbsent => "menu_item_absent",
    }
}

fn reason_from_token(token: &str) -> Option<StaleReason> {
    Some(match token {
        "membership_lost" => StaleReason::MembershipLost,
        "owner_replaced" => StaleReason::OwnerReplaced,
        "menu_revision_changed" => StaleReason::MenuRevisionChanged,
        "menu_item_absent" => StaleReason::MenuItemAbsent,
        _ => return None,
    })
}

fn operation_from_token(token: &str) -> Option<TrayOperation> {
    Some(match token {
        "activate" => TrayOperation::Activate,
        "secondary_activate" => TrayOperation::SecondaryActivate,
        "context_menu" => TrayOperation::ContextMenu,
        "scroll_horizontal" => TrayOperation::ScrollHorizontal,
        "scroll_vertical" => TrayOperation::ScrollVertical,
        "menu_about_to_show" => TrayOperation::MenuAboutToShow,
        "menu_event_clicked" => TrayOperation::MenuEventClicked,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_round_trips_with_its_operation() {
        let outcome = TrayIntentOutcome::Completed {
            operation: TrayOperation::Activate,
        };
        assert_eq!(decode(&encode(&outcome), TrayOperation::Activate), outcome);
    }

    #[test]
    fn refused_stale_round_trips_with_reason_and_operation() {
        let outcome = TrayIntentOutcome::RefusedStale {
            reason: StaleReason::OwnerReplaced,
            operation: TrayOperation::Activate,
        };
        assert_eq!(decode(&encode(&outcome), TrayOperation::Activate), outcome);
    }

    #[test]
    fn an_unrecognised_receipt_is_a_failure_not_a_success() {
        // The whole point of the receipt is that a lost or garbled reply must
        // never be mistaken for a completed remote action.
        assert_eq!(
            decode("garbage", TrayOperation::Activate),
            TrayIntentOutcome::Unavailable
        );
        assert_eq!(
            decode("", TrayOperation::Activate),
            TrayIntentOutcome::Unavailable
        );
    }

    #[test]
    fn every_outcome_variant_round_trips() {
        for outcome in [
            TrayIntentOutcome::Completed {
                operation: TrayOperation::ContextMenu,
            },
            TrayIntentOutcome::RefusedStale {
                reason: StaleReason::MenuRevisionChanged,
                operation: TrayOperation::MenuEventClicked,
            },
            TrayIntentOutcome::Unavailable,
            TrayIntentOutcome::Unsupported {
                operation: TrayOperation::SecondaryActivate,
            },
            TrayIntentOutcome::TransportFailure {
                operation: TrayOperation::ScrollHorizontal,
            },
        ] {
            assert_eq!(
                decode(&encode(&outcome), outcome.operation()),
                outcome,
                "outcome {outcome:?} must survive the relay receipt"
            );
        }
    }
}
