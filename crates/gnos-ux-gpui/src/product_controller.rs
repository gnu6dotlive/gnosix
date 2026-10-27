//! Typed side-effect controller for GNOS.UX product interactions.
//!
//! Product views dispatch through this controller. The controller owns local
//! freshness rejection, request IDs, one-shot activation transport and typed
//! action lifecycle. It never mutates semantic desktop truth optimistically.

use crate::{
    product_runtime::{
        ProductActionRecord, ProductActionTarget, ProductActionTerminal, ProductRuntime,
    },
    product_scene::{ProductApplicationContent, ProductWorkspaceContent},
};
use gnos_ux_activation_client::{
    ActivationClient, ActivationClientReceipt, ActivationClientRefusal,
};
use gnos_ux_product_intent::{ProductIntent, ProductIntentRequestId};
use gnosix_activation_protocol::ActivationReceiptCode;
use gnosix_desktop_model::DesktopContextGeneration;
use uuid::Uuid;

trait ProductActivationSender {
    fn send_if_current(
        &self,
        intent: ProductIntent,
        current_generation: DesktopContextGeneration,
    ) -> Result<ActivationClientReceipt, ActivationClientRefusal>;
}

impl ProductActivationSender for ActivationClient {
    fn send_if_current(
        &self,
        intent: ProductIntent,
        current_generation: DesktopContextGeneration,
    ) -> Result<ActivationClientReceipt, ActivationClientRefusal> {
        ActivationClient::send_if_current(self, intent, current_generation)
    }
}

#[derive(Default)]
pub struct ProductController;

impl ProductController {
    pub fn focus_application(
        &self,
        runtime: &mut ProductRuntime,
        selected: ProductApplicationContent,
    ) -> ProductActionRecord {
        self.focus_application_with(
            runtime,
            selected,
            ProductIntentRequestId::from_bytes(*Uuid::new_v4().as_bytes()),
            ActivationClient::session,
        )
    }

    pub fn activate_workspace(
        &self,
        runtime: &mut ProductRuntime,
        selected: ProductWorkspaceContent,
    ) -> ProductActionRecord {
        self.activate_workspace_with(
            runtime,
            selected,
            ProductIntentRequestId::from_bytes(*Uuid::new_v4().as_bytes()),
            ActivationClient::session,
        )
    }

    /// WS-02B: move an observed window onto an observed workspace, through
    /// the activation authority (it checks the window lands there).
    pub fn move_application(
        &self,
        runtime: &mut ProductRuntime,
        window: ProductApplicationContent,
        destination: ProductWorkspaceContent,
    ) -> ProductActionRecord {
        self.move_application_with(
            runtime,
            window,
            destination,
            ProductIntentRequestId::from_bytes(*Uuid::new_v4().as_bytes()),
            ActivationClient::session,
        )
    }

    fn move_application_with<S, F>(
        &self,
        runtime: &mut ProductRuntime,
        window: ProductApplicationContent,
        destination: ProductWorkspaceContent,
        request_id: ProductIntentRequestId,
        sender_factory: F,
    ) -> ProductActionRecord
    where
        S: ProductActivationSender,
        F: FnOnce() -> Result<S, ActivationClientRefusal>,
    {
        let current_generation = runtime.current_generation();
        runtime.begin_application_action(&window, current_generation);
        let Some(current_generation) = current_generation else {
            let record = runtime.finish_action(
                ProductActionTerminal::SemanticUnavailable,
                "current product semantics unavailable",
            );
            emit_product_activation(&record);
            return record;
        };
        // One view of the desktop: both from the current observation.
        let Some(intent) = ProductIntent::move_observed_application(
            request_id,
            window.reference,
            destination.reference,
        )
        .filter(|_| window.reference.observed_generation() == current_generation) else {
            let record = runtime.finish_action(
                ProductActionTerminal::LocalStale,
                format!(
                    "window generation {} / destination generation {} differ from current generation {}",
                    window.reference.observed_generation().0,
                    destination.reference.observed_generation().0,
                    current_generation.0
                ),
            );
            emit_product_activation(&record);
            return record;
        };
        let sender = match sender_factory() {
            Ok(sender) => sender,
            Err(refusal) => {
                let terminal = terminal_from_client_refusal(&refusal);
                let record = runtime.finish_action(terminal, format!("{refusal:?}"));
                emit_product_activation(&record);
                return record;
            }
        };
        let record = match sender.send_if_current(intent, current_generation) {
            Ok(receipt) => {
                runtime.finish_action(terminal_from_receipt(receipt.code), receipt.detail)
            }
            Err(refusal) => runtime.finish_action(
                terminal_from_client_refusal(&refusal),
                format!("{refusal:?}"),
            ),
        };
        emit_product_activation(&record);
        record
    }

