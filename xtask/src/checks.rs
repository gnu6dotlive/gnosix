//! The check catalogue.
//!
//! One definition per check, each with a class. `verify` and `ci` both
//! execute *these* definitions; `ci` simply filters to the repository-hermetic
//! ones. There is no second list, and there is deliberately no way to express
//! a CI-specific command, because a fork of the local contract is how local
//! truth and CI truth drift apart.
//!
//! Test counts are recorded as provenance, not asserted. A gate is command
//! success. Encoding `129` as an invariant would mean that a legitimate new
//! test could fail qualification, which is precisely the sort of accidental
//! coupling that makes people delete good tests.

use crate::model::CheckClass;

// Packages whose build needs the host's native libraries. gnosix-hyprland
// reaches libwayland through gnos-ux-surface's smithay-client-toolkit (pkg-config
// `wayland-client`), which the CI runner image does not have.
const HOST_NATIVE_PACKAGES: &[&str] = &["gnos-ux-gpui", "gnos-audio", "gnosix-hyprland"];

fn cargo_check_class(extra: &[&str]) -> CheckClass {
    let mut index = 0;
    while index + 1 < extra.len() {
        if extra[index] == "-p" && HOST_NATIVE_PACKAGES.contains(&extra[index + 1]) {
            return CheckClass::HostNative;
        }
        index += 1;
    }
    CheckClass::RepositoryHermetic
}

/// The clippy targets, preserved as a set rather than as a count.
///
/// Every target uses `-D warnings`. Separate checks retain diagnostics from
/// each independently bounded implementation and production feature set.
pub const CLIPPY_TARGETS: &[(&str, &[&str])] = &[
    (
        "clippy-profile-ingress",
        &[
            "-p",
            "in-gnu-os-profile-service",
            "-p",
            "gnos-ux-profile-client",
            "-p",
            "in-gnu-os-caller-auth",
        ],
    ),
    (
        "clippy-gpui-r12",
        &[
            "-p",
            "gnos-ux-gpui",
            "--features",
            "r12-production-client,r13-production-client",
        ],
    ),
    ("clippy-gpui-passive", &["-p", "gnos-ux-gpui"]),
    (
        "clippy-gpui-r13",
        &["-p", "gnos-ux-gpui", "--features", "r13-production-client"],
    ),
    (
        "clippy-gpui-component-lab",
        &["-p", "gnos-ux-gpui", "--features", "component-lab"],
    ),
    (
        "clippy-indicator-client",
        &["-p", "gnos-ux-indicator-client"],
    ),
    (
        "clippy-gpui-indicator",
        &[
            "-p",
            "gnos-ux-gpui",
            "--features",
            "indicator-production-client",
        ],
    ),
    (
        "clippy-application-launch",
        &["-p", "gnosix-application-launch"],
    ),
    (
        "clippy-application-catalog",
        &["-p", "gnosix-application-catalog"],
    ),
    ("clippy-input-core", &["-p", "gnosix-input-core"]),
    (
        "clippy-input-observation",
        &["-p", "gnosix-input-observation"],
    ),
    (
        "clippy-input-runtime-observer",
        &["-p", "gnosix-input-runtime-observer"],
    ),
    (
        "clippy-desktop-profiles",
        &["-p", "in-gnu-os-desktop-profiles"],
    ),
    // The qualification tool itself, at the same strictness. "It is only
    // tooling" is precisely the reasoning that lets a harness rot, and this
    // crate is now the thing every other gate is read through.
    // WS-04: the window identity the thumbnails rely on (host-native: it
    // needs libwayland) and the capture worker (pure Rust Wayland).
    ("clippy-hyprland", &["-p", "gnosix-hyprland"]),
    ("clippy-thumbnails", &["-p", "gnos-thumbnails"]),
    ("clippy-xtask", &["-p", "xtask"]),
];

