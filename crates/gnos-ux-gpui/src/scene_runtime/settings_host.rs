//! UXE-05B: Settings in the scene: its motion, receipt, lines and actions.

use super::*;

impl SceneHost {
    /// UXE-05B: advances the Settings extrusion; true while it still moves.
    pub(super) fn tick_settings(&mut self, motion_now: Instant) -> bool {
        #[cfg(not(feature = "component-lab"))]
        let _ = motion_now;
        #[cfg(feature = "component-lab")]
        if self.review_native_rail {
            let moving = self.settings.needs_frame();
            let reduced = self.effective_reduced_motion();
            let still = self.settings.tick(motion_now, reduced);
            if moving && !still {
                self.report_settings("settled");
            }
            return still;
        }
        false
    }

    /// UXE-05B: one receipt line with the panel, navigation and page lines.
    #[cfg(feature = "component-lab")]
    pub(super) fn report_settings(&self, state: &str) {
        let rect = |r: blobin_ui_core::layout::Rect| {
            format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
        };
        let Some(frame) = self.settings.frame() else {
            println!(
                "GNOS_UX_UXE05_SETTINGS state={state} open={} panel=None page={}",
                self.settings.is_open(),
                self.settings.page().slug()
            );
            return;
        };
        let nav = frame
            .nav
            .iter()
            .filter_map(|n| {
                n.page.map(|p| {
                    format!(
                        "{{{} rect={} current={}}}",
                        p.slug(),
                        rect(n.rect),
                        n.current
                    )
                })
            })
            .collect::<Vec<_>>()
            .join(";");
        let lines = frame
            .lines
            .iter()
            .map(|l| {
                let buttons = l
                    .buttons
                    .iter()
                    .map(|(r, a)| format!(" {}@{}", a.slug(), rect(*r)))
                    .collect::<String>();
                format!("{{{} rect={}{buttons}}}", l.line.name(), rect(l.rect))
            })
            .collect::<Vec<_>>()
            .join(";");
        println!(
            "GNOS_UX_UXE05_SETTINGS state={state} open={} page={} hover={} pressed={} focus={} panel={} hull={} close={} page_area={} nav=[{nav}] lines=[{lines}]",
            self.settings.is_open(),
            frame.page.slug(),
            frame.hovered.map_or("none".to_owned(), |t| t.slug()),
            frame.pressed,
            frame
                .focused
                .map_or("nav".to_owned(), |(_, a)| a.slug().to_owned()),
            rect(frame.panel),
            frame.hull.is_some(),
            rect(frame.close),
            rect(frame.page_area),
        );
    }

    #[cfg(feature = "component-lab")]
    pub(super) fn refresh_settings_lines(&mut self, cx: &mut Context<Self>) -> bool {
        let page = self.settings.page();
        // SETTINGS-02: the Display page reads the displays (slow: off the UI
        // thread, at most every 30 s).
        if page == crate::settings_pages::PageId::Display {
            self.utilities_detect_brightness(cx);
            if self.utilities_state.displays.is_empty() {
                self.settings_read_displays(cx);
            }
        }
        if page == crate::settings_pages::PageId::Network {
            self.settings_read_wifi(cx);
        }
        if page == crate::settings_pages::PageId::Lock {
            self.settings_read_lock();
        }
        let live = self.settings_live_facts(cx);
        #[cfg(feature = "r12-production-client")]
        let (profile_status, profile_ready) = {
            use crate::product_profile::ProfileStatus;
            let session = crate::product_profile::ProfileSession::shared(cx);
            let session = session.read(cx);
            (
                Some(format!("{:?}", session.status)),
                session.snapshot.is_some() && !matches!(session.status, ProfileStatus::Loading),
            )
        };
        #[cfg(not(feature = "r12-production-client"))]
        let (profile_status, profile_ready): (Option<String>, bool) = {
            let _ = cx;
            (None, false)
        };
        let lines = crate::settings_pages::page_lines(
            page,
            &crate::settings_pages::SettingsFacts {
                preferences: self.product_runtime.preferences(),
                network: self.product_runtime.network(),
                bluetooth: self.product_runtime.bluetooth(),
                power: self.product_runtime.power(),
                audio: self.product_runtime.audio(),
                profile_writes: cfg!(feature = "r12-production-client"),
                profile_status: profile_status.as_deref(),
                profile_ready,
                live: &live,
            },
        );
        self.settings.set_lines(lines)
    }

    /// UXE-05B: a Settings control asked for an edit. It becomes one typed
    /// request to the profile authority (R12), derived from the projection
    /// the user is looking at; nothing else can write.
    #[cfg(feature = "component-lab")]
    pub(super) fn apply_settings_action(
        &mut self,
        action: crate::settings_pages::SettingsAction,
        cx: &mut Context<Self>,
    ) {
        if let crate::settings_pages::SettingsAction::Live(control) = action {
            self.settings_live(control, cx);
            return;
        }
        let preferences = self.product_runtime.preferences();
        let edit = crate::settings_pages::bar_edit(
            action,
            preferences.bar_edge,
            &preferences.bar_overrides,
        );
        #[cfg(feature = "r12-production-client")]
        {
            let session = crate::product_profile::ProfileSession::shared(cx);
            match &edit {
                Some(crate::settings_pages::BarEdit::Edge(edge)) => {
                    let edge = *edge;
                    session.update(cx, |session, cx| session.select(edge, cx));
                }
                Some(crate::settings_pages::BarEdit::Overrides(overrides)) => {
                    let overrides = overrides.clone();
                    session.update(cx, |session, cx| session.select_preferences(overrides, cx));
                }
                None => {}
            }
            println!(
                "GNOS_UX_UXE05_EDIT action={} request={:?} status={:?}",
                action.slug(),
                edit,
                session.read(cx).status
            );
        }
        #[cfg(not(feature = "r12-production-client"))]
        {
            let _ = cx;
            println!(
                "GNOS_UX_UXE05_EDIT action={} request={edit:?} refused=no_profile_client",
                action.slug()
            );
        }
    }

    /// UXE-05B: apply a Settings outcome; returns whether the event was its.
    #[cfg(feature = "component-lab")]
    pub(super) fn apply_settings_outcome(
        &mut self,
        outcome: crate::settings_panel::SettingsOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let crate::settings_panel::SettingsOutcome {
            consumed,
            changed,
            opened,
            closed,
            action,
        } = outcome;
        if let Some(action) = action {
            self.apply_settings_action(action, cx);
        }
        if opened || changed {
            self.refresh_settings_lines(cx);
        }
        if opened {
            window.focus(&self.settings_focus, cx);
            // Opening Settings retries a profile projection that was not
            // available yet; it never retries a write.
            #[cfg(feature = "r12-production-client")]
            {
                let session = crate::product_profile::ProfileSession::shared(cx);
                if session.read(cx).snapshot.is_none() {
                    session.update(cx, |session, cx| session.refresh(cx));
                }
            }
            self.report_settings("opened");
        } else if closed {
            self.report_settings("closed");
        } else if changed {
            self.report_settings("changed");
        }
        if changed || opened || closed {
            self.start_motion_driver(cx);
            cx.notify();
        } else if consumed {
            // SET-04: a press or release repaints its state layer.
            cx.notify();
        }
        consumed
    }
}
