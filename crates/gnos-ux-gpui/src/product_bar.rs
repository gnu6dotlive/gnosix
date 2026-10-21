use crate::{
    product_audio::{ProductAudioEndpointState, ProductAudioPresentation},
    product_audio_action::{AudioIntent, ExactAudioEndpointRef},
    product_design::{ProductPalette, ProductRadius, ProductSpacing},
    product_network::{ProductNetworkPresentation, ProductNetworkPrimary, ProductNetworkWifi},
    product_preferences::EffectiveProductPreferences,
    product_scene::ProductSemanticState,
    product_tray::{TrayIcon, TrayItemId, TraySnapshot, TrayStatus},
    product_tray_action::{ExactTrayItemRef, TrayIntent, TrayInteractionAnchor},
};
pub use gnos_ux_surface::grammar::BarOrientation;
use gnos_ux_surface::{
    envelope::SurfaceEdge,
    grammar::{
        AlongEdgeAlignment, BarPlacement, BarReservationPolicy, BarSensingPolicy,
        ResolvedBarPlacement, ResponsiveRadiusToken, SurfaceViewport,
    },
};
use gpui::App;
use gpui::{Div, Stateful, div, prelude::*, px, rgba, white};
use std::rc::Rc;
use std::time::Duration;

const QUALIFICATION_LEG_DURATION: Duration = Duration::from_millis(900);
pub const PRODUCT_BAR_INSET: f32 = 16.0;

#[derive(Clone, Debug, PartialEq)]
pub enum BarTransitionQualification {
    Path {
        edges: Vec<SurfaceEdge>,
        leg_duration: Duration,
    },
    /// Development: a leg from `source` towards `via`, interrupted at `at`
    /// percent of it by a new target (the scene rebases it, BAR-01J).
    Retarget {
        source: SurfaceEdge,
        via: SurfaceEdge,
        target: SurfaceEdge,
        leg_duration: Duration,
        at: u8,
    },
    /// BAR-01J (B09): the leg a new target interrupted goes on for `span`,
    /// its progress slowing from `rate` (per second) to a stop, before the
    /// Bar turns: the scene then rebases it (`Rebased`).
    Decelerate {
        leg: Box<gnos_ux_surface::grammar::BarPlacementTransition>,
        progress: f32,
        rate: f32,
        span: Duration,
        rail: SurfaceEdge,
        target: SurfaceEdge,
        leg_duration: Duration,
    },
    /// BAR-01J (B09): a leg rebased from the live frame a new target
    /// interrupted, towards that target.
    Rebased {
        origin: Box<gnos_ux_surface::grammar::ResolvedBarTransitionFrame>,
        /// The edge the rail stood on when the travel was interrupted: as on
        /// any leg, it stays the rail's edge until the leg arrives.
        rail: SurfaceEdge,
        target: SurfaceEdge,
        leg_duration: Duration,
    },
}

impl BarTransitionQualification {
    /// Production Settings consumes the same qualified relocation plan and
    /// duration. This constructor exposes no development selector or geometry.
    pub fn between_edges(source: SurfaceEdge, target: SurfaceEdge) -> Self {
        Self::Path {
            edges: vec![source, target],
            leg_duration: QUALIFICATION_LEG_DURATION,
        }
    }

    /// The same leg at `tempo` times the qualified speed (BAR-01D), bounded
    /// to the profile's range.
    pub fn between_edges_at(source: SurfaceEdge, target: SurfaceEdge, tempo: f32) -> Self {
        let tempo = if tempo.is_finite() {
            tempo.clamp(0.5, 2.0)
        } else {
            1.0
        };
        Self::Path {
            edges: vec![source, target],
            leg_duration: QUALIFICATION_LEG_DURATION.div_f32(tempo),
        }
    }

    /// The edge the qualification starts from.
    pub fn first_edge(&self) -> SurfaceEdge {
        match self {
            Self::Path { edges, .. } => edges[0],
            Self::Retarget { source, .. } => *source,
            Self::Rebased { origin, .. } => origin.active_edge,
            Self::Decelerate { rail, .. } => *rail,
        }
    }

    pub fn final_target(&self) -> SurfaceEdge {
        match self {
            Self::Path { edges, .. } => *edges.last().expect("validated transition path"),
            Self::Retarget { target, .. }
            | Self::Rebased { target, .. }
            | Self::Decelerate { target, .. } => *target,
        }
    }

    pub fn total_duration(&self) -> Duration {
        match self {
            Self::Path {
                edges,
                leg_duration,
            } => *leg_duration * (edges.len() as u32 - 1),
            Self::Retarget {
                leg_duration, at, ..
            } => Self::retarget_at(*leg_duration, *at) + *leg_duration,
            Self::Rebased { leg_duration, .. } => *leg_duration,
            Self::Decelerate {
                span, leg_duration, ..
            } => *span + *leg_duration,
        }
    }

    /// When a development retarget interrupts its first leg.
    pub fn retarget_at(leg_duration: Duration, at: u8) -> Duration {
        leg_duration * u32::from(at) / 100
    }

    pub fn target_at(&self, elapsed: Duration) -> SurfaceEdge {
        match self {
            Self::Path {
                edges,
                leg_duration,
            } => {
                let leg = (elapsed.as_millis() / leg_duration.as_millis()) as usize;
                edges[leg.min(edges.len() - 1)]
            }
            Self::Retarget {
                source,
                target,
                leg_duration,
                at,
                ..
            } => {
                // As on any leg, the rail keeps its edge until it arrives;
                // the scene rebases the leg at `at` (BAR-01J).
                if elapsed < Self::retarget_at(*leg_duration, *at) + *leg_duration {
                    *source
                } else {
                    *target
                }
            }
            Self::Decelerate { rail, .. } => *rail,
            Self::Rebased {
                rail,
                target,
                leg_duration,
                ..
            } => {
                if elapsed < *leg_duration {
                    *rail
                } else {
                    *target
                }
            }
        }
    }
}

pub fn presentation_placement(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    font_px: f32,
) -> Option<BarPlacement> {
    presentation_placement_with_sensing(viewport, edge, font_px, BarSensingPolicy::None)
}

