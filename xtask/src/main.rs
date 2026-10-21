//! `cargo xtask` — the canonical local-first verification, provenance and CI
//! entry point for GNOSIX.
//!
//! The point of this crate is that there is exactly one qualification path.
//! A developer, an agent, and CI all ask the same questions of the same
//! definitions and get the same typed answers. When local truth and CI truth
//! are separate implementations they drift, and the drift is invisible until
//! the moment it matters.
//!
//! Commands:
//!
//! - `verify` — the full local contract, including host/live checks.
//! - `native` — repository-hermetic + host-native build/test qualification.
//! - `ci`     — repository-hermetic checks only; portable remote confirmation.
//! - `provenance` — a machine-readable record of what ran.
//! - `source-gates` — the enduring source invariants.
//! - `explain` — list the checks and their classes without running anything.
//!
//! Default output goes to `target/xtask/`, which is ignored. Running
//! verification therefore cannot dirty the tree it is verifying. Retaining
//! evidence is an explicit operation with an atomic, non-overwriting write.

mod checks;
mod evidence;
mod keybindings;
mod model;
mod provenance;
mod runner;
mod sourcegates;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
};

use checks::Profile;
use model::{Disposition, VerificationReport};

const USAGE: &str = "\
cargo xtask <command>

Commands:
  verify         Run the full local contract: repository, native and host/live checks.
  native         Run repository-hermetic plus host-native source/build checks.
                 This is the local pre-publication qualification on Gnosix.
  ci             Run repository-hermetic checks only. Portable remote CI uses
                 this profile and does not impersonate the Gnosix host.
  provenance     Print a provenance record for the current tree.
  source-gates   Evaluate the enduring source invariants.
  keybindings    Read-only gesture, binding, browser-role and dry-run inspector.
  explain        List the checks and their classes without running them.

