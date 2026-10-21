//! SURF-01C (U04): the OSD out of the Bar, beside the status icons.
//!
//! A compact level (output volume, microphone, a display's brightness)
//! extrudes from the Bar where Utilities does, whenever the scene observes
//! a change, whoever made it. It retracts after `DWELL`; while the pointer
//! is over it, it stays, and the dwell starts again when the pointer leaves.
//! It is pure: the scene gives it observed readings only, and it can change
//! nothing.
//!
//! While Utilities is open the OSD gives way: the panel shows the same
//! levels, in the same place.

use crate::bar_popout_host::PopoutFrame;
use crate::rail_topology::RailSide;
use crate::utilities_panel::beside_module;
use gnos_ux_surface::grammar::{ExtrusionMotion, TemporalResponseMode};
use std::time::{Duration, Instant};

pub const OSD_WIDTH: f32 = 240.0;
pub const OSD_HEIGHT: f32 = 56.0;
/// How long a reading stays out without a further change (AUDIO-01's dwell).
pub const DWELL: Duration = crate::product_audio_osd::AUDIO_OSD_DWELL;

/// SET-02A: the operator's dwell (`DWELL` by default).
fn dwell() -> Duration {
    #[cfg(feature = "component-lab")]
    {
        crate::surface_prefs::current().osd_dwell()
    }
    #[cfg(not(feature = "component-lab"))]
    {
        DWELL
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsdKind {
    Output,
    Input,
    Brightness,
}

impl OsdKind {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Input => "input",
            Self::Brightness => "brightness",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Output => gnos_i18n::t("Volume"),
            Self::Input => gnos_i18n::t("Microphone"),
            Self::Brightness => gnos_i18n::t("Brightness"),
        }
    }
}

/// One observed level.
#[derive(Clone, Debug, PartialEq)]
pub struct OsdReading {
    pub kind: OsdKind,
    /// What it is (a device or display name).
    pub label: String,
    /// `None`: the provider published no level; never shown as zero.
    pub percent: Option<u8>,
    pub muted: bool,
}

pub struct BarOsd {
    reading: Option<OsdReading>,
    open: bool,
    motion: ExtrusionMotion,
    anchor: Option<f32>,
    side: RailSide,
    viewport: (f32, f32),
    dismiss_at: Option<Instant>,
    hovered: bool,
    last_tick: Option<Instant>,
}

impl Default for BarOsd {
    fn default() -> Self {
        Self::new()
    }
}

impl BarOsd {
    pub fn new() -> Self {
        Self {
            reading: None,
            open: false,
            motion: ExtrusionMotion::closed(),
            anchor: None,
            side: RailSide::Left,
            viewport: (0.0, 0.0),
            dismiss_at: None,
            hovered: false,
            last_tick: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn reading(&self) -> Option<&OsdReading> {
        self.reading.as_ref()
    }

    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Placement inputs. Returns whether anything visible changed.
    pub fn place(&mut self, side: RailSide, viewport: (f32, f32), anchor: Option<f32>) -> bool {
        let changed = self.side != side || self.viewport != viewport || self.anchor != anchor;
        self.side = side;
        self.viewport = viewport;
        self.anchor = anchor;
        changed
    }

    /// An observed change: present it (again) for `DWELL` from `now`.
    pub fn show(&mut self, reading: OsdReading, now: Instant) {
        self.reading = Some(reading);
        self.open = true;
        self.motion.set_open(true);
        if !self.hovered {
            self.dismiss_at = Some(now + dwell());
        }
    }

    /// Retract at once (a Bar travel, Utilities opening).
    pub fn close(&mut self) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        self.hovered = false;
        self.dismiss_at = None;
        self.motion.set_open(false);
        true
    }

    /// The pointer moved: over the OSD it holds; leaving starts the dwell
    /// again. Returns whether that changed.
    pub fn pointer_moved(&mut self, x: f32, y: f32, now: Instant) -> bool {
        let over = self.open
            && self.frame().is_some_and(|f| {
                let v = f.visible;
                x >= v.left() && x < v.right() && y >= v.top() && y < v.bottom()
            });
        if over == self.hovered {
            return false;
        }
        self.hovered = over;
        self.dismiss_at = if over { None } else { Some(now + dwell()) };
        true
    }

    /// The pointer left the scene's surface (outside the OSD's input region
    /// no motion arrives): as leaving the OSD.
    pub fn pointer_left(&mut self, now: Instant) -> bool {
        if !self.hovered {
            return false;
        }
        self.hovered = false;
        self.dismiss_at = Some(now + dwell());
        true
    }

    /// Whether a frame (motion, or a pending dismissal) is still needed.
    pub fn needs_frame(&self) -> bool {
        self.motion.is_open() != self.open || self.motion.needs_frame() || self.dismiss_at.is_some()
    }

    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        if self.open && self.dismiss_at.is_some_and(|at| now >= at) {
            self.open = false;
            self.dismiss_at = None;
        }
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |p| now.duration_since(p).as_secs_f32())
            .clamp(0.0, 0.1);
        self.last_tick = Some(now);
        self.motion.set_open(self.open);
        let mode = if reduced {
            TemporalResponseMode::Instant
        } else {
            TemporalResponseMode::Full
        };
        if self.motion.step(dt, mode).is_err() {
            self.motion.snap();
        }
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    /// Whether the extrusion itself is moving (not only waiting to dismiss).
    pub fn moving(&self) -> bool {
        self.motion.is_open() != self.open || self.motion.needs_frame()
    }

