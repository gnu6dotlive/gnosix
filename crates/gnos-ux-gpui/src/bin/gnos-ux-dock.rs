#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

//! Qualification-only standalone Dock substrate.
//!
//! Production GNOS.UX is hosted by `gnos-ux-scene.service`. This binary exists
//! only behind the `qualification-surfaces` feature for historical/runtime
//! substrate probes and must not be treated as an independent product host.

use gnos_ux_gpui::run_surface;
use gnos_ux_surface::SurfaceRole;

fn main() {
    run_surface(SurfaceRole::Dock);
}
