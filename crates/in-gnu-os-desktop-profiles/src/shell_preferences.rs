//! BAR-01D (B03, B04): bounded shell preferences beyond DONOR-02.
//!
//! Each value is a type with its own range, checked when it is decoded, so
//! an out-of-range or unknown value can never reach a profile or the wire.
//! `None` (absence) always means "no durable override": the product keeps
//! its own default and nothing is imported from a donor.
//!
//! - the Bar's modules: which show, in which of the start, centre and end
//!   groups, in which order;
//! - its thickness (the rail around its modules) and its density;
//! - the travel's and the springs' tempo;
//! - the clock's format (24-hour, date), the active window title's maximum width, and which
//!   status icons show;
//! - the Deck's magnification wave (DECK-01 D09, P06 ranges);
//! - the Deck's pins, in order (DECK-01F, B10);
//! - the desktop modules' places (SURF-04, W02).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A preference outside its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{name} must be in {min}..={max}; got {value}")]
pub struct BoundedPreferenceError {
    pub name: &'static str,
    pub value: i64,
    pub min: i64,
    pub max: i64,
}

macro_rules! bounded {
    ($(#[$doc:meta])* $name:ident($inner:ty), $label:literal, $min:literal..=$max:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "i64", into = "i64")]
        pub struct $name($inner);

        impl $name {
            pub const MIN: $inner = $min;
            pub const MAX: $inner = $max;

            pub const fn get(self) -> $inner {
                self.0
            }
        }

        impl TryFrom<i64> for $name {
            type Error = BoundedPreferenceError;

            fn try_from(value: i64) -> Result<Self, Self::Error> {
                if ($min..=$max).contains(&value) {
                    Ok(Self(value as $inner))
                } else {
                    Err(BoundedPreferenceError {
                        name: $label,
                        value,
                        min: $min,
                        max: $max,
                    })
                }
            }
        }

        impl From<$name> for i64 {
            fn from(value: $name) -> Self {
                i64::from(value.0)
            }
        }
    };
}

bounded!(
    /// The Bar's thickness, in pixels: the rail around its 40 px modules.
    BarThickness(u8), "bar thickness", 44..=120
);
bounded!(
    /// The spacing between the Bar's modules, in percent of the default.
    BarDensityPercent(u8), "bar density", 50..=200
);
bounded!(
    /// A motion's speed, in percent of its default tempo. Reduced motion
    /// still snaps whatever it is.
    MotionTempoPercent(u8), "motion tempo", 25..=250
);
bounded!(
    /// The active window title's maximum width on a horizontal Bar, in pixels.
    TitleMaxWidth(u16), "title width", 80..=640
);
bounded!(
    /// The Deck's peak magnification, in percent (100 turns the wave off).
    DeckMagnifyPercent(u8), "deck magnification", 100..=250
);
bounded!(
    /// How far the Deck's wave reaches, in tenths of a slot.
    DeckReachTenths(u8), "deck reach", 10..=60
);
bounded!(
    /// The Deck's label tab shoulders, in pixels.
    DeckShoulderPx(u8), "deck tab shoulder", 4..=64
);

bounded!(
    /// SURF-04 (W02): a desktop module's position along one axis, in
    /// thousandths of the output (its top-left corner).
    DesktopFraction(u16), "desktop module position", 0..=1000
);

bounded!(
    /// A desktop anchor's distance from its edge, in pixels.
    DesktopAnchorOffset(u16), "desktop anchor offset", 0..=4096
);

/// What a desktop module hangs from.
///
/// The modes are examples, not a taxonomy: the wire format refuses an unknown
/// one, and the reader falls back on the default rather than failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DesktopAnchorMode {
    /// DICTATION-DOCK: the element lives in the Deck's strip, in its own
    /// slot, and its surface comes out of the Deck.
    #[default]
    Dock,
    /// Hangs from an edge, a distance away from it.
    Border,
    /// Hangs in a quadrant of the output.
    Quadrant,
    /// Free, but snaps to the nearest anchor.
    Snap,
}

/// Which edge a desktop anchor hangs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DesktopAnchorEdge {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

/// Where a desktop module hangs: what it hangs from, and where along it.
///
/// A module's *position* is a fraction of the output, and a position ignores
/// the movement. An anchor says what the module is attached to; the shell
/// resolves where that lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopModuleAnchor {
    pub mode: DesktopAnchorMode,
    pub edge: DesktopAnchorEdge,
    pub offset: DesktopAnchorOffset,
    /// Where along the edge, in thousandths of the output.
    pub along: DesktopFraction,
}

/// Where a desktop module stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopModulePlace {
    pub x: DesktopFraction,
    pub y: DesktopFraction,
}

/// SURF-04 (W02): the desktop modules' places; `None` is the product's
/// default place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopModuleLayout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<DesktopModulePlace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visualiser: Option<DesktopModulePlace>,
    /// DICTATION-01: the mic patch's anchor; absent means the default one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dictation: Option<DesktopModuleAnchor>,
}

