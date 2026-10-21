#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

//! Developer-only deterministic GNOS.UX component lab.
//!
//! This binary never observes or mutates the live compositor by default. It
//! renders real production primitives against synthetic ProductRuntime
//! fixtures selected on the command line.

use std::{env, process, sync::Arc, time::Duration};

use gnos_ux_gpui::{
    lab_appstack_surface,
    product_bar::{BarOrientation, BarTrayPresentation, render_audio_status, render_tray},
    product_design::{
        ProductControlFocus, ProductControlState, ProductDesignContext, ProductPalette,
        ProductSpacing, ProductStateKind, ProductStatusTone, ProductTextStyle, ProductTooltip,
        application_item, badge, button, icon_button, list_container, notification_card, panel,
        separator, state_presentation, text, workspace_item,
    },
    product_diagnostics::{ProductDiagnosticSemanticState, ProductDiagnosticSnapshot},
    product_focus::{ProductControlId, ProductFocusState, traversable_focus_handle},
    product_focus_coordinator::{ProductFocusCoordinator, ProductHostId},
    product_keyboard::{SceneKeyAction, route_scene_key},
    product_lab::{FIXTURE_NAMES, ProductLabFixture, ProductLabOutput, fixture},
    product_launch::launch_lab_application,
    product_runtime::{
        ProductActionState, ProductActionTerminal, ProductLaunchState, ProductRuntime,
    },
};
use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
};
use gpui::{
    AnyElement, App, Bounds, Context, FocusHandle, InteractiveElement, IntoElement, Render,
    StatefulInteractiveElement, Styled, Window, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};

const LAB_WIDTH: f32 = 1180.0;
const LAB_HEIGHT: f32 = 820.0;

#[derive(Debug)]
struct LabOptions {
    fixture: String,
    list_fixtures: bool,
    dump_fixture: bool,
    dump_a11y_tree: bool,
    /// Print the AppStack's resolved geometry: Dock, derived body, joint,
    /// connector, neck, and the hull's own command stream.
    ///
    /// This is the geometric evidence for the directional extrusion. A
    /// screenshot shows a shape; this shows the numbers a reviewer checks, so
    /// whether the AppStack reads as derived from the Dock is decidable rather
    /// than merely persuasive.
    dump_appstack_geometry: bool,
    /// Render only the AppStack surface, filling the window.
    ///
    /// The gallery's own panels use the same dark fills the hull does, so a
    /// capture of the full gallery cannot be measured: the question "did the
    /// hull paint where the geometry says" is unanswerable when something else
    /// painted the same colour. Surface-only makes the capture a review of the
    /// surface and nothing else, and it changes no geometry - the same
    /// `resolve_surface` and the same `render` run either way.
    surface_only: bool,
    /// Override the window size, as `WIDTHxHEIGHT`.
    ///
    /// The compositor clamps a window that does not fit the workspace, and
    /// GPUI then lays out for the size it asked for while the surface it is
    /// given is a different one. That mismatch is invisible in a gallery and
    /// fatal for capture: the geometry lands somewhere other than where the
    /// capture says it is, so a review of the picture is a review of nothing.
    /// Requesting a size the compositor will actually grant removes the
    /// mismatch instead of documenting it.
    size_override: Option<(f32, f32)>,
    /// Print a bounded, read-only projection of GPUI's own focus ownership.
    ///
    /// This is the independent oracle for focus qualification: it is derived
    /// from `FocusHandle::is_focused(window)` and never from AccessKit or
    /// AT-SPI, so agreement with the accessibility bus is real evidence
    /// rather than a circular read-back.
    focus_diagnostics: bool,
}

impl LabOptions {
    fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut fixture = "workspace-hidden".to_owned();
        let mut list_fixtures = false;
        let mut dump_fixture = false;
        let mut dump_a11y_tree = false;
        let mut dump_appstack_geometry = false;
        let mut surface_only = false;
        let mut size_override = None;
        let mut focus_diagnostics = false;

        for arg in args.into_iter().skip(1) {
            if arg == "--list-fixtures" {
                list_fixtures = true;
            } else if arg == "--dump-fixture" {
                dump_fixture = true;
            } else if arg == "--dump-a11y-tree" {
                dump_a11y_tree = true;
            } else if arg == "--dump-appstack-geometry" {
                dump_appstack_geometry = true;
            } else if arg == "--surface-only" {
                surface_only = true;
            } else if let Some(value) = arg.strip_prefix("--size=") {
                let (w, h) = value
                    .split_once('x')
                    .ok_or_else(|| format!("--size wants WIDTHxHEIGHT, got {value}"))?;
                let width: f32 = w
                    .parse()
                    .map_err(|_| format!("--size width is not a number: {w}"))?;
                let height: f32 = h
                    .parse()
                    .map_err(|_| format!("--size height is not a number: {h}"))?;
                if !(width > 0.0 && height > 0.0) {
                    return Err(format!("--size must be positive, got {value}"));
                }
                size_override = Some((width, height));
            } else if arg == "--focus-diagnostics" {
                focus_diagnostics = true;
            } else if arg == "--help" || arg == "-h" {
                return Err("help".to_owned());
            } else if let Some(value) = arg.strip_prefix("--fixture=") {
                if value.is_empty() {
                    return Err("fixture name cannot be empty".to_owned());
                }
                fixture = value.to_owned();
            } else {
                return Err(format!("unknown argument: {arg}"));
            }
        }

