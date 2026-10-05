//! LAB ONLY: three native panels for operator adjudication; no live catalog or executor.
use gnos_ux_gpui::lab_topology_view::TopologyLabView;
use gpui::{
    App, AppContext, Bounds, WindowBackgroundAppearance, WindowBounds, WindowOptions, px, size,
};
use std::{env, path::PathBuf};
fn main() {
    let mut control = None;
    let mut ack = None;
    let mut trace = None;
    for arg in env::args().skip(1) {
        if let Some(path) = arg.strip_prefix("--control=") {
            control = Some(PathBuf::from(path));
        } else if let Some(path) = arg.strip_prefix("--ack=") {
            ack = Some(PathBuf::from(path));
        } else if let Some(path) = arg.strip_prefix("--trace=") {
            trace = Some(PathBuf::from(path));
        } else {
            eprintln!(
                "usage: gnos-ux-topology-lab [--control=/tmp/command.json] [--ack=/tmp/paint.json] [--trace=/tmp/trace.jsonl]"
            );
            std::process::exit(2);
        }
    }
    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(3840.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("gnos.ux.topology.lab".into()),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|cx| {
                    TopologyLabView::new(control, ack, trace, cx)
                        .expect("valid isolated topology lab")
                })
            },
        )
        .expect("opening topology comparison lab");
    });
}
