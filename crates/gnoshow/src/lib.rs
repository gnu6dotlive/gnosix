mod gpu;
mod source;

use anyhow::{Context, Result};
#[cfg(feature = "hyp-focus")]
use gnos_ux_surface::{
    grammar::{GeometryRect, SurfaceViewport},
    pointer_regions::{PointerRegion, PointerRegionId, PointerRegionRole, PointerRegionSet},
};
use gpu::GpuRenderer;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use source::{FrameDemand, PresentationSource, ProceduralSource, SolidSource};
use std::time::Instant;
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};
#[cfg(feature = "hyp-focus")]
use wayland_client::{Dispatch, protocol::wl_region};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeSurface {
    RearPlane,
    #[cfg(feature = "hyp-focus")]
    HypFocusRegions,
}

#[cfg(feature = "hyp-focus")]
const HYP_FOCUS_SURFACE_WIDTH: u32 = 640;
#[cfg(feature = "hyp-focus")]
const HYP_FOCUS_SURFACE_HEIGHT: u32 = 160;

#[cfg(feature = "hyp-focus")]
fn hyp_focus_regions() -> PointerRegionSet {
    let viewport = SurfaceViewport::try_new(
        HYP_FOCUS_SURFACE_WIDTH as f32,
        HYP_FOCUS_SURFACE_HEIGHT as f32,
    )
    .expect("HYP-FOCUS viewport");

    let region = |id, role, left, top, right, bottom| {
        PointerRegion::try_new(
            PointerRegionId::try_new(id).expect("HYP-FOCUS region id"),
            PointerRegionRole::try_new(role).expect("HYP-FOCUS region role"),
            GeometryRect::try_new(left, top, right, bottom, 0.0).expect("HYP-FOCUS region bounds"),
            true,
        )
        .expect("HYP-FOCUS region")
    };

    PointerRegionSet::try_new(
        viewport,
        vec![
            region("left", "action", 20.0, 20.0, 300.0, 140.0),
            region("right", "status", 340.0, 20.0, 620.0, 140.0),
        ],
    )
    .expect("HYP-FOCUS region set")
}

fn create_probe_layer(
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    qh: &QueueHandle<Gnoshow>,
    probe_surface: ProbeSurface,
) -> Result<LayerSurface> {
    let surface = compositor.create_surface(qh);
    #[cfg(feature = "hyp-focus")]
    if probe_surface == ProbeSurface::HypFocusRegions {
        let declarations = hyp_focus_regions();
        let region = compositor.wl_compositor().create_region(qh, ());
        for declaration in declarations.regions() {
            let bounds = declaration.bounds();
            region.add(
                bounds.left() as i32,
                bounds.top() as i32,
                bounds.width() as i32,
                bounds.height() as i32,
            );
        }
        surface.set_input_region(Some(&region));
        region.destroy();

        let first = &declarations.regions()[0];
        let second = &declarations.regions()[1];
        let first_bounds = first.bounds();
        let second_bounds = second.bounds();
        eprintln!(
            "GNOSHOW_HYP_FOCUS_NATIVE_REGIONS_DECLARED surface=gnos.hyp-focus.lab count={} first={},{},{},{},{} second={},{},{},{},{}",
            declarations.regions().len(),
            first.id().as_str(),
            first_bounds.left() as i32,
            first_bounds.top() as i32,
            first_bounds.width() as i32,
            first_bounds.height() as i32,
            second.id().as_str(),
            second_bounds.left() as i32,
            second_bounds.top() as i32,
            second_bounds.width() as i32,
            second_bounds.height() as i32,
        );
    }

    let (layer_kind, namespace) = match probe_surface {
        ProbeSurface::RearPlane => (Layer::Background, "gnoshow"),
        #[cfg(feature = "hyp-focus")]
        ProbeSurface::HypFocusRegions => (Layer::Top, "gnos.hyp-focus.lab"),
    };
    let layer = layer_shell.create_layer_surface(qh, surface, layer_kind, Some(namespace), None);
    match probe_surface {
        ProbeSurface::RearPlane => {
            layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
            layer.set_exclusive_zone(-1);
            layer.set_size(0, 0);
        }
        #[cfg(feature = "hyp-focus")]
        ProbeSurface::HypFocusRegions => {
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_exclusive_zone(0);
            layer.set_size(HYP_FOCUS_SURFACE_WIDTH, HYP_FOCUS_SURFACE_HEIGHT);
        }
    }
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();
    Ok(layer)
}

pub fn run_probe(options: ProbeOptions) -> Result<()> {
    run_probe_with_surface(options, ProbeSurface::RearPlane)
}

#[cfg(feature = "hyp-focus")]
pub fn run_hyp_focus_probe() -> Result<()> {
    run_probe_with_surface(
        ProbeOptions {
            source: ProbeSourceKind::Solid,
            max_frames: None,
        },
        ProbeSurface::HypFocusRegions,
    )
}

