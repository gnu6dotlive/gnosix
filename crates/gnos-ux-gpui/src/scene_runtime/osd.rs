//! SURF-01C: the OSD from the Bar in the scene.
//!
//! The OSD (`bar_osd`) presents observed changes only:
//! - output volume and microphone: each accepted audio observation (the
//!   same one AUDIO-01's OSD consumed), whoever made the change;
//! - brightness: a backlight's level, read from sysfs every second (cheap,
//!   no bus traffic), and a display's level when Gnosix reads it again
//!   after a request. DDC/CI displays are not polled: every read is I2C
//!   traffic of about a second, so another program's change to a display
//!   is not observed (a known gap).
//!
//! It gives way to an open Utilities panel and retracts when the Bar
//! travels. `GNOS_UX_BAR_OSD` receipts every presentation.

use super::*;
use crate::bar_osd::{OsdKind, OsdReading};
use std::collections::BTreeMap;

const BACKLIGHT_POLL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct OsdState {
    output: Option<OsdReading>,
    input: Option<OsdReading>,
    /// The last observed percentage of each brightness target, by label.
    brightness: BTreeMap<String, u8>,
    watching_backlights: bool,
}

/// The output and input levels an audio observation presents.
fn audio_readings(snapshot: &gnos_audio::AudioSnapshot) -> [Option<OsdReading>; 2] {
    let model = crate::bar_audio_popout::AudioPopoutModel::from_snapshot(snapshot);
    let reading = |kind,
                   control: Option<crate::bar_audio_popout::EndpointControl>,
                   rows: &[crate::bar_audio_popout::DeviceRow]| {
        control.map(|c| OsdReading {
            kind,
            label: rows
                .iter()
                .find(|r| r.id == c.target.id)
                .map(|r| r.label.clone())
                .unwrap_or_default(),
            percent: c.level.map(|l| (l * 100.0).round().clamp(0.0, 255.0) as u8),
            muted: c.muted,
        })
    };
    [
        reading(OsdKind::Output, model.output, &model.outputs),
        reading(OsdKind::Input, model.input, &model.inputs),
    ]
}

impl SceneHost {
    /// An accepted audio observation: present what changed.
    pub(super) fn observe_osd_audio(&mut self, cx: &mut Context<Self>) {
        let [output, input] = audio_readings(self.product_runtime.audio());
        let changed = [
            crate::bar_osd::changed(self.osd_state.output.as_ref(), output.as_ref()),
            crate::bar_osd::changed(self.osd_state.input.as_ref(), input.as_ref()),
        ];
        self.osd_state.output = output;
        self.osd_state.input = input;
        for reading in changed.into_iter().flatten() {
            self.show_osd(reading, cx);
        }
    }

    /// A brightness level observed for `label`.
    pub(super) fn observe_osd_brightness(
        &mut self,
        label: String,
        percent: u8,
        cx: &mut Context<Self>,
    ) {
        let previous = self.osd_state.brightness.insert(label.clone(), percent);
        if previous.is_some_and(|p| p != percent) {
            self.show_osd(
                OsdReading {
                    kind: OsdKind::Brightness,
                    label,
                    percent: Some(percent),
                    muted: false,
                },
                cx,
            );
        }
    }

    fn show_osd(&mut self, reading: OsdReading, cx: &mut Context<Self>) {
        if !self.review_native_rail {
            return;
        }
        let why = if !crate::surface_prefs::current().osd_shown {
            Some("turned_off")
        } else if self.utilities.is_open() {
            Some("utilities")
        } else if self.bar_travelling {
            Some("travel")
        } else {
            None
        };
        println!(
            "GNOS_UX_BAR_OSD_EVENT kind={} percent={} muted={} presented={}",
            reading.kind.slug(),
            reading.percent.map_or("none".to_owned(), |p| p.to_string()),
            reading.muted,
            why.map_or("true".to_owned(), |w| format!("false reason={w}"))
        );
        if why.is_some() {
            return;
        }
        self.bar_osd.show(reading, Instant::now());
        self.report_osd("shown");
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// `GNOS_UX_BAR_OSD`: the OSD's state and where it stands.
    pub(super) fn report_osd(&self, state: &str) {
        let reading = self.bar_osd.reading();
        let rect = self.bar_osd.frame().map_or("None".to_owned(), |f| {
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
        println!(
            "GNOS_UX_BAR_OSD state={state} open={} hovered={} kind={} percent={} muted={} rect={rect}",
            self.bar_osd.is_open(),
            self.bar_osd.hovered(),
            reading.map_or("none", |r| r.kind.slug()),
            reading
                .and_then(|r| r.percent)
                .map_or("none".to_owned(), |p| p.to_string()),
            reading.is_some_and(|r| r.muted),
        );
    }

    /// Place the OSD beside the status icons, as Utilities.
    pub(super) fn sync_osd(&mut self, side: crate::rail_topology::RailSide, viewport: (f32, f32)) {
        let anchor = self.status_icons_anchor(side);
        self.bar_osd.place(side, viewport, anchor);
    }

    /// Read the backlights every second, off the UI thread; a change is an
    /// observed brightness change (and refreshes Utilities' sliders).
    pub(super) fn watch_backlights(&mut self, cx: &mut Context<Self>) {
        if self.osd_state.watching_backlights || !self.review_native_rail {
            return;
        }
        self.osd_state.watching_backlights = true;
        cx.spawn(async move |this, cx| {
            loop {
                let root = utilities::backlight_root();
                let found = cx
                    .background_executor()
                    .spawn(async move { gnos_utilities::read_backlights(&root) })
                    .await;
                let alive = this
                    .update(cx, |view, cx| {
                        for backlight in found {
                            if view.utilities_observed_backlight(&backlight) {
                                cx.notify();
                            }
                            view.observe_osd_brightness(
                                backlight.name.clone(),
                                backlight.level.percent(),
                                cx,
                            );
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
                cx.background_executor().timer(BACKLIGHT_POLL).await;
            }
        })
        .detach();
    }
}