    fn focus_application_with<S, F>(
        &self,
        runtime: &mut ProductRuntime,
        selected: ProductApplicationContent,
        request_id: ProductIntentRequestId,
        sender_factory: F,
    ) -> ProductActionRecord
    where
        S: ProductActivationSender,
        F: FnOnce() -> Result<S, ActivationClientRefusal>,
    {
        let selected_generation = selected.reference.observed_generation();
        let current_generation = runtime.current_generation();
        runtime.begin_application_action(&selected, current_generation);

        let Some(current_generation) = current_generation else {
            let record = runtime.finish_action(
                ProductActionTerminal::SemanticUnavailable,
                "current product semantics unavailable",
            );
            emit_product_activation(&record);
            return record;
        };

        if selected_generation != current_generation {
            let record = runtime.finish_action(
                ProductActionTerminal::LocalStale,
                format!(
                    "selected generation {} differs from current generation {}",
                    selected_generation.0, current_generation.0
                ),
            );
            emit_product_activation(&record);
            return record;
        }

        let sender = match sender_factory() {
            Ok(sender) => sender,
            Err(refusal) => {
                let terminal = terminal_from_client_refusal(&refusal);
                let record = runtime.finish_action(terminal, format!("{refusal:?}"));
                emit_product_activation(&record);
                return record;
            }
        };

        let intent = selected.focus_intent(request_id);
        let record = match sender.send_if_current(intent, current_generation) {
            Ok(receipt) => {
                runtime.finish_action(terminal_from_receipt(receipt.code), receipt.detail)
            }
            Err(refusal) => runtime.finish_action(
                terminal_from_client_refusal(&refusal),
                format!("{refusal:?}"),
            ),
        };
        emit_product_activation(&record);
        record
    }

    fn activate_workspace_with<S, F>(
        &self,
        runtime: &mut ProductRuntime,
        selected: ProductWorkspaceContent,
        request_id: ProductIntentRequestId,
        sender_factory: F,
    ) -> ProductActionRecord
    where
        S: ProductActivationSender,
        F: FnOnce() -> Result<S, ActivationClientRefusal>,
    {
        let selected_generation = selected.reference.observed_generation();
        let current_generation = runtime.current_generation();
        runtime.begin_workspace_action(&selected, current_generation);

        let Some(current_generation) = current_generation else {
            let record = runtime.finish_action(
                ProductActionTerminal::SemanticUnavailable,
                "current product semantics unavailable",
            );
            emit_product_activation(&record);
            return record;
        };

        if selected_generation != current_generation {
            let record = runtime.finish_action(
                ProductActionTerminal::LocalStale,
                format!(
                    "selected generation {} differs from current generation {}",
                    selected_generation.0, current_generation.0
                ),
            );
            emit_product_activation(&record);
            return record;
        }

        let sender = match sender_factory() {
            Ok(sender) => sender,
            Err(refusal) => {
                let terminal = terminal_from_client_refusal(&refusal);
                let record = runtime.finish_action(terminal, format!("{refusal:?}"));
                emit_product_activation(&record);
                return record;
            }
        };

        let intent = selected.activation_intent(request_id);
        let record = match sender.send_if_current(intent, current_generation) {
            Ok(receipt) => {
                runtime.finish_action(terminal_from_receipt(receipt.code), receipt.detail)
            }
            Err(refusal) => runtime.finish_action(
                terminal_from_client_refusal(&refusal),
                format!("{refusal:?}"),
            ),
        };
        emit_product_activation(&record);
        record
    }
}

