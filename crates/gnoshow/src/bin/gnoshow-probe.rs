use anyhow::{Context, Result};
use gnoshow::{ProbeOptions, ProbeSourceKind};

fn main() -> Result<()> {
    let options = parse_options()?;
    gnoshow::run_probe(options)
}

fn parse_options() -> Result<ProbeOptions> {
    let mut source = ProbeSourceKind::Procedural;
    let mut max_frames = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--solid" => source = ProbeSourceKind::Solid,
            "--procedural" => source = ProbeSourceKind::Procedural,
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
                    "gnoshow-probe [--procedural|--solid] [--frames N]\n\n\
                     Developer-only GNOSIX rear-plane render probe.\n\
                     --procedural  animated GPU source (default)\n\
                     --solid       on-change solid source\n\
                     --frames N    exit cleanly after N presented frames"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    if matches!(max_frames, Some(0)) {
        anyhow::bail!("--frames must be greater than zero");
    }

    Ok(ProbeOptions { source, max_frames })
}
