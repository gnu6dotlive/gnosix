//! FOCUS-01: the shell's attendants, on the conductor of
//! `gnos_ux_surface::grammar::attention`.
//!
//! One conductor decides, for every surface that comes out of the shell,
//! when it comes out and goes back in. The surfaces keep their content and
//! their motion; their triggers come from the operator's settings.
//! - The workspaces panel: by hover (the default), a click on the current
//!   workspace, or never (WS-02C).
//! - The dashboard: by hover on the clock, or a click (SET-02A); a click
//!   holds it out either way.
//! - The status popouts (audio, network, Bluetooth, battery): by hover on
//!   their icon.
//! - Session: by a click on the power button, and nothing else (locked).
//! - Utilities, the overflow "…" and Settings: by a click or a command.
//!
//! One surface at a time (FOCUS-01B): the next one waits until the last one
//! has gone back in, as its motion reports, never on a timer. The dashboard
//! and the status popouts are one kin: they share a surface that morphs from
//! one to the other.
//!
//! `GNOS_UX_ATTENTION` reports every change and its reason.

use crate::bar_pointer_routing::{BarPopout, StatusEntryKind};
use crate::surface_prefs::{PanelOpening, SurfacePrefs};
use gnos_ux_surface::grammar::attention::{Conductor, HoverTiming, Transition, Trigger};
use std::time::Duration;

pub use gnos_ux_surface::grammar::attention::{Area, Phase, Reason};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attendant {
    Workspaces,
    Dashboard,
    Status(StatusEntryKind),
    Session,
    Utilities,
    Overflow,
    Settings,
    /// FOCUS-01C: a tray item's menu, by the item's place among those the
    /// rail presents.
    Tray(usize),
}

/// The Bar popouts' kin: one surface, morphing between them.
pub const POPOUT_KIN: u16 = 1;
/// The tray menus' kin: one surface, morphing from item to item.
pub const TRAY_KIN: u16 = 2;

pub type Attention = Conductor<Attendant>;

impl Attendant {
    /// The Bar popouts, which share one host.
    pub const POPOUTS: [Self; 5] = [
        Self::Dashboard,
        Self::Status(StatusEntryKind::Audio),
        Self::Status(StatusEntryKind::Network),
        Self::Status(StatusEntryKind::Bluetooth),
        Self::Status(StatusEntryKind::BatteryOrProfile),
    ];

    /// The attendant a designated popout belongs to; popouts without
    /// content yet (the lock, tray menus, the active window) have none.
    pub fn of_popout(popout: BarPopout) -> Option<Self> {
        let attendant = match popout {
            BarPopout::Dashboard => Self::Dashboard,
            BarPopout::Status(kind) => Self::Status(kind),
            BarPopout::TrayMenu(index) => return Some(Self::Tray(index)),
            _ => return None,
        };
        Self::POPOUTS.contains(&attendant).then_some(attendant)
    }

    /// The panels opened by a click or a command, whose opening is the
    /// scene's to carry out (it needs the window).
    pub const PANELS: [Self; 4] = [
        Self::Session,
        Self::Utilities,
        Self::Overflow,
        Self::Settings,
    ];

    pub fn popout(self) -> Option<BarPopout> {
        match self {
            Self::Dashboard => Some(BarPopout::Dashboard),
            Self::Status(kind) => Some(BarPopout::Status(kind)),
            _ => None,
        }
    }

    pub fn slug(self) -> String {
        if let Self::Tray(index) = self {
            return format!("tray.{index}");
        }
        match self {
            Self::Workspaces => "workspaces",
            Self::Dashboard => "dashboard",
            Self::Status(StatusEntryKind::Audio) => "status.audio",
            Self::Status(StatusEntryKind::Network) => "status.network",
            Self::Status(StatusEntryKind::Bluetooth) => "status.bluetooth",
            Self::Status(StatusEntryKind::BatteryOrProfile) => "status.battery",
            Self::Status(StatusEntryKind::Lock) => "status.lock",
            Self::Session => "session",
            Self::Utilities => "utilities",
            Self::Overflow => "overflow",
            Self::Settings => "settings",
            Self::Tray(_) => "tray",
        }
        .to_owned()
    }
}

/// A hover trigger from the operator's timing (BEHAVIOUR-01).
fn hover(timing: crate::surface_prefs::Timing) -> Trigger {
    Trigger::Hover(HoverTiming {
        dwell: Duration::from_millis(timing.dwell_ms.into()),
        grace: Duration::from_millis(timing.grace_ms.into()),
    })
}

/// Each attendant's trigger, from the operator's settings.
pub fn triggers(prefs: &SurfacePrefs) -> Vec<(Attendant, Trigger)> {
    let mut all = vec![
        (
            Attendant::Workspaces,
            match prefs.workspaces_open {
                PanelOpening::Hover => hover(prefs.behaviour.workspaces),
                PanelOpening::Click => Trigger::Click,
                PanelOpening::Never => Trigger::Off,
            },
        ),
        (
            Attendant::Dashboard,
            if prefs.dashboard_on_hover {
                hover(prefs.behaviour.dashboard)
            } else {
                Trigger::Click
            },
        ),
    ];
    // The status popouts: by hover, or not at all (a click on the status
    // icons is Utilities').
    let status = if prefs.behaviour.status_on_hover {
        hover(prefs.behaviour.status)
    } else {
        Trigger::Off
    };
    all.extend(Attendant::POPOUTS[1..].iter().map(|a| (*a, status)));
    all.extend([
        // Its actions shut the hardware down: a click, and nothing else.
        (Attendant::Session, Trigger::click_only()),
        (Attendant::Utilities, Trigger::Click),
        (Attendant::Overflow, Trigger::Click),
        (Attendant::Settings, Trigger::Click),
    ]);
    all
}