pub fn presentation_placement_with_sensing(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    font_px: f32,
    sensing_policy: BarSensingPolicy,
) -> Option<BarPlacement> {
    presentation_placement_with_policies(
        viewport,
        edge,
        font_px,
        BarReservationPolicy::ReserveOccupiedExtent,
        sensing_policy,
    )
}

pub fn presentation_placement_with_policies(
    viewport: SurfaceViewport,
    edge: SurfaceEdge,
    font_px: f32,
    reservation_policy: BarReservationPolicy,
    sensing_policy: BarSensingPolicy,
) -> Option<BarPlacement> {
    if !font_px.is_finite() || font_px <= 0. {
        return None;
    }
    let radius = ResponsiveRadiusToken::candidate_default()
        .resolve(viewport)
        .ok()?;
    let vertical = matches!(edge, SurfaceEdge::Left | SurfaceEdge::Right);
    // Twelve upright glyph cells plus the existing 20px inset on each side.
    // This is a presentation capacity, not a geometry/persistence parameter.
    let thickness = if vertical {
        (font_px * 12. * 0.6 + 40.).max(radius * 2.)
    } else {
        56.0_f32.max(radius * 2.)
    };
    let along = if vertical {
        viewport.height
    } else {
        viewport.width
    };
    Some(BarPlacement {
        edge,
        along_edge_alignment: AlongEdgeAlignment::Center,
        thickness,
        span: (along - 32.).min(760.),
        inset: PRODUCT_BAR_INSET,
        reservation_policy,
        sensing_policy,
    })
}

/// Presentation model for an individual tray item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarTrayItemPresentation {
    pub id: TrayItemId,
    pub title: String,
    pub status: TrayStatus,
    pub icon: TrayIcon,
    pub tooltip: Option<String>,
    pub needs_attention: bool,
    pub is_compact: bool,
    /// Exact live identity, so a click resolves to the observed instance
    /// rather than to whatever now occupies the same slot.
    pub target: ExactTrayItemRef,
    /// Whether the exporter declared this item to be a menu itself.
    pub item_is_menu: bool,
    /// Whether the item currently exports a menu at all.
    pub has_menu: bool,
}

/// Presentation model for the complete system tray within the Bar.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BarTrayPresentation {
    pub available: bool,
    pub compact: bool,
    pub items: Vec<BarTrayItemPresentation>,
}