        Ok(Self {
            fixture,
            list_fixtures,
            dump_fixture,
            dump_a11y_tree,
            dump_appstack_geometry,
            surface_only,
            size_override,
            focus_diagnostics,
        })
    }
}

struct LabView {
    fixture: ProductLabFixture,
    last_interaction: String,
    dump_a11y_tree: bool,
    dump_appstack_geometry: bool,
    surface_only: bool,
    appstack_geometry_dumped: bool,
    a11y_redraw_requests: u8,
    a11y_dumped: bool,
    /// Root focus handle for the lab window.
    ///
    /// The Component Lab is a real application window, so it must take part
    /// in the platform accessibility stack exactly like the product scene:
    /// a root focus handle makes the lab window focusable and discoverable on
    /// the accessibility bus instead of publishing an empty application.
    focus_handle: FocusHandle,
    /// One stable focus handle per actionable lab control, keyed by stable
    /// product identity so focus survives rerenders of the same control.
    control_focus: ProductFocusState,
    /// The lab exercises the same single-focus arbitration the product
    /// scene uses, so a qualification run measures the shipped policy
    /// rather than a lab-only imitation of it.
    focus_coordinator: ProductFocusCoordinator,
    /// Runtime backing the AppStack section.
    ///
    /// Launch is a session capability rather than an output-local one, so it
    /// does not live in any single output's runtime. It is seeded from a real
    /// discovered application catalog so that what the lab exercises is the
    /// shipped resolution path, not a hand-written entry.
    launch_runtime: ProductRuntime,
    focus_diagnostics: bool,
    /// Last reported focus state per control, so the diagnostic prints only on
    /// transitions rather than once per frame.
    focus_report: std::collections::BTreeMap<String, bool>,
}

/// Look up a gallery control's focus handle by its gallery index.
fn gallery_focus(focus: &[FocusHandle], index: usize) -> ProductControlFocus<'_> {
    match focus.get(index) {
        Some(handle) => ProductControlFocus::new(handle, index as isize),
        None => ProductControlFocus::inert(),
    }
}

impl LabView {
    /// Reconcile the lab's focus handles with the fixture and return owned
    /// clones in layout order.
    ///
    /// Handles are cloned because `FocusHandle` is refcounted, and because the
    /// element tree must be able to borrow a handle while `self` is mutably
    /// borrowed for rendering.
    fn sync_control_focus(&mut self, cx: &mut App) -> Vec<(String, FocusHandle)> {
        let mut identities = Vec::new();
        for (output_index, output) in self.fixture.outputs.iter().enumerate() {
            for (index, workspace) in output
                .runtime
                .workspace_manager_targets()
                .iter()
                .enumerate()
            {
                identities.push(ProductControlId(
                    format!("lab-workspace-{output_index}-{index}:{}", workspace.label).into(),
                ));
            }
            for (index, application) in output.runtime.application_targets().iter().enumerate() {
                identities.push(ProductControlId(
                    format!(
                        "lab-application-{output_index}-{index}:{}",
                        application.title
                    )
                    .into(),
                ));
            }
        }
        for extra in [
            "lab-button-default",
            "lab-icon-button",
            "lab-button-selected",
            "lab-button-disabled",
        ] {
            identities.push(ProductControlId(extra.into()));
        }
        self.control_focus.reconcile(&identities);
        // `reconcile` only prunes handles for controls that disappeared; the
        // retained ones are reused and the new ones are minted here, so focus
        // identity survives an unrelated semantic generation.
        identities
            .iter()
            .map(|id| {
                let handle = self
                    .control_focus
                    .ensure(id, &mut || traversable_focus_handle(cx))
                    .clone();
                (id.0.to_string(), handle)
            })
            .collect()
    }