/// The clock's format. When set, both are explicit. (Seconds would need a
/// clock observer that wakes every second; they are not offered.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockFormat {
    pub hour24: bool,
    pub date: bool,
}

/// Which status icons the Bar shows. When set, all four are explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusIconSet {
    pub lock: bool,
    pub network: bool,
    pub bluetooth: bool,
    pub battery: bool,
}

impl Default for StatusIconSet {
    fn default() -> Self {
        Self {
            lock: true,
            network: true,
            bluetooth: true,
            battery: true,
        }
    }
}

/// The Deck's wave (D09). When set, all four are explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckTuning {
    pub magnify: DeckMagnifyPercent,
    pub reach: DeckReachTenths,
    pub tempo: MotionTempoPercent,
    pub shoulder: DeckShoulderPx,
}

/// One of the Bar's modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarModule {
    Logo,
    Workspaces,
    ActiveWindow,
    Tray,
    Clock,
    StatusIcons,
    Power,
}

impl BarModule {
    pub const ALL: [BarModule; 7] = [
        BarModule::Logo,
        BarModule::Workspaces,
        BarModule::ActiveWindow,
        BarModule::Tray,
        BarModule::Clock,
        BarModule::StatusIcons,
        BarModule::Power,
    ];

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|m| *m == self)
            .expect("every module is listed")
    }
}

/// Which group a module stands in, and where in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BarModulePlace {
    Hidden,
    /// Group 0 start, 1 centre, 2 end; then the rank in the group.
    Shown {
        group: u8,
        rank: u8,
    },
}

/// The Bar's modules in three ordered groups (start, centre, end); a module
/// in none of them is hidden. Copy-sized: one place per module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "BarModuleLayoutWire", into = "BarModuleLayoutWire")]
pub struct BarModuleLayout {
    places: [BarModulePlace; 7],
}

/// The layout as written: three lists of module names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarModuleLayoutWire {
    #[serde(default)]
    pub start: Vec<BarModule>,
    #[serde(default)]
    pub centre: Vec<BarModule>,
    #[serde(default)]
    pub end: Vec<BarModule>,
}

/// A module listed twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("bar module {0:?} is listed more than once")]
pub struct BarModuleLayoutError(pub BarModule);

impl BarModuleLayout {
    /// The groups, each in order.
    pub fn groups(&self) -> [Vec<BarModule>; 3] {
        let mut groups: [Vec<(u8, BarModule)>; 3] = Default::default();
        for module in BarModule::ALL {
            if let BarModulePlace::Shown { group, rank } = self.places[module.index()] {
                groups[usize::from(group.min(2))].push((rank, module));
            }
        }
        groups.map(|mut g| {
            g.sort();
            g.into_iter().map(|(_, m)| m).collect()
        })
    }

    pub fn place(&self, module: BarModule) -> BarModulePlace {
        self.places[module.index()]
    }

    pub fn from_groups(groups: [&[BarModule]; 3]) -> Result<Self, BarModuleLayoutError> {
        let mut places = [BarModulePlace::Hidden; 7];
        for (group, modules) in groups.iter().enumerate() {
            for (rank, module) in modules.iter().enumerate() {
                let slot = &mut places[module.index()];
                if *slot != BarModulePlace::Hidden {
                    return Err(BarModuleLayoutError(*module));
                }
                *slot = BarModulePlace::Shown {
                    group: group as u8,
                    rank: rank as u8,
                };
            }
        }
        Ok(Self { places })
    }
}

impl TryFrom<BarModuleLayoutWire> for BarModuleLayout {
    type Error = BarModuleLayoutError;

    fn try_from(wire: BarModuleLayoutWire) -> Result<Self, Self::Error> {
        Self::from_groups([&wire.start, &wire.centre, &wire.end])
    }
}

impl From<BarModuleLayout> for BarModuleLayoutWire {
    fn from(layout: BarModuleLayout) -> Self {
        let [start, centre, end] = layout.groups();
        Self { start, centre, end }
    }
}

/// A pin that is not a desktop entry ID, or a pin list out of bounds.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeckPinError {
    #[error("deck pin {0:?} is not a desktop entry ID")]
    InvalidId(String),
    #[error("the Deck holds at most {max} pins; got {count}")]
    TooMany { count: usize, max: usize },
    #[error("deck pin {0:?} is pinned more than once")]
    Duplicate(String),
}

/// One pinned application, by its desktop entry ID (`name.desktop`): ASCII
/// letters, digits, `.`, `_` and `-`, at most 255 bytes. Nothing else (no
/// path, no command) can reach the profile through it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DeckPinId(String);

impl DeckPinId {
    pub const MAX_LEN: usize = 255;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DeckPinId {
    type Error = DeckPinError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let stem = value.strip_suffix(".desktop").unwrap_or("");
        let valid = !stem.is_empty()
            && value.len() <= Self::MAX_LEN
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if valid {
            Ok(Self(value))
        } else {
            Err(DeckPinError::InvalidId(value))
        }
    }
}

impl From<DeckPinId> for String {
    fn from(value: DeckPinId) -> Self {
        value.0
    }
}