impl BarTrayPresentation {
    pub fn derive(snapshot: &TraySnapshot, preferences: &EffectiveProductPreferences) -> Self {
        let compact = preferences.bar_overrides.tray_compact.unwrap_or(false);
        match snapshot {
            TraySnapshot::Unavailable => Self {
                available: false,
                compact,
                items: Vec::new(),
            },
            TraySnapshot::Active { items } => {
                let presentation_items = items
                    .iter()
                    .filter(|item| item.status != TrayStatus::Passive)
                    .map(|item| {
                        let icon = match item.attention_icon() {
                            Some(attention) if item.status == TrayStatus::NeedsAttention => {
                                attention
                            }
                            _ => item.icon(),
                        };
                        let tooltip = item
                            .tooltip
                            .as_ref()
                            .map(|t| {
                                if t.description.is_empty() {
                                    t.title.clone()
                                } else if t.title.is_empty() {
                                    t.description.clone()
                                } else {
                                    format!("{}: {}", t.title, t.description)
                                }
                            })
                            .or_else(|| (!item.title.is_empty()).then(|| item.title.clone()));

                        BarTrayItemPresentation {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            status: item.status,
                            icon,
                            tooltip,
                            needs_attention: item.status == TrayStatus::NeedsAttention,
                            is_compact: compact,
                            target: ExactTrayItemRef::from_observed(item, item.owner.clone()),
                            item_is_menu: item.item_is_menu,
                            has_menu: item.menu_path.is_some(),
                        }
                    })
                    .collect();

                Self {
                    available: true,
                    compact,
                    items: presentation_items,
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Orientation the tray renders along, derived from the bar edge the
    /// effective preferences select.
    pub fn orientation_for(preferences: &EffectiveProductPreferences) -> BarOrientation {
        match crate::product_preferences::bar_surface_edge(preferences) {
            Some(SurfaceEdge::Top) | Some(SurfaceEdge::Bottom) => BarOrientation::Horizontal,
            _ => BarOrientation::Vertical,
        }
    }

    /// What a primary interaction on one presented item means.
    ///
    /// The decision is taken here, in the presentation model, rather than in a
    /// widget: an item that declares itself a menu is opened, not activated.
    pub fn primary_intent(
        &self,
        index: usize,
        anchor: TrayInteractionAnchor,
    ) -> Option<TrayIntent> {
        let item = self.items.get(index)?;
        let intent = match crate::product_tray_action::primary_interaction_policy(
            item.item_is_menu,
            item.has_menu,
        ) {
            crate::product_tray_action::PrimaryTrayInteraction::OpenMenu => TrayIntent::OpenMenu {
                item: item.target.clone(),
                anchor,
            },
            crate::product_tray_action::PrimaryTrayInteraction::Activate => TrayIntent::Activate {
                item: item.target.clone(),
                anchor,
            },
        };
        Some(intent)
    }

    /// A secondary interaction on one presented item.
    pub fn secondary_intent(
        &self,
        index: usize,
        anchor: TrayInteractionAnchor,
    ) -> Option<TrayIntent> {
        let item = self.items.get(index)?;
        Some(TrayIntent::SecondaryActivate {
            item: item.target.clone(),
            anchor,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarAudioStatus {
    pub glyph: &'static str,
    pub text: String,
    pub aria_label: String,
    pub available: bool,
}

impl BarAudioStatus {
    pub fn derive(presentation: &ProductAudioPresentation) -> Self {
        match &presentation.output {
            ProductAudioEndpointState::ProviderUnavailable => Self {
                glyph: "◇",
                text: "—".into(),
                aria_label: "Audio indisponible".into(),
                available: false,
            },
            ProductAudioEndpointState::DefaultUnresolved { .. } => Self {
                glyph: "◇",
                text: "?".into(),
                aria_label: "Sortie audio non résolue".into(),
                available: false,
            },
            ProductAudioEndpointState::Ready(output) => {
                let text = output
                    .level
                    .map(|level| format!("{}%", (level * 100.0).round() as i32))
                    .unwrap_or_else(|| "—".into());
                let glyph = if output.muted {
                    "🔇"
                } else if output.level.is_some_and(|level| level < 0.33) {
                    "🔈"
                } else if output.level.is_some_and(|level| level < 0.66) {
                    "🔉"
                } else {
                    "🔊"
                };
                let aria_label = if output.muted {
                    format!("Audio · {} · {} · muet", output.label, text)
                } else {
                    format!("Audio · {} · {}", output.label, text)
                };
                Self {
                    glyph,
                    text,
                    aria_label,
                    available: true,
                }
            }
        }
    }
}

pub type BarAudioDispatch = Rc<dyn Fn(&mut App, AudioIntent)>;

fn emit_audio_intent(dispatch: &BarAudioDispatch, app: &mut App, intent: AudioIntent) {
    dispatch(app, intent);
}

#[derive(Clone)]
pub struct BarAudioInteraction {
    pub dispatch: BarAudioDispatch,
}

pub fn primary_audio_intent(presentation: &ProductAudioPresentation) -> Option<AudioIntent> {
    let ProductAudioEndpointState::Ready(output) = &presentation.output else {
        return None;
    };
    Some(AudioIntent::ToggleOutputMute {
        target: ExactAudioEndpointRef::output(output.id),
    })
}

pub fn render_audio_status(
    presentation: &ProductAudioPresentation,
    orientation: BarOrientation,
    font_px: f32,
) -> Stateful<Div> {
    render_audio_status_with_interaction(presentation, orientation, font_px, None)
}

pub fn render_audio_status_with_interaction(
    presentation: &ProductAudioPresentation,
    orientation: BarOrientation,
    font_px: f32,
    handler: Option<&BarAudioInteraction>,
) -> Stateful<Div> {
    let status = BarAudioStatus::derive(presentation);
    let mut view = div()
        .id("bar-audio-status")
        .role(gpui::Role::Status)
        .aria_label(status.aria_label)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(ProductSpacing::XXS))
        .text_size(px(font_px))
        .text_color(if status.available {
            ProductPalette::text_primary()
        } else {
            ProductPalette::text_muted()
        })
        .child(status.glyph)
        .child(status.text);
    if let (Some(handler), Some(intent)) = (handler, primary_audio_intent(presentation)) {
        let dispatch = handler.dispatch.clone();
        view = view
            .role(gpui::Role::Button)
            .hover(|style| style.bg(ProductPalette::control_hover()))
            .active(|style| style.bg(ProductPalette::control_pressed()))
            .rounded(px(ProductRadius::CONTROL))
            .px(px(ProductSpacing::XXS))
            .on_click(move |_event, _window, cx| {
                emit_audio_intent(&dispatch, cx, intent.clone());
            });
    }
    if orientation == BarOrientation::Vertical {
        view = view.flex_col();
    }
    view
}

/// Renders the read-only network status.
///
/// Non-interactive by construction: no `on_click`, no hover or active style, no
/// focus handle. NETWORK-01 Phase 3 presents observed truth and owns no network
/// authority, so there is nothing to click.
pub fn render_network_status(
    presentation: &ProductNetworkPresentation,
    orientation: BarOrientation,
    font_px: f32,
) -> Stateful<Div> {
    let status = BarNetworkStatus::derive(presentation);
    let mut view = div()
        .id("bar-network-status")
        .role(gpui::Role::Status)
        .aria_label(status.aria_label)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(ProductSpacing::XXS))
        .text_size(px(font_px))
        .text_color(if status.available {
            ProductPalette::text_primary()
        } else {
            ProductPalette::text_muted()
        })
        .child(status.glyph)
        .child(status.text);
    if status.overlay {
        // The overlay is never presented as the transport; it rides alongside.
        view = view.child("⊕");
    }
    if orientation == BarOrientation::Vertical {
        view = view.flex_col();
    }
    view
}

/// Read-only network status for the Bar.
///
/// Derived only from [`ProductNetworkPresentation`]. The Bar cannot see the
/// provider: no D-Bus path, no `zbus`, no numeric NetworkManager state, no
/// connection UUID and no relevance rule reach this module, which a source-level
/// test enforces. There is no interaction: NETWORK-01 Phase 3 presents observed
/// truth and owns no network authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarNetworkStatus {
    pub glyph: &'static str,
    pub text: String,
    pub aria_label: String,
    /// Whether a real transport is up. Drives emphasis only.
    pub available: bool,
    /// A meaningful overlay is up, reported separately from the transport.
    pub overlay: bool,
}

impl BarNetworkStatus {
    pub fn derive(presentation: &ProductNetworkPresentation) -> Self {
        // Provider absence is stated as absence. It is never rendered as
        // "disconnected", "Wi-Fi off" or "no internet", because none of those is
        // what the provider told us.
        match &presentation.primary {
            ProductNetworkPrimary::Unavailable => Self {
                glyph: "◇",
                text: "—".into(),
                aria_label: "Réseau indisponible".into(),
                available: false,
                overlay: false,
            },
            ProductNetworkPrimary::Disconnected => Self {
                glyph: "⌀",
                text: "hors ligne".into(),
                aria_label: "Réseau · hors ligne".into(),
                available: false,
                overlay: presentation.overlay_active,
            },
            ProductNetworkPrimary::Connecting => Self {
                glyph: "◌",
                text: "connexion".into(),
                aria_label: "Réseau · connexion en cours".into(),
                available: false,
                overlay: presentation.overlay_active,
            },
            ProductNetworkPrimary::Ready(endpoint) => {
                let (glyph, label) = match (&endpoint.wifi, endpoint.transport) {
                    (Some(wifi), _) => (wifi_glyph(wifi), wifi.ssid.clone()),
                    (None, gnos_network::NetworkTransport::Ethernet) => ("▤", "filaire".into()),
                    (None, _) => ("▤", "réseau".into()),
                };
                Self {
                    glyph,
                    text: label,
                    aria_label: Self::ready_aria(presentation, endpoint.wifi.as_ref()),
                    available: true,
                    overlay: presentation.overlay_active,
                }
            }
        }
    }

    /// The accessible label states every observed fact the glyph compresses,
    /// including the two that must never be silently merged: the link
    /// assessment, and the fact that it is not a verified internet check.
    fn ready_aria(
        presentation: &ProductNetworkPresentation,
        wifi: Option<&ProductNetworkWifi>,
    ) -> String {
        let mut parts = vec!["Réseau".to_owned()];
        if let Some(wifi) = wifi {
            parts.push(wifi.ssid.clone());
            parts.push(format!("{}%", wifi.strength));
            parts.push(wifi.band.as_str().to_owned());
            if wifi.authenticated {
                parts.push(wifi.security_word().to_owned());
            }
        } else {
            parts.push("filaire".to_owned());
        }
        match presentation.connectivity {
            gnos_network::NetworkConnectivity::Portal => parts.push("portail captif".to_owned()),
            gnos_network::NetworkConnectivity::Limited => parts.push("accès limité".to_owned()),
            gnos_network::NetworkConnectivity::None => parts.push("sans accès".to_owned()),
            _ => {}
        }
        if presentation.connectivity_checked {
            parts.push("vérifié".to_owned());
        }
        if presentation.overlay_active {
            parts.push("surcouche active".to_owned());
        }
        parts.join(" · ")
    }
}

/// Signal bands as a printable metre. Redundant with the percentage on purpose:
/// the band must be legible without reading a number.
fn wifi_glyph(wifi: &ProductNetworkWifi) -> &'static str {
    match wifi.band {
        gnos_network::NetworkSignalBand::None => "▁",
        gnos_network::NetworkSignalBand::Weak => "▂",
        gnos_network::NetworkSignalBand::Medium => "▄",
        gnos_network::NetworkSignalBand::Strong => "█",
    }
}

/// Maps a tray icon/ID to a clean semantic unicode glyph.
pub fn tray_item_glyph(icon: &TrayIcon, id: &TrayItemId, title: &str) -> &'static str {
    match icon {
        TrayIcon::Named(name) => {
            let lower = name.to_ascii_lowercase();
            if lower.contains("blue") || lower.contains("bt") {
                "ᛒ"
            } else if lower.contains("steam") {
                "♨"
            } else if lower.contains("notif") || lower.contains("bell") {
                "🔔"
            } else if lower.contains("volume") || lower.contains("audio") || lower.contains("sound")
            {
                "🔊"
            } else if lower.contains("net") || lower.contains("wifi") || lower.contains("eth") {
                "📶"
            } else if lower.contains("batt") || lower.contains("power") {
                "🔋"
            } else if lower.contains("term") || lower.contains("tty") {
                "⌨"
            } else {
                "◈"
            }
        }
        TrayIcon::Pixmap { .. } => {
            let lower_id = id.as_str().to_ascii_lowercase();
            let lower_title = title.to_ascii_lowercase();
            if lower_id.contains("tty") || lower_title.contains("tty") {
                "⌨"
            } else {
                "▣"
            }
        }
        TrayIcon::None => "◇",
    }
}

/// Render the tray pill container and its items.
///
/// When `handler` is supplied the items become interactive and emit typed
/// intents; without it the tray renders read-only.
pub fn render_tray(
    presentation: &BarTrayPresentation,
    orientation: BarOrientation,
    font_px: f32,
    handler: Option<&BarTrayInteraction>,
) -> Stateful<Div> {
    let mut container = div()
        .id("product-tray")
        .role(gpui::Role::Group)
        .aria_label("Plateau système")
        .bg(ProductPalette::panel())
        .border_1()
        .border_color(ProductPalette::border())
        .rounded(px(ProductRadius::BADGE));

    if presentation.compact {
        container = container
            .px(px(ProductSpacing::XXS))
            .py(px(ProductSpacing::XXS))
            .gap(px(ProductSpacing::XXS));
    } else {
        container = container
            .px(px(ProductSpacing::XS))
            .py(px(ProductSpacing::XXS))
            .gap(px(ProductSpacing::XS));
    }

    if orientation == BarOrientation::Horizontal {
        container = container.flex().flex_row().items_center();
    } else {
        container = container.flex().flex_col().items_center();
    }

    let children = presentation.items.iter().map(|item| {
        let glyph = tray_item_glyph(&item.icon, &item.id, &item.title);
        let mut item_elem = div()
            .id(format!("tray-item-{}", item.id.as_str()))
            .role(gpui::Role::Image)
            .aria_label(format!("Plateau · {}", item.title))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(ProductRadius::CONTROL))
            .text_color(if item.needs_attention {
                ProductPalette::warning()
            } else {
                ProductPalette::text_primary()
            });

        if item.needs_attention {
            item_elem = item_elem
                .border_1()
                .border_color(ProductPalette::warning())
                .bg(rgba(0xe7bf7222));
        } else {
            item_elem = item_elem
                .hover(|style| style.bg(ProductPalette::control_hover()))
                .active(|style| style.bg(ProductPalette::control_pressed()));
        }

        if presentation.compact {
            item_elem = item_elem
                .px(px(ProductSpacing::XXS))
                .py(px(2.0))
                .text_size(px((font_px * 0.9).max(12.0)))
                .child(glyph);
        } else {
            item_elem = item_elem
                .px(px(ProductSpacing::XS))
                .py(px(ProductSpacing::XXS))
                .gap(px(ProductSpacing::XXS))
                .text_size(px(font_px))
                .child(glyph);

            if !item.title.is_empty() && item.title.len() <= 16 {
                item_elem = item_elem.child(
                    div()
                        .text_size(px((font_px * 0.8).max(11.0)))
                        .text_color(ProductPalette::text_secondary())
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(item.title.clone()),
                );
            }
        }

        if let Some(tooltip_text) = &item.tooltip {
            let tooltip_clone = tooltip_text.clone();
            item_elem = item_elem.tooltip(move |_, cx| {
                cx.new(|_| {
                    crate::product_design::ProductTooltip::new(
                        tooltip_clone.clone(),
                        crate::product_design::ProductDesignContext::default(),
                    )
                })
                .into()
            });
        }

        // The widget emits a typed intent. It never names a bus address, a
        // method, or a screen coordinate; the anchor is derived here from the
        // click's own resolved position and the owning output's origin.
        match handler {
            Some(handler) => {
                let target = item.target.clone();
                let item_is_menu = item.item_is_menu;
                let has_menu = item.has_menu;
                let origin = handler.output_origin;
                let dispatch = handler.dispatch.clone();
                item_elem.on_click(move |event, _window, cx| {
                    let position = event.position();
                    let anchor = TrayInteractionAnchor::from_local_point(
                        origin,
                        (f32::from(position.x), f32::from(position.y)),
                    );
                    if let Some(anchor) = anchor
                        && let Some(intent) =
                            primary_intent_for(item_is_menu, has_menu, target.clone(), anchor)
                    {
                        dispatch(cx, intent);
                    }
                })
            }
            None => item_elem,
        }
    });

    container.children(children)
}

/// Resolves the typed primary intent for one clicked tray item.
///
/// Kept next to the widget so the policy is visible at the call site while
/// still living in typed code rather than in a callback body.
fn primary_intent_for(
    item_is_menu: bool,
    has_menu: bool,
    target: ExactTrayItemRef,
    anchor: TrayInteractionAnchor,
) -> Option<TrayIntent> {
    use crate::product_tray_action::{PrimaryTrayInteraction, primary_interaction_policy};
    Some(match primary_interaction_policy(item_is_menu, has_menu) {
        PrimaryTrayInteraction::OpenMenu => TrayIntent::OpenMenu {
            item: target,
            anchor,
        },
        PrimaryTrayInteraction::Activate => TrayIntent::Activate {
            item: target,
            anchor,
        },
    })
}

/// Routes one typed tray intent from a widget to the product action layer.
pub type BarTrayDispatch = Rc<dyn Fn(&mut App, TrayIntent)>;

/// Sink for tray interaction emitted by the Bar.
///
/// The scene host supplies this; the Bar itself stays free of transport and
/// product-runtime ownership.
#[derive(Clone)]
pub struct BarTrayInteraction {
    /// Logical origin of the output the Bar is rendering on.
    pub output_origin: (i32, i32),
    /// Routes one typed intent to the product action layer.
    pub dispatch: BarTrayDispatch,
}

pub fn content(state: &ProductSemanticState, placement: ResolvedBarPlacement, font_px: f32) -> Div {
    content_with_tray_and_audio_interactive(state, None, None, placement, font_px, None, None)
}

pub fn content_with_tray(
    state: &ProductSemanticState,
    tray: Option<&BarTrayPresentation>,
    placement: ResolvedBarPlacement,
    font_px: f32,
    handler: Option<&BarTrayInteraction>,
) -> Div {
    content_with_tray_and_audio_interactive(state, tray, None, placement, font_px, handler, None)
}

pub fn content_with_tray_and_audio(
    state: &ProductSemanticState,
    tray: Option<&BarTrayPresentation>,
    audio: Option<&ProductAudioPresentation>,
    placement: ResolvedBarPlacement,
    font_px: f32,
    handler: Option<&BarTrayInteraction>,
) -> Div {
    content_with_tray_and_audio_interactive(state, tray, audio, placement, font_px, handler, None)
}

pub fn content_with_tray_and_audio_interactive(
    state: &ProductSemanticState,
    tray: Option<&BarTrayPresentation>,
    audio: Option<&ProductAudioPresentation>,
    placement: ResolvedBarPlacement,
    font_px: f32,
    handler: Option<&BarTrayInteraction>,
    audio_handler: Option<&BarAudioInteraction>,
) -> Div {
    content_with_network(
        state,
        tray,
        audio,
        None,
        placement,
        font_px,
        handler,
        audio_handler,
    )
}

/// The Bar content with an optional read-only network status.
///
/// Network is additive and passive: passing `None` reproduces the previous Bar
/// exactly, so no existing caller changes behaviour.
#[allow(clippy::too_many_arguments)]
pub fn content_with_network(
    state: &ProductSemanticState,
    tray: Option<&BarTrayPresentation>,
    audio: Option<&ProductAudioPresentation>,
    network: Option<&ProductNetworkPresentation>,
    placement: ResolvedBarPlacement,
    font_px: f32,
    handler: Option<&BarTrayInteraction>,
    audio_handler: Option<&BarAudioInteraction>,
) -> Div {
    let base = div()
        .flex()
        .items_center()
        .px(px(20.))
        .text_color(white())
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(font_px));
    if placement.orientation() == BarOrientation::Horizontal {
        let label = div()
            .flex()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(state.bar_label());
        let mut status = div().flex().items_center().gap(px(ProductSpacing::XS));
        if let Some(network) = network {
            status = status.child(render_network_status(
                network,
                BarOrientation::Horizontal,
                font_px,
            ));
        }
        if let Some(audio) = audio {
            status = status.child(render_audio_status_with_interaction(
                audio,
                BarOrientation::Horizontal,
                font_px,
                audio_handler,
            ));
        }
        if let Some(tray) = tray
            && tray.available
            && !tray.is_empty()
        {
            status = status.child(render_tray(
                tray,
                BarOrientation::Horizontal,
                font_px,
                handler,
            ));
        }
        base.justify_between().child(label).child(status)
    } else {
        let groups = div().flex_col().justify_center().gap(px(12.)).children(
            state
                .bar_groups()
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    div()
                        .id(("bar-group", i))
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label)
                }),
        );
        let mut status = div().flex_col().items_center().gap(px(ProductSpacing::XS));
        if let Some(audio) = audio {
            status = status.child(render_audio_status_with_interaction(
                audio,
                BarOrientation::Vertical,
                font_px,
                audio_handler,
            ));
        }
        if let Some(tray) = tray
            && tray.available
            && !tray.is_empty()
        {
            status = status.child(render_tray(
                tray,
                BarOrientation::Vertical,
                font_px,
                handler,
            ));
        }
        base.flex_col()
            .justify_between()
            .child(groups)
            .child(status)
    }
}

