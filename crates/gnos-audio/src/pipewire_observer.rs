use crate::{
    AudioEndpointId, AudioEndpointKind, AudioEndpointSnapshot, AudioModelError, AudioSnapshot,
    AudioState, AudioVolume, DefaultRoleKind, NativeVolume, parse_default_node_name,
};
use pipewire as pw;
use pw::proxy::{Listener, ProxyT};
use pw::spa::pod::{Value, ValueArray, deserialize::PodDeserializer};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

type PublishSink = Rc<RefCell<Box<dyn FnMut(AudioSnapshot)>>>;

#[derive(Clone, Debug, Default, PartialEq)]
struct PropsPatch {
    scalar: Option<NativeVolume>,
    channels: Option<Vec<NativeVolume>>,
    soft_channels: Option<Vec<NativeVolume>>,
    muted: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
struct RoutePatch {
    profile_device: i32,
    patch: PropsPatch,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioObserverError {
    #[error("PipeWire error: {0}")]
    PipeWire(#[from] pw::Error),
    #[error("PipeWire core did not publish its server cookie")]
    MissingServerCookie,
}

#[derive(Clone, Debug)]
struct NodeAccumulator {
    id: AudioEndpointId,
    kind: AudioEndpointKind,
    node_name: String,
    display_name: String,
    device_global_id: Option<u32>,
    profile_device: Option<i32>,
    scalar: Option<NativeVolume>,
    channels: Option<Vec<NativeVolume>>,
    soft_channels: Option<Vec<NativeVolume>>,
    muted: Option<bool>,
}

impl NodeAccumulator {
    fn route_managed(&self) -> bool {
        self.device_global_id.is_some() && self.profile_device.is_some()
    }

    fn apply_node_patch(&mut self, patch: PropsPatch) -> Option<AudioEndpointSnapshot> {
        if self.route_managed() {
            return None;
        }
        self.apply_patch(patch)
    }

    fn apply_route_patch(
        &mut self,
        profile_device: i32,
        patch: PropsPatch,
    ) -> Option<AudioEndpointSnapshot> {
        if self.profile_device != Some(profile_device) {
            return None;
        }
        self.apply_patch(patch)
    }

    fn apply_patch(&mut self, patch: PropsPatch) -> Option<AudioEndpointSnapshot> {
        if patch.scalar.is_some() {
            self.scalar = patch.scalar;
        }
        if patch.channels.is_some() {
            self.channels = patch.channels;
        }
        if patch.soft_channels.is_some() {
            self.soft_channels = patch.soft_channels;
        }
        if patch.muted.is_some() {
            self.muted = patch.muted;
        }
        let muted = self.muted?;
        if self.scalar.is_none()
            && self.channels.as_ref().is_none_or(Vec::is_empty)
            && self.soft_channels.as_ref().is_none_or(Vec::is_empty)
        {
            return None;
        }
        Some(AudioEndpointSnapshot::new(
            self.id,
            self.kind,
            self.node_name.clone(),
            self.display_name.clone(),
            AudioVolume::from_parts(
                self.scalar,
                self.channels.clone().unwrap_or_default(),
                self.soft_channels.clone().unwrap_or_default(),
            ),
            muted,
        ))
    }
}

struct ObserverRuntime {
    audio: AudioState,
    nodes: HashMap<u32, NodeAccumulator>,
    routes: HashMap<(u32, i32), PropsPatch>,
}

impl ObserverRuntime {
    fn new(server_cookie: u32) -> Self {
        Self {
            audio: AudioState::new(server_cookie),
            nodes: HashMap::new(),
            routes: HashMap::new(),
        }
    }

    fn insert_node(&mut self, global_id: u32, mut node: NodeAccumulator) -> bool {
        let endpoint = match (node.device_global_id, node.profile_device) {
            (Some(device_id), Some(profile_device)) => self
                .routes
                .get(&(device_id, profile_device))
                .cloned()
                .and_then(|patch| node.apply_route_patch(profile_device, patch)),
            _ => None,
        };
        self.nodes.insert(global_id, node);
        endpoint.is_some_and(|endpoint| self.audio.upsert_endpoint(endpoint))
    }

    fn bind_node_route(
        &mut self,
        global_id: u32,
        device_global_id: u32,
        profile_device: i32,
    ) -> bool {
        let Some(node) = self.nodes.get_mut(&global_id) else {
            return false;
        };
        node.device_global_id = Some(device_global_id);
        node.profile_device = Some(profile_device);
        let endpoint = self
            .routes
            .get(&(device_global_id, profile_device))
            .cloned()
            .and_then(|patch| node.apply_route_patch(profile_device, patch));
        endpoint.is_some_and(|endpoint| self.audio.upsert_endpoint(endpoint))
    }

    fn apply_route(&mut self, device_global_id: u32, route: RoutePatch) -> bool {
        self.routes.insert(
            (device_global_id, route.profile_device),
            route.patch.clone(),
        );
        let endpoints = self
            .nodes
            .values_mut()
            .filter(|node| {
                node.device_global_id == Some(device_global_id)
                    && node.profile_device == Some(route.profile_device)
            })
            .filter_map(|node| node.apply_route_patch(route.profile_device, route.patch.clone()))
            .collect::<Vec<_>>();
        let mut changed = false;
        for endpoint in endpoints {
            changed |= self.audio.upsert_endpoint(endpoint);
        }
        changed
    }
}

struct BoundObject {
    _proxy: Option<Box<dyn ProxyT>>,
    _device: Option<Rc<pw::device::Device>>,
    _listeners: Vec<Box<dyn Listener>>,
    endpoint_id: Option<AudioEndpointId>,
    default_metadata: bool,
}

pub struct PipeWireObserver;

impl PipeWireObserver {
    /// Standalone observer on the process main thread: also quits its loop on
    /// SIGINT/SIGTERM.
    pub fn run<F>(publish: F) -> Result<(), AudioObserverError>
    where
        F: FnMut(AudioSnapshot) + 'static,
    {
        Self::run_with(publish, true)
    }

    /// Observer embedded in a host process on a worker thread. Process signals
    /// belong to the host; pipewire-rs also only allows signal sources on the
    /// thread named `main`, so none are installed here.
    pub fn run_embedded<F>(publish: F) -> Result<(), AudioObserverError>
    where
        F: FnMut(AudioSnapshot) + 'static,
    {
        Self::run_with(publish, false)
    }

    fn run_with<F>(publish: F, quit_on_signals: bool) -> Result<(), AudioObserverError>
    where
        F: FnMut(AudioSnapshot) + 'static,
    {
        pw::init();
        let main_loop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&main_loop, None)?;
        let core = context.connect_rc(None)?;
        let server_cookie = initialize_server_cookie(&main_loop, &core)?;
        let runtime = Rc::new(RefCell::new(ObserverRuntime::new(server_cookie)));
        let publish: PublishSink = Rc::new(RefCell::new(Box::new(publish)));
        run_registry(main_loop, core, runtime, publish, quit_on_signals)
    }
}

pub(crate) fn initialize_server_cookie(
    main_loop: &pw::main_loop::MainLoopRc,
    core: &pw::core::CoreRc,
) -> Result<u32, AudioObserverError> {
    let cookie = Rc::new(Cell::new(None));
    let cookie_for_info = cookie.clone();
    let loop_weak = main_loop.downgrade();
    let listener = core
        .add_listener_local()
        .info(move |info| cookie_for_info.set(Some(info.cookie())))
        .done(move |_id, _seq| {
            if let Some(main_loop) = loop_weak.upgrade() {
                main_loop.quit();
            }
        })
        .register();
    core.sync(0)?;
    main_loop.run();
    drop(listener);
    cookie.get().ok_or(AudioObserverError::MissingServerCookie)
}

fn publish_snapshot(runtime: &Rc<RefCell<ObserverRuntime>>, publish: &PublishSink) {
    let snapshot = runtime.borrow().audio.snapshot();
    (publish.borrow_mut())(snapshot);
}

fn node_accumulator_from_global(
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
    server_cookie: u32,
) -> Option<NodeAccumulator> {
    if global.type_ != pw::types::ObjectType::Node {
        return None;
    }
    let props = global.props?;
    let kind = classify_media_class(props.get(*pw::keys::MEDIA_CLASS)?)?;
    let object_serial = props.get(*pw::keys::OBJECT_SERIAL)?.parse::<u64>().ok()?;
    let node_name = props.get(*pw::keys::NODE_NAME)?.to_owned();
    let display_name = props
        .get(*pw::keys::NODE_DESCRIPTION)
        .or_else(|| props.get(*pw::keys::NODE_NICK))
        .unwrap_or(node_name.as_str())
        .to_owned();
    Some(NodeAccumulator {
        id: AudioEndpointId::new(server_cookie, object_serial),
        kind,
        node_name,
        display_name,
        device_global_id: props
            .get(*pw::keys::DEVICE_ID)
            .and_then(|value| value.parse::<u32>().ok()),
        profile_device: props
            .get("card.profile.device")
            .and_then(|value| value.parse::<i32>().ok()),
        scalar: None,
        channels: None,
        soft_channels: None,
        muted: None,
    })
}

fn classify_media_class(media_class: &str) -> Option<AudioEndpointKind> {
    match media_class {
        "Audio/Sink" => Some(AudioEndpointKind::Output),
        "Audio/Source" => Some(AudioEndpointKind::Input),
        _ => None,
    }
}

fn metadata_role(key: &str) -> Option<(DefaultRoleKind, bool)> {
    match key {
        "default.audio.sink" => Some((DefaultRoleKind::Output, false)),
        "default.audio.source" => Some((DefaultRoleKind::Input, false)),
        "default.configured.audio.sink" => Some((DefaultRoleKind::Output, true)),
        "default.configured.audio.source" => Some((DefaultRoleKind::Input, true)),
        _ => None,
    }
}

fn apply_metadata_property(audio: &mut AudioState, key: Option<&str>, value: Option<&str>) -> bool {
    let Some(key) = key else {
        return clear_all_default_roles(audio);
    };
    let Some((kind, configured)) = metadata_role(key) else {
        return false;
    };
    match value {
        None => set_default_name(audio, kind, configured, None),
        Some(raw) => match parse_default_node_name(raw) {
            Ok(name) => set_default_name(audio, kind, configured, Some(name)),
            Err(error) => audio.set_default_metadata_error(kind, configured, error.to_string()),
        },
    }
}

fn set_default_name(
    audio: &mut AudioState,
    kind: DefaultRoleKind,
    configured: bool,
    name: Option<String>,
) -> bool {
    if configured {
        audio.set_configured_default_name(kind, name)
    } else {
        audio.set_effective_default_name(kind, name)
    }
}

fn clear_all_default_roles(audio: &mut AudioState) -> bool {
    let mut changed = false;
    for kind in [DefaultRoleKind::Output, DefaultRoleKind::Input] {
        changed |= audio.set_effective_default_name(kind, None);
        changed |= audio.set_configured_default_name(kind, None);
    }
    changed
}

fn run_registry(
    main_loop: pw::main_loop::MainLoopRc,
    core: pw::core::CoreRc,
    runtime: Rc<RefCell<ObserverRuntime>>,
    publish: PublishSink,
    quit_on_signals: bool,
) -> Result<(), AudioObserverError> {
    let registry = core.get_registry_rc()?;
    let bindings: Rc<RefCell<HashMap<u32, BoundObject>>> = Rc::new(RefCell::new(HashMap::new()));
    let server_cookie = runtime.borrow().audio.server_cookie();
    let registry_weak = registry.downgrade();
    let runtime_global = runtime.clone();
    let publish_global = publish.clone();
    let bindings_global = bindings.clone();
    let runtime_remove = runtime.clone();
    let publish_remove = publish.clone();
    let bindings_remove = bindings.clone();
    let runtime_error = runtime.clone();
    let publish_error = publish.clone();
    let error_loop = main_loop.downgrade();
    let core_error_listener = core
        .add_listener_local()
        .error(move |id, _seq, _res, _message| {
            if id != pw::core::PW_ID_CORE {
                return;
            }
            let changed = runtime_error
                .borrow_mut()
                .audio
                .set_provider_state(crate::AudioProviderState::Unavailable);
            if changed {
                publish_snapshot(&runtime_error, &publish_error);
            }
            if let Some(main_loop) = error_loop.upgrade() {
                main_loop.quit();
            }
        })
        .register();

    let registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            let Some(registry) = registry_weak.upgrade() else {
                return;
            };
            if global.type_ == pw::types::ObjectType::Device {
                if let Ok(bound) = bind_device(
                    &registry,
                    global,
                    global.id,
                    runtime_global.clone(),
                    publish_global.clone(),
                ) {
                    bindings_global.borrow_mut().insert(global.id, bound);
                }
                return;
            }

            if let Some(accumulator) = node_accumulator_from_global(global, server_cookie) {
                let global_id = global.id;
                let changed = runtime_global
                    .borrow_mut()
                    .insert_node(global_id, accumulator);
                if changed {
                    publish_snapshot(&runtime_global, &publish_global);
                }
                if let Ok(bound) = bind_node(
                    &registry,
                    global,
                    global_id,
                    runtime_global.clone(),
                    publish_global.clone(),
                ) {
                    bindings_global.borrow_mut().insert(global_id, bound);
                }
                return;
            }

            if is_default_metadata(global)
                && let Ok(bound) = bind_default_metadata(
                    &registry,
                    global,
                    runtime_global.clone(),
                    publish_global.clone(),
                )
            {
                bindings_global.borrow_mut().insert(global.id, bound);
            }
        })
        .global_remove(move |global_id| {
            let removed = bindings_remove.borrow_mut().remove(&global_id);
            let Some(removed) = removed else {
                return;
            };
            let mut runtime = runtime_remove.borrow_mut();
            let mut changed = false;
            if let Some(endpoint_id) = removed.endpoint_id {
                runtime.nodes.remove(&global_id);
                changed |= runtime.audio.remove_endpoint(&endpoint_id);
            }
            if removed.default_metadata {
                changed |= clear_all_default_roles(&mut runtime.audio);
            }
            drop(runtime);
            if changed {
                publish_snapshot(&runtime_remove, &publish_remove);
            }
        })
        .register();

