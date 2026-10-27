//! UXE-01 / SURF-03: audio and media in the scene: the audio interaction,
//! the OSD and Now Playing surfaces, and the MPRIS interaction owner.

use super::*;

impl SceneHost {
    /// Builds the Bar's audio interaction sink for this output.
    ///
    /// Audio has no cross-process broadcast path: a GPUI click belongs to the
    /// SceneHost that rendered the clicked output. Every host therefore owns a
    /// local sink and revalidates the exact endpoint against the process-global
    /// observed snapshot before issuing one native PipeWire mutation.
    pub(super) fn audio_interaction_readonly(
        &self,
        cx: &mut gpui::App,
    ) -> Option<crate::product_bar::BarAudioInteraction> {
        if !self.product_mode {
            return None;
        }
        let session = crate::product_audio_session::AudioSession::shared(cx);
        // The output this host painted, carried as non-authoritative
        // presentation context for the OSD.
        let output_id_for = self
            .runtime_output
            .as_ref()
            .map(|output| output.as_str().to_owned());
        Some(crate::product_bar::BarAudioInteraction {
            dispatch: std::rc::Rc::new(move |app: &mut gpui::App, intent| {
                let snapshot = session.read_with(app, |this, _cx| this.snapshot().clone());
                // AUDIO-01 Phase 5: record which output the user acted on, as
                // presentation context only. This is consumed by the OSD once
                // PipeWire confirms the change, purely to choose where the
                // surface appears. It never selects an endpoint, never forms
                // part of an intent, and the dispatch below does not read it.
                let osd = crate::product_audio_osd::AudioOsd::shared(app);
                let output_id = output_id_for.clone();
                osd.update(app, |osd, _cx| {
                    osd.note_interaction_context(output_id.as_deref());
                });
                let spawn = std::thread::Builder::new()
                    .name("gnos-audio-action".into())
                    .spawn(move || {
                        let outcome =
                            crate::product_audio_action::dispatch_intent(&snapshot, &intent);
                        println!("GNOS_UX_AUDIO_INTENT outcome={outcome:?}");
                    });
                if let Err(error) = spawn {
                    println!("GNOS_UX_AUDIO_INTENT outcome=ThreadSpawnFailure error={error}");
                }
            }),
        })
    }

    /// Builds the observed audio OSD for this output.
    ///
    /// Placement is decided when the event is accepted, so every output asks the
    /// same question and exactly one gets a surface: does the resolved owner
    /// equal the output I paint? An unresolvable owner therefore presents
    /// nowhere instead of duplicating a global audio event across monitors.
    pub(super) fn audio_osd_surface(
        &self,
        layout: &crate::product_scene::ProductLayout,
        design: crate::product_design::ProductDesignContext,
        my_output: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        // SURF-01C: on the native rail the OSD comes out of the Bar
        // instead (`bar_osd`); AUDIO-01's observation and receipt remain.
        if self.review_native_rail && cfg!(feature = "component-lab") {
            return None;
        }
        let osd = crate::product_audio_osd::AudioOsd::shared(cx);
        let owns = osd
            .read_with(cx, |this, _cx| this.state().owner().map(str::to_owned))
            .is_some_and(|owner| Some(owner.as_str()) == my_output);
        if !owns {
            return None;
        }
        crate::product_audio_osd::render_audio_osd(
            osd.read_with(cx, |this, _cx| this.state().clone()),
            design,
        )
        .map(|surface| {
            surface
                .absolute()
                .right(px(ProductSpacing::XS))
                .top(px(layout.bar.bottom() + ProductSpacing::XS))
        })
    }

