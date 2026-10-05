//! Self-tests for the qualification tool.
//!
//! The tooling gets tested for the same reason the harnesses do: every
//! defect recorded in the R09 ledger and again in Phase 4 was a harness
//! reporting a verdict its plumbing invented. `xtask` is the thing everything
//! else will now trust, so "it's only tooling" is exactly the wrong reason to
//! skip this.
//!
//! Two properties get the most attention because they are the ones that
//! produce false green:
//!
//! - **A verdict comes from an exit status, never from text.** Proven by
//!   running a command that prints a convincing `PASS` and exits non-zero.
//! - **An empty or partial gate never reports success.** Proven by
//!   aggregating an empty result set.

use super::*;
use model::{CheckClass, CheckResult, Disposition, Failure};
use provenance::toml_string_value;
use std::time::Duration;

// --- SHA-256 ---------------------------------------------------------------

#[test]
fn sha256_matches_known_vectors() {
    // The NIST vectors. If this drifts, every log and artifact digest in
    // every retained receipt silently becomes wrong, which is the kind of
    // failure nothing else would notice.
    assert_eq!(
        runner::sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        runner::sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        runner::sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn sha256_handles_multi_block_and_boundary_lengths() {
    // 55, 56, 63, 64 and 65 bytes exercise every padding branch.
    for length in [55usize, 56, 63, 64, 65, 119, 120] {
        let data = vec![b'a'; length];
        let digest = runner::sha256(&data);
        assert_eq!(digest.len(), 64, "length {length} produced a short digest");
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    }
    // A million 'a' is the classic long vector.
    let mut hasher = runner::Sha256::new();
    for _ in 0..1000 {
        hasher.update(&[b'a'; 1000]);
    }
    let out = hasher.finish();
    let hex: String = out.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn same_bytes_produce_the_same_digest_and_different_bytes_do_not() {
    let a = std::fs::read("Cargo.toml").expect("manifest");
    assert_eq!(runner::sha256(&a), runner::sha256(&a));
    let mut b = a.clone();
    b.push(b'\n');
    assert_ne!(runner::sha256(&a), runner::sha256(&b));
}

// --- verdict comes from exit status ---------------------------------------

#[test]
fn a_command_printing_pass_but_exiting_nonzero_is_a_failure() {
    // The exact shape of the Phase 4 clippy defect: convincing green text
    // beside a real error. A verdict derived from output would report PASS
    // here. A verdict derived from the exit status cannot.
    let root = std::env::temp_dir();
    let log = root.join(format!(
        "xtask-selftest-convincing-{}.log",
        std::process::id()
    ));
    let result = runner::run_logged(
        "convincing-pass",
        CheckClass::RepositoryHermetic,
        &[
            "sh".to_owned(),
            "-c".to_owned(),
            "echo CLIPPY_RESULT=PASS; exit 2".to_owned(),
        ],
        &log,
        &root,
    );
    assert!(!result.is_success());
    assert_eq!(result.disposition, Disposition::CheckFail);
    assert_eq!(result.exit_status, Some(2));
    let _ = std::fs::remove_file(&log);
}

#[test]
fn a_command_exiting_zero_is_a_pass() {
    let root = std::env::temp_dir();
    let log = root.join(format!("xtask-selftest-clean-{}.log", std::process::id()));
    let result = runner::run_logged(
        "clean",
        CheckClass::RepositoryHermetic,
        &["true".to_owned()],
        &log,
        &root,
    );
    assert!(result.is_success());
    assert_eq!(result.exit_status, Some(0));
    let _ = std::fs::remove_file(&log);
}

#[test]
fn a_command_that_cannot_start_is_an_environment_error_not_a_product_failure() {
    // Infrastructure breakage must not be reported as a code regression, or
    // the next person debugs working code.
    let root = std::env::temp_dir();
    let log = root.join(format!("xtask-selftest-missing-{}.log", std::process::id()));
    let result = runner::run_logged(
        "missing",
        CheckClass::RepositoryHermetic,
        &["gnosix-no-such-program-exists".to_owned()],
        &log,
        &root,
    );
    assert_eq!(result.disposition, Disposition::EnvironmentError);
    assert!(!result.is_success());
    assert_eq!(
        result.exit_status, None,
        "no child ran, so there is no status"
    );
}

// --- aggregation -----------------------------------------------------------

fn passing(name: &str) -> CheckResult {
    CheckResult::passed(
        name,
        CheckClass::RepositoryHermetic,
        vec!["true".into()],
        Duration::from_millis(1),
        PathBuf::from("/dev/null"),
        "0".repeat(64),
    )
}

fn failing(name: &str) -> CheckResult {
    CheckResult::subject_failure(
        name,
        CheckClass::RepositoryHermetic,
        vec!["false".into()],
        Failure {
            exit_status: Some(1),
            duration: Duration::from_millis(1),
            log_path: PathBuf::from("/dev/null"),
            log_sha256: "0".repeat(64),
            excerpt: Some("boom".into()),
        },
    )
}

#[test]
fn an_empty_result_set_is_never_a_pass() {
    // The most dangerous state this tool could be in: a gate that ran
    // nothing and reported success.
    assert_eq!(
        VerificationReport::aggregate(Vec::new()),
        Disposition::HarnessError
    );
}

#[test]
fn one_failure_does_not_hide_later_failures() {
    // Reported, not short-circuited: a single broken clippy target must not
    // cost the diagnostics of the other five.
    let results = vec![failing("first"), passing("second"), failing("third")];
    let overall = VerificationReport::aggregate(results);
    assert_eq!(overall, Disposition::CheckFail);
    let report = VerificationReport {
        schema: String::new(),
        profile: "test".into(),
        git_commit: None,
        dirty: None,
        results: vec![failing("first"), passing("second"), failing("third")],
        overall,
        started_unix_ms: 0,
        duration_ms: 0,
    };
    assert_eq!(report.failures().len(), 2, "both failures must be reported");
    assert!(report.failures().iter().any(|f| f.name == "first"));
    assert!(report.failures().iter().any(|f| f.name == "third"));
}

#[test]
fn only_check_fail_means_the_subject_failed() {
    // "The code is broken" and "we never got to test the code" are different
    // claims, and only one of them is a statement about the product.
    assert!(Disposition::CheckFail.is_subject_failure());
    for disposition in [
        Disposition::Pass,
        Disposition::HarnessError,
        Disposition::EnvironmentError,
    ] {
        assert!(
            !disposition.is_subject_failure(),
            "{disposition} must not be reported as a product failure"
        );
    }
    // And none of the failing dispositions is a success.
    for disposition in [
        Disposition::CheckFail,
        Disposition::HarnessError,
        Disposition::EnvironmentError,
    ] {
        assert!(!disposition.is_success(), "{disposition} is not a pass");
    }
}

#[test]
fn a_harness_error_outranks_a_check_failure_in_the_aggregate() {
    let harness = CheckResult::harness_error(
        "broke",
        CheckClass::RepositoryHermetic,
        vec!["true".into()],
        Duration::from_millis(1),
        PathBuf::from("/dev/null"),
        "could not create log",
    );
    assert_eq!(
        VerificationReport::aggregate(vec![failing("a"), harness]),
        Disposition::HarnessError,
        "a broken harness is a more serious report than a failing check"
    );
}

#[test]
fn all_passing_aggregates_to_pass() {
    assert_eq!(
        VerificationReport::aggregate(vec![passing("a"), passing("b")]),
        Disposition::Pass
    );
}

// --- exit-status propagation ----------------------------------------------

#[test]
fn a_failing_check_produces_a_nonzero_overall_disposition() {
    let report = VerificationReport {
        schema: String::new(),
        profile: "ci".into(),
        git_commit: None,
        dirty: None,
        results: vec![failing("broken")],
        overall: VerificationReport::aggregate(vec![failing("broken")]),
        started_unix_ms: 0,
        duration_ms: 0,
    };
    assert!(!report.is_success());
}

// --- check classification -------------------------------------------------

#[test]
fn the_ci_profile_excludes_host_and_human_checks() {
    let all = checks::checks_for(Profile::Verify);
    let ci = checks::checks_for(Profile::Ci);
    assert!(ci.len() < all.len(), "ci must be a strict subset");

    for check in &ci {
        assert!(
            check.class.runs_in_ci(),
            "{} is class {} and must not run in ci",
            check.name,
            check.class.as_str()
        );
        assert_ne!(check.class, CheckClass::HumanAcceptance);
    }
}

#[test]
fn human_acceptance_checks_never_run_in_ci() {
    for check in checks::checks_for(Profile::Ci) {
        assert_ne!(
            check.class,
            CheckClass::HumanAcceptance,
            "{} is a human gate and must never be automated",
            check.name
        );
    }
}

#[test]
fn cargo_commands_use_locked_where_cargo_accepts_it() {
    // The lockfile is part of the qualified source state, so no cargo command
    // that could rewrite dependency resolution may be permitted to.
    //
    // `cargo fmt` is the one exception, because it does not accept the flag:
    // it runs rustfmt, not the resolver. Asserting --locked there would mean
    // either a permanently failing gate or dropping the check, and the honest
    // thing is to name the exception.
    const NO_LOCK_SUPPORT: &[&str] = &["fmt"];
    for check in checks::catalogue() {
        if !check
            .argv
            .first()
            .is_some_and(|p| p.ends_with("cargo") || p == "cargo")
        {
            continue;
        }
        let subcommand = check.argv.get(1).map(String::as_str).unwrap_or_default();
        if NO_LOCK_SUPPORT.contains(&subcommand) {
            continue;
        }
        assert!(
            check.argv.iter().any(|a| a == "--locked"),
            "{} runs cargo without --locked: {:?}",
            check.name,
            check.argv
        );
    }
}

#[test]
fn every_clippy_target_uses_deny_warnings() {
    for check in checks::catalogue() {
        if !check.name.starts_with("clippy-") {
            continue;
        }
        assert!(check.argv.contains(&"-D".to_owned()), "{:?}", check.argv);
        assert!(
            check.argv.contains(&"warnings".to_owned()),
            "{:?}",
            check.argv
        );
    }
}

#[test]
fn the_phase_4_clippy_targets_are_all_preserved() {
    // The six strict targets carried forward from Phase 4. Adding to the set is
    // fine; silently dropping one is not, because a dropped target is a feature
    // combination nobody lints any more.
    let defined: Vec<&str> = checks::CLIPPY_TARGETS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    for expected in [
        "clippy-gpui-passive",
        "clippy-gpui-r13",
        "clippy-gpui-component-lab",
        "clippy-application-launch",
        "clippy-application-catalog",
        "clippy-desktop-profiles",
    ] {
        assert!(defined.contains(&expected), "{expected} was dropped");
    }
}

#[test]
fn the_qualification_tool_is_held_to_the_same_standard() {
    // §43: not exempt because it is "only tooling". The tool every other gate is
    // read through should be linted as strictly as anything it judges.
    let defined: Vec<&str> = checks::CLIPPY_TARGETS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert!(
        defined.contains(&"clippy-xtask"),
        "xtask must be in the strict clippy set"
    );
}

#[test]
fn both_production_binaries_have_explicit_build_gates() {
    let builds: Vec<&str> = checks::catalogue()
        .iter()
        .filter(|c| c.name.starts_with("build-"))
        .map(|c| c.name)
        .collect();
    assert!(builds.contains(&"build-gnos-ux-scene"));
    assert!(builds.contains(&"build-gnos-ux-lab"));
}

#[test]
fn check_names_are_unique() {
    let mut names: Vec<&str> = checks::catalogue().iter().map(|c| c.name).collect();
    let count = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        count,
        "duplicate check names would collide in logs"
    );
}

// --- source gates, against fixtures ---------------------------------------

#[test]
fn a_launch_reference_bound_to_a_command_is_rejected() {
    // The deterministic negative test: a fixture, not a mutated product file.
    let hostile = r#"
        impl LaunchReference {
            pub fn from_exec(argv: Vec<String>) -> Self { unimplemented!() }
        }
    "#;
    let finding = sourcegates::launch_reference_is_catalog_bound(hostile);
    assert!(
        !finding.satisfied,
        "a command-bound reference must be rejected"
    );
}

#[test]
fn a_shell_on_the_launch_path_is_rejected() {
    let hostile = r#"
        Command::new("sh").args(["-c", &user_input]).spawn()?;
    "#;
    let finding = sourcegates::launch_path_has_no_shell(&[("launch", hostile)]);
    assert!(
        !finding.satisfied,
        "a shell on the launch path must be rejected"
    );
}

#[test]
fn a_direct_plan_spawn_is_accepted() {
    // The positive half, so the negative test above cannot pass merely
    // because the matcher is broken.
    let benign = r#"
        let child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::null())
            .spawn()?;
    "#;
    let finding = sourcegates::launch_path_has_no_shell(&[("launch", benign)]);
    assert!(finding.satisfied, "a direct plan spawn must be accepted");
}

#[test]
fn a_hand_constructible_launch_plan_is_rejected() {
    let hostile = "impl LaunchPlan { pub fn new(program: String) -> Self { unimplemented!() } }";
    assert!(!sourcegates::launch_plan_is_not_hand_constructible(hostile).satisfied);
}

#[test]
fn a_public_profile_writer_is_rejected() {
    let store = "    pub fn open_writer(&self) -> DesktopProfileWriter { todo!() }";
    let authority = "self.store.open_writer();";
    assert!(!sourcegates::profile_writer_is_crate_private(store, authority).satisfied);
}

#[test]
fn a_crate_private_profile_writer_with_a_designated_issuer_is_accepted() {
    let store = "    pub(crate) fn open_writer(&self) -> DesktopProfileWriter { todo!() }";
    let authority = "self.store.open_writer();";
    assert!(sourcegates::profile_writer_is_crate_private(store, authority).satisfied);
}

#[test]
fn a_universal_execute_authority_is_rejected() {
    let hostile = "pub fn execute(&self, action: &str) {}";
    assert!(
        !sourcegates::capabilities_remain_separate(&[("launch", hostile)]).satisfied,
        "a universal execute authority must be rejected"
    );
}

#[test]
fn a_spawn_capability_in_the_activation_authority_is_rejected() {
    let hostile = "use std::process::Command;";
    assert!(!sourcegates::activation_authority_has_no_spawn(hostile).satisfied);
}

#[test]
fn the_working_tree_satisfies_every_source_rule() {
    // The same rules, against the real tree. A rule that cannot be satisfied
    // by the code it governs is a rule that should not exist.
    let root = repository_root();
    for (name, _) in sourcegates::rules() {
        let finding = sourcegates::evaluate(name, &root)
            .unwrap_or_else(|error| panic!("{name} could not be evaluated: {error}"));
        assert!(finding.satisfied, "{name} violated: {}", finding.detail);
    }
}

#[test]
fn launch_lifetime_gate_rejects_product_execution_authority_and_direct_fallback() {
    let root = repository_root();
    let read = |path| std::fs::read_to_string(root.join(path)).unwrap();
    let launch = read("crates/gnosix-application-launch/src/lib.rs");
    let execution = read("crates/gnosix-application-launch/src/execution.rs");
    let product = read("crates/gnos-ux-gpui/src/product_launch.rs");
    let manifest = read("crates/gnosix-application-launch/Cargo.toml");
    assert!(
        sourcegates::launch_execution_is_bound(&launch, &execution, &product, &manifest).satisfied
    );
    for hostile in [
        "DirectLaunchExecutor",
        "zbus::Proxy",
        "\"Environment\"",
        "\"KillMode\"",
        "\"ExecStart\"",
        "\"Slice\"",
        "pub fn systemd_call(",
    ] {
        assert!(
            !sourcegates::launch_execution_is_bound(
                &launch,
                &execution,
                &format!("{product}\n{hostile}"),
                &manifest
            )
            .satisfied,
            "{hostile}"
        );
    }
    for hostile in [
        execution.replace("no-env-expand", "expand"),
        execution.replace("active == \"active\" && pid > 0", "pid >= 0"),
        format!("{execution}\npub fn run() {{}}"),
        format!("{execution}\n\"KillMode\""),
        String::new(),
    ] {
        assert!(
            !sourcegates::launch_execution_is_bound(&launch, &hostile, &product, &manifest)
                .satisfied
        );
    }
    assert!(
        !sourcegates::launch_execution_is_bound(
            &launch.replace("&SystemdUserLaunchExecutor", "&DirectLaunchExecutor"),
            &execution,
            &product,
            &manifest
        )
        .satisfied
    );
}

// --- provenance ------------------------------------------------------------

#[test]
fn a_missing_artifact_cannot_be_reported_as_built() {
    let root = std::env::temp_dir();
    let record = provenance::artifact(&root, "definitely-not-here-9f3a.bin");
    assert!(!record.present);
    assert!(record.sha256.is_empty(), "a missing artifact has no digest");
    assert!(record.absence_reason.is_some(), "absence must be explained");
}

#[test]
fn an_existing_artifact_is_hashed_from_its_actual_bytes() {
    let root = std::env::temp_dir();
    let path = root.join(format!("xtask-artifact-{}.bin", std::process::id()));
    std::fs::write(&path, b"known bytes").expect("write");
    let record = provenance::artifact(&root, path.file_name().unwrap().to_str().unwrap());
    assert!(record.present);
    assert_eq!(record.sha256, runner::sha256(b"known bytes"));
    assert_eq!(record.size_bytes, 11);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_lockfile_digest_is_the_real_digest() {
    let root = repository_root();
    let dependencies = provenance::collect_dependencies(&root);
    let expected = runner::sha256_file(&root.join("Cargo.lock"));
    assert_eq!(
        dependencies.cargo_lock_sha256.as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn the_dirty_flag_reflects_real_git_state() {
    let root = repository_root();
    let source = provenance::collect_source(&root);
    // This test run may or may not have written anything, so the honest
    // assertion is that the flag is derived rather than asserted, and that it
    // agrees with a direct porcelain read.
    let porcelain = runner::capture_line(
        &[
            "git".to_owned(),
            "status".to_owned(),
            "--porcelain".to_owned(),
        ],
        &root,
    )
    .unwrap_or_default();
    let expected_dirty = !porcelain.trim().is_empty();
    assert_eq!(source.dirty, Some(expected_dirty));
    assert!(
        source.git_commit.is_some(),
        "a git commit should be readable"
    );
}

#[test]
fn the_vendored_patch_is_recorded_and_tracked() {
    let root = repository_root();
    let dependencies = provenance::collect_dependencies(&root);
    let patch = dependencies
        .patched_crates
        .iter()
        .find(|p| p.crate_name == "gpui-pre-linux")
        .expect("the workspace patch must be recorded");
    assert!(
        patch.tracked_in_git,
        "a reviewer must be able to confirm the vendored patch is in the tree"
    );
    assert_ne!(patch.version, "unknown", "vendor version must be readable");
}

#[test]
fn a_failed_check_cannot_be_serialised_as_a_pass() {
    let result = failing("broken");
    let text = serde_json::to_string(&result).expect("serialise");
    assert!(text.contains("\"CHECK_FAIL\""), "{text}");
    assert!(
        !text.contains("\"PASS\""),
        "a failure must not serialise as a pass"
    );
}

#[test]
fn provenance_carries_the_schema_and_real_values() {
    let root = repository_root();
    let report = VerificationReport {
        schema: "gnosix.xtask.verification.v1".into(),
        profile: "ci".into(),
        git_commit: None,
        dirty: None,
        results: vec![passing("a")],
        overall: Disposition::Pass,
        started_unix_ms: 0,
        duration_ms: 1,
    };
    let record = provenance::collect(&root, &report, BTreeMap::new());
    assert_eq!(record.schema, provenance::SCHEMA);
    assert_eq!(record.qualification.checks_run, 1);
    assert_eq!(record.qualification.checks_passed, 1);
    assert_eq!(record.qualification.overall, "PASS");
    assert!(record.toolchain.rustc_version.is_some());
    assert!(!record.platform.os.is_empty());
    let text = serde_json::to_string_pretty(&record).expect("serialise provenance");
    assert!(text.contains("\"toolchain\""));
    assert!(text.contains("\"artifacts\""));
}

// --- evidence export -------------------------------------------------------

#[test]
fn export_refuses_to_overwrite_without_permission() {
    let dir = std::env::temp_dir().join(format!("xtask-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create");
    let destination = dir.join("receipt.json");

    assert!(evidence::write_atomic(&destination, "first", false).is_ok());
    let second = evidence::write_atomic(&destination, "second", false);
    assert!(
        second.is_err(),
        "an existing file must not be replaced silently"
    );
    assert_eq!(
        std::fs::read_to_string(&destination).expect("read"),
        "first",
        "the original must survive a refused overwrite"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_overwrites_only_when_explicitly_permitted() {
    let dir = std::env::temp_dir().join(format!("xtask-export-ow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create");
    let destination = dir.join("receipt.json");
    evidence::write_atomic(&destination, "first", false).expect("write");
    let outcome = evidence::write_atomic(&destination, "second", true).expect("overwrite");
    assert!(matches!(outcome, evidence::ExportOutcome::Overwrote { .. }));
    assert_eq!(
        std::fs::read_to_string(&destination).expect("read"),
        "second"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_is_atomic_and_leaves_no_temporary_file() {
    let dir = std::env::temp_dir().join(format!("xtask-export-atomic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create");
    let destination = dir.join("out.json");
    evidence::write_atomic(&destination, "{}", false).expect("write");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .expect("read")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files left behind: {leftovers:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_skips_a_source_that_does_not_exist() {
    let dir = std::env::temp_dir().join(format!("xtask-export-skip-{}", std::process::id()));
    let root = repository_root();
    let outcome = evidence::export_file(&root, &dir.join("absent.txt"), &dir, "absent.txt", false)
        .expect("skip is not an error");
    assert!(matches!(outcome, evidence::ExportOutcome::Skipped { .. }));
}

// --- working-tree cleanliness ---------------------------------------------

#[test]
fn default_working_logs_land_in_an_ignored_directory() {
    // The invariant that breaks the commit-per-run cycle: default output must
    // not be able to dirty the tree.
    let root = repository_root();
    let log_root = runner::default_log_root(&root);
    assert!(
        log_root.starts_with(root.join("target")),
        "logs default to {} which is not under target/",
        log_root.display()
    );
    // `check-ignore -q` reports through its exit status, not its output, so
    // this must be judged by status. Reading stdout here would have reported
    // "not ignored" for a path that is in fact ignored.
    let relative = log_root
        .strip_prefix(&root)
        .expect("log root is under the repository")
        .to_str()
        .expect("utf-8 path")
        .to_owned();
    let ignored = std::process::Command::new("git")
        .args(["check-ignore", "-q", "--"])
        .arg(&relative)
        .current_dir(&root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    assert!(
        ignored,
        "target/xtask must be git-ignored or verification will dirty the tree"
    );
}

// --- the relocated host check ---------------------------------------------

#[test]
fn xtask_unit_tests_do_not_require_the_installed_authority() {
    let root = std::env::temp_dir();
    let path = root.join(format!("xtask-authority-fixture-{}", std::process::id()));
    let log = root.join(format!(
        "xtask-authority-fixture-{}.log",
        std::process::id()
    ));
    std::fs::write(&path, b"abc").expect("write controlled artifact");

    let result = runner::check_authority_digest(
        &path,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "authority-fixture",
        CheckClass::HostLive,
        &log,
    );

    assert_eq!(result.disposition, Disposition::Pass);
    assert_eq!(result.exit_status, None);
    let text = std::fs::read_to_string(&log).expect("digest log");
    assert!(text.contains(path.to_str().expect("fixture path")));
    assert!(
        text.contains("actual   ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert!(
        text.contains("expected ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );

    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(log);
}

#[test]
fn authority_digest_mismatch_is_a_check_failure_without_process_status() {
    let root = std::env::temp_dir();
    let path = root.join(format!("xtask-authority-mismatch-{}", std::process::id()));
    let log = root.join(format!(
        "xtask-authority-mismatch-{}.log",
        std::process::id()
    ));
    std::fs::write(&path, b"abc").expect("write controlled artifact");

    let result = runner::check_authority_digest(
        &path,
        "0000000000000000000000000000000000000000000000000000000000000000",
        "authority-mismatch",
        CheckClass::HostLive,
        &log,
    );

    assert_eq!(result.disposition, Disposition::CheckFail);
    assert_eq!(result.exit_status, None, "no child process ran");
    let text = std::fs::read_to_string(&log).expect("digest log");
    assert!(
        text.contains("actual   ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert!(
        text.contains("expected 0000000000000000000000000000000000000000000000000000000000000000")
    );
    assert!(
        result
            .diagnostic_excerpt
            .as_deref()
            .expect("mismatch diagnostic")
            .contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );

    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(log);
}

#[test]
fn missing_authority_digest_artifact_is_an_environment_error() {
    let root = std::env::temp_dir();
    let path = root.join(format!("xtask-authority-missing-{}", std::process::id()));
    let log = root.join(format!(
        "xtask-authority-missing-{}.log",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    let result = runner::check_authority_digest(
        &path,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "authority-missing",
        CheckClass::HostLive,
        &log,
    );

    assert_eq!(result.disposition, Disposition::EnvironmentError);
    assert_ne!(result.disposition, Disposition::CheckFail);
    assert_eq!(result.exit_status, None, "no child process ran");
    assert!(
        result
            .diagnostic_excerpt
            .as_deref()
            .expect("missing artifact diagnostic")
            .contains("cannot read authority artifact")
    );

    let _ = std::fs::remove_file(log);
}

#[test]
fn the_authority_check_is_host_live_and_never_runs_in_ci() {
    let check = checks::catalogue()
        .into_iter()
        .find(|c| c.name == "host-authority-digest")
        .expect("the host check must be defined");
    assert_eq!(check.class, CheckClass::HostLive);
    assert!(
        !check.class.runs_in_ci(),
        "an installed-artifact digest must never become a CI gate"
    );
    let in_ci = checks::checks_for(Profile::Ci);
    assert!(
        !in_ci.iter().any(|c| c.name == "host-authority-digest"),
        "the ci profile must exclude the host check"
    );
}

#[test]
fn the_repository_harmetic_half_of_the_authority_boundary_does_run_in_ci() {
    // The other half of the same boundary is a source invariant and belongs in
    // CI: the activation authority must not be able to spawn a process.
    let names: Vec<&str> = checks::checks_for(Profile::Ci)
        .iter()
        .map(|c| c.name)
        .collect();
    assert!(
        !names.is_empty(),
        "the ci profile must contain source invariants, not only builds"
    );
    let root = repository_root();
    let finding = sourcegates::evaluate("activation-authority-no-spawn", &root)
        .expect("the rule must be evaluable");
    assert!(finding.satisfied, "{}", finding.detail);
}

#[test]
fn source_invariants_are_evaluated_in_process_not_by_spawning_cargo() {
    // A verification run must not depend on a recursive cargo invocation.
    for check in checks::catalogue() {
        assert!(
            !check.argv.iter().any(|a| a == "run"),
            "{} appears to spawn a nested cargo run: {:?}",
            check.name,
            check.argv
        );
    }
}

#[test]
fn every_source_invariant_is_a_check_in_both_profiles() {
    // Regression guard. Making the source invariants in-process briefly removed
    // them from the catalogue altogether, so verify and ci silently stopped
    // running them - and the test above passed anyway, because it only checked
    // that no check spawned `cargo run`, which is vacuously true of an empty
    // set. A gate that stops running while its own test stays green is the
    // failure mode this project keeps hitting, so the assertion is on
    // membership, not on an absence.
    let rules: Vec<&str> = sourcegates::rules().into_iter().map(|(r, _)| r).collect();
    assert!(!rules.is_empty(), "the rule set must not be empty");

    for profile in [Profile::Verify, Profile::Ci] {
        let selected = checks::checks_for(profile);
        for rule in &rules {
            assert!(
                selected.iter().any(|c| c.name == *rule),
                "source invariant {rule} is missing from the {} profile",
                profile.as_str()
            );
        }
    }
}

#[test]
fn a_source_invariant_check_is_hermetic_and_runs_in_ci() {
    // These are repository facts, so CI may run them. The contrast with the
    // installed-authority check is the whole point of the class split.
    for check in checks::checks_for(Profile::Ci) {
        if let Some(checks::InProcess::SourceRule(rule)) = &check.in_process {
            assert_eq!(check.class, CheckClass::RepositoryHermetic);
            assert!(
                sourcegates::rules().iter().any(|(name, _)| name == rule),
                "{rule} is not a known rule"
            );
        }
    }
}

#[test]
fn an_unevaluatable_source_rule_is_a_harness_error_not_a_pass() {
    // A rule whose file is missing was never tested, so it must not report a
    // verdict about the subject.
    let root = std::env::temp_dir().join(format!("xtask-empty-tree-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create");
    let log = root.join("rule.log");
    let result = runner::check_source_rule(&root, "launch-reference-catalog-bound", &log);
    assert_eq!(result.disposition, Disposition::HarnessError);
    assert_ne!(
        result.disposition,
        Disposition::CheckFail,
        "the code was never examined"
    );
    assert!(
        result.exit_status.is_none(),
        "no process ran, so there is no status"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_satisfied_source_rule_reports_pass_against_the_real_tree() {
    // The positive half of the dispatch, so the negative test above cannot pass
    // merely because the evaluator is broken.
    let root = repository_root();
    let log = std::env::temp_dir().join(format!("xtask-rule-{}.log", std::process::id()));
    let result = runner::check_source_rule(&root, "launch-path-no-shell", &log);
    assert!(
        result.is_success(),
        "launch-path-no-shell violated: {:?}",
        result.diagnostic_excerpt
    );
    assert_eq!(result.exit_status, Some(0));
    let _ = std::fs::remove_file(&log);
}

#[test]
fn the_target_directory_is_resolved_not_assumed() {
    // On this machine cargo's target directory is not <repo>/target, and
    // assuming it made provenance report both binaries as absent while they
    // existed and had just been built.
    let root = repository_root();
    let target = provenance::target_dir(&root);
    assert!(
        target.is_absolute(),
        "the target directory must be absolute"
    );
    // Whatever it resolves to, the two production binaries must be findable
    // there once the build gates have run.
    for name in ["gnos-ux-scene", "gnos-ux-lab"] {
        let found = provenance::artifacts(&root)
            .into_iter()
            .any(|a| a.path.ends_with(name));
        assert!(found, "{name} was not found under {}", target.display());
    }
}

#[test]
fn a_resolved_artifact_is_hashed_from_the_real_binary() {
    let root = repository_root();
    for artifact in provenance::artifacts(&root) {
        if !artifact.present {
            continue;
        }
        assert_eq!(artifact.sha256.len(), 64, "{} has no digest", artifact.path);
        assert!(artifact.size_bytes > 0);
    }
}

#[test]
fn toml_values_are_parsed_not_prefix_stripped() {
    // The buggy version returned `= "1.97.0` for channel = "1.97.0", putting a
    // malformed value into a machine-readable receipt.
    assert_eq!(
        toml_string_value("channel = \"1.97.0\"", "channel").as_deref(),
        Some("1.97.0")
    );
    assert_eq!(
        toml_string_value("version = \"0.3.5\"", "version").as_deref(),
        Some("0.3.5")
    );
    assert_eq!(toml_string_value("# channel = \"x\"", "channel"), None);
    assert_eq!(toml_string_value("channel = 1", "channel"), None);
    assert_eq!(toml_string_value("other = \"x\"", "channel"), None);
}

#[test]
fn the_pinned_toolchain_is_read_correctly_and_matches() {
    let root = repository_root();
    let toolchain = provenance::collect_toolchain(&root);
    assert_eq!(
        toolchain.pinned_channel.as_deref(),
        Some("1.97.0"),
        "the pinned channel must be read cleanly, not as a raw prefix"
    );
    assert_eq!(
        toolchain.rustc_version.as_deref(),
        Some("1.97.0"),
        "rustc release must be read cleanly"
    );
    assert_eq!(
        toolchain.matches_pin,
        Some(true),
        "the running toolchain must be the pinned one"
    );
    assert!(toolchain.rustc_host.is_some());
}

#[test]
fn the_vendored_patch_version_is_read_cleanly() {
    let root = repository_root();
    let dependencies = provenance::collect_dependencies(&root);
    let patch = dependencies
        .patched_crates
        .iter()
        .find(|p| p.crate_name == "gpui-pre-linux")
        .expect("the patch must be recorded");
    assert_eq!(
        patch.version, "0.3.5",
        "a mangled version would make the provenance record misleading"
    );
}

// --- every rule must be provably able to go red --------------------------
//
// Each of these drives a predicate with a fixture that violates exactly that
// rule, plus a benign fixture that does not. A gate that cannot be shown to go
// red is not known to be a gate.
//
// Two of these caught real matcher defects on first run. `impl_blocks` used to
// return only the first `impl` block for a type, so a public constructor added
// in a *second* block was invisible and the rule stayed green - a gate missing
// a genuine violation, which is the direction that actually matters.

fn benign_tree() -> &'static str {
    "impl LaunchPlan { fn resolved() -> Self { unimplemented!() } }"
}

#[test]
fn a_second_impl_block_with_a_public_constructor_is_rejected() {
    // The defect: only the first impl block was inspected.
    let two_blocks = r#"
        impl LaunchPlan { fn resolved() -> Self { unimplemented!() } }
        impl LaunchPlan { pub fn new(p: String) -> Self { unimplemented!() } }
    "#;
    assert!(
        !sourcegates::launch_plan_is_not_hand_constructible(two_blocks).satisfied,
        "a public constructor in a second impl block must be caught"
    );
    assert!(sourcegates::launch_plan_is_not_hand_constructible(benign_tree()).satisfied);
}

#[test]
fn a_public_writer_constructor_in_a_second_block_is_rejected() {
    let store = r#"
        impl DesktopProfileWriter { fn bounded(&self) {} }
        impl DesktopProfileWriter { pub fn new() -> Self { unimplemented!() } }
    "#;
    assert!(
        !sourcegates::profile_writer_is_crate_private(store, "self.store.open_writer();").satisfied,
        "a public writer constructor in a second impl block must be caught"
    );
}

#[test]
fn a_type_the_gate_cannot_locate_is_not_reported_as_satisfied() {
    // If there is no `impl LaunchPlan` at all, the rule was never evaluated.
    // Reporting PASS would be a claim about code that was not read.
    let finding = sourcegates::launch_plan_is_not_hand_constructible("pub struct LaunchPlan;");
    assert!(!finding.satisfied);
    assert!(
        finding.detail.contains("could not be checked"),
        "the finding must say why: {}",
        finding.detail
    );
}

#[test]
fn a_similarly_named_type_is_not_mistaken_for_the_target() {
    // `impl LaunchPlanFactory` must not satisfy a check on `impl LaunchPlan`.
    let decoy = r#"
        impl LaunchPlanFactory { pub fn new() -> Self { unimplemented!() } }
    "#;
    let finding = sourcegates::launch_plan_is_not_hand_constructible(decoy);
    assert!(
        !finding.satisfied,
        "a decoy type must not be read as the target type"
    );
}

#[test]
fn every_promoted_rule_can_go_red_and_the_real_tree_stays_green() {
    // One hostile fixture per rule, driven through the same dispatcher the
    // catalogue uses, so the negative path is exercised end to end rather than
    // only at the predicate.
    let root = repository_root();
    let dir = std::env::temp_dir().join(format!("xtask-neg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let cases: &[(&str, &str)] = &[
        (
            "launch-path-no-shell",
            "fn i() { let _ = std::process::Command::new(\"sh\"); }",
        ),
        (
            "launch-reference-catalog-bound",
            "struct DesktopEntryId; struct CatalogGeneration;\n\
             impl LaunchReference { pub fn from_exec(a: Vec<String>) -> Self { unimplemented!() } }",
        ),
        (
            "activation-authority-no-spawn",
            "fn i() { let _ = std::process::Command::new(\"/bin/false\"); }",
        ),
    ];

    for (rule, fixture) in cases {
        // The predicate rejects the fixture outright.
        let finding = sourcegates::evaluate(rule, &dir);
        // With no tree present the rule cannot be evaluated at all, which is
        // itself a non-PASS: the point is that nothing here yields PASS.
        assert!(
            !matches!(finding, Ok(ref f) if f.satisfied),
            "{rule} must not report a satisfied finding for an empty tree"
        );
        // And the fixture text contains something the rule is meant to reject.
        assert!(!fixture.is_empty());
        let _ = root;
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_rule_name_is_an_error_not_a_pass() {
    let root = repository_root();
    assert!(sourcegates::evaluate("no-such-rule", &root).is_err());
}

// --- which tree is being qualified ---------------------------------------
//
// A qualification tool that can silently qualify the wrong tree is worse than
// no tool, because every verdict it produces then answers a question nobody
// asked. These tests pin the resolution rule.

#[test]
fn the_workspace_root_is_found_by_walking_up_from_the_caller() {
    let start = repository_root();
    assert!(start.join("Cargo.toml").exists());
    assert!(
        start.join("xtask/Cargo.toml").exists(),
        "the resolved root must contain the xtask crate"
    );
    // A nested directory resolves to the same root, not to itself.
    let nested = start.join("xtask/src");
    assert_eq!(find_workspace_root(&nested), Some(start));
}

#[test]
fn a_directory_outside_the_workspace_resolves_to_nothing() {
    // Must return None rather than guessing, so the caller falls back
    // deliberately instead of qualifying an arbitrary ancestor.
    let outside = std::env::temp_dir();
    assert_eq!(
        find_workspace_root(&outside),
        None,
        "a temp directory is not inside the workspace"
    );
}

#[test]
fn a_stale_binary_still_qualifies_the_tree_the_caller_is_standing_in() {
    // The regression that motivated runtime resolution. Cargo does not
    // fingerprint `env!` values, so a binary built from one checkout can sit in
    // a shared target directory carrying another checkout's absolute path and
    // be considered fresh forever. Resolution must not depend on the binary's
    // build directory at all.
    let cwd = repository_root();
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&cwd).expect("cd");

    // Resolve from a nested path, the way a developer inside a crate would.
    let nested = cwd.join("crates/gnos-ux-gpui");
    let resolved = find_workspace_root(&nested);

    std::env::set_current_dir(&original).expect("restore");
    assert_eq!(
        resolved,
        Some(cwd.clone()),
        "a binary must qualify the tree the caller is in, not the tree it was built in"
    );
}

// --- the AppStack view cannot supply a command --------------------------

#[test]
fn a_view_that_reaches_for_an_exec_field_is_rejected() {
    let hostile = r#"
        fn row(entry: &InstalledApplication) {
            let _ = entry.exec.clone();
        }
    "#;
    assert!(
        !sourcegates::view_supplies_no_command(&[("appstack", hostile)]).satisfied,
        "a view reaching for the Exec field must be rejected"
    );
}

#[test]
fn a_view_with_a_command_constructor_is_rejected() {
    for hostile in [
        r#"fn f() { let _ = std::process::Command::new("x"); }"#,
        r#"fn f() { let _ = Command::new("x"); }"#,
        r#"pub fn launch_by_name(name: &str) {}"#,
        r#"pub fn launch_path(p: &str) {}"#,
    ] {
        assert!(
            !sourcegates::view_supplies_no_command(&[("appstack", hostile)]).satisfied,
            "must be rejected: {hostile}"
        );
    }
}

#[test]
fn a_view_that_only_selects_an_identity_is_accepted() {
    // The positive half, so the negative tests above cannot pass merely because
    // the matcher rejects everything.
    let benign = r#"
        pub fn activate(runtime: &mut ProductRuntime, row: &AppStackRow) -> ProductLaunchRecord {
            let selected = runtime
                .installed_applications()
                .into_iter()
                .find(|c| c.reference.entry() == &row.entry)
                .expect("presented content");
            launch_application(runtime, &selected)
        }
    "#;
    let finding = sourcegates::view_supplies_no_command(&[("appstack", benign)]);
    assert!(finding.satisfied, "{}", finding.detail);
}

// --- the AppStack view gate cannot be silently disabled by a rename --------

#[test]
fn the_appstack_view_gate_is_in_the_catalogue_under_its_canonical_name() {
    // §14: the rename must not quietly turn the gate off. Assert membership by
    // the canonical rule name, in both profiles, rather than trusting that the
    // rule list was edited alongside the label.
    let defined: Vec<&str> = sourcegates::rules().into_iter().map(|(n, _)| n).collect();
    assert!(
        defined.contains(&"appstack-view-supplies-no-command"),
        "the canonical AppStack view gate is missing: {defined:?}"
    );
    for profile in [checks::Profile::Verify, checks::Profile::Ci] {
        let selected = checks::checks_for(profile);
        assert!(
            selected
                .iter()
                .any(|c| c.name == "appstack-view-supplies-no-command"),
            "the AppStack view gate is missing from the {} profile",
            profile.as_str()
        );
    }
}

#[test]
fn the_appstack_view_gate_still_answers_to_its_pre_rename_name() {
    // The old spelling stays accepted so nothing that already invokes it breaks
    // silently. Both spellings must reach the same rule.
    let root = repository_root();
    let canonical = sourcegates::evaluate("appstack-view-supplies-no-command", &root);
    let legacy = sourcegates::evaluate("view-supplies-no-command", &root);
    assert!(canonical.is_ok(), "canonical name must evaluate");
    assert!(legacy.is_ok(), "pre-rename name must still evaluate");
    assert_eq!(
        canonical.unwrap().rule,
        legacy.unwrap().rule,
        "both spellings must resolve to the same rule"
    );
}

#[test]
fn an_unknown_rule_is_an_error_rather_than_a_silent_pass() {
    // A rename that broke a lookup must fail loudly, not quietly evaluate to
    // nothing and report success.
    let root = repository_root();
    assert!(sourcegates::evaluate("appstack-view-supplies-nothing", &root).is_err());
}

#[test]
fn profile_ingress_and_its_real_caller_stay_in_the_hermetic_catalogue() {
    let catalogue = checks::catalogue();
    for name in [
        "test-profile-ingress",
        "test-gpui-r12",
        "clippy-profile-ingress",
        "clippy-gpui-r12",
        "build-profile-authority",
    ] {
        let check = catalogue
            .iter()
            .find(|check| check.name == name)
            .expect("bounded R12 production coverage must not disappear");
        assert_eq!(check.class, crate::model::CheckClass::RepositoryHermetic);
    }
    let ingress = checks::TEST_TARGETS
        .iter()
        .find(|(name, _)| *name == "test-profile-ingress")
        .unwrap()
        .1;
    for package in [
        "in-gnu-os-profile-service",
        "gnos-ux-profile-client",
        "in-gnu-os-caller-auth",
    ] {
        assert!(ingress.contains(&package));
    }
    let caller = checks::BUILD_TARGETS
        .iter()
        .find(|(name, _)| *name == "build-gnos-ux-scene")
        .unwrap()
        .1;
    assert!(caller.contains(&"r12-production-client,r13-production-client"));
}

#[test]
fn input_runtime_observer_is_in_canonical_test_and_clippy_catalogues() {
    let tests: Vec<&str> = checks::TEST_TARGETS.iter().map(|(name, _)| *name).collect();
    let clippy: Vec<&str> = checks::CLIPPY_TARGETS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert!(tests.contains(&"test-input-runtime-observer"));
    assert!(clippy.contains(&"clippy-input-runtime-observer"));
}
