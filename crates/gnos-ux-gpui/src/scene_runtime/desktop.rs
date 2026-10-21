//! SURF-04: the scene's side of the desktop modules.
//!
//! The scene host owns its output's desktop surface through the shared
//! `DesktopModel`: it gives it the clock (from the runtime's observation),
//! later the visualiser's bands, and keeps the places dropped there. When the
//! host is released, the surface is told to go.

use super::*;
use crate::dashboard_panel::{MONTHS, WEEKDAYS, weekday};
use crate::desktop_layer::{DesktopEvent, DesktopModel};
use crate::desktop_modules::{ModuleKind, ModulePlace};
use gpui::Entity;

/// How often the visualiser's bands are drawn, and how long a silence lasts
/// before it draws nothing.
const VISUALISER_FRAME: Duration = Duration::from_millis(33);
const VISUALISER_SILENCE: Duration = Duration::from_millis(1500);
/// Each frame a band falls to this share of its last height (rises at once).
const VISUALISER_FALL: f32 = 0.82;

/// The capture thread's latest levels, for the scene to draw.
type LatestBands = std::sync::Arc<std::sync::Mutex<(u64, Option<Vec<f32>>)>>;

/// How long a dropped place waits for the profile to carry it.
const PROFILE_WAIT: Duration = Duration::from_secs(3);

/// The profile's places, as fractions of the output.
fn profile_places(
    layout: Option<in_gnu_os_settings_read_model::DesktopModuleLayout>,
) -> std::collections::BTreeMap<ModuleKind, ModulePlace> {
    let mut places = std::collections::BTreeMap::new();
    let Some(layout) = layout else {
        return places;
    };
    let fraction = |p: in_gnu_os_settings_read_model::DesktopModulePlace| ModulePlace {
        x: f32::from(p.x.get()) / 1000.0,
        y: f32::from(p.y.get()) / 1000.0,
    };
    if let Some(p) = layout.clock {
        places.insert(ModuleKind::Clock, fraction(p));
    }
    if let Some(p) = layout.visualiser {
        places.insert(ModuleKind::Visualiser, fraction(p));
    }
    places
}

/// DICTATION-ANCHOR: the profile's anchor for the dictation element.
///
/// The element hangs from an anchor, not from a place: a place is a position,
/// and a position ignores the movement.
///
/// `None` means the element is in the Deck's slot, which is where it lives
/// unless it is detached onto the desktop.
/// A place, written as the anchor it stands for.
///
/// The drag works in fractions — the module's own machinery — and the anchor
/// is what the profile keeps: the nearest edge, the distance from it, and
/// where along it.
// Written to the profile, so only in a build with the profile client.
#[cfg(all(
    feature = "component-lab",
    feature = "indicator-production-client",
    feature = "r12-production-client"
))]
fn place_as_anchor(
    place: ModulePlace,
    viewport: (f32, f32),
    inset: [f32; 4],
) -> in_gnu_os_settings_read_model::DesktopModuleAnchor {
    use in_gnu_os_settings_read_model::{
        DesktopAnchorEdge, DesktopAnchorMode, DesktopAnchorOffset, DesktopFraction,
    };
    // The place is a fraction of the free area (the desktop modules' own
    // frame), and the anchor is read back in it.
    let [top, right, bottom, left] = inset;
    let free = (
        (viewport.0 - left - right).max(0.0),
        (viewport.1 - top - bottom).max(0.0),
    );
    let anchor = crate::product_anchor::DictationAnchor::default_anchor().moved_to(
        (
            place.x.clamp(0.0, 1.0) * free.0,
            place.y.clamp(0.0, 1.0) * free.1,
        ),
        free,
    );
    let mode = match anchor.mode {
        crate::product_anchor::AnchorMode::Quadrant => DesktopAnchorMode::Quadrant,
        crate::product_anchor::AnchorMode::Snap => DesktopAnchorMode::Snap,
        crate::product_anchor::AnchorMode::Border => DesktopAnchorMode::Border,
    };
    let edge = match anchor.edge {
        crate::product_anchor::AnchorEdge::Right => DesktopAnchorEdge::Right,
        crate::product_anchor::AnchorEdge::Top => DesktopAnchorEdge::Top,
        crate::product_anchor::AnchorEdge::Bottom => DesktopAnchorEdge::Bottom,
        crate::product_anchor::AnchorEdge::Left => DesktopAnchorEdge::Left,
    };
    let offset = DesktopAnchorOffset::try_from(anchor.offset.round().clamp(0.0, 4096.0) as i64)
        .unwrap_or(DesktopAnchorOffset::try_from(48).unwrap_or_else(|_| {
            DesktopAnchorOffset::try_from(0).expect("a zero offset is in range")
        }));
    let along = DesktopFraction::try_from(i64::from(anchor.along)).unwrap_or_else(|_| {
        DesktopFraction::try_from(120).expect("a twentieth of the output is in range")
    });
    in_gnu_os_settings_read_model::DesktopModuleAnchor {
        mode,
        edge,
        offset,
        along,
    }
}

