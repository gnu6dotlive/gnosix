pub mod compositor;
pub mod envelope;
pub mod grammar;
pub mod pointer_regions;
pub mod scene;

use anyhow::{Context, Result};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
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
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_region, wl_shm, wl_surface},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRole {
    Bar,
    Dock,
    Rail,
    Overlay,
}

impl SurfaceRole {
    pub fn product_identity(self) -> &'static str {
        match self {
            Self::Bar => "gnos.ux.bar",
            Self::Dock => "gnos.ux.dock",
            Self::Rail => "gnos.ux.rail",
            Self::Overlay => "gnos.ux.overlay",
        }
    }

    pub fn namespace(self) -> &'static str {
        match self {
            Self::Bar => "gnos.ux:bar",
            Self::Dock => "gnos.ux:dock",
            Self::Rail => "gnos.ux:rail",
            Self::Overlay => "gnos.ux:overlay",
        }
    }

    fn probe_geometry(self) -> ProbeGeometry {
        match self {
            Self::Bar => ProbeGeometry {
                layer: Layer::Top,
                anchors: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                width: 0,
                height: 36,
                margin: [0, 0, 0, 0],
                exclusive_zone: 36,
                color: [0x22, 0x2a, 0x38, 0xf0],
            },
            Self::Dock => ProbeGeometry {
                layer: Layer::Top,
                anchors: Anchor::BOTTOM,
                width: 680,
                height: 72,
                margin: [0, 0, 18, 0],
                exclusive_zone: 0,
                color: [0x2b, 0x35, 0x48, 0xee],
            },
            Self::Rail => ProbeGeometry {
                layer: Layer::Top,
                anchors: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT,
                width: 48,
                height: 0,
                margin: [0, 0, 0, 0],
                exclusive_zone: 0,
                color: [0x1d, 0x27, 0x35, 0xec],
            },
            Self::Overlay => ProbeGeometry {
                layer: Layer::Overlay,
                anchors: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                width: 0,
                height: 0,
                margin: [0, 0, 0, 0],
                exclusive_zone: -1,
                color: [0x3c, 0x55, 0x70, 0x70],
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProbeOptions {
    pub role: SurfaceRole,
    pub max_frames: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct ProbeGeometry {
    layer: Layer,
    anchors: Anchor,
    width: u32,
    height: u32,
    margin: [i32; 4],
    exclusive_zone: i32,
    color: [u8; 4],
}

pub fn run_probe(options: ProbeOptions) -> Result<()> {
    let conn = Connection::connect_to_env().context("connecting to Wayland compositor")?;
    let (globals, mut event_queue) =
        registry_queue_init(&conn).context("initializing Wayland registry")?;
    let qh = event_queue.handle();

    let compositor =
        CompositorState::bind(&globals, &qh).context("wl_compositor is unavailable")?;
    let layer_shell = LayerShell::bind(&globals, &qh).context("wlr-layer-shell is unavailable")?;
    let shm = Shm::bind(&globals, &qh).context("wl_shm is unavailable")?;

    let geometry = options.role.probe_geometry();
    let surface = compositor.create_surface(&qh);

    // The probe is intentionally click-through. It demonstrates shell geometry
    // without claiming input/interaction ownership.
    let empty_input = compositor.wl_compositor().create_region(&qh, ());
    surface.set_input_region(Some(&empty_input));
    empty_input.destroy();

    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        geometry.layer,
        Some(options.role.namespace()),
        None,
    );

    layer.set_anchor(geometry.anchors);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(geometry.width, geometry.height);
    layer.set_margin(
        geometry.margin[0],
        geometry.margin[1],
        geometry.margin[2],
        geometry.margin[3],
    );
    layer.set_exclusive_zone(geometry.exclusive_zone);
    layer.commit();

    let initial_width = geometry.width.max(64);
    let initial_height = geometry.height.max(64);
    let pool = SlotPool::new((initial_width * initial_height * 4) as usize, &shm)
        .context("creating wl_shm slot pool")?;

    let mut state = ProbeState {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        layer,
        role: options.role,
        width: initial_width,
        height: initial_height,
        rendered_frames: 0,
        max_frames: options.max_frames,
        exit: false,
        color: geometry.color,
    };

    while !state.exit {
        event_queue
            .blocking_dispatch(&mut state)
            .context("dispatching Wayland events")?;
    }

    eprintln!(
        "GNOS_UX_EXIT role={:?} frames={}",
        state.role, state.rendered_frames
    );
    Ok(())
}

struct ProbeState {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    role: SurfaceRole,
    width: u32,
    height: u32,
    rendered_frames: u64,
    max_frames: Option<u64>,
    exit: bool,
    color: [u8; 4],
}

impl ProbeState {
    fn draw(&mut self, qh: &QueueHandle<Self>) -> Result<()> {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let stride = width as i32 * 4;
        let required = width as usize * height as usize * 4;
        self.pool
            .resize(required)
            .context("resizing wl_shm slot pool")?;

        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .context("creating wl_shm buffer")?;

        let [r0, g0, b0, a] = self.color;
        for (index, px) in canvas.chunks_exact_mut(4).enumerate() {
            let x = (index % width as usize) as u32;
            let y = (index / width as usize) as u32;
            let t = ((x + y) % 64) as u8;
            let r = r0.saturating_add(t / 6);
            let g = g0.saturating_add(t / 8);
            let b = b0.saturating_add(t / 5);
            let argb = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
            px.copy_from_slice(&argb.to_le_bytes());
        }

        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);

        if self
            .max_frames
            .is_none_or(|max| self.rendered_frames + 1 < max)
        {
            self.layer
                .wl_surface()
                .frame(qh, self.layer.wl_surface().clone());
        }

        buffer
            .attach_to(self.layer.wl_surface())
            .context("attaching wl_shm buffer")?;
        self.layer.commit();

        self.rendered_frames += 1;
        if self
            .max_frames
            .is_some_and(|max| self.rendered_frames >= max)
        {
            self.exit = true;
        }

        Ok(())
    }
}

impl CompositorHandler for ProbeState {
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
            && let Err(error) = self.draw(qh)
        {
            eprintln!("GNOS_UX_RENDER_ERROR {error:#}");
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

impl LayerShellHandler for ProbeState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = configure.new_size.0.max(1);
        self.height = configure.new_size.1.max(1);

        eprintln!(
            "GNOS_UX_READY role={:?} product={} namespace={} size={}x{}",
            self.role,
            self.role.product_identity(),
            self.role.namespace(),
            self.width,
            self.height
        );

        if let Err(error) = self.draw(qh) {
            eprintln!("GNOS_UX_RENDER_ERROR {error:#}");
            self.exit = true;
        }
    }
}

impl OutputHandler for ProbeState {
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

impl ShmHandler for ProbeState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(ProbeState);
delegate_output!(ProbeState);
delegate_layer!(ProbeState);
delegate_shm!(ProbeState);
delegate_registry!(ProbeState);

impl ProvidesRegistryState for ProbeState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

impl Dispatch<wl_region::WlRegion, ()> for ProbeState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_identities_follow_ratified_dotted_taxonomy() {
        assert_eq!(SurfaceRole::Bar.product_identity(), "gnos.ux.bar");
        assert_eq!(SurfaceRole::Dock.product_identity(), "gnos.ux.dock");
        assert_eq!(SurfaceRole::Rail.product_identity(), "gnos.ux.rail");
        assert_eq!(SurfaceRole::Overlay.product_identity(), "gnos.ux.overlay");
    }

    #[test]
    fn namespaces_are_current_and_role_specific() {
        assert_eq!(SurfaceRole::Bar.namespace(), "gnos.ux:bar");
        assert_eq!(SurfaceRole::Dock.namespace(), "gnos.ux:dock");
        assert_eq!(SurfaceRole::Rail.namespace(), "gnos.ux:rail");
        assert_eq!(SurfaceRole::Overlay.namespace(), "gnos.ux:overlay");
    }

    #[test]
    fn probe_roles_have_noninteractive_geometry() {
        assert_eq!(SurfaceRole::Bar.probe_geometry().exclusive_zone, 36);
        assert_eq!(SurfaceRole::Dock.probe_geometry().exclusive_zone, 0);
        assert_eq!(SurfaceRole::Rail.probe_geometry().exclusive_zone, 0);
        assert_eq!(SurfaceRole::Overlay.probe_geometry().exclusive_zone, -1);
    }
}
