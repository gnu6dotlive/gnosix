//! Behavioural tests for the bounded launch capability.
//!
//! These exercise the decisions, not the plumbing. Spawning is covered by
//! fake executors: this suite never contacts the user manager or starts processes.

use std::{fs, path::PathBuf};

use super::*;
use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences, parse_exec,
};

/// A temporary XDG application root with one desktop entry per call.
/// Discover a catalog over one application root, with an environment that
/// keeps visibility filtering deterministic across the suite.
fn discover_over(root: PathBuf) -> ApplicationCatalog {
    ApplicationCatalog::discover(
        &ApplicationCatalogRoots::from_application_dirs(vec![root]),
        &LocalePreferences::neutral(),
        DesktopEnvironment::from_names(["GNOSIX".to_owned()]),
    )
}

struct CatalogFixture {
    root: PathBuf,
    _guard: TempDir,
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl CatalogFixture {
    /// Write one entry into a fresh application root.
    ///
    /// The catalog is not built here: every test reads it through
    /// `catalog()`, so a stale snapshot can never be what a test asserted
    /// against.
    fn with_entry(id: &str, body: &str) -> (Self, DesktopEntryId) {
        let root = std::env::temp_dir().join(format!(
            "gnosix-launch-test-{}-{}",
            std::process::id(),
            next_sequence()
        ));
        fs::create_dir_all(&root).expect("create application root");
        let id = DesktopEntryId::parse(id.to_owned()).expect("valid desktop entry id");
        fs::write(root.join(id.as_str()), body).expect("write desktop entry");

        (
            Self {
                _guard: TempDir(root.clone()),
                root,
            },
            id,
        )
    }

    fn catalog(&self) -> ApplicationCatalog {
        discover_over(self.root.clone())
    }
}

fn next_sequence() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

/// An ordinary launchable entry, plus any extra desktop-entry lines.
fn fixture(_id: &str, extra: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName=Test App\nExec=/usr/bin/true\n{extra}")
}

fn reference(catalog: &ApplicationCatalog, id: &DesktopEntryId) -> LaunchReference {
    LaunchReference::new(id.clone(), catalog.generation().clone())
}

fn launcher() -> ApplicationLauncher {
    ApplicationLauncher::new(LaunchConfiguration::new())
}

// --- the freshness fence --------------------------------------------------

#[test]
fn a_reference_from_an_older_catalog_generation_is_refused() {
    let (fixture, id) = CatalogFixture::with_entry("stale.desktop", &fixture("stale.desktop", ""));
    let catalog = fixture.catalog();
    let stale = reference(&catalog, &id);

    // A second discovery over a changed root yields a different generation.
    let other_root = std::env::temp_dir().join(format!(
        "gnosix-launch-test-{}-{}",
        std::process::id(),
        next_sequence()
    ));
    fs::create_dir_all(&other_root).expect("create dir");
    let other = discover_over(other_root.clone());
    let _ = fs::remove_dir_all(&other_root);

    assert_ne!(
        catalog.generation().hex(),
        other.generation().hex(),
        "precondition: a different catalog has a different generation"
    );

    let refusal = launcher()
        .plan(&stale, &other, &LaunchDocuments::None)
        .expect_err("a stale reference must not resolve");
    assert!(matches!(
        refusal,
        LaunchRefusal::CatalogGenerationChanged { .. }
    ));
    assert_eq!(refusal.code(), "CATALOG_GENERATION_CHANGED");
}

#[test]
fn an_unknown_entry_is_refused_rather_than_launched() {
    let (fixture, _) =
        CatalogFixture::with_entry("present.desktop", &fixture("present.desktop", ""));
    let catalog = fixture.catalog();
    let missing = DesktopEntryId::parse("absent.desktop").expect("valid id");
    let request = LaunchReference::new(missing, catalog.generation().clone());

    let refusal = launcher()
        .plan(&request, &catalog, &LaunchDocuments::None)
        .expect_err("an unknown entry must not resolve");
    assert_eq!(refusal, LaunchRefusal::UnknownEntry);
}

// --- no shell, ever -------------------------------------------------------

#[test]
fn a_shell_metacharacter_in_exec_is_literal_text_and_never_a_command_boundary() {
    // The dangerous part: `sh -c` must not appear anywhere in the plan, and
    // `;` must not split the entry into a second command. An `Exec` line is a
    // token list, so `;` is ordinary text inside the argument that contains it.
    let (fixture, id) = CatalogFixture::with_entry(
        "shellish.desktop",
        "[Desktop Entry]\nType=Application\nName=Shellish\nExec=/usr/bin/echo %f; rm -rf /\n",
    );
    let catalog = fixture.catalog();
    let request = reference(&catalog, &id);
    let documents = LaunchDocuments::Files(vec![PathBuf::from("/tmp/one")]);

    let plan = launcher()
        .plan(&request, &catalog, &documents)
        .expect("resolves");
    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert_eq!(argv[0], "/usr/bin/echo");
    assert!(
        !argv
            .iter()
            .any(|arg| arg == "sh" || arg == "-c" || arg == "bash"),
        "no shell may be introduced: {argv:?}"
    );
    assert_eq!(
        argv[1], "/tmp/one;",
        "the semicolon stays inside its argument instead of ending a command: {argv:?}"
    );
    assert!(
        argv.iter().skip(1).any(|arg| arg == "rm"),
        "the trailing words are arguments to that one program, not a second program: \
         without a shell they are inert text. argv={argv:?}"
    );
}

#[test]
fn exec_quoting_is_honoured_without_shell_interpretation() {
    let (fixture, id) = CatalogFixture::with_entry(
        "quoted.desktop",
        "[Desktop Entry]\nType=Application\nName=Quoted\nExec=\"/usr/bin/my app\" --flag \"two words\"\n",
    );
    let catalog = fixture.catalog();
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect("resolves");
    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert_eq!(argv, vec!["/usr/bin/my app", "--flag", "two words"]);
}

// --- no invented arguments ------------------------------------------------

#[test]
fn a_required_file_field_code_is_dropped_rather_than_filled_with_an_invention() {
    let (fixture, id) = CatalogFixture::with_entry(
        "needsfile.desktop",
        "[Desktop Entry]\nType=Application\nName=Needs File\nExec=viewer %F\n",
    );
    let catalog = fixture.catalog();
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect("resolves");

    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["viewer"], "no argument may be invented");
    assert_eq!(
        plan.substituted_field_codes()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec!['F'],
        "the dropped field code must be visible in the plan"
    );
}

