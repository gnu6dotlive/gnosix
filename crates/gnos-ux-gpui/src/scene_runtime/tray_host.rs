//! RAIL-01: the systray in the scene: its interaction (typed product
//! actions), menu row intents and outcomes, and the tray interaction owner.

use super::*;

/// Dispatches one menu-row intent through the same typed path as the tray.
///
/// Rows never name protocol vocabulary: the intent carries only the immutable
/// binding the row was rendered with, and the transport revalidates it against
/// the current menu before any remote call.
pub(super) fn spawn_menu_row_intent(
    session: gpui::Entity<crate::product_tray::TraySession>,
    controller: gpui::Entity<crate::product_tray_controller::TrayActionController>,
    app: &mut gpui::App,
    intent: crate::product_tray_action::TrayIntent,
) {
    app.spawn(async move |cx| {
        let (connection, snapshot) = session.read_with(&*cx, |this, _cx| {
            (this.connection().cloned(), this.snapshot().clone())
        });
        let Some(connection) = connection else {
            apply_tray_outcome(
                &controller,
                None,
                crate::product_tray_action::TrayIntentOutcome::Unavailable,
                cx,
            );
            return;
        };
        let open_menu = controller.read_with(&*cx, |this, _cx| this.menu().cloned());
        let outcome = crate::product_tray_controller::dispatch_intent(
            &connection,
            &snapshot,
            open_menu.as_ref(),
            &intent,
        )
        .await;
        apply_tray_outcome(&controller, None, outcome, cx);
    })
    .detach();
}

/// Records a resolved outcome on the shared controller and logs the receipt.
pub(super) fn apply_tray_outcome(
    controller: &gpui::Entity<crate::product_tray_controller::TrayActionController>,
    menu: Option<crate::product_tray_menu::TrayMenuSnapshot>,
    outcome: crate::product_tray_action::TrayIntentOutcome,
    cx: &mut gpui::AsyncApp,
) {
    use crate::product_tray_action::{StaleReason, TrayIntentOutcome};
    let receipt = match &outcome {
        TrayIntentOutcome::Completed { .. } => "completed".to_owned(),
        TrayIntentOutcome::RefusedStale { reason, .. } => match reason {
            StaleReason::MembershipLost => "refused_stale_membership".to_owned(),
            StaleReason::OwnerReplaced => "refused_stale_owner".to_owned(),
            StaleReason::MenuRevisionChanged => "refused_stale_menu_revision".to_owned(),
            StaleReason::MenuItemAbsent => "refused_stale_menu_item".to_owned(),
        },
        TrayIntentOutcome::Unavailable => "unavailable".to_owned(),
        TrayIntentOutcome::Unsupported { .. } => "unsupported".to_owned(),
        TrayIntentOutcome::TransportFailure { .. } => "transport_failure".to_owned(),
    };
    let operation = format!("{:?}", outcome.operation());
    let tone = format!("{:?}", outcome.feedback_tone());

    controller.update(cx, |this, cx| this.apply(menu, outcome, cx));
    println!("GNOS_UX_TRAY_INTENT outcome={receipt} operation={operation} tone={tone}");
}

impl SceneHost {
    /// Builds the Bar's tray interaction sink for this output.
    ///
    /// The dispatch closure owns the provider session connection and the shared
    /// controller, so the Bar never sees either. The coordinate hint the widget
    /// supplies is derived from the output origin recorded on this scene host.
    pub(super) fn tray_interaction(
        &mut self,
        cx: &mut gpui::App,
    ) -> Option<crate::product_bar::BarTrayInteraction> {
        let row_target = self.tray_menu_row_target.take();
        self.build_tray_interaction(cx, row_target)
    }

    /// The rendering path has only a shared borrow, so no one-shot row target is
    /// consumed there; a row click is driven by the qualified path instead.
    pub(super) fn tray_interaction_readonly(
        &self,
        cx: &mut gpui::App,
    ) -> Option<crate::product_bar::BarTrayInteraction> {
        self.build_tray_interaction(cx, None)
    }