    pub fn frame(&self) -> Option<PopoutFrame> {
        let anchor = self.anchor?;
        let openness = self.motion.phase();
        if openness <= 0.0 || self.reading.is_none() {
            return None;
        }
        beside_module(
            self.side,
            self.viewport,
            anchor,
            OSD_WIDTH,
            OSD_HEIGHT,
            openness,
        )
    }
}

/// The reading that changed between two observations of one control, if
/// any: the same thing observed before (a first observation, or another
/// device, is not a change), with another level or mute state.
pub fn changed(previous: Option<&OsdReading>, current: Option<&OsdReading>) -> Option<OsdReading> {
    let (before, now) = (previous?, current?);
    (before.kind == now.kind
        && before.label == now.label
        && (before.percent != now.percent || before.muted != now.muted))
        .then(|| now.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(kind: OsdKind, percent: u8, muted: bool) -> OsdReading {
        OsdReading {
            kind,
            label: "Speakers".to_owned(),
            percent: Some(percent),
            muted,
        }
    }

    fn settle(osd: &mut BarOsd, mut now: Instant, ms: u64) -> Instant {
        let end = now + Duration::from_millis(ms);
        while now < end {
            now += Duration::from_millis(16);
            osd.tick(now, false);
        }
        now
    }

    #[test]
    fn a_change_extrudes_it_beside_the_status_icons_on_any_side() {
        for side in [
            RailSide::Left,
            RailSide::Right,
            RailSide::Top,
            RailSide::Bottom,
        ] {
            let mut osd = BarOsd::new();
            osd.place(side, (1280.0, 800.0), Some(700.0));
            let now = Instant::now();
            osd.show(reading(OsdKind::Output, 40, false), now);
            settle(&mut osd, now, 400);
            let f = osd.frame().expect("out");
            assert_eq!(f.openness, 1.0);
            assert!(f.hull.is_some(), "{side:?}: joined to the Bar");
            let rail = crate::rail_topology::rail_width();
            match side {
                RailSide::Left => assert_eq!(f.rect.left(), rail),
                RailSide::Right => assert_eq!(f.rect.right(), 1280.0 - rail),
                RailSide::Top => assert_eq!(f.rect.top(), rail),
                RailSide::Bottom => assert_eq!(f.rect.bottom(), 800.0 - rail),
            }
            assert!(f.rect.left() >= 0.0 && f.rect.right() <= 1280.0);
            assert!(f.rect.top() >= 0.0 && f.rect.bottom() <= 800.0);
        }
    }

    #[test]
    fn it_retracts_after_the_dwell_unless_hovered() {
        let mut osd = BarOsd::new();
        osd.place(RailSide::Left, (1280.0, 800.0), Some(400.0));
        let start = Instant::now();
        osd.show(reading(OsdKind::Output, 40, false), start);
        let now = settle(&mut osd, start, 400);
        // Hovered: it holds well past the dwell.
        let f = osd.frame().unwrap();
        let (x, y) = (f.visible.left() + 20.0, f.visible.top() + 20.0);
        assert!(osd.pointer_moved(x, y, now));
        let now = settle(&mut osd, now, 3000);
        assert!(osd.is_open() && osd.frame().is_some());
        // Left (here, the surface itself): the dwell starts again, then it
        // retracts.
        assert!(osd.pointer_left(now));
        assert!(!osd.pointer_left(now), "once");
        let now = settle(&mut osd, now, 1000);
        assert!(osd.is_open());
        settle(&mut osd, now, 1500);
        assert!(!osd.is_open());
        assert!(osd.frame().is_none());
        assert!(!osd.needs_frame());
    }

    #[test]
    fn a_new_change_restarts_the_dwell_and_shows_the_newest_reading() {
        let mut osd = BarOsd::new();
        osd.place(RailSide::Top, (1280.0, 800.0), Some(1100.0));
        let start = Instant::now();
        osd.show(reading(OsdKind::Output, 40, false), start);
        let now = settle(&mut osd, start, 1200);
        osd.show(reading(OsdKind::Input, 0, true), now);
        let now = settle(&mut osd, now, 1200);
        assert!(osd.is_open(), "the second change restarted the dwell");
        assert_eq!(osd.reading().unwrap().kind, OsdKind::Input);
        settle(&mut osd, now, 600);
        assert!(!osd.is_open());
    }

    #[test]
    fn only_a_change_of_the_same_control_counts() {
        let a = reading(OsdKind::Output, 40, false);
        assert_eq!(changed(None, Some(&a)), None, "a first observation");
        assert_eq!(changed(Some(&a), Some(&a)), None);
        let louder = reading(OsdKind::Output, 50, false);
        assert_eq!(changed(Some(&a), Some(&louder)), Some(louder.clone()));
        let muted = reading(OsdKind::Output, 40, true);
        assert_eq!(changed(Some(&a), Some(&muted)), Some(muted));
        let other = OsdReading {
            label: "Headphones".to_owned(),
            ..louder
        };
        assert_eq!(changed(Some(&a), Some(&other)), None, "another device");
        assert_eq!(changed(Some(&a), None), None);
    }

    #[test]
    fn reduced_motion_snaps_and_close_retracts_at_once() {
        let mut osd = BarOsd::new();
        osd.place(RailSide::Bottom, (1280.0, 800.0), Some(1100.0));
        let now = Instant::now();
        osd.show(reading(OsdKind::Brightness, 70, false), now);
        osd.tick(now + Duration::from_millis(16), true);
        assert_eq!(osd.frame().unwrap().openness, 1.0);
        assert!(osd.close());
        osd.tick(now + Duration::from_millis(32), true);
        assert!(osd.frame().is_none());
    }
}
