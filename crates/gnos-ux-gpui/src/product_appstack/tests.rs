//! The AppStack's model, tested directly.
//!
//! The AppStack is the first production surface built on the launch capability,
//! so the properties that matter here are the ones a user would notice being
//! wrong: an application that moves when the list re-sorts, two identically
//! named applications where only one responds, a stale selection that launches
//! something the user did not pick, or a refusal presented as a success.
//!
//! The fixtures are constructed in memory rather than discovered from disk.
//! That keeps these tests hermetic, and it is what lets a case assert on an
//! exact ordering instead of whatever the test machine happens to have
//! installed.

use std::{collections::BTreeSet, sync::Arc};

use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
};

use super::{
    AppStackPreferenceProjection, AppStackQuery, AppStackRow, AppStackSearchMode, appstack_rows,
    appstack_rows_for, launch_feedback,
};
use crate::product_focus::{installed_application_control_identity, panel_control_identity};
use crate::product_runtime::{ProductInstalledApplicationContent, ProductRuntime};

/// A catalog built from desktop-entry text, in memory.
///
/// Each entry becomes a real `.desktop` in a temporary directory so the catalog
/// is produced by the same discovery path production uses, rather than by a
/// hand-built struct that could drift from it.
struct CatalogFixture {
    root: std::path::PathBuf,
}

impl CatalogFixture {
    fn new(id: &str, entries: &[(&str, &str)]) -> Self {
        let root =
            std::env::temp_dir().join(format!("gnosix-appstack-{id}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create fixture root");
        for (name, body) in entries {
            let path = root.join(name);
            std::fs::write(&path, body).expect("write entry");
        }
        Self { root }
    }

    fn catalog(&self) -> ApplicationCatalog {
        ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![self.root.clone()]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::from_names(["GNOSIX".to_owned()]),
        )
    }

    fn runtime(&self) -> ProductRuntime {
        let mut runtime = ProductRuntime::default();
        runtime.set_application_catalog(Arc::new(self.catalog()));
        runtime
    }
}

impl Drop for CatalogFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn entry(name: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec=/usr/bin/true\n")
}

fn terminal_entry(name: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec=/usr/bin/true\nTerminal=true\n")
}

struct RichEntrySpec<'a> {
    name: &'a str,
    generic: &'a str,
    comment: &'a str,
    command: &'a str,
    startup_class: &'a str,
    categories: &'a str,
    keywords: &'a str,
    terminal: bool,
}

fn rich_entry(spec: RichEntrySpec<'_>) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nGenericName={}\nComment={}\nExec={}\nStartupWMClass={}\nCategories={}\nKeywords={}\nTerminal={}\n",
        spec.name,
        spec.generic,
        spec.comment,
        spec.command,
        spec.startup_class,
        spec.categories,
        spec.keywords,
        spec.terminal,
    )
}

fn content_for(runtime: &ProductRuntime, id: &str) -> Option<ProductInstalledApplicationContent> {
    let wanted = gnosix_application_catalog::DesktopEntryId::parse(id).expect("valid id");
    runtime
        .installed_applications()
        .into_iter()
        .find(|candidate| candidate.reference.entry() == &wanted)
}

// --- ordering ------------------------------------------------------------

#[test]
fn applications_are_ordered_by_visible_name() {
    let fixture = CatalogFixture::new(
        "order",
        &[
            ("zebra.desktop", &entry("Alpha")),
            ("alpha.desktop", &entry("zulu")),
            ("mid.desktop", &entry("Mike")),
        ],
    );
    let rows = appstack_rows(&fixture.runtime());
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Mike", "zulu"]);
}

#[test]
fn ordering_ignores_case_rather_than_sorting_by_bytes() {
    // A byte sort would put every capitalised name before every lowercase one,
    // which is not a product ordering anyone would accept from a AppStack.
    let fixture = CatalogFixture::new(
        "case",
        &[
            ("b.desktop", &entry("banana")),
            ("a.desktop", &entry("Apple")),
            ("c.desktop", &entry("cherry")),
        ],
    );
    let rows = appstack_rows(&fixture.runtime());
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, vec!["Apple", "banana", "cherry"]);
}

