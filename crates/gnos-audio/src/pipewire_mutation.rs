use pipewire::spa::{
    param::ParamType,
    pod::{Object, Property, Value, ValueArray},
    utils::SpaTypes,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioMutationBuildError {
    VolumeOutOfRange(f32),
    ChannelCountZero,
}

pub fn props_for_user_volume(
    user_level: f32,
    channels: usize,
) -> Result<Value, AudioMutationBuildError> {
    if !user_level.is_finite() || !(0.0..=1.0).contains(&user_level) {
        return Err(AudioMutationBuildError::VolumeOutOfRange(user_level));
    }
    if channels == 0 {
        return Err(AudioMutationBuildError::ChannelCountZero);
    }
    let linear = user_level.powi(3);
    Ok(props_object(vec![Property::new(
        pipewire::spa::sys::SPA_PROP_channelVolumes,
        Value::ValueArray(ValueArray::Float(vec![linear; channels])),
    )]))
}

pub fn props_for_mute(muted: bool) -> Value {
    props_object(vec![Property::new(
        pipewire::spa::sys::SPA_PROP_mute,
        Value::Bool(muted),
    )])
}

fn props_object(properties: Vec<Property>) -> Value {
    Value::Object(Object {
        type_: SpaTypes::ObjectParamProps.as_raw(),
        id: ParamType::Props.as_raw(),
        properties,
    })
}

fn expected_media_class(kind: crate::AudioEndpointKind) -> &'static str {
    match kind {
        crate::AudioEndpointKind::Output => "Audio/Sink",
        crate::AudioEndpointKind::Input => "Audio/Source",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceRouteRef {
    index: i32,
    device: i32,
}

fn route_ref_from_value(value: &Value, profile_device: i32) -> Option<DeviceRouteRef> {
    let Value::Object(object) = value else {
        return None;
    };
    let mut index = None;
    let mut device = None;
    for property in &object.properties {
        match property.key {
            key if key == pipewire::spa::sys::SPA_PARAM_ROUTE_index => {
                if let Value::Int(value) = property.value {
                    index = Some(value);
                }
            }
            key if key == pipewire::spa::sys::SPA_PARAM_ROUTE_device => {
                if let Value::Int(value) = property.value {
                    device = Some(value);
                }
            }
            _ => {}
        }
    }
    match (index, device) {
        (Some(index), Some(device)) if device == profile_device => {
            Some(DeviceRouteRef { index, device })
        }
        _ => None,
    }
}

fn route_mutation_payload(route: DeviceRouteRef, props: Value) -> Value {
    Value::Object(Object {
        type_: SpaTypes::ObjectParamRoute.as_raw(),
        id: ParamType::Route.as_raw(),
        properties: vec![
            Property::new(
                pipewire::spa::sys::SPA_PARAM_ROUTE_index,
                Value::Int(route.index),
            ),
            Property::new(
                pipewire::spa::sys::SPA_PARAM_ROUTE_device,
                Value::Int(route.device),
            ),
            Property::new(pipewire::spa::sys::SPA_PARAM_ROUTE_props, props),
            Property::new(pipewire::spa::sys::SPA_PARAM_ROUTE_save, Value::Bool(true)),
        ],
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExactAudioMutation {
    SetUserVolume { level: f32, channels: usize },
    SetMute { muted: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactAudioTarget {
    pub id: crate::AudioEndpointId,
    pub kind: crate::AudioEndpointKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactAudioMutationOutcome {
    Completed,
    RefusedStaleServer,
    TargetAbsent,
    KindMismatch,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioMutationTransportError {
    #[error("PipeWire error: {0}")]
    PipeWire(#[from] pipewire::Error),
    #[error("PipeWire core did not publish its server cookie")]
    MissingServerCookie,
    #[error("invalid audio mutation payload")]
    InvalidPayload,
    #[error("failed to serialize PipeWire Props payload: {0}")]
    Serialize(String),
    #[error("PipeWire device {device_id} has no active route for profile device {profile_device}")]
    MissingDeviceRoute { device_id: u32, profile_device: i32 },
    #[error("PipeWire has no default metadata (no session manager?)")]
    MissingDefaultMetadata,
}

pub fn execute_exact_mutation(
    target: ExactAudioTarget,
    mutation: ExactAudioMutation,
) -> Result<ExactAudioMutationOutcome, AudioMutationTransportError> {
    use pipewire as pw;
    use pw::spa::pod::{Pod, deserialize::PodDeserializer, serialize::PodSerializer};
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        io::Cursor,
        rc::Rc,
    };

    let props = match mutation {
        ExactAudioMutation::SetUserVolume { level, channels } => {
            props_for_user_volume(level, channels)
                .map_err(|_| AudioMutationTransportError::InvalidPayload)?
        }
        ExactAudioMutation::SetMute { muted } => props_for_mute(muted),
    };
    let serialize = |value: &Value| -> Result<Vec<u8>, AudioMutationTransportError> {
        let (cursor, _) = PodSerializer::serialize(Cursor::new(Vec::new()), value)
            .map_err(|error| AudioMutationTransportError::Serialize(error.to_string()))?;
        Ok(cursor.into_inner())
    };
    let node_payload = serialize(&props)?;

    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_rc(None)?;
    let cookie =
        crate::pipewire_observer::initialize_server_cookie(&main_loop, &core).map_err(|error| {
            match error {
                crate::AudioObserverError::PipeWire(error) => {
                    AudioMutationTransportError::PipeWire(error)
                }
                crate::AudioObserverError::MissingServerCookie => {
                    AudioMutationTransportError::MissingServerCookie
                }
            }
        })?;
    if cookie != target.id.server_cookie {
        return Ok(ExactAudioMutationOutcome::RefusedStaleServer);
    }

    let registry = core.get_registry_rc()?;
    let registry_weak = registry.downgrade();
    let target_node: Rc<RefCell<Option<Rc<pw::node::Node>>>> = Rc::new(RefCell::new(None));
    let target_info_listener: Rc<RefCell<Option<pw::node::NodeListener>>> =
        Rc::new(RefCell::new(None));
    let target_global_id = Rc::new(Cell::new(None::<u32>));
    let target_alive = Rc::new(Cell::new(false));
    let target_route = Rc::new(Cell::new(None::<(u32, i32)>));
    let kind_mismatch = Rc::new(Cell::new(false));
    let devices: Rc<RefCell<HashMap<u32, Rc<pw::device::Device>>>> =
        Rc::new(RefCell::new(HashMap::new()));

    let target_node_global = target_node.clone();
    let target_info_listener_global = target_info_listener.clone();
    let target_global_id_global = target_global_id.clone();
    let target_alive_global = target_alive.clone();
    let target_route_global = target_route.clone();
    let kind_mismatch_global = kind_mismatch.clone();
    let devices_global = devices.clone();
    let registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            let Some(registry) = registry_weak.upgrade() else {
                return;
            };
            if global.type_ == pw::types::ObjectType::Device {
                if let Ok(device) = registry.bind::<pw::device::Device, _>(global) {
                    devices_global
                        .borrow_mut()
                        .insert(global.id, Rc::new(device));
                }
                return;
            }
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }
            let Some(props) = global.props else {
                return;
            };
            let serial = props
                .get(*pw::keys::OBJECT_SERIAL)
                .and_then(|value| value.parse::<u64>().ok());
            if serial != Some(target.id.object_serial) {
                return;
            }
            target_global_id_global.set(Some(global.id));
            target_alive_global.set(true);
            if props.get(*pw::keys::MEDIA_CLASS) != Some(expected_media_class(target.kind)) {
                kind_mismatch_global.set(true);
                return;
            }
            let Ok(node) = registry.bind::<pw::node::Node, _>(global) else {
                return;
            };
            let route_for_info = target_route_global.clone();
            let info_listener = node
                .add_listener_local()
                .info(move |info| {
                    let Some(props) = info.props() else {
                        return;
                    };
                    let device_id = props
                        .get(*pw::keys::DEVICE_ID)
                        .and_then(|value| value.parse::<u32>().ok());
                    let profile_device = props
                        .get("card.profile.device")
                        .and_then(|value| value.parse::<i32>().ok());
                    if let (Some(device_id), Some(profile_device)) = (device_id, profile_device) {
                        route_for_info.set(Some((device_id, profile_device)));
                    }
                })
                .register();
            *target_info_listener_global.borrow_mut() = Some(info_listener);
            *target_node_global.borrow_mut() = Some(Rc::new(node));
        })
        .global_remove({
            let target_global_id = target_global_id.clone();
            let target_alive = target_alive.clone();
            move |global_id| {
                if target_global_id.get() == Some(global_id) {
                    target_alive.set(false);
                }
            }
        })
        .register();

    let wait_seq = Rc::new(Cell::new(None::<i32>));
    let wait_seq_done = wait_seq.clone();
    let loop_weak = main_loop.downgrade();
    let done_listener = core
        .add_listener_local()
        .done(move |_id, seq| {
            if wait_seq_done.get() == Some(seq.seq())
                && let Some(main_loop) = loop_weak.upgrade()
            {
                main_loop.quit();
            }
        })
        .register();

    let sync_and_run = |tag: i32| -> Result<(), AudioMutationTransportError> {
        let seq = core.sync(tag)?;
        wait_seq.set(Some(seq.seq()));
        main_loop.run();
        wait_seq.set(None);
        Ok(())
    };

    sync_and_run(10)?;
    // Binding the exact Node happens while the initial registry burst is being
    // handled. Its NodeInfo can therefore arrive after the registry sync that
    // discovered it. Give that proxy one causal barrier before deciding whether
    // this endpoint is Device-Route managed or a software Node-Props endpoint.
    if target_node.borrow().is_some() && target_route.get().is_none() {
        sync_and_run(11)?;
    }
    if kind_mismatch.get() {
        return Ok(ExactAudioMutationOutcome::KindMismatch);
    }
    let Some(node) = target_node.borrow().clone() else {
        return Ok(ExactAudioMutationOutcome::TargetAbsent);
    };
    if !target_alive.get() {
        return Ok(ExactAudioMutationOutcome::TargetAbsent);
    }

    if let Some((device_id, profile_device)) = target_route.get() {
        let Some(device) = devices.borrow().get(&device_id).cloned() else {
            return Err(AudioMutationTransportError::MissingDeviceRoute {
                device_id,
                profile_device,
            });
        };
        let route_ref = Rc::new(Cell::new(None::<DeviceRouteRef>));
        let route_ref_param = route_ref.clone();
        let route_listener = device
            .add_listener_local()
            .param(move |_seq, param_type, _index, _next, param| {
                if param_type != ParamType::Route {
                    return;
                }
                let Some(param) = param else {
                    return;
                };
                let Ok((_rest, value)) =
                    PodDeserializer::deserialize_from::<Value>(param.as_bytes())
                else {
                    return;
                };
                if let Some(route) = route_ref_from_value(&value, profile_device) {
                    route_ref_param.set(Some(route));
                }
            })
            .register();
        device.enum_params(0, Some(ParamType::Route), 0, u32::MAX);
        sync_and_run(11)?;
        if !target_alive.get() {
            return Ok(ExactAudioMutationOutcome::TargetAbsent);
        }
        let Some(route) = route_ref.get() else {
            return Err(AudioMutationTransportError::MissingDeviceRoute {
                device_id,
                profile_device,
            });
        };
        let route_value = route_mutation_payload(route, props);
        let route_payload = serialize(&route_value)?;
        let Some(pod) = Pod::from_bytes(&route_payload) else {
            return Err(AudioMutationTransportError::InvalidPayload);
        };
        device.set_param(ParamType::Route, 0, pod);
        sync_and_run(12)?;
        drop(route_listener);
    } else {
        let Some(pod) = Pod::from_bytes(&node_payload) else {
            return Err(AudioMutationTransportError::InvalidPayload);
        };
        node.set_param(ParamType::Props, 0, pod);
        sync_and_run(12)?;
    }

    drop((done_listener, registry_listener, node, devices));
    Ok(ExactAudioMutationOutcome::Completed)
}

/// SETTINGS-02B: the `default` metadata key and value that make
/// `node_name` the configured default for `kind` (what `wpctl set-default`
/// writes; the session manager then follows it).
pub fn default_metadata_entry(
    kind: crate::AudioEndpointKind,
    node_name: &str,
) -> (&'static str, String) {
    let key = match kind {
        crate::AudioEndpointKind::Output => "default.configured.audio.sink",
        crate::AudioEndpointKind::Input => "default.configured.audio.source",
    };
    (key, serde_json::json!({ "name": node_name }).to_string())
}

/// SETTINGS-02B: make the exact endpoint `target` the configured default
/// for its kind. The node must still be the one observed (same server, same
/// serial, same kind and name); otherwise nothing is written.
pub fn execute_select_default(
    target: ExactAudioTarget,
    node_name: &str,
) -> Result<ExactAudioMutationOutcome, AudioMutationTransportError> {
    use pipewire as pw;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_rc(None)?;
    let cookie =
        crate::pipewire_observer::initialize_server_cookie(&main_loop, &core).map_err(|error| {
            match error {
                crate::AudioObserverError::PipeWire(error) => {
                    AudioMutationTransportError::PipeWire(error)
                }
                crate::AudioObserverError::MissingServerCookie => {
                    AudioMutationTransportError::MissingServerCookie
                }
            }
        })?;
    if cookie != target.id.server_cookie {
        return Ok(ExactAudioMutationOutcome::RefusedStaleServer);
    }

    let registry = core.get_registry_rc()?;
    let registry_weak = registry.downgrade();
    let found = Rc::new(Cell::new(false));
    let kind_mismatch = Rc::new(Cell::new(false));
    let metadata: Rc<RefCell<Option<pw::metadata::Metadata>>> = Rc::new(RefCell::new(None));
    let expected_name = node_name.to_owned();
    let found_global = found.clone();
    let kind_mismatch_global = kind_mismatch.clone();
    let metadata_global = metadata.clone();
    let registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            let Some(props) = global.props else {
                return;
            };
            if global.type_ == pw::types::ObjectType::Metadata
                && props.get("metadata.name") == Some("default")
            {
                if let Some(registry) = registry_weak.upgrade()
                    && let Ok(bound) = registry.bind::<pw::metadata::Metadata, _>(global)
                {
                    *metadata_global.borrow_mut() = Some(bound);
                }
                return;
            }
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }
            let serial = props
                .get(*pw::keys::OBJECT_SERIAL)
                .and_then(|value| value.parse::<u64>().ok());
            if serial != Some(target.id.object_serial) {
                return;
            }
            if props.get(*pw::keys::MEDIA_CLASS) != Some(expected_media_class(target.kind))
                || props.get(*pw::keys::NODE_NAME) != Some(expected_name.as_str())
            {
                kind_mismatch_global.set(true);
                return;
            }
            found_global.set(true);
        })
        .register();

    let wait_seq = Rc::new(Cell::new(None::<i32>));
    let wait_seq_done = wait_seq.clone();
    let loop_weak = main_loop.downgrade();
    let done_listener = core
        .add_listener_local()
        .done(move |_id, seq| {
            if wait_seq_done.get() == Some(seq.seq())
                && let Some(main_loop) = loop_weak.upgrade()
            {
                main_loop.quit();
            }
        })
        .register();
    let sync_and_run = |tag: i32| -> Result<(), AudioMutationTransportError> {
        let seq = core.sync(tag)?;
        wait_seq.set(Some(seq.seq()));
        main_loop.run();
        wait_seq.set(None);
        Ok(())
    };

    sync_and_run(20)?;
    if kind_mismatch.get() {
        return Ok(ExactAudioMutationOutcome::KindMismatch);
    }
    if !found.get() {
        return Ok(ExactAudioMutationOutcome::TargetAbsent);
    }
    let Some(metadata) = metadata.borrow_mut().take() else {
        return Err(AudioMutationTransportError::MissingDefaultMetadata);
    };
    let (key, value) = default_metadata_entry(target.kind, node_name);
    metadata.set_property(0, key, Some("Spa:String:JSON"), Some(&value));
    sync_and_run(21)?;
    drop((done_listener, registry_listener, metadata));
    Ok(ExactAudioMutationOutcome::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_entry_names_the_node_as_wireplumber_expects() {
        assert_eq!(
            default_metadata_entry(crate::AudioEndpointKind::Output, "alsa_output.usb-\"x\""),
            (
                "default.configured.audio.sink",
                r#"{"name":"alsa_output.usb-\"x\""}"#.to_owned()
            )
        );
        assert_eq!(
            default_metadata_entry(crate::AudioEndpointKind::Input, "mic").0,
            "default.configured.audio.source"
        );
    }
    use pipewire::spa::pod::{Value, ValueArray};

    #[test]
    fn set_user_volume_serializes_cubic_level_to_linear_channel_volumes() {
        let value = props_for_user_volume(0.75, 2).expect("bounded volume");
        let Value::Object(object) = value else {
            panic!("Props must be an object pod");
        };
        let property = object
            .properties
            .iter()
            .find(|property| property.key == pipewire::spa::sys::SPA_PROP_channelVolumes)
            .expect("channelVolumes property");
        let Value::ValueArray(ValueArray::Float(channels)) = &property.value else {
            panic!("channelVolumes must be float array");
        };
        assert_eq!(channels.len(), 2);
        assert!(
            channels
                .iter()
                .all(|value| (*value - 0.421875).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn mute_props_contains_only_exact_mute_value() {
        let Value::Object(object) = props_for_mute(true) else {
            panic!("Props must be an object pod");
        };
        assert_eq!(object.properties.len(), 1);
        assert_eq!(object.properties[0].key, pipewire::spa::sys::SPA_PROP_mute);
        assert_eq!(object.properties[0].value, Value::Bool(true));
    }

    #[test]
    fn native_volume_builder_rejects_product_amplification() {
        assert!(props_for_user_volume(1.01, 2).is_err());
        assert!(props_for_user_volume(1.0, 2).is_ok());
        assert!(props_for_user_volume(0.0, 2).is_ok());
    }

    #[test]
    fn native_volume_builder_rejects_zero_channels() {
        assert_eq!(
            props_for_user_volume(0.5, 0),
            Err(AudioMutationBuildError::ChannelCountZero)
        );
    }

    #[test]
    fn endpoint_kind_maps_to_exact_pipewire_media_class() {
        assert_eq!(
            expected_media_class(crate::AudioEndpointKind::Output),
            "Audio/Sink"
        );
        assert_eq!(
            expected_media_class(crate::AudioEndpointKind::Input),
            "Audio/Source"
        );
    }

    #[test]
    fn route_ref_selects_the_exact_profile_device() {
        let value = route_value(3, 5, props_for_mute(false));
        assert_eq!(
            route_ref_from_value(&value, 5),
            Some(DeviceRouteRef {
                index: 3,
                device: 5
            })
        );
        assert_eq!(route_ref_from_value(&value, 12), None);
    }

    #[test]
    fn route_mutation_payload_preserves_identity_and_requests_save() {
        let route = DeviceRouteRef {
            index: 3,
            device: 5,
        };
        let Value::Object(object) = route_mutation_payload(route, props_for_mute(true)) else {
            panic!("route payload must be an object");
        };
        assert_eq!(object.type_, SpaTypes::ObjectParamRoute.as_raw());
        assert_eq!(object.id, ParamType::Route.as_raw());
        assert!(object.properties.iter().any(|property| {
            property.key == pipewire::spa::sys::SPA_PARAM_ROUTE_index
                && property.value == Value::Int(3)
        }));
        assert!(object.properties.iter().any(|property| {
            property.key == pipewire::spa::sys::SPA_PARAM_ROUTE_device
                && property.value == Value::Int(5)
        }));
        assert!(object.properties.iter().any(|property| {
            property.key == pipewire::spa::sys::SPA_PARAM_ROUTE_save
                && property.value == Value::Bool(true)
        }));
    }

    fn route_value(index: i32, device: i32, props: Value) -> Value {
        Value::Object(Object {
            type_: SpaTypes::ObjectParamRoute.as_raw(),
            id: ParamType::Route.as_raw(),
            properties: vec![
                Property::new(pipewire::spa::sys::SPA_PARAM_ROUTE_index, Value::Int(index)),
                Property::new(
                    pipewire::spa::sys::SPA_PARAM_ROUTE_device,
                    Value::Int(device),
                ),
                Property::new(pipewire::spa::sys::SPA_PARAM_ROUTE_props, props),
            ],
        })
    }
}