    pub(super) fn build_tray_interaction(
        &self,
        cx: &mut gpui::App,
        row_target: Option<String>,
    ) -> Option<crate::product_bar::BarTrayInteraction> {
        if !self.product_mode {
            return None;
        }

        // Every enabled surface installs a dispatch sink. The lease elects an
        // executor, not an interactive output: a click on a non-owning output
        // must still reach execution, so the sink is installed regardless of
        // ownership and the dispatch closure routes to the owner when needed.
        let session = crate::product_tray::TraySession::shared(cx);
        let controller = self.tray_controller.clone();
        // Mirror this process's live tray state into the relay hub on every
        // render, so a forwarded intent revalidates against real observed state
        // rather than a snapshot captured at click time or not at all.
        if let Some(connection) = session.read_with(cx, |this, _cx| this.connection().cloned()) {
            let snapshot = session.read_with(cx, |this, _cx| this.snapshot().clone());
            let open_menu = controller.read_with(cx, |this, _cx| this.menu().cloned());
            crate::product_tray_relay::publish(connection, snapshot, open_menu);
        }
        let origin = self.output_origin;
        let output_id_for = self.runtime_output.as_ref().map(|r| r.as_str().to_owned());
        let process_owns = self.tray_process_owns.clone();
        let owner_name = crate::product_tray_ownership::TRAY_INTERACTION_OWNER_NAME;

        Some(crate::product_bar::BarTrayInteraction {
            output_origin: self.output_origin,
            dispatch: std::rc::Rc::new(move |app: &mut gpui::App, intent| {
                let session = session.clone();
                let controller = controller.clone();
                let row_target = row_target.clone();
                let output_id_for = output_id_for.clone();
                let process_owns = process_owns.clone();
                app.spawn(async move |cx| {
                    let (connection, snapshot) = session.read_with(&*cx, |this, _cx| {
                        (this.connection().cloned(), this.snapshot().clone())
                    });
                    let Some(connection) = connection else {
                        apply_tray_outcome(
                            &controller,
                            None,
                            crate::product_tray_action::TrayIntentOutcome::Unavailable,
                            cx,
                        );
                        return;
                    };

                    // The menu projection is read synchronously so the await below never
                    // holds a borrow on the controller.
                    let open_menu = controller.read_with(&*cx, |this, _cx| this.menu().cloned());

                    // Keep the owner's relay hub current, so a forwarded intent
                    // revalidates against real observed state rather than a stale
                    // copy. Harmless in a non-owner: nothing serves the hub there.
                    crate::product_tray_relay::publish(
                        connection.clone(),
                        snapshot.clone(),
                        open_menu.clone(),
                    );

                    if !process_owns.load(std::sync::atomic::Ordering::SeqCst) {
                        // Not the executor: route the exact typed intent to the
                        // lease owner, which revalidates and executes. An
                        // ambiguous failure is reported, never retried.
                        let source_output = output_id_for.as_deref().unwrap_or("unknown");
                        println!(
                            "GNOS_UX_TRAY_SOURCE output={source_output} route=relay local_lease=false target={owner_name}"
                        );
                        if let crate::product_tray_action::TrayIntent::OpenMenu { item, anchor } =
                            &intent
                        {
                            // The executor projects once (remote AboutToShow and
                            // GetLayout); the projection returns here and renders
                            // on the output the user clicked.
                            let (outcome, menu) = crate::product_tray_relay::relay_open_menu(
                                &connection,
                                owner_name,
                                source_output,
                                item,
                                anchor,
                            )
                            .await;
                            if let Some(menu) = menu {
                                let revision = menu.revision;
                                // Attribute the open to this output BEFORE
                                // applying the menu: the menu-validation
                                // observer runs on the controller change and
                                // would otherwise see an open menu with no item
                                // and dismiss it.
                                let output_id = output_id_for.clone();
                                let item_ref = item.clone();
                                controller.update(cx, |this, _cx| {
                                    this.set_open(output_id, Some(item_ref));
                                });
                                apply_tray_outcome(&controller, Some(menu), outcome, cx);
                                println!(
                                    "GNOS_UX_TRAY_MENU_ROUTED output={source_output} revision={revision}"
                                );
                                // Watch the executor. If it disappears, the
                                // popup is backed by no executor and is closed
                                // rather than left actionable or migrated to a
                                // different one. A new user action may reopen.
                                let watch_connection = connection.clone();
                                let watch_owner = owner_name.to_owned();
                                let watch_output = source_output.to_owned();
                                let watch = controller.update(cx, |_this, ctx| {
                                    let weak = ctx.entity().downgrade();
                                    ctx.spawn(async move |_this, task_cx| loop {
                                        task_cx
                                            .background_executor()
                                            .timer(std::time::Duration::from_millis(400))
                                            .await;
                                        let still_open = weak
                                            .update(&mut *task_cx, |this, _cx| this.menu().is_some())
                                            .unwrap_or(false);
                                        if !still_open {
                                            return;
                                        }
                                        if crate::interaction_lease::unique_name_owner(
                                            &watch_connection,
                                            &watch_owner,
                                        )
                                        .await
                                        .is_none()
                                        {
                                            let _ = weak.update(&mut *task_cx, |this, cx| {
                                                if this.clear_menu() {
                                                    println!(
                                                        "GNOS_UX_TRAY_MENU_CLOSED output={watch_output} reason=executor_gone"
                                                    );
                                                    cx.notify();
                                                }
                                            });
                                            return;
                                        }
                                    })
                                });
                                watch.detach();
                            } else {
                                apply_tray_outcome(&controller, None, outcome, cx);
                            }
                            return;
                        }
                        let outcome = crate::product_tray_relay::relay(
                            &connection,
                            owner_name,
                            source_output,
                            &intent,
                        )
                        .await;
                        // A menu action whose executor is gone is refused, and
                        // the now-unbacked popup is closed rather than left
                        // actionable against a vanished executor. The popup is
                        // not migrated to a different executor.
                        if matches!(
                            intent,
                            crate::product_tray_action::TrayIntent::MenuEvent { .. }
                        ) && matches!(
                            outcome,
                            crate::product_tray_action::TrayIntentOutcome::Unavailable
                        ) {
                            controller.update(cx, |this, cx| {
                                this.clear_menu();
                                cx.notify();
                            });
                            println!(
                                "GNOS_UX_TRAY_MENU_CLOSED output={source_output} reason=executor_gone"
                            );
                        }
                        apply_tray_outcome(&controller, None, outcome, cx);
                        return;
                    }

                    println!(
                        "GNOS_UX_TRAY_SOURCE output={} route=local local_lease=true",
                        output_id_for.as_deref().unwrap_or("unknown")
                    );

                    let (menu, outcome) =
                        match &intent {
                            crate::product_tray_action::TrayIntent::OpenMenu { item, .. } => {
                                // Clicking the owning item again dismisses its
                                // open menu explicitly. No remote call, no new
                                // projection: the surface is simply gone.
                                let already_open = controller.read_with(&*cx, |this, _cx| {
                                    this.menu().is_some()
                                        && this.open_item().map(|open| open == item).unwrap_or(false)
                                });
                                if already_open {
                                    controller.update(cx, |this, cx| {
                                        this.clear_menu();
                                        cx.notify();
                                    });
                                    println!("GNOS_UX_TRAY_MENU_DISMISS reason=explicit_toggle");
                                    return;
                                }
                                let observed = snapshot
                                    .items()
                                    .iter()
                                    .find(|observed| observed.id == item.id)
                                    .cloned();
                                match observed {
                        Some(observed) => {
                            let (outcome, menu) =
                                crate::product_tray_controller::read_exported_menu(
                                    &connection, &observed,
                                )
                                .await;
                            (menu, outcome)
                        }
                        None => (
                            None,
                            crate::product_tray_action::TrayIntentOutcome::RefusedStale {
                                reason: crate::product_tray_action::StaleReason::MembershipLost,
                                operation: crate::product_tray_action::TrayOperation::ContextMenu,
                            },
                        ),
                    }
                            }
                            other => {
                                let outcome = crate::product_tray_controller::dispatch_intent(
                                    &connection,
                                    &snapshot,
                                    open_menu.as_ref(),
                                    other,
                                )
                                .await;
                                (None, outcome)
                            }
                        };

                    let menu_opened = menu.is_some();
                    let was_open_menu = matches!(intent, crate::product_tray_action::TrayIntent::OpenMenu { .. });
                    // Attribute the open surface to this host's output (so only
                    // the opening output renders it) and remember which exact
                    // item it belongs to, BEFORE applying the menu: the
                    // menu-validation observer runs on the controller change
                    // and would otherwise see an open menu with no item and
                    // dismiss it (the first menu after every restart was).
                    let opened_item = (menu_opened && was_open_menu).then(|| match &intent {
                        crate::product_tray_action::TrayIntent::OpenMenu { item, .. } => item.clone(),
                        _ => unreachable!(),
                    });
                    if let Some(item_ref) = opened_item.clone() {
                        let output_id = output_id_for.clone();
                        controller.update(cx, |this, _cx| this.set_open(output_id, Some(item_ref)));
                    }
                    apply_tray_outcome(&controller, menu, outcome, cx);
                    if let Some(item_ref) = opened_item {
                        let open_item = snapshot
                            .items()
                            .iter()
                            .find(|observed| observed.id == item_ref.id)
                            .cloned();
                        // Follow the exporter's own revision truth from here on,
                        // so a later LayoutUpdated re-projects before any stale
                        // row can be mistaken for current.
                        if let Some(observed) = open_item {
                            let spawn = controller.update(cx, |this, ctx| {
                                if !this.note_observing(&observed) {
                                    return None;
                                }
                                let weak = ctx.entity().downgrade();
                                let span =
                                    crate::product_tray_transport::MenuObservationSpan {
                                        item: observed,
                                        connection: connection.clone(),
                                    };
                                Some(ctx.spawn(async move |_, task_cx| {
                                    crate::product_tray_transport::run_menu_observation_loop(
                                        span, weak, &mut *task_cx,
                                    )
                                    .await;
                                }))
                            });
                            if let Some(task) = spawn {
                                task.detach();
                            }
                        }
                    }

                    // A menu-row request follows the open with a click on one
                    // entry, bound to the revision just projected.
                    if menu_opened {
                        let row_target = row_target.clone();
                        if let Some(item_id) = row_target {
                            let observed = snapshot
                                .items()
                                .iter()
                                .find(|observed| observed.id.as_str() == item_id)
                                .cloned();
                            if let Some(observed) = observed {
                                let ref_for_item = crate::product_tray_action::ExactTrayItemRef::from_observed(
                                    &observed,
                                    observed.owner.clone(),
                                );
                                let bound = controller.read_with(&*cx, |this, _cx| {
                                    let actionable = this
                                        .menu()
                                        .map(|menu| menu.actionable_items())
                                        .unwrap_or_default();
                                    actionable.first().and_then(|entry| {
                                        this.bind_menu_item(ref_for_item.clone(), entry.id)
                                    })
                                });
                                if let Some(binding) = bound
                                    && let Some(anchor) =
                                        crate::product_tray_action::TrayInteractionAnchor::from_local_point(
                                            origin,
                                            (0.5, 0.5),
                                        )
                                {
                                        let intent = crate::product_tray_action::TrayIntent::MenuEvent {
                                            binding,
                                            anchor,
                                        };
                                        let open_menu = controller
                                            .read_with(&*cx, |this, _cx| this.menu().cloned());
                                        let outcome =
                                            crate::product_tray_controller::dispatch_intent(
                                                &connection,
                                                &snapshot,
                                                open_menu.as_ref(),
                                                &intent,
                                            )
                                            .await;
                                        apply_tray_outcome(
                                            &controller,
                                            None,
                                            outcome,
                                            cx,
                                        );
                                }
                            }
                        }
                    }
                })
                .detach();
            }),
        })
    }
}