fn terminal_from_receipt(code: ActivationReceiptCode) -> ProductActionTerminal {
    match code {
        ActivationReceiptCode::Applied => ProductActionTerminal::Applied,
        ActivationReceiptCode::AlreadySatisfied => ProductActionTerminal::AlreadySatisfied,
        ActivationReceiptCode::Stale => ProductActionTerminal::Stale,
        ActivationReceiptCode::UnknownTarget => ProductActionTerminal::UnknownTarget,
        ActivationReceiptCode::NotAuthorized => ProductActionTerminal::NotAuthorized,
        ActivationReceiptCode::TargetDisappeared => ProductActionTerminal::TargetDisappeared,
        ActivationReceiptCode::SessionUnavailable => ProductActionTerminal::SessionUnavailable,
        ActivationReceiptCode::BackendRejected => ProductActionTerminal::BackendRejected,
        ActivationReceiptCode::ObservationFailure => ProductActionTerminal::ObservationFailure,
        ActivationReceiptCode::ProtocolFailure => ProductActionTerminal::ProtocolFailure,
        ActivationReceiptCode::TransportFailure => ProductActionTerminal::TransportFailure,
        ActivationReceiptCode::PostconditionMismatch => {
            ProductActionTerminal::PostconditionMismatch
        }
        ActivationReceiptCode::ExecutionBindingChanged => {
            ProductActionTerminal::ExecutionBindingChanged
        }
    }
}

fn terminal_from_client_refusal(refusal: &ActivationClientRefusal) -> ProductActionTerminal {
    match refusal {
        ActivationClientRefusal::StaleLocalObservation { .. } => ProductActionTerminal::LocalStale,
        ActivationClientRefusal::Transport(_) => ProductActionTerminal::ClientTransport,
        ActivationClientRefusal::Protocol(_) => ProductActionTerminal::ClientProtocol,
    }
}