/// Development-only endpoint selection. Production builds reject the channel.
pub fn qualification_edge(args: &[String]) -> Result<Option<SurfaceEdge>, String> {
    if args.iter().any(|arg| arg == "--qualify-bar-edge") {
        return Err("qualify-bar-edge requires =top|right|bottom|left".into());
    }
    let values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-edge="))
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Ok(None);
    }
    if !cfg!(feature = "component-lab") {
        return Err("qualify-bar-edge requires component-lab development build".into());
    }
    if values.len() != 1 || !args.iter().any(|arg| arg == "--product-scene") {
        return Err("one qualify-bar-edge is required in product-scene mode".into());
    }
    match values[0] {
        "top" => Ok(Some(SurfaceEdge::Top)),
        "right" => Ok(Some(SurfaceEdge::Right)),
        "bottom" => Ok(Some(SurfaceEdge::Bottom)),
        "left" => Ok(Some(SurfaceEdge::Left)),
        _ => Err("invalid qualify-bar-edge; expected top|right|bottom|left".into()),
    }
}

/// Development-only timed relocation paths and a live mid-flight retarget.
pub fn qualification_transition(
    args: &[String],
) -> Result<Option<BarTransitionQualification>, String> {
    let path_values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-transition="))
        .collect::<Vec<_>>();
    let retarget_values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-retarget="))
        .collect::<Vec<_>>();
    let duration_values = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--qualify-bar-duration-ms="))
        .collect::<Vec<_>>();
    let at = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--qualify-bar-retarget-at="))
        .map_or(Ok(50), |value| {
            value
                .parse::<u8>()
                .ok()
                .filter(|at| (5..=95).contains(at))
                .ok_or("qualify-bar-retarget-at must be a percent in 5..=95")
        })?;
    if args.iter().any(|arg| arg == "--qualify-bar-duration-ms") {
        return Err("qualify-bar-duration-ms requires an integer value".into());
    }
    if args
        .iter()
        .any(|arg| arg == "--qualify-bar-transition" || arg == "--qualify-bar-retarget")
    {
        return Err("bar transition qualification requires an =edge:path value".into());
    }
    if path_values.is_empty() && retarget_values.is_empty() {
        if !duration_values.is_empty() {
            return Err("qualify-bar-duration-ms requires a transition qualification".into());
        }
        return Ok(None);
    }
    if !cfg!(feature = "component-lab") || !args.iter().any(|arg| arg == "--product-scene") {
        return Err("qualify-bar-transition requires component-lab product-scene mode".into());
    }
    if path_values.len() + retarget_values.len() != 1 {
        return Err("select exactly one bar transition qualification mode".into());
    }
    if duration_values.len() > 1 {
        return Err("only one qualify-bar-duration-ms is accepted".into());
    }
    let leg_duration: Duration = duration_values.first().map_or(
        Ok::<Duration, &'static str>(QUALIFICATION_LEG_DURATION),
        |value| {
            let millis = value
                .parse::<u64>()
                .map_err(|_| "qualify-bar-duration-ms must be an integer")?;
            if !(900..=10_000).contains(&millis) {
                return Err("qualify-bar-duration-ms must be between 900 and 10000");
            }
            Ok(Duration::from_millis(millis))
        },
    )?;
    let parse = |value: &str| match value {
        "top" => Some(SurfaceEdge::Top),
        "right" => Some(SurfaceEdge::Right),
        "bottom" => Some(SurfaceEdge::Bottom),
        "left" => Some(SurfaceEdge::Left),
        _ => None,
    };
    if let Some(value) = path_values.first() {
        let edges = value
            .split(':')
            .map(|edge| parse(edge).ok_or("invalid transition path edge"))
            .collect::<Result<Vec<_>, _>>()?;
        if !(2..=8).contains(&edges.len()) || edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(
                "transition path requires 2..=8 edges with no repeated adjacent edge".into(),
            );
        }
        Ok(Some(BarTransitionQualification::Path {
            edges,
            leg_duration,
        }))
    } else {
        let mut edges = retarget_values[0]
            .split(':')
            .map(|edge| parse(edge).ok_or("invalid retarget edge"))
            .collect::<Result<Vec<_>, _>>()?;
        if edges.len() != 3 || edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("retarget requires exactly three distinct path edges".into());
        }
        let target = edges.pop().expect("length checked");
        let via = edges.pop().expect("length checked");
        let source = edges.pop().expect("length checked");
        Ok(Some(BarTransitionQualification::Retarget {
            source,
            via,
            target,
            leg_duration,
            at,
        }))
    }
}