Options:
  --output <dir>     Retain summary, provenance and logs into <dir>.
                     Must not already exist unless --overwrite is given.
  --overwrite        Permit replacing files in the output directory.
  --only <name>      Run only the named check. Repeatable.
  --rule <name>      For source-gates: evaluate one named rule.
  --root <dir>       For source-gates: evaluate against this tree instead of
                     the repository, so a rule can be proven to go red without
                     editing product source.
  --json             Print the summary as JSON on stdout.
  -h, --help         Show this message.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let (command, flag_position) = match split_command(&args) {
        Some(found) => found,
        None => {
            eprint!("no command given\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    // `keybindings` owns a small, fixed command grammar (`inspect`,
    // `explain-super`, and so on). Do not feed those domain arguments through
    // the generic xtask option parser, which would otherwise warn that they
    // are unrecognized despite the inspector consuming them correctly.
    if command == "keybindings" {
        return keybindings::run(&args[flag_position + 1..]);
    }

    let root = repository_root();
    let options = Options::parse(&args[flag_position + 1..], &args[..flag_position]);

    match command.as_str() {
        "verify" => execute(&root, Profile::Verify, &options),
        "native" => execute(&root, Profile::Native, &options),
        "ci" => execute(&root, Profile::Ci, &options),
        "provenance" => print_provenance(&root),
        "source-gates" | "source-gate" => {
            let target = options.root.clone().unwrap_or(root);
            run_source_gates(&target, &options)
        }
        "explain" => explain(),
        other => {
            eprintln!("unknown command: {other}\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Flags that consume the following argument as their value.
///
/// Needed because `--root /some/path source-gates` would otherwise read the
/// path as the command. Taking argv[0] as the command instead made the
/// flag-first form print usage and exit 2, which reads as "unknown command" for
/// what is really an argument-order mistake.
const VALUE_FLAGS: &[&str] = &["--output", "--only", "--rule", "--root"];

/// Split the command out of the argument list, skipping flags and their values.
fn split_command(args: &[String]) -> Option<(String, usize)> {
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if VALUE_FLAGS.contains(&arg.as_str()) {
            index += 2;
            continue;
        }
        if !arg.starts_with('-') {
            return Some((arg.clone(), index));
        }
        index += 1;
    }
    None
}

/// The repository root to qualify.
///
/// Resolved at **runtime** from the current working directory, not baked in at
/// compile time.
///
/// The compile-time version used `env!("CARGO_MANIFEST_DIR")`, and that turned
/// out to be a genuine false-verdict hazard rather than a cosmetic one. Cargo
/// fingerprints a build from its source files, and a `env!` value is not part
/// of that fingerprint. So a binary built from one checkout and left in a
/// shared target directory is considered fresh forever while carrying the
/// absolute path of whichever tree it was originally built from - and if that
/// tree is edited, moved, or deleted, `cargo xtask` reports a confident verdict
/// about a repository the caller is not standing in. It surfaced here as six
/// source rules reporting HARNESS_ERROR against a directory that had been
/// cleaned up, which at least failed loudly; the same mechanism pointing at an
/// intact but *different* tree would pass or fail on the wrong source without
/// saying so.
///
/// Which tree is being qualified is not a detail. It is the question every
/// verdict answers, so it is answered by where the caller is standing.
fn repository_root() -> PathBuf {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| find_workspace_root(&cwd))
        .or_else(|| {
            // Fall back to the tree this binary was built from, so a direct
            // invocation from outside any workspace still does something
            // sensible rather than nothing.
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Walk up from `start` looking for the workspace root.
///
/// A directory is the workspace root when its `Cargo.toml` declares a
/// `[workspace]` table *and* lists this crate as a member. Both halves are
/// needed: the member list alone is not enough, because `xtask/Cargo.toml`
/// contains `name = "xtask"` and so a bare search for `"xtask"` matches the
/// crate's own manifest and resolves the root to the crate directory, from
/// where no product source is reachable.
fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        let Ok(text) = std::fs::read_to_string(candidate.join("Cargo.toml")) else {
            continue;
        };
        if text.contains("[workspace]") && text.contains("\"xtask\"") {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

#[derive(Default)]
struct Options {
    output: Option<PathBuf>,
    overwrite: bool,
    only: Vec<String>,
    rule: Option<String>,
    json: bool,
    /// Evaluate source rules against this tree instead of the repository.
    ///
    /// Exists so a rule can be proven to go red against real files without
    /// editing real product source. The earlier gates could only be tested by
    /// mutating the code they govern and hoping the restoration worked, and a
    /// silently-failed restoration is exactly the defect this project has hit
    /// three times. A fixture tree cannot be left dirty, because a fixture is
    /// not the repository.
    root: Option<PathBuf>,
}

impl Options {
    /// Parse `args` (the tail after the command) plus `leading` (any flags that
    /// appeared before it).
    fn parse(args: &[String], leading: &[String]) -> Self {
        let mut options = Self::default();
        let combined: Vec<String> = leading.iter().chain(args.iter()).cloned().collect();
        let args = &combined[..];
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--output" => {
                    index += 1;
                    if let Some(value) = args.get(index) {
                        options.output = Some(PathBuf::from(value));
                    }
                }
                "--overwrite" => options.overwrite = true,
                "--json" => options.json = true,
                "--root" => {
                    index += 1;
                    if let Some(value) = args.get(index) {
                        options.root = Some(PathBuf::from(value));
                    }
                }
                "--only" => {
                    index += 1;
                    if let Some(value) = args.get(index) {
                        options.only.push(value.clone());
                    }
                }
                "--rule" => {
                    index += 1;
                    options.rule = args.get(index).cloned();
                }
                other => eprintln!("warning: ignoring unrecognised argument {other}"),
            }
            index += 1;
        }
        options
    }
}

fn execute(root: &Path, profile: Profile, options: &Options) -> ExitCode {
    let log_root = runner::default_log_root(root);
    if let Err(error) = std::fs::create_dir_all(&log_root) {
        eprintln!(
            "HARNESS_ERROR: could not create log root {}: {error}",
            log_root.display()
        );
        return ExitCode::FAILURE;
    }

    let selected: Vec<checks::Check> = checks::checks_for(profile)
        .into_iter()
        .filter(|c| options.only.is_empty() || options.only.contains(&c.name.to_owned()))
        .collect();

    if selected.is_empty() {
        eprintln!("HARNESS_ERROR: no checks selected; an empty gate must never report success");
        return ExitCode::FAILURE;
    }

    let started = std::time::Instant::now();
    let started_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();

    let mut results = Vec::with_capacity(selected.len());
    for check in &selected {
        let log_path = log_root.join(format!("{}.log", check.name));
        // Most checks are a spawned command. A few are structural assertions
        // with no command to spawn; they dispatch to their own evaluator
        // rather than pretending they were processes with an exit status.
        let result = match &check.in_process {
            Some(checks::InProcess::InstalledAuthority) => {
                runner::check_installed_authority(root, check.name, check.class, &log_path)
            }
            Some(checks::InProcess::SourceRule(rule)) => {
                runner::check_source_rule(root, rule, &log_path)
            }
            None => runner::run_logged(check.name, check.class, &check.argv, &log_path, root),
        };
        print_line(&result);
        // Every check runs even after an earlier one fails, so a single
        // broken target cannot hide the diagnostics of the rest.
        results.push(result);
    }

    let duration = started.elapsed();
    let source = provenance::collect_source(root);
    let overall = VerificationReport::aggregate(results.clone());
    let report = VerificationReport {
        schema: "gnosix.xtask.verification.v1".to_owned(),
        profile: profile.as_str().to_owned(),
        git_commit: source.git_commit,
        dirty: source.dirty,
        results,
        overall,
        started_unix_ms,
        duration_ms: duration.as_millis() as u64,
    };

    print_summary(&report, options.json);

    let observed = observed_test_counts(&log_root, &report);
    let record = provenance::collect(root, &report, observed);

    // Working output stays in target/, which is ignored, so the tree is
    // unchanged by a verification run.
    let summary_path = log_root.join("summary.json");
    let provenance_path = log_root.join("provenance.json");
    let _ = write_json(&summary_path, &report);
    let _ = write_json(&provenance_path, &record);

    if let Some(destination) = &options.output
        && let Err(error) = export(root, &log_root, destination, options.overwrite)
    {
        eprintln!("HARNESS_ERROR: {error}");
        return ExitCode::FAILURE;
    }

    if report.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn export(root: &Path, log_root: &Path, destination: &Path, overwrite: bool) -> Result<(), String> {
    if destination.exists() && !overwrite {
        return Err(format!(
            "output directory {} already exists; pass --overwrite to replace it",
            destination.display()
        ));
    }
    std::fs::create_dir_all(destination)
        .map_err(|error| format!("could not create {}: {error}", destination.display()))?;
    let outcomes = evidence::export_directory(root, log_root, destination, overwrite)?;
    for outcome in &outcomes {
        println!("evidence: {}", outcome.describe());
    }
    Ok(())
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("could not serialise {}: {error}", path.display()))?;
    evidence::write_atomic(path, &text, true)
        .map(|_| ())
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Extract observed test counts from the logs.
///
/// This is reporting, never gating. The reason it lives here rather than in
/// the check definition is that a count is an observation about a run, not a
/// property to assert: encoding `129` as an invariant would make a
/// legitimate new test fail qualification.
fn observed_test_counts(log_root: &Path, report: &VerificationReport) -> BTreeMap<String, String> {
    let mut counts = BTreeMap::new();
    for result in report
        .results
        .iter()
        .filter(|r| r.name.starts_with("test-"))
    {
        let Ok(text) = std::fs::read_to_string(&result.log_path) else {
            continue;
        };
        // A test count is a *reported observation*, so reading it from a log
        // is appropriate here. It is never used to decide a disposition.
        for line in text.lines() {
            if line.starts_with("test result: ok.") {
                if let Some(count) = line.split_whitespace().nth(3) {
                    counts.insert(result.name.to_owned(), count.to_owned());
                }
                break;
            }
        }
    }
    let _ = log_root;
    counts
}

fn print_line(result: &model::CheckResult) {
    let disposition = result.disposition.as_str();
    let status = result
        .exit_status
        .map(|code| code.to_string())
        .unwrap_or_else(|| "-".to_owned());
    println!(
        "{disposition:<17} {:<34} exit={status:<4} {:>6}ms  {}",
        result.name,
        result.duration_ms,
        result.class.as_str()
    );
    if let Some(excerpt) = &result.diagnostic_excerpt
        && !result.is_success()
    {
        for line in excerpt.lines().take(12) {
            println!("    | {line}");
        }
        println!("    | full log: {}", result.log_path.display());
    }
}

fn print_summary(report: &VerificationReport, json: bool) {
    println!();
    if json {
        match serde_json::to_string_pretty(report) {
            Ok(text) => println!("{text}"),
            Err(error) => println!("{{\"error\":\"could not serialise summary: {error}\"}}"),
        }
    }
    let passed = report.results.iter().filter(|r| r.is_success()).count();
    println!(
        "{}: {} of {} checks passed in {}ms",
        report.profile,
        passed,
        report.results.len(),
        report.duration_ms
    );
    for failure in report.failures() {
        println!(
            "  FAILED {} [{}] exit={:?} — {}",
            failure.name,
            failure.class.as_str(),
            failure.exit_status,
            runner::describe(&failure.argv)
        );
    }
    println!("overall: {}", report.overall);
}

fn print_provenance(root: &Path) -> ExitCode {
    // Provenance describes a run. With no run in this invocation, the
    // qualification section says so rather than inventing a pass.
    let empty = VerificationReport {
        schema: "gnosix.xtask.verification.v1".to_owned(),
        profile: "provenance-only".to_owned(),
        git_commit: None,
        dirty: None,
        results: Vec::new(),
        overall: Disposition::HarnessError,
        started_unix_ms: 0,
        duration_ms: 0,
    };
    let record = provenance::collect(root, &empty, BTreeMap::new());
    match serde_json::to_string_pretty(&record) {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("HARNESS_ERROR: could not serialise provenance: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn run_source_gates(root: &Path, options: &Options) -> ExitCode {
    let wanted: Vec<&str> = match &options.rule {
        Some(rule) => vec![rule.as_str()],
        None => sourcegates::rules().iter().map(|(name, _)| *name).collect(),
    };

    let mut failed = false;
    for (name, _) in sourcegates::rules() {
        if !wanted.contains(&name) {
            continue;
        }
        match sourcegates::evaluate(name, root) {
            Ok(finding) => {
                println!(
                    "{:<8} {:<38} {}",
                    if finding.satisfied {
                        "PASS"
                    } else {
                        "CHECK_FAIL"
                    },
                    finding.rule,
                    finding.detail
                );
                failed |= !finding.satisfied;
            }
            Err(error) => {
                println!("HARNESS_ERROR {name}: {error}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn explain() -> ExitCode {
    println!("{:<34} {:<22} command", "check", "class");
    for check in checks::catalogue() {
        println!(
            "{:<34} {:<22} {}",
            check.name,
            check.class.as_str(),
            runner::describe(&check.argv)
        );
    }
    let total = checks::catalogue().len();
    let ci = checks::checks_for(Profile::Ci).len();
    let native = checks::checks_for(Profile::Native).len();
    println!();
    println!("{total} checks defined; {ci} portable ci; {native} native source/build");
    println!("host-native/live/human checks never run in remote CI by construction");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
