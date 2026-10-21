//! Product-level semantic icon roles and the smallest native icon primitive.
//!
//! A role names the *visual function* of an icon (web, terminal, files, ...),
//! never a graphic implementation. Provider facts (application categories)
//! resolve to a role; the role resolves to a Tabler icon (ICONS-01), stroked
//! natively by `crate::icons`. No font is involved.
//!
//! Roles are added when a product surface needs them; this is deliberately not
//! a complete icon system.

use gpui::{Path, Pixels};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconRole {
    Desktop,
    Web,
    Terminal,
    Game,
    Files,
    Settings,
    Development,
    Media,
    Security,
    System,
}

impl IconRole {
    pub const ALL: [Self; 10] = [
        Self::Desktop,
        Self::Web,
        Self::Terminal,
        Self::Game,
        Self::Files,
        Self::Settings,
        Self::Development,
        Self::Media,
        Self::Security,
        Self::System,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Web => "web",
            Self::Terminal => "terminal",
            Self::Game => "game",
            Self::Files => "files",
            Self::Settings => "settings",
            Self::Development => "development",
            Self::Media => "media",
            Self::Security => "security",
            Self::System => "system",
        }
    }

    /// ICONS-01: the Tabler icon that draws the role.
    pub fn tabler(self) -> &'static str {
        match self {
            Self::Desktop => "device-desktop",
            Self::Web => "world",
            Self::Terminal => "terminal-2",
            Self::Game => "device-gamepad-2",
            Self::Files => "folder",
            Self::Settings => "settings",
            Self::Development => "code",
            Self::Media => "player-play",
            Self::Security => "shield-lock",
            Self::System => "cpu",
        }
    }
}

/// XDG desktop-entry categories in resolution priority order. The first
/// category present on an application decides its role, so a browser that is
/// also tagged `Network` resolves to `Web`. Categories without a role yet are
/// simply absent: the caller falls back to `IconRole::Desktop`.
const CATEGORY_ROLES: &[(&str, IconRole)] = &[
    ("WebBrowser", IconRole::Web),
    ("Security", IconRole::Security),
    ("Development", IconRole::Development),
    ("IDE", IconRole::Development),
    ("TextEditor", IconRole::Development),
    ("Audio", IconRole::Media),
    ("Music", IconRole::Media),
    ("Player", IconRole::Media),
    ("Game", IconRole::Game),
    ("FileTools", IconRole::Files),
    ("FileManager", IconRole::Files),
    ("Filesystem", IconRole::Files),
    ("FileTransfer", IconRole::Files),
    ("Settings", IconRole::Settings),
    ("DesktopSettings", IconRole::Settings),
    ("HardwareSettings", IconRole::Settings),
    ("TerminalEmulator", IconRole::Terminal),
    ("ConsoleOnly", IconRole::Terminal),
    ("Monitor", IconRole::System),
    ("Midi", IconRole::Media),
    ("Mixer", IconRole::Media),
    ("AudioVideoEditing", IconRole::Media),
    ("AudioVideo", IconRole::Media),
    ("Video", IconRole::Media),
    ("TV", IconRole::Media),
    ("System", IconRole::System),
];

pub fn icon_role_for_categories(categories: &[String]) -> Option<IconRole> {
    CATEGORY_ROLES.iter().find_map(|(category, role)| {
        categories
            .iter()
            .any(|candidate| candidate == category)
            .then_some(*role)
    })
}

/// Layout box of an icon of a given size.
///
/// The icon is a square of side `size` sitting on a baseline inside a line
/// box of `1.2 * size`: `0.1 * size` above the square and `0.1 * size` below
/// the baseline, so text-like layout (stacking, centring, baseline alignment)
/// treats it like one line of text. Edges are whole pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconMetrics {
    pub size: f32,
    pub width: f32,
    pub height: f32,
    /// Distance from the top of the box to the baseline (the square's bottom).
    pub baseline: f32,
}

pub fn icon_metrics(size: f32) -> IconMetrics {
    IconMetrics {
        size,
        width: size,
        height: (size * 1.2).round(),
        baseline: (size * 1.1).round(),
    }
}

/// Native paths for `role`, with the icon square's top-left at `origin`.
pub fn icon_paths(role: IconRole, origin: (f32, f32), size: f32) -> Vec<Path<Pixels>> {
    crate::icons::paths(role.tabler(), origin, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_tabler_icon_of_its_own() {
        for (i, a) in IconRole::ALL.iter().enumerate() {
            assert_eq!(icon_paths(*a, (0.0, 0.0), 20.0).len(), 1, "{a:?}");
            for b in &IconRole::ALL[i + 1..] {
                assert_ne!(a.tabler(), b.tabler(), "{a:?} vs {b:?}");
                assert_ne!(a.name(), b.name());
            }
        }
    }

    #[test]
    fn category_priority_decides_the_role() {
        let roles = |categories: &[&str]| {
            icon_role_for_categories(&categories.iter().map(|c| (*c).into()).collect::<Vec<_>>())
        };
        // Priority is the table order, not the order on the application.
        assert_eq!(roles(&["Network", "WebBrowser"]), Some(IconRole::Web));
        assert_eq!(
            roles(&["TerminalEmulator", "System"]),
            Some(IconRole::Terminal)
        );
        assert_eq!(roles(&["Development", "WebBrowser"]), Some(IconRole::Web));
        assert_eq!(roles(&["Video", "AudioVideo"]), Some(IconRole::Media));
        assert_eq!(roles(&["Utility", "Office"]), None);
        assert_eq!(roles(&[]), None);
    }

    #[test]
    fn icon_box_is_a_line_box_derived_from_the_size() {
        let metrics = icon_metrics(20.0);
        assert_eq!(metrics.width, 20.0);
        assert_eq!(metrics.height, 24.0);
        assert_eq!(metrics.baseline, 22.0);
        // The square sits on the baseline, inside the box, scaling with size.
        let big = icon_metrics(40.0);
        assert_eq!((big.height, big.baseline), (48.0, 44.0));
        assert!(metrics.baseline - metrics.size >= 0.0 && metrics.baseline <= metrics.height);
    }
}