fn emit_product_activation(record: &ProductActionRecord) {
    let terminal = record
        .terminal
        .expect("terminal product action record must have terminal outcome");
    let current_generation = record
        .current_generation
        .map_or_else(|| "NONE".to_owned(), |generation| generation.0.to_string());
    let success = matches!(
        terminal,
        ProductActionTerminal::Applied | ProductActionTerminal::AlreadySatisfied
    );

    match record.target {
        ProductActionTarget::Workspace(reference) => {
            if success {
                println!(
                    "GNOS_UX_WORKSPACE_MANAGER_ACTIVATION workspace={:?} label={:?} selected_generation={} current_generation={} status=receipt code={} detail={:?}",
                    reference.workspace_id(),
                    record.label,
                    record.selected_generation.0,
                    current_generation,
                    terminal.as_str(),
                    record.detail,
                );
            } else {
                eprintln!(
                    "GNOS_UX_WORKSPACE_MANAGER_ACTIVATION workspace={:?} label={:?} selected_generation={} current_generation={} status=refused reason={} detail={:?}",
                    reference.workspace_id(),
                    record.label,
                    record.selected_generation.0,
                    current_generation,
                    terminal.as_str(),
                    record.detail,
                );
            }
        }
        ProductActionTarget::Application(reference) => {
            if success {
                println!(
                    "GNOS_UX_APPLICATION_FOCUS_ACTIVATION surface={:?} title={:?} selected_generation={} current_generation={} status=receipt code={} detail={:?}",
                    reference.surface_id(),
                    record.label,
                    record.selected_generation.0,
                    current_generation,
                    terminal.as_str(),
                    record.detail,
                );
            } else {
                eprintln!(
                    "GNOS_UX_APPLICATION_FOCUS_ACTIVATION surface={:?} title={:?} selected_generation={} current_generation={} status=refused reason={} detail={:?}",
                    reference.surface_id(),
                    record.label,
                    record.selected_generation.0,
                    current_generation,
                    terminal.as_str(),
                    record.detail,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{product_runtime::ProductActionKind, product_scene::ProductSemanticState};
    use gnos_ux_product_intent::ObservedWorkspaceReference;
    use gnosix_desktop_model::WorkspaceId;
    use std::{cell::Cell, rc::Rc};

    #[derive(Clone)]
    struct FakeSender {
        sends: Rc<Cell<u64>>,
        result: Result<ActivationClientReceipt, ActivationClientRefusal>,
    }

    impl ProductActivationSender for FakeSender {
        fn send_if_current(
            &self,
            _intent: ProductIntent,
            _current_generation: DesktopContextGeneration,
        ) -> Result<ActivationClientReceipt, ActivationClientRefusal> {
            self.sends.set(self.sends.get() + 1);
            self.result.clone()
        }
    }

    fn selected(generation: u64) -> ProductWorkspaceContent {
        ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(
                WorkspaceId::from_bytes([4; 16]),
                DesktopContextGeneration(generation),
            ),
            label: "2".to_owned(),
        }
    }

    fn runtime_with_generation(generation: u64) -> ProductRuntime {
        let mut runtime = ProductRuntime::default();
        runtime.refresh_semantic_for_test(ProductSemanticState::Current(Box::new(
            crate::product_scene::ProductContent {
                output_area: None,
                generation: DesktopContextGeneration(generation),
                connector: "HDMI-A-1".to_owned(),
                active_workspace: None,
                focused_workspace: None,
                workspace_manager_targets: vec![],
                visible_workspaces: vec![],
                applications: vec![],
                visible_application_count: 0,
                focused_application: None,
            },
        )));
        runtime
    }

    #[test]
    fn stale_selection_refuses_before_sender_factory_or_transport() {
        let controller = ProductController;
        let mut runtime = runtime_with_generation(8);
        let factory_calls = Cell::new(0);
        let sends = Rc::new(Cell::new(0));
        let sends_for_factory = Rc::clone(&sends);

        let record = controller.activate_workspace_with(
            &mut runtime,
            selected(7),
            ProductIntentRequestId::from_bytes([9; 16]),
            || {
                factory_calls.set(factory_calls.get() + 1);
                Ok(FakeSender {
                    sends: sends_for_factory,
                    result: Ok(ActivationClientReceipt {
                        code: ActivationReceiptCode::Applied,
                        detail: "unused".to_owned(),
                    }),
                })
            },
        );

        assert_eq!(factory_calls.get(), 0);
        assert_eq!(sends.get(), 0);
        assert_eq!(record.terminal, Some(ProductActionTerminal::LocalStale));
    }

    #[test]
    fn exact_current_selection_sends_once_and_records_receipt() {
        let controller = ProductController;
        let mut runtime = runtime_with_generation(7);
        let sends = Rc::new(Cell::new(0));
        let sender_sends = Rc::clone(&sends);

        let record = controller.activate_workspace_with(
            &mut runtime,
            selected(7),
            ProductIntentRequestId::from_bytes([10; 16]),
            || {
                Ok(FakeSender {
                    sends: sender_sends,
                    result: Ok(ActivationClientReceipt {
                        code: ActivationReceiptCode::Applied,
                        detail: "Applied: workspace focused".to_owned(),
                    }),
                })
            },
        );

        assert_eq!(sends.get(), 1);
        assert_eq!(record.terminal, Some(ProductActionTerminal::Applied));
        assert_eq!(
            runtime.workspace_activation_feedback().as_deref(),
            Some("2 · Applied")
        );
    }

    #[test]
    fn a_move_sends_one_intent_only_from_one_current_observation() {
        let controller = ProductController;
        let sends = Rc::new(Cell::new(0));
        let sender = |sends: &Rc<Cell<u64>>| {
            let sends = Rc::clone(sends);
            move || {
                Ok(FakeSender {
                    sends,
                    result: Ok(ActivationClientReceipt {
                        code: ActivationReceiptCode::Applied,
                        detail: "moved".to_owned(),
                    }),
                })
            }
        };
        // Window and destination from the current observation: one send.
        let mut runtime = runtime_with_generation(7);
        let record = controller.move_application_with(
            &mut runtime,
            application(7, 3),
            selected(7),
            ProductIntentRequestId::from_bytes([9; 16]),
            sender(&sends),
        );
        assert_eq!(sends.get(), 1);
        assert_eq!(record.terminal, Some(ProductActionTerminal::Applied));
        // A destination from an older observation: refused before sending.
        let record = controller.move_application_with(
            &mut runtime,
            application(7, 3),
            selected(6),
            ProductIntentRequestId::from_bytes([10; 16]),
            sender(&sends),
        );
        assert_eq!(sends.get(), 1, "nothing more sent");
        assert_eq!(record.terminal, Some(ProductActionTerminal::LocalStale));
    }

    fn application(generation: u64, surface_byte: u8) -> ProductApplicationContent {
        use gnos_ux_product_intent::ObservedSurfaceReference;
        use gnosix_desktop_model::{SurfaceFocus, SurfaceId, SurfaceVisibility};

        ProductApplicationContent {
            bounds: None,
            reference: ObservedSurfaceReference::new(
                SurfaceId::from_bytes([surface_byte; 16]),
                DesktopContextGeneration(generation),
            ),
            workspace: None,
            title: "Terminal".to_owned(),
            class: "com.example.Terminal".to_owned(),
            initial_class: "com.example.Terminal".to_owned(),
            tags: Vec::new(),
            process_id: 4242,
            xwayland: false,
            visibility: SurfaceVisibility::Visible,
            focus: SurfaceFocus::Unfocused,
        }
    }

    #[test]
    fn stale_application_selection_refuses_before_sender_factory_or_transport() {
        let controller = ProductController;
        let mut runtime = runtime_with_generation(8);
        let factory_calls = Cell::new(0);
        let sends = Rc::new(Cell::new(0));
        let sends_for_factory = Rc::clone(&sends);

        let record = controller.focus_application_with(
            &mut runtime,
            application(7, 12),
            ProductIntentRequestId::from_bytes([12; 16]),
            || {
                factory_calls.set(factory_calls.get() + 1);
                Ok(FakeSender {
                    sends: sends_for_factory,
                    result: Ok(ActivationClientReceipt {
                        code: ActivationReceiptCode::Applied,
                        detail: "unused".to_owned(),
                    }),
                })
            },
        );

        assert_eq!(factory_calls.get(), 0);
        assert_eq!(sends.get(), 0);
        assert_eq!(record.terminal, Some(ProductActionTerminal::LocalStale));
        assert!(matches!(
            record.target,
            ProductActionTarget::Application(reference)
                if reference.surface_id() == gnosix_desktop_model::SurfaceId::from_bytes([12; 16])
        ));
    }

    #[test]
    fn exact_current_application_selection_sends_once() {
        let controller = ProductController;
        let mut runtime = runtime_with_generation(7);
        let sends = Rc::new(Cell::new(0));
        let sender_sends = Rc::clone(&sends);

        let record = controller.focus_application_with(
            &mut runtime,
            application(7, 13),
            ProductIntentRequestId::from_bytes([13; 16]),
            || {
                Ok(FakeSender {
                    sends: sender_sends,
                    result: Ok(ActivationClientReceipt {
                        code: ActivationReceiptCode::Applied,
                        detail: "Applied: application focused".to_owned(),
                    }),
                })
            },
        );

        assert_eq!(sends.get(), 1);
        assert_eq!(record.kind, ProductActionKind::FocusApplication);
        assert_eq!(record.terminal, Some(ProductActionTerminal::Applied));
    }

    #[test]
    fn semantic_unavailable_refuses_before_sender_factory() {
        let controller = ProductController;
        let mut runtime = ProductRuntime::default();
        let factory_calls = Cell::new(0);

        let record = controller.activate_workspace_with::<FakeSender, _>(
            &mut runtime,
            selected(7),
            ProductIntentRequestId::from_bytes([11; 16]),
            || {
                factory_calls.set(factory_calls.get() + 1);
                unreachable!("unavailable semantics must not create activation transport")
            },
        );

        assert_eq!(factory_calls.get(), 0);
        assert_eq!(
            record.terminal,
            Some(ProductActionTerminal::SemanticUnavailable)
        );
    }
}
