//! Bounded Bar-edge Settings projection.
//!
//! This control can present a validated read projection and retain a typed
//! pending intent. With the production client feature, an authenticated R12
//! host can commit it; Applied requires a receipt and matching fresh read.

use gpui::{Context, FocusHandle, IntoElement, Render, Window, div, prelude::*, px};
use in_gnu_os_settings_core::{
    BarEdgeMutationAvailability, BarEdgeSettingProjection, BarEdgeSettingsControl,
    PendingBarEdgeMutation,
};

use crate::{
    product_design::ProductPalette,
    product_preferences::{BarEdgePreference, EffectiveProductPreferences},
};
#[cfg(feature = "r12-production-client")]
use in_gnu_os_settings_read_model::{
    BarPreferenceOverrides, WorkspaceDisplayCount, WorkspaceDisplayCountError, WorkspaceDisplayMode,
};

#[cfg(feature = "r12-production-client")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarPreferenceEdit {
    Persistent(Option<bool>),
    ShowOnHover(Option<bool>),
    WorkspaceCount(Option<i64>),
    WorkspaceDisplay(Option<WorkspaceDisplayMode>),
    TrayCompact(Option<bool>),
}

#[cfg(feature = "r12-production-client")]
fn apply_preference_edit(
    mut current: BarPreferenceOverrides,
    edit: BarPreferenceEdit,
) -> Result<BarPreferenceOverrides, WorkspaceDisplayCountError> {
    match edit {
        BarPreferenceEdit::Persistent(value) => current.persistent = value,
        BarPreferenceEdit::ShowOnHover(value) => current.show_on_hover = value,
        BarPreferenceEdit::WorkspaceCount(value) => {
            current.workspace_count = value.map(WorkspaceDisplayCount::try_from).transpose()?;
        }
        BarPreferenceEdit::WorkspaceDisplay(value) => current.workspace_display = value,
        BarPreferenceEdit::TrayCompact(value) => current.tray_compact = value,
    }
    Ok(current)
}

pub struct BarEdgeSettingsView {
    control: BarEdgeSettingsControl,
    focus: [FocusHandle; 4],
    #[cfg(feature = "r12-production-client")]
    refresh_focus: FocusHandle,
    #[cfg(feature = "r12-production-client")]
    preference_focus: [FocusHandle; 34],
    #[cfg(feature = "r12-production-client")]
    session: Option<gpui::Entity<crate::product_profile::ProfileSession>>,
    #[cfg(feature = "r12-production-client")]
    subscription: Option<gpui::Subscription>,
}

impl BarEdgeSettingsView {
    pub fn new(projection: BarEdgeSettingProjection, cx: &mut Context<Self>) -> Self {
        Self {
            control: BarEdgeSettingsControl::new(projection),
            #[cfg(feature = "r12-production-client")]
            session: None,
            #[cfg(feature = "r12-production-client")]
            subscription: None,
            #[cfg(feature = "r12-production-client")]
            refresh_focus: crate::product_focus::traversable_focus_handle(cx),
            #[cfg(feature = "r12-production-client")]
            preference_focus: std::array::from_fn(|_| {
                crate::product_focus::traversable_focus_handle(cx)
            }),
            focus: std::array::from_fn(|_| crate::product_focus::traversable_focus_handle(cx)),
        }
    }

    pub fn from_effective_preferences(
        preferences: &EffectiveProductPreferences,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(
            crate::product_preferences::bar_edge_settings_projection(preferences),
            cx,
        )
    }

    #[cfg(feature = "r12-production-client")]
    pub fn production(cx: &mut Context<Self>) -> Self {
        let session = crate::product_profile::ProfileSession::shared(cx);
        let mut view =
            Self::from_effective_preferences(&EffectiveProductPreferences::default(), cx);
        view.subscription = Some(cx.observe(&session, |_, _, cx| cx.notify()));
        view.session = Some(session);
        view
    }

