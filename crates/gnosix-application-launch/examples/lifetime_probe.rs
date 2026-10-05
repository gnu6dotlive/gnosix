//! Explicit host qualification only. Fixed harmless sleeper, real catalog
//! resolution and production executor. Never used by AppStack or crate tests.
use gnosix_application_catalog::{
    ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
};
use gnosix_application_launch::{
    ApplicationLauncher, LaunchDocuments, LaunchOutcome, LaunchReference,
};
use std::{fs, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("gnosix-lifetime-probe-{}", std::process::id()));
    fs::create_dir(&root)?;
    fs::write(
        root.join("probe.desktop"),
        "[Desktop Entry]\nType=Application\nName=Lifetime probe\nExec=/usr/bin/sleep 600\n",
    )?;
    let catalog = ApplicationCatalog::discover(
        &ApplicationCatalogRoots::from_application_dirs(vec![root.clone()]),
        &LocalePreferences::neutral(),
        DesktopEnvironment::from_names(["GNOSIX".into()]),
    );
    fs::remove_dir_all(root)?;
    let entry = catalog.entries().first().ok_or("probe catalog empty")?;
    let reference = LaunchReference::new(entry.id.clone(), catalog.generation().clone());
    let receipt =
        ApplicationLauncher::default().launch(&reference, &catalog, &LaunchDocuments::None);
    if !matches!(receipt.outcome, LaunchOutcome::Launched { .. }) {
        return Err(format!("probe start unqualified: {}", receipt.summary()).into());
    }
    println!(
        "GNOSIX_LIFETIME_PROBE caller_pid={} {}",
        std::process::id(),
        receipt.summary()
    );
    // The host driver stops this caller service and observes the target's
    // independent cgroup. It also explicitly stops/collects the target unit.
    std::thread::sleep(Duration::from_secs(600));
    Ok(())
}