#[test]
fn multiple_files_expand_into_separate_arguments() {
    let (fixture, id) = CatalogFixture::with_entry(
        "multifile.desktop",
        "[Desktop Entry]\nType=Application\nName=Multi\nExec=viewer %F\n",
    );
    let catalog = fixture.catalog();
    let documents = LaunchDocuments::Files(vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &documents)
        .expect("resolves");

    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["viewer", "/tmp/a", "/tmp/b"]);
    assert!(plan.substituted_field_codes().is_empty());
}

#[test]
fn a_single_file_field_code_inlines_into_its_token() {
    let (fixture, id) = CatalogFixture::with_entry(
        "onefile.desktop",
        "[Desktop Entry]\nType=Application\nName=One\nExec=viewer --file=%f\n",
    );
    let catalog = fixture.catalog();
    let documents = LaunchDocuments::Files(vec![PathBuf::from("/tmp/only")]);
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &documents)
        .expect("resolves");

    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["viewer", "--file=/tmp/only"]);
}

#[test]
fn urls_expand_and_are_reported_separately_from_files() {
    let (fixture, id) = CatalogFixture::with_entry(
        "urls.desktop",
        "[Desktop Entry]\nType=Application\nName=URLs\nExec=browse %U\n",
    );
    let catalog = fixture.catalog();
    let documents = LaunchDocuments::Urls(vec!["https://example.invalid".to_owned()]);
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &documents)
        .expect("resolves");
    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["browse", "https://example.invalid"]);
}

