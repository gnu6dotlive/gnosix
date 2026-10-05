use anyhow::{Context, Result};
use gnos_ux_surface::{ProbeOptions, SurfaceRole};

fn main() -> Result<()> {
    let mut role = SurfaceRole::Bar;
    let mut max_frames = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--role" => {
                let value = args.next().context("--role requires a value")?;
                role = match value.as_str() {
                    "bar" => SurfaceRole::Bar,
                    "dock" => SurfaceRole::Dock,
                    "rail" => SurfaceRole::Rail,
                    "overlay" => SurfaceRole::Overlay,
                    other => anyhow::bail!("unknown role: {other}"),
                };
            }
            "--frames" => {
                let value = args.next().context("--frames requires an integer")?;
                max_frames = Some(
                    value
                        .parse::<u64>()
                        .context("--frames must be a positive integer")?,
                );
            }
            "-h" | "--help" => {
                println!(
                    "gnos-ux-probe [--role bar|dock|rail|overlay] [--frames N]\n\n\
                     Developer-only gnos.ux layer-shell surface probe."
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    if matches!(max_frames, Some(0)) {
        anyhow::bail!("--frames must be greater than zero");
    }

    gnos_ux_surface::run_probe(ProbeOptions { role, max_frames })
}
