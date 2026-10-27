//! Qualification surface for the generic connective-hull input projection.
//!
//! Read-only. Runs only when `GNOS_UX_QUALIFY_HULL_INPUT` is set, so it is not
//! a product entry point. It consumes the production `ProductLayout::frame()`
//! input regions and logs the discriminating points A (`old \ new`, transparent)
//! and B (inside the paint) plus each pointer move's accept/pass-through
//! classification, for a physical click to be read against.
#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

fn main() {
    // Headless dump stays available without the live flag: it is how the exact
    // discriminating points are obtained before any surface is opened.
    if std::env::args().any(|arg| arg == "--dump") {
        let mut width = 1280.0;
        let mut height = 800.0;
        for arg in std::env::args() {
            if let Some(size) = arg.strip_prefix("--viewport=") {
                let mut parts = size.split('x');
                if let (Some(w), Some(h)) = (parts.next(), parts.next()) {
                    if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                        width = w;
                        height = h;
                    }
                }
            }
        }
        let viewport = gnos_ux_surface::grammar::SurfaceViewport::try_new(width, height)
            .expect("qualification viewport");
        gnos_ux_gpui::hull_input_qualification::dump(viewport);
        return;
    }
    if std::env::var_os("GNOS_UX_QUALIFY_HULL_INPUT").is_none() {
        eprintln!("gnos-ux-hull-input-qualify: set GNOS_UX_QUALIFY_HULL_INPUT=1 to run");
        std::process::exit(2);
    }
    gnos_ux_gpui::hull_input_qualification::run();
}
