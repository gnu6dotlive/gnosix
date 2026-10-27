//! Qualification runs the scene can be asked for (never in a plain
//! session): tray, MPRIS, menus, R13 activation and the global projection.

use super::*;

/// Runs the stale-row causal gate for one tray item.
///
/// The sequence is deliberately ordered so the stale case is reachable at all:
/// open the menu, capture one actionable row's immutable binding, wait for the
/// exporter to publish a newer revision into product state, then invoke the
/// captured old binding. It is never refreshed or rebound before that attempt.
/// A fresh binding from the current revision follows as the positive control.
pub(super) async fn run_stale_menu_qualification(
    session: gpui::Entity<crate::product_tray::TraySession>,
    controller: gpui::Entity<crate::product_tray_controller::TrayActionController>,
    output_id: Option<String>,
    item_id: String,
    cx: &mut gpui::AsyncApp,
) {
    let (connection, snapshot) = session.read_with(&*cx, |this, _cx| {
        (this.connection().cloned(), this.snapshot().clone())
    });
    let Some(connection) = connection else {
        println!("GNOS_UX_TRAY_QUALIFICATION outcome=unavailable item={item_id}");
        return;
    };
    let Some(observed) = snapshot
        .items()
        .iter()
        .find(|observed| observed.id.as_str() == item_id)
        .cloned()
    else {
        println!("GNOS_UX_TRAY_QUALIFICATION outcome=absent item={item_id}");
        return;
    };
    let item_ref = crate::product_tray_action::ExactTrayItemRef::from_observed(
        &observed,
        observed.owner.clone(),
    );

    // 1. Open the menu and publish it as product state.
    let (outcome, menu) =
        crate::product_tray_controller::read_exported_menu(&connection, &observed).await;
    let Some(menu) = menu else {
        println!("GNOS_UX_TRAY_QUALIFICATION outcome=menu_unavailable item={item_id}");
        apply_tray_outcome(&controller, None, outcome, cx);
        return;
    };
    let open_revision = menu.revision;
    controller.update(cx, |this, _cx| {
        this.set_open(output_id, Some(item_ref.clone()));
        this.set_menu(Some(menu.clone()));
    });

    // Follow the exporter's revision truth from here, so a published layout
    // update reaches product state before the stale action is attempted.
    let spawn = controller.update(cx, |this, ctx| {
        if !this.note_observing(&observed) {
            return None;
        }
        let weak = ctx.entity().downgrade();
        let span = crate::product_tray_transport::MenuObservationSpan {
            item: observed.clone(),
            connection: connection.clone(),
        };
        Some(ctx.spawn(async move |_, task_cx| {
            crate::product_tray_transport::run_menu_observation_loop(span, weak, &mut *task_cx)
                .await;
        }))
    });
    if let Some(task) = spawn {
        task.detach();
    }

    // 2. Capture one actionable row exactly as the surface rendered it.
    let captured = controller.read_with(&*cx, |this, _cx| {
        let menu = this.menu()?;
        let actionable = menu.actionable_items();
        let entry = actionable.first()?;
        this.bind_menu_item(item_ref.clone(), entry.id)
    });
    let Some(captured) = captured else {
        println!("GNOS_UX_TRAY_QUALIFICATION outcome=no_actionable_row item={item_id}");
        return;
    };
    let captured_menu_id = captured.menu_id;
    println!(
        "GNOS_UX_TRAY_CAPTURE item={} menu_id={} revision={} label={:?}",
        item_id, captured.menu_id, captured.revision, captured.rendered_label
    );

    // 3. Wait for the exporter to publish a newer revision into product state.
    let mut advanced_to = None;
    for _ in 0..40 {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(500))
            .await;
        let current = controller.read_with(&*cx, |this, _cx| this.menu().map(|m| m.revision));
        if let Some(revision) = current
            && revision != captured.revision
        {
            advanced_to = Some(revision);
            break;
        }
    }
    match advanced_to {
        Some(revision) => {
            println!("GNOS_UX_TRAY_REVISION_ADVANCED from={open_revision} to={revision}")
        }
        None => {
            println!("GNOS_UX_TRAY_QUALIFICATION outcome=no_revision_advance item={item_id}");
            return;
        }
    }

    // 4. Invoke the OLD bound action. The surface has already repainted to the
    // newer revision; this must be refused and must not reach the exporter.
    let anchor =
        crate::product_tray_action::TrayInteractionAnchor::from_local_point((0, 0), (0.5, 0.5))
            .expect("finite anchor");
    let stale_intent = crate::product_tray_action::TrayIntent::MenuEvent {
        binding: captured,
        anchor,
    };
    let current_menu = controller.read_with(&*cx, |this, _cx| this.menu().cloned());
    let stale_outcome = crate::product_tray_controller::dispatch_intent(
        &connection,
        &snapshot,
        current_menu.as_ref(),
        &stale_intent,
    )
    .await;
    apply_tray_outcome(&controller, None, stale_outcome.clone(), cx);
    println!("GNOS_UX_TRAY_STALE_ACTION outcome={:?}", stale_outcome);

    // 5. Positive control: a fresh binding from the current revision.
    //    Deliberately a DIFFERENT entry than the captured one, so the remote
    //    journal attributes each invocation unambiguously: an Event carrying the
    //    captured id could only have come from the stale attempt.
    let fresh = controller.read_with(&*cx, |this, _cx| {
        let menu = this.menu()?;
        let actionable = menu.actionable_items();
        let entry = actionable
            .iter()
            .find(|entry| entry.id != captured_menu_id)
            .or_else(|| actionable.first())?;
        this.bind_menu_item(item_ref.clone(), entry.id)
    });
    if let Some(fresh) = fresh {
        let fresh_intent = crate::product_tray_action::TrayIntent::MenuEvent {
            binding: fresh.clone(),
            anchor,
        };
        let fresh_outcome = crate::product_tray_controller::dispatch_intent(
            &connection,
            &snapshot,
            current_menu.as_ref(),
            &fresh_intent,
        )
        .await;
        apply_tray_outcome(&controller, None, fresh_outcome.clone(), cx);
        println!(
            "GNOS_UX_TRAY_FRESH_ACTION revision={} outcome={:?}",
            fresh.revision, fresh_outcome
        );
    }

    controller.update(cx, |this, _cx| {
        this.clear_menu();
    });
}