    /// The product's single keyboard routing point, matching the scene.
    fn handle_scene_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus_diagnostics {
            println!(
                "GNOS_UX_LAB_KEY_EVENT key={:?} shift={} window_active={}",
                event.keystroke.key,
                event.keystroke.modifiers.shift,
                window.is_window_active()
            );
        }
        let action = route_scene_key(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
        );
        match action {
            SceneKeyAction::TraverseForward => window.focus_next(cx),
            SceneKeyAction::TraverseBackward => window.focus_prev(cx),
            // gpui already activates the focused clickable element on
            // Enter and Space, so the lab does not activate a second time.
            SceneKeyAction::DeferToElement | SceneKeyAction::Ignore => {}
        }
        if self.focus_diagnostics {
            let ids: Vec<String> = self
                .control_focus
                .order()
                .iter()
                .map(|id| id.0.to_string())
                .collect();
            let handles: Vec<FocusHandle> = ids
                .iter()
                .filter_map(|id| {
                    self.control_focus
                        .handle(&ProductControlId(id.clone().into()))
                        .cloned()
                })
                .collect();
            let focused = ids
                .iter()
                .zip(handles.iter())
                .find(|(_, h)| h.is_focused(window))
                .map(|(id, _)| id.clone());
            println!(
                "GNOS_UX_LAB_TRAVERSE_RESULT action={action:?} focused_after={focused:?} known_controls={}",
                ids.len()
            );
        }
    }

    /// Keep the lab's focus claim in step with the coordinator, exactly as
    /// every product host does.
    fn arbitrate_focus(&mut self, controls: &[(String, FocusHandle)], window: &mut Window) {
        let local = controls
            .iter()
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(id, _)| id.clone());
        match local {
            Some(id) => {
                self.focus_coordinator
                    .claim(ProductHostId(1), ProductControlId(id.into()));
            }
            None => self.focus_coordinator.release(ProductHostId(1)),
        }
    }

    /// Report GPUI's own focus ownership, independent of any accessibility bus.
    ///
    /// The value comes from `FocusHandle::is_focused(window)`, so it observes
    /// the framework's focus state directly rather than what AccessKit chose
    /// to publish. Only transitions are printed, which keeps the diagnostic
    /// bounded without hiding any state change.
    fn report_focus(&mut self, controls: &[(String, FocusHandle)], window: &Window) {
        if !self.focus_diagnostics {
            return;
        }
        for (identity, handle) in controls {
            let focused = handle.is_focused(window);
            let previous = self.focus_report.get(identity).copied();
            if previous == Some(focused) {
                continue;
            }
            self.focus_report.insert(identity.clone(), focused);
            println!("GNOS_UX_LAB_FOCUS_ORACLE id={identity} focused={focused}");
        }
    }
}

