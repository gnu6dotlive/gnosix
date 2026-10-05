#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

use std::{
    env,
    process::{Command, ExitCode},
};

use gnos_ux_sdk::{
    dynamics::{SecondOrderSpec, SettlePolicy, simulate_step_response},
    geometry::AtlasCoverageLevel,
    grammar::{ATOMS, FRAMES, MOLECULES, recipe_catalog, subgrammar_catalog},
    lab::{
        LabLaunchPlan, LabTarget, ScenarioDescriptor, current_binary_dir, find_scenario,
        resolve_repo_root, scenarios, scenarios_for,
    },
    surface::surface_catalog,
};

fn usage() {
    println!(
        "GNOS.UX Surface Lab

USAGE:
  gnos-ux-lab list
  gnos-ux-lab scene [-- extra-args...]
  gnos-ux-lab describe <scene|envelope|bar|dock> [scenario]
  gnos-ux-lab plan <scene|envelope|bar|dock> <scenario> [-- extra-args...]
  gnos-ux-lab run <scene|envelope|bar|dock> <scenario> [-- extra-args...]
  gnos-ux-lab verify <scene|envelope|bar|dock> <scenario> [-- extra-args...]
  gnos-ux-lab surfaces
  gnos-ux-lab grammar
  gnos-ux-lab dynamics
  gnos-ux-lab coverage

The lab exposes the persistent Desktop Scene runtime plus proven Atlas-rich
qualification targets. Rail and Overlay remain PROBE_ONLY and are intentionally
not runnable product targets."
    );
}

fn coverage_label(level: AtlasCoverageLevel) -> &'static str {
    match level {
        AtlasCoverageLevel::AtlasRich => "ATLAS_RICH",
        AtlasCoverageLevel::ProductRuntimeOnly => "PRODUCT_RUNTIME_ONLY",
        AtlasCoverageLevel::ProbeOnly => "PROBE_ONLY",
        AtlasCoverageLevel::DeclaredOnly => "DECLARED_ONLY",
    }
}

fn print_scenario(scenario: &ScenarioDescriptor) {
    let capabilities = scenario
        .capabilities
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let markers = scenario.expected_markers.join(",");
    println!(
        "{} target={} coverage={} binary={} nested={}\n  purpose={}\n  args={}\n  capabilities={}\n  markers={}\n  completion={}",
        scenario.id,
        scenario.target.as_str(),
        coverage_label(scenario.coverage()),
        scenario.binary(),
        scenario.nested_required,
        scenario.purpose,
        scenario.args.join(" "),
        capabilities,
        markers,
        scenario.completion_marker.unwrap_or("INTERACTIVE"),
    );
}

fn parse_target(value: Option<&String>) -> Result<LabTarget, String> {
    let value = value.ok_or_else(|| "missing target".to_owned())?;
    LabTarget::parse(value).ok_or_else(|| {
        format!("unsupported target '{value}'; supported lab targets: scene, envelope, bar, dock")
    })
}

fn parse_scenario(
    target: LabTarget,
    value: Option<&String>,
) -> Result<&'static ScenarioDescriptor, String> {
    let value = value.ok_or_else(|| format!("missing scenario for {}", target.as_str()))?;
    find_scenario(target, value).ok_or_else(|| {
        let available = scenarios_for(target)
            .map(|scenario| scenario.name)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "unknown scenario '{} {}'; available: {}",
            target.as_str(),
            value,
            available
        )
    })
}

fn split_extra(args: &[String], start: usize) -> Vec<String> {
    if let Some(separator) = args[start..].iter().position(|value| value == "--") {
        args[(start + separator + 1)..].to_vec()
    } else {
        Vec::new()
    }
}