#[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
pub(super) fn profile_dictation_anchor(
    layout: Option<in_gnu_os_settings_read_model::DesktopModuleLayout>,
) -> Option<crate::product_anchor::DictationAnchor> {
    let anchor = layout?.dictation?;
    use in_gnu_os_settings_read_model::{DesktopAnchorEdge, DesktopAnchorMode};
    let mode = match anchor.mode {
        DesktopAnchorMode::Dock => return None,
        DesktopAnchorMode::Border => crate::product_anchor::AnchorMode::Border,
        DesktopAnchorMode::Quadrant => crate::product_anchor::AnchorMode::Quadrant,
        DesktopAnchorMode::Snap => crate::product_anchor::AnchorMode::Snap,
    };
    let edge = match anchor.edge {
        DesktopAnchorEdge::Left => crate::product_anchor::AnchorEdge::Left,
        DesktopAnchorEdge::Right => crate::product_anchor::AnchorEdge::Right,
        DesktopAnchorEdge::Top => crate::product_anchor::AnchorEdge::Top,
        DesktopAnchorEdge::Bottom => crate::product_anchor::AnchorEdge::Bottom,
    };
    Some(crate::product_anchor::DictationAnchor {
        mode,
        edge,
        offset: f32::from(anchor.offset.get()),
        along: anchor.along.get(),
    })
}

#[derive(Default)]
pub(super) struct DesktopState {
    pub(super) model: Option<Entity<DesktopModel>>,
    subscriptions: Vec<gpui::Subscription>,
    visualiser: Option<gnos_audio::VisualiserHandle>,
    /// WALL-01: the preferences as last read (and their file's time), the
    /// wallpaper asked for (picture, fit, size), and whether it is being
    /// prepared.
    wallpaper_prefs: Option<(
        crate::wallpaper::WallpaperPrefs,
        Option<std::time::SystemTime>,
    )>,
    wallpaper_key: Option<(
        Option<std::path::PathBuf>,
        crate::wallpaper::WallpaperFit,
        (u32, u32),
    )>,
    wallpaper_busy: bool,
    /// SET-02A: the visualiser was stopped because the operator turned it
    /// off (so turning it on starts it again).
    visualiser_off: bool,
    /// SET-02: the once-a-second sync for seconds on the clock is running.
    second_ticking: bool,
    /// WALL-01: what the shell reserves along each edge (top, right, bottom,
    /// left), for the desktop surface that spans the whole output.
    pub(super) inset: [f32; 4],
}