impl LabView {
    fn new(
        fixture: ProductLabFixture,
        dump_a11y_tree: bool,
        dump_appstack_geometry: bool,
        surface_only: bool,
        focus_diagnostics: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        // The window's own focus sink makes the surface focusable; it is
        // not a control, so it must not be a tab stop. Marking it as one
        // puts a non-control at the front of the traversal order, and
        // Tab lands on it instead of on the first product control.
        let focus_handle = cx.focus_handle();
        if dump_a11y_tree {
            let initial_delay_ms = env::var("GNOS_UX_LAB_A11Y_DELAY_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(24);
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(initial_delay_ms))
                    .await;
                for _ in 0..5 {
                    let Ok(done) = this.update(cx, |view, cx| {
                        if view.a11y_dumped {
                            true
                        } else {
                            view.a11y_redraw_requests = view.a11y_redraw_requests.saturating_add(1);
                            cx.notify();
                            false
                        }
                    }) else {
                        break;
                    };
                    if done {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(24))
                        .await;
                }
            })
            .detach();
        }

        Self {
            fixture,
            last_interaction: "aucune interaction".to_owned(),
            dump_a11y_tree,
            dump_appstack_geometry,
            surface_only,
            appstack_geometry_dumped: false,
            a11y_redraw_requests: 0,
            a11y_dumped: false,
            focus_handle,
            control_focus: ProductFocusState::new(),
            focus_coordinator: ProductFocusCoordinator::new(),
            launch_runtime: launch_runtime(),
            focus_diagnostics,
            focus_report: std::collections::BTreeMap::new(),
        }
    }

    fn render_output(
        &self,
        output_index: usize,
        output: &ProductLabOutput,
        design: ProductDesignContext,
        focus: &[FocusHandle],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let snapshot = ProductDiagnosticSnapshot::from_runtime(&output.runtime);
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(text(output.name.clone(), ProductTextStyle::Title, design))
            .child(badge(
                format!(
                    "gen {}",
                    snapshot
                        .generation
                        .map_or_else(|| "—".to_owned(), |generation| generation.to_string())
                ),
                ProductStatusTone::Neutral,
                design,
            ));

        let semantic_state = match snapshot.semantic_state {
            ProductDiagnosticSemanticState::Unavailable => Some(state_presentation(
                format!("lab-output-{output_index}-unavailable"),
                ProductStateKind::Unavailable,
                "Aucune projection sémantique disponible",
                design,
            )),
            ProductDiagnosticSemanticState::Invalid => Some(state_presentation(
                format!("lab-output-{output_index}-invalid"),
                ProductStateKind::Error,
                "Projection sémantique invalide",
                design,
            )),
            ProductDiagnosticSemanticState::Current => None,
        };

        let workspace_children = output
            .runtime
            .workspace_manager_targets()
            .iter()
            .enumerate()
            .map(|(index, workspace)| {
                let label = workspace.label.clone();
                let handle = focus.get(index);
                let product_focus = match handle {
                    Some(handle) => ProductControlFocus::new(handle, index as isize),
                    None => ProductControlFocus::inert(),
                };
                workspace_item(
                    format!("lab-output-{output_index}-workspace-{index}"),
                    label.clone(),
                    output.runtime.workspace_manager_is_active(workspace),
                    design,
                    &product_focus,
                )
                .mx(px(ProductSpacing::XXS))
                .on_click(cx.listener(move |this, _, _, cx| {
                    println!("GNOS_UX_LAB_INTERACTION kind=workspace label={label:?}");
                    this.last_interaction = format!("workspace {label}");
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();

        let workspace_section = panel()
            .flex()
            .flex_col()
            .gap(px(ProductSpacing::XS))
            .child(text("Workspace Manager", ProductTextStyle::Label, design))
            .child(if workspace_children.is_empty() {
                state_presentation(
                    format!("lab-output-{output_index}-workspace-empty"),
                    ProductStateKind::Empty,
                    "Aucun espace de travail",
                    design,
                )
                .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .children(workspace_children)
                    .into_any_element()
            });

        let app_children = output
            .runtime
            .application_targets()
            .iter()
            .enumerate()
            .map(|(index, app)| {
                let title = app.title.clone();
                let workspace_count = output.runtime.workspace_manager_targets().len();
                let handle = focus.get(workspace_count + index);
                let product_focus = match handle {
                    Some(handle) => {
                        ProductControlFocus::new(handle, (workspace_count + index) as isize)
                    }
                    None => ProductControlFocus::inert(),
                };
                application_item(
                    format!("lab-output-{output_index}-app-{index}"),
                    title.clone(),
                    app.class.clone(),
                    output.runtime.application_is_focused(app),
                    app.visibility == gnosix_desktop_model::SurfaceVisibility::Visible,
                    design,
                    &product_focus,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    println!("GNOS_UX_LAB_INTERACTION kind=application title={title:?}");
                    this.last_interaction = format!("application {title}");
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();

        let apps = if app_children.is_empty() {
            state_presentation(
                format!("lab-output-{output_index}-apps-empty"),
                ProductStateKind::Empty,
                "Aucune application observée",
                design,
            )
            .into_any_element()
        } else {
            list_container(format!("lab-output-{output_index}-apps"))
                .max_h(px(190.0))
                .children(app_children)
                .into_any_element()
        };

        let (action_label, action_tone) = match output.runtime.action_state() {
            ProductActionState::Idle => ("idle".to_owned(), ProductStatusTone::Neutral),
            ProductActionState::Pending(record) => (
                format!("{:?} · en cours", record.kind),
                ProductStatusTone::Neutral,
            ),
            ProductActionState::Terminal(record)
                if record
                    .terminal
                    .is_some_and(ProductActionTerminal::is_success) =>
            {
                (
                    record.terminal.map_or_else(
                        || "succès".to_owned(),
                        |terminal| terminal.as_str().to_owned(),
                    ),
                    ProductStatusTone::Accent,
                )
            }
            ProductActionState::Terminal(record)
                if record.terminal == Some(ProductActionTerminal::BackendRejected) =>
            {
                ("BackendRejected".to_owned(), ProductStatusTone::Danger)
            }
            ProductActionState::Terminal(record) => (
                record.terminal.map_or_else(
                    || "refusé".to_owned(),
                    |terminal| terminal.as_str().to_owned(),
                ),
                ProductStatusTone::Warning,
            ),
        };

        let diagnostic_lines = snapshot
            .compact_text()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        panel()
            .flex()
            .flex_col()
            .gap(px(ProductSpacing::SM))
            .min_w(px(0.0))
            .child(header)
            .when_some(semantic_state, |panel, semantic_state| {
                panel.child(semantic_state)
            })
            .child(workspace_section)
            .child(
                panel()
                    .flex()
                    .flex_col()
                    .gap(px(ProductSpacing::XS))
                    .child(text("Applications", ProductTextStyle::Label, design))
                    .child(apps),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(ProductSpacing::XS))
                    .child(text("Action", ProductTextStyle::Caption, design))
                    .child(badge(action_label, action_tone, design)),
            )
            .child({
                let tray_presentation = BarTrayPresentation::derive(
                    output.runtime.tray(),
                    output.runtime.preferences(),
                );
                let tray_content = if !tray_presentation.available {
                    state_presentation(
                        format!("lab-output-{output_index}-tray-unavail"),
                        ProductStateKind::Empty,
                        "Plateau indisponible",
                        design,
                    )
                    .into_any_element()
                } else if tray_presentation.is_empty() {
                    state_presentation(
                        format!("lab-output-{output_index}-tray-empty"),
                        ProductStateKind::Empty,
                        "Plateau vide (0 élément)",
                        design,
                    )
                    .into_any_element()
                } else {
                    render_tray(
                        &tray_presentation,
                        BarOrientation::Horizontal,
                        14.0 * design.text_scale(),
                        None,
                    )
                    .into_any_element()
                };
                panel()
                    .flex()
                    .flex_col()
                    .gap(px(ProductSpacing::XS))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(text("Plateau système", ProductTextStyle::Label, design))
                            .child(badge(
                                if tray_presentation.compact {
                                    "compact"
                                } else {
                                    "étendu"
                                },
                                ProductStatusTone::Neutral,
                                design,
                            )),
                    )
                    .child(tray_content)
            })
            .child({
                let audio = output.runtime.audio_presentation();
                let osd = output
                    .audio_osd
                    .as_ref()
                    .and_then(|state| {
                        gnos_ux_gpui::product_audio_osd::render_audio_osd(state.clone(), design)
                    })
                    .map(|surface| surface.mt(px(ProductSpacing::XXS)));
                let network = output.runtime.network_presentation();
                panel()
                    .flex()
                    .flex_col()
                    .gap(px(ProductSpacing::XS))
                    .child(text("Audio", ProductTextStyle::Label, design))
                    .child(render_audio_status(
                        &audio,
                        BarOrientation::Horizontal,
                        14.0 * design.text_scale(),
                    ))
                    .when_some(osd, |panel, surface| panel.child(surface))
                    .child(
                        panel()
                            .flex()
                            .flex_col()
                            .gap(px(ProductSpacing::XXS))
                            .child(text("Réseau", ProductTextStyle::Label, design))
                            .child(gnos_ux_gpui::product_bar::render_network_status(
                                &network,
                                BarOrientation::Horizontal,
                                14.0 * design.text_scale(),
                            )),
                    )
            })
            .child(
                panel()
                    .flex()
                    .flex_col()
                    .gap(px(ProductSpacing::XXS))
                    .children(
                        diagnostic_lines
                            .into_iter()
                            .map(|line| text(line, ProductTextStyle::Caption, design)),
                    ),
            )
            .into_any_element()
    }

    fn primitive_gallery(
        &self,
        design: ProductDesignContext,
        focus: &[FocusHandle],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tooltip_design = design;
        let default_focus = gallery_focus(focus, 0);
        let default_button = button(
            "lab-button-default",
            "Action",
            "Action de démonstration",
            ProductControlState::Default,
            design,
            &default_focus,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.last_interaction = "bouton Action".to_owned();
            cx.notify();
        }));

        let tooltip_focus = gallery_focus(focus, 1);
        let tooltip_button = icon_button(
            "lab-icon-button",
            "?",
            "Afficher l’aide du toolkit",
            ProductControlState::Default,
            design,
            &tooltip_focus,
        )
        .tooltip(move |_, cx| {
            cx.new(|_| {
                ProductTooltip::new(
                    "Tooltip rendu par la primitive de production",
                    tooltip_design,
                )
            })
            .into()
        });

        panel()
            .flex()
            .flex_col()
            .gap(px(ProductSpacing::SM))
            .child(text(
                "Galerie de primitives",
                ProductTextStyle::Title,
                design,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(ProductSpacing::XS))
                    .child(default_button)
                    .child(button(
                        "lab-button-selected",
                        "Sélectionné",
                        "État sélectionné",
                        ProductControlState::Selected,
                        design,
                        &gallery_focus(focus, 2),
                    ))
                    .child(button(
                        "lab-button-disabled",
                        "Désactivé",
                        "État désactivé",
                        ProductControlState::Disabled,
                        design,
                        &ProductControlFocus::inert(),
                    ))
                    .child(tooltip_button),
            )
            .child(separator())
            .child(notification_card(
                "lab-notification-long",
                "Notification qui doit rester entièrement lisible",
                "Cette notification volontairement longue vérifie que le corps se replie sur plusieurs lignes et que la carte grandit verticalement au lieu de couper le texte. À 2×, aucune ligne ne doit disparaître derrière une hauteur fixe; si la galerie dépasse le viewport, c’est le conteneur parent qui doit défiler.",
                ProductStatusTone::Warning,
                design,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(ProductSpacing::XS))
                    .child(badge("neutre", ProductStatusTone::Neutral, design))
                    .child(badge("accent", ProductStatusTone::Accent, design))
                    .child(badge("attention", ProductStatusTone::Warning, design))
                    .child(badge("erreur", ProductStatusTone::Danger, design))
                    .child(
                        div()
                            .px(px(ProductSpacing::XS))
                            .py(px(ProductSpacing::XXS))
                            .border_1()
                            .border_color(ProductPalette::focus_ring())
                            .child(text("focus token", ProductTextStyle::Caption, design)),
                    ),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(ProductSpacing::XS))
                    .child(state_presentation(
                        "lab-state-unavailable",
                        ProductStateKind::Unavailable,
                        "Service indisponible",
                        design,
                    ))
                    .child(state_presentation(
                        "lab-state-error",
                        ProductStateKind::Error,
                        "Échec vérifiable",
                        design,
                    )),
            )
            .child(text(
                format!("Dernière interaction : {}", self.last_interaction),
                ProductTextStyle::Caption,
                design,
            ))
    }
}

impl Render for LabView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.dump_a11y_tree
            && !self.a11y_dumped
            && self.a11y_redraw_requests > 0
            && let Some(tree) = window.debug_a11y_tree_json()
        {
            println!("GNOS_UX_LAB_A11Y fixture={} tree={tree}", self.fixture.name);
            self.a11y_dumped = true;
        }

        if self.dump_appstack_geometry && !self.appstack_geometry_dumped {
            // Resolved for the lab window's own size, which is the surface the
            // stage below is built at, so the numbers and the pixels describe
            // the same geometry.
            // The live window viewport, not the lab's requested size. The
            // compositor is free to grant less than was asked for - on this
            // host the AppStack monitor is rotated to portrait and clamps the
            // window - and a surface resolved for a size it is not being shown
            // at is a picture of a surface nobody will ever see.
            let stage = window.viewport_size();
            // The fixture's own runtime, so the row count in the dump is the
            // fixture's content and not whatever this host happens to have
            // installed. A dump that varies by machine is not evidence.
            let content_runtime = self
                .fixture
                .outputs
                .first()
                .map(|output| &output.runtime)
                .unwrap_or(&self.launch_runtime);
            if let Some(surface) = lab_appstack_surface::resolve_surface(stage, content_runtime) {
                if let Some(data) = lab_appstack_surface::measurement_json(&surface) {
                    println!(
                        "GNOS_UX_LAB_APPSTACK_MEASUREMENT fixture={} data={data}",
                        self.fixture.name
                    );
                }
                println!(
                    "GNOS_UX_LAB_APPSTACK_GEOMETRY fixture={} surface={}x{} rows={} {}",
                    self.fixture.name,
                    f32::from(stage.width),
                    f32::from(stage.height),
                    surface.rows.len(),
                    surface.description
                );
            }
            self.appstack_geometry_dumped = true;
        }

        let design = self.fixture.design;
        let focus_controls = self.sync_control_focus(cx);
        self.arbitrate_focus(&focus_controls, window);
        self.report_focus(&focus_controls, window);
        let focus_handles: Vec<FocusHandle> =
            focus_controls.iter().map(|(_, h)| h.clone()).collect();
        let output_count = self.fixture.outputs.len();
        let handles = focus_handles.clone();
        let workspace_base = 0usize;
        let mut application_base = 0usize;
        // Gallery controls are appended after every per-output control.
        let per_output_control_count = {
            let mut n = 0;
            for output in self.fixture.outputs.iter() {
                n += output.runtime.workspace_manager_targets().len();
                n += output.runtime.application_targets().len();
            }
            n
        };
        let gallery_base = per_output_control_count;
        let output_grid = div()
            .id("lab-output-grid")
            .grid()
            .grid_cols(if output_count > 1 { 2 } else { 1 })
            .gap(px(ProductSpacing::MD))
            .children(
                self.fixture
                    .outputs
                    .iter()
                    .enumerate()
                    .map(|(index, output)| {
                        let workspace_count = output.runtime.workspace_manager_targets().len();
                        let application_count = output.runtime.application_targets().len();
                        let start = workspace_base + application_base;
                        application_base += workspace_count + application_count;
                        self.render_output(
                            index,
                            output,
                            design,
                            &handles[start..start + workspace_count + application_count],
                            cx,
                        )
                    }),
            );
        let _ = gallery_base;

        // The AppStack, built from this machine's real
        // application catalog. Built inline rather than through a helper
        // because the element holds listeners minted from this context, and a
        // helper returning a value that borrows it would conflict with the
        // root's own listener below.
        // Content comes from the fixture's own runtime when it has one, so a
        // capture is reproducible on any host. The geometry is identical either
        // way - it is derived from the Dock, not from the row count - but a
        // screenshot of a moving catalog is not evidence anyone can re-check.
        let content_runtime = self
            .fixture
            .outputs
            .first()
            .map(|output| &output.runtime)
            .unwrap_or(&self.launch_runtime);
        let launch_state = if matches!(self.launch_runtime.launch_state(), ProductLaunchState::Idle)
        {
            content_runtime.launch_state()
        } else {
            self.launch_runtime.launch_state()
        };
        let launch_feedback = gnos_ux_gpui::product_appstack::launch_feedback(launch_state);
        let launch_installed = content_runtime.installed_applications();
        let launch_label = format!("Installed applications ({})", launch_installed.len());
        // The AppStack, rendered as production renders it: the Dock production
        // resolves, the derived body grown upward from it by the same solver,
        // and the connective hull painted by the same path builder. The
        // component below composes the surface and the rows; it computes no
        // geometry of its own.
        // The same live viewport the dump resolved for, so the numbers and the
        // pixels describe one surface at one size.
        let stage_size = window.viewport_size();
        let appstack_surface = lab_appstack_surface::resolve_surface(stage_size, content_runtime);
        let mut launch_section = panel()
            .flex()
            .flex_col()
            .gap(px(ProductSpacing::XS))
            .child(text("AppStack", ProductTextStyle::Label, design))
            .child(text(launch_label, ProductTextStyle::Caption, design));
        if let Some(surface) = appstack_surface.as_ref() {
            launch_section = launch_section
                .child(
                    div()
                        .id("lab-appstack-stage")
                        .relative()
                        .flex_shrink_0()
                        .child(lab_appstack_surface::render_rows(
                            surface,
                            design,
                            launch_feedback.as_ref(),
                            cx.listener(|this: &mut LabView, entry, _window, cx| {
                                // The same content the rows were built from. A
                                // launch looked up in a different catalog would
                                // not find a synthetic row at all.
                                let content_runtime = this
                                    .fixture
                                    .outputs
                                    .first()
                                    .map(|output| &output.runtime)
                                    .unwrap_or(&this.launch_runtime);
                                let Some(found) =
                                    content_runtime.installed_applications().into_iter().find(
                                        |candidate| candidate.reference.entry().as_str() == entry,
                                    )
                                else {
                                    return;
                                };
                                let record =
                                    launch_lab_application(&mut this.launch_runtime, &found);
                                println!(
                                    "GNOS_UX_LAB_APPSTACK entry={} outcome={} detail={}",
                                    record.entry.as_str(),
                                    record
                                        .terminal
                                        .as_ref()
                                        .map(|terminal| terminal.code())
                                        .unwrap_or("NONE"),
                                    record.detail
                                );
                                cx.notify();
                            }),
                        )),
                )
                .child(text(
                    surface.description.clone(),
                    ProductTextStyle::Caption,
                    design,
                ));
        }
        // Surface-only: the AppStack surface and nothing else, filling the
        // window. The geometry path is identical to the gallery's - the same
        // `resolve_surface`, the same `render`, the same rows - so what is
        // reviewed here is what production renders, with the gallery's own
        // panels removed so the capture can also be measured.
        if self.surface_only
            && let Some(surface) = appstack_surface.as_ref()
        {
            return div()
                .id("lab-root")
                .role(gpui::Role::Application)
                .aria_label("GNOS.UX component lab")
                .track_focus(&self.focus_handle)
                .size_full()
                .relative()
                .bg(ProductPalette::canvas())
                .child(
                    div()
                        .id("lab-appstack-stage")
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(stage_size.width)
                        .h(stage_size.height)
                        .relative()
                        .child(lab_appstack_surface::render(surface, self.surface_only))
                        .child(lab_appstack_surface::render_rows(
                            surface,
                            design,
                            launch_feedback.as_ref(),
                            |entry, _window, _app| {
                                println!("GNOS_UX_LAB_APPSTACK_SURFACE_ONLY entry={entry}");
                            },
                        )),
                );
        }

        div()
            .id("lab-root")
            .role(gpui::Role::Application)
            .aria_label("GNOS.UX component lab")
            .track_focus(&self.focus_handle)
            // One keyboard routing point for the whole lab, matching the
            // product scene, so lab qualification measures the shipped
            // policy rather than a lab-only imitation.
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_scene_key(event, window, cx);
            }))
            .size_full()
            .relative()
            .bg(ProductPalette::canvas())
            .text_color(ProductPalette::text_primary())
            .p(px(ProductSpacing::LG))
            .flex()
            .flex_col()
            .gap(px(ProductSpacing::MD))
            // The surface overlay is a direct child of the root, not a node in
            // the scrolling column.
            //
            // The canvas builds its paths from `bounds.origin`, so the surface's
            // geometry lands in window pixels only when the stage starts at the
            // root's origin. Nested in the column, an absolute `top_0` resolves
            // against the scroll content, which starts below the preceding
            // sections - a capture then shows the hull 200px below where the
            // geometry says it is, and reviewing it would be reviewing a
            // picture that does not correspond to the surface.
            .child(if let Some(surface) = appstack_surface.as_ref() {
                div()
                    .id("lab-appstack-stage")
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(stage_size.width)
                    .h(stage_size.height)
                    .relative()
                    .child(lab_appstack_surface::render(surface, self.surface_only))
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(launch_section)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(ProductSpacing::XXS))
                            .child(text(
                                format!("GNOS.UX Lab · {}", self.fixture.name),
                                ProductTextStyle::Title,
                                design,
                            ))
                            .child(text(
                                self.fixture.description,
                                ProductTextStyle::Body,
                                design,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(ProductSpacing::XS))
                            .child(badge(
                                format!("{:.2}×", design.text_scale()),
                                ProductStatusTone::Neutral,
                                design,
                            ))
                            .child(badge(
                                if design.reduced_motion() {
                                    "reduced motion"
                                } else {
                                    "normal motion"
                                },
                                if design.reduced_motion() {
                                    ProductStatusTone::Warning
                                } else {
                                    ProductStatusTone::Neutral
                                },
                                design,
                            )),
                    ),
            )
            .child(separator())
            .child(
                list_container("lab-scroll")
                    .flex_1()
                    .gap(px(ProductSpacing::MD))
                    .when(self.fixture.name == "notification-long-2x", |list| {
                        list.child(notification_card(
                            "lab-notification-stress",
                            "Une notification extrêmement importante avec un titre volontairement très long",
                            "Le contenu complet doit rester visible même lorsque le texte est rendu à deux fois sa taille normale. Cette phrase ajoute suffisamment de matière pour provoquer plusieurs lignes et vérifier que la carte augmente sa hauteur intrinsèque au lieu de tronquer le message. Les actions et indicateurs restent séparés du corps; si le contenu total dépasse le viewport, le défilement appartient au conteneur parent et non à la notification.",
                            ProductStatusTone::Warning,
                            design,
                        ))
                    })
                    .child(output_grid)
                    .child(self.primitive_gallery(
                        design,
                        &focus_handles[gallery_base..],
                        cx,
                    )),
            )
    }
}

