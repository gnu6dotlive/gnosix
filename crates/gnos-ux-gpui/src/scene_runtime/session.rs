//! SURF-02B: the session panel in the scene.
//!
//! The panel (`session_panel`) is pure. Here it gets what logind reports
//! (capabilities, inhibitors, the session's lock state), and its effects
//! become `gnos_utilities::request_session`, each with a receipt
//! (`GNOS_UX_SESSION_REQUEST`).
//! - logind is read once at start: the power button appears only once it
//!   answers. It is read again each time the panel opens.
//! - Lock is confirmed by observation: `LockedHint` within `LOCK_WAIT`, or
//!   the tile says *No locker*.
//!
//! Development builds send a session request (never a read) only to a
//! system bus named explicitly (`DBUS_SYSTEM_BUS_ADDRESS`, a private bus in
//! the qualification), or with `GNOS_UX_SESSION_LIVE=1`: a harness that
//! happens to press the power button can never power the host off.

use super::*;
use crate::session_panel::{SessionEffect, SessionModel, SessionTile};
use gnos_utilities::{
    ObservedInhibitor, SessionAction, SessionCapabilities, SessionOutcome as LogindOutcome,
};
use std::collections::{BTreeMap, BTreeSet};

const LOCK_WAIT: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct SessionState {
    capabilities: SessionCapabilities,
    inhibitors: Vec<ObservedInhibitor>,
    pending: BTreeSet<SessionAction>,
    notes: BTreeMap<SessionAction, String>,
    reading: bool,
    started: bool,
    /// The held press a timer is armed for.
    hold_timer: Option<Instant>,
    reported: Option<String>,
}

fn block_on<F: std::future::Future>(future: F) -> Option<F::Output> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .map(|rt| rt.block_on(future))
}

/// Whether this build may send session requests to the system bus it has:
/// always in release builds; in development builds only to a bus named
/// explicitly that is not the host's default socket, or when asked.
fn requests_allowed() -> bool {
    let private_bus = std::env::var("DBUS_SYSTEM_BUS_ADDRESS")
        .is_ok_and(|a| !a.is_empty() && !a.contains("/run/dbus/system_bus_socket"));
    !cfg!(debug_assertions)
        || private_bus
        || std::env::var("GNOS_UX_SESSION_LIVE").as_deref() == Ok("1")
}

async fn read_logind() -> Option<(SessionCapabilities, Vec<ObservedInhibitor>)> {
    let connection = zbus::Connection::system().await.ok()?;
    let capabilities = gnos_utilities::read_capabilities(&connection).await;
    let inhibitors = gnos_utilities::list_inhibitors(&connection)
        .await
        .unwrap_or_default();
    Some((capabilities, inhibitors))
}

impl SceneHost {
    /// Whether a panel out of the Bar (Utilities, Session) is open: it takes
    /// the whole output, so the Deck beneath answers no pointer (on a narrow
    /// output the panel can stand over it).
    pub(super) fn bar_panel_open(&self) -> bool {
        self.utilities.is_open() || self.session.is_open() || self.bar_popout.dashboard_open()
    }