#[test]
fn applications_sharing_a_name_get_a_deterministic_order() {
    // The tie-break is the entry id, so two "Terminal"s have a defined order
    // rather than whatever order the filesystem or a hash map produced.
    let fixture = CatalogFixture::new(
        "duplicates",
        &[
            ("zzz-terminal.desktop", &entry("Terminal")),
            ("aaa-terminal.desktop", &entry("Terminal")),
            ("mmm-terminal.desktop", &entry("Terminal")),
        ],
    );
    let runtime = fixture.runtime();
    let first = appstack_rows(&runtime);
    let second = appstack_rows(&runtime);
    assert_eq!(first, second, "ordering must be stable across calls");
    let ids: Vec<&str> = first.iter().map(|row| row.entry.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "aaa-terminal.desktop",
            "mmm-terminal.desktop",
            "zzz-terminal.desktop"
        ]
    );
}

#[test]
fn ordering_does_not_depend_on_the_filesystem_enumeration_order() {
    // The same set of applications, written in opposite file order, must produce
    // the same product order.
    let forward = CatalogFixture::new(
        "stable-forward",
        &[
            ("a.desktop", &entry("Alpha")),
            ("b.desktop", &entry("Beta")),
            ("c.desktop", &entry("Gamma")),
        ],
    );
    let backward = CatalogFixture::new(
        "stable-backward",
        &[
            ("c.desktop", &entry("Gamma")),
            ("b.desktop", &entry("Beta")),
            ("a.desktop", &entry("Alpha")),
        ],
    );
    let names = |runtime: &ProductRuntime| -> Vec<String> {
        appstack_rows(runtime)
            .into_iter()
            .map(|row| row.name)
            .collect()
    };
    assert_eq!(names(&forward.runtime()), names(&backward.runtime()));
}

// --- M03 search and preference projection -------------------------------

#[test]
fn donor_search_prefixes_parse_to_typed_modes() {
    for (input, mode, text) in [
        ("alpha", AppStackSearchMode::Name, "alpha"),
        ("@i firefox", AppStackSearchMode::Id, "firefox"),
        (
            "@c development",
            AppStackSearchMode::Categories,
            "development",
        ),
        ("@d editing", AppStackSearchMode::Description, "editing"),
        ("@e unity", AppStackSearchMode::Exec, "unity"),
        ("@w firefox", AppStackSearchMode::StartupClass, "firefox"),
        ("@g browser", AppStackSearchMode::GenericName, "browser"),
        ("@k www", AppStackSearchMode::Keywords, "www"),
        ("@t foot", AppStackSearchMode::TerminalOnly, "foot"),
    ] {
        let query = AppStackQuery::parse(input);
        assert_eq!(query.mode, mode, "{input}");
        assert_eq!(query.text, text, "{input}");
    }
}

#[test]
fn specialized_search_uses_catalog_metadata_without_changing_launch_identity() {
    let fixture = CatalogFixture::new(
        "m03-search",
        &[
            (
                "org.mozilla.firefox.desktop",
                &rich_entry(RichEntrySpec {
                    name: "Firefox",
                    generic: "Web Browser",
                    comment: "Browse the Web",
                    command: "/usr/bin/firefox %U",
                    startup_class: "firefox",
                    categories: "Network;WebBrowser;",
                    keywords: "internet;www;browser;",
                    terminal: false,
                }),
            ),
            (
                "com.visualstudio.code.desktop",
                &rich_entry(RichEntrySpec {
                    name: "Visual Studio Code",
                    generic: "Text Editor",
                    comment: "Code editing",
                    command: "/usr/bin/code --unity-launch %F",
                    startup_class: "code",
                    categories: "Development;IDE;",
                    keywords: "code;development;editor;",
                    terminal: false,
                }),
            ),
            (
                "foot.desktop",
                &rich_entry(RichEntrySpec {
                    name: "Foot",
                    generic: "Terminal Emulator",
                    comment: "Wayland terminal",
                    command: "/usr/bin/foot",
                    startup_class: "foot",
                    categories: "System;TerminalEmulator;",
                    keywords: "terminal;shell;",
                    terminal: true,
                }),
            ),
        ],
    );
    let runtime = fixture.runtime();
    let generation = runtime.application_catalog().generation().clone();
    let preferences = AppStackPreferenceProjection::default();

    for (query, expected) in [
        ("@i mozilla", "org.mozilla.firefox.desktop"),
        ("@c development", "com.visualstudio.code.desktop"),
        ("@d editing", "com.visualstudio.code.desktop"),
        ("@e unity", "com.visualstudio.code.desktop"),
        ("@w firefox", "org.mozilla.firefox.desktop"),
        ("@g browser", "org.mozilla.firefox.desktop"),
        ("@k www", "org.mozilla.firefox.desktop"),
        ("@t foot", "foot.desktop"),
    ] {
        let rows = appstack_rows_for(&runtime, AppStackQuery::parse(query), &preferences);
        assert_eq!(rows.len(), 1, "{query}: {rows:?}");
        assert_eq!(rows[0].entry.as_str(), expected, "{query}");
        assert_eq!(
            rows[0].generation, generation,
            "search must preserve the exact catalog generation"
        );
    }

    assert!(
        appstack_rows_for(&runtime, AppStackQuery::parse("editor"), &preferences).is_empty(),
        "ordinary search is name-only; specialized metadata must require its explicit mode"
    );
}

