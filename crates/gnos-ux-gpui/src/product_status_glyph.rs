//! Gnosix-owned status glyphs.
//!
//! Status entries carry a semantic glyph, never a font ligature name. Each
//! glyph is drawn by a Tabler icon (ICONS-01), stroked natively like
//! `IconRole`, so the Bar draws it in the caller's colour.

use gpui::{Path, Pixels};

/// Power profile as the platform exposes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerProfileGlyph {
    PowerSaver,
    Balanced,
    Performance,
}

/// Device classes the Bar distinguishes. Everything else is `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BluetoothDeviceClass {
    Headphones,
    Speaker,
    Phone,
    Mouse,
    Keyboard,
    Other,
}

impl BluetoothDeviceClass {
    pub const ALL: [Self; 6] = [
        Self::Headphones,
        Self::Speaker,
        Self::Phone,
        Self::Mouse,
        Self::Keyboard,
        Self::Other,
    ];

    /// Classify a BlueZ `Icon` (a freedesktop device-class name). Rules are
    /// ordered substring matches, so `audio-headset` is headphones, not a
    /// speaker. A device without an icon is `Other`.
    pub fn from_bluez_icon(icon: Option<&str>) -> Self {
        const RULES: &[(&[&str], BluetoothDeviceClass)] = &[
            (&["headset", "headphones"], BluetoothDeviceClass::Headphones),
            (&["audio"], BluetoothDeviceClass::Speaker),
            (&["phone"], BluetoothDeviceClass::Phone),
            (&["mouse"], BluetoothDeviceClass::Mouse),
            (&["keyboard"], BluetoothDeviceClass::Keyboard),
        ];
        let Some(icon) = icon else {
            return Self::Other;
        };
        RULES
            .iter()
            .find(|(needles, _)| needles.iter().any(|needle| icon.contains(needle)))
            .map_or(Self::Other, |(_, class)| *class)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Headphones => "headphones",
            Self::Speaker => "speaker",
            Self::Phone => "phone",
            Self::Mouse => "mouse",
            Self::Keyboard => "keyboard",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusGlyph {
    /// Filled signal bars, `0..=4`.
    Wifi {
        bars: u8,
    },
    WifiOff,
    Wired,
    CapsLock,
    NumLock,
    Bluetooth,
    BluetoothDisabled,
    BluetoothConnected,
    /// A connected Bluetooth device, drawn by its device class.
    BluetoothDevice(BluetoothDeviceClass),
    Battery {
        /// Donor-compatible visual level: 0..=6 for partial charge, 7 only
        /// for exactly full. Charging adjusts source levels 1 and 4 downward.
        level: u8,
        charging: bool,
    },
    Profile(PowerProfileGlyph),
    /// The Clock module's calendar mark.
    Calendar,
    /// Default output volume, chosen by `VolumeGlyph::from_level`.
    Volume(VolumeGlyph),
}

/// Speaker states, following the reference volume icon rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeGlyph {
    /// Two waves: level at or above one half.
    High,
    /// One wave: audible below one half.
    Low,
    /// Speaker alone: level exactly zero.
    Silent,
    /// Crossed out: muted, whatever the level.
    Muted,
}

impl VolumeGlyph {
    pub fn from_level(level: f32, muted: bool) -> Self {
        if muted {
            Self::Muted
        } else if level >= 0.5 {
            Self::High
        } else if level > 0.0 {
            Self::Low
        } else {
            Self::Silent
        }
    }
}

impl StatusGlyph {
    /// Wi-Fi bars from a `0..=100` signal strength: one bar per 20 %, capped.
    pub fn wifi_from_strength(strength: u8) -> Self {
        Self::Wifi {
            bars: (strength / 20).min(4),
        }
    }

