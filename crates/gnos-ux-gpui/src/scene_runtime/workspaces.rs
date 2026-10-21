//! WS-02: the workspace manager in the scene.
//!
//! The panel (`workspace_panel`) is pure. Here it gets the workspaces the
//! Bar shows and, for each, the windows the compositor reports on it (the
//! same filter and icons as the Bar's window marks). Its effects reach the
//! authorities that already carry them out: a workspace through the R13
//! activation path (as a click on the rail), a window through the R13 focus
//! request (as the Deck's sheet). Receipts: `GNOS_UX_WORKSPACES` and
//! `GNOS_UX_WORKSPACES_ACTION`.

use super::*;
use crate::workspace_panel::{WorkspaceEffect, WorkspaceModel, WorkspaceOutcome, WsCard, WsWindow};

#[derive(Default)]
pub(super) struct WorkspacesState {
    /// The windows behind each card, as the model was last built.
    windows: Vec<Vec<crate::product_scene::ProductApplicationContent>>,
    reported: Option<String>,
    /// WS-04: the shared live thumbnails, and the windows kept from them
    /// (`GNOS_UX_THUMBNAILS_WISH`, on change).
    thumbnails: Option<(
        gpui::Entity<crate::product_thumbnails::ThumbnailHub>,
        gpui::Subscription,
    )>,
    wish_reported: Option<String>,
}

impl SceneHost {
    /// The centre of the workspaces module along the Bar at rest on `side`.
    fn workspaces_anchor(&self, side: crate::rail_topology::RailSide) -> Option<f32> {
        self.module_anchor(side, crate::reference_bar::IdleBarEntry::Workspaces)
    }

    fn workspaces_model(
        &self,
    ) -> (
        WorkspaceModel,
        Vec<Vec<crate::product_scene::ProductApplicationContent>>,
    ) {
        let runtime = &self.product_runtime;
        let catalog = runtime.application_catalog();
        let (applications, area) = match runtime.semantic() {
            crate::product_scene::ProductSemanticState::Current(content) => {
                (content.applications.as_slice(), content.output_area)
            }
            _ => (&[][..], None),
        };
        // WS-03: a window's place as fractions of its output.
        let place = |a: &crate::product_scene::ProductApplicationContent| {
            let (area, b) = (area?, a.bounds?);
            let (w, h) = (area.size.width.get() as f32, area.size.height.get() as f32);
            let l = (b.origin.x - area.origin.x) as f32 / w;
            let t = (b.origin.y - area.origin.y) as f32 / h;
            Some([
                l,
                t,
                l + b.size.width.get() as f32 / w,
                t + b.size.height.get() as f32 / h,
            ])
        };
        let aspect = area.map_or(0.0, |a| {
            a.size.width.get() as f32 / a.size.height.get() as f32
        });
        let mut cards = Vec::new();
        let mut behind = Vec::new();
        for target in runtime.workspace_manager_targets() {
            let on_it: Vec<_> = applications
                .iter()
                .filter(|a| a.workspace == Some(target.reference))
                .filter(|a| !crate::reference_bar::workspace_window_is_ignored(a))
                .cloned()
                .collect();
            cards.push(WsCard {
                label: target.label.clone(),
                active: runtime.workspace_manager_is_active(target),
                windows: on_it
                    .iter()
                    .map(|a| WsWindow {
                        title: if a.title.is_empty() {
                            a.class.clone()
                        } else {
                            a.title.clone()
                        },
                        symbol: crate::reference_bar::window_symbol(catalog, &a.class),
                        focused: a.focus == gnosix_desktop_model::SurfaceFocus::Focused,
                        bounds: place(a),
                        surface: Some(a.reference.surface_id()),
                    })
                    .collect(),
            });
            behind.push(on_it);
        }
        (WorkspaceModel { cards, aspect }, behind)
    }

    /// Place the panel and give it what the compositor reports.
    pub(super) fn sync_workspaces(
        &mut self,
        side: crate::rail_topology::RailSide,
        viewport: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let anchor = self.workspaces_anchor(side);
        let mut changed = self.workspaces.place(side, viewport, anchor);
        let (model, behind) = self.workspaces_model();
        changed |= self.workspaces.set_model(model);
        self.workspaces_state.windows = behind;
        // WS-02: the operator's settings.
        let prefs = crate::surface_prefs::current();
        changed |= self
            .workspaces
            .set_max_windows(prefs.workspaces_windows as usize);
        // WS-03: the operator's view.
        let view = match prefs.workspaces_view {
            crate::surface_prefs::WorkspacesView::Map => crate::workspace_panel::WsView::Map,
            crate::surface_prefs::WorkspacesView::Strip => crate::workspace_panel::WsView::Strip,
            crate::surface_prefs::WorkspacesView::List => crate::workspace_panel::WsView::List,
        };
        changed |= self
            .workspaces
            .set_view(view, prefs.workspaces_tile_px as f32);
        if changed {
            self.report_workspaces("changed");
            if self.workspaces.needs_frame() {
                self.start_motion_driver(cx);
            }
            cx.notify();
        }
    }

