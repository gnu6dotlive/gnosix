use serde::{Deserialize, Serialize};

/// Every lock setting, bounded. `None` on an optional means "off".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LockConfig {
    /// Lock after this many idle seconds (60..=86400); `None` never.
    pub idle_lock_secs: Option<u32>,
    /// Lock before the system sleeps.
    pub lock_before_sleep: bool,
    /// Switch the screens off after this many seconds locked (10..=3600);
    /// `None` never.
    pub screens_off_secs: Option<u32>,
    pub show_clock: bool,
    pub show_date: bool,
    /// `None` follows the system's clock cycle.
    pub clock_24h: Option<bool>,
    pub show_user: bool,
    /// Shown under the clock (at most 120 characters).
    pub message: String,
    /// Failed attempts before the lock makes the user wait (1..=10).
    pub attempts_before_wait: u8,
    /// How long it waits (5..=600 seconds).
    pub wait_secs: u32,
    /// Show one dot per typed character.
    pub show_typed_length: bool,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            idle_lock_secs: Some(300),
            lock_before_sleep: true,
            screens_off_secs: Some(60),
            show_clock: true,
            show_date: true,
            clock_24h: None,
            show_user: true,
            message: String::new(),
            attempts_before_wait: 3,
            wait_secs: 30,
            show_typed_length: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigIssue {
    IdleLock,
    ScreensOff,
    Message,
    Attempts,
    Wait,
}

impl LockConfig {
    /// The first setting out of its bounds.
    pub fn validate(&self) -> Result<(), ConfigIssue> {
        let within = |v: u32, lo: u32, hi: u32| (lo..=hi).contains(&v);
        if self.idle_lock_secs.is_some_and(|v| !within(v, 60, 86_400)) {
            return Err(ConfigIssue::IdleLock);
        }
        if self.screens_off_secs.is_some_and(|v| !within(v, 10, 3_600)) {
            return Err(ConfigIssue::ScreensOff);
        }
        if self.message.chars().count() > 120 || self.message.chars().any(char::is_control) {
            return Err(ConfigIssue::Message);
        }
        if !(1..=10).contains(&self.attempts_before_wait) {
            return Err(ConfigIssue::Attempts);
        }
        if !within(self.wait_secs, 5, 600) {
            return Err(ConfigIssue::Wait);
        }
        Ok(())
    }

    /// The config as is when it is valid, the default otherwise: a bad
    /// profile never leaves the screen without a lock.
    pub fn or_default(self) -> Self {
        if self.validate().is_ok() {
            self
        } else {
            Self::default()
        }
    }
}

/// One setting a Settings control steps. A control names the setting and a
/// direction, never a value: the next value comes from the current config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockSetting {
    IdleLock,
    LockBeforeSleep,
    ScreensOff,
    ShowClock,
    ShowDate,
    /// System, 12 hour, 24 hour.
    ClockCycle,
    ShowUser,
    ShowTypedLength,
    Attempts,
    Wait,
}

/// The idle delays a control offers, in seconds (`None` is off).
pub const IDLE_PRESETS: [Option<u32>; 7] = [
    None,
    Some(60),
    Some(120),
    Some(300),
    Some(600),
    Some(900),
    Some(1800),
];
/// The screens-off delays a control offers, in seconds.
pub const SCREENS_OFF_PRESETS: [Option<u32>; 7] = [
    None,
    Some(10),
    Some(30),
    Some(60),
    Some(120),
    Some(300),
    Some(600),
];
/// The waits after failures a control offers, in seconds.
pub const WAIT_PRESETS: [u32; 7] = [5, 10, 30, 60, 120, 300, 600];

/// The preset one step from `current` (the nearest one when `current` is
/// not itself a preset), clamped at the ends.
fn step_preset<T: Copy + PartialEq>(
    presets: &[T],
    current: T,
    dir: i8,
    distance: impl Fn(T, T) -> u64,
) -> T {
    let index = presets
        .iter()
        .position(|p| *p == current)
        .unwrap_or_else(|| {
            presets
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| distance(**p, current))
                .map_or(0, |(i, _)| i)
        });
    let next = (index as isize + isize::from(dir.signum())).clamp(0, presets.len() as isize - 1);
    presets[next as usize]
}