    pub fn name(self) -> String {
        match self {
            Self::Wifi { bars } => format!("wifi-{bars}"),
            Self::WifiOff => "wifi-off".into(),
            Self::Wired => "wired".into(),
            Self::CapsLock => "caps-lock".into(),
            Self::NumLock => "num-lock".into(),
            Self::Bluetooth => "bluetooth".into(),
            Self::BluetoothDisabled => "bluetooth-disabled".into(),
            Self::BluetoothConnected => "bluetooth-connected".into(),
            Self::BluetoothDevice(class) => format!("bluetooth-device-{}", class.as_str()),
            Self::Battery { level, charging } => format!(
                "battery-{level}-{}",
                if charging { "charging" } else { "discharging" }
            ),
            Self::Profile(PowerProfileGlyph::PowerSaver) => "profile-power-saver".into(),
            Self::Profile(PowerProfileGlyph::Balanced) => "profile-balanced".into(),
            Self::Profile(PowerProfileGlyph::Performance) => "profile-performance".into(),
            Self::Calendar => "calendar".into(),
            Self::Volume(VolumeGlyph::High) => "volume-high".into(),
            Self::Volume(VolumeGlyph::Low) => "volume-low".into(),
            Self::Volume(VolumeGlyph::Silent) => "volume-silent".into(),
            Self::Volume(VolumeGlyph::Muted) => "volume-muted".into(),
        }
    }
}

impl StatusGlyph {
    /// ICONS-01: the Tabler icon that draws the glyph. Levels Tabler draws
    /// in fewer steps share an icon (Wi-Fi 3 and 4 bars; battery levels in
    /// pairs); the glyph's name stays distinct.
    pub fn tabler(self) -> &'static str {
        use BluetoothDeviceClass as D;
        match self {
            Self::Wifi { bars: 0 } => "wifi-0",
            Self::Wifi { bars: 1 } => "wifi-1",
            Self::Wifi { bars: 2 } => "wifi-2",
            Self::Wifi { .. } => "wifi",
            Self::WifiOff => "wifi-off",
            Self::Wired => "network",
            Self::CapsLock => "arrow-big-up-line",
            Self::NumLock => "number-123",
            Self::Bluetooth => "bluetooth",
            Self::BluetoothDisabled => "bluetooth-off",
            Self::BluetoothConnected => "bluetooth-connected",
            Self::BluetoothDevice(D::Headphones) => "headphones",
            Self::BluetoothDevice(D::Speaker) => "device-speaker",
            Self::BluetoothDevice(D::Phone) => "device-mobile",
            Self::BluetoothDevice(D::Mouse) => "mouse",
            Self::BluetoothDevice(D::Keyboard) => "keyboard",
            Self::BluetoothDevice(D::Other) => "devices",
            Self::Battery { charging: true, .. } => "battery-charging",
            Self::Battery { level: 0, .. } => "battery",
            Self::Battery { level: 1 | 2, .. } => "battery-1",
            Self::Battery { level: 3 | 4, .. } => "battery-2",
            Self::Battery { level: 5 | 6, .. } => "battery-3",
            Self::Battery { .. } => "battery-4",
            Self::Profile(PowerProfileGlyph::PowerSaver) => "leaf",
            Self::Profile(PowerProfileGlyph::Balanced) => "scale",
            Self::Profile(PowerProfileGlyph::Performance) => "rocket",
            Self::Calendar => "calendar",
            Self::Volume(VolumeGlyph::High) => "volume",
            Self::Volume(VolumeGlyph::Low) => "volume-2",
            Self::Volume(VolumeGlyph::Silent) => "volume-3",
            Self::Volume(VolumeGlyph::Muted) => "volume-off",
        }
    }
}

/// Native paths for `glyph`, icon square top-left at `origin`.
pub fn status_glyph_paths(glyph: StatusGlyph, origin: (f32, f32), size: f32) -> Vec<Path<Pixels>> {
    crate::icons::paths(glyph.tabler(), origin, size)
}

