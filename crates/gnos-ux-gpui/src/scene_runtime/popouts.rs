//! UXE-01 / UXE-04: the Bar popouts in the scene: their motion, receipt
//! and outcomes.

use super::*;

impl SceneHost {
    /// UXE-04: one receipt line whenever the settled read-only popout changes:
    /// kind, rectangle and every row (kind, rectangle, label).
    #[cfg(feature = "component-lab")]
    pub(super) fn report_status_popout(&mut self) {
        // FOCUS-01B: one receipt once a handoff has settled, not one a frame.
        if !self.bar_popout.handoff_settled() {
            return;
        }
        let frame = self.bar_popout.frame();
        let info = self.bar_popout.info();
        let line = match (frame, info) {
            (Some(frame), Some(info)) if frame.openness >= 1.0 => {
                let rows = info
                    .rows
                    .iter()
                    .map(|r| {
                        let state = match r.kind {
                            crate::bar_status_popouts::InfoRowKind::State { on } => {
                                format!(" on={on:?}")
                            }
                            crate::bar_status_popouts::InfoRowKind::Wifi {
                                glyph,
                                secure,
                                active,
                            } => {
                                format!(" glyph={} secure={secure} active={active}", glyph.name())
                            }
                            crate::bar_status_popouts::InfoRowKind::Wired { connected } => {
                                format!(" connected={connected}")
                            }
                            crate::bar_status_popouts::InfoRowKind::BluetoothDevice {
                                glyph,
                                connected,
                            } => format!(" glyph={} connected={connected}", glyph.name()),
                            crate::bar_status_popouts::InfoRowKind::Profiles { current } => {
                                format!(" current={current:?}")
                            }
                            _ => String::new(),
                        };
                        format!(
                            "{{{} rect=[{:.1},{:.1},{:.1},{:.1}]{state} label={:?}}}",
                            r.kind.name(),
                            frame.rect.left() + r.x,
                            frame.rect.top() + r.y,
                            r.width,
                            r.height,
                            r.label
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                format!(
                    "GNOS_UX_UXE04_POPOUT state=open kind={} rect=[{:.1},{:.1},{:.1},{:.1}] rows=[{rows}]",
                    info.kind.as_str(),
                    frame.rect.left(),
                    frame.rect.top(),
                    frame.rect.width(),
                    frame.rect.height()
                )
            }
            (None, _) => "GNOS_UX_UXE04_POPOUT state=closed".to_owned(),
            _ => return,
        };
        if self.status_popout_reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.status_popout_reported = Some(line);
        }
    }

    /// UXE-01: advances the Bar popout slide; true while it still moves.
    pub(super) fn tick_bar_popout(&mut self, motion_now: Instant) -> bool {
        #[cfg(not(feature = "component-lab"))]
        let _ = motion_now;
        #[cfg(feature = "component-lab")]
        if self.review_native_rail {
            let reduced = self.effective_reduced_motion();
            let moving = self.bar_popout.needs_frame();
            // SURF-03B: the dashboard settles open when its extrusion stops,
            // and closed when the host lets it go.
            let dashboard_was = self.bar_popout.dashboard_shown();
            let sliding = self.dashboard.needs_frame();
            let still_moving = self.bar_popout.tick(motion_now, reduced)
                | (dashboard_was && self.dashboard.tick(motion_now, reduced));
            if sliding && dashboard_was && !self.dashboard.needs_frame() {
                self.report_dashboard("slid");
            }
            let dashboard_now = self.bar_popout.dashboard_shown();
            if dashboard_was && !dashboard_now {
                self.report_dashboard("settled_closed");
            } else if moving
                && !self.bar_popout.needs_frame()
                && dashboard_now
                && self.bar_popout.dashboard_open()
            {
                self.report_dashboard("settled_open");
            }
            if moving && !still_moving {
                match self.bar_popout.frame() {
                    Some(frame) => println!(
                        "GNOS_UX_UXE01_POPOUT state=settled_open rect=[{:.1},{:.1},{:.1},{:.1}] hull={} slider={:?}",
                        frame.rect.left(),
                        frame.rect.top(),
                        frame.rect.right(),
                        frame.rect.bottom(),
                        frame.hull.is_some(),
                        self.bar_popout.slider_rect(),
                    ),
                    None => println!("GNOS_UX_UXE01_POPOUT state=settled_closed"),
                }
                self.report_status_popout();
            }
            // SURF-01B: the Utilities panel's extrusion.
            let was = self.utilities.needs_frame();
            let utilities_moving = self.utilities.tick(motion_now, reduced);
            if was && !utilities_moving {
                self.report_utilities(if self.utilities.is_open() {
                    "settled_open"
                } else {
                    "settled_closed"
                });
            }
            // SURF-02B: the session panel's extrusion and a hold under way.
            let session_was = self.session.needs_frame();
            let session_moving = self.session.tick(motion_now, reduced);
            if session_was && !session_moving {
                self.report_session(if self.session.is_open() {
                    "settled_open"
                } else {
                    "settled_closed"
                });
            }
            // BAR-02A: the overflow panel's extrusion.
            let overflow_was = self.overflow.needs_frame();
            let overflow_moving = self.overflow.tick(motion_now, reduced);
            if overflow_was && !overflow_moving {
                self.report_overflow(if self.overflow.is_open() {
                    "settled_open"
                } else {
                    "settled_closed"
                });
            }
            // FOCUS-01: the attention's timers (dwell, grace), then the
            // workspace manager's extrusion.
            #[cfg(feature = "component-lab")]
            let attention_pending = self.attention_tick(motion_now);
            #[cfg(not(feature = "component-lab"))]
            let attention_pending = false;
            let workspaces_was = self.workspaces.needs_frame();
            let was_open = self.workspaces.is_open();
            let workspaces_moving = self.workspaces.tick(motion_now, reduced);
            if was_open && !self.workspaces.is_open() {
                self.report_workspaces("closing");
            }
            if workspaces_was && !workspaces_moving {
                self.report_workspaces(if self.workspaces.is_open() {
                    "settled_open"
                } else {
                    "settled_closed"
                });
            }
            // SURF-01C: the OSD's extrusion and its dwell.
            let osd_was = self.bar_osd.moving();
            let was_open = self.bar_osd.is_open();
            let osd_pending = self.bar_osd.tick(motion_now, reduced);
            if was_open && !self.bar_osd.is_open() {
                self.report_osd("closing");
            }
            if osd_was && !self.bar_osd.moving() {
                self.report_osd(if self.bar_osd.is_open() {
                    "settled_open"
                } else {
                    "settled_closed"
                });
            }
            // SHELL-FIELD-01C: a tray menu's extrusion out of the Bar.
            let tray_menu_moving = self.tray_menu.tick(motion_now, reduced);
            return still_moving
                || utilities_moving
                || osd_pending
                || session_moving
                || workspaces_moving
                || attention_pending
                || overflow_moving
                || tray_menu_moving;
        }
        false
    }

    /// UXE-01: routes popout effects into the existing AUDIO-01 sink.
    /// Returns whether the event belonged to the popout.
    #[cfg(feature = "component-lab")]
    pub(super) fn apply_bar_popout_outcome(
        &mut self,
        outcome: crate::bar_popout_host::HostOutcome,
        cx: &mut Context<Self>,
    ) -> bool {
        let crate::bar_popout_host::HostOutcome {
            changed,
            consumed,
            intents,
            pending,
            open_settings: _,
            dashboard_press,
            bar_intent: _,
        } = outcome;
        if let Some((lx, ly, down)) = dashboard_press {
            self.dashboard_pointer(lx, ly, down, cx);
        }
        if !intents.is_empty()
            && let Some(audio) = self.audio_interaction_readonly(cx)
        {
            for intent in intents {
                println!("GNOS_UX_UXE01_AUDIO_INTENT intent={intent:?}");
                (audio.dispatch)(cx, intent);
            }
        }
        for capability in pending {
            println!("GNOS_UX_UXE01_PENDING capability={capability:?}");
        }
        if changed && self.bar_popout.dashboard_shown() {
            self.report_dashboard(if self.bar_popout.dashboard_open() {
                "opening"
            } else {
                "closing"
            });
        }
        if changed {
            match self.bar_popout.frame() {
                _ if !self.bar_popout.is_open() => {
                    println!("GNOS_UX_UXE01_POPOUT state=closing");
                }
                Some(frame) => println!(
                    "GNOS_UX_UXE01_POPOUT state=open rect=[{:.1},{:.1},{:.1},{:.1}] openness={:.3} hull={} slider={:?}",
                    frame.rect.left(),
                    frame.rect.top(),
                    frame.rect.right(),
                    frame.rect.bottom(),
                    frame.openness,
                    frame.hull.is_some(),
                    self.bar_popout.slider_rect(),
                ),
                None => println!("GNOS_UX_UXE01_POPOUT state=opening"),
            }
            self.start_motion_driver(cx);
            cx.notify();
        }
        consumed
    }
}
