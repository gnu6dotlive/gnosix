//! SURF-01B: the Utilities panel in the scene.
//!
//! The panel (`utilities_panel`) is pure; here it gets the observed state
//! and its effects become the typed powers of SURF-01A:
//! - Wi-Fi and Bluetooth: `gnos-network` / `gnos-bluetooth` requests on the
//!   system bus, revalidated against the snapshot the request was made from.
//! - Microphone: AUDIO-01's `ToggleInputMute`, through the scene's audio
//!   dispatch.
//! - Do not disturb: the notification service's command (UXE-02C).
//! - Keep awake: a logind idle inhibitor the scene holds; dropping it
//!   releases the lock.
//! - Brightness: DDC/CI displays and backlights (`gnos-utilities`), read when
//!   the panel opens, set on a slider's release, read again after.
//!
//! Every request is receipted (`GNOS_UX_UTILITIES_REQUEST`); a toggle stays
//! "pending" until the observed state changes or the request is refused, or
//! for at most `PENDING`.

use super::*;
use crate::utilities_panel::{
    BrightnessRow, ToggleKind, ToggleState, UtilitiesEffect, UtilitiesModel, UtilitiesOutcome,
};
use std::collections::BTreeMap;

const PENDING: Duration = Duration::from_secs(5);
/// SURF-01E: how often the brightness is read again while Utilities is open.
const REFRESH_EVERY: Duration = Duration::from_secs(4);

/// One brightness target.
#[derive(Clone, Debug)]
pub(super) enum BrightnessTarget {
    Display(gnos_utilities::DisplayRef),
    Backlight(gnos_utilities::Backlight),
}

#[derive(Clone, Debug)]
pub(super) struct BrightnessEntry {
    target: BrightnessTarget,
    level: Option<gnos_utilities::BrightnessLevel>,
    pending: bool,
}

impl BrightnessEntry {
    /// SURF-01E: what identifies the target between two reads.
    fn key(&self) -> String {
        match &self.target {
            BrightnessTarget::Display(d) => format!("bus{}", d.bus),
            BrightnessTarget::Backlight(b) => b.name.clone(),
        }
    }

    fn label(&self) -> String {
        match &self.target {
            BrightnessTarget::Display(d) => d.model.clone(),
            BrightnessTarget::Backlight(b) => b.name.clone(),
        }
    }
}

pub(super) struct WifiPrompt {
    pub(super) network: gnos_network::WifiNetwork,
    pub(super) text: String,
}

/// The scene's Utilities state beside the pure panel.
#[derive(Default)]
pub(super) struct UtilitiesState {
    pub(super) pending: BTreeMap<ToggleKind, (bool, Instant)>,
    pub(super) notes: BTreeMap<ToggleKind, String>,
    pub(super) inhibitor: Option<gnos_utilities::IdleInhibitor>,
    pub(super) brightness: Vec<BrightnessEntry>,
    pub(super) detecting: bool,
    pub(super) detected_at: Option<Instant>,
    /// SURF-01E: the known targets' levels being read again, and whether a
    /// read is scheduled while Utilities stays open.
    pub(super) refreshing: bool,
    pub(super) refresh_scheduled: bool,
    /// SETTINGS-02C: the Wi-Fi networks in range, and when they were read.
    pub(super) wifi: Vec<gnos_network::WifiNetwork>,
    pub(super) wifi_reading: bool,
    pub(super) wifi_read_at: Option<Instant>,
    /// LOCK-01: the lock screen's settings, read when their page opens and
    /// after each change.
    pub(super) lock: Option<gnos_lock::LockConfig>,
    /// DISPLAY-01: the outputs as last read; a change being applied; and,
    /// until it is kept, the outputs as they were and when they go back.
    pub(super) displays: Vec<gnos_utilities::DisplayOutput>,
    pub(super) display_busy: bool,
    pub(super) display_revert: Option<(Vec<gnos_utilities::DisplaySpec>, Instant)>,
    /// SETTINGS-03B: the notification preferences, and whether the running
    /// service has been given them.
    pub(super) notification_prefs: Option<gnos_notifications::prefs::NotificationPrefs>,
    pub(super) notification_prefs_sent: bool,
    /// SETTINGS-02C: a passphrase being typed (never logged, never kept
    /// after it is sent).
    pub(super) prompt: Option<WifiPrompt>,
    pub(super) reported: Option<String>,
}

/// Run a future on a private current-thread runtime (zbus here runs on
/// tokio), off the UI thread.
fn block_on<F: std::future::Future>(future: F) -> Option<F::Output> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .map(|rt| rt.block_on(future))
}

pub(super) fn backlight_root() -> std::path::PathBuf {
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os("GNOS_UX_BACKLIGHT_ROOT") {
        return root.into();
    }
    "/sys/class/backlight".into()
}

impl SceneHost {
    /// The toggles as observed now.
    fn utilities_toggles(&self) -> Vec<ToggleState> {
        let network = self.product_runtime.network();
        let bluetooth = self.product_runtime.bluetooth();
        let audio =
            crate::bar_audio_popout::AudioPopoutModel::from_snapshot(self.product_runtime.audio());
        let prefs = crate::surface_prefs::current();
        ToggleKind::ALL
            .iter()
            .filter(|kind| prefs.tile_shown(kind.slug()))
            .map(|&kind| {
                let on = match kind {
                    ToggleKind::Wifi => network.globals.as_ref().map(|g| g.wireless_enabled),
                    ToggleKind::Bluetooth => bluetooth.adapters.values().next().map(|a| a.powered),
                    ToggleKind::Microphone => audio.input.map(|i| !i.muted),
                    ToggleKind::DoNotDisturb => Some(self.notification_centre.do_not_disturb()),
                    ToggleKind::KeepAwake => Some(self.utilities_state.inhibitor.is_some()),
                };
                ToggleState {
                    kind,
                    on,
                    pending: self.utilities_state.pending.contains_key(&kind),
                    note: self.utilities_state.notes.get(&kind).cloned(),
                }
            })
            .collect()
    }

    /// The centre of the status icons along the Bar at rest on `side`.
    pub(super) fn status_icons_anchor(&self, side: crate::rail_topology::RailSide) -> Option<f32> {
        self.module_anchor(side, crate::reference_bar::IdleBarEntry::StatusIcons)
    }

    /// SURF-01C: a backlight read by the OSD's watch; Utilities' slider
    /// follows it (unless a request of its own is out). Returns whether it
    /// changed.
    pub(super) fn utilities_observed_backlight(
        &mut self,
        backlight: &gnos_utilities::Backlight,
    ) -> bool {
        let mut changed = false;
        for entry in &mut self.utilities_state.brightness {
            if let BrightnessTarget::Backlight(b) = &entry.target
                && b.name == backlight.name
                && !entry.pending
                && entry.level != Some(backlight.level)
            {
                entry.level = Some(backlight.level);
                changed = true;
            }
        }
        changed
    }