impl LockConfig {
    /// The config after one step of `setting` in direction `dir` (-1 or 1);
    /// always valid.
    pub fn stepped(&self, setting: LockSetting, dir: i8) -> Self {
        let mut next = self.clone();
        let secs = |o: Option<u32>| u64::from(o.unwrap_or(0));
        match setting {
            LockSetting::IdleLock => {
                next.idle_lock_secs =
                    step_preset(&IDLE_PRESETS, self.idle_lock_secs, dir, |a, b| {
                        secs(a).abs_diff(secs(b))
                    });
            }
            LockSetting::ScreensOff => {
                next.screens_off_secs =
                    step_preset(&SCREENS_OFF_PRESETS, self.screens_off_secs, dir, |a, b| {
                        secs(a).abs_diff(secs(b))
                    });
            }
            LockSetting::Wait => {
                next.wait_secs = step_preset(&WAIT_PRESETS, self.wait_secs, dir, |a, b| {
                    u64::from(a).abs_diff(u64::from(b))
                });
            }
            LockSetting::Attempts => {
                next.attempts_before_wait = (i16::from(self.attempts_before_wait)
                    + i16::from(dir.signum()))
                .clamp(1, 10) as u8;
            }
            LockSetting::LockBeforeSleep => next.lock_before_sleep = !self.lock_before_sleep,
            LockSetting::ShowClock => next.show_clock = !self.show_clock,
            LockSetting::ShowDate => next.show_date = !self.show_date,
            LockSetting::ShowUser => next.show_user = !self.show_user,
            LockSetting::ShowTypedLength => next.show_typed_length = !self.show_typed_length,
            LockSetting::ClockCycle => {
                next.clock_24h = match self.clock_24h {
                    None => Some(false),
                    Some(false) => Some(true),
                    Some(true) => None,
                };
            }
        }
        next
    }
}

/// "5 min", "Off".
pub fn minutes_label(secs: Option<u32>) -> String {
    match secs {
        None => "Off".into(),
        Some(s) if s < 60 => format!("{s} s"),
        Some(s) if s % 60 == 0 => format!("{} min", s / 60),
        Some(s) => format!("{} min {} s", s / 60, s % 60),
    }
}

/// Where the settings live: `$XDG_CONFIG_HOME/gnosix/lock.json`, else
/// `~/.config/gnosix/lock.json`.
pub fn default_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))?;
    Some(base.join("gnosix").join("lock.json"))
}

/// The settings at `path`; the default when the file is absent, unreadable
/// or out of bounds, so a bad file never leaves the screen without a lock.
pub fn load(path: &std::path::Path) -> LockConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<LockConfig>(&text).ok())
        .unwrap_or_default()
        .or_default()
}