    let quit_on = |signal| {
        let loop_weak = main_loop.downgrade();
        main_loop.loop_().add_signal_local(signal, move || {
            if let Some(main_loop) = loop_weak.upgrade() {
                main_loop.quit();
            }
        })
    };
    let sig_int = quit_on_signals.then(|| quit_on(pw::loop_::Signal::INT));
    let sig_term = quit_on_signals.then(|| quit_on(pw::loop_::Signal::TERM));

    core.sync(0)?;
    main_loop.run();
    drop((
        core_error_listener,
        registry_listener,
        sig_int,
        sig_term,
        bindings,
    ));
    Ok(())
}

fn is_default_metadata(
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) -> bool {
    if global.type_ != pw::types::ObjectType::Metadata {
        return false;
    }
    global.props.and_then(|props| props.get("metadata.name")) == Some("default")
}

fn should_refresh_device_route<I>(change_mask: pw::device::DeviceChangeMask, params: I) -> bool
where
    I: IntoIterator<Item = pw::spa::param::ParamType>,
{
    change_mask.contains(pw::device::DeviceChangeMask::PARAMS)
        && params
            .into_iter()
            .any(|param| param == pw::spa::param::ParamType::Route)
}

fn bind_device(
    registry: &pw::registry::Registry,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
    global_id: u32,
    runtime: Rc<RefCell<ObserverRuntime>>,
    publish: PublishSink,
) -> Result<BoundObject, pw::Error> {
    let device = Rc::new(registry.bind::<pw::device::Device, _>(global)?);
    let runtime_param = runtime.clone();
    let publish_param = publish.clone();
    let device_for_info = device.clone();
    let listener = device
        .add_listener_local()
        .info(move |info| {
            if should_refresh_device_route(
                info.change_mask(),
                info.params().iter().map(|param| param.id()),
            ) {
                device_for_info.enum_params(0, Some(pw::spa::param::ParamType::Route), 0, u32::MAX);
            }
        })
        .param(move |_seq, param_type, _index, _next, param| {
            if param_type != pw::spa::param::ParamType::Route {
                return;
            }
            let Some(param) = param else {
                return;
            };
            let Ok((_rest, value)) = PodDeserializer::deserialize_from::<Value>(param.as_bytes())
            else {
                return;
            };
            let Ok(route) = parse_route_value(&value) else {
                return;
            };
            let changed = runtime_param.borrow_mut().apply_route(global_id, route);
            if changed {
                publish_snapshot(&runtime_param, &publish_param);
            }
        })
        .register();
    device.subscribe_params(&[pw::spa::param::ParamType::Route]);
    device.enum_params(0, Some(pw::spa::param::ParamType::Route), 0, u32::MAX);
    Ok(BoundObject {
        _proxy: None,
        _device: Some(device),
        _listeners: vec![Box::new(listener)],
        endpoint_id: None,
        default_metadata: false,
    })
}