fn print_help() {
    println!(
        "gnos-ux-lab\n\n  --fixture=<name>\n  --list-fixtures\n  --dump-fixture\n  --dump-a11y-tree\n  --dump-appstack-geometry\n  --surface-only\n\nDefault fixture: workspace-hidden"
    );
}

fn dump_fixture(fixture: &ProductLabFixture) {
    println!(
        "GNOS_UX_LAB_FIXTURE name={} scale={:.2} reduced_motion={} description={:?}",
        fixture.name,
        fixture.design.text_scale(),
        fixture.design.reduced_motion(),
        fixture.description
    );
    for output in &fixture.outputs {
        println!("output={}", output.name);
        println!(
            "{}",
            ProductDiagnosticSnapshot::from_runtime(&output.runtime).compact_text()
        );
        let tray_presentation =
            BarTrayPresentation::derive(output.runtime.tray(), output.runtime.preferences());
        println!(
            "tray=available:{} compact:{} items:{}",
            tray_presentation.available,
            tray_presentation.compact,
            tray_presentation.items.len()
        );
        for item in &tray_presentation.items {
            println!(
                "  item id={} title={:?} status={:?} glyph={}",
                item.id.as_str(),
                item.title,
                item.status,
                gnos_ux_gpui::product_bar::tray_item_glyph(&item.icon, &item.id, &item.title)
            );
        }
    }
}