    /// Builds the Now Playing surface for this output.
    ///
    /// The surface is drawn on every output because it reads ProductRuntime,
    /// which is the shared source of truth. Only the output that owns the
    /// interaction receives a dispatch sink, so a click on one output produces
    /// exactly one remote call rather than one per output. This is the direct
    /// answer to the duplicate-host failure class DONOR-15 found.
    pub(super) fn now_playing_surface(
        &self,
        cx: &mut gpui::Context<Self>,
        layout: &crate::product_scene::ProductLayout,
        design: crate::product_design::ProductDesignContext,
        my_output: Option<&str>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        // SURF-03B: on the native rail the media lives in the dashboard (out
        // of the clock); the floating Now Playing card is the product Bar's.
        if self.review_native_rail && cfg!(feature = "component-lab") {
            return None;
        }
        let presentation = self.product_runtime.now_playing();
        let effective = presentation.effective()?.identity.clone();
        let derived = crate::product_now_playing::NowPlayingPresentation::derive(&presentation);

        // Every enabled surface installs a dispatch sink (see the tray path):
        // the lease elects an executor, not an interactive output. A click on a
        // non-owning output still reaches execution, because the dispatch
        // closure routes the exact typed intent to the lease owner.
        let process_owns = self.mpris_process_owns.clone();
        let session = crate::product_mpris_session::MprisSession::shared(cx);
        // Mirror the owner's live player state into the relay hub on every
        // render, so a forwarded intent revalidates against real observed state.
        if let Some(connection) = session.read_with(cx, |this, _cx| this.connection().cloned()) {
            let snapshot = session.read_with(cx, |this, _cx| this.snapshot().clone());
            crate::product_mpris_relay::publish(connection, snapshot);
        }
        let source_output = my_output.unwrap_or("unknown").to_owned();
        let sink: crate::product_now_playing::NowPlayingDispatch = {
            let session = session.clone();
            let process_owns = process_owns.clone();
            let source_output = source_output.clone();
            std::rc::Rc::new(
                move |app: &mut gpui::App, intent: crate::product_mpris_transport::MprisIntent| {
                    spawn_mpris_intent(
                        session.clone(),
                        process_owns.clone(),
                        source_output.clone(),
                        app,
                        intent,
                    );
                },
            )
        };
        let dispatch = Some(sink);

        Some(
            crate::product_now_playing::render_now_playing(
                &derived,
                design,
                dispatch.as_ref(),
                Some(&effective),
            )
            .absolute()
            .left(px(ProductSpacing::XS))
            .top(px(layout.bar.bottom() + ProductSpacing::XS)),
        )
    }
}

/// Claims the transport interaction for this process, arbitrated on the bus.
///
/// Each output is served by its own process, so a process-global cannot decide
/// which output owns the interaction. A first implementation used one and the
/// multi-output gate proved it wrong: three scene processes each claimed
/// ownership and nine remote calls were made where three were intended.
///
/// The session bus is therefore the arbiter, by requesting one well-known name.
/// Exactly one process receives it, and only that process may install a dispatch
/// sink on any of its hosts.
/// Process-wide Now Playing lease result, shared by every scene host in the
/// process. The claim is requested once per process; the result belongs to the
/// process, not to whichever host happened to start first.
pub(super) static MPRIS_PROCESS_OWNS: std::sync::OnceLock<
    std::sync::Arc<std::sync::atomic::AtomicBool>,
> = std::sync::OnceLock::new();

pub(super) fn claim_mpris_interaction_owner(cx: &mut gpui::App) {
    // The process-level claim needs a bus round trip, so it resolves later.
    // Until it does, the surface renders but installs no dispatch sink, which is
    // the safe direction: a missing control costs one click, a duplicated one
    // costs a wrong action on a real player.
    //
    // Only the first host in this process requests the name. A second request
    // from the same process would be answered as a rival claim and the process
    // would report itself as both owner and not-owner.
    static REQUESTED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
    if !crate::interaction_lease::claim_once(&REQUESTED) {
        return;
    }
    // The result is process-wide: every scene host reads the same bit. A
    // per-host copy would leave all but the first host reading a permanent
    // "not owner", and a second host in the owning process would relay to its
    // own process instead of executing locally.
    let process_owns = MPRIS_PROCESS_OWNS.get_or_init(Default::default).clone();
    let name = crate::product_mpris_ownership::INTERACTION_OWNER_NAME.to_owned();
    cx.spawn(async move |_this| {
        match crate::interaction_lease::InteractionLease::acquire(&name).await {
            Ok(lease) => {
                let owns = lease.owns();
                let relay_connection = lease.connection().clone();
                // Retain the lease for the process lifetime. A bus name is
                // released when its connection drops, so letting the lease fall
                // out of scope here would make ownership transient: this process
                // would win, drop the name, and the next process would win too.
                // Each process then reported itself as owner, and the
                // multi-output gate caught it.
                {
                    let mut held = INTERACTION_OWNER_CONNECTION
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    *held = Some(lease);
                }
                process_owns.store(owns, std::sync::atomic::Ordering::SeqCst);
                println!(
                    "GNOS_UX_MPRIS_INTERACTION outcome={} name={name} connection_retained=true",
                    if owns {
                        "process_owns"
                    } else {
                        "process_not_owner"
                    },
                );
                // The owner serves the relay on the same connection that holds
                // the name, so a non-owner reaches it by the well-known name.
                if owns
                    && let Err(error) = crate::product_mpris_relay::serve(&relay_connection).await
                {
                    println!("GNOS_UX_MPRIS_RELAY_SERVE_FAIL error={error}");
                }
            }
            Err(error) => {
                println!("GNOS_UX_MPRIS_INTERACTION outcome=bus_unavailable error={error}");
            }
        }
    })
    .detach();
}

