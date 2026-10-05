#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

use std::process::ExitCode;

use gnos_ux_gpui::scene_runtime::run_scene;

fn main() -> ExitCode {
    ExitCode::from(run_scene().clamp(0, u8::MAX as i32) as u8)
}