impl SceneHost {
    /// Take the desktop surface opened beside this host's scene window.
    pub(super) fn attach_desktop(&mut self, model: Entity<DesktopModel>, cx: &mut Context<Self>) {
        let placed = cx.subscribe(&model, |this, _, event: &DesktopEvent, cx| match event {
            DesktopEvent::Placed(kind, place) => this.desktop_placed(*kind, *place, cx),
        });
        let release = cx.on_release({
            let model = model.clone();
            move |_, cx| {
                model.update(cx, |m, cx| {
                    m.closed = true;
                    cx.notify();
                });
            }
        });
        self.desktop_state.subscriptions = vec![placed, release];
        self.desktop_state.model = Some(model);
        self.start_visualiser(cx);
        // WALL-01: every 2 s, the wallpaper follows a change made from
        // another output's Settings, and the first size the surface gets.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let alive = this
                    .update(cx, |view, cx| {
                        let Some(model) = view.desktop_state.model.clone() else {
                            return false;
                        };
                        view.sync_wallpaper(&model, cx);
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// SURF-04B: read the default sink's monitor and draw its bands.
    ///
    /// Development builds capture only when asked to
    /// (`GNOS_UX_VISUALISER_CAPTURE=1`: the SURF-04 harness on its private
    /// server, or a deployed session): a qualification that hands the scene
    /// the host's PipeWire for its audio icons must never open a capture on
    /// the operator's output.
    fn start_visualiser(&mut self, cx: &mut Context<Self>) {
        if cfg!(debug_assertions)
            && std::env::var("GNOS_UX_VISUALISER_CAPTURE").as_deref() != Ok("1")
        {
            println!(
                "GNOS_UX_DESKTOP_VISUALISER state=withheld note=development_build_without_capture_opt_in"
            );
            return;
        }
        if !crate::surface_prefs::current().desktop_visualiser {
            self.desktop_state.visualiser_off = true;
            println!("GNOS_UX_DESKTOP_VISUALISER state=off reason=turned_off");
            return;
        }
        let latest: LatestBands = Default::default();
        let feed = latest.clone();
        match gnos_audio::spawn_visualiser(move |bands| {
            if let Ok(mut slot) = feed.lock() {
                slot.0 += 1;
                slot.1 = bands;
            }
        }) {
            Ok(handle) => {
                println!("GNOS_UX_DESKTOP_VISUALISER state=capturing");
                self.desktop_state.visualiser = Some(handle);
            }
            Err(error) => {
                println!("GNOS_UX_DESKTOP_VISUALISER state=unavailable error={error}");
                return;
            }
        }
        cx.spawn(async move |this, cx| {
            let mut seen = 0u64;
            let mut shown: Vec<f32> = Vec::new();
            let mut sound_at: Option<Instant> = None;
            loop {
                cx.background_executor().timer(VISUALISER_FRAME).await;
                let (serial, bands) = latest
                    .lock()
                    .map(|slot| (slot.0, slot.1.clone()))
                    .unwrap_or((seen, None));
                let fresh = serial != seen;
                seen = serial;
                let now = Instant::now();
                match bands.filter(|_| fresh) {
                    Some(levels) => {
                        sound_at = Some(now);
                        shown.resize(levels.len(), 0.0);
                        for (s, l) in shown.iter_mut().zip(levels) {
                            *s = l.max(*s * VISUALISER_FALL);
                        }
                    }
                    None => {
                        for s in &mut shown {
                            *s *= VISUALISER_FALL;
                        }
                        if sound_at.is_none_or(|t| now.duration_since(t) > VISUALISER_SILENCE) {
                            shown.clear();
                        }
                    }
                }
                let alive = this
                    .update(cx, |view, cx| {
                        let Some(model) = view.desktop_state.model.clone() else {
                            return false;
                        };
                        // SET-02A: turned off, the capture stops (no audio is
                        // read at all) and the bars go.
                        if !crate::surface_prefs::current().desktop_visualiser {
                            view.desktop_state.visualiser = None;
                            view.desktop_state.visualiser_off = true;
                            println!("GNOS_UX_DESKTOP_VISUALISER state=stopped reason=turned_off");
                            model.update(cx, |m, cx| {
                                m.bands.clear();
                                cx.notify();
                            });
                            return false;
                        }
                        model.update(cx, |m, cx| {
                            if m.bands != shown {
                                m.bands = shown.clone();
                                cx.notify();
                            }
                        });
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    /// WALL-01: the preferences, read again when their file changed.
    pub(super) fn wallpaper_prefs(&mut self) -> crate::wallpaper::WallpaperPrefs {
        let modified = crate::wallpaper::prefs_path()
            .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        match &self.desktop_state.wallpaper_prefs {
            Some((prefs, at)) if *at == modified => prefs.clone(),
            _ => {
                let prefs = crate::wallpaper::load();
                self.desktop_state.wallpaper_prefs = Some((prefs.clone(), modified));
                prefs
            }
        }
    }

    /// WALL-01: show the wallpaper the preferences ask for, prepared at
    /// this output's size off the UI thread; none when Gnosix does not draw
    /// one.
    fn sync_wallpaper(&mut self, model: &Entity<DesktopModel>, cx: &mut Context<Self>) {
        let prefs = self.wallpaper_prefs();
        let size = model.read(cx).device_size;
        let key = prefs
            .enabled
            .then(|| (prefs.picture(), prefs.fit, size))
            .filter(|(_, _, s)| s.0 > 0 && s.1 > 0);
        if key == self.desktop_state.wallpaper_key || self.desktop_state.wallpaper_busy {
            return;
        }
        self.desktop_state.wallpaper_key = key.clone();
        let Some((Some(picture), fit, (w, h))) = key else {
            model.update(cx, |m, cx| {
                if m.wallpaper.take().is_some() {
                    cx.notify();
                }
            });
            return;
        };
        self.desktop_state.wallpaper_busy = true;
        let model = model.clone();
        cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { crate::wallpaper::prepare(&picture, fit, w, h) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.desktop_state.wallpaper_busy = false;
                println!(
                    "GNOS_UX_WALLPAPER size={w}x{h} fit={fit:?} outcome={}",
                    prepared
                        .as_ref()
                        .map_or_else(|e| e.clone(), |_| "ready".to_owned())
                );
                if let Ok(path) = prepared {
                    model.update(cx, |m, cx| {
                        m.wallpaper = Some(path);
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    /// The desktop surface spans the whole output; its modules keep to what
    /// the shell's reservation leaves free.
    pub(super) fn sync_desktop_inset(&mut self, cx: &mut Context<Self>) {
        let Some(model) = self.desktop_state.model.clone() else {
            return;
        };
        let inset = self.desktop_state.inset;
        model.update(cx, |m, cx| {
            if m.inset != inset {
                m.inset = inset;
                println!(
                    "GNOS_UX_DESKTOP inset=[{:.0},{:.0},{:.0},{:.0}]",
                    inset[0], inset[1], inset[2], inset[3]
                );
                cx.notify();
            }
        });
    }

    /// Give the desktop the clock as observed.
    pub(super) fn sync_desktop(&mut self, cx: &mut Context<Self>) {
        let Some(model) = self.desktop_state.model.clone() else {
            return;
        };
        self.sync_wallpaper(&model, cx);
        // SET-02A: the clock shown or not; the visualiser started again when
        // the operator turns it back on.
        let prefs = crate::surface_prefs::current();
        model.update(cx, |m, cx| {
            if m.clock_hidden == prefs.desktop_clock {
                m.clock_hidden = !prefs.desktop_clock;
                println!("GNOS_UX_DESKTOP clock_shown={}", prefs.desktop_clock);
                cx.notify();
            }
        });
        // DICTATION-01: what the published state says, if anything says.
        #[cfg(feature = "indicator-production-client")]
        {
            let view = crate::product_indicator::registry()
                .active_publisher()
                .map(|snapshot| crate::product_indicator::desktop_view_from_snapshot(&snapshot));
            model.update(cx, |m, cx| {
                if m.dictation != view {
                    m.dictation = view;
                    cx.notify();
                }
            });
        }
        if prefs.desktop_visualiser && self.desktop_state.visualiser_off {
            self.desktop_state.visualiser_off = false;
            self.start_visualiser(cx);
        }
        // SURF-04C: the places the profile holds.
        let layout = self
            .product_runtime
            .preferences()
            .bar_overrides
            .desktop_modules;
        let places = profile_places(layout);
        let clock = self.product_runtime.clock();
        let seconds = prefs.clock_seconds.then(gnos_clock::local_second);
        let time = clock.map(|c| {
            let (h, m) = (c.time.hour_24(), c.time.minute());
            let sec = seconds.map_or(String::new(), |s| format!(":{s:02}"));
            match c.cycle {
                gnos_clock::ClockCycle::TwentyFourHour => format!("{h:02}:{m:02}{sec}"),
                gnos_clock::ClockCycle::TwelveHour => {
                    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
                    format!("{h12}:{m:02}{sec} {}", if h < 12 { "am" } else { "pm" })
                }
            }
        });
        if seconds.is_some() {
            self.schedule_desktop_second(cx);
        }
        let date = clock.and_then(|c| {
            let (day, month) = c.time.date()?;
            let year = c.time.year()?;
            Some(format!(
                "{} {day} {}",
                gnos_i18n::t(WEEKDAYS[usize::from(weekday(year, month, day))]),
                gnos_i18n::t(MONTHS[usize::from(month - 1)])
            ))
        });
        model.update(cx, |m, cx| {
            let moved = m.modules.set_places(places);
            if m.time != time || m.date != date || moved {
                m.time = time;
                m.date = date;
                cx.notify();
            }
        });
    }

    /// While seconds are on, the desktop is synced every second; the loop
    /// ends by itself the moment they are turned off.
    fn schedule_desktop_second(&mut self, cx: &mut Context<Self>) {
        if self.desktop_state.second_ticking {
            return;
        }
        self.desktop_state.second_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                let go = this
                    .update(cx, |view, cx| {
                        if crate::surface_prefs::current().clock_seconds {
                            view.sync_desktop(cx);
                            true
                        } else {
                            view.desktop_state.second_ticking = false;
                            false
                        }
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
        })
        .detach();
    }

    /// SURF-04C: a module dropped at a new place is written to the R12
    /// profile; shown there until the profile reflects it, or for at most
    /// `PROFILE_WAIT`, after which it goes back.
    /// Whether a point lands on the dictation patch.
    ///
    /// The element's wrapper fills the surface, so the pointer lands on it
    /// wherever it is. Only the patch itself is the button.
    #[cfg(all(feature = "component-lab", feature = "indicator-production-client"))]
    pub(super) fn dictation_hit(&self, x: f32, y: f32) -> bool {
        let Some((left, top, width, height)) = self.dictation_reported else {
            return false;
        };
        x >= left && x <= left + width && y >= top && y <= top + height
    }

    /// DICTATION-01: the scene drives the dictation module's drag.
    ///
    /// The module is a desktop module — it has a place, it is dragged, its
    /// place is written to the profile. But the scene is what paints it, so
    /// the scene is what carries the pointer to it.
    pub(super) fn dictation_pointer(&mut self, x: f32, y: f32, phase: u8, cx: &mut Context<Self>) {
        println!("GNOS_UX_DICTATION_POINTER phase={phase} at=({x:.1},{y:.1})");
        let Some(model) = self.desktop_state.model.clone() else {
            return;
        };
        let mut placed = None;
        let mut changed = false;
        model.update(cx, |model, _cx| {
            let (x, y) = (x - model.inset[3], y - model.inset[0]);
            let out = match phase {
                0 => model.modules.pointer_down(x, y),
                1 => model.modules.pointer_moved(x, y),
                _ => model.modules.pointer_up(x, y),
            };
            placed = out.placed;
            changed = out.changed;
        });
        if let Some((kind, place)) = placed {
            self.desktop_placed(kind, place, cx);
        }
        if changed {
            cx.notify();
        }
    }

    fn desktop_placed(&mut self, kind: ModuleKind, place: ModulePlace, cx: &mut Context<Self>) {
        let outcome = self.write_desktop_place(kind, place, cx);
        println!(
            "GNOS_UX_DESKTOP_PLACE module={} place=[{:.4},{:.4}] outcome={outcome}",
            kind.slug(),
            place.x,
            place.y
        );
        if outcome == "SENT" {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(PROFILE_WAIT).await;
                let _ = this.update(cx, |view, cx| {
                    let held = profile_places(
                        view.product_runtime
                            .preferences()
                            .bar_overrides
                            .desktop_modules,
                    );
                    let landed = held.get(&kind).is_some_and(|p| {
                        (p.x - place.x).abs() < 0.002 && (p.y - place.y).abs() < 0.002
                    });
                    if !landed && let Some(model) = view.desktop_state.model.clone() {
                        model.update(cx, |m, cx| {
                            if m.modules.revert_drop(kind) {
                                println!(
                                    "GNOS_UX_DESKTOP_PLACE module={} outcome=REVERTED",
                                    kind.slug()
                                );
                                cx.notify();
                            }
                        });
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// DICTATION-ANCHOR: a secondary click on the microphone pins it to the
    /// Deck, or detaches it onto the desktop.
    pub(super) fn dictation_secondary(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        // The element is painted by the scene, wherever it is: in the Deck's
        // slot, or detached on the desktop.
        let Some((left, top, width, height)) = self.dictation_reported else {
            return false;
        };
        if !(x >= left && x <= left + width && y >= top && y <= top + height) {
            return false;
        }
        let detached = self
            .product_runtime
            .preferences()
            .bar_overrides
            .desktop_modules
            .as_ref()
            .and_then(|layout| layout.dictation)
            .is_some_and(|anchor| {
                !matches!(
                    anchor.mode,
                    in_gnu_os_settings_read_model::DesktopAnchorMode::Dock
                )
            });
        let outcome = self.write_dictation_anchor_mode(detached, cx);
        println!("GNOS_UX_DICTATION_ANCHOR detached={detached} outcome={outcome}");
        true
    }

    /// DICTATION-ANCHOR: move the microphone between the Deck's slot and
    /// the desktop, keeping the rest of the anchor.
    pub(super) fn write_dictation_anchor_mode(
        &mut self,
        detached: bool,
        cx: &mut Context<Self>,
    ) -> &'static str {
        #[cfg(feature = "r12-production-client")]
        {
            use in_gnu_os_settings_read_model::{
                DesktopAnchorEdge, DesktopAnchorMode, DesktopAnchorOffset, DesktopFraction,
                DesktopModuleAnchor,
            };
            let mut overrides = self.product_runtime.preferences().bar_overrides.clone();
            let mut layout = overrides.desktop_modules.unwrap_or_default();
            let anchor = layout.dictation.unwrap_or(DesktopModuleAnchor {
                mode: DesktopAnchorMode::Dock,
                edge: DesktopAnchorEdge::Left,
                offset: DesktopAnchorOffset::try_from(48)
                    .unwrap_or_else(|_| DesktopAnchorOffset::try_from(0).expect("zero")),
                along: DesktopFraction::try_from(120).expect("a twentieth"),
            });
            layout.dictation = Some(DesktopModuleAnchor {
                mode: if detached {
                    DesktopAnchorMode::Border
                } else {
                    DesktopAnchorMode::Dock
                },
                ..anchor
            });
            overrides.desktop_modules = Some(layout);
            let session = crate::product_profile::ProfileSession::shared(cx);
            session.update(cx, |session, cx| session.select_preferences(overrides, cx));
            "SENT"
        }
        #[cfg(not(feature = "r12-production-client"))]
        {
            let _ = (detached, cx);
            "NO_CLIENT"
        }
    }

    fn write_desktop_place(
        &mut self,
        kind: ModuleKind,
        place: ModulePlace,
        cx: &mut Context<Self>,
    ) -> &'static str {
        #[cfg(feature = "r12-production-client")]
        {
            use in_gnu_os_settings_read_model::{DesktopFraction, DesktopModulePlace};
            let permille =
                |v: f32| DesktopFraction::try_from((v.clamp(0.0, 1.0) * 1000.0).round() as i64);
            let (Ok(x), Ok(y)) = (permille(place.x), permille(place.y)) else {
                return "INVALID";
            };
            let mut overrides = self.product_runtime.preferences().bar_overrides.clone();
            let mut layout = overrides.desktop_modules.unwrap_or_default();
            let at = Some(DesktopModulePlace { x, y });
            match kind {
                ModuleKind::Clock => layout.clock = at,
                ModuleKind::Visualiser => layout.visualiser = at,
                ModuleKind::Dictation => {
                    // DICTATION-ANCHOR: the mic patch is written as an
                    // anchor. The drag works in the module's own fractions;
                    // the anchor is what the profile keeps — the nearest
                    // edge, the distance from it, and where along it.
                    let viewport = self.previous_viewport.map_or((1920.0, 1080.0), |viewport| {
                        (viewport.width, viewport.height)
                    });
                    layout.dictation =
                        Some(place_as_anchor(place, viewport, self.desktop_state.inset));
                }
            }
            overrides.desktop_modules = Some(layout);
            let session = crate::product_profile::ProfileSession::shared(cx);
            session.update(cx, |session, cx| session.select_preferences(overrides, cx));
            "SENT"
        }
        #[cfg(not(feature = "r12-production-client"))]
        {
            let _ = (kind, place, cx);
            "NO_CLIENT"
        }
    }
}