fn bind_node(
    registry: &pw::registry::Registry,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
    global_id: u32,
    runtime: Rc<RefCell<ObserverRuntime>>,
    publish: PublishSink,
) -> Result<BoundObject, pw::Error> {
    let node: pw::node::Node = registry.bind(global)?;
    let endpoint_id = runtime.borrow().nodes.get(&global_id).map(|node| node.id);
    let runtime_info = runtime.clone();
    let publish_info = publish.clone();
    let runtime_param = runtime.clone();
    let publish_param = publish.clone();
    let listener = node
        .add_listener_local()
        .info(move |info| {
            let Some(props) = info.props() else {
                return;
            };
            let device_global_id = props
                .get(*pw::keys::DEVICE_ID)
                .and_then(|value| value.parse::<u32>().ok());
            let profile_device = props
                .get("card.profile.device")
                .and_then(|value| value.parse::<i32>().ok());
            let (Some(device_global_id), Some(profile_device)) = (device_global_id, profile_device)
            else {
                return;
            };
            let changed = runtime_info.borrow_mut().bind_node_route(
                global_id,
                device_global_id,
                profile_device,
            );
            if changed {
                publish_snapshot(&runtime_info, &publish_info);
            }
        })
        .param(move |_seq, param_type, _index, _next, param| {
            if param_type != pw::spa::param::ParamType::Props {
                return;
            }
            let Some(param) = param else {
                return;
            };
            let Ok((_rest, value)) = PodDeserializer::deserialize_from::<Value>(param.as_bytes())
            else {
                return;
            };
            let Ok(patch) = parse_props_value(&value) else {
                return;
            };
            let changed = {
                let mut runtime = runtime_param.borrow_mut();
                let Some(accumulator) = runtime.nodes.get_mut(&global_id) else {
                    return;
                };
                let Some(endpoint) = accumulator.apply_node_patch(patch) else {
                    return;
                };
                runtime.audio.upsert_endpoint(endpoint)
            };
            if changed {
                publish_snapshot(&runtime_param, &publish_param);
            }
        })
        .register();
    node.subscribe_params(&[pw::spa::param::ParamType::Props]);
    node.enum_params(0, Some(pw::spa::param::ParamType::Props), 0, u32::MAX);
    Ok(BoundObject {
        _proxy: Some(Box::new(node)),
        _device: None,
        _listeners: vec![Box::new(listener)],
        endpoint_id,
        default_metadata: false,
    })
}