/// The Deck's pins in order (DECK-01F, B10): at most `MAX`, each once.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "Vec<DeckPinId>", into = "Vec<DeckPinId>")]
pub struct DeckPinList(Vec<DeckPinId>);

impl DeckPinList {
    pub const MAX: usize = 32;

    pub fn ids(&self) -> &[DeckPinId] {
        &self.0
    }

    /// The pins from plain IDs, checked.
    pub fn from_ids<I, S>(ids: I) -> Result<Self, DeckPinError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        ids.into_iter()
            .map(|id| DeckPinId::try_from(id.into()))
            .collect::<Result<Vec<_>, _>>()
            .and_then(Self::try_from)
    }
}

impl TryFrom<Vec<DeckPinId>> for DeckPinList {
    type Error = DeckPinError;

    fn try_from(ids: Vec<DeckPinId>) -> Result<Self, Self::Error> {
        if ids.len() > Self::MAX {
            return Err(DeckPinError::TooMany {
                count: ids.len(),
                max: Self::MAX,
            });
        }
        for (i, id) in ids.iter().enumerate() {
            if ids[..i].contains(id) {
                return Err(DeckPinError::Duplicate(id.as_str().to_owned()));
            }
        }
        Ok(Self(ids))
    }
}

impl From<DeckPinList> for Vec<DeckPinId> {
    fn from(value: DeckPinList) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_values_refuse_their_outside() {
        assert_eq!(BarThickness::try_from(60).unwrap().get(), 60);
        assert!(BarThickness::try_from(43).is_err());
        assert!(BarThickness::try_from(121).is_err());
        assert!(MotionTempoPercent::try_from(251).is_err());
        assert!(serde_json::from_str::<TitleMaxWidth>("79").is_err());
        assert_eq!(
            serde_json::to_string(&DeckReachTenths::try_from(25).unwrap()).unwrap(),
            "25"
        );
        let error = DeckMagnifyPercent::try_from(251).unwrap_err();
        assert_eq!((error.min, error.max), (100, 250));
    }

    #[test]
    fn a_module_layout_round_trips_and_refuses_duplicates() {
        use BarModule as M;
        let layout = BarModuleLayout::from_groups([
            &[M::Logo, M::Workspaces],
            &[M::Clock],
            &[M::StatusIcons, M::Power],
        ])
        .unwrap();
        assert_eq!(layout.place(M::Tray), BarModulePlace::Hidden);
        let text = serde_json::to_string(&layout).unwrap();
        assert_eq!(
            text,
            r#"{"start":["logo","workspaces"],"centre":["clock"],"end":["status_icons","power"]}"#
        );
        assert_eq!(
            serde_json::from_str::<BarModuleLayout>(&text).unwrap(),
            layout
        );
        assert!(
            serde_json::from_str::<BarModuleLayout>(r#"{"start":["clock"],"end":["clock"]}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<BarModuleLayout>(r#"{"middle":[]}"#).is_err());
        assert!(serde_json::from_str::<BarModuleLayout>(r#"{"start":["exec"]}"#).is_err());
    }

    #[test]
    fn whole_blocks_are_explicit() {
        assert!(serde_json::from_str::<ClockFormat>(r#"{"hour24":true}"#).is_err());
        let deck: DeckTuning =
            serde_json::from_str(r#"{"magnify":150,"reach":25,"tempo":100,"shoulder":24}"#)
                .unwrap();
        assert_eq!(deck.magnify.get(), 150);
        assert!(serde_json::from_str::<DeckTuning>(
            r#"{"magnify":251,"reach":25,"tempo":100,"shoulder":24}"#
        )
        .is_err());
    }

    #[test]
    fn deck_pins_are_desktop_ids_bounded_and_unique() {
        let pins =
            DeckPinList::from_ids(["org.gnome.Nautilus.desktop", "firefox.desktop"]).unwrap();
        assert_eq!(pins.ids()[1].as_str(), "firefox.desktop");
        assert_eq!(
            serde_json::to_string(&pins).unwrap(),
            r#"["org.gnome.Nautilus.desktop","firefox.desktop"]"#
        );
        for bad in [
            ".desktop",
            "firefox",
            "../x.desktop",
            "a b.desktop",
            "sh -c.desktop",
        ] {
            assert!(DeckPinId::try_from(bad.to_owned()).is_err(), "{bad}");
        }
        assert!(DeckPinId::try_from(format!("{}.desktop", "a".repeat(250))).is_err());
        assert!(matches!(
            DeckPinList::from_ids(["a.desktop", "a.desktop"]),
            Err(DeckPinError::Duplicate(_))
        ));
        let many: Vec<String> = (0..33).map(|i| format!("app{i}.desktop")).collect();
        assert!(matches!(
            DeckPinList::from_ids(many),
            Err(DeckPinError::TooMany { count: 33, max: 32 })
        ));
        assert!(serde_json::from_str::<DeckPinList>(r#"["x.desktop","/bin/sh"]"#).is_err());
        assert_eq!(
            DeckPinList::from_ids(Vec::<String>::new())
                .unwrap()
                .ids()
                .len(),
            0
        );
    }
}
