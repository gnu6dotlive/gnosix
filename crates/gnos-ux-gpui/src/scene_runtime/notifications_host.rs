//! UXE-02: toasts and the notification centre in the scene: their motion,
//! receipts and outcomes.

use super::*;

impl SceneHost {
    /// UXE-02: advances toast slides; true while any card still moves.
    pub(super) fn tick_notification_toasts(&mut self, motion_now: Instant) -> bool {
        #[cfg(not(feature = "component-lab"))]
        let _ = motion_now;
        #[cfg(feature = "component-lab")]
        if self.review_native_rail {
            let reduced = self.effective_reduced_motion();
            let now = motion_now;
            let moving = self.notification_toasts.needs_frame();
            let still_moving = self.notification_toasts.tick(now, reduced);
            if moving && !still_moving {
                self.report_toasts("settled");
            }
            let centre_moving = self.notification_centre.needs_frame();
            let centre_still = self.notification_centre.tick(now, reduced);
            if centre_moving && !centre_still {
                self.report_centre("settled");
            }
            return still_moving || centre_still;
        }
        false
    }

    /// UXE-02: one receipt line with every card and button rectangle.
    #[cfg(feature = "component-lab")]
    pub(super) fn report_toasts(&self, state: &str) {
        let Some(frame) = self.notification_toasts.frame() else {
            println!("GNOS_UX_UXE02_TOASTS state={state} panel=None cards=[]");
            return;
        };
        let rect = |r: blobin_ui_core::layout::Rect| {
            format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
        };
        let cards = frame
            .cards
            .iter()
            .map(|c| {
                let buttons = c
                    .buttons
                    .iter()
                    .map(|b| {
                        let kind = match &b.kind {
                            crate::notification_toasts::ButtonKind::Expand { .. } => {
                                "expand".to_owned()
                            }
                            crate::notification_toasts::ButtonKind::Close => "close".to_owned(),
                            crate::notification_toasts::ButtonKind::Action { label } => {
                                format!("action:{}", label.replace(' ', "_"))
                            }
                        };
                        format!("{kind}={}", rect(b.rect))
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{{id={} summary={:?} rect={} expanded={} leaving={} buttons={{{buttons}}}}}",
                    c.card.id.get(),
                    c.card.summary,
                    rect(c.rect),
                    c.expanded,
                    c.leaving
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        println!(
            "GNOS_UX_UXE02_TOASTS state={state} panel={} cards=[{cards}]",
            rect(frame.panel)
        );
    }

    /// UXE-02C: one receipt line with the centre's chrome, groups and entries.
    #[cfg(feature = "component-lab")]
    pub(super) fn report_centre(&self, state: &str) {
        let rect = |r: blobin_ui_core::layout::Rect| {
            format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
        };
        let Some(frame) = self.notification_centre.frame() else {
            println!(
                "GNOS_UX_UXE02_CENTRE state={state} open={} panel=None",
                self.notification_centre.is_open()
            );
            return;
        };
        let groups = frame
            .groups
            .iter()
            .map(|g| {
                let entries = g
                    .entries
                    .iter()
                    .map(|e| {
                        let buttons = e
                            .buttons
                            .iter()
                            .map(|b| {
                                let kind = match &b.kind {
                                    crate::notification_toasts::ButtonKind::Close => {
                                        "close".to_owned()
                                    }
                                    crate::notification_toasts::ButtonKind::Action { label } => {
                                        format!("action:{}", label.replace(' ', "_"))
                                    }
                                    crate::notification_toasts::ButtonKind::Expand { .. } => {
                                        "expand".to_owned()
                                    }
                                };
                                format!("{kind}={}", rect(b.rect))
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        format!(
                            "(id={} rect={} buttons={{{buttons}}})",
                            e.entry.id.get(),
                            rect(e.rect)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("");
                format!(
                    "{{app={:?} rect={} pill={} expanded={} leaving={} entries=[{entries}]}}",
                    g.group.app_name,
                    rect(g.rect),
                    rect(g.pill),
                    g.expanded,
                    g.leaving
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        println!(
            "GNOS_UX_UXE02_CENTRE state={state} open={} panel={} count={} dnd={} dnd_rect={} clear={} list={} groups=[{groups}]",
            self.notification_centre.is_open(),
            rect(frame.panel),
            frame.count,
            frame.do_not_disturb,
            rect(frame.dnd),
            frame.clear.map_or("None".to_owned(), rect),
            rect(frame.list),
        );
    }

    /// UXE-02: sends toast commands to the service. Returns whether the
    /// event belonged to the toast drawer.
    #[cfg(feature = "component-lab")]
    pub(super) fn apply_toast_outcome(
        &mut self,
        outcome: crate::notification_toasts::ToastOutcome,
        cx: &mut Context<Self>,
    ) -> bool {
        let crate::notification_toasts::ToastOutcome {
            changed,
            consumed,
            commands,
        } = outcome;
        if !commands.is_empty() {
            let session = crate::product_notification_session::NotificationSession::shared(cx);
            for command in commands {
                let sent = session.read(cx).send(command.clone());
                println!("GNOS_UX_UXE02_COMMAND command={command:?} sent={sent}");
            }
        }
        if changed {
            self.start_motion_driver(cx);
            cx.notify();
        }
        consumed
    }
}