/// DONOR-17 section 10: drives the live transport gate.
///
/// The sequence is ordered so each case is reachable and each is attributable
/// against the exporter's own journal:
///   1. observe the live players and publish them as product state
///   2. dispatch play/pause, next and previous against the effective player
///   3. dispatch an operation the player does not advertise, which must be
///      refused locally before any call is made
pub(super) async fn run_mpris_transport_qualification(cx: &mut gpui::AsyncApp) {
    let session = crate::product_mpris_session::MprisSession::shared_async(cx);
    let Some(connection) = session.read_with(&*cx, |this, _cx| this.connection().cloned()) else {
        println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_connection");
        return;
    };

    // 1. Observe.
    let snapshot = crate::product_mpris_transport::discover_players(&connection).await;
    let observed = snapshot.len();
    session.update(cx, |this, cx| {
        this.set_snapshot(snapshot.clone(), cx);
        this.set_observing(!snapshot.is_empty(), cx);
    });

    // A qualification names the player it intends to exercise. Selection is
    // deliberately not used here: it resolves deterministically by well-known
    // name, so on a machine with other players running it would drive a
    // bystander instead of the exporter this run is judging.
    let target = crate::product_mpris_session::qualification_target();
    let effective = snapshot
        .players()
        .iter()
        .find(|player| target.as_deref() == Some(player.identity.service.as_str()))
        .cloned()
        .or_else(|| {
            crate::product_mpris::MprisPresentation::select(snapshot.players())
                .effective()
                .cloned()
        });
    let Some(effective) = effective else {
        println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_player observed={observed}");
        return;
    };
    println!(
        "GNOS_UX_MPRIS_QUALIFICATION phase=observed players={observed} player={} owner={} state={:?} capabilities=control:{},play:{},pause:{},next:{},previous:{}",
        effective.label,
        effective.identity.owner,
        effective.playback,
        effective.capabilities.can_control,
        effective.capabilities.can_play,
        effective.capabilities.can_pause,
        effective.capabilities.can_go_next,
        effective.capabilities.can_go_previous,
    );

    // 2. Dispatch each advertised operation exactly once, in a fixed order, so
    //    the journal can attribute every call.
    for operation in [
        crate::product_mpris::MprisOperation::PlayPause,
        crate::product_mpris::MprisOperation::Next,
        crate::product_mpris::MprisOperation::Previous,
    ] {
        let intent = match operation {
            crate::product_mpris::MprisOperation::Previous => {
                crate::product_mpris_transport::MprisIntent::previous(effective.identity.clone())
            }
            crate::product_mpris::MprisOperation::Next => {
                crate::product_mpris_transport::MprisIntent::next(effective.identity.clone())
            }
            _ => {
                crate::product_mpris_transport::MprisIntent::play_pause(effective.identity.clone())
            }
        };
        let outcome =
            crate::product_mpris_transport::dispatch(&connection, &snapshot, &intent).await;
        session.update(cx, |this, cx| {
            this.set_last_outcome(Some(outcome.clone()), cx);
        });
        println!(
            "GNOS_UX_MPRIS_QUALIFICATION phase=dispatch operation={} delivered={} outcome={:?}",
            operation.name(),
            outcome.is_delivered(),
            outcome
        );
    }

    // 3. A capability the exporter does not advertise. Seek is chosen when the
    //    exporter advertises it but the product surface never exercises it:
    //    dispatching it must be refused as unsupported, because a transport
    //    action the surface cannot offer has no place in the product.
    //
    //    An earlier version of this gate picked its forbidden operation from the
    //    player's own advertised capabilities and so, on a player advertising
    //    everything, fell through to Seek and dispatched it. That was a defect
    //    in the gate, not in the transport: the refusal being sought is a
    //    product-scope refusal, so it must not depend on the exporter being
    //    incomplete.
    let forbidden = crate::product_mpris::MprisOperation::Seek(std::time::Duration::from_secs(5));
    let intent = crate::product_mpris_transport::MprisIntent {
        player: effective.identity.clone(),
        operation: forbidden,
    };
    let outcome = crate::product_mpris_transport::dispatch(&connection, &snapshot, &intent).await;
    println!(
        "GNOS_UX_MPRIS_QUALIFICATION phase=capability_gate operation={} refused_locally={} outcome={:?}",
        forbidden.name(),
        matches!(
            outcome,
            crate::product_mpris_transport::MprisTransportOutcome::Unsupported { .. }
        ),
        outcome
    );

    // 4. Re-observe so product state reflects what the transport did.
    let after = crate::product_mpris_transport::discover_players(&connection).await;
    session.update(cx, |this, cx| {
        this.set_snapshot(after, cx);
    });
    println!("GNOS_UX_MPRIS_QUALIFICATION phase=complete");
}