/// Declare (or update) every attendant from the settings, with its kin.
pub fn declare(attention: &mut Attention, prefs: &SurfacePrefs) -> Vec<Transition<Attendant>> {
    let changes = triggers(prefs)
        .into_iter()
        .flat_map(|(a, t)| attention.declare(a, t))
        .collect();
    for a in Attendant::POPOUTS {
        attention.set_kin(a, Some(POPOUT_KIN));
    }
    changes
}

/// FOCUS-01D: the operator's choice for surfaces that meet.
pub fn meeting(prefs: &SurfacePrefs) -> gnos_ux_surface::grammar::attention::Meeting {
    use crate::surface_prefs::SurfacesMeeting as S;
    use gnos_ux_surface::grammar::attention::Meeting as M;
    match prefs.surfaces_meeting {
        S::OneAtATime => M::OneAtATime,
        S::MakeRoom => M::MakeRoom,
        S::Layer => M::Layer,
    }
}

/// FOCUS-01B: how the pointer's heading is read, from the operator's
/// settings (Behaviour); `None` when prediction is off. An aim holds a
/// surface for its grace, or the heading's window if longer, renewed by
/// each move that keeps heading there.
pub fn aim(prefs: &SurfacePrefs) -> Option<gnos_ux_surface::grammar::attention::Aim> {
    let b = &prefs.behaviour;
    b.aim.then(|| gnos_ux_surface::grammar::attention::Aim {
        slack: b.aim_slack_px as f32,
        min_speed: b.aim_min_speed as f32,
        window: Duration::from_millis(b.aim_window_ms.into()),
    })
}

/// FOCUS-01C: the tray's items, by hover, with the operator's timings
/// (Settings › Behaviour), one kin.
pub fn declare_tray(
    attention: &mut Attention,
    count: usize,
    prefs: &SurfacePrefs,
) -> Vec<Transition<Attendant>> {
    // By hover, or on a click (left on a menu-only item, right on any).
    let timing = if prefs.behaviour.tray_on_hover {
        hover(prefs.behaviour.tray)
    } else {
        Trigger::Click
    };
    let mut changes = Vec::new();
    for index in 0..count {
        changes.extend(attention.declare(Attendant::Tray(index), timing));
        attention.set_kin(Attendant::Tray(index), Some(TRAY_KIN));
    }
    changes
}

/// `GNOS_UX_ATTENTION`: one change and its reason.
pub fn receipt(t: &Transition<Attendant>) -> String {
    format!(
        "GNOS_UX_ATTENTION surface={} from={} to={} reason={}",
        t.key.slug(),
        t.from.as_str(),
        t.to.as_str(),
        t.reason.as_str()
    )
}

/// The source the pointer designates: the workspaces module, or the popout
/// the rail's router names.
pub fn over(on_workspaces: bool, designated: Option<BarPopout>) -> Option<Attendant> {
    if on_workspaces {
        return Some(Attendant::Workspaces);
    }
    designated.and_then(Attendant::of_popout)
}

/// Carry one change out on the popout host. Returns whether it changed.
pub fn apply_to_host(
    host: &mut crate::bar_popout_host::BarPopoutHost,
    t: &Transition<Attendant>,
    fallback: Option<f32>,
) -> bool {
    let Some(popout) = t.key.popout() else {
        return false;
    };
    if t.to.is_out() {
        host.show(popout, fallback)
    } else if host.shown_popout() == Some(popout) {
        host.hide()
    } else {
        false
    }
}

/// The popouts' bodies: the host's frame belongs to the popout that is out.
pub fn place_popouts(
    attention: &mut Attention,
    host: &crate::bar_popout_host::BarPopoutHost,
    source: impl Fn(Attendant) -> Option<Area>,
) {
    let shown = host.shown_popout().and_then(Attendant::of_popout);
    let body = host.frame().map(|f| {
        let v = f.visible;
        Area::new(v.left(), v.top(), v.right(), v.bottom())
    });
    for a in Attendant::POPOUTS {
        attention.place(a, source(a), body.filter(|_| shown == Some(a)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_follow_the_settings() {
        let mut prefs = SurfacePrefs::default();
        let t = triggers(&prefs);
        assert!(
            matches!(t[0], (Attendant::Workspaces, Trigger::Hover(h)) if h.grace == Duration::from_millis(280))
        );
        assert!(matches!(t[1], (Attendant::Dashboard, Trigger::Hover(_))));
        assert_eq!(
            t.len(),
            1 + Attendant::POPOUTS.len() + Attendant::PANELS.len()
        );
        assert!(t.contains(&(Attendant::Session, Trigger::click_only())));
        prefs.workspaces_open = PanelOpening::Never;
        prefs.dashboard_on_hover = false;
        let t = triggers(&prefs);
        assert_eq!(t[0], (Attendant::Workspaces, Trigger::Off));
        assert_eq!(t[1], (Attendant::Dashboard, Trigger::Click));
    }

    #[test]
    fn only_popouts_with_content_are_attendants() {
        assert_eq!(
            Attendant::of_popout(BarPopout::Status(StatusEntryKind::Lock)),
            None
        );
        assert_eq!(Attendant::of_popout(BarPopout::ActiveWindow), None);
        assert_eq!(
            Attendant::of_popout(BarPopout::Dashboard),
            Some(Attendant::Dashboard)
        );
        assert_eq!(
            over(true, Some(BarPopout::Dashboard)),
            Some(Attendant::Workspaces)
        );
    }
}
