//! DSC-01: the desktop semantic consumer's per-output projections, as
//! published to each scene and read back as context text.

use super::*;

#[derive(Clone, Default)]
pub(super) struct SceneProjectionState {
    pub(super) projections: Arc<Mutex<BTreeMap<ConnectorName, DesktopOutputProjection>>>,
    pub(super) revision: Arc<AtomicU64>,
    pub(super) subscribers: Arc<Mutex<Vec<async_mpsc::UnboundedSender<u64>>>>,
}

impl SceneProjectionState {
    pub(super) fn clear(&self) -> Result<(), String> {
        let mut subscribers = self
            .subscribers
            .lock()
            .map_err(|_| "scene_projection_subscribers_poisoned".to_owned())?;
        let mut projections = self
            .projections
            .lock()
            .map_err(|_| "scene_projection_state_poisoned".to_owned())?;
        projections.clear();
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        drop(projections);
        subscribers.retain(|subscriber| subscriber.unbounded_send(revision).is_ok());
        Ok(())
    }

    pub(super) fn publish(
        &self,
        projections: BTreeMap<ConnectorName, DesktopOutputProjection>,
    ) -> Result<(), String> {
        let mut subscribers = self
            .subscribers
            .lock()
            .map_err(|_| "scene_projection_subscribers_poisoned".to_owned())?;
        let mut current = self
            .projections
            .lock()
            .map_err(|_| "scene_projection_state_poisoned".to_owned())?;
        *current = projections;
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        drop(current);
        subscribers.retain(|subscriber| subscriber.unbounded_send(revision).is_ok());
        Ok(())
    }

    pub(super) fn get(&self, output: Option<&RuntimeId>) -> Option<DesktopOutputProjection> {
        let connector = ConnectorName::new(output?.as_str().to_owned()).ok()?;
        self.projections.lock().ok()?.get(&connector).cloned()
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub(super) fn subscribe(&self) -> Result<async_mpsc::UnboundedReceiver<u64>, String> {
        let (sender, receiver) = async_mpsc::unbounded();
        self.subscribers
            .lock()
            .map_err(|_| "scene_projection_subscribers_poisoned".to_owned())?
            .push(sender);
        Ok(receiver)
    }
}

#[derive(Clone, Default)]
pub(super) struct SceneSemanticConsumer {
    pub(super) enabled: bool,
    pub(super) state: SceneProjectionState,
}

impl SceneSemanticConsumer {
    pub(super) fn enabled() -> Self {
        Self {
            enabled: true,
            state: SceneProjectionState::default(),
        }
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn clear(&self) -> Result<(), String> {
        if self.enabled {
            self.state.clear()?;
        }
        Ok(())
    }

    pub(super) fn publish(
        &self,
        projections: BTreeMap<ConnectorName, DesktopOutputProjection>,
    ) -> Result<(), String> {
        if self.enabled {
            self.state.publish(projections)?;
        }
        Ok(())
    }

    pub(super) fn get(&self, output: Option<&RuntimeId>) -> Option<DesktopOutputProjection> {
        if self.enabled {
            self.state.get(output)
        } else {
            None
        }
    }

    pub(super) fn revision(&self) -> u64 {
        self.state.revision()
    }

    pub(super) fn subscribe(&self) -> Result<async_mpsc::UnboundedReceiver<u64>, String> {
        self.state.subscribe()
    }
}

pub(super) fn pair_projections_by_connector(
    route_connectors: impl IntoIterator<Item = ConnectorName>,
    projections: &[DesktopOutputProjection],
    semantic_generation: DesktopContextGeneration,
) -> Result<BTreeMap<ConnectorName, DesktopOutputProjection>, String> {
    let mut routes = BTreeMap::new();
    for connector in route_connectors {
        if routes.insert(connector.clone(), ()).is_some() {
            return Err(format!("duplicate_route_connector:{}", connector.as_str()));
        }
    }

    let mut by_connector = BTreeMap::new();
    for projection in projections {
        if projection.generation != semantic_generation {
            return Err(format!(
                "projection_generation_mismatch:expected={} observed={}",
                semantic_generation.0, projection.generation.0
            ));
        }
        let connector = projection.output.connector.clone();
        if by_connector
            .insert(connector.clone(), projection.clone())
            .is_some()
        {
            return Err(format!(
                "duplicate_projection_connector:{}",
                connector.as_str()
            ));
        }
    }

    if routes.len() != by_connector.len() {
        return Err(format!(
            "route_projection_count_mismatch:routes={} projections={}",
            routes.len(),
            by_connector.len()
        ));
    }

    let mut paired = BTreeMap::new();
    for connector in routes.keys() {
        let projection = by_connector
            .remove(connector)
            .ok_or_else(|| format!("missing_projection_for_route:{}", connector.as_str()))?;
        paired.insert(connector.clone(), projection);
    }
    if !by_connector.is_empty() {
        return Err(format!(
            "unpaired_projection_connectors:{}",
            by_connector
                .keys()
                .map(ConnectorName::as_str)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    Ok(paired)
}

pub(super) fn desktop_consumer_context_text(
    projection: Option<&DesktopOutputProjection>,
) -> String {
    let Some(view) = projection else {
        return "DSC01 semantic=unavailable".to_owned();
    };
    let active_workspace = view
        .output
        .active_workspace_id
        .and_then(|id| {
            view.workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == id)
        })
        .map(|workspace| workspace.name.as_str())
        .unwrap_or("none");
    let focused_workspace = view
        .workspaces
        .iter()
        .find(|workspace| workspace.focus == WorkspaceFocus::Focused)
        .map(|workspace| workspace.name.as_str())
        .unwrap_or("none");
    let visible_workspaces = view
        .workspaces
        .iter()
        .filter(|workspace| workspace.visibility == WorkspaceVisibility::Visible)
        .map(|workspace| workspace.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let focused_application = view
        .surfaces
        .iter()
        .find(|surface| {
            surface.role == DesktopSurfaceRole::Application
                && surface.focus == SurfaceFocus::Focused
        })
        .and_then(|surface| surface.application.as_ref())
        .map(|application| application.class.as_str())
        .unwrap_or("none");
    let application_surfaces = view
        .surfaces
        .iter()
        .filter(|surface| surface.role == DesktopSurfaceRole::Application)
        .collect::<Vec<_>>();
    let applications = application_surfaces.len();
    let application_hidden = application_surfaces
        .iter()
        .filter(|surface| surface.visibility == SurfaceVisibility::Hidden)
        .count();
    let application_unmapped = application_surfaces
        .iter()
        .filter(|surface| surface.visibility == SurfaceVisibility::Unmapped)
        .count();
    format!(
        "DSC01 connector={} semantic_generation={} active_workspace={} focused_workspace={} visible_workspaces={} focused_application={} applications={} application_hidden={} application_unmapped={}",
        view.output.connector.as_str(),
        view.generation.0,
        active_workspace,
        focused_workspace,
        if visible_workspaces.is_empty() {
            "none"
        } else {
            visible_workspaces.as_str()
        },
        focused_application,
        applications,
        application_hidden,
        application_unmapped
    )
}