#[test]
fn url_field_codes_are_dropped_when_only_files_were_supplied() {
    let (fixture, id) = CatalogFixture::with_entry(
        "urlsonly.desktop",
        "[Desktop Entry]\nType=Application\nName=URLs Only\nExec=browse %U\n",
    );
    let catalog = fixture.catalog();
    let documents = LaunchDocuments::Files(vec![PathBuf::from("/tmp/a")]);
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &documents)
        .expect("resolves");
    assert_eq!(plan.argv().len(), 1);
    assert!(plan.substituted_field_codes().contains(&'U'));
}

#[test]
fn a_percent_literal_is_preserved() {
    let (fixture, id) = CatalogFixture::with_entry(
        "percent.desktop",
        "[Desktop Entry]\nType=Application\nName=Percent\nExec=/usr/bin/echo 100%%\n",
    );
    let catalog = fixture.catalog();
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect("resolves");
    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["/usr/bin/echo", "100%"]);
}

#[test]
fn the_icon_and_name_field_codes_expand() {
    let (fixture, id) = CatalogFixture::with_entry(
        "meta.desktop",
        "[Desktop Entry]\nType=Application\nName=Meta App\nIcon=meta-icon\nExec=/usr/bin/echo %i %c\n",
    );
    let catalog = fixture.catalog();
    let plan = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect("resolves");
    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["/usr/bin/echo", "meta-icon", "Meta App"]);
}

// --- TryExec gating -------------------------------------------------------

#[test]
fn a_missing_try_exec_refuses_an_otherwise_valid_entry() {
    let (fixture, id) = CatalogFixture::with_entry(
        "tryexec.desktop",
        &fixture(
            "tryexec.desktop",
            "TryExec=gnosix-definitely-not-installed-xyz\n",
        ),
    );
    let catalog = fixture.catalog();
    let refusal = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect_err("TryExec must gate the entry");
    assert!(matches!(refusal, LaunchRefusal::TryExecMissing { .. }));
    assert_eq!(refusal.code(), "TRY_EXEC_MISSING");
}

#[test]
fn a_present_try_exec_allows_the_launch() {
    let (fixture, id) = CatalogFixture::with_entry(
        "tryexecok.desktop",
        &fixture("tryexecok.desktop", "TryExec=/usr/bin/true\n"),
    );
    let catalog = fixture.catalog();
    assert!(
        launcher()
            .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
            .is_ok()
    );
}

// --- terminal and D-Bus refusals -----------------------------------------

#[test]
fn a_terminal_entry_is_refused_when_no_terminal_is_configured() {
    let (fixture, id) =
        CatalogFixture::with_entry("term.desktop", &fixture("term.desktop", "Terminal=true\n"));
    let catalog = fixture.catalog();
    let refusal = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect_err("no terminal may be guessed");
    assert_eq!(refusal, LaunchRefusal::TerminalRequired);
}

#[test]
fn a_terminal_entry_wraps_the_argv_in_the_configured_terminal() {
    let (fixture, id) = CatalogFixture::with_entry(
        "termok.desktop",
        &fixture("termok.desktop", "Terminal=true\n"),
    );
    let catalog = fixture.catalog();
    let configured = ApplicationLauncher::new(
        LaunchConfiguration::new()
            .with_terminal_command(["/usr/bin/gnosix-terminal".to_owned().into()]),
    );
    let plan = configured
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect("wraps");
    let argv: Vec<String> = plan
        .argv()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["/usr/bin/gnosix-terminal", "/usr/bin/true"]);
    assert!(plan.via_terminal());
}