/// DONOR-17: captures an exact player ref, waits for the exporter to
/// re-request its well-known name under a different owner, then invokes the
/// captured ref. The captured ref must be refused with zero remote calls, and a
/// freshly bound ref for the replacement must dispatch exactly once.
pub(super) async fn run_stale_mpris_qualification(cx: &mut gpui::AsyncApp) {
    let session = crate::product_mpris_session::MprisSession::shared_async(cx);
    let Some(connection) = session.read_with(&*cx, |this, _cx| this.connection().cloned()) else {
        println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_connection");
        return;
    };

    let snapshot = crate::product_mpris_transport::discover_players(&connection).await;
    let target = crate::product_mpris_session::qualification_target();
    let observed_player = target
        .as_deref()
        .and_then(|service| {
            snapshot
                .players()
                .iter()
                .find(|player| player.identity.service == service)
        })
        .or_else(|| snapshot.players().first())
        .cloned();
    let Some(player) = observed_player else {
        println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_player");
        return;
    };
    session.update(cx, |this, cx| {
        this.set_snapshot(snapshot.clone(), cx);
    });

    // The captured ref is held across the owner change and is never rebound.
    let captured = player.identity.clone();
    println!(
        "GNOS_UX_MPRIS_CAPTURE player={} owner={} service={}",
        player.label, captured.owner, captured.service
    );

    // Wait for the exporter to re-request its name under a different owner.
    // The observation loop must see the replacement before the stale action is
    // attempted, or the case is not reachable at all.
    let mut advanced_to = None;
    for _ in 0..40 {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(500))
            .await;
        let current = crate::product_mpris_transport::discover_players(&connection).await;
        let target_now = crate::product_mpris_session::qualification_target();
        if let Some(replacement) = target_now
            .as_deref()
            .and_then(|service| {
                current
                    .players()
                    .iter()
                    .find(|player| player.identity.service == service)
            })
            .or_else(|| current.players().first())
            && replacement.identity.owner != captured.owner
        {
            advanced_to = Some(replacement.identity.owner.clone());
            session.update(cx, |this, cx| {
                this.set_snapshot(current.clone(), cx);
            });
            break;
        }
    }
    match advanced_to {
        Some(owner) => println!(
            "GNOS_UX_MPRIS_OWNER_REPLACED from={} to={owner}",
            captured.owner
        ),
        None => {
            println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_owner_replacement");
            return;
        }
    }

    // The captured ref, dispatched against post-replacement product state.
    let after = session.read_with(&*cx, |this, _cx| this.snapshot().clone());
    let stale_intent = crate::product_mpris_transport::MprisIntent {
        player: captured.clone(),
        operation: crate::product_mpris::MprisOperation::PlayPause,
    };
    let stale_outcome =
        crate::product_mpris_transport::dispatch(&connection, &after, &stale_intent).await;
    println!(
        "GNOS_UX_MPRIS_STALE_ACTION delivered={} outcome={:?}",
        stale_outcome.is_delivered(),
        stale_outcome
    );

    // The fresh ref for the replacement, as the positive control. Same target
    // rule as the capture, so the control acts on the replacement of the player
    // under test rather than on a bystander.
    let target = crate::product_mpris_session::qualification_target();
    let Some(replacement) = target
        .as_deref()
        .and_then(|service| {
            after
                .players()
                .iter()
                .find(|p| p.identity.service == service)
        })
        .or_else(|| after.players().first())
        .cloned()
    else {
        println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_replacement_player");
        return;
    };
    let fresh_intent = crate::product_mpris_transport::MprisIntent {
        player: replacement.identity.clone(),
        operation: crate::product_mpris::MprisOperation::PlayPause,
    };
    let fresh_outcome =
        crate::product_mpris_transport::dispatch(&connection, &after, &fresh_intent).await;
    println!(
        "GNOS_UX_MPRIS_FRESH_ACTION owner={} delivered={} outcome={:?}",
        replacement.identity.owner,
        fresh_outcome.is_delivered(),
        fresh_outcome
    );
}

/// A bounded tray qualification request.
///
/// Only a product-level tray item id and an intent kind are accepted. No bus
/// address, object path, interface, method, or coordinate can be supplied, so
/// this cannot become an external-call interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TrayQualificationRequest {
    pub(super) item_id: String,
    pub(super) kind: TrayQualificationKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrayQualificationKind {
    Primary,
    Secondary,
    /// EXTERNAL-RUNTIME-01: fires one host-local sink per process, routed the
    /// same way a real click is (owner executes, non-owner relays to it).
    /// Deliberately not gated on lease ownership: this is the decisive
    /// multiplicity proof, and a broadcast gated on ownership cannot prove
    /// that a non-owner output is actionable.
    LocalPrimary,
    /// Re-reads the exported menu, then dispatches a click on one entry.
    MenuRow,
    /// Opens a menu, captures one actionable row's immutable binding, waits for
    /// the exporter to publish a newer revision, then invokes the captured old
    /// binding and a fresh one, in that order.
    StaleMenuRow,
    /// EXTERNAL-RUNTIME-01: fires one host-local Now Playing sink per process
    /// (PlayPause on the effective player), routed the same way a real control
    /// click is. Not gated on the gate-runner claim: the decisive proof that a
    /// non-owner output's Now Playing controls are actionable.
    LocalMpris,
    /// TRAY-MENU-OUTPUT-01: opens the item's menu through the host-local sink,
    /// then clicks its first row, so the routed-menu projection and the routed
    /// menu action are both exercised from one output.
    LocalMenu,
    /// DONOR-17: re-reads the live MPRIS players, then dispatches each transport
    /// operation in turn through the revalidating typed path.
    MprisTransport,
    /// DONOR-17: captures an exact player ref, waits for the exporter to
    /// re-request its well-known name under a different owner, then invokes the
    /// captured ref.
    StaleMprisPlayer,
}