/// The test suites, preserved as a set.
pub const TEST_TARGETS: &[(&str, &[&str])] = &[
    (
        "test-profile-ingress",
        &[
            "-p",
            "in-gnu-os-profile-service",
            "-p",
            "gnos-ux-profile-client",
            "-p",
            "in-gnu-os-caller-auth",
        ],
    ),
    (
        "test-gpui-r12",
        &[
            "-p",
            "gnos-ux-gpui",
            "--lib",
            "--features",
            "r12-production-client,r13-production-client",
        ],
    ),
    ("test-gpui-passive", &["-p", "gnos-ux-gpui", "--lib"]),
    (
        "test-gpui-r13",
        &[
            "-p",
            "gnos-ux-gpui",
            "--lib",
            "--features",
            "r13-production-client",
        ],
    ),
    (
        "test-gpui-component-lab",
        &[
            "-p",
            "gnos-ux-gpui",
            "--features",
            "component-lab",
            "--bin",
            "gnos-ux-lab",
        ],
    ),
    ("test-indicator-client", &["-p", "gnos-ux-indicator-client"]),
    (
        "test-gpui-indicator",
        &[
            "-p",
            "gnos-ux-gpui",
            "--lib",
            "--features",
            "indicator-production-client",
        ],
    ),
    (
        "test-application-launch",
        &["-p", "gnosix-application-launch", "--lib"],
    ),
    (
        "test-application-catalog",
        &["-p", "gnosix-application-catalog", "--lib"],
    ),
    ("test-input-core", &["-p", "gnosix-input-core", "--lib"]),
    (
        "test-input-observation",
        &["-p", "gnosix-input-observation"],
    ),
    (
        "test-input-runtime-observer",
        &["-p", "gnosix-input-runtime-observer"],
    ),
    (
        "test-desktop-profiles",
        &["-p", "in-gnu-os-desktop-profiles", "--lib"],
    ),
    ("test-hyprland", &["-p", "gnosix-hyprland", "--lib"]),
    ("test-thumbnails", &["-p", "gnos-thumbnails"]),
    ("test-text", &["-p", "gnos-text"]),
    ("test-xtask", &["-p", "xtask", "--bin", "xtask"]),
];

/// The production binaries, built explicitly.
///
/// Not inferred from the tests: a test target compiles the library, not
/// necessarily the binary with the exact feature set that ships.
pub const BUILD_TARGETS: &[(&str, &[&str])] = &[
    (
        "build-profile-authority",
        &[
            "-p",
            "in-gnu-os-profile-service",
            "--bin",
            "in-gnu-os-profiled",
        ],
    ),
    (
        "build-gnos-ux-scene",
        &[
            "-p",
            "gnos-ux-gpui",
            "--features",
            "r12-production-client,r13-production-client,indicator-production-client",
            "--bin",
            "gnos-ux-scene",
        ],
    ),
    (
        "build-gnos-ux-lab",
        &[
            "-p",
            "gnos-ux-gpui",
            "--features",
            "component-lab",
            "--bin",
            "gnos-ux-lab",
        ],
    ),
];

/// A check that is evaluated in-process rather than spawned.
///
/// Some invariants have no command to run: they are structural facts about the
/// source or the host. Giving them a synthesised `argv` and an invented exit
/// status would be the same class of error as deriving a verdict from log
/// text, so they are modelled as their own kind instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InProcess {
    /// One named enduring source invariant.
    SourceRule(&'static str),
    /// The installed R13 activation authority digest.
    InstalledAuthority,
}

impl InProcess {
    /// A stable, non-executable description for the report.
    pub fn describe(&self) -> String {
        match self {
            Self::SourceRule(rule) => format!("<in-process:source-rule:{rule}>"),
            Self::InstalledAuthority => "<in-process:installed-authority>".to_owned(),
        }
    }
}

/// One defined check.
#[derive(Clone, Debug)]
pub struct Check {
    pub name: &'static str,
    pub class: CheckClass,
    /// The command to spawn, for a spawned check.
    pub argv: Vec<String>,
    /// The in-process evaluation, for a check that has no command.
    pub in_process: Option<InProcess>,
}

impl Check {
    fn spawn(name: &'static str, class: CheckClass, argv: Vec<String>) -> Self {
        Self {
            name,
            class,
            argv,
            in_process: None,
        }
    }

    fn internal(name: &'static str, class: CheckClass, in_process: InProcess) -> Self {
        Self {
            name,
            class,
            argv: vec![in_process.describe()],
            in_process: Some(in_process),
        }
    }
}