fn main() {
    let options = match LabOptions::from_args(env::args()) {
        Ok(options) => options,
        Err(error) if error == "help" => {
            print_help();
            return;
        }
        Err(error) => {
            eprintln!("gnos-ux-lab: {error}");
            print_help();
            process::exit(2);
        }
    };

    if options.list_fixtures {
        for name in FIXTURE_NAMES {
            println!("{name}");
        }
        return;
    }

    let Some(fixture) = fixture(&options.fixture) else {
        eprintln!("gnos-ux-lab: unknown fixture {:?}", options.fixture);
        eprintln!("available: {}", FIXTURE_NAMES.join(", "));
        process::exit(2);
    };

    if options.dump_fixture {
        dump_fixture(&fixture);
        return;
    }

    let fixture_name = fixture.name;
    let scale = fixture.design.text_scale();
    let reduced_motion = fixture.design.reduced_motion();
    let dump_a11y = options.dump_a11y_tree;
    let dump_appstack_geometry = options.dump_appstack_geometry;
    let surface_only = options.surface_only;
    let size_override = options.size_override;
    let focus_diagnostics = options.focus_diagnostics;

    gpui_platform::application().run(move |cx: &mut App| {
        let (want_w, want_h) = size_override.unwrap_or((LAB_WIDTH, LAB_HEIGHT));
        let bounds = Bounds::centered(None, size(px(want_w), px(want_h)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("gnos.ux.lab".to_owned()),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|cx| {
                    LabView::new(
                        fixture,
                        dump_a11y,
                        dump_appstack_geometry,
                        surface_only,
                        focus_diagnostics,
                        cx,
                    )
                })
            },
        )
        .expect("opening GNOS.UX component lab");

        println!(
            "GNOS_UX_LAB_READY fixture={} width={} height={} scale={:.2} reduced_motion={} live_desktop_observer=false compositor_mutation=false",
            fixture_name,
            want_w as u32,
            want_h as u32,
            scale,
            reduced_motion,
        );
    });
}