pub fn workspace_content(
    state: &ProductSemanticState,
    placement: ResolvedBarPlacement,
    font_px: f32,
) -> Div {
    let vertical = placement.orientation() == BarOrientation::Vertical;
    let detail = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .when(!vertical, |d| d.whitespace_nowrap().text_ellipsis())
        .child(state.workspace_detail());
    let close = div()
        .flex_shrink_0()
        .when(!vertical, |d| {
            d.ml(px(crate::product_typography::WORKSPACE_CLOSE_GAP_PX))
        })
        .child(crate::product_typography::WORKSPACE_CLOSE_LABEL);
    div()
        .flex()
        .items_center()
        .px(px(20.))
        .text_color(white())
        .overflow_hidden()
        .text_size(px(font_px))
        .when(vertical, |d| {
            d.flex_col()
                .items_start()
                .justify_center()
                .py(px(20.))
                .gap(px(16.))
        })
        .child(detail)
        .child(close)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_is_bounded_and_development_only() {
        assert_eq!(qualification_edge(&[]), Ok(None));
        for edge in ["top", "right", "bottom", "left"] {
            let args = vec![
                "--product-scene".into(),
                format!("--qualify-bar-edge={edge}"),
            ];
            assert_eq!(
                qualification_edge(&args).is_ok(),
                cfg!(feature = "component-lab")
            );
        }
        for args in [
            vec!["--qualify-bar-edge"],
            vec!["--product-scene", "--qualify-bar-edge=diagonal"],
            vec!["--qualify-bar-edge=top"],
            vec![
                "--product-scene",
                "--qualify-bar-edge=top",
                "--qualify-bar-edge=left",
            ],
        ] {
            assert!(
                qualification_edge(&args.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }

    #[test]
    #[cfg(feature = "component-lab")]
    fn transition_selector_is_bounded_and_development_only() {
        let args = vec![
            "--product-scene".into(),
            "--qualify-bar-transition=top:right".into(),
        ];
        assert_eq!(
            qualification_transition(&args).is_ok(),
            cfg!(feature = "component-lab")
        );
        for args in [
            vec!["--qualify-bar-transition"],
            vec!["--product-scene", "--qualify-bar-transition=top:diagonal"],
            vec!["--product-scene", "--qualify-bar-transition=top:top"],
            vec![
                "--product-scene",
                "--qualify-bar-transition=top:right",
                "--qualify-bar-retarget=top:left:right",
            ],
            vec!["--product-scene", "--qualify-bar-duration-ms=3000"],
            vec![
                "--product-scene",
                "--qualify-bar-transition=top:right",
                "--qualify-bar-duration-ms=899",
            ],
        ] {
            assert!(
                qualification_transition(&args.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }

    #[test]
    #[cfg(feature = "component-lab")]
    fn transition_selector_supports_perimeter_path_and_live_retarget() {
        let path_args = vec![
            "--product-scene".into(),
            "--qualify-bar-transition=top:right:bottom:left".into(),
        ];
        let path = qualification_transition(&path_args)
            .unwrap()
            .expect("path selector");
        assert_eq!(path.total_duration(), Duration::from_millis(2700));
        assert_eq!(
            path.target_at(Duration::from_millis(1800)),
            SurfaceEdge::Bottom
        );
        assert_eq!(
            path.target_at(Duration::from_millis(2700)),
            SurfaceEdge::Left
        );

        let retarget_args = vec![
            "--product-scene".into(),
            "--qualify-bar-retarget=top:left:right".into(),
        ];
        let retarget = qualification_transition(&retarget_args)
            .unwrap()
            .expect("retarget selector");
        assert_eq!(retarget.total_duration(), Duration::from_millis(1350));
        // The rail keeps its edge until the rebased leg arrives (BAR-01J).
        assert_eq!(
            retarget.target_at(Duration::from_millis(1349)),
            SurfaceEdge::Top
        );
        assert_eq!(
            retarget.target_at(Duration::from_millis(1350)),
            SurfaceEdge::Right
        );
        let early = qualification_transition(&[
            "--product-scene".into(),
            "--qualify-bar-retarget=top:left:right".into(),
            "--qualify-bar-retarget-at=20".into(),
        ])
        .unwrap()
        .expect("retarget point");
        assert_eq!(early.total_duration(), Duration::from_millis(1080));
        assert!(
            qualification_transition(&[
                "--product-scene".into(),
                "--qualify-bar-retarget=top:left:right".into(),
                "--qualify-bar-retarget-at=99".into(),
            ])
            .is_err()
        );

        let slow_path = qualification_transition(&[
            "--product-scene".into(),
            "--qualify-bar-transition=top:right:bottom:left:top".into(),
            "--qualify-bar-duration-ms=3000".into(),
        ])
        .unwrap()
        .expect("duration override");
        assert_eq!(slow_path.total_duration(), Duration::from_secs(12));
        assert_eq!(
            slow_path.target_at(Duration::from_secs(6)),
            SurfaceEdge::Bottom
        );
    }

    #[test]
    fn tray_presentation_derivation_respects_compact_and_attention() {
        let item_active = crate::product_tray::TrayItemSnapshot {
            id: TrayItemId::new("steam"),
            service: ":1.1373".into(),
            path: "/steam".into(),
            title: "Steam".into(),
            status: TrayStatus::Active,
            category: crate::product_tray::TrayCategory::ApplicationStatus,
            icons: crate::product_tray::TrayIconFacts::named("steam_tray_mono"),
            tooltip: Some(crate::product_tray::TrayTooltip {
                title: "Steam".into(),
                description: "Friends online: 3".into(),
            }),
            menu_path: None,
            item_is_menu: false,
            owner: ":1.donor15".to_owned(),
        };

        let item_passive = crate::product_tray::TrayItemSnapshot {
            id: TrayItemId::new("background-daemon"),
            service: ":1.99".into(),
            path: "/bg".into(),
            title: "Daemon".into(),
            status: TrayStatus::Passive,
            category: crate::product_tray::TrayCategory::SystemServices,
            icons: crate::product_tray::TrayIconFacts::none(),
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.donor15".to_owned(),
        };

        let item_attention = crate::product_tray::TrayItemSnapshot {
            id: TrayItemId::new("shelly"),
            service: ":1.3204".into(),
            path: "/shelly".into(),
            title: "Shelly".into(),
            status: TrayStatus::NeedsAttention,
            category: crate::product_tray::TrayCategory::ApplicationStatus,
            icons: crate::product_tray::TrayIconFacts::named("shelly-idle")
                .with_attention(crate::product_tray::TrayIconSlot::named("shelly-urgent")),
            tooltip: None,
            menu_path: None,
            item_is_menu: false,
            owner: ":1.donor15".to_owned(),
        };

        let snapshot =
            TraySnapshot::from_items(vec![item_active.clone(), item_passive, item_attention]);

        // Default preferences (tray_compact = None) -> Gnosix default expanded (compact = false)
        let prefs_default = EffectiveProductPreferences::default();
        let pres_default = BarTrayPresentation::derive(&snapshot, &prefs_default);
        assert!(pres_default.available);
        assert!(!pres_default.compact);
        // Passive item filtered out: 2 items remain
        assert_eq!(pres_default.items.len(), 2);
        assert_eq!(pres_default.items[0].id.as_str(), "shelly");
        assert!(pres_default.items[0].needs_attention);
        assert_eq!(
            pres_default.items[0].icon,
            TrayIcon::Named("shelly-urgent".into())
        );
        assert!(!pres_default.items[0].is_compact);

        assert_eq!(pres_default.items[1].id.as_str(), "steam");
        assert!(!pres_default.items[1].needs_attention);
        assert_eq!(
            pres_default.items[1].tooltip.as_deref(),
            Some("Steam: Friends online: 3")
        );

        // Compact override (tray_compact = Some(true))
        let mut prefs_compact = EffectiveProductPreferences::default();
        prefs_compact.bar_overrides.tray_compact = Some(true);
        let pres_compact = BarTrayPresentation::derive(&snapshot, &prefs_compact);
        assert!(pres_compact.compact);
        assert_eq!(pres_compact.items.len(), 2);
        assert!(pres_compact.items[0].is_compact);
        assert!(pres_compact.items[1].is_compact);

        // Compact mode preserves exact identities and attributes
        assert_eq!(pres_default.items[0].id, pres_compact.items[0].id);
        assert_eq!(pres_default.items[1].id, pres_compact.items[1].id);
    }

    #[test]
    fn tray_glyphs_map_predictably() {
        assert_eq!(
            tray_item_glyph(
                &TrayIcon::Named("blueman-active".into()),
                &TrayItemId::new("blueman"),
                "Bluetooth"
            ),
            "ᛒ"
        );
        assert_eq!(
            tray_item_glyph(
                &TrayIcon::Named("steam_tray_mono".into()),
                &TrayItemId::new("steam"),
                "Steam"
            ),
            "♨"
        );
        assert_eq!(
            tray_item_glyph(
                &TrayIcon::Named("notification-new".into()),
                &TrayItemId::new("notif"),
                "Notifications"
            ),
            "🔔"
        );
        assert_eq!(
            tray_item_glyph(&TrayIcon::None, &TrayItemId::new("missing"), "Unknown"),
            "◇"
        );
    }

    #[test]
    fn audio_status_never_turns_unavailable_into_zero_percent() {
        let presentation = crate::product_audio::ProductAudioPresentation {
            output: crate::product_audio::ProductAudioEndpointState::ProviderUnavailable,
            input: crate::product_audio::ProductAudioEndpointState::ProviderUnavailable,
        };
        let status = BarAudioStatus::derive(&presentation);
        assert_eq!(status.glyph, "◇");
        assert_eq!(status.text, "—");
        assert_eq!(status.aria_label, "Audio indisponible");
    }

    #[test]
    fn audio_status_projects_observed_level_and_mute_independently() {
        let output = crate::product_audio::ProductAudioEndpoint {
            id: gnos_audio::AudioEndpointId::new(7, 42),
            label: "Built-in Audio".into(),
            level: Some(0.9),
            muted: true,
        };
        let presentation = crate::product_audio::ProductAudioPresentation {
            output: crate::product_audio::ProductAudioEndpointState::Ready(output),
            input: crate::product_audio::ProductAudioEndpointState::DefaultUnresolved {
                role_name: None,
                metadata_error: None,
            },
        };
        let status = BarAudioStatus::derive(&presentation);
        assert_eq!(status.glyph, "🔇");
        assert_eq!(status.text, "90%");
        assert!(status.aria_label.contains("muet"));
    }

    #[test]
    fn ready_audio_status_emits_exact_output_toggle_intent() {
        let id = gnos_audio::AudioEndpointId::new(7, 42);
        let presentation = crate::product_audio::ProductAudioPresentation {
            output: crate::product_audio::ProductAudioEndpointState::Ready(
                crate::product_audio::ProductAudioEndpoint {
                    id,
                    label: "Built-in Audio".into(),
                    level: Some(0.9),
                    muted: false,
                },
            ),
            input: crate::product_audio::ProductAudioEndpointState::ProviderUnavailable,
        };
        assert_eq!(
            primary_audio_intent(&presentation),
            Some(AudioIntent::ToggleOutputMute {
                target: ExactAudioEndpointRef::output(id),
            })
        );
    }

    #[test]
    fn unavailable_or_unresolved_audio_status_has_no_primary_intent() {
        let unavailable = crate::product_audio::ProductAudioPresentation {
            output: crate::product_audio::ProductAudioEndpointState::ProviderUnavailable,
            input: crate::product_audio::ProductAudioEndpointState::ProviderUnavailable,
        };
        assert_eq!(primary_audio_intent(&unavailable), None);

        let unresolved = crate::product_audio::ProductAudioPresentation {
            output: crate::product_audio::ProductAudioEndpointState::DefaultUnresolved {
                role_name: Some("sink.a".into()),
                metadata_error: None,
            },
            input: crate::product_audio::ProductAudioEndpointState::ProviderUnavailable,
        };
        assert_eq!(primary_audio_intent(&unresolved), None);
    }

    #[test]
    fn host_local_audio_dispatches_do_not_fan_out_to_each_other() {
        use gpui::TestAppContext;
        use std::cell::Cell;

        let app = TestAppContext::single();
        let left_calls = Rc::new(Cell::new(0_u32));
        let right_calls = Rc::new(Cell::new(0_u32));
        let left_dispatch: BarAudioDispatch = {
            let calls = left_calls.clone();
            Rc::new(move |_app, _intent| calls.set(calls.get() + 1))
        };
        let right_dispatch: BarAudioDispatch = {
            let calls = right_calls.clone();
            Rc::new(move |_app, _intent| calls.set(calls.get() + 1))
        };
        let intent = AudioIntent::ToggleOutputMute {
            target: ExactAudioEndpointRef::output(gnos_audio::AudioEndpointId::new(7, 42)),
        };

        app.update(|cx| emit_audio_intent(&left_dispatch, cx, intent.clone()));
        assert_eq!(left_calls.get(), 1);
        assert_eq!(right_calls.get(), 0);

        app.update(|cx| emit_audio_intent(&right_dispatch, cx, intent));
        assert_eq!(left_calls.get(), 1);
        assert_eq!(right_calls.get(), 1);
    }
}
