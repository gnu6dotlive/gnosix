#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

//! GNOS.UX client transport for the private in.gnu.os activation boundary.
//!
//! This crate never retries automatically. It refuses locally when the
//! selected semantic reference is not from the caller's current projection.

use gnos_ux_product_intent::ProductIntent;
use gnosix_activation_protocol::{
    ACTIVATION_BUS_NAME, ACTIVATION_INTERFACE, ACTIVATION_OBJECT_PATH, ActivationReceiptCode,
};
use gnosix_desktop_model::DesktopContextGeneration;
use zbus::blocking::{Connection, Proxy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationClientReceipt {
    pub code: ActivationReceiptCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationClientRefusal {
    StaleLocalObservation {
        selected: DesktopContextGeneration,
        current: DesktopContextGeneration,
    },
    Transport(String),
    Protocol(String),
}

trait ActivationTransport {
    fn send(&self, intent: ProductIntent) -> Result<(u32, String), String>;
}

struct DbusTransport {
    connection: Connection,
}

impl ActivationTransport for DbusTransport {
    fn send(&self, intent: ProductIntent) -> Result<(u32, String), String> {
        let proxy = Proxy::new(
            &self.connection,
            ACTIVATION_BUS_NAME,
            ACTIVATION_OBJECT_PATH,
            ACTIVATION_INTERFACE,
        )
        .map_err(|error| error.to_string())?;

        match intent {
            ProductIntent::ActivateObservedWorkspace { request_id, target } => proxy
                .call(
                    "ActivateObservedWorkspace",
                    &(
                        request_id.as_bytes().to_vec(),
                        target.observed_generation().0,
                        target.workspace_id().into_bytes().to_vec(),
                    ),
                )
                .map_err(|error| error.to_string()),
            ProductIntent::FocusObservedApplication { request_id, target } => proxy
                .call(
                    "FocusObservedApplication",
                    &(
                        request_id.as_bytes().to_vec(),
                        target.observed_generation().0,
                        target.surface_id().into_bytes().to_vec(),
                    ),
                )
                .map_err(|error| error.to_string()),
            ProductIntent::MoveObservedApplication {
                request_id,
                window,
                destination,
            } => proxy
                .call(
                    "MoveObservedApplication",
                    &(
                        request_id.as_bytes().to_vec(),
                        window.observed_generation().0,
                        window.surface_id().into_bytes().to_vec(),
                        destination.workspace_id().into_bytes().to_vec(),
                    ),
                )
                .map_err(|error| error.to_string()),
        }
    }
}

pub struct ActivationClient {
    transport: DbusTransport,
}

impl ActivationClient {
    pub fn session() -> Result<Self, ActivationClientRefusal> {
        Ok(Self {
            transport: DbusTransport {
                connection: Connection::session()
                    .map_err(|error| ActivationClientRefusal::Transport(error.to_string()))?,
            },
        })
    }

    /// Send once only if the selected target is still from the caller's
    /// current semantic generation. No automatic retry occurs on any result.
    pub fn send_if_current(
        &self,
        intent: ProductIntent,
        current_generation: DesktopContextGeneration,
    ) -> Result<ActivationClientReceipt, ActivationClientRefusal> {
        send_if_current(&self.transport, intent, current_generation)
    }
}

fn intent_generation(intent: ProductIntent) -> DesktopContextGeneration {
    match intent {
        ProductIntent::ActivateObservedWorkspace { target, .. } => target.observed_generation(),
        ProductIntent::FocusObservedApplication { target, .. } => target.observed_generation(),
        ProductIntent::MoveObservedApplication { window, .. } => window.observed_generation(),
    }
}

fn send_if_current<T: ActivationTransport>(
    transport: &T,
    intent: ProductIntent,
    current_generation: DesktopContextGeneration,
) -> Result<ActivationClientReceipt, ActivationClientRefusal> {
    let selected = intent_generation(intent);
    if selected != current_generation {
        return Err(ActivationClientRefusal::StaleLocalObservation {
            selected,
            current: current_generation,
        });
    }

    let (raw_code, detail) = transport
        .send(intent)
        .map_err(ActivationClientRefusal::Transport)?;
    let code = ActivationReceiptCode::try_from(raw_code).map_err(|_| {
        ActivationClientRefusal::Protocol(format!(
            "unknown in.gnu.os activation receipt code {raw_code}"
        ))
    })?;
    Ok(ActivationClientReceipt { code, detail })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_ux_product_intent::{ObservedWorkspaceReference, ProductIntentRequestId};
    use gnosix_desktop_model::WorkspaceId;
    use std::cell::Cell;

    struct FakeTransport {
        calls: Cell<u64>,
        response: (u32, &'static str),
    }

    impl ActivationTransport for FakeTransport {
        fn send(&self, _intent: ProductIntent) -> Result<(u32, String), String> {
            self.calls.set(self.calls.get() + 1);
            Ok((self.response.0, self.response.1.to_owned()))
        }
    }

    fn workspace_intent(generation: u64) -> ProductIntent {
        ProductIntent::activate_observed_workspace(
            ProductIntentRequestId::from_bytes([1; 16]),
            ObservedWorkspaceReference::new(
                WorkspaceId::from_bytes([2; 16]),
                DesktopContextGeneration(generation),
            ),
        )
    }

    #[test]
    fn stale_local_generation_refuses_before_transport() {
        let transport = FakeTransport {
            calls: Cell::new(0),
            response: (ActivationReceiptCode::Applied as u32, "unused"),
        };
        let refusal = send_if_current(&transport, workspace_intent(7), DesktopContextGeneration(8))
            .unwrap_err();

        assert_eq!(transport.calls.get(), 0);
        assert_eq!(
            refusal,
            ActivationClientRefusal::StaleLocalObservation {
                selected: DesktopContextGeneration(7),
                current: DesktopContextGeneration(8),
            }
        );
    }

    #[test]
    fn exact_current_generation_sends_once_and_decodes_receipt() {
        let transport = FakeTransport {
            calls: Cell::new(0),
            response: (ActivationReceiptCode::Applied as u32, "APPLIED"),
        };
        let receipt =
            send_if_current(&transport, workspace_intent(7), DesktopContextGeneration(7)).unwrap();

        assert_eq!(transport.calls.get(), 1);
        assert_eq!(receipt.code, ActivationReceiptCode::Applied);
        assert_eq!(receipt.detail, "APPLIED");
    }

    #[test]
    fn unknown_server_receipt_fails_protocol_without_retry() {
        let transport = FakeTransport {
            calls: Cell::new(0),
            response: (99, "future"),
        };
        let refusal = send_if_current(&transport, workspace_intent(7), DesktopContextGeneration(7))
            .unwrap_err();

        assert_eq!(transport.calls.get(), 1);
        assert!(matches!(refusal, ActivationClientRefusal::Protocol(_)));
    }
}