#[test]
fn fuzzy_name_search_is_deterministic_and_identity_preserving() {
    let fixture = CatalogFixture::new(
        "m03-fuzzy",
        &[
            ("code.desktop", &entry("Visual Studio Code")),
            ("calendar.desktop", &entry("Calendar")),
        ],
    );
    let runtime = fixture.runtime();
    let preferences = AppStackPreferenceProjection::default();
    let first = appstack_rows_for(&runtime, AppStackQuery::parse("vsc"), &preferences);
    let second = appstack_rows_for(&runtime, AppStackQuery::parse("vsc"), &preferences);
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].entry.as_str(), "code.desktop");
}

#[test]
fn exact_favourites_precede_matches_and_exact_hidden_ids_disappear() {
    let fixture = CatalogFixture::new(
        "m03-preferences",
        &[
            ("alpha.desktop", &entry("Alpha")),
            ("alpine.desktop", &entry("Alpine")),
            ("atlas.desktop", &entry("Atlas")),
        ],
    );
    let runtime = fixture.runtime();
    let favourite = gnosix_application_catalog::DesktopEntryId::parse("atlas.desktop").unwrap();
    let hidden = gnosix_application_catalog::DesktopEntryId::parse("alpha.desktop").unwrap();
    let preferences = AppStackPreferenceProjection {
        favourites: BTreeSet::from([favourite]),
        hidden: BTreeSet::from([hidden]),
    };
    let rows = appstack_rows_for(&runtime, AppStackQuery::parse("a"), &preferences);
    let ids: Vec<&str> = rows.iter().map(|row| row.entry.as_str()).collect();
    assert_eq!(ids, ["atlas.desktop", "alpine.desktop"]);
    assert!(rows[0].favourite);
    assert!(!rows[1].favourite);
}

#[test]
fn terminal_mode_filters_without_changing_the_launch_reference() {
    let fixture = CatalogFixture::new(
        "m03-terminal",
        &[
            ("gui.desktop", &entry("Terminal Viewer")),
            ("term.desktop", &terminal_entry("Terminal")),
        ],
    );
    let runtime = fixture.runtime();
    let preferences = AppStackPreferenceProjection::default();
    let rows = appstack_rows_for(&runtime, AppStackQuery::parse("@t terminal"), &preferences);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.as_str(), "term.desktop");
    let selected = content_for(&runtime, rows[0].entry.as_str()).unwrap();
    assert_eq!(selected.reference.entry(), &rows[0].entry);
    assert_eq!(selected.reference.generation(), &rows[0].generation);
}

// --- identity ------------------------------------------------------------

#[test]
fn two_applications_sharing_a_name_keep_distinct_identities() {
    let fixture = CatalogFixture::new(
        "identity",
        &[
            ("one.desktop", &entry("Editor")),
            ("two.desktop", &entry("Editor")),
        ],
    );
    let rows = appstack_rows(&fixture.runtime());
    assert_eq!(rows.len(), 2);
    let identities: Vec<_> = rows
        .iter()
        .map(|row| installed_application_control_identity(&row.entry))
        .collect();
    assert_ne!(
        identities[0], identities[1],
        "same visible name must not collapse to one focus identity"
    );
    assert_ne!(
        rows[0].element_id(),
        rows[1].element_id(),
        "same visible name must not collapse to one element id"
    );
}