#[test]
fn a_dbus_activatable_entry_without_exec_refuses_rather_than_falling_back() {
    let (fixture, id) = CatalogFixture::with_entry(
        "dbusonly.desktop",
        "[Desktop Entry]\nType=Application\nName=DBus Only\nDBusActivatable=true\n",
    );
    let catalog = fixture.catalog();
    let refusal = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect_err("no D-Bus activation may be simulated");
    assert_eq!(refusal, LaunchRefusal::RequiresDBusActivation);
}

#[test]
fn an_entry_with_neither_exec_nor_dbus_activation_is_filtered_by_the_catalog() {
    // The refusal this crate defines for the case exists defensively, but the
    // catalog never admits such an entry in the first place: it reports
    // MissingExec and excludes it. Testing the layer that actually decides it
    // is the honest place for this property.
    let (fixture, _) = CatalogFixture::with_entry(
        "nothing.desktop",
        "[Desktop Entry]\nType=Application\nName=Nothing\n",
    );
    let catalog = fixture.catalog();
    let id = DesktopEntryId::parse("nothing.desktop").expect("valid id");

    assert!(
        catalog.get(&id).is_none(),
        "an entry with no launch method must not reach the product catalog"
    );
    assert!(
        catalog
            .issues()
            .iter()
            .any(|issue| issue.kind == gnosix_application_catalog::CatalogIssueKind::MissingExec),
        "and the reason must be recorded as a catalog issue"
    );
}

#[test]
fn a_built_argv_with_no_program_is_refused_rather_than_launched() {
    // A catalog that somehow admitted an Exec template with nothing usable in
    // it must still not produce a plan. This is the defensive path the catalog
    // filter normally prevents.
    let template = parse_exec("/usr/bin/true").expect("template");
    let entry = InstalledApplication {
        id: DesktopEntryId::parse("defensive.desktop").expect("id"),
        source_path: PathBuf::from("/defensive.desktop"),
        source_precedence: 0,
        name: "Defensive".to_owned(),
        generic_name: None,
        comment: None,
        icon: None,
        dbus_activatable: false,
        exec: Some(template),
        try_exec: None,
        startup_wm_class: None,
        terminal: false,
        no_display: false,
        hidden: false,
        only_show_in: vec![],
        not_show_in: vec![],
        mime_types: vec![],
        actions: vec![],
    };
    let substitution = FieldSubstitution::for_documents(&LaunchDocuments::None);
    let (program, args, _dropped) =
        build_argv(entry.exec.as_ref().expect("exec"), &substitution, &entry).expect("builds");
    assert_eq!(program, OsString::from("/usr/bin/true"));
    assert!(args.is_empty());
}

#[test]
fn a_hidden_entry_is_not_presentable_for_a_product_surface() {
    let (fixture, id) = CatalogFixture::with_entry(
        "hidden.desktop",
        &fixture("hidden.desktop", "Hidden=true\n"),
    );
    let catalog = fixture.catalog();
    let refusal = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect_err("a hidden entry is not launchable from a product surface");
    assert_eq!(refusal, LaunchRefusal::NotPresentable);
}

#[test]
fn a_no_display_entry_is_not_presentable() {
    let (fixture, id) = CatalogFixture::with_entry(
        "nodisplay.desktop",
        &fixture("nodisplay.desktop", "NoDisplay=true\n"),
    );
    let catalog = fixture.catalog();
    let refusal = launcher()
        .plan(&reference(&catalog, &id), &catalog, &LaunchDocuments::None)
        .expect_err("NoDisplay entries are not product-launchable");
    assert_eq!(refusal, LaunchRefusal::NotPresentable);
}

// --- receipts -------------------------------------------------------------

#[test]
fn a_launchable_entry_produces_a_launched_receipt() {
    let (fixture, id) =
        CatalogFixture::with_entry("launchable.desktop", &fixture("launchable.desktop", ""));
    let catalog = fixture.catalog();
    let receipt = launcher().launch_with_executor(
        &reference(&catalog, &id),
        &catalog,
        &LaunchDocuments::None,
        &FakeExecutor,
    );
    assert!(
        matches!(receipt.outcome, LaunchOutcome::Launched { .. }),
        "a resolvable entry with a real program reports Launched: {:?}",
        receipt.outcome
    );
    assert!(receipt.summary().contains("outcome=LAUNCHED"));
}

