//! The scene's windows: one per output, opened on its display.

use super::*;

pub(super) fn wayland_runtime_name_key(name: &str) -> Uuid {
    // The pinned GPUI Wayland backend derives PlatformDisplay::uuid() from the
    // current wl_output name. SDK7 uses that implementation detail only as an
    // in-session bridge from an observed Hyprland output name to DisplayId.
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, name.as_bytes())
}

pub(super) fn runtime_display_id_for_output_name(
    cx: &App,
    name: &str,
) -> Result<DisplayId, String> {
    let expected = wayland_runtime_name_key(name);
    let matches = cx
        .displays()
        .into_iter()
        .filter_map(|display| {
            display
                .uuid()
                .ok()
                .filter(|uuid| *uuid == expected)
                .map(|_| display.id())
        })
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(format!("missing_gpui_display:{name}")),
        [display_id] => Ok(*display_id),
        _ => Err(format!("ambiguous_gpui_display:{name}")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn open_scene_window(
    cx: &mut App,
    scene_id: MonitorSceneId,
    runtime_output: Option<RuntimeId>,
    output_origin: (i32, i32),
    tray_qualification: Option<TrayQualificationRequest>,
    display_id: Option<DisplayId>,
    semantic_consumer: SceneSemanticConsumer,
    application_catalog: Arc<ApplicationCatalog>,
    product_focus: Option<ProductFocusScope>,
) -> Result<AnyWindowHandle, String> {
    let plan_size = gpui::Size::new(px(0.0), px(0.0));
    let runtime_output_for_host = runtime_output.clone();
    let semantic_consumer_for_host = semantic_consumer.clone();
    let application_catalog_for_host = Arc::clone(&application_catalog);
    let product_focus_for_host = product_focus.clone();
    let product_focus_registrar = product_focus;
    cx.open_window(
        WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: plan_size,
            })),
            display_id,
            app_id: Some(SCENE_IDENTITY.to_owned()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: SCENE_NAMESPACE.to_owned(),
                layer: Layer::Top,
                anchor: Anchor::TOP | Anchor::RIGHT | Anchor::BOTTOM | Anchor::LEFT,
                exclusive_zone: None,
                keyboard_interactivity: PRODUCT_SCENE_KEYBOARD_INTERACTIVITY,
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| {
            let runtime_output = runtime_output_for_host.clone();
            let semantic_consumer = semantic_consumer_for_host.clone();
            let application_catalog = Arc::clone(&application_catalog_for_host);
            let (coordinator, host_windows) = product_focus_for_host
                .clone()
                .map(|scope| (scope.coordinator, scope.windows))
                .unwrap_or_else(|| (ProductFocusCoordinator::new(), ProductHostWindows::new()));
            cx.new(|cx| {
                SceneHost::new(
                    scene_id,
                    runtime_output,
                    output_origin,
                    tray_qualification,
                    semantic_consumer,
                    application_catalog,
                    coordinator,
                    host_windows,
                    cx,
                )
            })
        },
    )
    .map_err(|error| format!("open_scene_window:{error}"))
    .map(|handle| {
        // SURF-04A: on the native rail each scene host has a desktop surface
        // beside it, on the background layer of the same output.
        #[cfg(feature = "component-lab")]
        {
            let args = std::env::args().collect::<Vec<_>>();
            if args.iter().any(|a| a == "--product-scene")
                && args.iter().any(|a| a == "--review-native-rail")
                && let Some(model) = crate::desktop_layer::open_desktop_layer(cx, display_id)
            {
                let _ = handle.update(cx, |host, _window, cx| host.attach_desktop(model, cx));
            }
        }
        let handle: AnyWindowHandle = handle.into();
        if let Some(scope) = &product_focus_registrar {
            scope
                .windows
                .register(ProductHostId(scene_id.value()), handle);
        }
        handle
    })
}