    fn choose(&mut self, edge: BarEdgePreference, cx: &mut Context<Self>) {
        #[cfg(feature = "r12-production-client")]
        if let Some(session) = &self.session {
            session.update(cx, |session, cx| session.select(edge, cx));
            return;
        }
        self.control.select(edge);
        cx.notify();
    }

    #[cfg(feature = "r12-production-client")]
    fn choose_preference(&mut self, edit: BarPreferenceEdit, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let current = session
            .read(cx)
            .snapshot
            .as_ref()
            .map_or_else(BarPreferenceOverrides::default, |snapshot| {
                snapshot.preferences.clone()
            });
        let Ok(preferences) = apply_preference_edit(current, edit) else {
            return;
        };
        session.update(cx, |session, cx| {
            session.select_preferences(preferences, cx)
        });
    }

    pub fn focus_first(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus[0].focus(window, cx);
    }

    pub fn pending_intent(&self) -> Option<PendingBarEdgeMutation> {
        self.control.pending()
    }
}

impl Render for BarEdgeSettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let projection = self.control.projection();
        let pending = self.control.pending();
        let status = match self.control.mutation_availability() {
            BarEdgeMutationAvailability::Unavailable if pending.is_some() => {
                "Choix en attente. Non enregistré : l’autorité d’écriture R12-B est indisponible."
            }
            BarEdgeMutationAvailability::Unavailable => {
                "Écriture durable indisponible dans cette session."
            }
        };
        #[cfg(feature = "r12-production-client")]
        let (projection, pending, status, overrides) = if let Some(session) = &self.session {
            let session = session.read(cx);
            let mut effective = EffectiveProductPreferences::default();
            crate::product_profile::apply_bar_projection(&mut effective, session.snapshot.as_ref());
            let status = match session.status {
                crate::product_profile::ProfileStatus::Loading => "Lecture du profil…",
                crate::product_profile::ProfileStatus::Current => "Profil actuel vérifié.",
                crate::product_profile::ProfileStatus::Pending => "Enregistrement en cours…",
                crate::product_profile::ProfileStatus::Applied => "Enregistré et relu.",
                crate::product_profile::ProfileStatus::Conflict => {
                    "Conflit : profil relu. Sélectionnez à nouveau si nécessaire."
                }
                crate::product_profile::ProfileStatus::Unavailable => {
                    "Autorité indisponible. Aucun succès présumé."
                }
                crate::product_profile::ProfileStatus::Failed => {
                    "Échec. Vérifiez le profil avant une nouvelle sélection."
                }
                crate::product_profile::ProfileStatus::CommittedUnconfirmed => {
                    "Commit reçu, relecture non confirmée. Actualisez."
                }
                crate::product_profile::ProfileStatus::OutcomeUnknown => {
                    "Résultat inconnu. Aucun renvoi automatique. Actualisez."
                }
            };
            let pending_edge = if session.status == crate::product_profile::ProfileStatus::Pending {
                session
                    .last_request
                    .as_ref()
                    .map(|request| PendingBarEdgeMutation { edge: request.edge })
            } else {
                None
            };
            let shown_overrides =
                if session.status == crate::product_profile::ProfileStatus::Pending {
                    session.last_preferences_request.as_ref().map_or_else(
                        || effective.bar_overrides.clone(),
                        |request| request.preferences.clone(),
                    )
                } else {
                    effective.bar_overrides.clone()
                };
            (
                crate::product_preferences::bar_edge_settings_projection(&effective),
                pending_edge,
                status,
                shown_overrides,
            )
        } else {
            (
                projection,
                pending,
                status,
                BarPreferenceOverrides::default(),
            )
        };
        let effective = match (projection.effective_edge, projection.provenance) {
            (Some(edge), Some(source)) => format!(
                "Position actuelle : {} ({})",
                BarEdgeSettingsControl::accessible_name(edge),
                match source {
                    in_gnu_os_settings_core::BarEdgeSettingProvenance::Default => "par défaut",
                    in_gnu_os_settings_core::BarEdgeSettingProvenance::Explicit => "profil",
                }
            ),
            _ => "Position actuelle indisponible : profil à vérifier.".to_owned(),
        };

        let mut choices = div().flex().gap(px(8.));
        for (index, edge) in BarEdgeSettingsControl::CHOICES.into_iter().enumerate() {
            let name = BarEdgeSettingsControl::accessible_name(edge);
            let selected = pending
                .map(|intent| intent.edge)
                .or(projection.effective_edge)
                == Some(edge);
            let control = div()
                .id(format!("bar-edge-{name}"))
                .role(gpui::Role::Button)
                .aria_label(format!("Position de la barre : {name}"))
                .aria_selected(selected)
                .tab_stop(true)
                .tab_index(index as isize)
                .track_focus(&self.focus[index])
                .px(px(12.))
                .py(px(8.))
                .rounded(px(8.))
                .bg(if selected {
                    ProductPalette::control_selected()
                } else {
                    ProductPalette::control()
                })
                .text_color(gpui::white())
                .child(name)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.choose(edge, cx);
                }))
                .on_key_down(
                    cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                        match event.keystroke.key.as_str() {
                            "enter" | "space" => {
                                this.choose(edge, cx);
                                cx.stop_propagation();
                            }
                            "left" | "up" => {
                                this.focus[(index + 3) % 4].focus(window, cx);
                                cx.stop_propagation();
                            }
                            "right" | "down" => {
                                this.focus[(index + 1) % 4].focus(window, cx);
                                cx.stop_propagation();
                            }
                            _ => {}
                        }
                    }),
                );
            choices = choices.child(control);
        }

        #[cfg(feature = "r12-production-client")]
        let choices = choices.when_some(self.session.clone(), |choices, session| {
            choices.child(
                div()
                    .id("bar-edge-refresh")
                    .role(gpui::Role::Button)
                    .aria_label("Actualiser le profil")
                    .tab_stop(true)
                    .tab_index(4)
                    .track_focus(&self.refresh_focus)
                    .child("Actualiser")
                    .on_click(cx.listener({
                        let session = session.clone();
                        move |_, _, _, cx| session.update(cx, |session, cx| session.refresh(cx))
                    }))
                    .on_key_down(cx.listener(move |_, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            session.update(cx, |session, cx| session.refresh(cx));
                            cx.stop_propagation();
                        }
                    })),
            )
        });
        #[cfg(feature = "r12-production-client")]
        let preference_controls = {
            let mut root = div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child("Préférences de barre");

            for (offset, (label, edit, selected)) in [
                (
                    "Hérité",
                    BarPreferenceEdit::Persistent(None),
                    overrides.persistent.is_none(),
                ),
                (
                    "Toujours",
                    BarPreferenceEdit::Persistent(Some(true)),
                    overrides.persistent == Some(true),
                ),
                (
                    "Auto",
                    BarPreferenceEdit::Persistent(Some(false)),
                    overrides.persistent == Some(false),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                root = root.child(
                    div()
                        .id(format!("bar-persistent-{offset}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("Persistance de la barre : {label}"))
                        .aria_selected(selected)
                        .tab_stop(true)
                        .tab_index((5 + offset) as isize)
                        .track_focus(&self.preference_focus[offset])
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .bg(if selected {
                            ProductPalette::control_selected()
                        } else {
                            ProductPalette::control()
                        })
                        .child(format!("Persistante · {label}"))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.choose_preference(edit, cx)),
                        ),
                );
            }

            for (local, (label, edit, selected)) in [
                (
                    "Hérité",
                    BarPreferenceEdit::ShowOnHover(None),
                    overrides.show_on_hover.is_none(),
                ),
                (
                    "Oui",
                    BarPreferenceEdit::ShowOnHover(Some(true)),
                    overrides.show_on_hover == Some(true),
                ),
                (
                    "Non",
                    BarPreferenceEdit::ShowOnHover(Some(false)),
                    overrides.show_on_hover == Some(false),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let offset = 3 + local;
                root = root.child(
                    div()
                        .id(format!("bar-hover-{local}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("Révélation au survol : {label}"))
                        .aria_selected(selected)
                        .tab_stop(true)
                        .tab_index((5 + offset) as isize)
                        .track_focus(&self.preference_focus[offset])
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .bg(if selected {
                            ProductPalette::control_selected()
                        } else {
                            ProductPalette::control()
                        })
                        .child(format!("Survol · {label}"))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.choose_preference(edit, cx)),
                        ),
                );
            }

            let mut workspace_row = div().flex().flex_wrap().gap(px(5.)).child("Slots :");
            for raw in 0_i64..=20 {
                let (label, edit, selected) = if raw == 0 {
                    (
                        "Hérité".to_owned(),
                        BarPreferenceEdit::WorkspaceCount(None),
                        overrides.workspace_count.is_none(),
                    )
                } else {
                    let count = WorkspaceDisplayCount::try_from(raw).expect("1..=20 is valid");
                    (
                        raw.to_string(),
                        BarPreferenceEdit::WorkspaceCount(Some(raw)),
                        overrides.workspace_count == Some(count),
                    )
                };
                let offset = 6 + raw as usize;
                workspace_row = workspace_row.child(
                    div()
                        .id(format!("bar-workspace-count-{raw}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("Nombre de slots workspace : {label}"))
                        .aria_selected(selected)
                        .tab_stop(true)
                        .tab_index((5 + offset) as isize)
                        .track_focus(&self.preference_focus[offset])
                        .px(px(8.))
                        .py(px(5.))
                        .rounded(px(7.))
                        .bg(if selected {
                            ProductPalette::control_selected()
                        } else {
                            ProductPalette::control()
                        })
                        .child(label)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.choose_preference(edit, cx)),
                        ),
                );
            }
            root = root.child(workspace_row);

            for (local, (label, value)) in [
                ("Hérité", None),
                ("Formes", Some(WorkspaceDisplayMode::Shapes)),
                ("Texte", Some(WorkspaceDisplayMode::Text)),
                ("Icônes", Some(WorkspaceDisplayMode::Icons)),
            ]
            .into_iter()
            .enumerate()
            {
                let offset = 27 + local;
                let selected = overrides.workspace_display == value;
                let edit = BarPreferenceEdit::WorkspaceDisplay(value);
                root = root.child(
                    div()
                        .id(format!("bar-workspace-display-{local}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("Affichage workspace : {label}"))
                        .aria_selected(selected)
                        .tab_stop(true)
                        .tab_index((5 + offset) as isize)
                        .track_focus(&self.preference_focus[offset])
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .bg(if selected {
                            ProductPalette::control_selected()
                        } else {
                            ProductPalette::control()
                        })
                        .child(format!("Workspaces · {label}"))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.choose_preference(edit, cx)),
                        ),
                );
            }

            for (local, (label, edit, selected)) in [
                (
                    "Hérité",
                    BarPreferenceEdit::TrayCompact(None),
                    overrides.tray_compact.is_none(),
                ),
                (
                    "Compact",
                    BarPreferenceEdit::TrayCompact(Some(true)),
                    overrides.tray_compact == Some(true),
                ),
                (
                    "Étendu",
                    BarPreferenceEdit::TrayCompact(Some(false)),
                    overrides.tray_compact == Some(false),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let offset = 31 + local;
                root = root.child(
                    div()
                        .id(format!("bar-tray-compact-{local}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("Présentation du tray : {label}"))
                        .aria_selected(selected)
                        .tab_stop(true)
                        .tab_index((5 + offset) as isize)
                        .track_focus(&self.preference_focus[offset])
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .bg(if selected {
                            ProductPalette::control_selected()
                        } else {
                            ProductPalette::control()
                        })
                        .child(format!("Tray · {label}"))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.choose_preference(edit, cx)),
                        ),
                );
            }
            root
        };

        let root = div()
            .id("bar-edge-settings")
            .size_full()
            .p(px(16.))
            .bg(ProductPalette::panel())
            .text_color(ProductPalette::text_primary())
            .role(gpui::Role::Group)
            .aria_label("Préférences de la barre")
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(effective)
            .child(choices);
        #[cfg(feature = "r12-production-client")]
        let root = root.child(preference_controls);
        root.child(status).when_some(pending, |element, intent| {
            element.child(format!(
                "Intention en attente : {}",
                BarEdgeSettingsControl::accessible_name(intent.edge)
            ))
        })
    }
}

/// Typed choices for a host to construct the control with a read-only profile
/// projection. Kept public so a future Settings host does not need to inspect
/// persisted JSON or infer placement from rendered geometry.
pub const fn edge_choices() -> [BarEdgePreference; 4] {
    BarEdgeSettingsControl::CHOICES
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_choice_has_stable_focus_and_never_claims_unavailable_write() {
        let mut app = gpui::TestAppContext::single();
        let window = app.add_window(|_, cx| {
            BarEdgeSettingsView::from_effective_preferences(
                &EffectiveProductPreferences::default(),
                cx,
            )
        });
        let visual = gpui::VisualTestContext::from_window(window.into(), &app).into_mut();
        visual.run_until_parked();
        window
            .update(&mut app, |view, window, cx| view.focus_first(window, cx))
            .unwrap();
        app.simulate_keystrokes(window.into(), "right enter");
        window
            .update(&mut app, |view, window, _| {
                assert!(view.focus[1].is_focused(window));
                assert_eq!(
                    view.pending_intent(),
                    Some(PendingBarEdgeMutation {
                        edge: BarEdgePreference::Right
                    })
                );
                assert_eq!(
                    view.control.mutation_availability(),
                    BarEdgeMutationAvailability::Unavailable
                );
            })
            .unwrap();
    }
}

#[cfg(all(test, feature = "r12-production-client"))]
mod donor11_settings_tests {
    use super::*;
    use gnos_ux_profile_client::{
        BarPreferenceOverrides, WorkspaceDisplayCount, WorkspaceDisplayMode,
    };

    fn baseline() -> BarPreferenceOverrides {
        BarPreferenceOverrides {
            persistent: Some(true),
            show_on_hover: Some(false),
            workspace_count: Some(WorkspaceDisplayCount::try_from(5_i64).unwrap()),
            workspace_display: Some(WorkspaceDisplayMode::Shapes),
            tray_compact: Some(true),
            ..Default::default()
        }
    }

    #[test]
    fn one_settings_edit_preserves_the_other_four_durable_overrides() {
        let before = baseline();
        let after =
            apply_preference_edit(before.clone(), BarPreferenceEdit::ShowOnHover(Some(true)))
                .unwrap();
        assert_eq!(after.show_on_hover, Some(true));
        assert_eq!(after.persistent, before.persistent);
        assert_eq!(after.workspace_count, before.workspace_count);
        assert_eq!(after.workspace_display, before.workspace_display);
        assert_eq!(after.tray_compact, before.tray_compact);
    }

    #[test]
    fn unset_is_explicit_absence_and_invalid_workspace_count_is_refused() {
        let before = baseline();
        let after =
            apply_preference_edit(before.clone(), BarPreferenceEdit::Persistent(None)).unwrap();
        assert_eq!(after.persistent, None);
        assert_eq!(after.show_on_hover, before.show_on_hover);

        assert!(
            apply_preference_edit(before.clone(), BarPreferenceEdit::WorkspaceCount(Some(0)))
                .is_err()
        );
        assert!(
            apply_preference_edit(before.clone(), BarPreferenceEdit::WorkspaceCount(Some(21)))
                .is_err()
        );
        let cleared =
            apply_preference_edit(before.clone(), BarPreferenceEdit::WorkspaceCount(None)).unwrap();
        assert_eq!(cleared.workspace_count, None);
    }
}