#[test]
fn focus_identity_is_keyed_by_entry_id_not_by_name() {
    let entry = gnosix_application_catalog::DesktopEntryId::parse("firefox.desktop").expect("id");
    let identity = installed_application_control_identity(&entry);
    assert!(
        identity.as_str().starts_with("app:"),
        "identity must be namespaced, got {}",
        identity.as_str()
    );
    // The visible name must not appear in the identity, or a rename would move
    // focus onto a different application.
    assert!(!identity.as_str().contains("firefox"));
}

#[test]
fn the_affordance_has_its_own_identity_namespace() {
    let appstack = panel_control_identity("appstack");
    let entry = gnosix_application_catalog::DesktopEntryId::parse("appstack.desktop").expect("id");
    let application = installed_application_control_identity(&entry);
    assert_ne!(
        appstack, application,
        "the Dock affordance must not collide with an application named after it"
    );
}

#[test]
fn a_reorder_does_not_change_any_rows_identity() {
    let fixture = CatalogFixture::new(
        "reorder",
        &[
            ("b.desktop", &entry("Beta")),
            ("a.desktop", &entry("Alpha")),
        ],
    );
    let runtime = fixture.runtime();
    let by_name: Vec<(String, String)> = appstack_rows(&runtime)
        .into_iter()
        .map(|row| (row.name.clone(), row.element_id()))
        .collect();
    // A different catalog with the same applications in the other order.
    let other = CatalogFixture::new(
        "reorder-other",
        &[
            ("a.desktop", &entry("Alpha")),
            ("b.desktop", &entry("Beta")),
        ],
    );
    let other_by_name: Vec<(String, String)> = appstack_rows(&other.runtime())
        .into_iter()
        .map(|row| (row.name.clone(), row.element_id()))
        .collect();
    assert_eq!(
        by_name, other_by_name,
        "element identity must follow the application, not its position"
    );
}

// --- generation fence ----------------------------------------------------

#[test]
fn every_row_is_bound_to_the_generation_it_was_presented_from() {
    let fixture = CatalogFixture::new("fence", &[("a.desktop", &entry("Alpha"))]);
    let runtime = fixture.runtime();
    let rows = appstack_rows(&runtime);
    let generation = runtime.application_catalog().generation().clone();
    assert_eq!(rows[0].generation, generation);
}

#[test]
fn rows_from_one_catalog_generation_are_not_silently_retargeted() {
    // The AppStack never re-resolves a selection against whatever the catalog
    // holds now; the capability refuses instead. This test pins that the row
    // carries the generation it was rendered from, which is what makes the
    // refusal reachable.
    let first = CatalogFixture::new("gen-a", &[("a.desktop", &entry("Alpha"))]);
    let presented = appstack_rows(&first.runtime());
    let second = CatalogFixture::new(
        "gen-b",
        &[
            ("a.desktop", &entry("Alpha")),
            ("b.desktop", &entry("Beta")),
        ],
    );
    let now = second.runtime();
    assert_ne!(
        presented[0].generation,
        now.application_catalog().generation().clone(),
        "adding an entry must produce a new generation"
    );
    // The presented row still names the generation it came from, so the
    // capability can compare and refuse rather than assume.
    assert_eq!(
        presented[0].generation,
        first.catalog().generation().clone()
    );
}

// --- feedback ------------------------------------------------------------

#[test]
fn a_appstack_that_has_never_launched_shows_no_feedback() {
    let fixture = CatalogFixture::new("idle", &[("a.desktop", &entry("Alpha"))]);
    let runtime = fixture.runtime();
    assert_eq!(launch_feedback(runtime.launch_state()), None);
}

#[test]
fn a_successful_launch_is_reported_as_launched() {
    let fixture = CatalogFixture::new("ok", &[("ok.desktop", &entry("Alpha"))]);
    let mut runtime = fixture.runtime();
    let selected = content_for(&runtime, "ok.desktop").expect("entry offered");
    let record = crate::product_launch::launch_test_application(&mut runtime, &selected);
    assert_eq!(record.terminal.as_ref().map(|t| t.code()), Some("LAUNCHED"));
    let feedback = launch_feedback(runtime.launch_state()).expect("feedback shown");
    assert_eq!(feedback.code, "LAUNCHED");
    assert!(feedback.launched);
    assert!(
        super::closes_on_successful_launch(&feedback),
        "v1 policy closes the AppStack on success"
    );
}