fn run_probe_with_surface(options: ProbeOptions, probe_surface: ProbeSurface) -> Result<()> {
    let conn = Connection::connect_to_env().context("connecting to Wayland compositor")?;
    let (globals, mut event_queue) =
        registry_queue_init(&conn).context("initializing Wayland registry")?;
    let qh = event_queue.handle();

    let compositor =
        CompositorState::bind(&globals, &qh).context("wl_compositor is unavailable")?;
    let layer_shell = LayerShell::bind(&globals, &qh).context("wlr-layer-shell is unavailable")?;
    let layer = create_probe_layer(&compositor, &layer_shell, &qh, probe_surface)?;

    let source: Box<dyn PresentationSource> = match options.source {
        ProbeSourceKind::Solid => Box::new(SolidSource::new([0.028, 0.035, 0.055, 1.0])),
        ProbeSourceKind::Procedural => Box::new(ProceduralSource),
    };

    let mut state = Gnoshow {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        layer,
        gpu: None,
        width: 1,
        height: 1,
        source,
        started_at: Instant::now(),
        rendered_frames: 0,
        max_frames: options.max_frames,
        exit: false,
    };

    while !state.exit {
        event_queue
            .blocking_dispatch(&mut state)
            .context("dispatching Wayland events")?;
    }

    eprintln!(
        "GNOSHOW_EXIT frames={} source={}",
        state.rendered_frames,
        state.source.label()
    );
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub enum ProbeSourceKind {
    Solid,
    Procedural,
}

#[derive(Clone, Copy, Debug)]
pub struct ProbeOptions {
    pub source: ProbeSourceKind,
    pub max_frames: Option<u64>,
}

struct Gnoshow {
    registry_state: RegistryState,
    output_state: OutputState,
    layer: LayerSurface,
    gpu: Option<GpuRenderer>,
    width: u32,
    height: u32,
    source: Box<dyn PresentationSource>,
    started_at: Instant,
    rendered_frames: u64,
    max_frames: Option<u64>,
    exit: bool,
}

impl Gnoshow {
    fn render(&mut self, qh: &QueueHandle<Self>) -> Result<()> {
        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };

        let params = self.source.frame(self.started_at.elapsed());
        let continuous = self.source.frame_demand() == FrameDemand::Continuous;

        if continuous {
            self.layer
                .wl_surface()
                .frame(qh, self.layer.wl_surface().clone());
        }

        self.layer.wl_surface().damage_buffer(
            0,
            0,
            self.width.min(i32::MAX as u32) as i32,
            self.height.min(i32::MAX as u32) as i32,
        );

        gpu.render(&params)?;
        self.rendered_frames += 1;

        if self
            .max_frames
            .is_some_and(|max_frames| self.rendered_frames >= max_frames)
        {
            self.exit = true;
        }

        Ok(())
    }
}

impl CompositorHandler for Gnoshow {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if surface == self.layer.wl_surface()
            && let Err(error) = self.render(qh)
        {
            eprintln!("GNOSHOW_RENDER_ERROR {error:#}");
            self.exit = true;
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Gnoshow {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = configure.new_size.0.max(1);
        self.height = configure.new_size.1.max(1);

        if self.gpu.is_none() {
            match GpuRenderer::new(conn, layer, self.width, self.height) {
                Ok(gpu) => {
                    eprintln!(
                        "GNOSHOW_READY source={} size={}x{} adapter={:?} format={:?}",
                        self.source.label(),
                        self.width,
                        self.height,
                        gpu.adapter_name(),
                        gpu.format()
                    );
                    self.gpu = Some(gpu);
                }
                Err(error) => {
                    eprintln!("GNOSHOW_GPU_INIT_ERROR {error:#}");
                    self.exit = true;
                    return;
                }
            }
        } else if let Some(gpu) = &mut self.gpu {
            gpu.resize(self.width, self.height);
        }

        if let Err(error) = self.render(qh) {
            eprintln!("GNOSHOW_RENDER_ERROR {error:#}");
            self.exit = true;
        }
    }
}

impl OutputHandler for Gnoshow {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

delegate_compositor!(Gnoshow);
delegate_output!(Gnoshow);
delegate_layer!(Gnoshow);
delegate_registry!(Gnoshow);

impl ProvidesRegistryState for Gnoshow {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

#[cfg(feature = "hyp-focus")]
impl Dispatch<wl_region::WlRegion, ()> for Gnoshow {
    fn event(
        _state: &mut Self,
        _region: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(all(test, feature = "hyp-focus"))]
mod tests {
    use super::*;

    #[test]
    fn hyp_focus_regions_use_shared_native_pointer_region_model() {
        let declarations = hyp_focus_regions();

        assert_eq!(declarations.regions().len(), 2);
        assert_eq!(
            declarations.viewport().width,
            HYP_FOCUS_SURFACE_WIDTH as f32
        );
        assert_eq!(
            declarations.viewport().height,
            HYP_FOCUS_SURFACE_HEIGHT as f32
        );
        assert!(
            declarations
                .regions()
                .iter()
                .all(|region| region.decoratable())
        );
        assert_eq!(declarations.regions()[0].id().as_str(), "left");
        assert_eq!(declarations.regions()[1].id().as_str(), "right");
    }
}