fn bind_default_metadata(
    registry: &pw::registry::Registry,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
    runtime: Rc<RefCell<ObserverRuntime>>,
    publish: PublishSink,
) -> Result<BoundObject, pw::Error> {
    let metadata: pw::metadata::Metadata = registry.bind(global)?;
    let runtime_property = runtime.clone();
    let publish_property = publish.clone();
    let listener = metadata
        .add_listener_local()
        .property(move |_subject, key, _type_, value| {
            let changed = {
                let mut runtime = runtime_property.borrow_mut();
                apply_metadata_property(&mut runtime.audio, key, value)
            };
            if changed {
                publish_snapshot(&runtime_property, &publish_property);
            }
            0
        })
        .register();
    Ok(BoundObject {
        _proxy: Some(Box::new(metadata)),
        _device: None,
        _listeners: vec![Box::new(listener)],
        endpoint_id: None,
        default_metadata: true,
    })
}

fn parse_props_value(value: &Value) -> Result<PropsPatch, AudioModelError> {
    let Value::Object(object) = value else {
        return Err(AudioModelError::InvalidProps("expected SPA object".into()));
    };
    let mut patch = PropsPatch::default();
    for property in &object.properties {
        match property.key {
            key if key == pw::spa::sys::SPA_PROP_volume => {
                let Value::Float(value) = property.value else {
                    return Err(AudioModelError::InvalidProps("volume must be Float".into()));
                };
                patch.scalar = Some(NativeVolume::new(value)?);
            }
            key if key == pw::spa::sys::SPA_PROP_mute => {
                let Value::Bool(value) = property.value else {
                    return Err(AudioModelError::InvalidProps("mute must be Bool".into()));
                };
                patch.muted = Some(value);
            }
            key if key == pw::spa::sys::SPA_PROP_channelVolumes => {
                patch.channels = Some(parse_volume_array(&property.value)?);
            }
            key if key == pw::spa::sys::SPA_PROP_softVolumes => {
                patch.soft_channels = Some(parse_volume_array(&property.value)?);
            }
            _ => {}
        }
    }
    Ok(patch)
}