    /// Read logind (off the UI thread); the first answer makes the power
    /// button appear.
    pub(super) fn read_session(&mut self, cx: &mut Context<Self>) {
        if self.session_state.reading || !self.review_native_rail {
            return;
        }
        self.session_state.reading = true;
        self.session_state.started = true;
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { block_on(read_logind()).flatten() })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.session_state.reading = false;
                let Some((capabilities, inhibitors)) = read else {
                    println!("GNOS_UX_SESSION_LOGIND state=unavailable");
                    return;
                };
                println!(
                    "GNOS_UX_SESSION_LOGIND state=read suspend={:?} reboot={:?} poweroff={:?} inhibitors={}",
                    capabilities.suspend,
                    capabilities.reboot,
                    capabilities.power_off,
                    inhibitors.len()
                );
                let observed = capabilities.observed();
                view.session_state.capabilities = capabilities;
                view.session_state.inhibitors = inhibitors;
                if view.product_runtime.set_session_powers(observed) {
                    view.print_status_receipt("session", cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Start reading logind once.
    pub(super) fn start_session(&mut self, cx: &mut Context<Self>) {
        if !self.session_state.started {
            self.read_session(cx);
        }
    }

    fn session_model(&self) -> SessionModel {
        let state = &self.session_state;
        let tiles = SessionAction::ALL
            .iter()
            .map(|&action| SessionTile {
                action,
                available: match action {
                    SessionAction::Lock | SessionAction::Logout => Some(true),
                    _ => state.capabilities.of(action).map(|c| c.allows()),
                },
                blocked: gnos_utilities::blockers(&state.inhibitors, action).len(),
                pending: state.pending.contains(&action),
                note: state.notes.get(&action).cloned(),
            })
            .collect();
        let mut seen = BTreeSet::new();
        let blockers = state
            .inhibitors
            .iter()
            .filter(|i| {
                i.mode == "block" && i.what.split(':').any(|w| w == "shutdown" || w == "sleep")
            })
            .filter(|i| seen.insert((i.who.clone(), i.what.clone(), i.why.clone())))
            .map(|i| format!("{} · {}: {}", i.who, i.what.replace(':', ", "), i.why))
            .collect();
        SessionModel { tiles, blockers }
    }

    /// The centre of the power button along the Bar at rest on `side`.
    fn power_anchor(&self, side: crate::rail_topology::RailSide) -> Option<f32> {
        self.module_anchor(side, crate::reference_bar::IdleBarEntry::Power)
    }

    /// Place the panel and give it what logind reports.
    pub(super) fn sync_session(
        &mut self,
        side: crate::rail_topology::RailSide,
        viewport: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let anchor = self.power_anchor(side);
        let mut changed = self.session.place(side, viewport, anchor);
        changed |= self.session.set_model(self.session_model());
        if changed {
            self.report_session("changed");
            if self.session.needs_frame() {
                self.start_motion_driver(cx);
            }
            cx.notify();
        }
    }

    /// `GNOS_UX_SESSION`: the panel, its tiles and where they stand.
    pub(super) fn report_session(&mut self, state: &str) {
        let tiles = self
            .session
            .model()
            .tiles
            .iter()
            .map(|t| {
                format!(
                    "{{{} available={} blocked={} pending={} note={}}}",
                    t.action.slug(),
                    t.available.map_or("none".to_owned(), |b| b.to_string()),
                    t.blocked,
                    t.pending,
                    t.note.as_deref().unwrap_or("-").replace(' ', "_")
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let frame = self.session.frame();
        let rect = frame.as_ref().map_or("None".to_owned(), |f| {
            format!(
                "[{:.1},{:.1},{:.1},{:.1}] openness={:.3} hull={}",
                f.rect.left(),
                f.rect.top(),
                f.rect.width(),
                f.rect.height(),
                f.openness,
                f.hull.is_some()
            )
        });
        let controls = frame
            .map(|f| {
                self.session
                    .rows()
                    .into_iter()
                    .filter_map(|r| match &r {
                        crate::session_panel::SessionRow::Tile { tile, .. } => {
                            let (x, y, w, h) = r.rect();
                            Some(format!(
                                "{}@[{:.1},{:.1},{:.1},{:.1}]",
                                tile.action.slug(),
                                f.rect.left() + x,
                                f.rect.top() + y,
                                w,
                                h
                            ))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .unwrap_or_default();
        let line = format!(
            "GNOS_UX_SESSION state={state} open={} rect={rect} blockers={} hint={} tiles=[{tiles}] controls=[{controls}]",
            self.session.is_open(),
            self.session.model().blockers.len(),
            self.session.hint().map_or("-", |a| a.slug()),
        );
        if state != "changed" || self.session_state.reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.session_state.reported = Some(line);
        }
    }

    /// A click on the power button: open or close the panel.
    /// A click on the power button: through the shell's attention (FOCUS-01),
    /// which opens it once whatever is out has gone in.
    pub(super) fn toggle_session(&mut self, cx: &mut Context<Self>) {
        self.attention_toggle(crate::attention::Attendant::Session, cx);
    }

    /// The panel out or in, as the shell's attention decided.
    pub(super) fn session_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.session.is_open() == open {
            return;
        }
        self.session.toggle_open();
        if open {
            // The OSD gives way (the other surfaces already have: one at a
            // time).
            if self.bar_osd.close() {
                self.report_osd("closing");
            }
            self.session_state.notes.clear();
            self.read_session(cx);
        }
        self.report_session(if open { "opened" } else { "closing" });
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// Route the panel's outcome; returns whether the event was its.
    pub(super) fn apply_session_outcome(
        &mut self,
        outcome: crate::session_panel::SessionOutcome,
        cx: &mut Context<Self>,
    ) -> bool {
        // A held press fires when its time is up, whether or not a frame
        // happens to be drawn then.
        if let Some(started) = self.session.held_press()
            && self.session_state.hold_timer != Some(started)
        {
            self.session_state.hold_timer = Some(started);
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(crate::session_panel::HOLD + Duration::from_millis(5))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.poll_session(Instant::now(), cx);
                    cx.notify();
                });
            })
            .detach();
        }
        for effect in outcome.effects {
            self.session_effect(effect, cx);
        }
        if outcome.changed {
            self.report_session(if self.session.is_open() {
                "changed"
            } else {
                "closing"
            });
            self.start_motion_driver(cx);
            cx.notify();
        }
        outcome.consumed
    }

    /// A held press that completed this frame.
    pub(super) fn poll_session(&mut self, now: Instant, cx: &mut Context<Self>) {
        if let Some(effect) = self.session.poll(now) {
            self.session_effect(effect, cx);
        }
    }

    fn session_effect(&mut self, effect: SessionEffect, cx: &mut Context<Self>) {
        let SessionEffect::Request { action, force } = effect;
        if !requests_allowed() {
            println!(
                "GNOS_UX_SESSION_REQUEST action={} force={force} outcome=Withheld note=development_build_on_the_default_system_bus",
                action.slug()
            );
            self.session_state
                .notes
                .insert(action, "Withheld".to_owned());
            cx.notify();
            return;
        }
        println!(
            "GNOS_UX_SESSION_REQUEST action={} force={force} outcome=Sent",
            action.slug()
        );
        self.session_state.notes.remove(&action);
        self.session_state.pending.insert(action);
        let capabilities = self.session_state.capabilities.clone();
        let inhibitors = self.session_state.inhibitors.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    block_on(async {
                        let connection = match zbus::Connection::system().await {
                            Ok(c) => c,
                            Err(e) => {
                                return (LogindOutcome::TransportFailure(e.to_string()), None);
                            }
                        };
                        let outcome = gnos_utilities::request_session(
                            &connection,
                            &capabilities,
                            &inhibitors,
                            action,
                            force,
                        )
                        .await;
                        // Lock: confirmed only by logind's LockedHint.
                        let mut locked = None;
                        if action == SessionAction::Lock && outcome == LogindOutcome::Accepted {
                            let until = Instant::now() + LOCK_WAIT;
                            locked = Some(false);
                            while Instant::now() < until {
                                if gnos_utilities::session_locked(&connection).await == Ok(true) {
                                    locked = Some(true);
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        }
                        (outcome, locked)
                    })
                    .unwrap_or((LogindOutcome::TransportFailure("No runtime".into()), None))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let (outcome, locked) = result;
                view.session_state.pending.remove(&action);
                let note = match (&outcome, locked) {
                    (LogindOutcome::Accepted, Some(true)) => Some("Locked".to_owned()),
                    (LogindOutcome::Accepted, Some(false)) => Some("No locker".to_owned()),
                    (LogindOutcome::Accepted, None) => None,
                    (LogindOutcome::Refused(gnos_utilities::SessionRefusal::Blocked(_)), _) => {
                        Some("Blocked".to_owned())
                    }
                    (LogindOutcome::Refused(_), _) => Some("Unavailable".to_owned()),
                    (LogindOutcome::Denied(_), _) => Some("Denied".to_owned()),
                    (LogindOutcome::TransportFailure(_), _) => Some("Failed".to_owned()),
                };
                println!(
                    "GNOS_UX_SESSION_REQUEST action={} force={force} outcome={outcome:?} locked={}",
                    action.slug(),
                    locked.map_or("-".to_owned(), |l| l.to_string())
                );
                if let Some(note) = note {
                    view.session_state.notes.insert(action, note);
                }
                cx.notify();
            });
        })
        .detach();
    }
}