#[test]
fn a_refused_launch_produces_a_receipt_that_does_not_claim_a_launch() {
    let (fixture, id) = CatalogFixture::with_entry(
        "refused.desktop",
        &fixture("refused.desktop", "Terminal=true\n"),
    );
    let catalog = fixture.catalog();
    let receipt = launcher().launch_with_executor(
        &reference(&catalog, &id),
        &catalog,
        &LaunchDocuments::None,
        &FakeExecutor,
    );
    assert!(matches!(
        receipt.outcome,
        LaunchOutcome::Refused(LaunchRefusal::TerminalRequired)
    ));
    assert!(!receipt.outcome.is_launched());
    assert_eq!(receipt.outcome.code(), "REFUSED");
    assert!(receipt.summary().contains("outcome=REFUSED"));
}

#[test]
fn a_spawn_failure_is_reported_as_a_failure_not_a_launch() {
    let (fixture, id) = CatalogFixture::with_entry(
        "badprog.desktop",
        "[Desktop Entry]\nType=Application\nName=Bad\nExec=/gnosix/no/such/program\n",
    );
    let catalog = fixture.catalog();
    let receipt = launcher().launch_with_executor(
        &reference(&catalog, &id),
        &catalog,
        &LaunchDocuments::None,
        &FakeExecutor,
    );
    assert!(
        matches!(receipt.outcome, LaunchOutcome::SpawnFailed { .. }),
        "a program that cannot start must not be reported as launched: {:?}",
        receipt.outcome
    );
    assert!(!receipt.outcome.is_launched());
    assert_eq!(receipt.outcome.code(), "SPAWN_FAILED");
}

// --- determinism ----------------------------------------------------------

#[test]
fn resolution_is_deterministic_for_an_unchanged_catalog() {
    let (fixture, id) =
        CatalogFixture::with_entry("stable.desktop", &fixture("stable.desktop", ""));
    let catalog = fixture.catalog();
    let request = reference(&catalog, &id);
    let launcher = launcher();

    let first = launcher
        .plan(&request, &catalog, &LaunchDocuments::None)
        .expect("resolves");
    let second = launcher
        .plan(&request, &catalog, &LaunchDocuments::None)
        .expect("resolves");
    assert_eq!(first, second, "the same request must resolve identically");
}

#[test]
fn the_parser_still_rejects_an_unsupported_field_code_at_catalog_time() {
    // The catalog refuses the entry outright, so it never reaches this crate
    // as a plan. This pins the boundary between the two layers.
    assert!(parse_exec("/usr/bin/app %z").is_err());
}

#[test]
fn the_reference_exposes_the_generation_it_is_bound_to() {
    let (fixture, id) = CatalogFixture::with_entry("gen.desktop", &fixture("gen.desktop", ""));
    let catalog = fixture.catalog();
    let request = reference(&catalog, &id);
    assert_eq!(request.generation(), catalog.generation());
    assert_eq!(request.entry(), &id);
}

struct FakeExecutor;
impl LaunchExecutor for FakeExecutor {
    fn execute(&self, plan: &LaunchPlan) -> Result<LaunchExecutionReceipt, LaunchExecutionError> {
        if plan.program() != std::ffi::OsStr::new("/usr/bin/true") {
            return Err(LaunchExecutionError::ExecutableStartFailed(
                "test executable absent".into(),
            ));
        }
        Ok(LaunchExecutionReceipt {
            pid: 42,
            owner: LaunchExecutionOwner::ControlledHarness,
        })
    }
}