/// Write the settings atomically (a temporary file, then a rename), with
/// the owner as the only reader. Refuses a config out of bounds.
pub fn save(path: &std::path::Path, config: &LockConfig) -> std::io::Result<()> {
    use std::io::Write;
    config.validate().map_err(|issue| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{issue:?}"))
    })?;
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no directory"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".lock.json.{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    file.write_all(serde_json::to_string_pretty(config)?.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_valid_and_every_bound_is_enforced() {
        assert_eq!(LockConfig::default().validate(), Ok(()));
        let with = |f: fn(&mut LockConfig)| {
            let mut c = LockConfig::default();
            f(&mut c);
            c.validate()
        };
        assert_eq!(
            with(|c| c.idle_lock_secs = Some(59)),
            Err(ConfigIssue::IdleLock)
        );
        assert_eq!(
            with(|c| c.idle_lock_secs = Some(86_401)),
            Err(ConfigIssue::IdleLock)
        );
        assert_eq!(with(|c| c.idle_lock_secs = None), Ok(()));
        assert_eq!(
            with(|c| c.screens_off_secs = Some(9)),
            Err(ConfigIssue::ScreensOff)
        );
        assert_eq!(
            with(|c| c.message = "x".repeat(121)),
            Err(ConfigIssue::Message)
        );
        assert_eq!(
            with(|c| c.message = "a\nb".into()),
            Err(ConfigIssue::Message)
        );
        assert_eq!(
            with(|c| c.attempts_before_wait = 0),
            Err(ConfigIssue::Attempts)
        );
        assert_eq!(
            with(|c| c.attempts_before_wait = 11),
            Err(ConfigIssue::Attempts)
        );
        assert_eq!(with(|c| c.wait_secs = 4), Err(ConfigIssue::Wait));
    }

    #[test]
    fn a_bad_config_falls_back_to_the_default_not_to_no_lock() {
        let bad = LockConfig {
            wait_secs: 0,
            ..LockConfig::default()
        };
        assert_eq!(bad.or_default(), LockConfig::default());
        let good = LockConfig {
            idle_lock_secs: None,
            ..LockConfig::default()
        };
        assert_eq!(good.clone().or_default(), good);
    }

    #[test]
    fn a_step_moves_one_preset_clamps_at_the_ends_and_stays_valid() {
        let c = LockConfig::default();
        assert_eq!(
            c.stepped(LockSetting::IdleLock, 1).idle_lock_secs,
            Some(600)
        );
        assert_eq!(
            c.stepped(LockSetting::IdleLock, -1).idle_lock_secs,
            Some(120)
        );
        let off = LockConfig {
            idle_lock_secs: None,
            ..c.clone()
        };
        assert_eq!(off.stepped(LockSetting::IdleLock, -1).idle_lock_secs, None);
        assert_eq!(
            off.stepped(LockSetting::IdleLock, 1).idle_lock_secs,
            Some(60)
        );
        let top = LockConfig {
            idle_lock_secs: Some(1800),
            ..c.clone()
        };
        assert_eq!(
            top.stepped(LockSetting::IdleLock, 1).idle_lock_secs,
            Some(1800)
        );
        // Off a preset (a hand-edited 400 s): the nearest, then a step.
        let odd = LockConfig {
            idle_lock_secs: Some(400),
            ..c.clone()
        };
        assert_eq!(
            odd.stepped(LockSetting::IdleLock, 1).idle_lock_secs,
            Some(600)
        );
        assert_eq!(c.stepped(LockSetting::Wait, 1).wait_secs, 60);
        let one = LockConfig {
            attempts_before_wait: 1,
            ..c.clone()
        };
        assert_eq!(
            one.stepped(LockSetting::Attempts, -1).attempts_before_wait,
            1
        );
        let ten = LockConfig {
            attempts_before_wait: 10,
            ..c.clone()
        };
        assert_eq!(
            ten.stepped(LockSetting::Attempts, 1).attempts_before_wait,
            10
        );
        assert!(!c.stepped(LockSetting::ShowClock, 1).show_clock);
        let mut cycle = c.clone();
        let seen: Vec<_> = (0..3)
            .map(|_| {
                cycle = cycle.stepped(LockSetting::ClockCycle, 1);
                cycle.clock_24h
            })
            .collect();
        assert_eq!(seen, [Some(false), Some(true), None]);
        for setting in [
            LockSetting::IdleLock,
            LockSetting::LockBeforeSleep,
            LockSetting::ScreensOff,
            LockSetting::ShowClock,
            LockSetting::ShowDate,
            LockSetting::ClockCycle,
            LockSetting::ShowUser,
            LockSetting::ShowTypedLength,
            LockSetting::Attempts,
            LockSetting::Wait,
        ] {
            for dir in [-1, 1] {
                assert_eq!(
                    c.stepped(setting, dir).validate(),
                    Ok(()),
                    "{setting:?} {dir}"
                );
            }
        }
    }

    #[test]
    fn the_store_round_trips_refuses_bad_values_and_falls_back_to_the_default() {
        let dir = std::env::temp_dir().join(format!("gnos-lock-test-{}", std::process::id()));
        let path = dir.join("gnosix").join("lock.json");
        assert_eq!(load(&path), LockConfig::default());
        let c = LockConfig {
            idle_lock_secs: Some(600),
            message: "hi".into(),
            ..LockConfig::default()
        };
        save(&path, &c).unwrap();
        assert_eq!(load(&path), c);
        let bad = LockConfig {
            wait_secs: 1,
            ..c.clone()
        };
        assert!(save(&path, &bad).is_err());
        assert_eq!(load(&path), c);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), LockConfig::default());
        std::fs::write(&path, r#"{"wait_secs": 1}"#).unwrap();
        assert_eq!(load(&path), LockConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delays_read_as_minutes() {
        assert_eq!(minutes_label(None), "Off");
        assert_eq!(minutes_label(Some(30)), "30 s");
        assert_eq!(minutes_label(Some(300)), "5 min");
        assert_eq!(minutes_label(Some(90)), "1 min 30 s");
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(serde_json::from_str::<LockConfig>(r#"{"idle_lock_secs":120,"x":1}"#).is_err());
    }
}