    /// WS-04: wish for the thumbnails of the windows the panel shows as
    /// miniatures (map and strip), at their miniature's size in device
    /// pixels; nothing while the panel is in, thumbnails are off, or for an
    /// application the operator's list excludes.
    pub(super) fn sync_thumbnails(&mut self, scale: f32, cx: &mut Context<Self>) {
        let prefs = crate::surface_prefs::current();
        let mut wish = Vec::new();
        let mut excluded = Vec::new();
        if prefs.thumbnails.live && self.workspaces.frame().is_some() {
            for row in self.workspaces.rows() {
                let crate::workspace_panel::WsRow::Mini {
                    card,
                    index,
                    window,
                    width,
                    height,
                    ..
                } = row
                else {
                    continue;
                };
                let Some(surface) = window.surface else {
                    continue;
                };
                let Some(app) = self
                    .workspaces_state
                    .windows
                    .get(card)
                    .and_then(|w| w.get(index))
                else {
                    continue;
                };
                if let Some(entry) = prefs.thumbnails.excluded_by(&app.class, &app.class) {
                    excluded.push(format!("{}:{entry}", app.class));
                    continue;
                }
                let size = (
                    (width * scale).round().max(1.0) as u32,
                    (height * scale).round().max(1.0) as u32,
                );
                wish.push((surface, size));
            }
        }
        let line = format!(
            "GNOS_UX_THUMBNAILS_WISH host={} windows={} excluded=[{}]",
            self.host_token,
            wish.len(),
            excluded.join(",")
        );
        if self.workspaces_state.wish_reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.workspaces_state.wish_reported = Some(line);
        }
        if wish.is_empty() && self.workspaces_state.thumbnails.is_none() {
            return;
        }
        let hub = match &self.workspaces_state.thumbnails {
            Some((hub, _)) => hub.clone(),
            None => {
                let hub = crate::product_thumbnails::ThumbnailHub::shared(cx);
                let observe = cx.observe(&hub, |_, _, cx| cx.notify());
                self.workspaces_state.thumbnails = Some((hub.clone(), observe));
                hub
            }
        };
        let host = self.host_token;
        let refresh = prefs.thumbnails.refresh();
        hub.update(cx, |hub, cx| hub.wish(host, wish, refresh, cx));
    }

    /// WS-04: the applications Settings lists for thumbnails: those with a
    /// window now, then the rest of the list in force; each with the entry
    /// that keeps it from being captured.
    pub(super) fn thumbnail_apps(&self) -> Vec<(String, Option<String>)> {
        let prefs = crate::surface_prefs::current();
        let mut classes: Vec<String> = match self.product_runtime.semantic() {
            crate::product_scene::ProductSemanticState::Current(content) => content
                .applications
                .iter()
                .map(|a| a.class.clone())
                .filter(|c| !c.is_empty())
                .collect(),
            _ => Vec::new(),
        };
        classes.sort_by_key(|c| c.to_lowercase());
        classes.dedup();
        let mut rows: Vec<(String, Option<String>)> = classes
            .iter()
            .map(|c| (c.clone(), prefs.thumbnails.excluded_by(c, c)))
            .collect();
        for entry in prefs.thumbnails.excluded() {
            if !rows
                .iter()
                .any(|(_, by)| by.as_deref() == Some(entry.as_str()))
            {
                rows.push((entry.clone(), Some(entry)));
            }
        }
        rows.truncate(usize::from(u8::MAX));
        rows
    }

    /// WS-04: the thumbnails taken of the windows the panel shows.
    pub(super) fn workspace_thumbnails(
        &self,
        cx: &App,
    ) -> std::collections::HashMap<gnosix_desktop_model::SurfaceId, std::sync::Arc<gpui::RenderImage>>
    {
        let Some((hub, _)) = &self.workspaces_state.thumbnails else {
            return Default::default();
        };
        let hub = hub.read(cx);
        self.workspaces
            .model()
            .cards
            .iter()
            .flat_map(|c| &c.windows)
            .filter_map(|w| {
                let surface = w.surface?;
                Some((surface, hub.image(surface)?))
            })
            .collect()
    }

    /// `GNOS_UX_WORKSPACES`: the panel, its cards and where they stand.
    pub(super) fn report_workspaces(&mut self, state: &str) {
        let cards = self
            .workspaces
            .model()
            .cards
            .iter()
            .map(|c| {
                format!(
                    "{{{} active={} windows={}}}",
                    c.label,
                    c.active,
                    c.windows.len()
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let frame = self.workspaces.frame();
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
        // LIVE-03 / WS-02B: where each card and window row stands (scene
        // coordinates), so a check aims at a row rather than guessing.
        let rows = frame.as_ref().map_or(String::new(), |f| {
            self.workspaces
                .rows()
                .iter()
                .filter_map(|r| {
                    let name = match r {
                        crate::workspace_panel::WsRow::Card { index, .. } => {
                            format!("card:{index}")
                        }
                        crate::workspace_panel::WsRow::Tile { index, .. } => {
                            format!("tile:{index}")
                        }
                        crate::workspace_panel::WsRow::Mini { card, index, .. } => {
                            let pid = self
                                .workspaces_state
                                .windows
                                .get(*card)
                                .and_then(|w| w.get(*index))
                                .map_or(0, |w| w.process_id);
                            format!("mini:{card}.{index}:pid{pid}")
                        }
                        crate::workspace_panel::WsRow::Window { card, index, .. } => {
                            let pid = self
                                .workspaces_state
                                .windows
                                .get(*card)
                                .and_then(|w| w.get(*index))
                                .map_or(0, |w| w.process_id);
                            format!("window:{card}.{index}:pid{pid}")
                        }
                        _ => return None,
                    };
                    let (x, y, w, h) = r.rect();
                    Some(format!(
                        "{name}@[{:.1},{:.1},{w:.1},{h:.1}]",
                        f.rect.left() + x,
                        f.rect.top() + y
                    ))
                })
                .collect::<Vec<_>>()
                .join(";")
        });
        let line = format!(
            "GNOS_UX_WORKSPACES state={state} open={} rect={rect} cards=[{cards}] rows=[{rows}]",
            self.workspaces.is_open(),
        );
        if state != "changed" || self.workspaces_state.reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.workspaces_state.reported = Some(line);
        }
    }

    /// Carry out what the panel asks; returns whether it consumed the event.
    pub(super) fn apply_workspaces_outcome(
        &mut self,
        outcome: WorkspaceOutcome,
        cx: &mut Context<Self>,
    ) -> bool {
        for effect in &outcome.effects {
            match *effect {
                WorkspaceEffect::Activate(index) => {
                    let done = self.rail_workspace(index, cx);
                    println!("GNOS_UX_WORKSPACES_ACTION action=activate index={index} done={done}");
                }
                WorkspaceEffect::Focus(card, index) => {
                    let Some(target) = self
                        .workspaces_state
                        .windows
                        .get(card)
                        .and_then(|w| w.get(index))
                        .cloned()
                    else {
                        println!(
                            "GNOS_UX_WORKSPACES_ACTION action=focus card={card} index={index} outcome=STALE"
                        );
                        continue;
                    };
                    #[cfg(feature = "r13-production-client")]
                    {
                        let record = self
                            .product_controller
                            .focus_application(&mut self.product_runtime, target.clone());
                        println!(
                            "GNOS_UX_WORKSPACES_ACTION action=focus pid={} outcome={}",
                            target.process_id,
                            record.terminal.map_or("NONE", |t| t.as_str())
                        );
                    }
                    #[cfg(not(feature = "r13-production-client"))]
                    println!(
                        "GNOS_UX_WORKSPACES_ACTION action=focus pid={} outcome=NO_CLIENT",
                        target.process_id
                    );
                }
                // WS-02B: the carried window onto another workspace, through
                // the activation authority, which checks it lands there.
                WorkspaceEffect::Move { card, index, to } => {
                    let window = self
                        .workspaces_state
                        .windows
                        .get(card)
                        .and_then(|w| w.get(index))
                        .cloned();
                    let destination = self
                        .product_runtime
                        .workspace_manager_targets()
                        .get(to)
                        .cloned();
                    let (Some(window), Some(destination)) = (window, destination) else {
                        println!(
                            "GNOS_UX_WORKSPACES_ACTION action=move card={card} index={index} to={to} outcome=STALE"
                        );
                        continue;
                    };
                    #[cfg(feature = "r13-production-client")]
                    {
                        let record = self.product_controller.move_application(
                            &mut self.product_runtime,
                            window.clone(),
                            destination,
                        );
                        println!(
                            "GNOS_UX_WORKSPACES_ACTION action=move pid={} to={to} outcome={}",
                            window.process_id,
                            record.terminal.map_or("NONE", |t| t.as_str())
                        );
                    }
                    #[cfg(not(feature = "r13-production-client"))]
                    {
                        let _ = destination;
                        println!(
                            "GNOS_UX_WORKSPACES_ACTION action=move pid={} to={to} outcome=NO_CLIENT",
                            window.process_id
                        );
                    }
                }
            }
        }
        // A move keeps the panel out: the window is seen arriving on its new
        // workspace's card.
        if outcome
            .effects
            .iter()
            .any(|e| !matches!(e, WorkspaceEffect::Move { .. }))
        {
            // A choice made: the panel goes back into the Bar, and stays
            // there until the pointer leaves the module.
            self.attention_dismiss(crate::attention::Attendant::Workspaces, cx);
        }
        if outcome.changed || !outcome.effects.is_empty() {
            self.start_motion_driver(cx);
            cx.notify();
        }
        outcome.consumed
    }
}