fn plan_for(
    target: LabTarget,
    scenario_name: Option<&String>,
    extra: &[String],
) -> Result<LabLaunchPlan, String> {
    let scenario = parse_scenario(target, scenario_name)?;
    let repo = resolve_repo_root().ok_or_else(|| {
        "unable to locate GNOS.UX repository root; run from the repo or set GNOS_UX_REPO_ROOT"
            .to_owned()
    })?;
    let binary_dir = current_binary_dir()
        .ok_or_else(|| "unable to resolve current GNOS.UX target binary directory".to_owned())?;
    Ok(LabLaunchPlan::new(&repo, &binary_dir, scenario, extra))
}

fn print_plan(plan: &LabLaunchPlan) {
    println!("scenario={}", plan.scenario_id);
    println!("harness={}", plan.harness.display());
    println!("candidate={}", plan.candidate.display());
    println!("namespace={}", plan.namespace);
    for (key, value) in &plan.env {
        println!("env.{key}={value}");
    }
    println!("args={}", plan.args.join(" "));
    println!(
        "completion={}",
        plan.completion_marker.unwrap_or("INTERACTIVE")
    );
}

fn execute(plan: LabLaunchPlan) -> Result<(), String> {
    if !plan.harness.is_file() {
        return Err(format!(
            "nested harness is missing: {}",
            plan.harness.display()
        ));
    }
    if !plan.candidate.is_file() {
        return Err(format!(
            "candidate binary is missing: {}\nbuild it first with: cargo build -p gnos-ux-gpui --bin {}",
            plan.candidate.display(),
            plan.candidate
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<qualifier>")
        ));
    }

    print_plan(&plan);
    println!("GNOS_UX_LAB_RUN_BEGIN scenario={}", plan.scenario_id);

    let mut command = Command::new(&plan.harness);
    command.args(&plan.args);
    for (key, value) in &plan.env {
        command.env(key, value);
    }
    let status = command
        .status()
        .map_err(|error| format!("launching {}: {error}", plan.harness.display()))?;

    println!(
        "GNOS_UX_LAB_RUN_END scenario={} success={} code={}",
        plan.scenario_id,
        status.success(),
        status.code().unwrap_or(-1)
    );

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "scenario {} exited unsuccessfully with {status}",
            plan.scenario_id
        ))
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return Ok(());
    };

    match command {
        "list" => {
            println!(
                "GNOS_UX_LAB_CATALOG version=1 scenarios={}",
                scenarios().len()
            );
            for scenario in scenarios() {
                println!(
                    "{}\t{}\t{}\t{}",
                    scenario.id,
                    scenario.target.as_str(),
                    coverage_label(scenario.coverage()),
                    scenario.purpose
                );
            }
            Ok(())
        }
        "scene" => {
            let scenario = find_scenario(LabTarget::Scene, "desktop")
                .ok_or_else(|| "scene.desktop scenario is missing".to_owned())?;
            let extra = split_extra(&args, 1);
            let plan = plan_for(LabTarget::Scene, Some(&scenario.name.to_owned()), &extra)?;
            execute(plan)
        }
        "surfaces" => {
            println!("GNOS_UX_SURFACE_CATALOG version=1");
            for entry in surface_catalog() {
                println!(
                    "{:?}\t{}\t{}\truntime={}\tatlas={}\tscenarios={}",
                    entry.role,
                    entry.identity,
                    entry.namespace,
                    entry.runtime.as_str(),
                    coverage_label(entry.atlas),
                    entry.scenario_count,
                );
            }
            Ok(())
        }
        "grammar" => {
            println!("GNOS_UX_GRAMMAR version=1");
            for subsystem in subgrammar_catalog() {
                println!(
                    "GRAMMAR\t{}@{}\t{}",
                    subsystem.id, subsystem.revision, subsystem.purpose
                );
            }
            for entry in ATOMS.iter().chain(MOLECULES).chain(FRAMES) {
                println!("{}\t{}\t{}", entry.kind.as_str(), entry.id, entry.purpose);
            }
            for recipe in recipe_catalog() {
                println!(
                    "RECIPE\t{}@{}\ttarget={} state={} frame={} molecules={}",
                    recipe.id,
                    recipe.revision,
                    recipe.target,
                    recipe.state,
                    recipe.frame,
                    recipe.molecules.join(",")
                );
            }
            Ok(())
        }
        "dynamics" => {
            let spec = SecondOrderSpec::new(92.0, 13.0);
            let settle = SettlePolicy::new(0.001, 0.01, true);
            println!(
                "GNOS_UX_DYNAMICS_QUALIFICATION grammar=gnos.ux.grammar.dynamics@1 response={:.2} damping={:.2} duration=3.0",
                spec.response(),
                spec.damping()
            );
            for (label, dts) in [
                ("30hz", vec![1.0 / 30.0]),
                ("60hz", vec![1.0 / 60.0]),
                ("120hz", vec![1.0 / 120.0]),
                (
                    "variable",
                    vec![1.0 / 120.0, 1.0 / 60.0, 1.0 / 30.0, 1.0 / 75.0],
                ),
            ] {
                let diagnostics = simulate_step_response(0.0, 0.0, 1.0, spec, settle, &dts, 3.0)
                    .map_err(|error| format!("dynamics qualification {label}: {error}"))?;
                println!(
                    "{label}\tfinal_position={:.6}\tfinal_velocity={:.6}\tpeak_overshoot={:.6}\tsettle_time={}\tfinite={}",
                    diagnostics.final_position,
                    diagnostics.final_velocity,
                    diagnostics.peak_overshoot,
                    diagnostics
                        .settle_time
                        .map(|value| format!("{value:.6}"))
                        .unwrap_or_else(|| "NONE".to_owned()),
                    diagnostics.finite,
                );
            }
            Ok(())
        }
        "coverage" => {
            println!("GNOS_UX_SURFACE_COVERAGE version=1");
            for entry in surface_catalog() {
                println!("{:?}\t{}", entry.role, coverage_label(entry.atlas));
            }
            Ok(())
        }
        "describe" => {
            let target = parse_target(args.get(1))?;
            if let Some(name) = args.get(2) {
                print_scenario(parse_scenario(target, Some(name))?);
            } else {
                for scenario in scenarios_for(target) {
                    print_scenario(scenario);
                }
            }
            Ok(())
        }
        "plan" | "run" | "verify" => {
            let target = parse_target(args.get(1))?;
            let scenario_name = args.get(2);
            let extra = split_extra(&args, 3);
            let scenario = parse_scenario(target, scenario_name)?;
            let mut plan = plan_for(target, scenario_name, &extra)?;

            if command == "verify" {
                let marker = scenario.completion_marker.ok_or_else(|| {
                    format!(
                        "scenario {} is interactive and has no deterministic completion marker",
                        scenario.id
                    )
                })?;
                plan = plan
                    .with_completion_marker(marker)
                    .with_required_markers(scenario.expected_markers);
                if target == LabTarget::Scene {
                    plan.env
                        .push(("GNOS_UX_ECHO_CANDIDATE_LOG".to_owned(), "1".to_owned()));
                    plan.env.push((
                        "GNOS_UX_NESTED_RESIZE_SEQUENCE".to_owned(),
                        "900x560,1200x720".to_owned(),
                    ));
                    plan.env
                        .push(("GNOS_UX_EXPECTED_LAYER_COUNT".to_owned(), "1".to_owned()));
                    if scenario.name == "projection" {
                        plan.env.push((
                            "GNOS_UX_VERIFY_SCENE_RESERVATION".to_owned(),
                            "1".to_owned(),
                        ));
                    }
                }
            }

            if command == "plan" {
                print_plan(&plan);
                Ok(())
            } else {
                execute(plan)
            }
        }
        "help" | "--help" | "-h" => {
            usage();
            Ok(())
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("GNOS_UX_LAB_ERROR {error}");
            usage();
            ExitCode::FAILURE
        }
    }
}