impl TrayQualificationKind {
    pub(super) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "primary" => Some(Self::Primary),
            "local-primary" => Some(Self::LocalPrimary),
            "secondary" => Some(Self::Secondary),
            "menu-row" => Some(Self::MenuRow),
            "stale-menu-row" => Some(Self::StaleMenuRow),
            "mpris-transport" => Some(Self::MprisTransport),
            "local-mpris" => Some(Self::LocalMpris),
            "local-menu" => Some(Self::LocalMenu),
            "stale-mpris-player" => Some(Self::StaleMprisPlayer),
            _ => None,
        }
    }
}

/// Reads the bounded qualification request from argv.
pub(super) fn tray_qualification_request(args: &[String]) -> Option<TrayQualificationRequest> {
    let flag = "--qualify-tray-action";
    let index = args.iter().position(|arg| arg == flag)?;
    let item_id = args.get(index + 1)?.clone();
    let kind = TrayQualificationKind::parse(args.get(index + 2)?)?;
    Some(TrayQualificationRequest { item_id, kind })
}

impl SceneHost {
    /// Serves the bounded tray qualification request, if one was supplied.
    ///
    /// Everything the dispatch needs is resolved from live observed state: the
    /// bus address and owner from the provider snapshot, and the coordinate hint
    /// from this output's origin and the item's index in the real presentation
    /// run. Nothing protocol-shaped can be supplied from the command line.
    pub(super) fn serve_tray_qualification(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(request) = self.tray_qualification.clone() else {
            return;
        };
        // One dispatch per process: a process hosts one scene host per output,
        // and a broadcast must not become one call per host. The guard is set
        // only when a dispatch actually happens, so the absent-item retry loop
        // can still reach resolution in a second host.
        static QUALIFICATION_FIRED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
        // The MPRIS gates are their own sequences: each must observe and hold
        // state across an external change, which a single dispatch cannot
        // express.
        //
        // Only one process may run them. A process-local claim is not enough:
        // each output is its own process, and the multi-output gate proved
        // that three processes each believed they owned the run, producing
        // nine remote calls where three were intended. The interaction-owner
        // bus name arbitrates instead, so exactly one process across all
        // outputs may drive a gate. A process that loses observes but does not
        // run: it still renders the same product state, which is precisely the
        // case multi-output ownership exists to cover.
        if matches!(
            request.kind,
            TrayQualificationKind::MprisTransport | TrayQualificationKind::StaleMprisPlayer
        ) {
            self.tray_qualification = None;
            let kind = request.kind;
            crate::product_mpris_session::set_qualification_target(format!(
                "org.mpris.MediaPlayer2.{}",
                request.item_id
            ));
            let session = crate::product_mpris_session::MprisSession::shared(cx);
            session.update(cx, |this, cx| {
                crate::product_mpris_session::ensure_connection(this, cx);
            });
            cx.spawn(async move |_this, task_cx| {
                // The bus connection is established by the session itself; wait
                // for it rather than assuming the observer has already run.
                let mut connection_ready = false;
                for _ in 0..40 {
                    if session.read_with(&*task_cx, |this, _cx| this.connection().is_some()) {
                        connection_ready = true;
                        break;
                    }
                    task_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                }
                if !connection_ready {
                    println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_connection");
                    return;
                }
                // Exactly one process across all outputs may run the gate. The
                // bus name decides; every other process observes but declines.
                let owns_run = session
                    .read_with(&*task_cx, |this, _cx| this.connection().cloned())
                    .is_some_and(|connection| {
                        futures::executor::block_on(claim_gate_run(connection))
                    });
                if !owns_run {
                    println!("GNOS_UX_MPRIS_QUALIFICATION outcome=already_claimed kind={kind:?}");
                    return;
                }
                match kind {
                    TrayQualificationKind::MprisTransport => {
                        run_mpris_transport_qualification(task_cx).await;
                    }
                    _ => {
                        run_stale_mpris_qualification(task_cx).await;
                    }
                }
            })
            .detach();
            return;
        }
        // EXTERNAL-RUNTIME-01 decisive MPRIS path: every process fires one
        // host-local Now Playing sink, routed exactly like a real control
        // click. The gate-runner claim is deliberately not consulted, because a
        // gate claim proves which process may run a script, not that a
        // non-owner output's controls are actionable.
        if request.kind == TrayQualificationKind::LocalMpris {
            self.tray_qualification = None;
            crate::product_mpris_session::set_qualification_target(format!(
                "org.mpris.MediaPlayer2.{}",
                request.item_id
            ));
            let session = crate::product_mpris_session::MprisSession::shared(cx);
            session.update(cx, |this, cx| {
                crate::product_mpris_session::ensure_connection(this, cx);
            });
            let process_owns = self.mpris_process_owns.clone();
            let output = self.runtime_output.as_ref().map(|r| r.as_str().to_owned());
            cx.spawn(async move |_this, task_cx| {
                let mut connection_ready = false;
                for _ in 0..40 {
                    if session.read_with(&*task_cx, |this, _cx| this.connection().is_some()) {
                        connection_ready = true;
                        break;
                    }
                    task_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                }
                if !connection_ready {
                    println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_connection");
                    return;
                }
                // Wait for the named player to be observed.
                let mut intent = None;
                for _ in 0..80 {
                    let snapshot =
                        session.read_with(&*task_cx, |this, _cx| this.snapshot().clone());
                    let target = crate::product_mpris_session::qualification_target();
                    let player = snapshot
                        .players()
                        .iter()
                        .find(|player| target.as_deref() == Some(player.identity.service.as_str()))
                        .cloned()
                        .or_else(|| {
                            crate::product_mpris::MprisPresentation::select(snapshot.players())
                                .effective()
                                .cloned()
                        });
                    if let Some(player) = player {
                        intent = Some(crate::product_mpris_transport::MprisIntent::play_pause(
                            player.identity.clone(),
                        ));
                        break;
                    }
                    task_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                }
                let Some(intent) = intent else {
                    println!("GNOS_UX_MPRIS_QUALIFICATION outcome=no_player");
                    return;
                };
                // One host-local sink per process, routed by lease ownership.
                {
                    let mut fired = QUALIFICATION_FIRED
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if *fired {
                        println!("GNOS_UX_MPRIS_QUALIFICATION outcome=local_already_fired");
                        return;
                    }
                    *fired = true;
                }
                let outcome = route_mpris_intent(
                    &session,
                    &process_owns,
                    output.as_deref().unwrap_or("unknown"),
                    intent,
                    task_cx,
                )
                .await;
                println!(
                    "GNOS_UX_MPRIS_SOURCE output={} local_lease={} delivered={}",
                    output.as_deref().unwrap_or("unknown"),
                    process_owns.load(std::sync::atomic::Ordering::SeqCst),
                    outcome.is_delivered()
                );
            })
            .detach();
            return;
        }
        // The target may not have reached the provider yet, so an absent item is
        // retried against the next snapshot instead of being consumed. The
        // attempt bound keeps a request for an item that never arrives from
        // retrying for the life of the session.
        self.tray_qualification_attempts = self.tray_qualification_attempts.saturating_add(1);
        if self.tray_qualification_attempts > 8 {
            self.tray_qualification = None;
            println!(
                "GNOS_UX_TRAY_QUALIFICATION outcome=expired item={}",
                request.item_id
            );
            return;
        }

        let presentation = crate::product_bar::BarTrayPresentation::derive(
            self.product_runtime.tray(),
            self.product_runtime.preferences(),
        );
        let Some(index) = presentation
            .items
            .iter()
            .position(|item| item.id.as_str() == request.item_id)
        else {
            println!(
                "GNOS_UX_TRAY_QUALIFICATION outcome=absent item={}",
                request.item_id
            );
            return;
        };
        // The anchor comes from the item's real position in the rendered run.
        let orientation = crate::product_bar::BarTrayPresentation::orientation_for(
            self.product_runtime.preferences(),
        );
        let count = presentation.items.len();
        let step = if count == 0 { 0.0 } else { 1.0 / count as f32 };
        let local = match orientation {
            crate::product_bar::BarOrientation::Horizontal => ((index as f32 + 0.5) * step, 0.5),
            crate::product_bar::BarOrientation::Vertical => (0.5, (index as f32 + 0.5) * step),
        };
        let Some(anchor) = crate::product_tray_action::TrayInteractionAnchor::from_local_point(
            self.output_origin,
            local,
        ) else {
            println!(
                "GNOS_UX_TRAY_QUALIFICATION outcome=no_anchor item={}",
                request.item_id
            );
            return;
        };

        let intent = match request.kind {
            // The MPRIS kinds returned above and never reach the tray path.
            TrayQualificationKind::MprisTransport
            | TrayQualificationKind::StaleMprisPlayer
            | TrayQualificationKind::LocalMpris => {
                return;
            }
            TrayQualificationKind::Primary | TrayQualificationKind::LocalPrimary => presentation
                .primary_intent(index, anchor)
                .expect("index resolved above"),
            TrayQualificationKind::LocalMenu => presentation
                .primary_intent(index, anchor)
                .expect("index resolved above"),
            TrayQualificationKind::Secondary => presentation
                .secondary_intent(index, anchor)
                .expect("index resolved above"),
            TrayQualificationKind::MenuRow | TrayQualificationKind::StaleMenuRow => presentation
                .primary_intent(index, anchor)
                .expect("index resolved above"),
        };

        println!(
            "GNOS_UX_TRAY_QUALIFICATION outcome=resolved item={} kind={:?} anchor={:?}",
            request.item_id, request.kind, anchor.screen
        );

        if request.kind == TrayQualificationKind::LocalPrimary {
            self.tray_qualification = None;
            {
                let mut fired = QUALIFICATION_FIRED
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if *fired {
                    println!(
                        "GNOS_UX_TRAY_QUALIFICATION outcome=local_already_fired item={}",
                        request.item_id
                    );
                    return;
                }
                *fired = true;
            }
            // The local sink is the same one a real click reaches: it routes by
            // lease ownership inside the dispatch closure.
            if let Some(handler) = self.tray_interaction(cx) {
                (handler.dispatch)(cx, intent);
            }
            println!(
                "GNOS_UX_TRAY_QUALIFICATION outcome=local_dispatched item={}",
                request.item_id
            );
            return;
        }

        // TRAY-MENU-OUTPUT-01: open the menu through the local sink (routed to
        // the executor when this process is not the owner), then click the first
        // row through the same sink. The routed projection must return here and
        // render on this output, and the routed row click must execute once.
        if request.kind == TrayQualificationKind::LocalMenu {
            self.tray_qualification = None;
            {
                let mut fired = QUALIFICATION_FIRED
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if *fired {
                    println!(
                        "GNOS_UX_TRAY_QUALIFICATION outcome=local_already_fired item={}",
                        request.item_id
                    );
                    return;
                }
                *fired = true;
            }
            let item_ref = intent.item().clone();
            let Some(handler) = self.tray_interaction(cx) else {
                return;
            };
            let dispatch = handler.dispatch.clone();
            (dispatch)(
                cx,
                crate::product_tray_action::TrayIntent::OpenMenu {
                    item: item_ref,
                    anchor,
                },
            );
            println!(
                "GNOS_UX_TRAY_QUALIFICATION outcome=local_menu_open item={}",
                request.item_id
            );
            let controller = self.tray_controller.clone();
            cx.spawn(async move |_this, task_cx| {
                for _ in 0..60 {
                    task_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                    let binding = controller.read_with(&*task_cx, |this, _cx| {
                        let menu = this.menu()?;
                        let open = this.open_item()?.clone();
                        let items = menu.actionable_items();
                        let entry = items.first()?;
                        menu.bind(
                            open,
                            entry.id,
                            crate::product_tray_action::TrayMenuEvent::Clicked,
                        )
                    });
                    if let Some(binding) = binding {
                        task_cx.update(|app| {
                            (dispatch)(
                                app,
                                crate::product_tray_action::TrayIntent::MenuEvent {
                                    binding,
                                    anchor,
                                },
                            );
                        });
                        println!(
                            "GNOS_UX_TRAY_QUALIFICATION outcome=local_menu_event item={}",
                            request.item_id
                        );
                        return;
                    }
                }
                println!(
                    "GNOS_UX_TRAY_QUALIFICATION outcome=local_menu_no_rows item={}",
                    request.item_id
                );
            })
            .detach();
            return;
        }

        // Exactly one process across all outputs performs the invocation.
        //
        // The previous implementation claimed with a process-global, which
        // cannot arbitrate between outputs because each output is its own
        // process. DONOR-17's multi-output gate falsified the same assumption
        // for Now Playing: three processes each claimed and nine calls were made
        // where three were intended. This claim is therefore the same bus-name
        // lease the MPRIS path uses.
        let owns_run = self
            .tray_process_owns
            .load(std::sync::atomic::Ordering::SeqCst);
        if !owns_run {
            println!(
                "GNOS_UX_TRAY_QUALIFICATION outcome=already_claimed item={}",
                request.item_id
            );
            self.tray_qualification = None;
            return;
        }
        self.tray_qualification = None;

        {
            let mut fired = QUALIFICATION_FIRED
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *fired {
                return;
            }
            *fired = true;
        }

        match request.kind {
            // The MPRIS kinds returned above and never reach the tray path.
            TrayQualificationKind::MprisTransport
            | TrayQualificationKind::StaleMprisPlayer
            | TrayQualificationKind::LocalMpris => {}
            TrayQualificationKind::MenuRow => {
                // Open the menu first; the row click follows through the same
                // typed controller path a rendered menu row would use.
                self.tray_menu_row_target = Some(request.item_id.clone());
                if let Some(handler) = self.tray_interaction(cx) {
                    (handler.dispatch)(cx, intent);
                }
            }
            TrayQualificationKind::StaleMenuRow => {
                // The gate runs as its own sequence: it must hold one captured
                // binding across a revision change, which a single dispatch
                // cannot express.
                let session = crate::product_tray::TraySession::shared(cx);
                let controller = self.tray_controller.clone();
                let output_id = self
                    .runtime_output
                    .as_ref()
                    .map(|output| output.as_str().to_owned());
                let item_id = request.item_id.clone();
                cx.spawn(async move |_, task_cx| {
                    run_stale_menu_qualification(
                        session,
                        controller,
                        output_id,
                        item_id,
                        &mut *task_cx,
                    )
                    .await;
                })
                .detach();
            }
            TrayQualificationKind::Primary
            | TrayQualificationKind::Secondary
            | TrayQualificationKind::LocalPrimary
            | TrayQualificationKind::LocalMenu => {
                if let Some(handler) = self.tray_interaction(cx) {
                    (handler.dispatch)(cx, intent);
                }
            }
        }
    }
}

