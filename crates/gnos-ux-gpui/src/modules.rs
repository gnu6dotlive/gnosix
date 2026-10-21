//! MODS-01: the modules the hull booted.
//!
//! The registry is booted once per process from Gnosix's native manifests
//! (`gnos-modules`), and lists what the mods folder holds without running
//! any of it. The Bar and the desktop show only the modules it booted for
//! their place. Receipt: `GNOS_UX_MODULES`.

use crate::desktop_modules::ModuleKind;
use crate::reference_bar::IdleBarEntry;
use gnos_modules::{Place, Registry};
use std::sync::OnceLock;

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// The booted registry (booting it, and printing its receipt, on first use).
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let registry =
            gnos_modules::boot(gnos_modules::NATIVE, gnos_modules::mods_dir().as_deref());
        let booted: Vec<&str> = registry.booted.iter().map(|m| m.id.as_str()).collect();
        let refused: Vec<String> = registry
            .refused
            .iter()
            .map(|(o, r)| format!("{}:{r}", o.label()))
            .collect();
        let found: Vec<String> = registry
            .found
            .iter()
            .map(|f| format!("{}:{}", f.origin.label(), f.refusal))
            .collect();
        println!("GNOS_UX_MODULES booted={booted:?} refused={refused:?} found={found:?}");
        registry
    })
}

/// The native entry a Bar module boots through; `None` for the Bar's own
/// parts (the spacer and the "…").
pub fn bar_entry(entry: IdleBarEntry) -> Option<&'static str> {
    Some(match entry {
        IdleBarEntry::Logo => "logo",
        IdleBarEntry::Workspaces => "workspaces",
        IdleBarEntry::ActiveWindow => "active_window",
        IdleBarEntry::Tray => "tray",
        IdleBarEntry::Clock => "clock",
        IdleBarEntry::StatusIcons => "status_icons",
        IdleBarEntry::Power => "power",
        IdleBarEntry::Spacer | IdleBarEntry::Overflow => return None,
    })
}

/// Whether the Bar may show `entry`.
pub fn on_bar(entry: IdleBarEntry) -> bool {
    bar_entry(entry).is_none_or(|e| registry().provides(e, Place::Bar))
}

/// Whether the desktop may show `kind`.
pub fn on_desktop(kind: ModuleKind) -> bool {
    registry().provides(kind.slug(), Place::Desktop)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The native modules' names, for the catalogue: their manifests say
    /// them in English, and Settings translates them.
    const NAMES: &[&str] = &[
        "Workspaces",     // i18n
        "Active window",  // i18n
        "Tray",           // i18n
        "Clock",          // i18n
        "Quick settings", // i18n
        "Session",        // i18n
        "Visualiser",     // i18n
        "Dictation",      // i18n
    ];

    /// The native modules' descriptions, for the catalogue (MODS-01B shows
    /// them in Settings › Bar).
    const DESCRIPTIONS: &[&str] = &[
        "Lock, log out, suspend, restart and power off.", // i18n
        "Network, Bluetooth, sound and battery at a glance; Utilities opens out of it.", // i18n
        "The time and date; on the Bar the dashboard opens out of it.", // i18n
        "The focused window's icon and title.",           // i18n
        "The applications' status items, their menus and actions.", // i18n
        "The sound playing, as bars on the desktop.",     // i18n
        "The workspaces of the output; the workspace manager opens out of it.", // i18n
        "The microphone, and what it is hearing.",        // i18n
    ];

    #[test]
    fn every_native_description_is_in_the_catalogue_list() {
        for m in &registry().booted {
            assert!(
                DESCRIPTIONS.contains(&m.description.as_str()),
                "{}",
                m.description
            );
        }
    }

    #[test]
    fn every_native_name_is_in_the_catalogue_list() {
        for m in &registry().booted {
            assert!(NAMES.contains(&m.name.as_str()), "{}", m.name);
        }
    }

    #[test]
    fn every_bar_and_desktop_module_has_a_booted_manifest() {
        // The donor's logo button: Gnosix does not boot it (its role is the
        // AppStack in the Dock).
        assert!(!on_bar(IdleBarEntry::Logo));
        for entry in [
            IdleBarEntry::Workspaces,
            IdleBarEntry::ActiveWindow,
            IdleBarEntry::Tray,
            IdleBarEntry::Clock,
            IdleBarEntry::StatusIcons,
            IdleBarEntry::Power,
            IdleBarEntry::Spacer,
            IdleBarEntry::Overflow,
        ] {
            assert!(on_bar(entry), "{entry:?}");
        }
        for kind in ModuleKind::ALL {
            assert!(on_desktop(kind), "{kind:?}");
        }
    }
}