/// A product runtime holding a real discovered application catalog.
///
/// The AppStack section must resolve through the shipped catalog path, so a
/// launch in the lab is a genuine resolution against this machine's installed
/// entries rather than a fixture invented for the lab. When discovery cannot
/// run the runtime is simply empty and the section says so.
fn launch_runtime() -> ProductRuntime {
    let mut runtime = ProductRuntime::default();
    let Ok(roots) = ApplicationCatalogRoots::from_environment() else {
        return runtime;
    };
    let environment = DesktopEnvironment::from_environment();
    let catalog =
        ApplicationCatalog::discover(&roots, &LocalePreferences::from_environment(), environment);
    runtime.set_application_catalog(Arc::new(catalog));
    runtime
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_select_deterministic_fixture() {
        let options = LabOptions::from_args(["gnos-ux-lab".to_owned()]).unwrap();
        assert_eq!(options.fixture, "workspace-hidden");
        assert!(!options.dump_fixture);
        assert!(!options.dump_a11y_tree);
    }

    #[test]
    fn options_accept_fixture_dump_and_a11y() {
        let options = LabOptions::from_args([
            "gnos-ux-lab".to_owned(),
            "--fixture=apps-duplicates".to_owned(),
            "--dump-fixture".to_owned(),
            "--dump-a11y-tree".to_owned(),
        ])
        .unwrap();
        assert_eq!(options.fixture, "apps-duplicates");
        assert!(options.dump_fixture);
        assert!(options.dump_a11y_tree);
    }

    #[test]
    fn unknown_arguments_fail_closed() {
        assert!(
            LabOptions::from_args([
                "gnos-ux-lab".to_owned(),
                "--observe-live-desktop".to_owned(),
            ])
            .is_err()
        );
    }
}
