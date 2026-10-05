//! Private product view derived from one current output projection.

use gnos_ux_product_intent::{
    ObservedSurfaceReference, ObservedWorkspaceReference, ProductIntent, ProductIntentRequestId,
};
use gnos_ux_surface::{
    envelope::{RuntimeId, SurfaceEdge},
    grammar::{
        AlongEdgeAlignment, BarPlacement, BarReservationPolicy, BarSensingPolicy, GeometryRect,
        Point2, ResolvedBarPlacement, ResolvedBarTransitionFrame, ResponsiveRadiusToken,
        SurfaceViewport, UnitInterval, Vector2,
        reactive_envelope::{
            ReactiveAttachment, ReactiveEnvelopeFrame, ReactiveEnvelopeInput,
            ReactiveEnvelopeSolver, ReactiveEnvelopeTuning, ReactiveGrowthDirection,
        },
    },
    scene::{
        MonitorSceneId, SceneCycleStep, SceneEpoch, SceneFrame, SceneNodeFrame, SceneNodeId,
        SceneNodeRole, SceneRelationFrame, SceneRelationState, SceneReservationIntent,
    },
};
use gnosix_desktop_model::{
    DesktopContextGeneration, DesktopOutputProjection, DesktopSurfaceRole, SurfaceFocus,
    SurfaceVisibility, WorkspaceFocus, WorkspaceVisibility,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductSemanticState {
    Unavailable,
    Invalid,
    Current(Box<ProductContent>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductWorkspaceContent {
    pub reference: ObservedWorkspaceReference,
    pub label: String,
}

impl ProductWorkspaceContent {
    /// Pure intent construction from one exact observed workspace reference.
    pub fn activation_intent(&self, request_id: ProductIntentRequestId) -> ProductIntent {
        ProductIntent::activate_observed_workspace(request_id, self.reference)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductApplicationContent {
    pub reference: ObservedSurfaceReference,
    pub workspace: Option<ObservedWorkspaceReference>,
    pub title: String,
    pub class: String,
    pub initial_class: String,
    pub process_id: u32,
    pub xwayland: bool,
    pub visibility: SurfaceVisibility,
    pub focus: SurfaceFocus,
}

impl ProductApplicationContent {
    /// Pure intent construction from one exact observed application surface.
    pub fn focus_intent(&self, request_id: ProductIntentRequestId) -> ProductIntent {
        ProductIntent::focus_observed_application(request_id, self.reference)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductContent {
    pub generation: DesktopContextGeneration,
    pub connector: String,
    pub active_workspace: Option<ProductWorkspaceContent>,
    pub focused_workspace: Option<ProductWorkspaceContent>,
    pub workspace_manager_targets: Vec<ProductWorkspaceContent>,
    pub visible_workspaces: Vec<ProductWorkspaceContent>,
    pub applications: Vec<ProductApplicationContent>,
    pub visible_application_count: usize,
    pub focused_application: Option<ProductApplicationContent>,
}

impl ProductSemanticState {
    pub fn from_projection(
        expected_output: &RuntimeId,
        projection: Option<&DesktopOutputProjection>,
    ) -> Self {
        let Some(view) = projection else {
            return Self::Unavailable;
        };
        if view.output.connector.as_str() != expected_output.as_str()
            || view
                .workspaces
                .iter()
                .any(|workspace| workspace.output_hub_id != Some(view.output.output_hub_id))
            || view
                .surfaces
                .iter()
                .any(|surface| surface.output_hub_id != Some(view.output.output_hub_id))
        {
            return Self::Invalid;
        }
        let active = view.output.active_workspace_id.and_then(|id| {
            view.workspaces.iter().find(|workspace| {
                workspace.workspace_id == id
                    && workspace.output_hub_id == Some(view.output.output_hub_id)
                    && workspace.visibility == WorkspaceVisibility::Visible
            })
        });
        if view.output.active_workspace_id.is_some() && active.is_none() {
            return Self::Invalid;
        }
        let active_workspace = active.map(|workspace| ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(workspace.workspace_id, view.generation),
            label: workspace.name.as_str().to_owned(),
        });
        let focused = view
            .workspaces
            .iter()
            .find(|workspace| workspace.focus == WorkspaceFocus::Focused);
        let focused_workspace = focused.map(|workspace| ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(workspace.workspace_id, view.generation),
            label: workspace.name.as_str().to_owned(),
        });
        let workspace_manager_targets = view
            .workspaces
            .iter()
            .map(|workspace| ProductWorkspaceContent {
                reference: ObservedWorkspaceReference::new(workspace.workspace_id, view.generation),
                label: workspace.name.as_str().to_owned(),
            })
            .collect();
        let visible_workspaces = view
            .workspaces
            .iter()
            .filter(|workspace| workspace.visibility == WorkspaceVisibility::Visible)
            .map(|workspace| ProductWorkspaceContent {
                reference: ObservedWorkspaceReference::new(workspace.workspace_id, view.generation),
                label: workspace.name.as_str().to_owned(),
            })
            .collect();
        let applications = view
            .surfaces
            .iter()
            .filter(|surface| surface.role == DesktopSurfaceRole::Application)
            .filter_map(|surface| {
                surface
                    .application
                    .as_ref()
                    .map(|application| ProductApplicationContent {
                        reference: ObservedSurfaceReference::new(
                            surface.surface_id,
                            view.generation,
                        ),
                        workspace: surface.workspace_id.map(|workspace_id| {
                            ObservedWorkspaceReference::new(workspace_id, view.generation)
                        }),
                        title: application.title.clone(),
                        class: application.class.clone(),
                        initial_class: application.initial_class.clone(),
                        process_id: application.process_id,
                        xwayland: application.xwayland,
                        visibility: surface.visibility,
                        focus: surface.focus,
                    })
            })
            .collect::<Vec<_>>();
        let visible_application_count = applications
            .iter()
            .filter(|application| {
                application.visibility == SurfaceVisibility::Visible
                    && active.is_some_and(|workspace| {
                        application.workspace.is_some_and(|reference| {
                            reference.workspace_id() == workspace.workspace_id
                        })
                    })
            })
            .count();
        let focused_application = applications
            .iter()
            .find(|application| {
                application.focus == SurfaceFocus::Focused
                    && focused.is_some_and(|workspace| {
                        application.workspace.is_some_and(|reference| {
                            reference.workspace_id() == workspace.workspace_id
                        })
                    })
            })
            .cloned();
        Self::Current(Box::new(ProductContent {
            generation: view.generation,
            connector: view.output.connector.as_str().to_owned(),
            active_workspace,
            focused_workspace,
            workspace_manager_targets,
            visible_workspaces,
            applications,
            visible_application_count,
            focused_application,
        }))
    }

    /// Logical groups in source order; no dependence on edge or signed tangent.
    pub fn bar_groups(&self) -> Vec<String> {
        match self {
            Self::Unavailable => vec!["gnos.ux".into(), "état indisponible".into()],
            Self::Invalid => vec!["gnos.ux".into(), "état invalide".into()],
            Self::Current(content) => vec![
                "gnos.ux".into(),
                content.connector.clone(),
                content
                    .active_workspace
                    .as_ref()
                    .map_or("aucun espace actif", |workspace| workspace.label.as_str())
                    .into(),
            ],
        }
    }

    pub fn bar_label(&self) -> String {
        match self {
            Self::Unavailable => "gnos.ux · état indisponible".to_owned(),
            Self::Invalid => "gnos.ux · état invalide".to_owned(),
            Self::Current(content) => format!(
                "gnos.ux  /  {}  /  {}",
                content.connector,
                content
                    .active_workspace
                    .as_ref()
                    .map_or("aucun espace actif", |workspace| workspace.label.as_str())
            ),
        }
    }

    pub fn dock_label(&self) -> String {
        match self {
            Self::Unavailable | Self::Invalid => "Espaces indisponibles".to_owned(),
            Self::Current(content) if content.visible_workspaces.is_empty() => {
                "Aucun espace visible".to_owned()
            }
            Self::Current(content) => content
                .visible_workspaces
                .iter()
                .map(|workspace| workspace.label.as_str())
                .collect::<Vec<_>>()
                .join("   ·   "),
        }
    }

    pub fn workspace_detail(&self) -> String {
        match self {
            Self::Unavailable => "État du bureau indisponible".to_owned(),
            Self::Invalid => "Projection du bureau invalide".to_owned(),
            Self::Current(content) => match &content.active_workspace {
                Some(workspace) => format!(
                    "{}  ·  {} application(s) visibles  ·  {}",
                    workspace.label,
                    content.visible_application_count,
                    content
                        .focused_application
                        .as_ref()
                        .map_or("aucune application focalisée", |application| application
                            .title
                            .as_str(),)
                ),
                None => "Aucun espace actif sur cette sortie".to_owned(),
            },
        }
    }

    pub fn can_open_workspace(&self) -> bool {
        matches!(self, Self::Current(content) if content.active_workspace.is_some())
    }

    /// Whether the AppStack can be shown on this output at all.
    ///
    /// Deliberately *not* gated on an active workspace, unlike the workspace
    /// summary. Installed applications are a property of the application
    /// catalog, not of the workspace the user happens to be on, so a user with
    /// no active workspace still has applications they can start.
    ///
    /// It is gated on `Current` semantics, because the AppStack is a product
    /// surface of a working session; on an unavailable or invalid projection the
    /// product should not be advertising that it can start applications.
    pub fn can_open_appstack(&self) -> bool {
        matches!(self, Self::Current(_))
    }

    pub fn bar_accessibility_action_label(&self, envelope_open: bool) -> Option<String> {
        let Self::Current(content) = self else {
            return None;
        };
        let workspace = content.active_workspace.as_ref()?;
        Some(if envelope_open {
            format!(
                "Fermer le résumé de l’espace de travail {}",
                workspace.label
            )
        } else {
            format!(
                "Ouvrir le résumé de l’espace de travail {}",
                workspace.label
            )
        })
    }

    /// The accessible name of the Dock's Applications affordance, reflecting
    /// whether the AppStack is currently open.
    ///
    /// `None` when the AppStack cannot be opened at all, which is what degrades
    /// the element to a plain label rather than advertising a button that does
    /// nothing.
    pub fn appstack_accessibility_action_label(&self, open: bool) -> Option<String> {
        // Unlike the workspace summary, the AppStack does not depend on an
        // active workspace: installed applications are a property of the
        // catalog, not of the workspace the user happens to be on.
        if !matches!(self, Self::Current(_)) {
            return None;
        }
        Some(if open {
            "Fermer AppStack".to_owned()
        } else {
            "Ouvrir AppStack".to_owned()
        })
    }

    pub fn current_generation(&self) -> Option<DesktopContextGeneration> {
        match self {
            Self::Current(content) => Some(content.generation),
            Self::Unavailable | Self::Invalid => None,
        }
    }

    pub fn workspace_manager_targets(&self) -> &[ProductWorkspaceContent] {
        match self {
            Self::Current(content) => &content.workspace_manager_targets,
            Self::Unavailable | Self::Invalid => &[],
        }
    }

    pub fn application_targets(&self) -> &[ProductApplicationContent] {
        match self {
            Self::Current(content) => &content.applications,
            Self::Unavailable | Self::Invalid => &[],
        }
    }

    pub fn application_is_focused(&self, target: &ProductApplicationContent) -> bool {
        matches!(
            self,
            Self::Current(content)
                if content
                    .focused_application
                    .as_ref()
                    .is_some_and(|focused| focused.reference == target.reference)
        )
    }

    pub fn workspace_manager_is_active(&self, target: &ProductWorkspaceContent) -> bool {
        matches!(
            self,
            Self::Current(content)
                if content
                    .active_workspace
                    .as_ref()
                    .is_some_and(|active| active.reference == target.reference)
        )
    }

    /// Pure intent construction only. No capability, transport, admission, or execution occurs here.
    pub fn activate_active_workspace_intent(
        &self,
        request_id: ProductIntentRequestId,
    ) -> Option<ProductIntent> {
        let Self::Current(content) = self else {
            return None;
        };
        let target = content.active_workspace.as_ref()?.reference;
        Some(ProductIntent::activate_observed_workspace(
            request_id, target,
        ))
    }

    /// Pure intent construction only. The authority remains responsible for fresh revalidation.
    pub fn focus_focused_application_intent(
        &self,
        request_id: ProductIntentRequestId,
    ) -> Option<ProductIntent> {
        let Self::Current(content) = self else {
            return None;
        };
        let target = content.focused_application.as_ref()?.reference;
        Some(ProductIntent::focus_observed_application(
            request_id, target,
        ))
    }
}

pub fn reactive_motion_needs_frame(
    solver: Option<&ReactiveEnvelopeSolver>,
    target_open: bool,
    reduced_motion: bool,
) -> bool {
    !reduced_motion && solver.map_or(target_open, |solver| !solver.is_settled_for(target_open))
}

pub fn reactive_surfaces_need_frame(
    workspace_solver: Option<&ReactiveEnvelopeSolver>,
    workspace_target_open: bool,
    appstack_solver: Option<&ReactiveEnvelopeSolver>,
    appstack_target_open: bool,
    reduced_motion: bool,
) -> bool {
    reactive_motion_needs_frame(workspace_solver, workspace_target_open, reduced_motion)
        || reactive_motion_needs_frame(appstack_solver, appstack_target_open, reduced_motion)
}

pub fn reactive_surface_needs_presentation(
    solver: Option<&ReactiveEnvelopeSolver>,
    target_open: bool,
) -> bool {
    target_open || solver.is_some_and(|solver| !solver.is_settled_for(target_open))
}

/// Which product surface is open on a SceneHost, if any.
///
/// A bounded enum rather than a growing set of booleans. `workspace_open` plus
/// `appstack_open` plus `settings_open` is a state space where two flags can be
/// simultaneously true, and the resulting "both panels are open" state is not
/// something the product can render meaningfully or explain to a user. Here the
/// state space is exactly the three states the product has, and "at most one
/// reaction surface is active" is a property of the type rather than a
/// convention that has to be maintained at every mutation site.
///
/// The one transition rule is that opening a surface closes whatever was open.
/// That is what makes the mutual exclusion hold without a runtime check.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProductPanelState {
    #[default]
    Closed,
    WorkspaceSummary,
    AppStack,
}

impl ProductPanelState {
    /// The state this becomes when its own affordance is activated.
    ///
    /// An affordance toggles: activating the open surface's control closes it.
    /// For a surface that is not currently open this is the identity, so a
    /// caller can pass any panel and get the correct toggle behaviour.
    pub fn toggled(self, requested: Self) -> Self {
        if self == requested {
            Self::Closed
        } else {
            requested
        }
    }

    /// Existing product policy: only a successful launch relinquishes AppStack.
    pub fn after_launch(self, record: &crate::product_runtime::ProductLaunchRecord) -> Self {
        if self.is_appstack() && record.terminal.as_ref().is_some_and(|t| t.is_launched()) {
            Self::Closed
        } else {
            self
        }
    }

    /// A stable token for evidence lines.
    ///
    /// Not `Debug`: these strings are read by qualification tooling, and a
    /// derived format would let a cosmetic change silently break a parser.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "CLOSED",
            Self::WorkspaceSummary => "WORKSPACE_SUMMARY",
            Self::AppStack => "APPSTACK",
        }
    }

    /// Whether a reaction surface is showing.
    pub fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub fn is_appstack(self) -> bool {
        matches!(self, Self::AppStack)
    }

    pub fn is_workspace_summary(self) -> bool {
        matches!(self, Self::WorkspaceSummary)
    }

    /// The state this falls back to when the product's capability to show a
    /// surface is lost, for example when the semantic projection goes away.
    pub fn constrained_by(self, can_open_workspace: bool, can_open_appstack: bool) -> Self {
        match self {
            Self::Closed => Self::Closed,
            Self::WorkspaceSummary if can_open_workspace => Self::WorkspaceSummary,
            Self::AppStack if can_open_appstack => Self::AppStack,
            Self::WorkspaceSummary | Self::AppStack => Self::Closed,
        }
    }
}

impl std::fmt::Display for ProductPanelState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a derived Envelope's body comes from.
///
/// Canonical AppStack lineage per the ratified Envelope corpus: parent Dock,
/// origin the Dock hull or the AppStack control anchor, material inherited with
/// override, initial attachment Fused.
///
/// A declared fact, not something read back out of the geometry. A surface
/// whose parent is only *inferred* from where its rectangle lands is a generic
/// panel; the declaration is what makes it a derived body, and a qualifier has
/// to be able to state the lineage without measuring pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppStackLineage {
    /// The Envelope this body is derived from.
    pub parent: SceneNodeId,
    /// Where on the parent the body grows from.
    pub origin: AppStackOrigin,
    /// How the body's material relates to the parent's.
    pub material: EnvelopeMaterialRelation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppStackOrigin {
    /// The parent's own surface hull.
    ParentSurfaceHull,
    /// The AppStack control inside the parent.
    ControlAnchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeMaterialRelation {
    /// Inherit the parent's material, allowing local override.
    InheritWithOverride,
}

impl AppStackLineage {
    /// The canonical AppStack lineage: a derived body of the Dock.
    pub const DOCK_DERIVED: Self = Self {
        parent: SceneNodeId::DOCK,
        origin: AppStackOrigin::ParentSurfaceHull,
        material: EnvelopeMaterialRelation::InheritWithOverride,
    };

    /// A stable token for evidence lines.
    pub fn describe(&self) -> String {
        let parent = match self.parent {
            SceneNodeId::BAR => "BAR",
            SceneNodeId::BODY => "BODY",
            SceneNodeId::DOCK => "DOCK",
            SceneNodeId::APPSTACK => "APPSTACK",
            other => return format!("NODE{}", other.value()),
        };
        let origin = match self.origin {
            AppStackOrigin::ParentSurfaceHull => "PARENT_HULL",
            AppStackOrigin::ControlAnchor => "CONTROL_ANCHOR",
        };
        format!("{parent}->APPSTACK origin={origin} material=INHERIT_WITH_OVERRIDE")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductHit {
    Bar,
    Dock,
    Workspace,
    AppStack,
}

/// Physical bottom space already occupied by the Bar. No independent size input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BottomBarOccupiedExtent(f32);
impl BottomBarOccupiedExtent {
    pub fn from_placement(viewport: SurfaceViewport, bar: ResolvedBarPlacement) -> Self {
        Self(
            if bar.edge() == SurfaceEdge::Bottom
                && bar.reservation() != SceneReservationIntent::NONE
            {
                viewport.height - bar.allocation().top()
            } else {
                0.0
            },
        )
    }
    pub fn extent(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct ProductLayout {
    pub bar: GeometryRect,
    pub bar_placement: ResolvedBarPlacement,
    pub dock: GeometryRect,
    pub workspace: Option<GeometryRect>,
    /// Historical A realization retained for lab comparison and primitive tests.
    /// Production AppStack uses `shared_dock`; connective hull remains generic capital.
    pub appstack: Option<ReactiveEnvelopeFrame>,
    pub shared_dock: Option<crate::product_shared_envelope::SharedDockEnvelope>,
    pub viewport: SurfaceViewport,
    reactive: Option<ReactiveEnvelopeFrame>,
}

impl ProductLayout {
    pub const WORKSPACE_PARENT: SceneNodeId = SceneNodeId::BAR;

    // CANDIDATE_DEFAULT: sizing and colors require real perceptual review.
    pub fn resolve(viewport: SurfaceViewport, openness: f32) -> Option<Self> {
        if viewport.width < 240.0 || viewport.height < 240.0 {
            return None;
        }
        let radius = ResponsiveRadiusToken::candidate_default()
            .resolve(viewport)
            .ok()?;
        let bar_width = (viewport.width - 32.0).min(760.0);
        let bar_left = (viewport.width - bar_width) * 0.5;
        let bar_height = 56.0_f32.max(radius * 2.0);
        let bar_placement = BarPlacement {
            edge: SurfaceEdge::Top,
            along_edge_alignment: AlongEdgeAlignment::Center,
            span: bar_width,
            thickness: bar_height,
            inset: 16.0,
            reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
            sensing_policy: BarSensingPolicy::None,
        }
        .resolve(viewport)
        .ok()?
        .with_radius(radius)
        .ok()?;
        let bar = bar_placement.allocation();
        let dock_width = (viewport.width - 32.0).min(520.0);
        let dock_left = (viewport.width - dock_width) * 0.5;
        let dock_height = 64.0_f32.max(radius * 2.0);
        let dock_bottom = viewport.height - 16.0;
        let dock = GeometryRect::try_new(
            dock_left,
            dock_bottom - dock_height,
            dock_left + dock_width,
            dock_bottom,
            radius,
        )
        .ok()?;
        let progress = openness.clamp(0.0, 1.0);
        let workspace = if progress > 0.01 {
            let top = bar.bottom() + radius * 2.5 * progress;
            let height = (156.0 * progress).min((dock.top() - top - 12.0).max(0.0));
            if height > 1.0 {
                GeometryRect::try_new(bar_left, top, bar_left + bar_width, top + height, radius)
                    .ok()
            } else {
                None
            }
        } else {
            None
        };
        Some(Self {
            bar,
            bar_placement,
            dock,
            workspace,
            appstack: None,
            shared_dock: None,
            viewport,
            reactive: None,
        })
    }

    /// Explicit endpoint resolver. Existing BODY/Dock choreography remains a
    /// top/bottom product recipe; moving the Bar does not rotate that recipe.
    pub fn resolve_with_bar_placement(
        viewport: SurfaceViewport,
        openness: f32,
        placement: BarPlacement,
    ) -> Option<Self> {
        let mut layout = Self::resolve(viewport, openness)?;
        let radius = layout.bar.radius();
        layout.bar_placement = placement.resolve(viewport).ok()?.with_radius(radius).ok()?;
        layout.bar = layout.bar_placement.allocation();
        // BODY is a top-Bar recipe, not a generalized attachment renderer.
        if placement.edge != SurfaceEdge::Top {
            layout.workspace = None;
        } else {
            let progress = openness.clamp(0.0, 1.0);
            let top = layout.bar.bottom() + radius * 2.5 * progress;
            let height = (156.0 * progress).min((layout.dock.top() - top - 12.0).max(0.0));
            layout.workspace = if progress > 0.01 && height > 1.0 {
                GeometryRect::try_new(
                    layout.bar.left(),
                    top,
                    layout.bar.right(),
                    top + height,
                    radius,
                )
                .ok()
            } else {
                None
            };
        }
        layout.resolve_dock_constraint(None)?;
        Some(layout)
    }

    /// Bottom-specific Dock policy, consuming current Bar/body geometry.
    fn resolve_dock_constraint(&mut self, workspace_far: Option<f32>) -> Option<()> {
        let occupied = BottomBarOccupiedExtent::from_placement(self.viewport, self.bar_placement);
        if occupied.extent() > 0.0 {
            let bottom = (self.viewport.height - occupied.extent() - 12.0)
                .min(workspace_far.map_or(f32::INFINITY, |top| top - 12.0));
            self.dock = GeometryRect::try_new(
                self.dock.left(),
                bottom - self.dock.height(),
                self.dock.right(),
                bottom,
                self.dock.radius(),
            )
            .ok()?;
        }
        let safe = self.dock_safe_bounds()?;
        let width = self.dock.width().min(safe.width());
        // Select the actual constraint edge, rather than reconstructing it
        // through a center subtraction that can escape by one f32 ulp.
        let left = self
            .dock
            .left()
            .clamp(safe.left(), (safe.right() - width).max(safe.left()));
        let right = (left + width).min(safe.right());
        self.dock = GeometryRect::try_new(
            left,
            self.dock.top(),
            right,
            self.dock.bottom(),
            self.dock.radius(),
        )
        .ok()?;
        Some(())
    }

    /// Moves the bottom-specific Dock/AppStack owner above Bar arms that
    /// actually intersect its current painted outer in this transition frame.
    /// The caller re-resolves the shared outer after a move, keeping both
    /// products tied to the same evaluated SceneFrame epoch.
    pub(crate) fn constrain_dock_for_bar_transition(
        &mut self,
        transition: &ResolvedBarTransitionFrame,
        dock_outer: GeometryRect,
    ) -> Option<bool> {
        let intersects = |a: GeometryRect, b: GeometryRect| {
            a.left() < b.right()
                && a.right() > b.left()
                && a.top() < b.bottom()
                && a.bottom() > b.top()
        };
        let safe_bottom = transition
            .arms
            .iter()
            .filter(|arm| intersects(arm.bounds, dock_outer))
            .fold(self.dock.bottom(), |bottom, arm| {
                bottom.min(arm.bounds.top() - 12.0)
            });
        if safe_bottom >= self.dock.bottom() {
            return Some(false);
        }
        let dock_top = safe_bottom - self.dock.height();
        if dock_top < 16.0 {
            return None;
        }
        self.dock = GeometryRect::try_new(
            self.dock.left(),
            dock_top,
            self.dock.right(),
            safe_bottom,
            self.dock.radius(),
        )
        .ok()?;
        Some(true)
    }

    /// Available rectangle for the unchanged bottom-anchored shared outer.
    pub fn dock_safe_bounds(&self) -> Option<GeometryRect> {
        let mut left: f32 = 16.0;
        let mut right = self.viewport.width - 16.0;
        let mut top = 16.0;
        let anticipated_top = self.dock.top() - 156.0;
        match self.bar_placement.edge() {
            SurfaceEdge::Top if anticipated_top < self.bar.bottom() + 12.0 => {
                top = self.bar.bottom() + 12.0
            }
            SurfaceEdge::Left
                if self.bar.bottom() > anticipated_top && self.bar.top() < self.dock.bottom() =>
            {
                left = left.max(self.bar.right() + 12.0)
            }
            SurfaceEdge::Right
                if self.bar.bottom() > anticipated_top && self.bar.top() < self.dock.bottom() =>
            {
                right = right.min(self.bar.left() - 12.0)
            }
            _ => {}
        }
        GeometryRect::try_new(left, top, right, self.dock.bottom(), 0.0).ok()
    }

    /// Historical topology A resolver. Production AppStack no longer consumes
    /// this connective hull; retained for comparison and generic regression capital.
    /// Resolve the historical AppStack as a derived body of the Dock.
    ///
    /// The Dock rectangle is passed as the solver's `parent`, and the body grows
    /// upward from the Dock's top edge. Everything that makes the body read as
    /// derived - the attachment state, the neck, the connector, the input
    /// regions - is an output of that lineage rather than decoration applied
    /// around an independently-placed rectangle.
    #[cfg(any(test, feature = "component-lab"))]
    pub fn resolve_appstack_envelope(
        viewport: SurfaceViewport,
        solver: &mut Option<ReactiveEnvelopeSolver>,
        dock: GeometryRect,
        target_open: bool,
        reduced_motion: bool,
        pointer: Option<Point2>,
    ) -> Option<ReactiveEnvelopeFrame> {
        // The body may grow upward only as far as the viewport allows.
        let available = GeometryRect::try_new(0.0, 0.0, viewport.width, dock.top(), 0.0).ok()?;
        let current = match solver {
            Some(solver) => solver,
            slot @ None => slot.insert(ReactiveEnvelopeSolver::new(dock, viewport).ok()?),
        };
        let frame = current
            .step(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        gnos_ux_surface::grammar::reactive_envelope::ReactiveAttachmentIntent::Fused,
                    parent: dock,
                    available,
                    viewport,
                    target_open,
                    pointer,
                    // The Dock is the parent, not a neighbour, so nothing is
                    // pressing on it from a second body.
                    neighbor: None,
                    reduced_motion,
                    growth: ReactiveGrowthDirection::Upward,
                },
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .ok()?;
        (frame.geometry.height() > 1.0).then_some(frame)
    }

    /// Resolve the AppStack to its settled geometry, with no animation.
    ///
    /// The production scene animates the derived body, stepping the solver once
    /// per frame. A visual fixture has to land on the same settled shape without
    /// playing the animation, so the solver is stepped to rest here.
    ///
    /// This is the *only* way a fixture should obtain AppStack geometry. It runs
    /// the same resolver, with the same Dock parent and the same growth
    /// direction, that production does; a fixture that assembled its own
    /// rectangle would approve a shape the product never renders.
    #[cfg(any(test, feature = "component-lab"))]
    pub fn settled_appstack_envelope(
        viewport: SurfaceViewport,
        dock: GeometryRect,
        reduced_motion: bool,
    ) -> Option<ReactiveEnvelopeFrame> {
        let mut solver: Option<ReactiveEnvelopeSolver> = None;
        // The solver's settle horizon. Well past the point the second-order
        // response converges, and fixed, so the result is deterministic.
        const SETTLE_STEPS: usize = 400;
        let mut frame = None;
        for _ in 0..SETTLE_STEPS {
            frame = Self::resolve_appstack_envelope(
                viewport,
                &mut solver,
                dock,
                true,
                reduced_motion,
                None,
            );
        }
        frame
    }

    /// Content uses an inscribed rectangle in the child's body, not the hull's
    /// allocation (which includes the parent). The 45-degree corner inset is
    /// derived from the rounded body and shared by production and lab.
    #[cfg(any(test, feature = "component-lab"))]
    pub fn appstack_content_bounds(frame: &ReactiveEnvelopeFrame) -> GeometryRect {
        let g = frame.geometry;
        let inset = g.radius() * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
        GeometryRect::try_new(
            g.left() + inset,
            g.top() + inset,
            g.right() - inset,
            g.bottom() - inset,
            0.0,
        )
        .expect("inscribed body content bounds")
    }

    /// The Dock and Bar geometry production resolves for a viewport.
    ///
    /// A fixture's parent has to be the real Dock, not a rectangle the fixture
    /// picked, or "derived from the Dock" would be an unverifiable claim.
    pub fn resolved_dock(viewport: SurfaceViewport) -> Option<GeometryRect> {
        Self::resolve(viewport, 1.0).map(|layout| layout.dock)
    }

    pub fn resolve_reactive(
        viewport: SurfaceViewport,
        solver: &mut Option<ReactiveEnvelopeSolver>,
        target_open: bool,
        reduced_motion: bool,
    ) -> Option<Self> {
        Self::resolve_reactive_with_pointer(viewport, solver, target_open, reduced_motion, None)
    }

    pub fn resolve_reactive_with_pointer(
        viewport: SurfaceViewport,
        solver: &mut Option<ReactiveEnvelopeSolver>,
        target_open: bool,
        reduced_motion: bool,
        pointer: Option<Point2>,
    ) -> Option<Self> {
        Self::resolve_reactive_at_edge(
            viewport,
            solver,
            target_open,
            reduced_motion,
            pointer,
            SurfaceEdge::Top,
            14.0,
        )
    }

    pub fn resolve_reactive_at_edge(
        viewport: SurfaceViewport,
        solver: &mut Option<ReactiveEnvelopeSolver>,
        target_open: bool,
        reduced_motion: bool,
        pointer: Option<Point2>,
        edge: SurfaceEdge,
        font_px: f32,
    ) -> Option<Self> {
        Self::resolve_reactive_at_edge_with_sensing(
            viewport,
            solver,
            target_open,
            reduced_motion,
            pointer,
            edge,
            font_px,
            BarSensingPolicy::None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn resolve_reactive_at_edge_with_sensing(
        viewport: SurfaceViewport,
        solver: &mut Option<ReactiveEnvelopeSolver>,
        target_open: bool,
        reduced_motion: bool,
        pointer: Option<Point2>,
        edge: SurfaceEdge,
        font_px: f32,
        sensing_policy: BarSensingPolicy,
    ) -> Option<Self> {
        Self::resolve_reactive_at_edge_with_policies(
            viewport,
            solver,
            target_open,
            reduced_motion,
            pointer,
            edge,
            font_px,
            BarReservationPolicy::ReserveOccupiedExtent,
            sensing_policy,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn resolve_reactive_at_edge_with_policies(
        viewport: SurfaceViewport,
        solver: &mut Option<ReactiveEnvelopeSolver>,
        target_open: bool,
        reduced_motion: bool,
        pointer: Option<Point2>,
        edge: SurfaceEdge,
        font_px: f32,
        reservation_policy: BarReservationPolicy,
        sensing_policy: BarSensingPolicy,
    ) -> Option<Self> {
        let placement = crate::product_bar::presentation_placement_with_policies(
            viewport,
            edge,
            font_px,
            reservation_policy,
            sensing_policy,
        )?;
        let mut layout = Self::resolve_with_bar_placement(viewport, 1.0, placement)?;
        let available = match edge {
            SurfaceEdge::Top => GeometryRect::try_new(
                0.,
                layout.bar.bottom(),
                viewport.width,
                (layout.dock.top() - 12.).max(layout.bar.bottom()),
                0.,
            ),
            SurfaceEdge::Bottom => GeometryRect::try_new(
                0.,
                16. + layout.dock.height() + 12.,
                viewport.width,
                layout.bar.top(),
                0.,
            ),
            SurfaceEdge::Left => GeometryRect::try_new(
                layout.bar.right(),
                0.,
                viewport.width - 16.,
                (layout.dock.top() - 12.).max(layout.bar.bottom()),
                0.,
            ),
            SurfaceEdge::Right => GeometryRect::try_new(
                16.,
                0.,
                layout.bar.left(),
                (layout.dock.top() - 12.).max(layout.bar.bottom()),
                0.,
            ),
        }
        .ok()?;
        let current_solver = match solver {
            Some(solver) => solver,
            slot @ None => slot.insert(ReactiveEnvelopeSolver::new(layout.bar, viewport).ok()?),
        };
        let reactive = current_solver
            .step_from_edge(
                ReactiveEnvelopeInput {
                    attachment_intent:
                        gnos_ux_surface::grammar::reactive_envelope::ReactiveAttachmentIntent::Auto,
                    parent: layout.bar,
                    available,
                    viewport,
                    target_open,
                    pointer,
                    neighbor: (edge == SurfaceEdge::Top).then_some(layout.dock),
                    reduced_motion,
                    growth: ReactiveGrowthDirection::Downward,
                },
                edge,
                1.0 / 60.0,
                ReactiveEnvelopeTuning::candidate_default(),
            )
            .ok()?;
        layout.workspace = (reactive.openness > 0.01
            && reactive.geometry.width() > 1.0
            && reactive.geometry.height() > 1.0)
            .then_some(reactive.geometry);
        if edge == SurfaceEdge::Bottom {
            layout.resolve_dock_constraint(layout.workspace.map(|rect| rect.top()))?;
        }
        layout.reactive = Some(reactive);
        Some(layout)
    }

    /// Width from the same resolved frame that supplied connector input bounds.
    pub fn connector_paint_width(&self) -> f32 {
        self.reactive.map_or(0.0, |frame| frame.connector_width)
    }

    pub fn reactive_frame(&self) -> Option<ReactiveEnvelopeFrame> {
        self.reactive
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<ProductHit> {
        if gnos_ux_surface::pointer_regions::project_resolved_bar_pointer_regions(
            self.viewport,
            self.bar_placement,
        )
        .ok()
        .is_some_and(|regions| {
            regions
                .regions()
                .iter()
                .any(|r| r.role().as_str() == "surface" && r.bounds().contains_point(x, y))
        }) {
            return Some(ProductHit::Bar);
        }
        let workspace_hit = self.reactive.map_or_else(
            || self.workspace.is_some_and(|rect| contains(rect, x, y)),
            |frame| frame.contains_shape_input(x, y),
        );
        if workspace_hit {
            Some(ProductHit::Workspace)
        } else if self
            .appstack
            .as_ref()
            .is_some_and(|frame| frame.contains_shape_input(x, y))
        {
            Some(ProductHit::AppStack)
        } else if let Some(shared) = self.shared_dock {
            if !shared.contains_input(x, y) {
                None
            } else if shared.appstack.interactable && shared.appstack.clip.contains_point(x, y) {
                Some(ProductHit::AppStack)
            } else {
                Some(ProductHit::Dock)
            }
        } else if contains(self.dock, x, y) {
            Some(ProductHit::Dock)
        } else {
            None
        }
    }

    pub fn frame(&self, scene_id: MonitorSceneId, epoch: SceneEpoch) -> SceneFrame {
        let mut nodes = vec![
            node(
                SceneNodeId::BAR,
                SceneNodeRole::Bar,
                self.bar_placement.allocation(),
            ),
            node(SceneNodeId::DOCK, SceneNodeRole::Dock, self.dock),
        ];
        let bar_input = gnos_ux_surface::pointer_regions::project_resolved_bar_pointer_regions(
            self.viewport,
            self.bar_placement,
        )
        .expect("resolved Bar placement fits its scene viewport");
        let mut input_regions = bar_input
            .regions()
            .iter()
            .map(|region| region.bounds())
            .collect::<Vec<_>>();
        if let Some(shared) = self.shared_dock {
            input_regions.extend(shared.input_regions());
        } else {
            input_regions.push(self.dock);
        }
        let mut relations = Vec::new();
        // The workspace summary and the AppStack are mutually exclusive, so
        // they share `SceneNodeId::BODY`. That is deliberate rather than a
        // shortcut: the scene's body node is the product's single reaction
        // surface, and giving each panel its own node would make a state in
        // which two body nodes existed representable, when the product has no
        // way to render it. Sharing the id makes the invariant structural.
        if let Some(shared) = self.shared_dock
            && shared.appstack.visible
        {
            nodes.push(node(
                SceneNodeId::APPSTACK,
                SceneNodeRole::AppStack,
                shared.appstack.bounds,
            ));
        }
        if let Some(appstack) = self.appstack {
            // The AppStack gets its own node and role, not `Body`. `Body` is an
            // internal composition slot; the product identity here is AppStack,
            // and the scene graph should say so.
            nodes.push(node(
                SceneNodeId::APPSTACK,
                SceneNodeRole::AppStack,
                appstack.paint_bounds,
            ));
            // Input follows the derived hull, so the clickable region and the
            // painted region are the same fact.
            input_regions.extend(appstack.shape_input_regions());
            // The declared lineage, stated rather than inferred. Recorded as a
            // relation so a reviewer reads the parent off the scene graph.
            //
            // The state is Fused because the body is anchored to its parent and
            // has not separated from it. It is Fused *by construction* here, not
            // by a solver transition: see the `appstack` field's KNOWN GAP.
            relations.push(SceneRelationFrame {
                from: SceneNodeId::DOCK,
                to: SceneNodeId::APPSTACK,
                state: match appstack.attachment {
                    ReactiveAttachment::Fused => SceneRelationState::Connected,
                    ReactiveAttachment::Bridged => SceneRelationState::Bridged,
                    ReactiveAttachment::Detached => SceneRelationState::Detached,
                },
                presence: UnitInterval::clamped(appstack.openness),
                // A zero-width connector. The body is anchored flush to its
                // parent with no neck, so there is no joint to draw; the
                // `presence` above carries the relation and the KNOWN GAP on the
                // `appstack` field records that the extrusion is not yet
                // solver-derived.
                connector: appstack.connector,
            });
        } else if let Some(workspace) = self.workspace {
            nodes.push(node(
                SceneNodeId::BODY,
                SceneNodeRole::Body,
                self.reactive.map_or(workspace, |frame| frame.paint_bounds),
            ));
            if let Some(reactive) = self.reactive {
                input_regions.extend(reactive.shape_input_regions());
            } else {
                input_regions.push(workspace);
            }
            if let Some(reactive) = self.reactive
                && reactive.openness > 0.01
            {
                relations.push(SceneRelationFrame {
                    from: SceneNodeId::BAR,
                    to: SceneNodeId::BODY,
                    state: match reactive.attachment {
                        ReactiveAttachment::Fused => SceneRelationState::Connected,
                        ReactiveAttachment::Bridged => SceneRelationState::Bridged,
                        ReactiveAttachment::Detached => SceneRelationState::Detached,
                    },
                    // Openness is scene presence; detachment changes resolved
                    // connector geometry/width, never its alpha as a substitute.
                    presence: UnitInterval::clamped(reactive.openness),
                    connector: reactive.connector,
                });
            }
        }
        SceneFrame {
            scene_id,
            epoch,
            viewport: self.viewport,
            cycle_step: SceneCycleStep::Rest,
            nodes,
            relations,
            input_regions,
            reservation: self.bar_placement.reservation(),
            influence_pressure: 0.0,
        }
    }

    /// Resolve a single scene epoch from one evaluated Bar transition frame.
    /// The existing BAR node identity is retained; its envelope, hit geometry,
    /// and reservation all come from the same transition evaluation.
    pub fn frame_with_bar_transition(
        &self,
        scene_id: MonitorSceneId,
        epoch: SceneEpoch,
        transition: &ResolvedBarTransitionFrame,
    ) -> SceneFrame {
        let mut frame = self.frame(scene_id, epoch);
        if let Some(bar) = frame
            .nodes
            .iter_mut()
            .find(|node| node.id == SceneNodeId::BAR)
        {
            bar.envelope = transition.allocation;
            bar.role = SceneNodeRole::Bar;
        }
        frame.input_regions.retain(|region| {
            !transition.source_allocation.contains(*region)
                && !transition.target_allocation.contains(*region)
        });
        frame.input_regions.extend(transition.input_regions());
        frame.reservation = transition.reservation;
        frame
    }
}

fn contains(rect: GeometryRect, x: f32, y: f32) -> bool {
    x >= rect.left() && x <= rect.right() && y >= rect.top() && y <= rect.bottom()
}

fn node(id: SceneNodeId, role: SceneNodeRole, envelope: GeometryRect) -> SceneNodeFrame {
    SceneNodeFrame {
        id,
        role,
        envelope,
        velocity: Vector2::new(0.0, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_typography::ProductTextScale;
    use gnosix_desktop_model::{
        ApplicationSurfaceDescriptors, ConnectorName, DesktopContextGeneration, LogicalPoint,
        LogicalRect, LogicalSize, OutputHubId, OutputState, OutputTransform,
        PhysicalOutputDescriptors, PhysicalPixelSize, ScaleMilli, SurfaceId, SurfaceState,
        WorkspaceId, WorkspaceName, WorkspaceState,
    };

    fn projection(name: &str) -> DesktopOutputProjection {
        let output_hub_id = OutputHubId::from_bytes([1; 16]);
        DesktopOutputProjection {
            generation: DesktopContextGeneration(7),
            output: OutputState {
                output_hub_id,
                connector: ConnectorName::new("DP-1").unwrap(),
                descriptors: PhysicalOutputDescriptors::default(),
                active_workspace_id: Some(WorkspaceId::from_bytes([2; 16])),
                physical_size: PhysicalPixelSize::new(1920, 1080).unwrap(),
                logical_position: LogicalPoint::ORIGIN,
                logical_size: LogicalSize::new(1920, 1080).unwrap(),
                scale: ScaleMilli::from_milli(1000).unwrap(),
                transform: OutputTransform::Normal,
            },
            workspaces: vec![WorkspaceState {
                workspace_id: WorkspaceId::from_bytes([2; 16]),
                name: WorkspaceName::new(name).unwrap(),
                output_hub_id: Some(output_hub_id),
                visibility: WorkspaceVisibility::Visible,
                focus: WorkspaceFocus::Focused,
            }],
            surfaces: Vec::new(),
        }
    }

    fn projection_with_focused_application(
        title: &str,
        surface_byte: u8,
    ) -> DesktopOutputProjection {
        let mut view = projection("Code");
        let output_hub_id = view.output.output_hub_id;
        let workspace_id = view.workspaces[0].workspace_id;
        view.surfaces.push(SurfaceState {
            surface_id: SurfaceId::from_bytes([surface_byte; 16]),
            role: DesktopSurfaceRole::Application,
            output_hub_id: Some(output_hub_id),
            workspace_id: Some(workspace_id),
            bounds: LogicalRect::new(
                LogicalPoint::new(20, 20),
                LogicalSize::new(800, 600).unwrap(),
            ),
            application: Some(ApplicationSurfaceDescriptors {
                class: "terminal".to_owned(),
                initial_class: "terminal".to_owned(),
                title: title.to_owned(),
                initial_title: title.to_owned(),
                process_id: 42,
                xwayland: false,
            }),
            visibility: SurfaceVisibility::Visible,
            focus: SurfaceFocus::Focused,
        });
        view
    }

    #[test]
    fn product_layout_uses_one_radius_for_bar_dock_and_workspace_per_output() {
        let wide = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let layout = ProductLayout::resolve(wide, 1.0).unwrap();
        let radius = layout.bar.radius();
        assert!(radius > 0.0);
        assert!((layout.dock.radius() - radius).abs() < 0.001);
        assert!((layout.workspace.unwrap().radius() - radius).abs() < 0.001);

        let compact = SurfaceViewport::try_new(1280.0, 320.0).unwrap();
        let compact_layout = ProductLayout::resolve(compact, 1.0).unwrap();
        assert!(compact_layout.bar.radius() < radius);
        assert!((compact_layout.dock.radius() - compact_layout.bar.radius()).abs() < 0.001);
        assert!(
            (compact_layout.workspace.unwrap().radius() - compact_layout.bar.radius()).abs()
                < 0.001
        );
    }

    #[test]
    fn text_scale_changes_type_size_without_changing_surface_input_or_reservation_geometry() {
        let viewports = [
            SurfaceViewport::try_new(1280.0, 720.0).unwrap(),
            SurfaceViewport::try_new(2560.0, 1440.0).unwrap(),
        ];
        let scales = [1.0_f32, 1.25, 1.5, 2.0].map(|factor| ProductTextScale::new(factor).unwrap());

        for viewport in viewports {
            let baseline = ProductLayout::resolve(viewport, 1.0).unwrap();
            let baseline_frame = baseline.frame(MonitorSceneId::new(1), SceneEpoch::new(1));
            let baseline_input = baseline_frame.input_regions.clone();
            let baseline_reservation = baseline_frame.reservation;
            for scale in scales {
                let font_size = scale.font_size_px();
                assert!((14.0..=28.0).contains(&font_size));
                let layout = ProductLayout::resolve(viewport, 1.0).unwrap();
                let frame = layout.frame(MonitorSceneId::new(1), SceneEpoch::new(1));
                assert!(font_size <= layout.bar.height());
                assert!(font_size <= layout.dock.height());
                assert!(font_size <= layout.workspace.unwrap().height());
                assert_eq!(layout.bar, baseline.bar);
                assert_eq!(layout.dock, baseline.dock);
                assert_eq!(layout.workspace, baseline.workspace);
                assert_eq!(frame.input_regions, baseline_input);
                assert_eq!(frame.reservation, baseline_reservation);
                assert_eq!(
                    layout.hit(
                        (layout.bar.left() + layout.bar.right()) * 0.5,
                        (layout.bar.top() + layout.bar.bottom()) * 0.5
                    ),
                    Some(ProductHit::Bar)
                );
                assert!(frame.validate().is_valid());
            }
        }
    }

    #[test]
    fn semantic_changes_reach_bar_and_details() {
        let output = RuntimeId::new("DP-1");
        let first = ProductSemanticState::from_projection(&output, Some(&projection("Code")));
        let second = ProductSemanticState::from_projection(&output, Some(&projection("Design")));
        assert!(first.bar_label().contains("Code"));
        assert!(second.workspace_detail().contains("Design"));
        assert_ne!(first.bar_label(), second.bar_label());
        assert!(first.can_open_workspace());
    }

    #[test]
    fn workspace_intent_preserves_exact_id_when_display_labels_collide() {
        let output = RuntimeId::new("DP-1");
        let first = projection("same-label");
        let mut second = projection("same-label");
        second.output.active_workspace_id = Some(WorkspaceId::from_bytes([9; 16]));
        second.workspaces[0].workspace_id = WorkspaceId::from_bytes([9; 16]);

        let a = ProductSemanticState::from_projection(&output, Some(&first));
        let b = ProductSemanticState::from_projection(&output, Some(&second));
        let request = ProductIntentRequestId::from_bytes([31; 16]);

        let ProductIntent::ActivateObservedWorkspace {
            target: a_target, ..
        } = a.activate_active_workspace_intent(request).unwrap()
        else {
            panic!("workspace intent");
        };
        let ProductIntent::ActivateObservedWorkspace {
            target: b_target, ..
        } = b.activate_active_workspace_intent(request).unwrap()
        else {
            panic!("workspace intent");
        };
        assert_ne!(a_target.workspace_id(), b_target.workspace_id());
        assert_eq!(a_target.observed_generation(), DesktopContextGeneration(7));
        assert_eq!(b_target.observed_generation(), DesktopContextGeneration(7));
    }

    #[test]
    fn workspace_manager_retains_hidden_inactive_targets_with_exact_identity() {
        let output = RuntimeId::new("DP-1");
        let mut view = projection("same-label");
        let hidden_id = WorkspaceId::from_bytes([9; 16]);
        view.workspaces.push(WorkspaceState {
            workspace_id: hidden_id,
            name: WorkspaceName::new("same-label").unwrap(),
            output_hub_id: Some(view.output.output_hub_id),
            visibility: WorkspaceVisibility::Hidden,
            focus: WorkspaceFocus::Unfocused,
        });

        let state = ProductSemanticState::from_projection(&output, Some(&view));
        let targets = state.workspace_manager_targets();
        assert_eq!(targets.len(), 2);
        assert_eq!(
            targets
                .iter()
                .map(|target| target.reference.workspace_id())
                .collect::<Vec<_>>(),
            vec![WorkspaceId::from_bytes([2; 16]), hidden_id]
        );
        assert_eq!(
            targets
                .iter()
                .map(|target| target.reference.observed_generation())
                .collect::<Vec<_>>(),
            vec![DesktopContextGeneration(7), DesktopContextGeneration(7)]
        );
        assert_eq!(targets[0].label, targets[1].label);
        assert!(state.workspace_manager_is_active(&targets[0]));
        assert!(!state.workspace_manager_is_active(&targets[1]));

        let request = ProductIntentRequestId::from_bytes([41; 16]);
        let ProductIntent::ActivateObservedWorkspace { target, .. } =
            targets[1].activation_intent(request)
        else {
            panic!("workspace manager intent");
        };
        assert_eq!(target.workspace_id(), hidden_id);
        assert_eq!(target.observed_generation(), DesktopContextGeneration(7));
    }

    #[test]
    fn application_targets_preserve_exact_surface_identity_when_titles_collide() {
        let output = RuntimeId::new("DP-1");
        let mut view = projection_with_focused_application("Terminal", 30);
        let output_hub_id = view.output.output_hub_id;
        let workspace_id = view.workspaces[0].workspace_id;
        view.surfaces.push(SurfaceState {
            surface_id: SurfaceId::from_bytes([31; 16]),
            role: DesktopSurfaceRole::Application,
            output_hub_id: Some(output_hub_id),
            workspace_id: Some(workspace_id),
            bounds: LogicalRect::new(
                LogicalPoint::new(40, 40),
                LogicalSize::new(640, 480).unwrap(),
            ),
            application: Some(ApplicationSurfaceDescriptors {
                class: "terminal".to_owned(),
                initial_class: "terminal".to_owned(),
                title: "Terminal".to_owned(),
                initial_title: "Terminal".to_owned(),
                process_id: 43,
                xwayland: false,
            }),
            visibility: SurfaceVisibility::Hidden,
            focus: SurfaceFocus::Unfocused,
        });

        let state = ProductSemanticState::from_projection(&output, Some(&view));
        let targets = state.application_targets();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].title, targets[1].title);
        assert_ne!(
            targets[0].reference.surface_id(),
            targets[1].reference.surface_id()
        );
        assert!(state.application_is_focused(&targets[0]));
        assert!(!state.application_is_focused(&targets[1]));

        let request = ProductIntentRequestId::from_bytes([42; 16]);
        let ProductIntent::FocusObservedApplication { target, .. } =
            targets[1].focus_intent(request)
        else {
            panic!("application focus intent");
        };
        assert_eq!(target.surface_id(), SurfaceId::from_bytes([31; 16]));
        assert_eq!(target.observed_generation(), DesktopContextGeneration(7));
    }

    #[test]
    fn unavailable_and_invalid_semantics_expose_no_workspace_manager_targets() {
        assert!(
            ProductSemanticState::Unavailable
                .workspace_manager_targets()
                .is_empty()
        );
        assert!(
            ProductSemanticState::Invalid
                .workspace_manager_targets()
                .is_empty()
        );

        let mut wrong = projection("Code");
        wrong.output.connector = ConnectorName::new("HDMI-A-1").unwrap();
        let state = ProductSemanticState::from_projection(&RuntimeId::new("DP-1"), Some(&wrong));
        assert!(state.workspace_manager_targets().is_empty());
    }

    #[test]
    fn observation_generation_is_part_of_actionable_workspace_reference() {
        let output = RuntimeId::new("DP-1");
        let first = projection("Code");
        let mut second = projection("Code");
        second.generation = DesktopContextGeneration(8);

        let a = ProductSemanticState::from_projection(&output, Some(&first));
        let b = ProductSemanticState::from_projection(&output, Some(&second));
        let ProductSemanticState::Current(a) = a else {
            panic!("current")
        };
        let ProductSemanticState::Current(b) = b else {
            panic!("current")
        };
        assert_eq!(
            a.active_workspace.unwrap().reference.observed_generation(),
            DesktopContextGeneration(7)
        );
        assert_eq!(
            b.active_workspace.unwrap().reference.observed_generation(),
            DesktopContextGeneration(8)
        );
    }

    #[test]
    fn application_intent_preserves_surface_id_when_titles_collide() {
        let output = RuntimeId::new("DP-1");
        let first = projection_with_focused_application("Terminal", 5);
        let second = projection_with_focused_application("Terminal", 6);
        let a = ProductSemanticState::from_projection(&output, Some(&first));
        let b = ProductSemanticState::from_projection(&output, Some(&second));
        let request = ProductIntentRequestId::from_bytes([32; 16]);

        let ProductIntent::FocusObservedApplication {
            target: a_target, ..
        } = a.focus_focused_application_intent(request).unwrap()
        else {
            panic!("surface intent");
        };
        let ProductIntent::FocusObservedApplication {
            target: b_target, ..
        } = b.focus_focused_application_intent(request).unwrap()
        else {
            panic!("surface intent");
        };
        assert_ne!(a_target.surface_id(), b_target.surface_id());
        assert_eq!(a_target.observed_generation(), DesktopContextGeneration(7));
        assert_eq!(b_target.observed_generation(), DesktopContextGeneration(7));
    }

    #[test]
    fn bar_accessibility_action_name_tracks_open_state_and_fails_closed() {
        let output = RuntimeId::new("DP-1");
        let current = ProductSemanticState::from_projection(&output, Some(&projection("Code")));
        assert_eq!(
            current.bar_accessibility_action_label(false).as_deref(),
            Some("Ouvrir le résumé de l’espace de travail Code")
        );
        assert_eq!(
            current.bar_accessibility_action_label(true).as_deref(),
            Some("Fermer le résumé de l’espace de travail Code")
        );
        assert_eq!(
            ProductSemanticState::Unavailable.bar_accessibility_action_label(false),
            None
        );
        assert_eq!(
            ProductSemanticState::Invalid.bar_accessibility_action_label(false),
            None
        );
    }

    #[test]
    fn each_output_can_open_its_local_active_workspace_without_claiming_global_focus() {
        let first = projection("1");
        let mut second = projection("11");
        let second_hub = OutputHubId::from_bytes([3; 16]);
        second.output.output_hub_id = second_hub;
        second.output.connector = ConnectorName::new("HDMI-A-1").unwrap();
        second.output.active_workspace_id = Some(WorkspaceId::from_bytes([11; 16]));
        second.workspaces[0].workspace_id = WorkspaceId::from_bytes([11; 16]);
        second.workspaces[0].output_hub_id = Some(second_hub);
        second.workspaces[0].focus = WorkspaceFocus::Unfocused;
        second.workspaces.push(WorkspaceState {
            workspace_id: WorkspaceId::from_bytes([12; 16]),
            name: WorkspaceName::new("special:scratch").unwrap(),
            output_hub_id: Some(second_hub),
            visibility: WorkspaceVisibility::Visible,
            focus: WorkspaceFocus::Unfocused,
        });
        let a = ProductSemanticState::from_projection(&RuntimeId::new("DP-1"), Some(&first));
        let b = ProductSemanticState::from_projection(&RuntimeId::new("HDMI-A-1"), Some(&second));
        assert!(a.can_open_workspace());
        assert!(b.can_open_workspace());
        assert!(b.bar_label().contains("11"));
        assert!(b.workspace_detail().contains("11"));
        assert_eq!(
            b.bar_accessibility_action_label(false).as_deref(),
            Some("Ouvrir le résumé de l’espace de travail 11")
        );
        let ProductSemanticState::Current(content) = b else {
            panic!("current");
        };
        assert_eq!(content.focused_workspace, None);
        assert_eq!(content.focused_application, None);
        assert_eq!(content.visible_workspaces.len(), 2);
    }

    #[test]
    fn absent_and_wrong_output_disable_workspace_opening() {
        let output = RuntimeId::new("DP-1");
        assert_eq!(
            ProductSemanticState::from_projection(&output, None),
            ProductSemanticState::Unavailable
        );
        let wrong = ProductSemanticState::from_projection(
            &RuntimeId::new("DP-2"),
            Some(&projection("Code")),
        );
        assert_eq!(wrong, ProductSemanticState::Invalid);
        assert!(!wrong.can_open_workspace());
        let mut stale = projection("Code");
        stale.output.active_workspace_id = Some(WorkspaceId::from_bytes([9; 16]));
        let invalid = ProductSemanticState::from_projection(&output, Some(&stale));
        assert_eq!(invalid, ProductSemanticState::Invalid);
        assert!(!invalid.can_open_workspace());
    }

    #[test]
    fn pointer_position_biases_the_real_product_contour_during_opening() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut left_solver = None;
        let mut right_solver = None;
        let mut left = None;
        let mut right = None;
        for _ in 0..6 {
            left = ProductLayout::resolve_reactive_with_pointer(
                viewport,
                &mut left_solver,
                true,
                false,
                Some(Point2::new(450.0, 500.0)),
            );
            right = ProductLayout::resolve_reactive_with_pointer(
                viewport,
                &mut right_solver,
                true,
                false,
                Some(Point2::new(830.0, 500.0)),
            );
        }
        let left = left.unwrap().reactive_frame().unwrap();
        let right = right.unwrap().reactive_frame().unwrap();

        assert!(left.deformation > 0.001);
        assert!(right.deformation > 0.001);
        assert!(
            left.top_contour[1].y() < left.top_contour[3].y(),
            "left pointer should localize opening deformation to the left side"
        );
        assert!(
            right.top_contour[3].y() < right.top_contour[1].y(),
            "right pointer should localize opening deformation to the right side"
        );
    }

    #[test]
    fn constrained_viewport_resolves_centered_product_strain_into_paint_and_input_geometry() {
        let viewport = SurfaceViewport::try_new(1062.0, 943.0).unwrap();
        let mut solver = None;
        let layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, true).unwrap();
        let frame = layout.reactive_frame().unwrap();
        let center = (frame.geometry.left() + frame.geometry.right()) * 0.5;

        assert_eq!(
            center, 531.0,
            "bilateral confinement must preserve centering"
        );
        assert!(
            frame.deformation > 0.0,
            "the product consumer must resolve available-space confinement as shape strain"
        );
        assert!(
            (frame.top_contour[2].y() - frame.geometry.top()).abs() > 1.0,
            "the product contour must visibly express the constrained shape"
        );
        assert_eq!(frame.input_bounds, frame.paint_bounds);
        assert!((frame.geometry.width() - layout.bar.width() * 0.65).abs() < 0.01);
        assert!(frame.hull.contour_count() == 1);
        assert!(
            frame.geometry.top() - layout.bar.bottom() >= layout.bar.width() * 0.075 - 0.01,
            "the resting connective contour must have enough depth to expose its shoulders"
        );
        assert_eq!(layout.workspace, Some(frame.geometry));
    }

    #[test]
    fn pointer_contour_exceeds_candidate_visual_review_floor() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut left_solver = None;
        let mut right_solver = None;
        let mut peak = 0.0_f32;
        let mut frames_above_eight = 0;
        for _ in 0..60 {
            let left = ProductLayout::resolve_reactive_with_pointer(
                viewport,
                &mut left_solver,
                true,
                false,
                Some(Point2::new(450.0, 500.0)),
            )
            .unwrap()
            .reactive_frame()
            .unwrap();
            let right = ProductLayout::resolve_reactive_with_pointer(
                viewport,
                &mut right_solver,
                true,
                false,
                Some(Point2::new(830.0, 500.0)),
            )
            .unwrap()
            .reactive_frame()
            .unwrap();
            let left_lift = left.top_contour[3].y() - left.top_contour[1].y();
            let right_lift = right.top_contour[1].y() - right.top_contour[3].y();
            let weakest_side = left_lift.min(right_lift);
            peak = peak.max(weakest_side);
            frames_above_eight += usize::from(weakest_side >= 8.0);
        }
        // Internal review floor, not a claim of operator acceptance.
        eprintln!("directional lift: peak={peak:.2}px, frames >=8px={frames_above_eight}");
        assert!(peak >= 10.0, "peak directional lift was only {peak:.2} px");
        assert!(
            frames_above_eight >= 8,
            "only {frames_above_eight} frames had at least 8 px directional lift"
        );
    }

    #[test]
    fn lateral_pointer_motion_moves_the_product_body_continuously() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let mut frame = ProductLayout::resolve_reactive_with_pointer(
            viewport,
            &mut solver,
            true,
            true,
            Some(Point2::new(640.0, 100.0)),
        )
        .unwrap()
        .reactive_frame()
        .unwrap();
        let rest_center = (frame.geometry.left() + frame.geometry.right()) * 0.5;
        let rest_neck = frame.neck_width;
        let rest_contour = frame.top_contour;
        for _ in 0..12 {
            frame = ProductLayout::resolve_reactive_with_pointer(
                viewport,
                &mut solver,
                true,
                false,
                Some(Point2::new(830.0, 100.0)),
            )
            .unwrap()
            .reactive_frame()
            .unwrap();
        }
        let displaced_center = (frame.geometry.left() + frame.geometry.right()) * 0.5;
        assert!(
            displaced_center > rest_center + 8.0,
            "the open body should follow a lateral pointer target; moved only {:.2}px",
            displaced_center - rest_center
        );
        assert!(
            frame.neck_width < rest_neck,
            "lateral parent-child strain should tighten the connector"
        );
        let contour_change = frame
            .top_contour
            .iter()
            .zip(rest_contour)
            .map(|(current, rest)| (current.y() - rest.y()).abs())
            .fold(0.0_f32, f32::max);
        eprintln!(
            "lateral response: body_shift={:.2}px neck_loss={:.2}px contour_change={contour_change:.2}px",
            displaced_center - rest_center,
            rest_neck - frame.neck_width
        );
        assert!(
            contour_change > 1.0,
            "local contour did not visibly respond"
        );
        assert_eq!(frame.input_bounds, frame.paint_bounds);

        let shoulder_lag = (displaced_center - frame.connector.p0().x()).abs();
        eprintln!("shoulder lag: {shoulder_lag:.2}px");
        assert!(shoulder_lag > 1.0, "shoulder must lag the moving body");

        let before_reverse = displaced_center;
        frame = ProductLayout::resolve_reactive_with_pointer(
            viewport,
            &mut solver,
            true,
            false,
            Some(Point2::new(450.0, 100.0)),
        )
        .unwrap()
        .reactive_frame()
        .unwrap();
        let after_reverse = (frame.geometry.left() + frame.geometry.right()) * 0.5;
        assert!(
            after_reverse > before_reverse,
            "reversal must preserve the existing rightward movement for its first step"
        );
        assert_eq!(frame.input_bounds, frame.paint_bounds);

        let mut closed = None;
        for _ in 0..600 {
            let layout = ProductLayout::resolve_reactive_with_pointer(
                viewport,
                &mut solver,
                false,
                false,
                Some(Point2::new(830.0, 100.0)),
            )
            .unwrap();
            let settled = solver.as_ref().unwrap().is_settled_for(false);
            closed = Some(layout);
            if settled {
                break;
            }
        }
        let closed = closed.unwrap();
        let closed_frame = closed.reactive_frame().unwrap();
        assert!(closed.workspace.is_none());
        assert_eq!(closed_frame.openness, 0.0);
        assert!((closed_frame.geometry.width() - closed.bar.width() * 0.65).abs() < 0.01);
        assert!(solver.as_ref().unwrap().is_settled_for(false));
        let repeated = ProductLayout::resolve_reactive_with_pointer(
            viewport,
            &mut solver,
            false,
            false,
            Some(Point2::new(830.0, 100.0)),
        )
        .unwrap()
        .reactive_frame()
        .unwrap();
        assert_eq!(repeated, closed_frame);
    }

    #[test]
    fn reduced_motion_matches_the_settled_lateral_product_geometry() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let pointer = Some(Point2::new(830.0, 100.0));
        let mut reduced_solver = None;
        let reduced = ProductLayout::resolve_reactive_with_pointer(
            viewport,
            &mut reduced_solver,
            true,
            true,
            pointer,
        )
        .unwrap()
        .reactive_frame()
        .unwrap();

        let mut animated_solver = None;
        let mut animated = None;
        for _ in 0..600 {
            let layout = ProductLayout::resolve_reactive_with_pointer(
                viewport,
                &mut animated_solver,
                true,
                false,
                pointer,
            )
            .unwrap();
            animated = layout.reactive_frame();
            if animated_solver.as_ref().unwrap().is_settled_for(true) {
                break;
            }
        }
        let animated = animated.unwrap();
        assert!(animated_solver.as_ref().unwrap().is_settled_for(true));
        assert_eq!(reduced.geometry, animated.geometry);
        assert_eq!(reduced.top_contour, animated.top_contour);
        assert_eq!(reduced.connector_bounds, animated.connector_bounds);
        assert_eq!(reduced.input_bounds, reduced.paint_bounds);
    }

    #[test]
    fn reduced_motion_needs_no_animation_timer() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let open = ProductLayout::resolve_reactive(viewport, &mut solver, true, true).unwrap();
        assert_eq!(open.workspace.unwrap().height(), 156.0);
        assert!(!reactive_motion_needs_frame(solver.as_ref(), true, true));
        let closed = ProductLayout::resolve_reactive(viewport, &mut solver, false, true).unwrap();
        assert!(closed.workspace.is_none());
        assert!(!reactive_motion_needs_frame(solver.as_ref(), false, true));
    }

    #[test]
    fn reactive_product_layout_emits_parent_relation_and_shared_child_geometry() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let mut layout =
            ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
        for _ in 0..120 {
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
        }
        let frame = layout.frame(MonitorSceneId::new(1), SceneEpoch::new(1));
        assert!(frame.validate().is_valid());
        assert_eq!(frame.relations.len(), 1);
        assert_eq!(frame.relations[0].from, SceneNodeId::BAR);
        assert_eq!(frame.relations[0].to, SceneNodeId::BODY);
        assert_eq!(
            frame.node(SceneNodeId::BODY).unwrap().envelope,
            layout.reactive.unwrap().paint_bounds
        );
        assert_eq!(
            layout.hit(
                frame.relations[0].connector.first.point(0.5).x(),
                frame.relations[0].connector.first.point(0.5).y()
            ),
            Some(ProductHit::Workspace)
        );
    }

    #[test]
    fn reactive_workspace_contour_excludes_rounded_corners_from_hit_and_input() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let mut layout =
            ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
        for _ in 0..120 {
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
        }
        let workspace = layout.workspace.unwrap();
        let transparent_corner = (workspace.left() + 1.0, workspace.top() + 1.0);
        let frame = layout.frame(MonitorSceneId::new(1), SceneEpoch::new(1));

        assert!(
            !frame.input_regions.iter().any(|region| contains(
                *region,
                transparent_corner.0,
                transparent_corner.1
            )),
            "the transparent rounded corner must not receive Wayland input"
        );
        assert_eq!(
            layout.hit(transparent_corner.0, transparent_corner.1),
            None,
            "product hit testing must agree with the painted contour"
        );
    }

    #[test]
    fn reactive_motion_driver_settles_open_and_closed_without_late_resizing() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let mut solver = None;
        let mut layout =
            ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
        let mut open_frames = 1;
        while reactive_motion_needs_frame(solver.as_ref(), true, false) {
            assert!(open_frames < 120, "opening did not settle in two seconds");
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
            open_frames += 1;
        }
        assert!(
            open_frames > 9,
            "solver settling outlived the old nine-frame driver"
        );
        assert_eq!(layout.workspace.unwrap().height(), 156.0);
        for _ in 0..3 {
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
            assert_eq!(layout.workspace.unwrap().height(), 156.0);
        }
        layout = ProductLayout::resolve_reactive(viewport, &mut solver, false, false).unwrap();
        let mut close_frames = 1;
        while reactive_motion_needs_frame(solver.as_ref(), false, false) {
            assert!(
                close_frames < 120,
                "closing did not settle in two seconds: {solver:?}"
            );
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, false, false).unwrap();
            close_frames += 1;
        }
        assert!(
            close_frames > 9,
            "the new timer must outlive the old nine-frame driver"
        );
        assert!(layout.workspace.is_none());
        for _ in 0..3 {
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, false, false).unwrap();
            assert!(layout.workspace.is_none());
        }
    }

    #[test]
    fn short_output_lets_known_dock_compress_the_workspace_before_collision() {
        let viewport = SurfaceViewport::try_new(1280.0, 400.0).unwrap();
        let mut solver = None;
        let layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, true).unwrap();
        let frame = layout.reactive_frame().unwrap();

        assert!(frame.geometry.height() < 156.0);
        assert!(frame.geometry.bottom() < layout.dock.top() - 12.0);
        assert!(frame.constraint_pressure > 0.0);
        assert!(frame.neighbor_pressure > 0.0);
        assert_eq!(layout.workspace, Some(frame.geometry));
        assert_eq!(frame.input_bounds, frame.paint_bounds);

        let wide = ProductLayout::resolve_reactive(
            SurfaceViewport::try_new(1280.0, 800.0).unwrap(),
            &mut None,
            true,
            true,
        )
        .unwrap();
        let wide_frame = wide.reactive_frame().unwrap();
        assert_eq!(wide_frame.neighbor_pressure, 0.0);
        assert_eq!(wide_frame.geometry.height(), 156.0);

        let severe = ProductLayout::resolve_reactive(
            SurfaceViewport::try_new(1280.0, 240.0).unwrap(),
            &mut None,
            true,
            true,
        )
        .unwrap();
        let severe_frame = severe.reactive_frame().unwrap();
        assert!(severe_frame.neighbor_pressure > 0.0);
        assert!(severe_frame.geometry.bottom() <= severe.dock.top() - 12.0);
        eprintln!(
            "dock-neighbor 800px: height={:.2} pressure={:.3}; 400px: height={:.2} pressure={:.3}; 240px: height={:.2} pressure={:.3}",
            wide_frame.geometry.height(),
            wide_frame.neighbor_pressure,
            frame.geometry.height(),
            frame.neighbor_pressure,
            severe_frame.geometry.height(),
            severe_frame.neighbor_pressure,
        );
    }

    #[test]
    fn short_output_neighbor_response_settles_open_and_closed() {
        let viewport = SurfaceViewport::try_new(1280.0, 400.0).unwrap();
        let mut solver = None;
        let mut layout =
            ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
        let mut open_frames = 1;
        while reactive_motion_needs_frame(solver.as_ref(), true, false) {
            assert!(open_frames < 120, "neighbor opening did not settle");
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
            open_frames += 1;
        }
        let settled_open = layout.workspace.unwrap();
        assert!(settled_open.height() < 156.0);
        assert!(layout.reactive_frame().unwrap().neighbor_pressure > 0.0);
        for _ in 0..3 {
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, true, false).unwrap();
            assert_eq!(layout.workspace, Some(settled_open));
        }

        layout = ProductLayout::resolve_reactive(viewport, &mut solver, false, false).unwrap();
        let mut close_frames = 1;
        while reactive_motion_needs_frame(solver.as_ref(), false, false) {
            assert!(close_frames < 120, "neighbor closure did not settle");
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, false, false).unwrap();
            close_frames += 1;
        }
        assert!(layout.workspace.is_none());
        for _ in 0..3 {
            layout = ProductLayout::resolve_reactive(viewport, &mut solver, false, false).unwrap();
            assert!(layout.workspace.is_none());
        }
    }

    #[test]
    fn one_layout_drives_nodes_inputs_hits_and_reservation() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let layout = ProductLayout::resolve(viewport, 1.0).unwrap();
        let frame = layout.frame(MonitorSceneId::new(1), SceneEpoch::new(1));
        assert!(frame.validate().is_valid());
        assert!(frame.input_regions.len() >= frame.nodes.len());
        assert_eq!(frame.reservation.top, layout.bar.bottom());
        assert_eq!(ProductLayout::WORKSPACE_PARENT, SceneNodeId::BAR);
        assert_eq!(
            layout.hit(
                (layout.bar.left() + layout.bar.right()) * 0.5,
                (layout.bar.top() + layout.bar.bottom()) * 0.5
            ),
            Some(ProductHit::Bar)
        );
        assert_eq!(
            layout.hit(
                layout.workspace.unwrap().left() + 4.0,
                layout.workspace.unwrap().top() + 4.0
            ),
            Some(ProductHit::Workspace)
        );
        assert_eq!(layout.hit(0.0, 200.0), None);
    }
}