pub(super) fn run_global_projection_qualification(
    source: CompositorIntentSource,
) -> Result<(), String> {
    let backend =
        HyprctlLuaBackend::from_env().map_err(|error| format!("backend_init:{error:?}"))?;

    let outputs = backend
        .observe_outputs()
        .map_err(|error| format!("observe_outputs:{error:?}"))?;
    let first_output = outputs
        .first()
        .ok_or_else(|| "observe_outputs:empty".to_owned())?;
    let output_target = RuntimeOutputIdentity {
        session_signature: backend.runtime_target().to_owned(),
        name: first_output.name.clone(),
    };
    validate_runtime_output_target(&output_target, &outputs)
        .map_err(|error| format!("output_target:{error:?}"))?;
    println!(
        "GNOSIX_RUNTIME_OUTPUT_TARGET_PASS session={} output={}",
        backend.runtime_target(),
        output_target.name
    );

    let probe = HyprlandGlobalMutation::QualificationBorderSize(0);
    let before = backend
        .observe(probe)
        .map_err(|error| format!("observe_before:{error:?}"))?;
    let requested = if before < 64 {
        before + 1
    } else {
        before.saturating_sub(1)
    };
    if requested == before {
        return Err("qualification_mutation_has_no_delta".to_owned());
    }

    let mut boundary = ProjectionBoundary::default();
    let mut receipt = boundary.apply_global(
        &backend,
        GlobalProjectionIntent {
            source,
            mutation: HyprlandGlobalMutation::QualificationBorderSize(requested),
            expected_before: Some(before),
        },
    );
    if receipt.application_outcome != ApplicationOutcome::Applied
        || receipt.observed_after != Some(requested)
    {
        return Err(format!(
            "apply_outcome={:?} before={:?} after={:?}",
            receipt.application_outcome, receipt.observed_before, receipt.observed_after
        ));
    }

    let stale_source = CompositorIntentSource {
        scene_id: source.scene_id,
        scene_epoch: SceneEpoch::new(source.scene_epoch.value().saturating_sub(1)),
    };
    let stale = boundary.apply_global(
        &backend,
        GlobalProjectionIntent {
            source: stale_source,
            mutation: HyprlandGlobalMutation::QualificationBorderSize(requested + 1),
            expected_before: Some(requested),
        },
    );
    if stale.application_outcome != ApplicationOutcome::Stale {
        return Err(format!(
            "stale_fence_outcome={:?}",
            stale.application_outcome
        ));
    }

    boundary.rollback_global(&backend, &mut receipt);
    if receipt.rollback_outcome != Some(RollbackOutcome::Restored)
        || receipt.observed_final != Some(before)
    {
        return Err(format!(
            "rollback_outcome={:?} final={:?}",
            receipt.rollback_outcome, receipt.observed_final
        ));
    }

    println!(
        "GNOSIX_HYPRLAND_GLOBAL_PROJECTION_PASS session={} scene_id={} epoch={} before={} requested={} observed_after={} stale=STALE rollback=RESTORED final={}",
        backend.runtime_target(),
        source.scene_id.value(),
        source.scene_epoch.value(),
        before,
        requested,
        receipt.observed_after.unwrap_or(requested),
        receipt.observed_final.unwrap_or(before)
    );
    Ok(())
}