fn parse_route_value(value: &Value) -> Result<RoutePatch, AudioModelError> {
    let Value::Object(object) = value else {
        return Err(AudioModelError::InvalidProps(
            "expected Route SPA object".into(),
        ));
    };
    let mut profile_device = None;
    let mut props = None;
    for property in &object.properties {
        match property.key {
            key if key == pw::spa::sys::SPA_PARAM_ROUTE_device => {
                let Value::Int(value) = property.value else {
                    return Err(AudioModelError::InvalidProps(
                        "route device must be Int".into(),
                    ));
                };
                profile_device = Some(value);
            }
            key if key == pw::spa::sys::SPA_PARAM_ROUTE_props => {
                props = Some(parse_props_value(&property.value)?);
            }
            _ => {}
        }
    }
    let profile_device = profile_device
        .ok_or_else(|| AudioModelError::InvalidProps("route missing device".into()))?;
    let patch = props.ok_or_else(|| AudioModelError::InvalidProps("route missing props".into()))?;
    Ok(RoutePatch {
        profile_device,
        patch,
    })
}

fn parse_volume_array(value: &Value) -> Result<Vec<NativeVolume>, AudioModelError> {
    let Value::ValueArray(ValueArray::Float(values)) = value else {
        return Err(AudioModelError::InvalidProps(
            "volume array must be Float[]".into(),
        ));
    };
    values.iter().copied().map(NativeVolume::new).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pw::spa::pod::{Object, Property};

    #[test]
    fn device_route_refresh_requires_params_change_and_route_param() {
        use pw::device::DeviceChangeMask;
        use pw::spa::param::ParamType;

        assert!(!should_refresh_device_route(
            DeviceChangeMask::PROPS,
            [ParamType::Route],
        ));
        assert!(!should_refresh_device_route(
            DeviceChangeMask::PARAMS,
            [ParamType::Props],
        ));
        assert!(should_refresh_device_route(
            DeviceChangeMask::PARAMS,
            [ParamType::Props, ParamType::Route],
        ));
    }

    #[test]
    fn only_real_audio_sink_and_source_classes_are_admitted() {
        assert_eq!(
            classify_media_class("Audio/Sink"),
            Some(AudioEndpointKind::Output)
        );
        assert_eq!(
            classify_media_class("Audio/Source"),
            Some(AudioEndpointKind::Input)
        );
        assert_eq!(classify_media_class("Stream/Output/Audio"), None);
        assert_eq!(classify_media_class("Audio/Source/Virtual"), None);
    }

    #[test]
    fn props_payload_preserves_scalar_channels_soft_channels_and_mute() {
        let value = props_value(vec![
            Property::new(pw::spa::sys::SPA_PROP_volume, Value::Float(1.25)),
            Property::new(pw::spa::sys::SPA_PROP_mute, Value::Bool(true)),
            Property::new(
                pw::spa::sys::SPA_PROP_channelVolumes,
                Value::ValueArray(ValueArray::Float(vec![0.7, 0.8])),
            ),
            Property::new(
                pw::spa::sys::SPA_PROP_softVolumes,
                Value::ValueArray(ValueArray::Float(vec![0.9, 1.0])),
            ),
        ]);
        let patch = parse_props_value(&value).expect("valid Props object");
        assert_eq!(patch.scalar.unwrap().get(), 1.25);
        assert_eq!(patch.channels.unwrap()[1].get(), 0.8);
        assert_eq!(patch.soft_channels.unwrap()[0].get(), 0.9);
        assert_eq!(patch.muted, Some(true));
    }

    #[test]
    fn malformed_negative_volume_is_rejected() {
        let value = props_value(vec![
            Property::new(pw::spa::sys::SPA_PROP_volume, Value::Float(-0.1)),
            Property::new(pw::spa::sys::SPA_PROP_mute, Value::Bool(false)),
        ]);
        assert!(parse_props_value(&value).is_err());
    }

    #[test]
    fn metadata_key_maps_effective_and_configured_roles_separately() {
        assert_eq!(
            metadata_role("default.audio.sink"),
            Some((DefaultRoleKind::Output, false))
        );
        assert_eq!(
            metadata_role("default.configured.audio.source"),
            Some((DefaultRoleKind::Input, true))
        );
        assert_eq!(metadata_role("irrelevant"), None);
    }

    #[test]
    fn route_payload_extracts_profile_device_and_authoritative_props() {
        let route = Value::Object(Object {
            type_: pw::spa::utils::SpaTypes::ObjectParamRoute.as_raw(),
            id: pw::spa::param::ParamType::Route.as_raw(),
            properties: vec![
                Property::new(pw::spa::sys::SPA_PARAM_ROUTE_device, Value::Int(5)),
                Property::new(
                    pw::spa::sys::SPA_PARAM_ROUTE_props,
                    props_value(vec![
                        Property::new(
                            pw::spa::sys::SPA_PROP_channelVolumes,
                            Value::ValueArray(ValueArray::Float(vec![0.728985, 0.728985])),
                        ),
                        Property::new(pw::spa::sys::SPA_PROP_mute, Value::Bool(false)),
                    ]),
                ),
            ],
        });
        let route = parse_route_value(&route).expect("valid route");
        assert_eq!(route.profile_device, 5);
        assert_eq!(route.patch.channels.unwrap()[0].get(), 0.728985);
        assert_eq!(route.patch.muted, Some(false));
    }

    #[test]
    fn late_node_info_route_binding_applies_cached_route() {
        let mut runtime = ObserverRuntime::new(7);
        let route = RoutePatch {
            profile_device: 5,
            patch: PropsPatch {
                channels: Some(vec![NativeVolume::new(0.728985).unwrap(); 2]),
                muted: Some(false),
                ..PropsPatch::default()
            },
        };
        assert!(!runtime.apply_route(47, route));
        let node = NodeAccumulator {
            id: AudioEndpointId::new(7, 62),
            kind: AudioEndpointKind::Output,
            node_name: "sink.a".into(),
            display_name: "Sink A".into(),
            device_global_id: Some(47),
            profile_device: None,
            scalar: None,
            channels: None,
            soft_channels: None,
            muted: None,
        };
        assert!(!runtime.insert_node(55, node));
        assert!(runtime.bind_node_route(55, 47, 5));
        let endpoint = runtime.audio.snapshot().endpoints[&AudioEndpointId::new(7, 62)].clone();
        assert_eq!(endpoint.volume.channels()[0].get(), 0.728985);
    }

    #[test]
    fn route_before_node_is_retained_and_applied_when_node_arrives() {
        let mut runtime = ObserverRuntime::new(7);
        let route = RoutePatch {
            profile_device: 5,
            patch: PropsPatch {
                channels: Some(vec![NativeVolume::new(0.728985).unwrap(); 2]),
                muted: Some(false),
                ..PropsPatch::default()
            },
        };
        assert!(!runtime.apply_route(47, route));
        let node = NodeAccumulator {
            id: AudioEndpointId::new(7, 62),
            kind: AudioEndpointKind::Output,
            node_name: "sink.a".into(),
            display_name: "Sink A".into(),
            device_global_id: Some(47),
            profile_device: Some(5),
            scalar: None,
            channels: None,
            soft_channels: None,
            muted: None,
        };
        assert!(runtime.insert_node(55, node));
        let endpoint = runtime.audio.snapshot().endpoints[&AudioEndpointId::new(7, 62)].clone();
        assert_eq!(endpoint.volume.channels()[0].get(), 0.728985);
    }

    #[test]
    fn route_managed_node_ignores_node_props_and_accepts_matching_route() {
        let mut node = NodeAccumulator {
            id: AudioEndpointId::new(7, 62),
            kind: AudioEndpointKind::Output,
            node_name: "sink.a".into(),
            display_name: "Sink A".into(),
            device_global_id: Some(47),
            profile_device: Some(5),
            scalar: None,
            channels: None,
            soft_channels: None,
            muted: None,
        };
        let stale = PropsPatch {
            channels: Some(vec![NativeVolume::new(0.778688).unwrap(); 2]),
            muted: Some(false),
            ..PropsPatch::default()
        };
        assert!(node.apply_node_patch(stale).is_none());

        let authoritative = PropsPatch {
            channels: Some(vec![NativeVolume::new(0.728985).unwrap(); 2]),
            muted: Some(false),
            ..PropsPatch::default()
        };
        let endpoint = node
            .apply_route_patch(5, authoritative)
            .expect("matching route must publish endpoint");
        assert_eq!(endpoint.volume.channels()[0].get(), 0.728985);
        assert!(node.apply_route_patch(12, PropsPatch::default()).is_none());
    }

    fn props_value(properties: Vec<Property>) -> Value {
        Value::Object(Object {
            type_: pw::spa::utils::SpaTypes::ObjectParamProps.as_raw(),
            id: pw::spa::param::ParamType::Props.as_raw(),
            properties,
        })
    }
}
