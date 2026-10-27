//! MODS-01: Gnosix's modules, as manifests the hull boots.
//!
//! A module is a capability the hull boots. Its manifest says what it is
//! (id, name, icon), where it can stand (the Bar, the workspace manager, the
//! desktop) and what it needs (time, audio, windows…). Gnosix's own modules
//! are described by the same manifest a mod would carry, embedded in the
//! binary (`manifests/`), and the hull boots only what a valid manifest
//! declares.
//!
//! The mods channel is open but nothing third-party runs: [`discover`] reads
//! the manifests found in the mods folder so Settings can list them, and
//! [`boot`] refuses any provider that is not native.
//!
//! ```toml
//! schema = "gnosix.module.v1"
//! id = "gnosix.clock"
//! name = "Clock"
//! description = "The time and date."
//! icon = "clock"            # a Tabler icon (gnos-icons)
//! version = "1.0.0"
//! places = ["bar", "desktop"]
//! needs = ["time"]
//!
//! [provider]
//! kind = "native"           # the only kind that runs today
//! entry = "clock"           # the native implementation it boots
//! ```

use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};

/// The manifest format this hull reads.
pub const SCHEMA: &str = "gnosix.module.v1";

/// Gnosix's own modules, embedded.
pub const NATIVE: &[(&str, &str)] = &[
    ("workspaces", include_str!("../manifests/workspaces.toml")),
    (
        "active-window",
        include_str!("../manifests/active-window.toml"),
    ),
    ("tray", include_str!("../manifests/tray.toml")),
    ("clock", include_str!("../manifests/clock.toml")),
    (
        "quick-settings",
        include_str!("../manifests/quick-settings.toml"),
    ),
    ("session", include_str!("../manifests/session.toml")),
    ("visualiser", include_str!("../manifests/visualiser.toml")),
    ("dictation", include_str!("../manifests/dictation.toml")),
];

/// The native implementations a manifest can boot.
pub const NATIVE_ENTRIES: &[&str] = &[
    "workspaces",
    "active_window",
    "tray",
    "clock",
    "status_icons",
    "power",
    "visualiser",
    "dictation",
];

/// Where a module can stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    Bar,
    WorkspaceManager,
    Desktop,
}

impl Place {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::WorkspaceManager => "workspace_manager",
            Self::Desktop => "desktop",
        }
    }
}

/// What a module needs from the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    Time,
    Applications,
    Workspaces,
    Windows,
    Tray,
    Network,
    Bluetooth,
    Audio,
    Power,
    Backlight,
    Session,
    /// The state applications publish about themselves on the session bus.
    Indicators,
}

/// Who carries a module out.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provider {
    /// Built into Gnosix: `entry` names the implementation.
    Native { entry: String },
    /// Anything else a mod could declare (a process, a script…); read so it
    /// can be listed, never run.
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub icon: String,
    pub version: String,
    pub places: Vec<Place>,
    #[serde(default)]
    pub needs: Vec<Need>,
    pub provider: Provider,
}

/// Why a manifest was not booted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Unreadable(String),
    Schema(String),
    Id(String),
    NoPlace,
    Icon(String),
    UnknownEntry(String),
    Duplicate(String),
    /// A provider other than native: third-party code does not run.
    NotNative,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(e) => write!(f, "unreadable: {e}"),
            Self::Schema(s) => write!(f, "schema {s:?} is not {SCHEMA}"),
            Self::Id(id) => write!(f, "id {id:?} is not lowercase words joined by dots"),
            Self::NoPlace => write!(f, "no place to stand"),
            Self::Icon(i) => write!(f, "icon {i:?} is not a Tabler icon"),
            Self::UnknownEntry(e) => write!(f, "no native module {e:?}"),
            Self::Duplicate(id) => write!(f, "id {id:?} already booted"),
            Self::NotNative => write!(f, "third-party modules do not run yet"),
        }
    }
}