    /// Place the panel beside the status icons and give it the observed
    /// state; settle pending toggles whose state arrived.
    pub(super) fn sync_utilities(
        &mut self,
        side: crate::rail_topology::RailSide,
        viewport: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let anchor = self.status_icons_anchor(side);
        let mut changed = self.utilities.place(side, viewport, anchor);
        let toggles = self.utilities_toggles();
        // A pending toggle settles when its state is observed, or times out.
        let mut settled = Vec::new();
        for (kind, (wanted, since)) in &self.utilities_state.pending {
            let observed = toggles.iter().find(|t| t.kind == *kind).and_then(|t| t.on);
            if observed == Some(*wanted) {
                settled.push((*kind, None));
            } else if since.elapsed() > PENDING {
                settled.push((*kind, Some("No change".to_owned())));
            }
        }
        for (kind, note) in settled {
            self.utilities_state.pending.remove(&kind);
            if let Some(note) = note {
                self.utilities_state.notes.insert(kind, note);
            }
            println!(
                "GNOS_UX_UTILITIES_REQUEST power={} state=settled note={}",
                kind.slug(),
                self.utilities_state
                    .notes
                    .get(&kind)
                    .map_or("-", |n| n.as_str())
            );
        }
        let model = UtilitiesModel {
            toggles: self.utilities_toggles(),
            brightness: self
                .utilities_state
                .brightness
                .iter()
                .map(|e| BrightnessRow {
                    label: e.label(),
                    percent: e.level.map(|l| l.percent()),
                    pending: e.pending,
                })
                .collect(),
            detecting: self.utilities_state.detecting,
        };
        changed |= self.utilities.set_model(model);
        if changed {
            self.report_utilities("changed");
            // New rows: the panel's height follows them.
            if self.utilities.needs_frame() {
                self.start_motion_driver(cx);
            }
            cx.notify();
        }
    }