/// `status_glyph_paths`, drawn on to `drawn` (ICONS-01 motion).
pub fn status_glyph_paths_drawn(
    glyph: StatusGlyph,
    origin: (f32, f32),
    size: f32,
    drawn: f32,
) -> Vec<Path<Pixels>> {
    crate::icons::paths_drawn(glyph.tabler(), origin, size, drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_glyph() -> Vec<StatusGlyph> {
        let mut all: Vec<_> = (0..=4).map(|bars| StatusGlyph::Wifi { bars }).collect();
        all.extend((0..=7).flat_map(|level| {
            [
                StatusGlyph::Battery {
                    level,
                    charging: false,
                },
                StatusGlyph::Battery {
                    level,
                    charging: true,
                },
            ]
        }));
        all.extend(
            BluetoothDeviceClass::ALL
                .into_iter()
                .map(StatusGlyph::BluetoothDevice),
        );
        all.extend([
            StatusGlyph::WifiOff,
            StatusGlyph::Wired,
            StatusGlyph::CapsLock,
            StatusGlyph::NumLock,
            StatusGlyph::Bluetooth,
            StatusGlyph::BluetoothDisabled,
            StatusGlyph::BluetoothConnected,
            StatusGlyph::Profile(PowerProfileGlyph::PowerSaver),
            StatusGlyph::Profile(PowerProfileGlyph::Balanced),
            StatusGlyph::Profile(PowerProfileGlyph::Performance),
            StatusGlyph::Calendar,
            StatusGlyph::Volume(VolumeGlyph::High),
            StatusGlyph::Volume(VolumeGlyph::Low),
            StatusGlyph::Volume(VolumeGlyph::Silent),
            StatusGlyph::Volume(VolumeGlyph::Muted),
        ]);
        all
    }

    #[test]
    fn volume_glyph_follows_the_reference_icon_rule() {
        assert_eq!(VolumeGlyph::from_level(0.8, true), VolumeGlyph::Muted);
        assert_eq!(VolumeGlyph::from_level(0.5, false), VolumeGlyph::High);
        assert_eq!(VolumeGlyph::from_level(0.49, false), VolumeGlyph::Low);
        assert_eq!(VolumeGlyph::from_level(0.0, false), VolumeGlyph::Silent);
    }

    #[test]
    fn every_glyph_has_a_tabler_icon_and_a_unique_name() {
        let all = every_glyph();
        for (i, a) in all.iter().enumerate() {
            assert_eq!(status_glyph_paths(*a, (0.0, 0.0), 18.0).len(), 1, "{a:?}");
            for b in &all[i + 1..] {
                assert_ne!(a.name(), b.name());
            }
        }
    }

    #[test]
    fn wifi_and_battery_levels_step_through_tabler_levels() {
        let wifi: Vec<_> = (0..=4)
            .map(|bars| StatusGlyph::Wifi { bars }.tabler())
            .collect();
        assert_eq!(wifi, ["wifi-0", "wifi-1", "wifi-2", "wifi", "wifi"]);
        let battery = |level| StatusGlyph::Battery {
            level,
            charging: false,
        };
        assert_eq!(battery(0).tabler(), "battery");
        assert_eq!(battery(7).tabler(), "battery-4");
        assert_eq!(
            StatusGlyph::Battery {
                level: 3,
                charging: true
            }
            .tabler(),
            "battery-charging"
        );
    }

    #[test]
    fn wifi_strength_maps_one_bar_per_twenty_percent_capped_at_four() {
        for (strength, bars) in [
            (0, 0),
            (19, 0),
            (20, 1),
            (40, 2),
            (60, 3),
            (80, 4),
            (100, 4),
        ] {
            assert_eq!(
                StatusGlyph::wifi_from_strength(strength),
                StatusGlyph::Wifi { bars }
            );
        }
    }

    #[test]
    fn bluez_device_icons_classify_by_ordered_rules() {
        use BluetoothDeviceClass as C;
        for (icon, class) in [
            (Some("audio-headset"), C::Headphones),
            (Some("audio-headphones"), C::Headphones),
            (Some("audio-card"), C::Speaker),
            (Some("phone"), C::Phone),
            (Some("input-mouse"), C::Mouse),
            (Some("input-keyboard"), C::Keyboard),
            (Some("input-gaming"), C::Other),
            (Some("computer"), C::Other),
            (None, C::Other),
        ] {
            assert_eq!(
                BluetoothDeviceClass::from_bluez_icon(icon),
                class,
                "{icon:?}"
            );
        }
    }
}