/// Holds the Now Playing interaction lease for this process's lifetime. See the
/// retention note at its only use.
pub(super) static INTERACTION_OWNER_CONNECTION: std::sync::Mutex<
    Option<crate::interaction_lease::InteractionLease>,
> = std::sync::Mutex::new(None);

/// Claims the right to run a qualification gate for this process.
///
/// Separate from the interaction owner: the gate runner drives scripted dispatches
/// that a would-be interactive host must not duplicate. Where the interaction
/// owner is a long-lived claim, this one is named per run intent and released
/// when the gate completes, so a later run is not locked out by an earlier one.
///
/// `block_on` in the caller is safe because the future performs one bounded bus
/// call with no GPUI work inside it.
pub(super) async fn claim_gate_run(connection: zbus::Connection) -> bool {
    // A distinct name per qualification kind is unnecessary: at most one
    // MPRIS gate runs at a time, and a stale-player gate must never overlap a
    // transport gate on the same exporter anyway.
    let name = crate::product_mpris_ownership::QUALIFICATION_GATE_NAME.to_owned();
    let granted = connection
        .request_name_with_flags(name.clone(), zbus::fdo::RequestNameFlags::DoNotQueue.into())
        .await;
    let owns = matches!(granted, Ok(zbus::fdo::RequestNameReply::PrimaryOwner));
    println!(
        "GNOS_UX_MPRIS_QUALIFICATION gate_claim={} name={name}",
        if owns { "won" } else { "lost" },
    );
    owns
}

/// Dispatches one typed MPRIS intent through the revalidating transport path.
/// Routes one Now Playing intent by lease ownership: the owner executes, a
/// non-owner relays the exact typed intent to the owner. Kept as a single
/// function so the render sink and the qualification sink cannot drift apart.
pub(super) async fn route_mpris_intent(
    session: &gpui::Entity<crate::product_mpris_session::MprisSession>,
    process_owns: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    source_output: &str,
    intent: crate::product_mpris_transport::MprisIntent,
    cx: &mut gpui::AsyncApp,
) -> crate::product_mpris_transport::MprisTransportOutcome {
    // Keep the owner's relay hub current so a forwarded intent revalidates
    // against real observed state. Harmless in a non-owner.
    let (connection, snapshot) = session.read_with(&*cx, |this, _cx| {
        (this.connection().cloned(), this.snapshot().clone())
    });
    if let Some(connection) = connection.clone() {
        crate::product_mpris_relay::publish(connection, snapshot);
    }
    if process_owns.load(std::sync::atomic::Ordering::SeqCst) {
        println!("GNOS_UX_MPRIS_SOURCE output={source_output} route=local local_lease=true");
        crate::product_mpris_session::dispatch_intent(session, intent, cx).await
    } else {
        println!(
            "GNOS_UX_MPRIS_SOURCE output={source_output} route=relay local_lease=false target={}",
            crate::product_mpris_ownership::INTERACTION_OWNER_NAME
        );
        match connection {
            Some(connection) => {
                crate::product_mpris_relay::relay(
                    &connection,
                    crate::product_mpris_ownership::INTERACTION_OWNER_NAME,
                    source_output,
                    &intent,
                )
                .await
            }
            None => crate::product_mpris_transport::MprisTransportOutcome::Unavailable {
                operation: intent.operation,
                error: "no bus connection".to_owned(),
            },
        }
    }
}

pub(super) fn spawn_mpris_intent(
    session: gpui::Entity<crate::product_mpris_session::MprisSession>,
    process_owns: std::sync::Arc<std::sync::atomic::AtomicBool>,
    source_output: String,
    app: &mut gpui::App,
    intent: crate::product_mpris_transport::MprisIntent,
) {
    app.spawn(async move |cx| {
        let outcome =
            route_mpris_intent(&session, &process_owns, &source_output, intent, &mut *cx).await;
        println!(
            "GNOS_UX_MPRIS_DISPATCH delivered={} operation={}",
            outcome.is_delivered(),
            outcome.operation().name()
        );
    })
    .detach();
}