fn valid_id(id: &str) -> bool {
    let words: Vec<&str> = id.split('.').collect();
    words.len() >= 2
        && words.iter().all(|w| {
            !w.is_empty()
                && w.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Read a manifest, without judging it.
pub fn parse(text: &str) -> Result<Manifest, Refusal> {
    toml::from_str(text).map_err(|e| Refusal::Unreadable(e.message().to_owned()))
}

/// Whether the hull may boot this manifest.
pub fn check(m: &Manifest) -> Result<(), Refusal> {
    if m.schema != SCHEMA {
        return Err(Refusal::Schema(m.schema.clone()));
    }
    if !valid_id(&m.id) {
        return Err(Refusal::Id(m.id.clone()));
    }
    if m.places.is_empty() {
        return Err(Refusal::NoPlace);
    }
    if gnos_icons::icon(gnos_icons::Style::Outline, &m.icon).is_none() {
        return Err(Refusal::Icon(m.icon.clone()));
    }
    match &m.provider {
        Provider::Native { entry } if NATIVE_ENTRIES.contains(&entry.as_str()) => Ok(()),
        Provider::Native { entry } => Err(Refusal::UnknownEntry(entry.clone())),
        Provider::Other => Err(Refusal::NotNative),
    }
}

/// Where a manifest came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Embedded in Gnosix, by its file stem.
    Native(&'static str),
    /// A file in the mods folder.
    File(PathBuf),
}

impl Origin {
    pub fn label(&self) -> String {
        match self {
            Self::Native(stem) => format!("native:{stem}"),
            Self::File(path) => path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        }
    }
}

/// A manifest found in the mods folder: listed, not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub origin: Origin,
    pub manifest: Option<Manifest>,
    pub refusal: Refusal,
}

/// What the hull booted, and what it did not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    pub booted: Vec<Manifest>,
    /// Native manifests refused (a defect in Gnosix itself).
    pub refused: Vec<(Origin, Refusal)>,
    /// Manifests found in the mods folder.
    pub found: Vec<Found>,
}

impl Registry {
    /// Whether a booted module provides the native `entry` at `place`.
    pub fn provides(&self, entry: &str, place: Place) -> bool {
        self.booted.iter().any(|m| {
            m.places.contains(&place)
                && matches!(&m.provider, Provider::Native { entry: e } if e == entry)
        })
    }

    /// The booted module that provides `entry`.
    pub fn module(&self, entry: &str) -> Option<&Manifest> {
        self.booted
            .iter()
            .find(|m| matches!(&m.provider, Provider::Native { entry: e } if e == entry))
    }
}

/// Boot the native manifests, and list (never run) what the mods folder
/// holds.
pub fn boot(native: &[(&'static str, &str)], mods: Option<&Path>) -> Registry {
    let mut registry = Registry::default();
    for (stem, text) in native {
        let origin = Origin::Native(stem);
        match parse(text).and_then(|m| check(&m).map(|()| m)) {
            Ok(m) if registry.booted.iter().any(|b| b.id == m.id) => {
                registry.refused.push((origin, Refusal::Duplicate(m.id)));
            }
            Ok(m) => registry.booted.push(m),
            Err(refusal) => registry.refused.push((origin, refusal)),
        }
    }
    if let Some(dir) = mods {
        registry.found = discover(dir);
    }
    registry
}

/// The manifests in the mods folder: `<dir>/*.toml` and
/// `<dir>/*/module.toml`, in name order. Each is read and judged, and none
/// is booted: today every one is refused, at the least as not native.
pub fn discover(dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter_map(|p| {
            if p.is_dir() {
                Some(p.join("module.toml")).filter(|m| m.is_file())
            } else {
                (p.extension().is_some_and(|x| x == "toml")).then_some(p)
            }
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let read = std::fs::read_to_string(&path)
                .map_err(|e| Refusal::Unreadable(e.to_string()))
                .and_then(|t| parse(&t));
            let (manifest, refusal) = match read {
                Ok(m) => {
                    // A mod may claim a native entry; it is still a file
                    // the operator added, so it does not run.
                    let refusal = check(&m).err().unwrap_or(Refusal::NotNative);
                    (Some(m), refusal)
                }
                Err(refusal) => (None, refusal),
            };
            Found {
                origin: Origin::File(path),
                manifest,
                refusal,
            }
        })
        .collect()
}

/// The mods folder: `$XDG_DATA_HOME/gnosix/mods`, else
/// `~/.local/share/gnosix/mods`.
pub fn mods_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("gnosix/mods"))
}

#[cfg(test)]
mod tests;
