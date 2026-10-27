//! DECK-01C / DECK-02: the AppStack in the scene: its motion, receipt,
//! rows, icons, launches and motion trace.

use super::*;

impl SceneHost {
    /// DECK-01C: advances the AppStack's extrusion; true while it moves.
    pub(super) fn tick_stack(&mut self, motion_now: Instant) -> bool {
        #[cfg(not(feature = "component-lab"))]
        let _ = motion_now;
        #[cfg(feature = "component-lab")]
        if self.review_native_rail {
            let moving = self.stack.needs_frame();
            let reduced = self.effective_reduced_motion();
            let still = self.stack.tick(motion_now, reduced);
            if moving && !still {
                self.report_stack("settled");
            }
            return still;
        }
        false
    }

    /// DECK-01C: `GNOS_UX_APPSTACK`: the envelope, the search field, the
    /// grid and every painted tile.
    #[cfg(feature = "component-lab")]
    pub(super) fn report_stack(&self, state: &str) {
        let rect = |r: blobin_ui_core::layout::Rect| {
            format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
        };
        let current = self
            .stack
            .current()
            .map_or("None".to_owned(), |row| row.entry.as_str().to_owned());
        let Some(frame) = self.stack.frame() else {
            println!(
                "GNOS_UX_APPSTACK state={state} open={} envelope=None current={current}",
                self.stack.is_open()
            );
            return;
        };
        let tiles = frame
            .tiles
            .iter()
            .map(|t| {
                format!(
                    "{{entry={} rect={} current={} icon={} running={} pinned={} marked={}}}",
                    t.row.entry.as_str(),
                    rect(t.rect),
                    t.current,
                    t.row.icon.is_some(),
                    t.row.running,
                    t.row.pinned,
                    t.marked.len()
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        // DECK-02: the tabs, the headings and the chips painted.
        let tabs = frame
            .tabs
            .iter()
            .map(|(tab, r, current)| {
                format!("{{{} rect={} current={current}}}", tab.slug(), rect(*r))
            })
            .collect::<Vec<_>>()
            .join(";");
        let headings = frame
            .headings
            .iter()
            .map(|(label, r)| format!("{{{label} rect={}}}", rect(*r)))
            .collect::<Vec<_>>()
            .join(";");
        let chips = frame
            .chips
            .iter()
            .map(|(_, label, r, active)| {
                format!(
                    "{{{} rect={} active={active}}}",
                    label.to_lowercase(),
                    rect(*r)
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        println!(
            "GNOS_UX_APPSTACK state={state} open={} phase={:.3} envelope={} search={} grid={} columns={} empty={} feedback={} current={current} tab={} tabs=[{tabs}] chips=[{chips}] headings=[{headings}] tiles=[{tiles}]",
            self.stack.is_open(),
            frame.phase,
            rect(frame.envelope),
            rect(frame.control),
            rect(frame.grid),
            frame.columns,
            frame.empty,
            frame.feedback.is_some(),
            self.stack.tab().slug(),
        );
    }

    /// UXE-03: an icon file for a desktop-entry icon name, cached.
    #[cfg(feature = "component-lab")]
    pub(super) fn app_icon(&mut self, name: &str) -> Option<std::path::PathBuf> {
        if let Some(known) = self.app_icons.get(name) {
            return known.clone();
        }
        let search = self.app_icon_search.get_or_insert_with(|| {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let data_home = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from);
            let data_dirs = std::env::var("XDG_DATA_DIRS").ok();
            let theme = std::env::var("GNOS_UX_REFERENCE_ICON_THEME")
                .unwrap_or_else(|_| "hicolor".to_owned());
            crate::product_reference_consumer::reference_icon_search(
                home.as_deref(),
                data_home.as_deref(),
                data_dirs.as_deref(),
                &theme,
            )
        });
        let resolved = match gnosix_icon_theme::lookup(
            search,
            &gnosix_icon_theme::IconRequest {
                name,
                size: 32,
                scale: 1,
                item_theme_path: None,
            },
        ) {
            gnosix_icon_theme::IconLookup::Resolved(icon) => Some(icon.path),
            _ => None,
        };
        self.app_icons.insert(name.to_owned(), resolved.clone());
        resolved
    }

    /// DECK-01C: re-run the search for the AppStack. The tiles reuse the
    /// UXE-03 search (its `@` modes, ordering and frequency) and its typed
    /// catalog identities (D03).
    #[cfg(feature = "component-lab")]
    pub(super) fn refresh_stack_rows(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.appstack_search.read(cx).query().to_owned();
        let query = crate::product_application_search::ApplicationSearchQuery::parse(&text);
        let query_empty = query.text.is_empty();
        let content = self.product_runtime.installed_applications();
        let hits = crate::product_application_search::search_installed_applications(
            content.clone(),
            &query,
            &crate::product_application_search::ApplicationSearchPreferences::default(),
        );
        let rows = hits
            .into_iter()
            .map(|hit| {
                let item = content.iter().find(|c| c.reference.entry() == &hit.entry);
                let detail = item
                    .and_then(|c| c.comment.clone().or_else(|| c.generic_name.clone()))
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| hit.name.clone());
                let icon = item
                    .and_then(|c| c.icon.clone())
                    .and_then(|name| self.app_icon(&name));
                // DECK-02 (A04): whether it runs and whether it is pinned,
                // as the Deck shows it.
                let card = self.deck_cards.iter().find(|c| c.key == hit.entry.as_str());
                crate::deck::stack::AppRow {
                    entry: hit.entry,
                    generation: hit.generation,
                    name: hit.name,
                    detail,
                    icon,
                    favourite: hit.favourite,
                    categories: item.map(|c| c.categories.clone()).unwrap_or_default(),
                    running: card.is_some_and(|c| !c.windows.is_empty()),
                    pinned: card.is_some_and(|c| c.pinned),
                }
            })
            .collect();
        let ordered = crate::deck::stack::order_rows(rows, self.stack.frequency(), query_empty);
        self.stack.set_rows(ordered, &text)
    }

    /// DECK-01C: apply an AppStack outcome. Returns whether the event was
    /// the AppStack's.
    #[cfg(feature = "component-lab")]
    pub(super) fn apply_stack_outcome(
        &mut self,
        outcome: crate::deck::stack::StackOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let crate::deck::stack::StackOutcome {
            consumed,
            changed,
            opened,
            closed,
            launch,
            drag,
            mode,
        } = outcome;
        // DECK-02 (A03): a chip sets the search's mode prefix.
        if let Some(mode) = mode {
            let query = self.appstack_search.read(cx).query().to_owned();
            let next = crate::deck::stack_content::with_mode(&query, mode);
            self.appstack_search
                .update(cx, |search, cx| search.set_query(next, cx));
            self.refresh_stack_rows(cx);
            self.report_stack("mode");
            let handle = gpui::Focusable::focus_handle(self.appstack_search.read(cx), cx);
            window.focus(&handle, cx);
        }
        if opened {
            self.refresh_product_runtime();
            self.appstack_search
                .update(cx, |search, cx| search.clear(cx));
            self.refresh_stack_rows(cx);
            let handle = gpui::Focusable::focus_handle(self.appstack_search.read(cx), cx);
            window.focus(&handle, cx);
            self.report_stack("opened");
        }
        if closed {
            self.appstack_search
                .update(cx, |search, cx| search.clear(cx));
            self.report_stack("closed");
        }
        // DECK-01D: a tile pulled out of the grid is carried to the Deck.
        if let Some(row) = drag {
            self.deck_tile_drag(row, cx);
        }
        if let Some((entry, generation)) = launch {
            self.launch_from_stack(entry, generation, window, cx);
        } else if changed && !opened && !closed {
            self.report_stack("changed");
        }
        if changed || opened || closed {
            self.start_motion_driver(cx);
            cx.notify();
        }
        consumed
    }

    /// DECK-01C: launch the tile the user saw, through the typed capability.
    /// The tile's catalog generation must still be current; otherwise the
    /// launch is refused rather than retargeted (the AppStack identity rule).
    #[cfg(feature = "component-lab")]
    pub(super) fn launch_from_stack(
        &mut self,
        entry: gnosix_application_catalog::DesktopEntryId,
        generation: gnosix_application_catalog::CatalogGeneration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_product_runtime();
        let selected = self
            .product_runtime
            .installed_applications()
            .into_iter()
            .find(|c| c.reference.entry() == &entry && c.reference.generation() == &generation);
        let Some(selected) = selected else {
            println!(
                "GNOS_UX_APPSTACK_LAUNCH entry={} outcome=STALE detail=catalog_changed",
                entry.as_str()
            );
            self.stack
                .set_feedback(Some("The application list changed; try again".to_owned()));
            self.refresh_stack_rows(cx);
            self.report_stack("feedback");
            return;
        };
        let record =
            crate::product_launch::launch_application(&mut self.product_runtime, &selected);
        // CURSOR-01B: the pointer says it is starting until its window is
        // there.
        #[cfg(feature = "component-lab")]
        self.note_launch(&record, cx);
        let launched = record.terminal.as_ref().is_some_and(|t| t.is_launched());
        println!(
            "GNOS_UX_APPSTACK_LAUNCH entry={} outcome={} detail={}",
            record.entry.as_str(),
            record.terminal.as_ref().map_or("NONE", |t| t.code()),
            record.detail
        );
        if launched {
            self.stack.record_launch(&entry);
            let outcome = self.stack.close();
            self.apply_stack_outcome(outcome, window, cx);
        } else {
            self.stack
                .set_feedback(Some(format!("Could not start {}", selected.name)));
            self.report_stack("feedback");
        }
    }

    pub(super) fn begin_appstack_motion_trace(&mut self, target_open: bool) {
        if self.appstack_motion_trace {
            self.appstack_motion_trace_target = Some(target_open);
            self.appstack_motion_trace_frame = 0;
            self.appstack_motion_trace_started = Some(Instant::now());
            self.appstack_motion_trace_previous = None;
        }
    }

    pub(super) fn record_appstack_motion_frame(
        &mut self,
        frame: &crate::product_shared_envelope::SharedDockEnvelope,
    ) {
        if !self.appstack_motion_trace {
            return;
        }
        let open = frame.motion.target_open;
        if self.appstack_motion_trace_target != Some(open) {
            self.begin_appstack_motion_trace(open);
        }
        if self.appstack_motion_trace_frame >= 512 {
            return;
        }
        let now = Instant::now();
        let dt = self
            .appstack_motion_trace_previous
            .replace(now)
            .map_or(0.0, |p| now.duration_since(p).as_secs_f64() * 1000.0);
        let elapsed = self
            .appstack_motion_trace_started
            .map_or(0.0, |p| now.duration_since(p).as_secs_f64() * 1000.0);
        self.appstack_motion_trace_frame += 1;
        let rect =
            |r: gnos_ux_surface::grammar::GeometryRect| [r.left(), r.top(), r.right(), r.bottom()];
        println!(
            "GNOS_UX_APPSTACK_MOTION_TRACE {{\"schema\":\"gnosix.shared-envelope.debug.v1\",\"surface\":\"AppStackInner\",\"parent\":\"SharedDockEnvelope\",\"material_owner\":\"SharedDockEnvelope\",\"inner_roles\":[\"DockControls\",\"ApplicationList\"],\"frame\":{},\"elapsed_ms\":{elapsed:.3},\"delta_ms\":{dt:.3},\"driver_active\":{},\"needs_next_frame\":{},\"target_open\":{open},\"horizontal\":{:.6},\"vertical\":{:.6},\"reveal\":{:.6},\"velocity\":[{:.6},{:.6},{:.6}],\"outer_bounds\":{:?},\"structural_dock\":{:?},\"painted_dock_request\":{:?},\"DockInner\":{:?},\"AppStackInner\":{:?},\"content_clip\":{:?},\"interactable\":{},\"baseline\":{},\"input_regions\":{:?}}}",
            self.appstack_motion_trace_frame,
            self.motion_driver.active,
            !frame.motion.is_settled_for(open),
            frame.motion.horizontal.value,
            frame.motion.vertical.value,
            frame.motion.reveal.value,
            frame.motion.horizontal.velocity,
            frame.motion.vertical.velocity,
            frame.motion.reveal.velocity,
            rect(frame.geometry.outer),
            rect(frame.geometry.structural),
            rect(frame.geometry.painted_request),
            rect(frame.dock.bounds),
            rect(frame.appstack.bounds),
            rect(frame.content_clip),
            frame.appstack.interactable,
            frame.geometry.outer.bottom(),
            frame
                .input_regions()
                .into_iter()
                .map(rect)
                .collect::<Vec<_>>()
        );
    }

    /// Toggle the AppStack surface.
    ///
    /// Opening the AppStack closes the workspace summary and vice versa, which
    /// is what `ProductPanelState::toggled` encodes. The transition is a
    /// replacement rather than a second flag so the two surfaces can never both
    /// be open.
    ///
    /// Gated with the Dock control that owns it: the affordance is a production
    /// Dock item, alongside the workspace controls, and a build without that
    /// Dock has no affordance to activate.
    #[cfg(feature = "r13-production-client")]
    pub(super) fn toggle_appstack_panel(&mut self, cx: &mut Context<Self>) {
        self.refresh_product_runtime();
        if !self.product_runtime.semantic().can_open_appstack() {
            return;
        }
        self.product_panel = self.product_panel.toggled(ProductPanelState::AppStack);
        self.begin_appstack_motion_trace(self.product_panel.is_appstack());
        println!(
            "GNOS_UX_PRODUCT_APPSTACK panel={}",
            self.product_panel.as_str()
        );
        if let Some(perf) = &self.perf_counters {
            perf.interaction_notifies.fetch_add(1, Ordering::Relaxed);
        }
        // The workspace summary animates out of the bar, so closing one surface
        // to open another is a motion the solver still has to resolve.
        self.start_motion_driver(cx);
        cx.notify();
    }
}