struct RecordingExecutor {
    plans: std::cell::RefCell<Vec<LaunchPlan>>,
    result: Result<LaunchExecutionReceipt, LaunchExecutionError>,
}
impl LaunchExecutor for RecordingExecutor {
    fn execute(&self, plan: &LaunchPlan) -> Result<LaunchExecutionReceipt, LaunchExecutionError> {
        self.plans.borrow_mut().push(plan.clone());
        self.result.clone()
    }
}

#[test]
fn validated_plan_is_forwarded_once_with_exact_program_arguments_and_owner() {
    let (fixture, id) = CatalogFixture::with_entry(
        "exact.desktop",
        "[Desktop Entry]\nType=Application\nName=Exact\nExec=/usr/bin/printf \"literal space\" $HOME semi;colon\n",
    );
    let catalog = fixture.catalog();
    let request = reference(&catalog, &id);
    let launcher = launcher();
    let expected = launcher
        .plan(&request, &catalog, &LaunchDocuments::None)
        .unwrap();
    let executor = RecordingExecutor {
        plans: Default::default(),
        result: Ok(LaunchExecutionReceipt {
            pid: 321,
            owner: LaunchExecutionOwner::SystemdUserTransientService {
                unit: "gnosix-app-1-0.service".into(),
            },
        }),
    };
    let receipt =
        launcher.launch_with_executor(&request, &catalog, &LaunchDocuments::None, &executor);
    assert_eq!(*executor.plans.borrow(), vec![expected.clone()]);
    assert_eq!(expected.program(), std::ffi::OsStr::new("/usr/bin/printf"));
    assert_eq!(
        expected.args(),
        &["literal space", "$HOME", "semi;colon"].map(OsString::from)
    );
    assert!(matches!(
        receipt.outcome,
        LaunchOutcome::Launched { pid: 321, .. }
    ));
    assert!(
        receipt
            .summary()
            .contains("pid=321 owner=systemd-user-transient:gnosix-app-1-0.service")
    );
}

#[test]
fn policy_and_stale_generation_never_invoke_executor() {
    let (fixture, id) = CatalogFixture::with_entry(
        "refused.desktop",
        &fixture("refused.desktop", "Terminal=true\n"),
    );
    let catalog = fixture.catalog();
    let executor = RecordingExecutor {
        plans: Default::default(),
        result: Err(LaunchExecutionError::UserManagerUnavailable(
            "must not be reached".into(),
        )),
    };
    let launcher = launcher();
    let request = reference(&catalog, &id);
    let receipt =
        launcher.launch_with_executor(&request, &catalog, &LaunchDocuments::None, &executor);
    assert!(matches!(
        receipt.outcome,
        LaunchOutcome::Refused(LaunchRefusal::TerminalRequired)
    ));
    let stale = launcher.launch_with_executor(
        &request,
        &ApplicationCatalog::empty(),
        &LaunchDocuments::None,
        &executor,
    );
    assert!(matches!(
        stale.outcome,
        LaunchOutcome::Refused(LaunchRefusal::CatalogGenerationChanged { .. })
    ));
    assert!(executor.plans.borrow().is_empty());
}

#[test]
fn typed_execution_failure_is_not_a_policy_refusal_or_success() {
    let (fixture, id) =
        CatalogFixture::with_entry("failed.desktop", &fixture("failed.desktop", ""));
    let catalog = fixture.catalog();
    for failure in [
        LaunchExecutionError::UserManagerUnavailable("offline".into()),
        LaunchExecutionError::TransientUnitRejected("rejected".into()),
        LaunchExecutionError::ExecutableStartFailed("exec".into()),
        LaunchExecutionError::MainPidUnavailable("missing".into()),
    ] {
        let executor = RecordingExecutor {
            plans: Default::default(),
            result: Err(failure.clone()),
        };
        let receipt = launcher().launch_with_executor(
            &reference(&catalog, &id),
            &catalog,
            &LaunchDocuments::None,
            &executor,
        );
        assert!(
            matches!(receipt.outcome, LaunchOutcome::SpawnFailed { error, .. } if error == failure)
        );
        assert_eq!(executor.plans.borrow().len(), 1);
    }
}