#[cfg(feature = "r13-production-client")]
pub(super) fn r13_qualification_output_from(
    args: &[String],
    environment_output: Option<String>,
) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--r13-qualify-active-workspace="))
        .filter(|connector| !connector.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| environment_output.filter(|connector| !connector.trim().is_empty()))
}

#[cfg(feature = "r13-production-client")]
pub(super) fn r13_qualification_output(args: &[String]) -> Option<String> {
    r13_qualification_output_from(args, env::var("GNOSIX_R13_QUALIFY_ACTIVE_WORKSPACE").ok())
}

#[cfg(feature = "r13-production-client")]
pub(super) fn r13_application_focus_qualification_output(args: &[String]) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--r13-qualify-focus-application="))
        .filter(|connector| !connector.trim().is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(feature = "r13-production-client")]
pub(super) fn r13_stale_qualification_output(args: &[String]) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("--r13-qualify-stale-active-workspace="))
        .filter(|connector| !connector.trim().is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(feature = "r13-production-client")]
pub(super) fn run_r13_stale_active_workspace_qualification(connector: &str) -> i32 {
    let backend = match HyprctlLuaBackend::from_env() {
        Ok(backend) => backend,
        Err(error) => {
            eprintln!("GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=backend reason={error:?}");
            return 1;
        }
    };
    let mut semantic_runtime = DesktopSemanticRuntime::default();

    let first = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=select_observation reason={error:?}"
            );
            return 1;
        }
    };
    let first_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let output = RuntimeId::new(connector);
    let selected_state = ProductSemanticState::from_projection(&output, first_projection.as_ref());
    let Some(selected) = (match selected_state {
        ProductSemanticState::Current(content) => content.active_workspace.clone(),
        ProductSemanticState::Unavailable | ProductSemanticState::Invalid => None,
    }) else {
        eprintln!(
            "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_active_workspace",
            connector, first.generation
        );
        return 1;
    };
    println!(
        "GNOS_UX_R13_STALE_QUALIFICATION_ARMED output={} selected_generation={} waiting_for_new_generation=true",
        connector, first.generation
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let second = loop {
        match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
            Ok(receipt) if receipt.generation != first.generation => break receipt,
            Ok(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(receipt) => {
                eprintln!(
                    "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=await_generation output={} selected_generation={} current_generation={} reason=timeout",
                    connector, first.generation, receipt.generation
                );
                return 1;
            }
            Err(error) => {
                eprintln!(
                    "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=await_generation reason={error:?}"
                );
                return 1;
            }
        }
    };

    let second_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let mut product_runtime =
        ProductRuntime::from_projection(Some(&output), second_projection.as_ref());
    let record = ProductController.activate_workspace(&mut product_runtime, selected);

    if record.terminal == Some(ProductActionTerminal::LocalStale)
        && record.selected_generation == DesktopContextGeneration(first.generation)
        && record.current_generation == Some(DesktopContextGeneration(second.generation))
    {
        println!(
            "GNOS_UX_R13_STALE_QUALIFICATION_PASS output={} selected_generation={} current_generation={} transport=NOT_CALLED",
            connector, record.selected_generation.0, second.generation
        );
        0
    } else {
        eprintln!(
            "GNOS_UX_R13_STALE_QUALIFICATION_FAIL stage=send output={} selected_generation={} current_generation={} terminal={:?} detail={}",
            connector, first.generation, second.generation, record.terminal, record.detail
        );
        1
    }
}

