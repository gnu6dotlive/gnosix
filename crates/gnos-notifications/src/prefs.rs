//! SETTINGS-03B: the operator's notification preferences.
//!
//! Stored as `~/.config/gnosix/notifications.json`, applied by the store:
//! - how long a toast stays when the application leaves it to the server;
//! - whether a critical notification still shows while do not disturb is on;
//! - one rule per application: shown, silent (no toast, kept in the centre)
//!   or off (not kept at all).

use crate::Urgency;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What happens to one application's notifications.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppRule {
    #[default]
    Show,
    /// No toast; the notification is kept in the centre.
    Silent,
    /// Not kept: closed as soon as it arrives.
    Off,
}

impl AppRule {
    pub fn next(self) -> Self {
        match self {
            Self::Show => Self::Silent,
            Self::Silent => Self::Off,
            Self::Off => Self::Show,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Show => "Show",
            Self::Silent => "Silent",
            Self::Off => "Off",
        }
    }
}

/// The toast lives a control offers, in seconds.
pub const TOAST_PRESETS: [u32; 6] = [3, 5, 8, 12, 20, 30];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NotificationPrefs {
    /// Toast life when the application leaves it to the server (3..=30 s).
    pub toast_secs: u32,
    /// A critical notification shows its toast through do not disturb.
    pub critical_through_dnd: bool,
    /// Rules by application name; an application with no rule is shown.
    pub apps: BTreeMap<String, AppRule>,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            toast_secs: 5,
            critical_through_dnd: true,
            apps: BTreeMap::new(),
        }
    }
}

impl NotificationPrefs {
    pub fn is_valid(&self) -> bool {
        (3..=30).contains(&self.toast_secs)
            && self.apps.len() <= 500
            && self
                .apps
                .keys()
                .all(|k| !k.is_empty() && k.chars().count() <= 200)
    }

    pub fn rule(&self, app_name: &str) -> AppRule {
        self.apps.get(app_name).copied().unwrap_or_default()
    }

    /// The toast life one step from now, clamped at the ends.
    pub fn stepped_toast(&self, dir: i8) -> u32 {
        let at = TOAST_PRESETS
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.abs_diff(self.toast_secs))
            .map_or(1, |(i, _)| i);
        let next =
            (at as isize + isize::from(dir.signum())).clamp(0, TOAST_PRESETS.len() as isize - 1);
        TOAST_PRESETS[next as usize]
    }

    /// Whether a toast may show for this notification, given do not disturb.
    pub fn toast_through(&self, app_name: &str, urgency: Urgency, do_not_disturb: bool) -> bool {
        if self.rule(app_name) != AppRule::Show {
            return false;
        }
        !do_not_disturb || (self.critical_through_dnd && urgency == Urgency::Critical)
    }
}

/// `$XDG_CONFIG_HOME/gnosix/notifications.json`, else under `~/.config`.
pub fn default_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))?;
    Some(base.join("gnosix").join("notifications.json"))
}

/// The preferences at `path`; the default when absent or invalid.
pub fn load(path: &std::path::Path) -> NotificationPrefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<NotificationPrefs>(&t).ok())
        .filter(NotificationPrefs::is_valid)
        .unwrap_or_default()
}

/// Write atomically; refuses invalid preferences.
pub fn save(path: &std::path::Path, prefs: &NotificationPrefs) -> std::io::Result<()> {
    if !prefs.is_valid() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid preferences",
        ));
    }
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no directory"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".notifications.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(prefs)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_and_do_not_disturb_decide_the_toast() {
        let mut p = NotificationPrefs::default();
        assert!(p.toast_through("mail", Urgency::Normal, false));
        assert!(!p.toast_through("mail", Urgency::Normal, true));
        assert!(p.toast_through("mail", Urgency::Critical, true));
        p.critical_through_dnd = false;
        assert!(!p.toast_through("mail", Urgency::Critical, true));
        p.apps.insert("mail".into(), AppRule::Silent);
        assert!(!p.toast_through("mail", Urgency::Critical, false));
        assert_eq!(p.rule("chat"), AppRule::Show);
        assert_eq!(AppRule::Show.next().next().next(), AppRule::Show);
    }

    #[test]
    fn the_toast_life_steps_through_presets_and_the_store_round_trips() {
        let p = NotificationPrefs::default();
        assert_eq!(p.stepped_toast(1), 8);
        assert_eq!(p.stepped_toast(-1), 3);
        let top = NotificationPrefs {
            toast_secs: 30,
            ..p.clone()
        };
        assert_eq!(top.stepped_toast(1), 30);
        let dir = std::env::temp_dir().join(format!("gnos-notif-prefs-{}", std::process::id()));
        let path = dir.join("gnosix").join("notifications.json");
        assert_eq!(load(&path), p);
        let mut q = p.clone();
        q.apps.insert("Spotify".into(), AppRule::Off);
        save(&path, &q).unwrap();
        assert_eq!(load(&path), q);
        assert!(
            save(
                &path,
                &NotificationPrefs {
                    toast_secs: 99,
                    ..q.clone()
                }
            )
            .is_err()
        );
        std::fs::write(&path, r#"{"toast_secs": 99}"#).unwrap();
        assert_eq!(load(&path), NotificationPrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
