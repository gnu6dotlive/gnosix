//! Behavioural tests for the product-side launch action.
//!
//! These are about the boundary between the product and the capability, not
//! about launching: the capability's own resolution decisions are covered in
//! `gnosix-application-launch`. What matters here is that the product cannot
//! express a program, that a stale selection is refused rather than resolved
//! against a catalog the user never saw, and that the recorded outcome never
//! claims a launch that did not happen.

use super::*;

use crate::product_runtime::ProductLaunchState;
use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

/// A temporary application root holding exactly the entries given.
struct Fixture {
    root: PathBuf,
    _guard: Guard,
}

struct Guard(PathBuf);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn next_sequence() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

fn discover_over(root: &Path) -> ApplicationCatalog {
    ApplicationCatalog::discover(
        &ApplicationCatalogRoots::from_application_dirs(vec![root.to_path_buf()]),
        &LocalePreferences::neutral(),
        DesktopEnvironment::from_names(["GNOSIX".to_owned()]),
    )
}

impl Fixture {
    /// Write one entry. The identity comes from the file name, which is what
    /// the catalog uses, so passing an `id` line would only be misleading.
    fn with(id: &str, extra: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "gnosix-launch-product-test-{}-{}",
            std::process::id(),
            next_sequence()
        ));
        fs::create_dir_all(&root).expect("create root");
        let body = format!("[Desktop Entry]\nType=Application\nName=Test App\n{extra}");
        fs::write(root.join(id), body).expect("write entry");
        Self {
            _guard: Guard(root.clone()),
            root,
        }
    }

    fn runtime(&self) -> ProductRuntime {
        let mut runtime = ProductRuntime::default();
        runtime.set_application_catalog(Arc::new(discover_over(&self.root)));
        runtime
    }
}

fn content_for(runtime: &ProductRuntime, id: &str) -> Option<ProductInstalledApplicationContent> {
    let wanted = gnosix_application_catalog::DesktopEntryId::parse(id.to_owned()).ok()?;
    runtime
        .installed_applications()
        .into_iter()
        .find(|candidate| candidate.reference.entry() == &wanted)
}

#[test]
fn installed_applications_bind_every_entry_to_the_current_generation() {
    let fixture = Fixture::with("a.desktop", "Exec=/usr/bin/true\n");
    let runtime = fixture.runtime();
    let content = content_for(&runtime, "a.desktop").expect("entry is present");

    assert_eq!(
        content.reference.generation(),
        runtime.application_catalog().generation(),
        "content must be fenced to the catalog the runtime actually holds"
    );
    assert_eq!(content.name, "Test App");
    assert!(!content.terminal);
}

#[test]
fn a_launchable_entry_reports_a_launched_outcome() {
    let fixture = Fixture::with("a.desktop", "Exec=/usr/bin/true\n");
    let mut runtime = fixture.runtime();
    let content = content_for(&runtime, "a.desktop").expect("entry is present");

    let record = launch_test_application(&mut runtime, &content);
    assert_eq!(record.entry.as_str(), "a.desktop");
    let outcome = record.terminal.expect("a terminal outcome is recorded");
    assert!(outcome.is_launched(), "expected a launch: {outcome:?}");
    assert!(matches!(
        runtime.launch_state(),
        ProductLaunchState::Terminal(_)
    ));
}

#[test]
fn a_terminal_entry_is_refused_and_the_refusal_is_preserved_verbatim() {
    let fixture = Fixture::with("t.desktop", "Exec=/usr/bin/true\nTerminal=true\n");
    let mut runtime = fixture.runtime();
    let content = content_for(&runtime, "t.desktop").expect("entry is present");

    let record = launch_test_application(&mut runtime, &content);
    let outcome = record.terminal.expect("a terminal outcome is recorded");
    assert!(!outcome.is_launched());
    assert_eq!(
        outcome.code(),
        "TERMINAL_REQUIRED",
        "the specific refusal must survive into the product record"
    );
}

#[test]
fn a_selection_made_against_an_older_catalog_is_refused() {
    // The property the freshness fence exists for: content the user could see
    // and click, after the catalog underneath it changed.
    let fixture = Fixture::with("a.desktop", "Exec=/usr/bin/true\n");
    let mut runtime = fixture.runtime();
    let stale = content_for(&runtime, "a.desktop").expect("entry is present");

    // A different root yields a different generation.
    let other = Fixture::with("b.desktop", "Exec=/usr/bin/true\n");
    runtime.set_application_catalog(Arc::new(discover_over(&other.root)));

    let record = launch_test_application(&mut runtime, &stale);
    let outcome = record.terminal.expect("a terminal outcome is recorded");
    assert!(!outcome.is_launched(), "a stale selection must not launch");
    assert_eq!(outcome.code(), "CATALOG_GENERATION_CHANGED");
}

#[test]
fn a_missing_program_is_a_spawn_failure_not_a_launch() {
    let fixture = Fixture::with("a.desktop", "Exec=/gnosix/no/such/program\n");
    let mut runtime = fixture.runtime();
    let content = content_for(&runtime, "a.desktop").expect("entry is present");

    let record = launch_test_application(&mut runtime, &content);
    let outcome = record.terminal.expect("a terminal outcome is recorded");
    assert!(!outcome.is_launched());
    assert_eq!(outcome.code(), "SPAWN_FAILED");
}

#[test]
fn a_try_exec_that_does_not_resolve_refuses_before_anything_starts() {
    let fixture = Fixture::with(
        "a.desktop",
        "Exec=/usr/bin/true\nTryExec=gnosix-absolutely-not-installed\n",
    );
    let mut runtime = fixture.runtime();
    let content = content_for(&runtime, "a.desktop").expect("entry is present");

    let record = launch_test_application(&mut runtime, &content);
    let outcome = record.terminal.expect("a terminal outcome is recorded");
    assert_eq!(outcome.code(), "TRY_EXEC_MISSING");
    assert!(!outcome.is_launched());
}

#[test]
fn a_lab_action_cannot_launch_a_synthetic_application() {
    let fixture = Fixture::with("a.desktop", "Exec=/usr/bin/true\n");
    let mut runtime = fixture.runtime();
    let content = content_for(&runtime, "a.desktop").expect("entry is present");
    let record = launch_lab_application(&mut runtime, &content);
    assert_eq!(record.terminal.as_ref().unwrap().code(), "SPAWN_FAILED");
    assert!(record.detail.contains("LAB_EXECUTION_DISABLED"));
    assert!(!record.terminal.as_ref().unwrap().is_launched());
}

#[test]
fn a_hidden_entry_is_never_offered_as_launchable_content() {
    let fixture = Fixture::with("h.desktop", "Exec=/usr/bin/true\nHidden=true\n");
    let runtime = fixture.runtime();
    assert!(
        content_for(&runtime, "h.desktop").is_none(),
        "a hidden entry must not appear as a launchable row"
    );
}

#[test]
fn an_empty_catalog_offers_nothing_and_launching_is_impossible() {
    let runtime = ProductRuntime::default();
    assert!(runtime.installed_applications().is_empty());
    // With no content there is no reference to launch, which is the point:
    // the product cannot invent a target.
    assert!(content_for(&runtime, "anything.desktop").is_none());
}