#[cfg(test)]
mod panel_state_tests {
    use super::{ProductPanelState, ProductSemanticState};

    /// Every surface, so a new variant cannot be added without a test noticing
    /// that the exclusivity argument below is incomplete.
    const SURFACES: [ProductPanelState; 3] = [
        ProductPanelState::Closed,
        ProductPanelState::WorkspaceSummary,
        ProductPanelState::AppStack,
    ];

    #[test]
    fn the_state_space_is_exactly_the_product_surfaces() {
        // Three states, not a set of flags. A state space that grows by
        // combination is a state space with states the product cannot render.
        assert_eq!(SURFACES.len(), 3);
    }

    #[test]
    fn at_most_one_reaction_surface_is_active() {
        for current in SURFACES {
            for requested in SURFACES {
                let next = current.toggled(requested);
                let active =
                    usize::from(next.is_workspace_summary()) + usize::from(next.is_appstack());
                assert!(
                    active <= 1,
                    "{current:?} + {requested:?} produced {next:?} with {active} surfaces"
                );
            }
        }
    }

    #[test]
    fn opening_a_surface_closes_whatever_was_open() {
        for current in SURFACES {
            for requested in SURFACES {
                let next = current.toggled(requested);
                if requested != ProductPanelState::Closed && next != ProductPanelState::Closed {
                    assert_eq!(
                        next, requested,
                        "{current:?} should have been replaced by {requested:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn activating_the_open_surface_closes_it() {
        for surface in [
            ProductPanelState::WorkspaceSummary,
            ProductPanelState::AppStack,
        ] {
            assert_eq!(surface.toggled(surface), ProductPanelState::Closed);
        }
        assert_eq!(
            ProductPanelState::Closed.toggled(ProductPanelState::Closed),
            ProductPanelState::Closed
        );
    }

    #[test]
    fn a_surface_is_closed_when_its_capability_is_lost() {
        assert_eq!(
            ProductPanelState::AppStack.constrained_by(true, false),
            ProductPanelState::Closed
        );
        assert_eq!(
            ProductPanelState::WorkspaceSummary.constrained_by(false, true),
            ProductPanelState::Closed
        );
        assert_eq!(
            ProductPanelState::AppStack.constrained_by(false, true),
            ProductPanelState::AppStack,
            "the workspace capability is not the appstack's"
        );
    }

    #[test]
    fn the_state_has_stable_evidence_tokens() {
        // These strings are read by qualification tooling; a derived format
        // would let a cosmetic change silently break a parser.
        assert_eq!(ProductPanelState::Closed.as_str(), "CLOSED");
        assert_eq!(
            ProductPanelState::WorkspaceSummary.as_str(),
            "WORKSPACE_SUMMARY"
        );
        assert_eq!(ProductPanelState::AppStack.as_str(), "APPSTACK");
    }

    #[test]
    fn the_appstack_does_not_require_an_active_workspace() {
        // Installed applications are a property of the catalog, not of the
        // workspace the user happens to be on.
        let no_workspace = ProductSemanticState::Current(Box::new(super::ProductContent {
            connector: "wl-1".to_owned(),
            generation: super::DesktopContextGeneration(7),
            active_workspace: None,
            workspace_manager_targets: Vec::new(),
            visible_workspaces: Vec::new(),
            visible_application_count: 0,
            focused_application: None,
            applications: Vec::new(),
            focused_workspace: None,
        }));
        assert!(!no_workspace.can_open_workspace());
        assert!(
            no_workspace.can_open_appstack(),
            "a user with no active workspace still has applications to start"
        );
    }
}

#[cfg(test)]
mod appstack_lineage_tests {
    use super::{
        AppStackLineage, AppStackOrigin, EnvelopeMaterialRelation, ProductLayout,
        ProductPanelState, reactive_motion_needs_frame, reactive_surface_needs_presentation,
        reactive_surfaces_need_frame,
    };
    use gnos_ux_surface::grammar::{GeometryRect, SurfaceViewport};
    use gnos_ux_surface::scene::SceneNodeId;

    fn viewport() -> SurfaceViewport {
        SurfaceViewport::try_new(1920.0, 1080.0).expect("viewport")
    }

    fn dock() -> GeometryRect {
        // The Dock, as the product layout resolves it for a 1920x1080 output.
        let width: f32 = (1920.0_f32 - 32.0).min(520.0);
        let left = (1920.0_f32 - width) * 0.5;
        GeometryRect::try_new(
            left,
            1080.0 - 16.0 - 64.0,
            left + width,
            1080.0 - 16.0,
            12.0,
        )
        .expect("dock")
    }

    #[test]
    fn the_appstack_declares_the_dock_as_its_parent() {
        // Canonical lineage, stated rather than measured. A surface whose parent
        // can only be inferred from geometry is a generic panel.
        assert_eq!(AppStackLineage::DOCK_DERIVED.parent, SceneNodeId::DOCK);
        assert_ne!(AppStackLineage::DOCK_DERIVED.parent, SceneNodeId::BAR);
        assert_ne!(AppStackLineage::DOCK_DERIVED.parent, SceneNodeId::BODY);
    }

    #[test]
    fn the_appstack_inherits_its_parents_material() {
        assert_eq!(
            AppStackLineage::DOCK_DERIVED.material,
            EnvelopeMaterialRelation::InheritWithOverride
        );
        assert_eq!(
            AppStackLineage::DOCK_DERIVED.origin,
            AppStackOrigin::ParentSurfaceHull
        );
    }

    #[test]
    fn the_lineage_is_reportable_without_measuring_geometry() {
        let line = AppStackLineage::DOCK_DERIVED.describe();
        assert!(line.starts_with("DOCK->APPSTACK"), "{line}");
        assert!(line.contains("material=INHERIT_WITH_OVERRIDE"), "{line}");
    }

    #[test]
    fn an_open_appstack_body_is_anchored_to_the_dock() {
        let dock = dock();
        let mut solver = None;
        let mut body = None;
        for _ in 0..400 {
            body = ProductLayout::resolve_appstack_envelope(
                viewport(),
                &mut solver,
                dock,
                true,
                false,
                None,
            )
            .map(|frame| frame.geometry);
        }
        let body = body.expect("a body");
        // Derived from its declared parent: the body sits above the Dock and
        // never inside it. Explicit Fused places its facing edge flush with
        // the parent joint.
        assert!(
            body.top() < dock.top(),
            "the body must sit above the Dock: {:?} vs {:?}",
            body,
            dock
        );
        assert!(
            body.bottom() <= dock.top() + 1.0,
            "the body must not overlap its parent: {:?}",
            body
        );
        // Horizontal centre shared with the parent, because it is the parent's
        // body rather than an unrelated panel.
        let body_center = (body.left() + body.right()) * 0.5;
        let dock_center = (dock.left() + dock.right()) * 0.5;
        assert!(
            (body_center - dock_center).abs() < 1.0,
            "body centre {} vs dock centre {}",
            body_center,
            dock_center
        );
        // Capped, so a long application list does not become a second desktop.
        assert!(body.width() <= 1920.0 - 32.0);
    }

    #[test]
    fn shared_material_contains_the_outer_straight_side_above_the_dock() {
        let frame =
            crate::product_shared_envelope::SharedDockEnvelope::endpoint(viewport(), dock(), true)
                .unwrap();
        let outer = frame.geometry.outer;
        // C has a straight outer side above the Dock, rather than a second
        // narrower body and shoulders. This point is outside historical A.
        assert!(frame.contains_input(outer.left() + 2.0, outer.top() + outer.radius() + 8.0));
    }

    #[test]
    fn a_bottom_dock_produces_an_appstack_body_above_it() {
        // The positive replacement for the old "the extrusion does not exist"
        // sentinel. A Dock on the bottom edge, opened through the same
        // resolver production uses, must yield a real derived body above it.
        let dock = dock();
        let mut solver = None;
        let mut frame = None;
        for _ in 0..400 {
            frame = ProductLayout::resolve_appstack_envelope(
                viewport(),
                &mut solver,
                dock,
                true,
                false,
                None,
            );
        }
        let frame = frame.expect("an open AppStack resolves a derived body");
        assert_eq!(
            frame.attachment,
            gnos_ux_surface::grammar::reactive_envelope::ReactiveAttachment::Fused
        );
        assert!((dock.top() - frame.geometry.bottom()).abs() < 0.001);
        assert_eq!(frame.hull.contour_count(), 1);
        assert!(
            frame.geometry.height() > 1.0,
            "the derived body must be visible: {:?}",
            frame.geometry
        );
        assert!(
            frame.geometry.top() < dock.top(),
            "the AppStack must sit above the Dock: {:?} vs {:?}",
            frame.geometry,
            dock
        );
        assert!(
            frame.geometry.bottom() <= dock.top() + 1.0,
            "the AppStack must not overlap through its parent: {:?}",
            frame.geometry
        );
        assert!(
            !frame.shape_input_regions().is_empty(),
            "a visible derived body must carry input regions"
        );
    }

    #[test]
    fn appstack_motion_keeps_the_shared_driver_armed_until_open_and_close_settle() {
        use std::time::{Duration, Instant};

        fn trace_frame(
            frame: Option<&gnos_ux_surface::grammar::reactive_envelope::ReactiveEnvelopeFrame>,
            dock: GeometryRect,
            target_open: bool,
            step: usize,
            elapsed_ms: f64,
            delta_ms: f64,
        ) {
            if std::env::var_os("GNOS_UX_QUALIFY_APPSTACK_MOTION_TRACE").is_none() {
                return;
            }
            if let Some(frame) = frame {
                let hull = frame.hull.bounds();
                println!(
                    "APPSTACK_MOTION_MODEL_FRAME {{\"surface\":\"AppStack\",\"parent\":\"Dock\",\"growth\":\"Upward\",\"measurement\":\"production resolver with local 16ms sleep schedule; no compositor\",\"target_open\":{},\"resolve\":{},\"elapsed_ms\":{},\"delta_ms\":{},\"openness\":{},\"velocity_y\":{},\"attachment\":\"{:?}\",\"attachment_phase\":{},\"joint_gap\":{},\"connector_width\":{},\"body_bounds\":{:?},\"hull_bounds\":{:?},\"input_region_count\":{}}}",
                    target_open,
                    step,
                    elapsed_ms,
                    delta_ms,
                    frame.openness,
                    frame.velocity_y,
                    frame.attachment,
                    frame.attachment_phase,
                    dock.top() - frame.geometry.bottom(),
                    frame.connector_width,
                    [
                        frame.geometry.left(),
                        frame.geometry.top(),
                        frame.geometry.width(),
                        frame.geometry.height()
                    ],
                    [hull.left(), hull.top(), hull.width(), hull.height()],
                    frame.shape_input_regions().len(),
                );
            }
        }

        let dock = dock();
        let mut solver = None;
        let tick = Duration::from_millis(16);
        let mut baseline_solver = None;
        let mut baseline_frame = None;
        for _ in 0..2 {
            baseline_frame = ProductLayout::resolve_appstack_envelope(
                viewport(),
                &mut baseline_solver,
                dock,
                true,
                false,
                None,
            );
        }
        let baseline_openness = baseline_frame.unwrap().openness;
        assert!(baseline_openness < 1.0);
        let mut frame = ProductLayout::resolve_appstack_envelope(
            viewport(),
            &mut solver,
            dock,
            true,
            false,
            None,
        );
        assert!(frame.is_some());
        // Replaying the old Workspace-only decision: it considers Workspace
        // settled while AppStack has only received its interaction render.
        assert!(
            !reactive_motion_needs_frame(None, false, false),
            "the prior Workspace-only driver predicate goes idle here"
        );
        assert!(
            reactive_surfaces_need_frame(None, false, solver.as_ref(), true, false),
            "AppStack motion must arm the scene driver while Workspace is settled"
        );

        let open_started = Instant::now();
        let mut open_gaps_ms = Vec::new();
        let mut previous_tick = open_started;
        let mut open_steps = 1;
        while reactive_surfaces_need_frame(None, false, solver.as_ref(), true, false)
            && open_steps < 500
        {
            std::thread::sleep(tick);
            let now = Instant::now();
            open_gaps_ms.push(now.duration_since(previous_tick).as_secs_f64() * 1000.0);
            previous_tick = now;
            frame = ProductLayout::resolve_appstack_envelope(
                viewport(),
                &mut solver,
                dock,
                true,
                false,
                None,
            );
            open_steps += 1;
            trace_frame(
                frame.as_ref(),
                dock,
                true,
                open_steps,
                now.duration_since(open_started).as_secs_f64() * 1000.0,
                *open_gaps_ms.last().unwrap(),
            );
        }
        let open_settled_at = Instant::now();
        std::thread::sleep(tick);
        let open_stop_tick = Instant::now();
        open_gaps_ms.push(open_stop_tick.duration_since(previous_tick).as_secs_f64() * 1000.0);
        assert!(open_steps > 1, "opening must be continuously advanced");
        assert!(!reactive_surfaces_need_frame(
            None,
            false,
            solver.as_ref(),
            true,
            false
        ));
        assert_eq!(frame.unwrap().openness, 1.0);

        let close_started = Instant::now();
        let mut close_gaps_ms = Vec::new();
        previous_tick = close_started;
        frame = ProductLayout::resolve_appstack_envelope(
            viewport(),
            &mut solver,
            dock,
            false,
            false,
            None,
        );
        let mut close_steps = 1;
        assert!(reactive_surface_needs_presentation(solver.as_ref(), false));
        while reactive_surfaces_need_frame(None, false, solver.as_ref(), false, false)
            && close_steps < 500
        {
            std::thread::sleep(tick);
            let now = Instant::now();
            close_gaps_ms.push(now.duration_since(previous_tick).as_secs_f64() * 1000.0);
            previous_tick = now;
            frame = ProductLayout::resolve_appstack_envelope(
                viewport(),
                &mut solver,
                dock,
                false,
                false,
                None,
            );
            close_steps += 1;
            trace_frame(
                frame.as_ref(),
                dock,
                false,
                close_steps,
                now.duration_since(close_started).as_secs_f64() * 1000.0,
                *close_gaps_ms.last().unwrap(),
            );
        }
        let close_settled_at = Instant::now();
        std::thread::sleep(tick);
        let close_stop_tick = Instant::now();
        close_gaps_ms.push(close_stop_tick.duration_since(previous_tick).as_secs_f64() * 1000.0);
        assert!(close_steps > 1, "closing must remain driven until settled");
        assert!(!reactive_surfaces_need_frame(
            None,
            false,
            solver.as_ref(),
            false,
            false
        ));
        assert!(!reactive_surface_needs_presentation(solver.as_ref(), false));
        assert!(frame.is_none_or(|frame| frame.openness <= 0.01));
        let mut all_gaps = open_gaps_ms;
        all_gaps.extend(close_gaps_ms);
        all_gaps.sort_by(f64::total_cmp);
        let median_gap_ms = all_gaps[all_gaps.len() / 2];
        let largest_gap_ms = *all_gaps.last().unwrap();
        println!(
            "APPSTACK_MOTION_MODEL {{\"previous_workspace_only_predicate\":{{\"needs_next_frame\":false,\"illustrative_appstack_resolves\":2,\"openness_after_frames\":{:.5},\"settled\":false}},\"timer_ticks_open\":{},\"appstack_resolves_open\":{},\"wall_open_ms\":{:.3},\"timer_ticks_close\":{},\"appstack_resolves_close\":{},\"wall_close_ms\":{:.3},\"median_tick_delta_ms\":{:.3},\"largest_tick_gap_ms\":{:.3},\"settled_open\":true,\"settled_closed\":true,\"driver_predicate\":\"workspace OR appstack\"}}",
            baseline_openness,
            open_steps,
            open_steps,
            open_settled_at.duration_since(open_started).as_secs_f64() * 1000.0,
            close_steps,
            close_steps,
            close_settled_at.duration_since(close_started).as_secs_f64() * 1000.0,
            median_gap_ms,
            largest_gap_ms,
        );
    }

    #[test]
    fn reduced_motion_resolves_appstack_open_and_close_without_driver_frames() {
        let mut solver = None;
        let dock = dock();
        let open = ProductLayout::resolve_appstack_envelope(
            viewport(),
            &mut solver,
            dock,
            true,
            true,
            None,
        )
        .expect("reduced-motion open frame");
        assert_eq!(open.openness, 1.0);
        assert!(!reactive_surfaces_need_frame(
            None,
            false,
            solver.as_ref(),
            true,
            true
        ));
        let close = ProductLayout::resolve_appstack_envelope(
            viewport(),
            &mut solver,
            dock,
            false,
            true,
            None,
        );
        assert!(close.is_none_or(|frame| frame.openness <= 0.01));
        assert!(!reactive_surface_needs_presentation(solver.as_ref(), false));
    }

    #[test]
    fn a_closed_appstack_contributes_no_body() {
        // Closed states contribute no target body. An unsettled AppStack stays
        // presented during retraction through `reactive_surface_needs_presentation`.
        assert!(!ProductPanelState::Closed.is_appstack());
        assert!(!ProductPanelState::WorkspaceSummary.is_appstack());
        assert!(ProductPanelState::AppStack.is_appstack());
    }

    #[test]
    fn the_declared_lineage_and_the_geometry_agree() {
        // §25: a declaration that says Dock while the geometry comes from
        // somewhere else is a failure. The parent handed to the solver is the
        // Dock the product resolved, and the derived body is above that same
        // rectangle.
        assert_eq!(AppStackLineage::DOCK_DERIVED.parent, SceneNodeId::DOCK);
        let dock = dock();
        let mut solver = None;
        let mut frame = None;
        for _ in 0..400 {
            frame = ProductLayout::resolve_appstack_envelope(
                viewport(),
                &mut solver,
                dock,
                true,
                false,
                None,
            );
        }
        let frame = frame.expect("derived body");
        // The connector joint is the parent's own edge, which is only true if the
        // solver's parent really was this Dock.
        assert!(
            (frame.connector_bounds.bottom() - dock.top()).abs() < 2.0
                || (frame.connector_bounds.top() - dock.top()).abs() < 2.0,
            "the joint must sit on the Dock's top edge: {:?} vs {:?}",
            frame.connector_bounds,
            dock
        );
    }

    #[test]
    fn the_appstack_is_not_composed_from_the_bar() {
        // The two product body modes have different parents. Sharing one parent
        // would make the AppStack a second workspace summary.
        let bar = GeometryRect::try_new(580.0, 16.0, 1340.0, 72.0, 12.0).expect("bar");
        assert_ne!(AppStackLineage::DOCK_DERIVED.parent, SceneNodeId::BAR);
        // The Dock and the Bar are different rectangles, so a solver parented to
        // one cannot be mistaken for a solver parented to the other.
        assert_ne!(bar.left(), dock().left());
    }

    #[test]
    fn the_appstack_node_is_not_the_internal_body_slot() {
        // `Body` is an internal composition node. The product identity is
        // AppStack, and the scene graph has to be able to say which is which.
        assert_ne!(SceneNodeId::APPSTACK, SceneNodeId::BODY);
        assert_ne!(
            gnos_ux_surface::scene::SceneNodeRole::AppStack,
            gnos_ux_surface::scene::SceneNodeRole::Body
        );
    }
}

#[cfg(test)]
mod bar_placement_tests {
    use super::*;
    use gnos_ux_surface::{
        compositor::ResolvedCompositorIntent, pointer_regions::project_resolved_bar_pointer_regions,
    };

    #[test]
    fn four_edge_frames_share_bar_identity_epoch_input_and_reservation() {
        let viewport = SurfaceViewport::try_new(1200., 720.).unwrap();
        let default = ProductLayout::resolve(viewport, 0.).unwrap();
        for edge in [
            SurfaceEdge::Top,
            SurfaceEdge::Right,
            SurfaceEdge::Bottom,
            SurfaceEdge::Left,
        ] {
            let layout = ProductLayout::resolve_with_bar_placement(
                viewport,
                0.,
                BarPlacement {
                    edge,
                    along_edge_alignment: AlongEdgeAlignment::Center,
                    thickness: 56.,
                    span: 300.,
                    inset: 16.,
                    reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
                    sensing_policy: BarSensingPolicy::EdgeStrip { thickness: 8. },
                },
            )
            .unwrap();
            let frame = layout.frame(MonitorSceneId::new(42), SceneEpoch::new(19));
            let bar = frame
                .nodes
                .iter()
                .find(|node| node.id == SceneNodeId::BAR)
                .unwrap();
            assert_eq!(bar.role, SceneNodeRole::Bar);
            assert_eq!(bar.envelope, layout.bar_placement.allocation());
            assert_eq!(frame.epoch, SceneEpoch::new(19));
            assert_eq!(frame.scene_id, MonitorSceneId::new(42));
            let pointer =
                project_resolved_bar_pointer_regions(viewport, layout.bar_placement).unwrap();
            for region in pointer.regions() {
                assert!(frame.input_regions.contains(&region.bounds()));
            }
            assert_eq!(frame.reservation, layout.bar_placement.reservation());
            let compositor = ResolvedCompositorIntent::try_from_scene_frame(&frame).unwrap();
            assert_eq!(compositor.edge, edge);
            assert_eq!(compositor.source.scene_epoch, frame.epoch);
            let plan = gnosix_hyprland::surface_reservation_plan(&compositor).unwrap();
            assert_eq!(plan.edge, edge);
            assert_eq!(plan.exclusive_zone, 72);
            assert_eq!(plan.source.scene_epoch, frame.epoch);
            if edge == SurfaceEdge::Bottom {
                assert!(layout.dock.bottom() + 12.0 <= layout.bar.top());
            } else {
                assert_eq!(layout.dock, default.dock);
            }
            assert!(frame.validate().is_valid());
        }
    }

    #[test]
    fn default_bar_and_reservation_remain_equivalent_to_qualified_formula() {
        for (w, h) in [
            (1920., 1080.),
            (1080., 1920.),
            (1200., 720.),
            (720., 720.),
            (900., 560.),
        ] {
            let viewport = SurfaceViewport::try_new(w, h).unwrap();
            let layout = ProductLayout::resolve(viewport, 1.).unwrap();
            let radius = ResponsiveRadiusToken::candidate_default()
                .resolve(viewport)
                .unwrap();
            // Independent historical oracle from the qualified pre-DONOR-05 formula.
            let width = (w - 32.).min(760.);
            let height = 56.0_f32.max(radius * 2.);
            let old = GeometryRect::try_new(
                (w - width) / 2.,
                16.,
                (w + width) / 2.,
                16. + height,
                radius,
            )
            .unwrap();
            assert_eq!(layout.bar, old);
            assert_eq!(layout.bar_placement.allocation(), old);
            let frame = layout.frame(MonitorSceneId::new(1), SceneEpoch::new(3));
            assert_eq!(frame.reservation.top, old.bottom());
            assert_eq!(
                [
                    frame.reservation.right,
                    frame.reservation.bottom,
                    frame.reservation.left
                ],
                [0.; 3]
            );
        }
    }

    #[test]
    fn bottom_overlay_bar_does_not_displace_the_dock() {
        let viewport = SurfaceViewport::try_new(900., 560.).unwrap();
        let base = ProductLayout::resolve(viewport, 0.).unwrap();
        let overlay = ProductLayout::resolve_with_bar_placement(
            viewport,
            0.,
            BarPlacement {
                edge: SurfaceEdge::Bottom,
                along_edge_alignment: AlongEdgeAlignment::Center,
                thickness: 56.,
                span: 500.,
                inset: 16.,
                reservation_policy: BarReservationPolicy::Overlay,
                sensing_policy: BarSensingPolicy::EdgeStrip { thickness: 16. },
            },
        )
        .unwrap();

        assert_eq!(overlay.dock, base.dock);
        assert_eq!(
            overlay
                .frame(MonitorSceneId::new(1), SceneEpoch::new(1))
                .reservation,
            gnos_ux_surface::scene::SceneReservationIntent::NONE
        );
    }

    #[test]
    fn overlay_bar_changes_no_dock_or_appstack_geometry() {
        let viewport = SurfaceViewport::try_new(900., 560.).unwrap();
        let base = ProductLayout::resolve(viewport, 0.).unwrap();
        let moved = ProductLayout::resolve_with_bar_placement(
            viewport,
            0.,
            BarPlacement {
                edge: SurfaceEdge::Left,
                along_edge_alignment: AlongEdgeAlignment::Start,
                thickness: 30.,
                span: 200.,
                inset: 0.,
                reservation_policy: BarReservationPolicy::Overlay,
                sensing_policy: BarSensingPolicy::None,
            },
        )
        .unwrap();
        assert_eq!(base.dock, moved.dock);
        assert!(moved.appstack.is_none());
        assert!(moved.shared_dock.is_none());
        assert_eq!(
            moved
                .frame(MonitorSceneId::new(1), SceneEpoch::new(1))
                .reservation,
            gnos_ux_surface::scene::SceneReservationIntent::NONE
        );
    }

    #[test]
    fn transition_frame_uses_one_epoch_for_bar_input_and_reservation() {
        use gnos_ux_surface::grammar::{BarPlacementTransition, UnitInterval};
        let viewport = SurfaceViewport::try_new(1280., 800.).unwrap();
        let source_layout = ProductLayout::resolve(viewport, 0.).unwrap();
        let target_layout = ProductLayout::resolve_with_bar_placement(
            viewport,
            0.,
            BarPlacement {
                edge: SurfaceEdge::Right,
                along_edge_alignment: AlongEdgeAlignment::Center,
                thickness: 56.,
                span: 480.,
                inset: 16.,
                reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
                sensing_policy: BarSensingPolicy::None,
            },
        )
        .unwrap();
        let transition = BarPlacementTransition::new(
            viewport,
            source_layout.bar_placement,
            target_layout.bar_placement,
        )
        .unwrap();
        let evaluated = transition
            .evaluate(UnitInterval::try_new(0.73).unwrap())
            .unwrap();
        let frame = target_layout.frame_with_bar_transition(
            MonitorSceneId::new(18),
            SceneEpoch::new(77),
            &evaluated,
        );
        let bar = frame.node(SceneNodeId::BAR).unwrap();
        assert_eq!(bar.role, SceneNodeRole::Bar);
        assert_eq!(bar.envelope, evaluated.allocation);
        assert_eq!(frame.epoch, SceneEpoch::new(77));
        assert_eq!(frame.reservation, evaluated.reservation);
        for region in evaluated.input_regions() {
            assert!(frame.input_regions.contains(&region));
        }
        let old_source_input =
            project_resolved_bar_pointer_regions(viewport, source_layout.bar_placement).unwrap();
        for region in old_source_input.regions() {
            if !evaluated.input_regions().contains(&region.bounds()) {
                assert!(
                    !frame.input_regions.contains(&region.bounds()),
                    "stale source endpoint input is absent"
                );
            }
        }
        assert!(frame.validate().is_valid());
    }

    #[test]
    fn right_to_bottom_transition_uses_transient_bar_occupancy_for_dock_clearance() {
        use gnos_ux_surface::grammar::{BarPlacementTransition, UnitInterval};

        fn overlaps(a: GeometryRect, b: GeometryRect) -> bool {
            a.left() < b.right()
                && a.right() > b.left()
                && a.top() < b.bottom()
                && a.bottom() > b.top()
        }

        // This matches DP-2's rotated logical viewport and the retained
        // cadence-final frame at elapsed_ms=5892 (Right -> Bottom, p ~= .964).
        let viewport = SurfaceViewport::try_new(1080.0, 1920.0).unwrap();
        let right = ProductLayout::resolve_reactive_at_edge(
            viewport,
            &mut None,
            false,
            true,
            None,
            SurfaceEdge::Right,
            16.0,
        )
        .unwrap();
        let bottom = ProductLayout::resolve_reactive_at_edge(
            viewport,
            &mut None,
            false,
            true,
            None,
            SurfaceEdge::Bottom,
            16.0,
        )
        .unwrap();
        let transition =
            BarPlacementTransition::new(viewport, right.bar_placement, bottom.bar_placement)
                .unwrap();
        let evaluated = transition
            .evaluate(UnitInterval::try_new(0.964).unwrap())
            .unwrap();
        let mut layout = right;
        let mut shared = crate::product_shared_envelope::SharedDockEnvelope::resolve_with_safe(
            viewport,
            layout.dock,
            layout.dock,
            crate::product_shared_envelope::SharedMotion::endpoint(false),
            layout.dock_safe_bounds().unwrap(),
        )
        .unwrap();
        while layout
            .constrain_dock_for_bar_transition(&evaluated, shared.geometry.outer)
            .unwrap()
        {
            shared = crate::product_shared_envelope::SharedDockEnvelope::resolve_with_safe(
                viewport,
                layout.dock,
                layout.dock,
                crate::product_shared_envelope::SharedMotion::endpoint(false),
                layout.dock_safe_bounds().unwrap(),
            )
            .unwrap();
        }
        layout.shared_dock = Some(shared);
        let frame = layout.frame_with_bar_transition(
            MonitorSceneId::new(7),
            SceneEpoch::new(355),
            &evaluated,
        );
        assert_eq!(evaluated.reservation_edge, SurfaceEdge::Bottom);
        assert!(
            evaluated
                .arms
                .iter()
                .all(|arm| !overlaps(arm.bounds, layout.dock)),
            "painted Bar arms must clear the Dock in the same evaluated transition frame"
        );
        assert!(
            evaluated.input_regions().iter().all(|region| {
                shared
                    .input_regions()
                    .iter()
                    .all(|dock_input| !overlaps(*region, *dock_input))
            }),
            "Bar input regions must clear the Dock in the same evaluated transition frame"
        );
        assert_eq!(frame.epoch, SceneEpoch::new(355));
        assert!(frame.validate().is_valid());
    }

    #[test]
    fn bottom_entry_and_exit_transitions_keep_paint_and_input_clear_of_dock() {
        use gnos_ux_surface::grammar::{BarPlacementTransition, UnitInterval};

        fn overlaps(a: GeometryRect, b: GeometryRect) -> bool {
            a.left() < b.right()
                && a.right() > b.left()
                && a.top() < b.bottom()
                && a.bottom() > b.top()
        }

        fn make_shared(
            layout: &ProductLayout,
            viewport: SurfaceViewport,
            open: bool,
        ) -> crate::product_shared_envelope::SharedDockEnvelope {
            crate::product_shared_envelope::SharedDockEnvelope::resolve_with_safe(
                viewport,
                layout.dock,
                layout.dock,
                crate::product_shared_envelope::SharedMotion::endpoint(open),
                layout.dock_safe_bounds().unwrap(),
            )
            .unwrap()
        }

        let mut collisions = Vec::new();
        for (width, height) in [(1080.0, 1920.0), (2560.0, 1440.0)] {
            let viewport = SurfaceViewport::try_new(width, height).unwrap();
            for (source_edge, target_edge) in [
                (SurfaceEdge::Top, SurfaceEdge::Bottom),
                (SurfaceEdge::Right, SurfaceEdge::Bottom),
                (SurfaceEdge::Left, SurfaceEdge::Bottom),
                (SurfaceEdge::Bottom, SurfaceEdge::Top),
                (SurfaceEdge::Bottom, SurfaceEdge::Right),
                (SurfaceEdge::Bottom, SurfaceEdge::Left),
            ] {
                let source = ProductLayout::resolve_reactive_at_edge(
                    viewport,
                    &mut None,
                    false,
                    true,
                    None,
                    source_edge,
                    16.0,
                )
                .unwrap();
                let target = ProductLayout::resolve_reactive_at_edge(
                    viewport,
                    &mut None,
                    false,
                    true,
                    None,
                    target_edge,
                    16.0,
                )
                .unwrap();
                let transition = BarPlacementTransition::new(
                    viewport,
                    source.bar_placement,
                    target.bar_placement,
                )
                .unwrap();
                for appstack_open in [false, true] {
                    for step in 0..1000 {
                        let evaluated = transition
                            .evaluate(UnitInterval::try_new(step as f32 / 1000.0).unwrap())
                            .unwrap();
                        let mut layout = source.clone();
                        let mut shared_layout = make_shared(&layout, viewport, appstack_open);
                        let mut converged = false;
                        for _ in 0..8 {
                            match layout.constrain_dock_for_bar_transition(
                                &evaluated,
                                shared_layout.geometry.outer,
                            ) {
                                Some(true) => {
                                    shared_layout = make_shared(&layout, viewport, appstack_open);
                                }
                                Some(false) => {
                                    converged = true;
                                    break;
                                }
                                None => break,
                            }
                        }
                        layout.shared_dock = Some(shared_layout);
                        let frame = layout.frame_with_bar_transition(
                            MonitorSceneId::new(4),
                            SceneEpoch::new(step + 1),
                            &evaluated,
                        );
                        let dock_bounds = frame.node(SceneNodeId::DOCK).unwrap().envelope;
                        let appstack_bounds =
                            frame.node(SceneNodeId::APPSTACK).map(|node| node.envelope);
                        let paint_overlap = evaluated.arms.iter().any(|arm| {
                            overlaps(arm.bounds, dock_bounds)
                                || appstack_bounds
                                    .is_some_and(|bounds| overlaps(arm.bounds, bounds))
                        });
                        let input_overlap = evaluated.input_regions().iter().any(|bar_input| {
                            shared_layout
                                .input_regions()
                                .iter()
                                .any(|dock_input| overlaps(*bar_input, *dock_input))
                        });
                        if !converged
                            || paint_overlap
                            || input_overlap
                            || !frame.validate().is_valid()
                        {
                            collisions.push((
                                width,
                                height,
                                source_edge,
                                target_edge,
                                appstack_open,
                                step,
                                converged,
                                paint_overlap,
                                input_overlap,
                                frame.validate().is_valid(),
                            ));
                            break;
                        }
                    }
                }
            }
        }
        assert!(
            collisions.is_empty(),
            "transient Bar paint/input must not intersect the bottom Dock/AppStack owner: {collisions:?}"
        );
    }
}

#[cfg(test)]
mod visible_bar_tests {
    use super::*;
    #[test]
    fn bottom_bar_gives_the_bottom_specific_dock_disjoint_space() {
        let viewport = SurfaceViewport::try_new(1280., 800.).unwrap();
        let layout = ProductLayout::resolve_with_bar_placement(
            viewport,
            0.,
            BarPlacement {
                edge: SurfaceEdge::Bottom,
                along_edge_alignment: AlongEdgeAlignment::Center,
                thickness: 56.,
                span: 760.,
                inset: 16.,
                reservation_policy: BarReservationPolicy::ReserveOccupiedExtent,
                sensing_policy: BarSensingPolicy::None,
            },
        )
        .unwrap();
        assert!(layout.dock.bottom() + 12. <= layout.bar.top());
    }
}

#[cfg(test)]
mod bar_corner_tests {
    use super::*;
    #[test]
    fn transparent_bar_corner_is_not_an_actionable_hit() {
        let layout =
            ProductLayout::resolve(SurfaceViewport::try_new(1280., 800.).unwrap(), 0.).unwrap();
        assert_eq!(
            layout.hit(layout.bar.left() + 1., layout.bar.top() + 1.),
            None
        );
        assert_eq!(
            layout.hit(
                (layout.bar.left() + layout.bar.right()) / 2.,
                (layout.bar.top() + layout.bar.bottom()) / 2.
            ),
            Some(ProductHit::Bar)
        );
    }
}

#[cfg(test)]
mod cardinal_product_tests {
    use super::*;
    fn overlaps(a: GeometryRect, b: GeometryRect) -> bool {
        a.left() < b.right() && a.right() > b.left() && a.top() < b.bottom() && a.bottom() > b.top()
    }
    #[test]
    fn every_visible_endpoint_keeps_workspace_and_shared_appstack_disjoint() {
        for (w, h) in [(1280., 800.), (800., 1280.), (720., 720.)] {
            let viewport = SurfaceViewport::try_new(w, h).unwrap();
            for edge in [
                SurfaceEdge::Top,
                SurfaceEdge::Right,
                SurfaceEdge::Bottom,
                SurfaceEdge::Left,
            ] {
                for workspace_open in [false, true] {
                    let mut layout = ProductLayout::resolve_reactive_at_edge(
                        viewport,
                        &mut None,
                        workspace_open,
                        true,
                        None,
                        edge,
                        14.,
                    )
                    .unwrap();
                    let shared=crate::product_shared_envelope::SharedDockEnvelope::resolve_with_safe(viewport,layout.dock,layout.dock,crate::product_shared_envelope::SharedMotion::endpoint(!workspace_open),layout.dock_safe_bounds().unwrap()).unwrap_or_else(|error|panic!("{w}x{h} {edge:?} open={workspace_open} dock={:?} safe={:?} {error:?}",layout.dock,layout.dock_safe_bounds()));
                    assert!(
                        !overlaps(layout.bar, shared.geometry.outer),
                        "{w}x{h} {edge:?} bar vs Dock/AppStack"
                    );
                    if let Some(workspace) = layout.workspace {
                        assert!(
                            !overlaps(workspace, layout.dock),
                            "{w}x{h} {edge:?} workspace vs Dock"
                        );
                        match edge {
                            SurfaceEdge::Top => assert!(workspace.top() >= layout.bar.bottom()),
                            SurfaceEdge::Right => assert!(workspace.right() <= layout.bar.left()),
                            SurfaceEdge::Bottom => assert!(workspace.bottom() <= layout.bar.top()),
                            SurfaceEdge::Left => assert!(workspace.left() >= layout.bar.right()),
                        }
                    }
                    if !workspace_open {
                        layout.shared_dock = Some(shared);
                    }
                    let frame = layout.frame(MonitorSceneId::new(5), SceneEpoch::new(6));
                    assert_eq!(frame.epoch, SceneEpoch::new(6));
                    assert!(frame.validate().is_valid());
                    assert_eq!(gnos_ux_surface::compositor::ResolvedCompositorIntent::try_from_scene_frame(&frame).unwrap().edge,edge);
                }
            }
        }
    }
}