#[cfg(feature = "r13-production-client")]
pub(super) fn run_r13_application_focus_qualification(connector: &str) -> i32 {
    let backend = match HyprctlLuaBackend::from_env() {
        Ok(backend) => backend,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=backend reason={error:?}"
            );
            return 1;
        }
    };
    let mut semantic_runtime = DesktopSemanticRuntime::default();
    let output = RuntimeId::new(connector);

    let first = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_observation reason={error:?}"
            );
            return 1;
        }
    };
    let first_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let first_state = ProductSemanticState::from_projection(&output, first_projection.as_ref());
    let ProductSemanticState::Current(first_content) = first_state else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=semantic_unavailable",
            connector, first.generation
        );
        return 1;
    };
    let Some(original) = first_content.focused_application.clone() else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_focused_application",
            connector, first.generation
        );
        return 1;
    };
    let Some(selected) = first_content
        .applications
        .iter()
        .find(|application| {
            application.reference.surface_id() != original.reference.surface_id()
                && application.visibility == SurfaceVisibility::Visible
        })
        .cloned()
    else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_alternate_visible_application",
            connector, first.generation
        );
        return 1;
    };

    let second = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=send_fence_observation reason={error:?}"
            );
            return 1;
        }
    };
    let second_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let mut product_runtime =
        ProductRuntime::from_projection(Some(&output), second_projection.as_ref());
    let controller = ProductController;
    let first_record = controller.focus_application(&mut product_runtime, selected.clone());
    if first_record.terminal != Some(ProductActionTerminal::Applied)
        || first_record.current_generation != Some(DesktopContextGeneration(second.generation))
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=focus_selected output={} terminal={:?} detail={}",
            connector, first_record.terminal, first_record.detail
        );
        return 1;
    }

    let third = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=selected_post_observation reason={error:?}"
            );
            return 1;
        }
    };
    let third_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let third_state = ProductSemanticState::from_projection(&output, third_projection.as_ref());
    let ProductSemanticState::Current(third_content) = third_state else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=selected_postcondition reason=semantic_unavailable"
        );
        return 1;
    };
    if !third_content
        .focused_application
        .as_ref()
        .is_some_and(|application| {
            application.reference.surface_id() == selected.reference.surface_id()
        })
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=selected_postcondition reason=surface_not_focused"
        );
        return 1;
    }

    let Some(restore_target) = third_content
        .applications
        .iter()
        .find(|application| application.reference.surface_id() == original.reference.surface_id())
        .cloned()
    else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_target reason=original_surface_disappeared"
        );
        return 1;
    };
    let mut restore_runtime =
        ProductRuntime::from_projection(Some(&output), third_projection.as_ref());
    let restore_record = controller.focus_application(&mut restore_runtime, restore_target);
    if restore_record.terminal != Some(ProductActionTerminal::Applied)
        || restore_record.current_generation != Some(DesktopContextGeneration(third.generation))
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_send output={} terminal={:?} detail={}",
            connector, restore_record.terminal, restore_record.detail
        );
        return 1;
    }

    let fourth = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_post_observation reason={error:?}"
            );
            return 1;
        }
    };
    let fourth_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let fourth_state = ProductSemanticState::from_projection(&output, fourth_projection.as_ref());
    let ProductSemanticState::Current(fourth_content) = fourth_state else {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_postcondition reason=semantic_unavailable"
        );
        return 1;
    };
    if !fourth_content
        .focused_application
        .as_ref()
        .is_some_and(|application| {
            application.reference.surface_id() == original.reference.surface_id()
        })
    {
        eprintln!(
            "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_FAIL stage=restore_postcondition reason=original_surface_not_focused"
        );
        return 1;
    }

    println!(
        "GNOS_UX_R13_APPLICATION_FOCUS_QUALIFICATION_PASS output={} first_generation={} selected_generation={} selected_surface={:?} selected_title={:?} selected_receipt={} selected_post_generation={} restore_generation={} original_surface={:?} original_title={:?} restore_receipt={} restore_post_generation={}",
        connector,
        first.generation,
        first_record.selected_generation.0,
        selected.reference.surface_id(),
        selected.title,
        first_record.terminal.expect("selected terminal").as_str(),
        third.generation,
        restore_record.selected_generation.0,
        original.reference.surface_id(),
        original.title,
        restore_record.terminal.expect("restore terminal").as_str(),
        fourth.generation,
    );
    0
}