/// The full catalogue, in the order they should run.
///
/// Tests before clippy: a failing test is a faster and more specific signal
/// than a lint, and running the cheap, most-likely-to-inform checks first
/// makes a red run more useful.
pub fn catalogue() -> Vec<Check> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut checks = Vec::new();

    for (name, extra) in TEST_TARGETS {
        let mut argv = vec![
            cargo.clone(),
            "test".into(),
            "--locked".into(),
            "--all-targets".into(),
        ];
        argv.extend(extra.iter().map(|s| (*s).to_owned()));
        checks.push(Check::spawn(name, cargo_check_class(extra), argv));
    }

    for (name, extra) in CLIPPY_TARGETS {
        let mut argv = vec![
            cargo.clone(),
            "clippy".into(),
            "--locked".into(),
            "--all-targets".into(),
        ];
        argv.extend(extra.iter().map(|s| (*s).to_owned()));
        argv.push("--".to_owned());
        argv.push("-D".to_owned());
        argv.push("warnings".to_owned());
        checks.push(Check::spawn(name, cargo_check_class(extra), argv));
    }

    checks.push(Check::spawn(
        "fmt",
        CheckClass::RepositoryHermetic,
        vec![cargo, "fmt".into(), "--check".into()],
    ));

    // The engineering corpus and its catalogue live in the private tree
    // only; the public projection publishes no catalogue to index.
    if has_engineering_catalogue() {
        checks.push(Check::spawn(
            "docs-index",
            CheckClass::RepositoryHermetic,
            vec![
                "python3".into(),
                "tools/docs_index.py".into(),
                "--check".into(),
            ],
        ));
    }

    // I18N-01: every text the shell shows has its French, and the catalogue
    // keeps nothing the code no longer says.
    checks.push(Check::spawn(
        "i18n-coverage",
        CheckClass::RepositoryHermetic,
        vec!["python3".into(), "tools/i18n.py".into(), "--check".into()],
    ));

    checks.push(Check::spawn(
        "diff-check",
        CheckClass::RepositoryHermetic,
        vec!["git".into(), "diff".into(), "--check".into()],
    ));

    checks.push(Check::spawn(
        "test-r13-staging",
        CheckClass::RepositoryHermetic,
        vec![
            "python3".into(),
            "tools/test_stage_r13_production_caller.py".into(),
        ],
    ));

    // The lockfile is part of the qualified source state. `--locked`
    // throughout already enforces this for cargo commands; this check makes
    // the fact explicit and covers the non-cargo paths.
    checks.push(Check::spawn(
        "lockfile-present",
        CheckClass::RepositoryHermetic,
        vec![
            "git".into(),
            "ls-files".into(),
            "--error-unmatch".into(),
            "Cargo.lock".into(),
        ],
    ));

    for (name, extra) in BUILD_TARGETS {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
        let mut argv = vec![cargo, "build".into(), "--locked".into()];
        argv.extend(extra.iter().map(|s| (*s).to_owned()));
        checks.push(Check::spawn(name, cargo_check_class(extra), argv));
    }

    // Host/live qualification, excluded from `ci` by class rather than by
    // omission. This is the check §24 asks to relocate: it is a fact about
    // *this* workstation's installed authority, and a GitHub runner would
    // pass it or fail it for reasons that have nothing to do with the code.
    checks.push(Check::internal(
        "host-authority-digest",
        CheckClass::HostLive,
        InProcess::InstalledAuthority,
    ));

    checks.extend(source_invariant_checks());

    checks
}

/// The enduring source invariants.
///
/// Evaluated in-process rather than as a spawned `cargo run`, because making a
/// verification run depend on a recursive cargo invocation is slow and
/// fragile for no benefit. The *count* of the original Phase 4 gate set is
/// deliberately not encoded: "Phase 4 had 26 gates" is a historical fact
/// about one tranche, not an architectural invariant a future change could
/// break.
/// The enduring source invariants, one check per rule.
///
/// One check per rule rather than a single combined gate: if the launch path
/// grew a shell, that finding must not hide a simultaneously-broken activation
/// boundary. This is the same reason the six clippy targets are separate checks.
pub fn source_invariant_checks() -> Vec<Check> {
    crate::sourcegates::rules()
        .into_iter()
        .map(|(rule, _)| {
            Check::internal(
                rule,
                CheckClass::RepositoryHermetic,
                InProcess::SourceRule(rule),
            )
        })
        .collect()
}

/// The checks a given profile runs.
///
/// Profiles are projections of one catalogue, never separately maintained
/// command lists.
pub fn checks_for(profile: Profile) -> Vec<Check> {
    let all = catalogue();
    match profile {
        Profile::Ci => all.into_iter().filter(|c| c.class.runs_in_ci()).collect(),
        Profile::Native => all
            .into_iter()
            .filter(|c| c.class.runs_in_native())
            .collect(),
        Profile::Verify => all,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Everything, including host/live checks. Local workstation use.
    Verify,
    /// Repository-hermetic plus host-native source/build qualification.
    Native,
    /// Repository-hermetic checks only. Safe for generic remote CI.
    Ci,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verify => "verify",
            Self::Native => "native",
            Self::Ci => "ci",
        }
    }
}

/// Whether this tree carries the engineering corpus's catalogue (the
/// private tree does; a public projection does not).
pub fn has_engineering_catalogue() -> bool {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/catalog.json")
        .is_file()
}
