//! R13: keys and focus in the scene: the scene's keys, launching an
//! installed application, and which surface holds the focus.

use super::*;

impl SceneHost {
    /// Report the scene's own focus ownership, independently of any
    /// accessibility bus.
    ///
    /// The value comes from `FocusHandle::is_focused(window)`, so it observes
    /// the framework's focus state directly rather than what AccessKit chose
    /// to publish. That makes agreement with the AT-SPI observer real evidence
    /// instead of a circular read-back, and it is the only reason a
    /// `GrabFocus` returning true is not accepted as proof of focus.
    /// The product's single keyboard routing point.
    ///
    /// Traversal and activation are decided here, once, for the whole
    /// interactive surface, rather than per control. Traversal uses gpui's
    /// own tab order, which is built from the same `tab_stop` and
    /// `tab_index` the accessibility tree reflects, so the two cannot
    /// drift apart. Each key produces at most one action, so a keypress
    /// can never move focus twice.
    #[cfg(feature = "r13-production-client")]
    pub(super) fn handle_scene_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        dispatch_scene_key(event, window, cx);
    }

    /// The single product action behind a AppStack row.
    ///
    /// Every route into a launch - pointer, keyboard, assistive activation -
    /// lands here. That is the whole reason this method exists rather than three
    /// call sites: a second activation path is a second chance to run the same
    /// intent twice, and the AppStack's contract is that one selection produces
    /// one receipt.
    ///
    /// The selection is carried as a `DesktopEntryId` and re-resolved against the
    /// runtime's *current* content, which preserves the content's own
    /// generation fence. Looking the entry up in whatever the catalog holds now
    /// would silently retarget a stale selection onto a newer catalog, and the
    /// capability exists precisely to refuse that.
    pub(super) fn activate_installed_application(
        &mut self,
        entry: gnosix_application_catalog::DesktopEntryId,
        cx: &mut Context<Self>,
    ) {
        self.refresh_product_runtime();
        let Some(selected) = self
            .product_runtime
            .installed_applications()
            .into_iter()
            .find(|candidate| candidate.reference.entry() == &entry)
        else {
            // The entry vanished from the catalog between render and click. Not
            // a launch attempt, so there is no receipt; say so plainly rather
            // than reporting a refusal the capability never issued.
            println!(
                "GNOS_UX_PRODUCT_LAUNCH panel={} entry={} status=absent",
                self.product_panel.as_str(),
                entry.as_str()
            );
            cx.notify();
            return;
        };
        let record =
            crate::product_launch::launch_application(&mut self.product_runtime, &selected);
        // CURSOR-01B: the pointer says it is starting until its window is
        // there.
        #[cfg(feature = "component-lab")]
        self.note_launch(&record, cx);
        let terminal = record.terminal.as_ref().map(|t| t.code()).unwrap_or("NONE");
        println!(
            "GNOS_UX_PRODUCT_LAUNCH panel={} entry={} outcome={} detail={}",
            self.product_panel.as_str(),
            record.entry.as_str(),
            terminal,
            record.detail
        );
        if let Some(perf) = &self.perf_counters {
            perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
        }
        // v1 policy: a successful launch hands the desktop back to the user.
        // A refusal leaves the AppStack open so the feedback line stays visible,
        // which is the difference between "nothing happened" and an explanation.
        if record.terminal.as_ref().is_some_and(|t| t.is_launched()) {
            self.product_panel = self.product_panel.after_launch(&record);
            self.begin_appstack_motion_trace(false);
        }
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// Keep this host's focus in step with the session-wide arbitration.
    ///
    /// Called from render, which is the one place every focus source
    /// passes through: a key, a click, or an assistive `GrabFocus` all
    /// end up changing `window.focus` and repainting. Claiming here means
    /// there is no accessibility-only focus universe, and losing here
    /// means a stale handle cannot keep claiming the session.
    #[cfg(feature = "r13-production-client")]
    pub(super) fn arbitrate_product_focus(
        &mut self,
        identities: &[ProductControlId],
        handles: &[gpui::FocusHandle],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row_index) = focused_appstack_row_index(identities, handles, window) {
            // GPUI moves focus through every painted tab stop, but does not
            // reveal a focused child inside a scroll container. Queue the
            // nearest-edge scroll before this frame's list prepaint.
            self.appstack_scroll.scroll_to_item(row_index);
        }
        let local = identities
            .iter()
            .zip(handles.iter())
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(id, _)| id.clone());

        match local {
            Some(control) => {
                if let Some(displaced) = self.focus_coordinator.claim(self.host_id, control) {
                    // Another output still holds focus. Blur it so the
                    // desktop has exactly one keyboard focus, and defer
                    // the call: touching another window from inside this
                    // render would be re-entrant.
                    if let Some(window) = self.host_windows.window(displaced.host) {
                        cx.defer(move |cx| {
                            // A host that has already gone away is not a
                            // failure: the invariant it would have
                            // enforced no longer applies to it.
                            let _ = window.update(cx, |_, window, cx| window.blur(cx));
                        });
                    }
                }
            }
            None => {
                // This host has no focus of its own, so it cannot be the
                // session's focus owner.
                self.focus_coordinator.release(self.host_id);
            }
        }
    }

    #[cfg(feature = "r13-production-client")]
    /// Record key events the window actually receives.
    ///
    /// Read-only and opt-in, like the focus oracle. Without it, "Enter did
    /// nothing" cannot be distinguished from "Enter never arrived", which are
    /// different defects in different layers.
    pub(super) fn report_key_event(&self, event: &gpui::KeyDownEvent, window: &Window) {
        if !self.key_qualification_diagnostics {
            return;
        }
        let focused = self
            .product_focus
            .order()
            .iter()
            .enumerate()
            .find(|(_, identity)| {
                self.product_focus
                    .handle(identity)
                    .is_some_and(|handle| handle.is_focused(window))
            });
        println!(
            "GNOS_UX_SCENE_KEY_EVENT scene={} key={:?} keystroke={:?} is_focused={} focused_control={} tab_index={:?}",
            self.scene_id.value(),
            format!("{:?}", event.keystroke.key),
            format!("{:?}", event.keystroke.modifiers),
            window.is_window_active(),
            focused.map_or("NONE", |(_, identity)| identity.as_str()),
            focused.map(|(index, _)| index),
        );
    }

    #[cfg(feature = "r13-production-client")]
    pub(super) fn report_product_focus(
        &mut self,
        identities: &[ProductControlId],
        handles: &[gpui::FocusHandle],
        window: &Window,
    ) {
        if !self.focus_qualification_diagnostics {
            return;
        }
        for (identity, handle) in identities.iter().zip(handles.iter()) {
            let focused = handle.is_focused(window);
            let previous = self.focus_report.get(identity.as_str()).copied();
            if previous == Some(focused) {
                continue;
            }
            self.focus_report
                .insert(identity.as_str().to_owned(), focused);
            println!(
                "GNOS_UX_SCENE_FOCUS_ORACLE scene={} id={} focused={focused}",
                self.scene_id.value(),
                identity.as_str(),
            );
        }
    }
}

/// The focus scope shared by every output host of one session.
///
/// Created by the session that opens the hosts and cloned into each of
/// them, so the whole product shares one arbitration point without any
/// global mutable state. A single-output session simply creates its own.
#[derive(Clone, Default)]
pub(super) struct ProductFocusScope {
    pub(super) coordinator: ProductFocusCoordinator,
    pub(super) windows: ProductHostWindows,
}

impl ProductFocusScope {
    pub(super) fn new() -> Self {
        Self::default()
    }
}