#[cfg(feature = "r13-production-client")]
pub(super) fn run_r13_active_workspace_qualification(connector: &str) -> i32 {
    let backend = match HyprctlLuaBackend::from_env() {
        Ok(backend) => backend,
        Err(error) => {
            eprintln!("GNOS_UX_R13_QUALIFICATION_FAIL stage=backend reason={error:?}");
            return 1;
        }
    };
    let mut semantic_runtime = DesktopSemanticRuntime::default();

    let first = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!("GNOS_UX_R13_QUALIFICATION_FAIL stage=select_observation reason={error:?}");
            return 1;
        }
    };
    let first_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let output = RuntimeId::new(connector);
    let selected_state = ProductSemanticState::from_projection(&output, first_projection.as_ref());
    let Some(selected) = (match selected_state {
        ProductSemanticState::Current(content) => content.active_workspace.clone(),
        ProductSemanticState::Unavailable | ProductSemanticState::Invalid => None,
    }) else {
        eprintln!(
            "GNOS_UX_R13_QUALIFICATION_FAIL stage=select_target output={} generation={} reason=no_active_workspace",
            connector, first.generation
        );
        return 1;
    };

    let second = match backend.observe_and_reconcile_desktop(&mut semantic_runtime) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!(
                "GNOS_UX_R13_QUALIFICATION_FAIL stage=send_fence_observation reason={error:?}"
            );
            return 1;
        }
    };
    let second_projection = project_all_output_contexts(semantic_runtime.baseline())
        .into_iter()
        .find(|projection| projection.output.connector.as_str() == connector);
    let mut product_runtime =
        ProductRuntime::from_projection(Some(&output), second_projection.as_ref());

    let send_started = Instant::now();
    let record = ProductController.activate_workspace(&mut product_runtime, selected);
    let send_elapsed_us = send_started.elapsed().as_micros();
    match record.terminal {
        Some(ProductActionTerminal::Applied | ProductActionTerminal::AlreadySatisfied)
            if record.current_generation == Some(DesktopContextGeneration(second.generation)) =>
        {
            println!(
                "GNOS_UX_R13_QUALIFICATION_RECEIPT output={} selected_generation={} current_generation={} send_elapsed_us={} code={} detail={}",
                connector,
                record.selected_generation.0,
                record
                    .current_generation
                    .map_or(0, |generation| generation.0),
                send_elapsed_us,
                record.terminal.expect("matched terminal outcome").as_str(),
                record.detail
            );
            0
        }
        terminal => {
            eprintln!(
                "GNOS_UX_R13_QUALIFICATION_FAIL stage=send output={} selected_generation={} current_generation={} send_elapsed_us={} terminal={terminal:?} detail={}",
                connector,
                record.selected_generation.0,
                record
                    .current_generation
                    .map_or(0, |generation| generation.0),
                send_elapsed_us,
                record.detail
            );
            1
        }
    }
}
