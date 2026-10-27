//! SURF-03B: the dashboard in the scene.
//!
//! The popout host shows it on hovering the clock and pins it on a click;
//! here it gets its facts and its presses:
//! - the date and time from the runtime's clock observation (with its year);
//! - the system (user, host, kernel) once, its uptime with each read;
//! - the resources every second while it is out, off the UI thread;
//! - the media from the runtime's MPRIS presentation. Its controls go to the
//!   existing MPRIS transport (revalidated against the player there), as
//!   Now Playing's do.
//!
//! Development builds read `/proc` and the hwmon tree from
//! `GNOS_UX_PROC_ROOT` / `GNOS_UX_HWMON_ROOT` when set (the qualification's
//! stand-ins). `GNOS_UX_DASHBOARD` receipts every change.

use super::*;
use crate::dashboard_panel::{
    CPU_HISTORY, CivilDate, DashRow, DashboardModel, MediaControl, MediaView, ProfileView,
    ResourcesView,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

const RESOURCE_POLL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct DashboardState {
    cpu_before: Option<gnos_utilities::CpuTimes>,
    cpu: Option<f32>,
    cpu_history: VecDeque<f32>,
    sample: gnos_utilities::ResourceSample,
    system: Option<gnos_utilities::SystemFacts>,
    polling: bool,
    reported: Option<String>,
    /// The MPRIS session's snapshots, projected into the runtime.
    media: Option<gpui::Subscription>,
}

fn root(var: &str, default: &str) -> PathBuf {
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os(var) {
        return root.into();
    }
    let _ = var;
    default.into()
}

fn clock_text(observation: gnos_clock::ClockObservation) -> String {
    let (h, m) = (observation.time.hour_24(), observation.time.minute());
    match observation.cycle {
        gnos_clock::ClockCycle::TwentyFourHour => format!("{h:02}:{m:02}"),
        gnos_clock::ClockCycle::TwelveHour => {
            let h12 = if h % 12 == 0 { 12 } else { h % 12 };
            format!("{h12}:{m:02} {}", if h < 12 { "am" } else { "pm" })
        }
    }
}

impl SceneHost {
    fn dashboard_model(
        &self,
        presentation: crate::product_mpris::MprisPresentation,
    ) -> DashboardModel {
        let clock = self.product_runtime.clock();
        let date = clock.and_then(|c| {
            let (day, month) = c.time.date()?;
            Some(CivilDate {
                year: c.time.year()?,
                month,
                day,
            })
        });
        let system = self.dashboard_state.system.clone().unwrap_or_default();
        let profile = ProfileView {
            user_host: match (&system.user, &system.host) {
                (Some(u), Some(h)) => Some(format!("{u}@{h}")),
                (Some(u), None) => Some(u.clone()),
                (None, Some(h)) => Some(h.clone()),
                (None, None) => None,
            },
            kernel: system.kernel.as_ref().map(|k| format!("Linux {k}")),
            uptime: system.uptime_secs.map(|s| {
                gnos_i18n::tf(
                    "Up {duration}",
                    &[("duration", &gnos_utilities::format_uptime(s))],
                )
            }),
        };
        let media = presentation.effective().map(|_| {
            let now = crate::product_now_playing::NowPlayingPresentation::derive(&presentation);
            MediaView {
                title: now.title.clone().unwrap_or_default(),
                artists: now.artists.join(", "),
                player: self.player_display_name(&now.player_label),
                playback: now.playback_label,
                progress: now.progress.map(|p| p as f32),
                controls: now
                    .controls
                    .iter()
                    .map(|c| MediaControl {
                        operation: c.operation,
                        glyph: c.glyph,
                        enabled: c.enabled,
                    })
                    .collect(),
            }
        });
        let state = &self.dashboard_state;
        DashboardModel {
            date,
            time: clock.map(clock_text),
            profile,
            media,
            resources: ResourcesView {
                cpu: state.cpu,
                cpu_history: state.cpu_history.iter().copied().collect(),
                memory: state.sample.memory.map(|u| (u.used, u.total)),
                disk: state.sample.disk.map(|u| (u.used, u.total)),
                temperature: state.sample.temperature,
            },
        }
    }

    /// Give the dashboard its facts; start reading the resources while it
    /// is out.
    pub(super) fn sync_dashboard(&mut self, cx: &mut Context<Self>) {
        if !self.bar_popout.dashboard_shown() {
            if self.dashboard.month_offset() != 0 {
                self.dashboard.reset();
            }
            return;
        }
        self.poll_resources(cx);
        self.observe_media(cx);
        let presentation = crate::product_mpris_session::MprisSession::shared(cx)
            .read(cx)
            .presentation();
        let model = self.dashboard_model(presentation);
        if self.dashboard.set_model(model) {
            self.report_dashboard("changed");
            cx.notify();
        }
    }

    /// The product scene observes MPRIS only while a surface needs it: the
    /// dashboard (re)starts the session's observation, which stops by itself
    /// once no player is left, and projects each snapshot into the runtime.
    fn observe_media(&mut self, cx: &mut Context<Self>) {
        let session = crate::product_mpris_session::MprisSession::shared(cx);
        if self.dashboard_state.media.is_none() {
            self.dashboard_state.media = Some(cx.observe(&session, |this, session, cx| {
                let snapshot = session.read(cx).snapshot().clone();
                let players = snapshot.len();
                if this.product_runtime.set_mpris(snapshot) {
                    println!("GNOS_UX_DASHBOARD_MEDIA_OBSERVED players={players}");
                }
                cx.notify();
            }));
        }
        // One observation loop at a time: `observing` is its lock.
        session.update(cx, |this, cx| {
            if this.is_observing() {
                return;
            }
            this.set_observing(true, cx);
            if this.connection().is_none() {
                crate::product_mpris_session::ensure_connection(this, cx);
            } else {
                crate::product_mpris_session::start_observation(&cx.entity(), cx);
            }
        });
    }

    fn poll_resources(&mut self, cx: &mut Context<Self>) {
        if self.dashboard_state.polling {
            return;
        }
        self.dashboard_state.polling = true;
        let read_system = self.dashboard_state.system.is_none();
        cx.spawn(async move |this, cx| {
            let mut first = true;
            loop {
                let proc = root("GNOS_UX_PROC_ROOT", "/proc");
                let hwmon = root("GNOS_UX_HWMON_ROOT", "/sys/class/hwmon");
                let read = cx
                    .background_executor()
                    .spawn(async move {
                        let sample = gnos_utilities::read_resources(&proc, &hwmon, Path::new("/"));
                        let system = gnos_utilities::read_system(&proc);
                        (sample, system)
                    })
                    .await;
                let shown = this
                    .update(cx, |view, cx| {
                        let (sample, system) = read;
                        let state = &mut view.dashboard_state;
                        if let (Some(before), Some(now)) = (state.cpu_before, sample.cpu_times)
                            && let Some(p) = gnos_utilities::cpu_percent(before, now)
                        {
                            state.cpu = Some(p);
                            state.cpu_history.push_back(p);
                            while state.cpu_history.len() > CPU_HISTORY {
                                state.cpu_history.pop_front();
                            }
                        }
                        state.cpu_before = sample.cpu_times;
                        state.sample = sample;
                        if first && read_system || state.system.is_none() {
                            state.system = Some(system);
                        } else if let Some(s) = state.system.as_mut() {
                            s.uptime_secs = system.uptime_secs;
                        }
                        let shown = view.bar_popout.dashboard_shown();
                        if !shown {
                            view.dashboard_state.polling = false;
                        }
                        cx.notify();
                        shown
                    })
                    .unwrap_or(false);
                first = false;
                if !shown {
                    break;
                }
                cx.background_executor().timer(RESOURCE_POLL).await;
            }
        })
        .detach();
    }

    /// A press or release in the dashboard (host coordinates).
    pub(super) fn dashboard_pointer(
        &mut self,
        lx: f32,
        ly: f32,
        down: bool,
        cx: &mut Context<Self>,
    ) {
        let outcome = if down {
            self.dashboard.pointer_down(lx, ly)
        } else {
            self.dashboard.pointer_up(lx, ly)
        };
        for operation in outcome.effects {
            self.dashboard_media(operation, cx);
        }
        if outcome.changed {
            self.report_dashboard("changed");
            self.start_motion_driver(cx);
            cx.notify();
        }
    }

    /// A player that gives no identity is labelled by its bus name
    /// (`org.mpris.MediaPlayer2.firefox`): name it by its application when
    /// the catalogue knows it, and show the bare suffix otherwise.
    fn player_display_name(&self, label: &str) -> String {
        let Some(suffix) = label.strip_prefix("org.mpris.MediaPlayer2.") else {
            return label.to_owned();
        };
        let base = suffix.split('.').next().unwrap_or(suffix);
        let catalog = self.product_runtime.application_catalog();
        let id = crate::reference_bar::resolve_window_entry(catalog, suffix)
            .or_else(|| crate::reference_bar::resolve_window_entry(catalog, base));
        id.and_then(|id| catalog.entries().iter().find(|e| e.id == id))
            .map_or_else(|| base.to_owned(), |e| e.name.clone())
    }

    fn dashboard_media(
        &mut self,
        operation: crate::product_mpris::MprisOperation,
        cx: &mut Context<Self>,
    ) {
        let presentation = crate::product_mpris_session::MprisSession::shared(cx)
            .read(cx)
            .presentation();
        let Some(player) = presentation.effective().map(|p| p.identity.clone()) else {
            println!(
                "GNOS_UX_DASHBOARD_MEDIA operation={} outcome=NoPlayer",
                operation.name()
            );
            return;
        };
        println!(
            "GNOS_UX_DASHBOARD_MEDIA operation={} outcome=Sent",
            operation.name()
        );
        let session = crate::product_mpris_session::MprisSession::shared(cx);
        let source = self
            .runtime_output
            .as_ref()
            .map_or("unknown", RuntimeId::as_str)
            .to_owned();
        spawn_mpris_intent(
            session,
            self.mpris_process_owns.clone(),
            source,
            cx,
            crate::product_mpris_transport::MprisIntent { player, operation },
        );
    }

    /// `GNOS_UX_DASHBOARD`: the dashboard, its page, its facts, its controls.
    pub(super) fn report_dashboard(&mut self, state: &str) {
        let frame = self
            .bar_popout
            .frame()
            .filter(|_| self.bar_popout.dashboard_shown());
        let rect = frame.as_ref().map_or("None".to_owned(), |f| {
            format!(
                "[{:.1},{:.1},{:.1},{:.1}] openness={:.3} hull={}",
                f.rect.left(),
                f.rect.top(),
                f.rect.width(),
                f.rect.height(),
                f.openness,
                f.hull.is_some()
            )
        });
        let model = self.dashboard.model();
        let controls = frame
            .map(|f| {
                self.dashboard
                    .rows()
                    .into_iter()
                    .filter_map(|r| match &r {
                        DashRow::Button { button, .. } => {
                            let (x, y, w, h) = r.rect();
                            (x >= 0.0 && x + w <= f.rect.width() + 0.5).then(|| {
                                format!(
                                    "{}@[{:.1},{:.1},{:.1},{:.1}]",
                                    button.slug(),
                                    f.rect.left() + x,
                                    f.rect.top() + y,
                                    w,
                                    h
                                )
                            })
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .unwrap_or_default();
        let today = model.date.map_or("none".to_owned(), |d| {
            format!("{}-{:02}-{:02}", d.year, d.month, d.day)
        });
        let r = &model.resources;
        let line = format!(
            "GNOS_UX_DASHBOARD state={state} open={} pinned={} tab={} month_offset={} date={today} media={} cpu={} memory={} disk={} temp={} rect={rect} controls=[{controls}]",
            self.bar_popout.dashboard_open(),
            self.attention
                .is_held(crate::attention::Attendant::Dashboard),
            self.dashboard.tab().slug(),
            self.dashboard.month_offset(),
            model
                .media
                .as_ref()
                .map_or("none".to_owned(), |m| m.title.replace(' ', "_")),
            r.cpu.map_or("none".to_owned(), |c| format!("{c:.1}")),
            r.memory
                .map_or("none".to_owned(), |(u, t)| format!("{u}/{t}")),
            r.disk
                .map_or("none".to_owned(), |(u, t)| format!("{u}/{t}")),
            r.temperature
                .map_or("none".to_owned(), |t| format!("{t:.1}")),
        );
        if state != "changed" || self.dashboard_state.reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.dashboard_state.reported = Some(line);
        }
    }
}