    /// `GNOS_UX_UTILITIES`: the panel, its toggles and its sliders.
    pub(super) fn report_utilities(&mut self, state: &str) {
        let model = self.utilities.model();
        let toggles = model
            .toggles
            .iter()
            .map(|t| {
                format!(
                    "{{{} on={} pending={} note={}}}",
                    t.kind.slug(),
                    t.on.map_or("none".to_owned(), |b| b.to_string()),
                    t.pending,
                    t.note.as_deref().unwrap_or("-").replace(' ', "_")
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let brightness = model
            .brightness
            .iter()
            .map(|b| {
                format!(
                    "{{{} percent={} pending={}}}",
                    b.label.replace(' ', "_"),
                    b.percent.map_or("none".to_owned(), |p| p.to_string()),
                    b.pending
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let rect = self.utilities.frame().map_or("None".to_owned(), |f| {
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
        let controls = self
            .utilities
            .frame()
            .map(|f| {
                self.utilities
                    .rows()
                    .into_iter()
                    .filter_map(|r| {
                        let (x, y, w, h) = r.rect();
                        let name = match &r {
                            crate::utilities_panel::UtilitiesRow::Toggle { state, .. } => {
                                state.kind.slug().to_owned()
                            }
                            crate::utilities_panel::UtilitiesRow::Slider { index, .. } => {
                                format!("slider{index}")
                            }
                            _ => return None,
                        };
                        Some(format!(
                            "{name}@[{:.1},{:.1},{:.1},{:.1}]",
                            f.rect.left() + x,
                            f.rect.top() + y,
                            w,
                            h
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .unwrap_or_default();
        let line = format!(
            "GNOS_UX_UTILITIES state={state} open={} rect={rect} detecting={} toggles=[{toggles}] brightness=[{brightness}] controls=[{controls}]",
            self.utilities.is_open(),
            model.detecting
        );
        if state != "changed" || self.utilities_state.reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.utilities_state.reported = Some(line);
        }
    }

    /// A click on the status icons: open or close the panel.
    /// A click on the status icons: through the shell's attention
    /// (FOCUS-01), which opens it once whatever is out has gone in.
    pub(super) fn toggle_utilities(&mut self, cx: &mut Context<Self>) {
        self.attention_toggle(crate::attention::Attendant::Utilities, cx);
    }

    /// The panel out or in, as the shell's attention decided.
    pub(super) fn utilities_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.utilities.is_open() == open {
            return;
        }
        let opening = open;
        self.utilities.toggle_open();
        if opening {
            // The OSD gives way (the other surfaces already have).
            if self.bar_osd.close() {
                self.report_osd("closing");
            }
            self.utilities_state.notes.clear();
            // SURF-01E: the true brightness. A full detection (slow: it
            // scans the buses) at most every 30 s; otherwise the known
            // targets are read again, and again while the panel stays.
            if self
                .utilities_state
                .detected_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(30))
            {
                self.refresh_brightness(cx);
            } else {
                self.detect_brightness(cx);
            }
            self.schedule_brightness_refresh(cx);
        }
        self.report_utilities(if opening { "opened" } else { "closing" });
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// Read the displays and backlights (slow: off the UI thread).
    fn detect_brightness(&mut self, cx: &mut Context<Self>) {
        if self.utilities_state.detecting
            || self
                .utilities_state
                .detected_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(30))
        {
            return;
        }
        self.utilities_state.detecting = true;
        let root = backlight_root();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let mut entries = Vec::new();
                    for b in gnos_utilities::read_backlights(&root) {
                        entries.push(BrightnessEntry {
                            level: Some(b.level),
                            target: BrightnessTarget::Backlight(b),
                            pending: false,
                        });
                    }
                    if let Ok(displays) = gnos_utilities::detect_displays() {
                        for d in displays {
                            let level = gnos_utilities::read_display(&d).ok();
                            entries.push(BrightnessEntry {
                                target: BrightnessTarget::Display(d),
                                level,
                                pending: false,
                            });
                        }
                    }
                    entries
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                println!(
                    "GNOS_UX_UTILITIES_BRIGHTNESS state=detected targets={}",
                    found.len()
                );
                view.utilities_state.brightness = found;
                view.utilities_state.detecting = false;
                view.utilities_state.detected_at = Some(Instant::now());
                cx.notify();
            });
        })
        .detach();
    }

    /// SURF-01E: read the known targets' levels again (off the UI thread),
    /// so a change made by another program or the display's own buttons
    /// shows. A target with a change of ours in flight keeps its value.
    fn refresh_brightness(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.utilities_state;
        if state.detecting || state.refreshing || state.brightness.is_empty() {
            return;
        }
        state.refreshing = true;
        let targets: Vec<(String, BrightnessTarget)> = state
            .brightness
            .iter()
            .map(|e| (e.key(), e.target.clone()))
            .collect();
        let root = backlight_root();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let backlights = gnos_utilities::read_backlights(&root);
                    targets
                        .into_iter()
                        .map(|(key, target)| {
                            let level = match &target {
                                BrightnessTarget::Display(d) => gnos_utilities::read_display(d).ok(),
                                BrightnessTarget::Backlight(b) => backlights
                                    .iter()
                                    .find(|x| x.name == b.name)
                                    .map(|x| x.level),
                            };
                            (key, level)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let state = &mut view.utilities_state;
                state.refreshing = false;
                let mut changed = 0;
                let mut gone = 0;
                for (key, level) in read {
                    let Some(entry) = state.brightness.iter_mut().find(|e| e.key() == key) else {
                        continue;
                    };
                    match level {
                        Some(level) if !entry.pending && entry.level != Some(level) => {
                            entry.level = Some(level);
                            changed += 1;
                        }
                        Some(_) => {}
                        // Unreadable: perhaps unplugged; the next opening
                        // detects again.
                        None => gone += 1,
                    }
                }
                if gone > 0 {
                    state.detected_at = None;
                }
                println!(
                    "GNOS_UX_UTILITIES_BRIGHTNESS state=refreshed changed={changed} unreadable={gone}"
                );
                if changed > 0 {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// SURF-01E: while Utilities stays open, read the levels every
    /// `REFRESH_EVERY`.
    fn schedule_brightness_refresh(&mut self, cx: &mut Context<Self>) {
        if self.utilities_state.refresh_scheduled {
            return;
        }
        self.utilities_state.refresh_scheduled = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH_EVERY).await;
                let open = this
                    .update(cx, |view, cx| {
                        if view.utilities.is_open() {
                            view.refresh_brightness(cx);
                            true
                        } else {
                            view.utilities_state.refresh_scheduled = false;
                            false
                        }
                    })
                    .unwrap_or(false);
                if !open {
                    break;
                }
            }
        })
        .detach();
    }

    /// Route the panel's outcome; returns whether the event was its.
    pub(super) fn apply_utilities_outcome(
        &mut self,
        outcome: UtilitiesOutcome,
        cx: &mut Context<Self>,
    ) -> bool {
        for effect in outcome.effects {
            self.utilities_effect(effect, cx);
        }
        if outcome.changed {
            self.report_utilities(if self.utilities.is_open() {
                "changed"
            } else {
                "closing"
            });
            self.start_motion_driver(cx);
            cx.notify();
        }
        outcome.consumed
    }

    fn utilities_effect(&mut self, effect: UtilitiesEffect, cx: &mut Context<Self>) {
        match effect {
            UtilitiesEffect::Toggle { kind, enable } => self.utilities_toggle(kind, enable, cx),
            UtilitiesEffect::Brightness { index, percent } => {
                self.utilities_brightness(index, percent, cx)
            }
        }
    }

    fn request_receipt(kind: ToggleKind, enable: bool, outcome: &str) {
        println!(
            "GNOS_UX_UTILITIES_REQUEST power={} enable={enable} outcome={outcome}",
            kind.slug()
        );
    }

    fn utilities_toggle(&mut self, kind: ToggleKind, enable: bool, cx: &mut Context<Self>) {
        self.utilities_state.notes.remove(&kind);
        self.utilities_state
            .pending
            .insert(kind, (enable, Instant::now()));
        match kind {
            ToggleKind::Wifi => {
                let snapshot = self.product_runtime.network().clone();
                self.utilities_async(kind, enable, cx, async move {
                    let connection = zbus::Connection::system()
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(
                        match gnos_network::request_wireless(&connection, &snapshot, enable).await {
                            gnos_network::WirelessOutcome::Accepted => None,
                            gnos_network::WirelessOutcome::Refused(r) => Some(format!("{r:?}")),
                            gnos_network::WirelessOutcome::Denied(_) => Some("Denied".to_owned()),
                            gnos_network::WirelessOutcome::TransportFailure(e) => Some(e),
                        },
                    )
                });
            }
            ToggleKind::Bluetooth => {
                let snapshot = self.product_runtime.bluetooth().clone();
                let Some(adapter) = snapshot.adapters.keys().next().cloned() else {
                    self.utilities_refused(kind, enable, "No adapter".to_owned());
                    return;
                };
                self.utilities_async(kind, enable, cx, async move {
                    let connection = zbus::Connection::system()
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(
                        match gnos_bluetooth::request_power(
                            &connection,
                            &snapshot,
                            &adapter,
                            enable,
                        )
                        .await
                        {
                            gnos_bluetooth::BluetoothPowerOutcome::Accepted => None,
                            gnos_bluetooth::BluetoothPowerOutcome::Refused(r) => {
                                Some(format!("{r:?}"))
                            }
                            gnos_bluetooth::BluetoothPowerOutcome::Denied(_) => {
                                Some("Denied".to_owned())
                            }
                            gnos_bluetooth::BluetoothPowerOutcome::TransportFailure(e) => Some(e),
                        },
                    )
                });
            }
            ToggleKind::Microphone => {
                let model = crate::bar_audio_popout::AudioPopoutModel::from_snapshot(
                    self.product_runtime.audio(),
                );
                match (model.input, self.audio_interaction_readonly(cx)) {
                    (Some(input), Some(audio)) => {
                        Self::request_receipt(kind, enable, "Sent");
                        (audio.dispatch)(
                            cx,
                            crate::product_audio_action::AudioIntent::ToggleInputMute {
                                target: input.target,
                            },
                        );
                    }
                    _ => self.utilities_refused(kind, enable, "No microphone".to_owned()),
                }
            }
            ToggleKind::DoNotDisturb => {
                let session = crate::product_notification_session::NotificationSession::shared(cx);
                let sent = session
                    .read(cx)
                    .send(gnos_notifications::Command::SetDoNotDisturb(enable));
                if sent {
                    Self::request_receipt(kind, enable, "Sent");
                } else {
                    self.utilities_refused(kind, enable, "Unavailable".to_owned());
                }
            }
            ToggleKind::KeepAwake => {
                if !enable {
                    // Dropping the descriptor releases the lock at once.
                    self.utilities_state.inhibitor = None;
                    Self::request_receipt(kind, enable, "Released");
                    return;
                }
                cx.spawn(async move |this, cx| {
                    let held = cx
                        .background_executor()
                        .spawn(async move {
                            block_on(async {
                                let connection = zbus::Connection::system().await.ok()?;
                                let outcome = gnos_utilities::hold_idle_inhibitor(
                                    &connection,
                                    "Gnosix",
                                    "Keep awake (Utilities)",
                                )
                                .await;
                                let observed = gnos_utilities::list_inhibitors(&connection)
                                    .await
                                    .map(|l| {
                                        gnos_utilities::holds_idle(&l, "Gnosix", std::process::id())
                                    })
                                    .unwrap_or(false);
                                Some((outcome, observed))
                            })
                            .flatten()
                        })
                        .await;
                    let _ = this.update(cx, |view, cx| {
                        match held {
                            Some((gnos_utilities::IdleInhibitOutcome::Held(lock), observed)) => {
                                Self::request_receipt(
                                    kind,
                                    true,
                                    if observed {
                                        "Held observed=true"
                                    } else {
                                        "Held observed=false"
                                    },
                                );
                                if observed {
                                    view.utilities_state.inhibitor = Some(lock);
                                } else {
                                    view.utilities_refused(kind, true, "Not observed".to_owned());
                                }
                            }
                            Some((gnos_utilities::IdleInhibitOutcome::Denied(_), _)) => {
                                view.utilities_refused(kind, true, "Denied".to_owned())
                            }
                            Some((gnos_utilities::IdleInhibitOutcome::TransportFailure(e), _)) => {
                                view.utilities_refused(kind, true, e)
                            }
                            None => view.utilities_refused(kind, true, "No system bus".to_owned()),
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
    }

    fn utilities_refused(&mut self, kind: ToggleKind, enable: bool, note: String) {
        Self::request_receipt(
            kind,
            enable,
            &format!("Refused note={}", note.replace(' ', "_")),
        );
        self.utilities_state.pending.remove(&kind);
        self.utilities_state.notes.insert(kind, note);
    }

    /// A system-bus request off the UI thread; `Ok(None)` is an accepted
    /// request (the observer settles it), `Ok(Some(note))` a refusal.
    fn utilities_async<F>(
        &mut self,
        kind: ToggleKind,
        enable: bool,
        cx: &mut Context<Self>,
        request: F,
    ) where
        F: std::future::Future<Output = Result<Option<String>, String>> + Send + 'static,
    {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { block_on(request).unwrap_or_else(|| Err("No runtime".into())) })
                .await;
            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(None) => Self::request_receipt(kind, enable, "Accepted"),
                    Ok(Some(note)) | Err(note) => view.utilities_refused(kind, enable, note),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// SETTINGS-02: what the live controls show.
    pub(super) fn settings_live_facts(
        &self,
        cx: &mut Context<Self>,
    ) -> crate::settings_pages::LiveFacts {
        crate::settings_pages::LiveFacts {
            do_not_disturb: self.notification_centre.do_not_disturb(),
            keep_awake: self.utilities_state.inhibitor.is_some(),
            brightness: self
                .utilities_state
                .brightness
                .iter()
                .map(|e| {
                    (
                        e.label(),
                        (!e.pending).then_some(()).and(e.level.map(|l| l.percent())),
                    )
                })
                .collect(),
            reading_brightness: self.utilities_state.detecting,
            wifi: self.utilities_state.wifi.clone(),
            reading_wifi: self.utilities_state.wifi_reading,
            lock: self.utilities_state.lock.clone().unwrap_or_default(),
            default_apps: self.default_apps(),
            surfaces: crate::surface_prefs::current(),
            thumbnail_apps: self.thumbnail_apps(),
            wallpaper: {
                let prefs = crate::wallpaper::load();
                let folder = prefs.folder();
                let count = folder.as_deref().map_or_else(
                    || "0".to_owned(),
                    |f| crate::wallpaper::pictures(f).count_label(),
                );
                let name = prefs
                    .picture()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| gnos_i18n::t("None").to_owned());
                Some((
                    prefs.enabled,
                    name,
                    prefs.fit.label(),
                    count,
                    folder.map_or_else(String::new, |f| f.display().to_string()),
                ))
            },
            language_choice: gnos_i18n::load_choice().language,
            language_active: gnos_i18n::language().to_owned(),
            notification_prefs: self.notification_prefs(),
            notification_apps: self.notification_apps(cx),
            notification_service: {
                use crate::product_notification_session::NotificationServiceState as S;
                let session = crate::product_notification_session::NotificationSession::shared(cx);
                match session.read(cx).state() {
                    S::Serving => None,
                    S::NameTaken { owner } => Some(
                        owner
                            .clone()
                            .unwrap_or_else(|| gnos_i18n::t("unknown").to_owned()),
                    ),
                    S::NotStarted | S::Starting | S::Failed(_) => {
                        Some(gnos_i18n::t("not Gnosix: its service is not running").to_owned())
                    }
                }
            },
            displays: self.utilities_state.displays.clone(),
            display_revert_in: self
                .utilities_state
                .display_revert
                .as_ref()
                .map(|(_, at)| at.saturating_duration_since(Instant::now()).as_secs() as u32),
            bluetooth: self
                .paired_bluetooth()
                .into_iter()
                .map(|d| {
                    (
                        d.alias.clone().unwrap_or_else(|| "Unnamed device".into()),
                        d.connected,
                    )
                })
                .collect(),
            prompt: self.utilities_state.prompt.as_ref().map(|p| {
                (
                    gnos_i18n::tf("Password for {network}", &[("network", &p.network.ssid)]),
                    p.text.chars().count(),
                )
            }),
        }
    }

    /// The paired Bluetooth devices, in a stable order.
    fn paired_bluetooth(&self) -> Vec<gnos_bluetooth::BluetoothDeviceSnapshot> {
        let mut devices: Vec<_> = self
            .product_runtime
            .bluetooth()
            .devices
            .values()
            .filter(|d| d.paired)
            .cloned()
            .collect();
        devices.sort_by(|a, b| a.alias.cmp(&b.alias).then(a.id.cmp(&b.id)));
        devices
    }

    /// SETTINGS-02C: a key while a passphrase is being typed. Returns
    /// whether the prompt took it.
    pub(super) fn settings_prompt_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(prompt) = self.utilities_state.prompt.as_mut() else {
            return false;
        };
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "escape" => {
                self.utilities_state.prompt = None;
                println!("GNOS_UX_SETTINGS_LIVE prompt=cancelled");
            }
            "backspace" => {
                prompt.text.pop();
            }
            "enter" => {
                let prompt = self.utilities_state.prompt.take().expect("prompt");
                let length = prompt.text.chars().count();
                if !(8..=63).contains(&length) {
                    println!("GNOS_UX_SETTINGS_LIVE prompt=refused length={length}");
                    self.utilities_state.prompt = Some(prompt);
                } else {
                    println!("GNOS_UX_SETTINGS_LIVE wifi_connect passphrase_length={length}");
                    let WifiPrompt { network, text } = prompt;
                    cx.spawn(async move |this, cx| {
                        let outcome = cx
                            .background_executor()
                            .spawn(async move {
                                match zbus::Connection::system().await {
                                    Ok(bus) => {
                                        gnos_network::connect_with_passphrase(&bus, &network, &text)
                                            .await
                                    }
                                    Err(error) => {
                                        gnos_network::WifiConnectOutcome::Failed(error.to_string())
                                    }
                                }
                            })
                            .await;
                        let _ = this.update(cx, |view, cx| {
                            println!("GNOS_UX_SETTINGS_LIVE wifi_connect outcome={outcome:?}");
                            view.utilities_state.wifi_read_at = None;
                            view.refresh_settings_lines(cx);
                            cx.notify();
                        });
                    })
                    .detach();
                }
            }
            _ => {
                if !keystroke.modifiers.control
                    && !keystroke.modifiers.alt
                    && !keystroke.modifiers.platform
                    && let Some(ch) = keystroke.key_char.as_deref()
                    && ch.chars().all(|c| !c.is_control())
                    && prompt.text.chars().count() < 63
                {
                    prompt.text.push_str(ch);
                }
            }
        }
        self.refresh_settings_lines(cx);
        cx.notify();
        true
    }

    /// APPS-01: each role's default (by name) and its number of candidates.
    fn default_apps(&self) -> Vec<(Option<String>, usize)> {
        use gnosix_application_catalog::defaults::{
            DefaultRole, candidates, mimeapps_default, mimeapps_path,
        };
        let catalog = self.product_runtime.application_catalog();
        let text = mimeapps_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        DefaultRole::ALL
            .iter()
            .map(|role| {
                let current = mimeapps_default(&text, role.types()[0]).map(|id| {
                    catalog
                        .entries()
                        .iter()
                        .find(|a| a.id.as_str() == id)
                        .map_or(id, |a| a.name.clone())
                });
                (current, candidates(catalog, *role).len())
            })
            .collect()
    }

    /// SETTINGS-03B: the preferences as stored (read once).
    fn notification_prefs(&self) -> gnos_notifications::prefs::NotificationPrefs {
        self.utilities_state
            .notification_prefs
            .clone()
            .unwrap_or_else(|| {
                gnos_notifications::prefs::default_path()
                    .map(|p| gnos_notifications::prefs::load(&p))
                    .unwrap_or_default()
            })
    }

    /// SETTINGS-03B: the applications that sent a notification still kept,
    /// and those with a rule, by name.
    fn notification_apps(&self, cx: &mut Context<Self>) -> Vec<String> {
        let session = crate::product_notification_session::NotificationSession::shared(cx);
        let mut apps: std::collections::BTreeSet<String> = session
            .read(cx)
            .latest()
            .map(|u| {
                u.snapshot
                    .notifications
                    .iter()
                    .map(|n| n.app_name.clone())
                    .filter(|a| !a.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        apps.extend(self.notification_prefs().apps.keys().cloned());
        apps.into_iter().collect()
    }

    /// SETTINGS-03B: give the running service the preferences, once it
    /// serves (and again after each change).
    pub(super) fn notification_prefs_sync(&mut self, cx: &mut Context<Self>) {
        if self.utilities_state.notification_prefs_sent {
            return;
        }
        let prefs = self.notification_prefs();
        let session = crate::product_notification_session::NotificationSession::shared(cx);
        if session
            .read(cx)
            .send(gnos_notifications::Command::SetPreferences(prefs))
        {
            self.utilities_state.notification_prefs_sent = true;
            println!("GNOS_UX_SETTINGS_LIVE notification_prefs=sent");
        }
    }

    /// SETTINGS-03B: store a change, then hand it to the service.
    fn notification_prefs_store(
        &mut self,
        prefs: gnos_notifications::prefs::NotificationPrefs,
        cx: &mut Context<Self>,
    ) {
        let outcome = match gnos_notifications::prefs::default_path() {
            Some(path) => gnos_notifications::prefs::save(&path, &prefs)
                .map(|()| "saved".to_owned())
                .unwrap_or_else(|e| e.to_string()),
            None => "no config directory".to_owned(),
        };
        println!("GNOS_UX_SETTINGS_LIVE notification_prefs={outcome}");
        if outcome == "saved" {
            self.utilities_state.notification_prefs = Some(prefs);
            self.utilities_state.notification_prefs_sent = false;
            self.notification_prefs_sync(cx);
        }
        self.refresh_settings_lines(cx);
    }

    /// DISPLAY-01: read the outputs (off the UI thread).
    pub(super) fn settings_read_displays(&mut self, cx: &mut Context<Self>) {
        if self.utilities_state.display_busy {
            return;
        }
        self.utilities_state.display_busy = true;
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { gnos_utilities::read_outputs() })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.utilities_state.display_busy = false;
                match read {
                    Ok(outputs) => view.utilities_state.displays = outputs,
                    Err(error) => {
                        println!("GNOS_UX_SETTINGS_LIVE displays=unreadable error={error:?}")
                    }
                }
                view.refresh_settings_lines(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// DISPLAY-01: apply specs one after the other, then read the outputs
    /// back (what is shown is what Hyprland reports).
    fn display_apply(
        &mut self,
        specs: Vec<gnos_utilities::DisplaySpec>,
        what: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.utilities_state.display_busy = true;
        cx.spawn(async move |this, cx| {
            let (outcome, read) = cx
                .background_executor()
                .spawn(async move {
                    let outcome = specs
                        .iter()
                        .map(gnos_utilities::apply)
                        .find(Result::is_err)
                        .unwrap_or(Ok(()));
                    (outcome, gnos_utilities::read_outputs())
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.utilities_state.display_busy = false;
                println!("GNOS_UX_SETTINGS_LIVE display={what} outcome={outcome:?}");
                if let Ok(outputs) = read {
                    view.utilities_state.displays = outputs;
                }
                view.refresh_settings_lines(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// DISPLAY-01: count the confirmation down, and revert when it runs out.
    fn display_countdown(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(go_on) = this.update(cx, |view, cx| {
                    let Some((_, at)) = &view.utilities_state.display_revert else {
                        return false;
                    };
                    if Instant::now() >= *at {
                        let (previous, _) =
                            view.utilities_state.display_revert.take().expect("pending");
                        view.display_apply(previous, "reverted_unconfirmed", cx);
                        return false;
                    }
                    view.refresh_settings_lines(cx);
                    cx.notify();
                    true
                }) else {
                    break;
                };
                if !go_on {
                    break;
                }
            }
        })
        .detach();
    }

    /// LOCK-01: read the lock screen's settings once (the page, and each
    /// change, refresh them).
    pub(super) fn settings_read_lock(&mut self) {
        if self.utilities_state.lock.is_none() {
            self.utilities_state.lock = Some(
                gnos_lock::default_path()
                    .map(|p| gnos_lock::load(&p))
                    .unwrap_or_default(),
            );
        }
    }

    /// SETTINGS-02C: read the networks in range (off the UI thread, at most
    /// every 10 s).
    pub(super) fn settings_read_wifi(&mut self, cx: &mut Context<Self>) {
        if self.utilities_state.wifi_reading
            || self
                .utilities_state
                .wifi_read_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
        {
            return;
        }
        self.utilities_state.wifi_reading = true;
        self.utilities_state.wifi_read_at = Some(Instant::now());
        cx.spawn(async move |this, cx| {
            let networks = cx
                .background_executor()
                .spawn(async move {
                    match zbus::Connection::system().await {
                        Ok(bus) => gnos_network::list_networks(&bus).await,
                        Err(_) => Vec::new(),
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                println!("GNOS_UX_SETTINGS_LIVE wifi_networks={}", networks.len());
                view.utilities_state.wifi = networks;
                view.utilities_state.wifi_reading = false;
                view.refresh_settings_lines(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// SETTINGS-02: a live control in Settings, carried out by the power
    /// Utilities and the Bar already use.
    pub(super) fn settings_live(
        &mut self,
        control: crate::settings_pages::LiveControl,
        cx: &mut Context<Self>,
    ) {
        use crate::settings_pages::LiveControl;
        println!("GNOS_UX_SETTINGS_LIVE control={control:?}");
        let toggle = |this: &Self, kind: ToggleKind| {
            this.utilities_toggles()
                .into_iter()
                .find(|t| t.kind == kind)
                .and_then(|t| t.on)
        };
        let flip = |this: &mut Self, kind: ToggleKind, cx: &mut Context<Self>| {
            if this.utilities_state.pending.contains_key(&kind) {
                return;
            }
            let on = toggle(this, kind).unwrap_or(false);
            this.utilities_toggle(kind, !on, cx);
        };
        match control {
            LiveControl::Wifi => flip(self, ToggleKind::Wifi, cx),
            LiveControl::Bluetooth => flip(self, ToggleKind::Bluetooth, cx),
            LiveControl::DoNotDisturb => flip(self, ToggleKind::DoNotDisturb, cx),
            LiveControl::KeepAwake => flip(self, ToggleKind::KeepAwake, cx),
            LiveControl::Microphone => flip(self, ToggleKind::Microphone, cx),
            LiveControl::OutputMute | LiveControl::OutputVolume(_) => {
                let model = crate::bar_audio_popout::AudioPopoutModel::from_snapshot(
                    self.product_runtime.audio(),
                );
                let intents = match (control, model.output) {
                    (LiveControl::OutputMute, Some(output)) => {
                        vec![crate::product_audio_action::AudioIntent::ToggleOutputMute {
                            target: output.target,
                        }]
                    }
                    (LiveControl::OutputVolume(d), _) => model.step(if d < 0 {
                        crate::bar_pointer_routing::StepDirection::Decrease
                    } else {
                        crate::bar_pointer_routing::StepDirection::Increase
                    }),
                    _ => Vec::new(),
                };
                if let Some(audio) = self.audio_interaction_readonly(cx) {
                    for intent in intents {
                        println!("GNOS_UX_UXE01_AUDIO_INTENT intent={intent:?}");
                        (audio.dispatch)(cx, intent);
                    }
                }
            }
            LiveControl::Brightness(index, d) => {
                let index = usize::from(index);
                if let Some(current) = self.utilities_brightness_percent(index) {
                    let next = if d < 0 {
                        current.saturating_sub(5)
                    } else {
                        current.saturating_add(5).min(100)
                    };
                    if next != current {
                        self.utilities_brightness(index, next, cx);
                    }
                }
            }
            LiveControl::WifiConnect(index) => {
                let Some(network) = self.utilities_state.wifi.get(usize::from(index)).cloned()
                else {
                    return;
                };
                cx.spawn(async move |this, cx| {
                    let outcome = cx
                        .background_executor()
                        .spawn(async move {
                            match zbus::Connection::system().await {
                                Ok(bus) => gnos_network::connect(&bus, &network).await,
                                Err(error) => {
                                    gnos_network::WifiConnectOutcome::Failed(error.to_string())
                                }
                            }
                        })
                        .await;
                    let _ = this.update(cx, |view, cx| {
                        println!("GNOS_UX_SETTINGS_LIVE wifi_connect outcome={outcome:?}");
                        // Read again: the list shows what NetworkManager did.
                        view.utilities_state.wifi_read_at = None;
                        cx.notify();
                    });
                })
                .detach();
            }
            LiveControl::BluetoothDevice(index) => {
                let Some(device) = self.paired_bluetooth().get(usize::from(index)).cloned() else {
                    return;
                };
                let snapshot = self.product_runtime.bluetooth().clone();
                let connect = !device.connected;
                cx.spawn(async move |this, cx| {
                    let outcome = cx
                        .background_executor()
                        .spawn(async move {
                            match zbus::Connection::system().await {
                                Ok(bus) => {
                                    gnos_bluetooth::request_device_connection(
                                        &bus, &snapshot, &device.id, connect,
                                    )
                                    .await
                                }
                                Err(error) => gnos_bluetooth::BluetoothConnectOutcome::Failed(
                                    error.to_string(),
                                ),
                            }
                        })
                        .await;
                    let _ = this.update(cx, |view, cx| {
                        println!(
                            "GNOS_UX_SETTINGS_LIVE bluetooth_connect={connect} outcome={outcome:?}"
                        );
                        view.refresh_settings_lines(cx);
                        cx.notify();
                    });
                })
                .detach();
            }
            LiveControl::Lock(setting, dir) => {
                self.settings_read_lock();
                let current = self.utilities_state.lock.clone().unwrap_or_default();
                let next = current.stepped(setting, dir);
                let outcome = match gnos_lock::default_path() {
                    Some(path) => gnos_lock::save(&path, &next)
                        .map(|()| "saved".to_owned())
                        .unwrap_or_else(|e| e.to_string()),
                    None => "no config directory".to_owned(),
                };
                println!(
                    "GNOS_UX_SETTINGS_LIVE lock_setting={setting:?} dir={dir} outcome={outcome}"
                );
                if outcome == "saved" {
                    self.utilities_state.lock = Some(next);
                }
                self.refresh_settings_lines(cx);
            }
            LiveControl::Surface(setting, dir) => {
                use crate::settings_pages::SurfaceSetting as S;
                use crate::surface_prefs::{HOLD_PRESETS, OSD_DWELL_PRESETS, step_preset};
                let mut p = crate::surface_prefs::current();
                match setting {
                    S::OsdShown => p.osd_shown = !p.osd_shown,
                    S::Advanced(i) => {
                        if let Some(tune) = crate::surface_prefs::TUNABLES.get(usize::from(i)) {
                            p.step_advanced(tune.slug, dir);
                        }
                    }
                    S::OsdDwell => {
                        p.osd_dwell_ms = step_preset(&OSD_DWELL_PRESETS, p.osd_dwell_ms, dir)
                    }
                    S::SessionHold => {
                        p.session_hold_ms = step_preset(&HOLD_PRESETS, p.session_hold_ms, dir)
                    }
                    S::DashboardOnHover => p.dashboard_on_hover = !p.dashboard_on_hover,
                    S::DesktopClock => p.desktop_clock = !p.desktop_clock,
                    S::DesktopVisualiser => p.desktop_visualiser = !p.desktop_visualiser,
                    S::Translucent => p.material.translucent = !p.material.translucent,
                    S::ShellOpacity => {
                        p.material.shell_opacity_pct = step_preset(
                            &crate::surface_prefs::SHELL_OPACITY_PRESETS,
                            p.material.shell_opacity_pct,
                            dir,
                        )
                    }
                    S::LayerOpacity => {
                        p.material.layer_opacity_pct = step_preset(
                            &crate::surface_prefs::LAYER_OPACITY_PRESETS,
                            p.material.layer_opacity_pct,
                            dir,
                        )
                    }
                    S::Tint => p.material.tint = p.material.tint.step(dir),
                    S::Blur => p.material.blur = !p.material.blur,
                    S::BlurMask => p.material.blur_mask = p.material.blur_mask.toggled(),
                    S::SurfaceOpacity(i) => {
                        if let Some((slug, _)) =
                            crate::surface_prefs::MATERIAL_SURFACES.get(usize::from(i))
                        {
                            p.material.step_surface(slug, dir);
                        }
                    }
                    S::DashboardWidth => {
                        p.dashboard_width = step_preset(
                            &crate::surface_prefs::DASHBOARD_WIDTH_PRESETS,
                            p.dashboard_width,
                            dir,
                        )
                    }
                    S::DashboardTab(i) => {
                        if let Some(tab) = crate::dashboard_panel::DashTab::ALL.get(usize::from(i))
                        {
                            let slug = tab.slug().to_owned();
                            if !p.dashboard_hidden_tabs.remove(&slug) {
                                p.dashboard_hidden_tabs.insert(slug);
                            }
                            // One tab always stays.
                            if p.dashboard_hidden_tabs.len() >= 3 {
                                p.dashboard_hidden_tabs.remove(tab.slug());
                            }
                        }
                    }
                    S::WorkspacesOpen => p.workspaces_open = p.workspaces_open.step(dir),
                    S::WorkspacesView => p.workspaces_view = p.workspaces_view.step(dir),
                    S::WorkspacesTile => {
                        p.workspaces_tile_px = step_preset(
                            &crate::surface_prefs::WORKSPACE_TILE_PRESETS,
                            p.workspaces_tile_px,
                            dir,
                        )
                    }
                    S::SurfacesMeeting => p.surfaces_meeting = p.surfaces_meeting.step(dir),
                    S::WarmHandoff => p.behaviour.warm_handoff = !p.behaviour.warm_handoff,
                    S::Aim => p.behaviour.aim = !p.behaviour.aim,
                    S::AimSlack => {
                        p.behaviour.aim_slack_px = step_preset(
                            &crate::surface_prefs::AIM_SLACK_PRESETS,
                            p.behaviour.aim_slack_px,
                            dir,
                        )
                    }
                    S::AimSpeed => {
                        p.behaviour.aim_min_speed = step_preset(
                            &crate::surface_prefs::AIM_SPEED_PRESETS,
                            p.behaviour.aim_min_speed,
                            dir,
                        )
                    }
                    S::AimWindow => {
                        p.behaviour.aim_window_ms = step_preset(
                            &crate::surface_prefs::AIM_WINDOW_PRESETS,
                            p.behaviour.aim_window_ms,
                            dir,
                        )
                    }
                    S::StatusOnHover => p.behaviour.status_on_hover = !p.behaviour.status_on_hover,
                    S::TrayOnHover => p.behaviour.tray_on_hover = !p.behaviour.tray_on_hover,
                    S::ThumbnailsLive => p.thumbnails.live = !p.thumbnails.live,
                    S::ThumbnailRefresh => {
                        p.thumbnails.refresh_ms = step_preset(
                            &crate::surface_prefs::THUMBNAIL_REFRESH_PRESETS,
                            p.thumbnails.refresh_ms,
                            dir,
                        )
                    }
                    S::ThumbnailApp(index) => {
                        // Not shown: let its entry go; shown: exclude it.
                        if let Some((app, excluded_by)) =
                            self.thumbnail_apps().get(usize::from(index))
                        {
                            p.thumbnails.toggle(excluded_by.as_deref().unwrap_or(app));
                        }
                    }
                    S::LaunchFeedback => {
                        p.behaviour.launch_feedback_s = step_preset(
                            &crate::surface_prefs::LAUNCH_FEEDBACK_PRESETS,
                            p.behaviour.launch_feedback_s,
                            dir,
                        )
                    }
                    S::Dwell(module) => {
                        if let Some(t) = p.behaviour.timing_mut(module) {
                            t.dwell_ms =
                                step_preset(&crate::surface_prefs::DWELL_PRESETS, t.dwell_ms, dir);
                        }
                    }
                    S::Grace(module) => {
                        if let Some(t) = p.behaviour.timing_mut(module) {
                            t.grace_ms =
                                step_preset(&crate::surface_prefs::GRACE_PRESETS, t.grace_ms, dir);
                        }
                    }
                    S::WorkspacesWindows => {
                        p.workspaces_windows = step_preset(
                            &crate::surface_prefs::WORKSPACE_WINDOWS_PRESETS,
                            p.workspaces_windows,
                            dir,
                        )
                    }
                    S::ClockScale => {
                        p.clock_scale_pct = step_preset(
                            &crate::surface_prefs::MODULE_SCALE_PRESETS,
                            p.clock_scale_pct,
                            dir,
                        )
                    }
                    S::ClockDate => p.clock_date = !p.clock_date,
                    S::ClockSeconds => p.clock_seconds = !p.clock_seconds,
                    S::VisualiserScale => {
                        p.visualiser_scale_pct = step_preset(
                            &crate::surface_prefs::MODULE_SCALE_PRESETS,
                            p.visualiser_scale_pct,
                            dir,
                        )
                    }
                    S::BarFitHide => {
                        p.bar_fit.hide_what_does_not_fit = !p.bar_fit.hide_what_does_not_fit
                    }
                    S::BarGiveWay(i) => {
                        if let Some((slug, _)) =
                            crate::surface_prefs::BAR_MODULES.get(usize::from(i))
                        {
                            p.bar_fit.move_module(slug, dir);
                        }
                    }
                    S::Tile(i) => {
                        if let Some(kind) = ToggleKind::ALL.get(usize::from(i)) {
                            let slug = kind.slug().to_owned();
                            if !p.hidden_tiles.remove(&slug) {
                                p.hidden_tiles.insert(slug);
                            }
                        }
                    }
                }
                let outcome = crate::surface_prefs::save(&p).map_err(|e| e.to_string());
                println!("GNOS_UX_SETTINGS_LIVE surface={setting:?} dir={dir} outcome={outcome:?}");
                // Every output follows at once, this one included.
                crate::surface_prefs::SettingsChanged::announce(cx);
                self.refresh_settings_lines(cx);
            }
            LiveControl::WallpaperEnabled
            | LiveControl::WallpaperPicture(_)
            | LiveControl::WallpaperFit => {
                let mut prefs = crate::wallpaper::load();
                match control {
                    LiveControl::WallpaperEnabled => prefs.enabled = !prefs.enabled,
                    LiveControl::WallpaperFit => prefs.fit = prefs.fit.next(),
                    LiveControl::WallpaperPicture(dir) => {
                        let list = prefs
                            .folder()
                            .map(|f| crate::wallpaper::pictures(&f).list)
                            .unwrap_or_default();
                        let current = prefs.picture();
                        if let Some(next) =
                            crate::wallpaper::step_picture(&list, current.as_deref(), dir)
                        {
                            prefs.picture = Some(next);
                        }
                    }
                    _ => {}
                }
                let outcome = crate::wallpaper::save(&prefs).map_err(|e| e.to_string());
                println!(
                    "GNOS_UX_SETTINGS_LIVE wallpaper enabled={} fit={:?} outcome={outcome:?}",
                    prefs.enabled, prefs.fit
                );
                // Every output follows at once, this one included.
                crate::surface_prefs::SettingsChanged::announce(cx);
                self.refresh_settings_lines(cx);
            }
            LiveControl::Language => {
                // The system's, then each shipped language, round again.
                let current = gnos_i18n::load_choice().language;
                let codes: Vec<Option<String>> = std::iter::once(None)
                    .chain(
                        gnos_i18n::LANGUAGES
                            .iter()
                            .map(|(c, _)| Some((*c).to_owned())),
                    )
                    .collect();
                let at = codes.iter().position(|c| *c == current).unwrap_or(0);
                let next = codes[(at + 1) % codes.len()].clone();
                let outcome = gnos_i18n::save_choice(&gnos_i18n::LanguageChoice {
                    language: next.clone(),
                })
                .map_err(|e| e.to_string());
                println!(
                    "GNOS_UX_SETTINGS_LIVE language={next:?} active={} outcome={outcome:?}",
                    gnos_i18n::language()
                );
                self.refresh_settings_lines(cx);
                cx.notify();
            }
            LiveControl::DefaultApp(index) => {
                use gnosix_application_catalog::defaults::{
                    DefaultRole, candidates, mimeapps_default, mimeapps_path, set_default,
                };
                let Some(role) = DefaultRole::ALL.get(usize::from(index)).copied() else {
                    return;
                };
                let Some(path) = mimeapps_path() else {
                    return;
                };
                let catalog = self.product_runtime.application_catalog();
                let choices = candidates(catalog, role);
                if choices.is_empty() {
                    return;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let current = mimeapps_default(&text, role.types()[0]);
                let at = choices
                    .iter()
                    .position(|a| Some(a.id.as_str()) == current.as_deref());
                let next = choices[at.map_or(0, |i| (i + 1) % choices.len())];
                let id = next.id.as_str().to_owned();
                let outcome = set_default(&path, role, &id).map_err(|e| e.to_string());
                println!(
                    "GNOS_UX_SETTINGS_LIVE default_app role={role:?} app={id} outcome={outcome:?}"
                );
                self.refresh_settings_lines(cx);
            }
            LiveControl::NotificationToast(dir) => {
                let mut prefs = self.notification_prefs();
                prefs.toast_secs = prefs.stepped_toast(dir);
                self.notification_prefs_store(prefs, cx);
            }
            LiveControl::NotificationCritical => {
                let mut prefs = self.notification_prefs();
                prefs.critical_through_dnd = !prefs.critical_through_dnd;
                self.notification_prefs_store(prefs, cx);
            }
            LiveControl::NotificationApp(index) => {
                let apps = self.notification_apps(cx);
                let Some(app) = apps.get(usize::from(index)) else {
                    return;
                };
                let mut prefs = self.notification_prefs();
                let next = prefs.rule(app).next();
                if next == gnos_notifications::prefs::AppRule::Show {
                    prefs.apps.remove(app);
                } else {
                    prefs.apps.insert(app.clone(), next);
                }
                self.notification_prefs_store(prefs, cx);
            }
            LiveControl::Display(index, setting, dir) => {
                if self.utilities_state.display_busy {
                    return;
                }
                let Some(spec) = gnos_utilities::next_spec(
                    &self.utilities_state.displays,
                    usize::from(index),
                    setting,
                    dir,
                ) else {
                    return;
                };
                if self.utilities_state.display_revert.is_none() {
                    let previous = self
                        .utilities_state
                        .displays
                        .iter()
                        .map(gnos_utilities::DisplaySpec::of)
                        .collect();
                    self.utilities_state.display_revert =
                        Some((previous, Instant::now() + Duration::from_secs(15)));
                    self.display_countdown(cx);
                } else if let Some((_, at)) = self.utilities_state.display_revert.as_mut() {
                    // Another change: the confirmation starts again.
                    *at = Instant::now() + Duration::from_secs(15);
                }
                println!(
                    "GNOS_UX_SETTINGS_LIVE display_change output={} setting={setting:?} dir={dir}",
                    spec.output
                );
                self.display_apply(vec![spec], "applied", cx);
            }
            LiveControl::DisplayKeep => {
                if self.utilities_state.display_revert.take().is_none() {
                    return;
                }
                let specs: Vec<_> = self
                    .utilities_state
                    .displays
                    .iter()
                    .map(gnos_utilities::DisplaySpec::of)
                    .collect();
                let written = gnos_utilities::layout_lua(&specs)
                    .map_err(|e| format!("{e:?}"))
                    .and_then(|lua| {
                        let path = gnos_lock::default_path()
                            .and_then(|p| p.parent().map(|d| d.join("displays.lua")))
                            .ok_or_else(|| "no config directory".to_owned())?;
                        let tmp = path.with_extension("lua.tmp");
                        std::fs::write(&tmp, lua)
                            .and_then(|()| std::fs::rename(&tmp, &path))
                            .map_err(|e| e.to_string())
                    });
                println!("GNOS_UX_SETTINGS_LIVE display=kept written={written:?}");
            }
            LiveControl::DisplayRevert => {
                if let Some((previous, _)) = self.utilities_state.display_revert.take() {
                    self.display_apply(previous, "reverted", cx);
                }
            }
            LiveControl::WifiPassword(index) => {
                if let Some(network) = self.utilities_state.wifi.get(usize::from(index)).cloned() {
                    self.utilities_state.prompt = Some(WifiPrompt {
                        network,
                        text: String::new(),
                    });
                    println!("GNOS_UX_SETTINGS_LIVE prompt=opened");
                }
            }
            LiveControl::DefaultDevice(id, kind) => {
                let target = crate::product_audio_action::ExactAudioEndpointRef { id, kind };
                if let Some(audio) = self.audio_interaction_readonly(cx) {
                    let intent = crate::product_audio_action::AudioIntent::SelectDefault { target };
                    println!("GNOS_UX_UXE01_AUDIO_INTENT intent={intent:?}");
                    (audio.dispatch)(cx, intent);
                }
            }
            LiveControl::PowerProfile => {
                let gnos_power::PowerSnapshot::Profile(profile) = self.product_runtime.power()
                else {
                    return;
                };
                let next = gnos_power::next_profile(profile);
                cx.spawn(async move |_, _| {
                    let outcome = match zbus::Connection::system().await {
                        Ok(bus) => gnos_power::request_profile(&bus, next).await,
                        Err(error) => Err(error.to_string()),
                    };
                    println!(
                        "GNOS_UX_SETTINGS_LIVE power_profile={} outcome={}",
                        gnos_power::profile_name(next),
                        outcome.err().unwrap_or_else(|| "Sent".to_owned())
                    );
                })
                .detach();
            }
        }
        cx.notify();
    }

    /// RAIL-01: the brightness entry for `output` (a display on that
    /// connector), else a backlight, else the first.
    pub(super) fn utilities_brightness_target(&self, output: Option<&str>) -> Option<usize> {
        let entries = &self.utilities_state.brightness;
        let on_output = output.and_then(|output| {
            entries.iter().position(|e| match &e.target {
                BrightnessTarget::Display(d) => d
                    .connector
                    .as_deref()
                    .is_some_and(|c| c == output || c.ends_with(&format!("-{output}"))),
                BrightnessTarget::Backlight(_) => false,
            })
        });
        on_output
            .or_else(|| {
                entries
                    .iter()
                    .position(|e| matches!(e.target, BrightnessTarget::Backlight(_)))
            })
            .or((!entries.is_empty()).then_some(0))
    }

    /// RAIL-01: an entry's level in percent, when known and not changing.
    pub(super) fn utilities_brightness_percent(&self, index: usize) -> Option<u8> {
        let entry = self.utilities_state.brightness.get(index)?;
        (!entry.pending).then_some(())?;
        entry.level.map(|level| level.percent())
    }

    pub(super) fn utilities_detect_brightness(&mut self, cx: &mut Context<Self>) {
        self.detect_brightness(cx);
    }

    pub(super) fn utilities_set_brightness(
        &mut self,
        index: usize,
        percent: u8,
        cx: &mut Context<Self>,
    ) {
        self.utilities_brightness(index, percent, cx);
    }

    fn utilities_brightness(&mut self, index: usize, percent: u8, cx: &mut Context<Self>) {
        let Some(entry) = self.utilities_state.brightness.get_mut(index) else {
            return;
        };
        entry.pending = true;
        let target = entry.target.clone();
        let observed = entry.level;
        println!(
            "GNOS_UX_UTILITIES_REQUEST power=brightness{index} percent={percent} outcome=Sent"
        );
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match &target {
                        BrightnessTarget::Display(display) => {
                            let outcome = gnos_utilities::request_display(display, observed, percent);
                            (outcome, gnos_utilities::read_display(display).ok())
                        }
                        BrightnessTarget::Backlight(backlight) => {
                            let outcome = block_on(async {
                                match zbus::Connection::system().await {
                                    Ok(c) => {
                                        gnos_utilities::request_backlight(&c, backlight, percent)
                                            .await
                                    }
                                    Err(e) => gnos_utilities::BrightnessOutcome::Failed(e.to_string()),
                                }
                            })
                            .unwrap_or(gnos_utilities::BrightnessOutcome::Failed("No runtime".into()));
                            let level = gnos_utilities::read_backlights(&backlight_root())
                                .into_iter()
                                .find(|b| b.name == backlight.name)
                                .map(|b| b.level);
                            (outcome, level)
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let (outcome, level) = result;
                println!(
                    "GNOS_UX_UTILITIES_REQUEST power=brightness{index} percent={percent} outcome={outcome:?} observed={}",
                    level.map_or("none".to_owned(), |l| l.percent().to_string())
                );
                let mut observed = None;
                if let Some(entry) = view.utilities_state.brightness.get_mut(index) {
                    entry.pending = false;
                    if level.is_some() {
                        entry.level = level;
                    }
                    observed = entry.level.map(|l| (entry.label(), l.percent()));
                }
                // SURF-01C: an observed level (the OSD gives way to the
                // open panel, but remembers it).
                if let Some((label, percent)) = observed {
                    view.observe_osd_brightness(label, percent, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}