/// Process-wide tray lease result. See the Now Playing twin.
pub(super) static TRAY_PROCESS_OWNS: std::sync::OnceLock<
    std::sync::Arc<std::sync::atomic::AtomicBool>,
> = std::sync::OnceLock::new();

/// Claims tray interaction for this process, arbitrated on the bus.
///
/// Mirrors `claim_mpris_interaction_owner` deliberately. The three corrections
/// DONOR-17's gate forced are carried over intact, because each was found the
/// hard way:
///
/// * `DoNotQueue` only. `ReplaceExisting` would let a later claimant take the
///   name from the process already holding it, and would let a process's second
///   host steal from its own first.
/// * the connection is retained, because a bus name is released when its
///   connection drops. Letting it fall out of scope made ownership transient.
/// * only the first host in a process requests, or the second is answered as a
///   rival claim by the same process.
pub(super) fn claim_tray_interaction_owner(cx: &mut gpui::App) {
    // Only the first host in this process requests the name. A second request
    // from the same process would be answered as a rival claim and the process
    // would report itself as both owner and not-owner.
    static REQUESTED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
    if !crate::interaction_lease::claim_once(&REQUESTED) {
        return;
    }
    // Process-wide, for the same reason as the Now Playing lease: every scene
    // host must read the same ownership bit.
    let process_owns = TRAY_PROCESS_OWNS.get_or_init(Default::default).clone();
    let name = crate::product_tray_ownership::TRAY_INTERACTION_OWNER_NAME.to_owned();
    cx.spawn(async move |_this| {
        match crate::interaction_lease::InteractionLease::acquire(&name).await {
            Ok(lease) => {
                let owns = lease.owns();
                let relay_connection = lease.connection().clone();
                // Retain the lease for the process lifetime, for the same reason
                // as the Now Playing lease: a dropped connection releases the
                // name and the next process wins it.
                {
                    let mut held = TRAY_OWNER_CONNECTION
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    *held = Some(lease);
                }
                process_owns.store(owns, std::sync::atomic::Ordering::SeqCst);
                println!(
                    "GNOS_UX_TRAY_OWNERSHIP outcome={} name={name} connection_retained=true",
                    if owns {
                        "process_owns"
                    } else {
                        "process_not_owner"
                    },
                );
                // The owner serves the relay on the same connection that holds
                // the name, so a non-owner reaches it by the well-known name.
                if owns
                    && let Err(error) = crate::product_tray_relay::serve(&relay_connection).await
                {
                    println!("GNOS_UX_TRAY_RELAY_SERVE_FAIL error={error}");
                }
            }
            Err(error) => {
                println!("GNOS_UX_TRAY_OWNERSHIP outcome=bus_unavailable error={error}");
            }
        }
    })
    .detach();
}

/// Holds the tray interaction lease for this process's lifetime. See the
/// retention note at its only use.
pub(super) static TRAY_OWNER_CONNECTION: std::sync::Mutex<
    Option<crate::interaction_lease::InteractionLease>,
> = std::sync::Mutex::new(None);