pub(super) fn open_multi_output_scene_windows(
    tray_qualification: Option<TrayQualificationRequest>,
    cx: &mut App,
    application_catalog: Arc<ApplicationCatalog>,
) -> Result<usize, String> {
    // One focus scope for the whole session: the desktop has one keyboard
    // focus, so the product keeps one.
    let product_focus = ProductFocusScope::new();
    let backend =
        HyprctlLuaBackend::from_env().map_err(|error| format!("backend_init:{error:?}"))?;
    let mut outputs = backend
        .observe_outputs()
        .map_err(|error| format!("observe_outputs:{error:?}"))?;
    outputs.sort_by(|left, right| left.name.cmp(&right.name));

    let gpui_displays = cx.displays();
    for display in &gpui_displays {
        println!(
            "GNOS_UX_GPUI_DISPLAY_OBSERVED id={:?} uuid={} bounds={:?}",
            display.id(),
            display
                .uuid()
                .map(|uuid| uuid.to_string())
                .unwrap_or_else(|_| "UNAVAILABLE".to_owned()),
            display.bounds()
        );
    }

    let mut routes = Vec::with_capacity(outputs.len());
    let mut output_origins: std::collections::BTreeMap<String, (i32, i32)> =
        std::collections::BTreeMap::new();
    for (index, observed) in outputs.iter().enumerate() {
        let target = RuntimeOutputIdentity {
            session_signature: backend.runtime_target().to_owned(),
            name: observed.name.clone(),
        };
        validate_runtime_output_target(&target, &outputs)
            .map_err(|error| format!("runtime_target:{}:{error:?}", target.name))?;
        let display_id = runtime_display_id_for_output_name(cx, &target.name)?;
        // The output origin travels with the scene so tray coordinate hints are
        // derived from the route's real position rather than assumed, including
        // negative origins on outputs left of the primary.
        output_origins.insert(target.name.clone(), (observed.x as i32, observed.y as i32));
        routes.push((
            MonitorSceneId::new((index + 1) as u64),
            RuntimeId::new(target.name),
            display_id,
        ));
    }

    if routes.is_empty() {
        return Err("no_outputs".to_owned());
    }

    let semantic_consumer = SceneSemanticConsumer::default();
    // AUDIO-01 Phase 5: the reconciled output order is the deterministic
    // placement fact for the audio OSD. Published to the process-global OSD
    // before any host exists, so an early observation already has an output to
    // resolve against rather than failing closed.
    let osd = crate::product_audio_osd::AudioOsd::shared(cx);
    let outputs = routes
        .iter()
        .map(|(_, output, _)| output.as_str().to_owned())
        .collect::<Vec<_>>();
    osd.update(cx, |osd, _cx| {
        osd.set_placement(crate::product_audio_osd::AudioOsdPlacement::new(outputs));
    });
    println!(
        "GNOS_UX_AUDIO_OSD_PLACEMENT session={} outputs={:?} primary={}",
        backend.runtime_target(),
        routes
            .iter()
            .map(|(_, output, _)| output.as_str())
            .collect::<Vec<_>>(),
        routes
            .first()
            .map(|(_, output, _)| output.as_str())
            .unwrap_or("UNRESOLVED"),
    );
    for (scene_id, output, display_id) in routes.iter().cloned() {
        // Cloned per host: the request is applied to every routed output.
        let tray_qualification = tray_qualification.clone();
        println!(
            "GNOS_UX_MULTI_OUTPUT_BINDING_REQUEST session={} scene_id={} output={} display_id={:?}",
            backend.runtime_target(),
            scene_id.value(),
            output.as_str(),
            display_id
        );
        let output_origin = output_origins
            .get(output.as_str())
            .copied()
            .unwrap_or((0, 0));
        open_scene_window(
            cx,
            scene_id,
            Some(output.clone()),
            output_origin,
            tray_qualification,
            Some(display_id),
            semantic_consumer.clone(),
            Arc::clone(&application_catalog),
            Some(product_focus.clone()),
        )
        .map_err(|error| format!("output={}:{}", output.as_str(), error))?;
    }

    println!(
        "GNOS_UX_MULTI_OUTPUT_HOSTS_READY session={} count={}",
        backend.runtime_target(),
        routes.len()
    );
    Ok(routes.len())
}

pub(super) fn scene_host_token(scene_id: MonitorSceneId) -> u64 {
    (u64::from(std::process::id()) << 32) | scene_id.value()
}

pub(super) fn close_scene_window(cx: &mut App, handle: AnyWindowHandle) -> Result<(), String> {
    if !cx.windows().contains(&handle) {
        return Ok(());
    }
    handle
        .update(cx, |_, window, _| window.remove_window())
        .map_err(|error| format!("close_scene_window:{error}"))
}

pub(super) fn resolve_current_display_ids(
    cx: &App,
    backend: &HyprctlLuaBackend,
    observed: &[ObservedRuntimeOutput],
) -> Result<BTreeMap<String, DisplayId>, DynamicTopologyError> {
    let mut resolved = BTreeMap::new();
    for output in observed {
        let target = RuntimeOutputIdentity {
            session_signature: backend.runtime_target().to_owned(),
            name: output.name.clone(),
        };
        validate_runtime_output_target(&target, observed).map_err(|error| {
            DynamicTopologyError::Plan(format!("runtime_target:{}:{error:?}", output.name))
        })?;
        let display_id = runtime_display_id_for_output_name(cx, &output.name).map_err(|error| {
            if error.starts_with("missing_gpui_display:") {
                DynamicTopologyError::MappingPending(error)
            } else {
                DynamicTopologyError::Plan(error)
            }
        })?;
        resolved.insert(output.name.clone(), display_id);
    }
    Ok(resolved)
}