#[test]
fn a_refusal_is_reported_honestly_and_is_not_a_launch() {
    // A terminal entry has no configured terminal command, so the capability
    // refuses. The AppStack must say "refused" and must not present it as a
    // launch.
    let fixture = CatalogFixture::new("refuse", &[("term.desktop", &terminal_entry("Term"))]);
    let mut runtime = fixture.runtime();
    let selected = content_for(&runtime, "term.desktop").expect("entry offered");
    let record = crate::product_launch::launch_test_application(&mut runtime, &selected);
    assert_eq!(
        record.terminal.as_ref().map(|t| t.code()),
        Some("TERMINAL_REQUIRED")
    );
    let feedback = launch_feedback(runtime.launch_state()).expect("feedback shown");
    assert_eq!(feedback.code, "TERMINAL_REQUIRED");
    assert!(!feedback.launched, "a refusal must never read as a launch");
    assert!(
        !super::closes_on_successful_launch(&feedback),
        "a refusal must leave the AppStack open so the reason stays visible"
    );
}

#[test]
fn a_stale_selection_is_refused_rather_than_retargeted() {
    let first = CatalogFixture::new("stale-a", &[("a.desktop", &entry("Alpha"))]);
    let mut runtime = first.runtime();
    let selected = content_for(&runtime, "a.desktop").expect("entry offered");
    // A different catalog is taken after the selection was built.
    let second = CatalogFixture::new(
        "stale-b",
        &[
            ("a.desktop", &entry("Alpha")),
            ("b.desktop", &entry("Beta")),
        ],
    );
    runtime.set_application_catalog(Arc::new(second.catalog()));
    let record = crate::product_launch::launch_test_application(&mut runtime, &selected);
    assert_eq!(
        record.terminal.as_ref().map(|t| t.code()),
        Some("CATALOG_GENERATION_CHANGED"),
        "a selection made against a catalog the user saw must not be retargeted"
    );
    let feedback = launch_feedback(runtime.launch_state()).expect("feedback shown");
    assert_eq!(feedback.code, "CATALOG_GENERATION_CHANGED");
    assert!(!feedback.launched);
}

#[test]
fn a_spawn_failure_is_distinguished_from_a_refusal() {
    // An Exec that resolves to nothing reaches the spawn and fails there, which
    // is a different product outcome from a refusal: nothing about the request
    // was rejected, the start simply did not work.
    let fixture = CatalogFixture::new(
        "spawn",
        &[(
            "broken.desktop",
            "[Desktop Entry]\nType=Application\nName=Broken\nExec=/nonexistent/gnosix-probe\n",
        )],
    );
    let mut runtime = fixture.runtime();
    let selected = content_for(&runtime, "broken.desktop").expect("entry offered");
    let record = crate::product_launch::launch_test_application(&mut runtime, &selected);
    assert_eq!(
        record.terminal.as_ref().map(|t| t.code()),
        Some("SPAWN_FAILED")
    );
    let feedback = launch_feedback(runtime.launch_state()).expect("feedback shown");
    assert_eq!(feedback.code, "SPAWN_FAILED");
    assert!(!feedback.launched);
}

// --- presentability ------------------------------------------------------

#[test]
fn a_hidden_entry_is_never_offered() {
    let fixture = CatalogFixture::new(
        "hidden",
        &[
            ("shown.desktop", &entry("Shown")),
            (
                "hidden.desktop",
                "[Desktop Entry]\nType=Application\nName=Hidden\nExec=/usr/bin/true\nHidden=true\n",
            ),
        ],
    );
    let rows = appstack_rows(&fixture.runtime());
    let ids: Vec<&str> = rows.iter().map(|row| row.entry.as_str()).collect();
    assert_eq!(ids, vec!["shown.desktop"]);
}

#[test]
fn an_empty_catalog_renders_an_empty_list_rather_than_failing() {
    let fixture = CatalogFixture::new("empty", &[]);
    let rows = appstack_rows(&fixture.runtime());
    assert!(
        rows.is_empty(),
        "an empty AppStack is a state, not an error"
    );
}

#[test]
fn rows_can_be_built_without_a_runtime_at_all() {
    // The ordering rule is a pure function of presented content, so a caller can
    // re-order content it already holds without going back to a catalog.
    let content: Vec<ProductInstalledApplicationContent> = Vec::new();
    assert!(AppStackRow::from_content(content).is_empty());
}
