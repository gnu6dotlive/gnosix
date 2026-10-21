//! LOCK-01: `gnos-lockd`, the Gnosix lock screen daemon.
//!
//! A user service. It waits for a lock (logind's `Lock` signal for this
//! session, the idle timer, the system about to sleep, or `--lock-now`),
//! holds the compositor's session lock (`ext-session-lock-v1`) with one
//! surface per output, and unlocks only when PAM (service `gnosix-lock`)
//! accepts the typed password. If this process dies, the compositor keeps
//! the session locked.
//!
//! The deployed daemon never honours logind's `Unlock` signal.

use chrono::Datelike;
use gnos_i18n::{t, tf};
use gnos_lock::{LockConfig, LockEffect, LockEvent, LockPhase, Machine, Notice};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_registry, delegate_seat,
    delegate_session_lock, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RepeatInfo},
    },
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};

const PAM_SERVICE: &str = "gnosix-lock";

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday", // i18n
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June", // i18n
    "July",
    "August",
    "September",
    "October",
    "November",
    "December", // i18n
];

/// The login name of this process's real uid, from the kernel's view
/// (never an environment variable, which a process could set): the user PAM
/// authenticates. Only a debug build (the qualification's) takes
/// `GNOS_LOCK_USER`, to authenticate a fixture user.
fn current_user() -> String {
    #[cfg(debug_assertions)]
    if let Ok(user) = std::env::var("GNOS_LOCK_USER") {
        return user;
    }
    let uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|l| l.strip_prefix("Uid:"))
                .and_then(|v| v.split_whitespace().next().map(str::to_owned))
        })
        .unwrap_or_default();
    std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|passwd| {
            passwd.lines().find_map(|line| {
                let mut fields = line.split(':');
                let name = fields.next()?;
                (fields.nth(1)? == uid).then(|| name.to_owned())
            })
        })
        .unwrap_or_else(|| "nobody".to_owned())
}

/// The PAM service. Only a debug build (the qualification's) can be pointed
/// at another one, by `GNOS_LOCK_PAM_SERVICE`; a release build always uses
/// `gnosix-lock`.
fn pam_service() -> String {
    #[cfg(debug_assertions)]
    if let Ok(service) = std::env::var("GNOS_LOCK_PAM_SERVICE") {
        return service;
    }
    PAM_SERVICE.to_owned()
}

/// What the other threads tell the Wayland loop.
enum Message {
    Event(LockEvent),
}

/// What the Wayland loop tells the logind thread.
enum Logind {
    /// The settings changed: whether to lock before sleep.
    LockBeforeSleep(bool),
    SetLockedHint(bool),
    /// The compositor holds the lock: release the sleep inhibitor.
    Locked,
}

struct Surface {
    lock_surface: SessionLockSurface,
    size: (u32, u32),
    pool: Option<SlotPool>,
}

struct Lockd {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    lock_state: SessionLockState,
    lock: Option<SessionLock>,
    surfaces: Vec<Surface>,
    machine: Machine,
    user: String,
    tx: mpsc::Sender<Message>,
    logind: tokio::sync::mpsc::UnboundedSender<Logind>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Where the settings are read from, and when they were last read.
    config_path: Option<std::path::PathBuf>,
    config_modified: Option<std::time::SystemTime>,
    idle: Option<Idle>,
    /// When the screens-off timer started, and whether it has fired.
    locked_since: Option<Instant>,
    screens_off: bool,
    text: Text,
    exit: bool,
    tick: bool,
    last_drawn: String,
}

/// The idle notification in force: re-armed when its delay changes.
struct Idle {
    notifier: ExtIdleNotifierV1,
    seat: wl_seat::WlSeat,
    notification: Option<ExtIdleNotificationV1>,
    secs: Option<u32>,
}

impl Idle {
    fn arm(&mut self, secs: Option<u32>, qh: &QueueHandle<Lockd>) {
        if self.secs == secs && self.notification.is_some() == secs.is_some() {
            return;
        }
        if let Some(old) = self.notification.take() {
            old.destroy();
        }
        self.secs = secs;
        if let Some(secs) = secs {
            self.notification = Some(self.notifier.get_idle_notification(
                secs.saturating_mul(1000),
                &self.seat,
                qh,
                (),
            ));
        }
        println!("GNOS_LOCK idle_lock_secs={secs:?}");
    }
}

struct Text {
    fonts: cosmic_text::FontSystem,
    cache: cosmic_text::SwashCache,
}

impl Text {
    fn new() -> Self {
        Self {
            fonts: cosmic_text::FontSystem::new(),
            cache: cosmic_text::SwashCache::new(),
        }
    }

    /// Draw `text` centred on `cx`, its top at `y`; returns its height.
    fn centred(
        &mut self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        size: f32,
        cx: f32,
        y: f32,
        color: (u8, u8, u8),
    ) -> f32 {
        use cosmic_text::{Attrs, Buffer, Color, Family, Metrics, Shaping};
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * 1.25));
        buffer.set_size(Some(pixmap.width() as f32), None);
        buffer.set_text(
            text,
            &Attrs::new().family(Family::Name("IBM Plex Sans")),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        let width = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0_f32, f32::max);
        let left = (cx - width / 2.0).round() as i32;
        let top = y.round() as i32;
        let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
        let data = pixmap.data_mut();
        buffer.draw(
            &mut self.fonts,
            &mut self.cache,
            Color::rgb(color.0, color.1, color.2),
            |x, yy, w, h, c| {
                let a = c.a() as u32;
                if a == 0 {
                    return;
                }
                for dy in 0..h as i32 {
                    for dx in 0..w as i32 {
                        let (px, py) = (left + x + dx, top + yy + dy);
                        if px < 0 || py < 0 || px >= pw || py >= ph {
                            continue;
                        }
                        let i = ((py * pw + px) * 4) as usize;
                        for (k, v) in [c.r(), c.g(), c.b()].into_iter().enumerate() {
                            let old = data[i + k] as u32;
                            data[i + k] = ((v as u32 * a + old * (255 - a)) / 255) as u8;
                        }
                        data[i + 3] = 255;
                    }
                }
            },
        );
        size * 1.25
    }
}

impl Lockd {
    fn send(&self, event: LockEvent) {
        let _ = self.tx.send(Message::Event(event));
    }

    fn handle(&mut self, qh: &QueueHandle<Self>, event: LockEvent) {
        let effects = self.machine.handle(event);
        self.apply(qh, effects);
    }

    fn apply(&mut self, qh: &QueueHandle<Self>, effects: Vec<LockEffect>) {
        for effect in effects {
            match effect {
                LockEffect::RequestCompositorLock => match self.lock_state.lock(qh) {
                    Ok(lock) => self.lock = Some(lock),
                    Err(error) => {
                        println!("GNOS_LOCK state=refused error={error}");
                        self.handle(qh, LockEvent::CompositorFinished);
                    }
                },
                LockEffect::Authenticate { attempt, password } => {
                    let (tx, user) = (self.tx.clone(), self.user.clone());
                    std::thread::spawn(move || {
                        let success = authenticate(&user, password);
                        let _ = tx.send(Message::Event(LockEvent::AuthResult { attempt, success }));
                    });
                }
                LockEffect::SetLockedHint(true) => {
                    self.locked_since = Some(Instant::now());
                    self.screens_off = false;
                    let _ = self.logind.send(Logind::SetLockedHint(true));
                    let _ = self.logind.send(Logind::Locked);
                }
                LockEffect::SetLockedHint(false) => {
                    self.locked_since = None;
                    self.wake_screens();
                    let _ = self.logind.send(Logind::SetLockedHint(false));
                }
                LockEffect::Unlock => {
                    if let Some(lock) = self.lock.take() {
                        lock.unlock();
                    }
                    self.surfaces.clear();
                    println!("GNOS_LOCK state=unlocked");
                }
                LockEffect::Redraw => self.draw_all(),
            }
        }
    }

    fn add_surface(&mut self, qh: &QueueHandle<Self>, output: &wl_output::WlOutput) {
        let Some(lock) = &self.lock else { return };
        let surface = self.compositor.create_surface(qh);
        let lock_surface = lock.create_lock_surface(surface, output, qh);
        self.surfaces.push(Surface {
            lock_surface,
            size: (0, 0),
            pool: None,
        });
    }

    /// Switch the screens off (`hyprctl`, DPMS) once they have been locked
    /// for `screens_off_secs`; any key wakes them and restarts the timer.
    fn screens_off_if_due(&mut self) {
        let (Some(since), Some(secs)) = (self.locked_since, self.machine.config().screens_off_secs)
        else {
            return;
        };
        if !self.screens_off && since.elapsed() >= Duration::from_secs(secs.into()) {
            self.screens_off = true;
            println!("GNOS_LOCK screens=off");
            dpms(false);
        }
    }

    fn wake_screens(&mut self) {
        if self.screens_off {
            self.screens_off = false;
            println!("GNOS_LOCK screens=on");
            dpms(true);
        }
    }

    /// Read the settings again when their file changed. A lock in progress
    /// keeps its own (`Machine::set_config`); the idle delay and the sleep
    /// inhibitor follow at once.
    fn reload(&mut self, qh: &QueueHandle<Self>) {
        let Some(path) = self.config_path.clone() else {
            return;
        };
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if modified == self.config_modified {
            return;
        }
        self.config_modified = modified;
        let config = gnos_lock::load(&path);
        println!("GNOS_LOCK config=reloaded idle={:?}", config.idle_lock_secs);
        let _ = self
            .logind
            .send(Logind::LockBeforeSleep(config.lock_before_sleep));
        if let Some(idle) = self.idle.as_mut() {
            idle.arm(config.idle_lock_secs, qh);
        }
        self.machine.set_config(config);
    }

    fn clock_text(&self) -> (String, String) {
        let now = chrono::Local::now();
        let config = self.machine.config();
        let hour24 = config.clock_24h.unwrap_or_else(|| {
            matches!(
                gnos_clock::locale_clock_cycle(),
                gnos_clock::ClockCycle::TwentyFourHour
            )
        });
        let time = if hour24 {
            now.format("%H:%M").to_string()
        } else {
            now.format("%-I:%M %p").to_string()
        };
        // The names, translated: "%A" and "%B" are English in chrono.
        let weekday = t(WEEKDAYS[now.weekday().num_days_from_monday() as usize]);
        let month = t(MONTHS[now.month0() as usize]);
        (time, format!("{weekday} {} {month}", now.day()))
    }

    fn draw_all(&mut self) {
        for index in 0..self.surfaces.len() {
            self.draw(index);
        }
    }

    fn draw(&mut self, index: usize) {
        let (width, height) = self.surfaces[index].size;
        if width == 0 || height == 0 {
            return;
        }
        let config = self.machine.config().clone();
        let (time, date) = self.clock_text();
        let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("a lock surface has a size");
        pixmap.fill(tiny_skia::Color::from_rgba8(16, 16, 20, 255));
        let (cx, mut y) = (width as f32 / 2.0, height as f32 * 0.28);
        let text = &mut self.text;
        let soft = (190, 188, 205);
        if config.show_clock {
            y += text.centred(&mut pixmap, &time, 96.0, cx, y, (236, 235, 245));
        }
        if config.show_date {
            y += text.centred(&mut pixmap, &date, 24.0, cx, y, soft);
        }
        if !config.message.is_empty() {
            text.centred(&mut pixmap, &config.message, 18.0, cx, y + 12.0, soft);
        }
        y = height as f32 * 0.58;
        if config.show_user {
            y += text.centred(&mut pixmap, &self.user, 22.0, cx, y, (236, 235, 245));
        }
        let prompt = match self.machine.phase() {
            LockPhase::Authenticating => t("Checking…").to_owned(),
            LockPhase::Waiting => match self.machine.notice() {
                Some(Notice::Waiting { secs_left }) => tf(
                    "Too many attempts. Wait {secs} s",
                    &[("secs", &secs_left.to_string())],
                ),
                _ => t("Wait").to_owned(),
            },
            _ => {
                let dots = self.machine.dots();
                if dots == 0 {
                    t("Password").to_owned()
                } else {
                    "•".repeat(dots.min(40))
                }
            }
        };
        y += 8.0 + text.centred(&mut pixmap, &prompt, 22.0, cx, y + 8.0, (179, 171, 255));
        if self.machine.notice() == Some(Notice::Wrong) {
            text.centred(
                &mut pixmap,
                t("Wrong password"),
                18.0,
                cx,
                y + 8.0,
                (240, 150, 150),
            );
        }
        let surface = &mut self.surfaces[index];
        let stride = width as i32 * 4;
        let pool = surface.pool.get_or_insert_with(|| {
            SlotPool::new((stride * height as i32) as usize, &self.shm).expect("shm pool")
        });
        let Ok((buffer, canvas)) = pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Xrgb8888,
        ) else {
            return;
        };
        for (dst, src) in canvas
            .chunks_exact_mut(4)
            .zip(pixmap.data().chunks_exact(4))
        {
            // tiny-skia is RGBA; Xrgb8888 is BGRX in memory.
            dst.copy_from_slice(&[src[2], src[1], src[0], 255]);
        }
        let wl = surface.lock_surface.wl_surface();
        wl.damage_buffer(0, 0, width as i32, height as i32);
        if buffer.attach_to(wl).is_ok() {
            wl.commit();
        }
        self.last_drawn = format!("{time}{date}");
    }
}

/// DPMS through Hyprland's own dispatcher (the instance in this process's
/// environment). Fire and forget; a compositor that does not know it just
/// leaves the screens on.
fn dpms(on: bool) {
    let action = if on { "on" } else { "off" };
    std::thread::spawn(move || {
        let _ = std::process::Command::new("hyprctl")
            .args([
                "dispatch",
                &format!("hl.dsp.dpms({{ action = \"{action}\" }})"),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// PAM, service `gnosix-lock`: authenticate, then the account check.
fn authenticate(user: &str, password: String) -> bool {
    use pam_client::{Context, Flag, conv_mock::Conversation};
    let mut context = match Context::new(
        &pam_service(),
        Some(user),
        Conversation::with_credentials(user, password),
    ) {
        Ok(context) => context,
        Err(error) => {
            println!("GNOS_LOCK pam=unavailable error={error}");
            return false;
        }
    };
    let ok = context.authenticate(Flag::NONE).is_ok() && context.acct_mgmt(Flag::NONE).is_ok();
    println!("GNOS_LOCK pam={}", if ok { "accepted" } else { "refused" });
    ok
}

impl CompositorHandler for Lockd {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Lockd {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        // An output that appears while locked gets its own lock surface.
        if self.machine.phase() != LockPhase::Unlocked && self.lock.is_some() {
            self.add_surface(qh, &output);
        }
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl SessionLockHandler for Lockd {
    fn locked(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: SessionLock) {
        println!("GNOS_LOCK state=locked");
        let outputs: Vec<_> = self.outputs.outputs().collect();
        for output in &outputs {
            self.add_surface(qh, output);
        }
        self.handle(qh, LockEvent::CompositorLocked);
    }
    fn finished(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: SessionLock) {
        println!("GNOS_LOCK state=finished");
        self.lock = None;
        self.surfaces.clear();
        self.handle(qh, LockEvent::CompositorFinished);
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _: u32,
    ) {
        if let Some(index) = self
            .surfaces
            .iter()
            .position(|s| s.lock_surface.wl_surface() == surface.wl_surface())
        {
            self.surfaces[index].size = configure.new_size;
            self.surfaces[index].pool = None;
            self.draw(index);
        }
    }
}

impl SeatHandler for Lockd {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Lockd {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(qh, event);
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(qh, event);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: smithay_client_toolkit::seat::keyboard::RawModifiers,
        _: u32,
    ) {
    }
    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: RepeatInfo,
    ) {
    }
}

impl Lockd {
    fn key(&mut self, qh: &QueueHandle<Self>, event: KeyEvent) {
        // A key wakes the screens and restarts the screens-off timer.
        if self.machine.is_locked() {
            self.wake_screens();
            self.locked_since = Some(Instant::now());
        }
        let key = event.keysym;
        let lock_event = if key == Keysym::Return || key == Keysym::KP_Enter {
            Some(LockEvent::Submit)
        } else if key == Keysym::BackSpace {
            Some(LockEvent::Backspace)
        } else if key == Keysym::Escape {
            Some(LockEvent::Clear)
        } else {
            None
        };
        if let Some(lock_event) = lock_event {
            self.handle(qh, lock_event);
            return;
        }
        for c in event.utf8.iter().flat_map(|s| s.chars()) {
            self.handle(qh, LockEvent::Char(c));
        }
    }
}

impl ShmHandler for Lockd {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Lockd {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<ExtIdleNotificationV1, ()> for Lockd {
    fn event(
        state: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, ext_idle_notification_v1::Event::Idled) {
            println!("GNOS_LOCK request=idle");
            state.send(LockEvent::LockRequested);
        }
    }
}
delegate_noop!(Lockd: ExtIdleNotifierV1);

delegate_compositor!(Lockd);
delegate_output!(Lockd);
delegate_seat!(Lockd);
delegate_keyboard!(Lockd);
delegate_shm!(Lockd);
delegate_session_lock!(Lockd);
delegate_registry!(Lockd);

/// logind, on its own thread: lock requests for this session, the locked
/// hint, and a delay inhibitor so the lock is up before the system sleeps.
fn logind_thread(
    mut config: LockConfig,
    tx: mpsc::Sender<Message>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Logind>,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async move {
        use futures::StreamExt;
        let Ok(bus) = zbus::Connection::system().await else {
            println!("GNOS_LOCK logind=unavailable");
            return;
        };
        let manager = |method: &'static str| {
            let bus = bus.clone();
            async move {
                bus.call_method(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    Some("org.freedesktop.login1.Manager"),
                    method,
                    &"auto",
                )
                .await
            }
        };
        let Ok(reply) = manager("GetSession").await else {
            println!("GNOS_LOCK logind=no_session");
            return;
        };
        let Ok(session) = reply
            .body()
            .deserialize::<zbus::zvariant::OwnedObjectPath>()
        else {
            return;
        };
        println!("GNOS_LOCK logind=session path={session}");
        let lock_rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.login1.Session")
            .unwrap()
            .member("Lock")
            .unwrap()
            .path(session.clone())
            .unwrap()
            .build();
        let sleep_rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.login1.Manager")
            .unwrap()
            .member("PrepareForSleep")
            .unwrap()
            .build();
        let mut locks = zbus::MessageStream::for_match_rule(lock_rule, &bus, None)
            .await
            .unwrap();
        let mut sleeps = zbus::MessageStream::for_match_rule(sleep_rule, &bus, None)
            .await
            .unwrap();
        let inhibit = || {
            let bus = bus.clone();
            async move {
                bus.call_method(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    Some("org.freedesktop.login1.Manager"),
                    "Inhibit",
                    &(
                        "sleep",
                        "Gnosix lock",
                        "Lock the screen before sleep",
                        "delay",
                    ),
                )
                .await
                .ok()
                .and_then(|r| r.body().deserialize::<zbus::zvariant::OwnedFd>().ok())
            }
        };
        let mut inhibitor = if config.lock_before_sleep {
            inhibit().await
        } else {
            None
        };
        // The inhibitor is held all the time; it is released only when the
        // system is about to sleep and the lock is up.
        let mut sleeping = false;
        loop {
            tokio::select! {
                Some(Ok(_)) = locks.next() => {
                    println!("GNOS_LOCK request=logind");
                    let _ = tx.send(Message::Event(LockEvent::LockRequested));
                }
                Some(Ok(message)) = sleeps.next() => {
                    let going: bool = message.body().deserialize().unwrap_or(false);
                    sleeping = going;
                    if going && config.lock_before_sleep {
                        println!("GNOS_LOCK request=sleep");
                        let _ = tx.send(Message::Event(LockEvent::LockRequested));
                        // Held until the lock is up (logind waits up to its
                        // InhibitDelayMaxSec).
                    } else if !going && config.lock_before_sleep && inhibitor.is_none() {
                        inhibitor = inhibit().await;
                    }
                }
                Some(command) = rx.recv() => match command {
                    Logind::LockBeforeSleep(on) => {
                        config.lock_before_sleep = on;
                        if on && inhibitor.is_none() && !sleeping {
                            inhibitor = inhibit().await;
                        } else if !on {
                            inhibitor = None;
                        }
                    }
                    Logind::SetLockedHint(hint) => {
                        let _ = bus.call_method(
                            Some("org.freedesktop.login1"),
                            session.as_str(),
                            Some("org.freedesktop.login1.Session"),
                            "SetLockedHint",
                            &hint,
                        ).await;
                    }
                    Logind::Locked => {
                        // The lock is up: if the system is going to sleep,
                        // let it. An ordinary lock keeps the inhibitor.
                        if sleeping {
                            inhibitor = None;
                        }
                    }
                },
                else => break,
            }
        }
    });
}

fn main() {
    gnos_i18n::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lock_now = args.iter().any(|a| a == "--lock-now");
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .or_else(gnos_lock::default_path);
    let config = config_path
        .as_deref()
        .map(gnos_lock::load)
        .unwrap_or_default();
    let config_modified = config_path
        .as_deref()
        .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let user = current_user();

    let conn = Connection::connect_to_env().expect("a Wayland compositor");
    let (globals, queue) = registry_queue_init::<Lockd>(&conn).expect("the registry");
    let qh = queue.handle();
    let mut event_loop: calloop::EventLoop<Lockd> = calloop::EventLoop::try_new().expect("loop");
    let (tx, rx) = mpsc::channel::<Message>();
    let (logind_tx, logind_rx) = tokio::sync::mpsc::unbounded_channel();
    {
        let (config, tx) = (config.clone(), tx.clone());
        std::thread::spawn(move || logind_thread(config, tx, logind_rx));
    }
    let seats = SeatState::new(&globals, &qh);
    let mut lockd = Lockd {
        registry: RegistryState::new(&globals),
        seats,
        outputs: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh).expect("wl_compositor"),
        shm: Shm::bind(&globals, &qh).expect("wl_shm"),
        lock_state: SessionLockState::new(&globals, &qh),
        lock: None,
        surfaces: Vec::new(),
        machine: Machine::new(config.clone()),
        user,
        tx: tx.clone(),
        logind: logind_tx,
        keyboard: None,
        config_path,
        config_modified,
        idle: None,
        locked_since: None,
        screens_off: false,
        text: Text::new(),
        exit: false,
        tick: false,
        last_drawn: String::new(),
    };

    if let Ok(notifier) = globals.bind::<ExtIdleNotifierV1, _, _>(&qh, 1..=2, ())
        && let Some(seat) = lockd.seats.seats().next()
    {
        let mut idle = Idle {
            notifier,
            seat,
            notification: None,
            secs: None,
        };
        idle.arm(config.idle_lock_secs, &qh);
        lockd.idle = Some(idle);
    }

    calloop_wayland_source::WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .expect("wayland source");
    // A one-second tick: the wait after failures, and the clock.
    event_loop
        .handle()
        .insert_source(
            calloop::timer::Timer::from_duration(Duration::from_secs(1)),
            |_, _, lockd| {
                lockd.tick = true;
                calloop::timer::TimeoutAction::ToDuration(Duration::from_secs(1))
            },
        )
        .expect("timer");

    println!("GNOS_LOCK state=ready user={}", lockd.user);
    if lock_now {
        lockd.send(LockEvent::LockRequested);
    }
    let mut event_queue_handle = qh;
    while !lockd.exit {
        // Everything the other threads said.
        while let Ok(Message::Event(event)) = rx.try_recv() {
            lockd.handle(&event_queue_handle, event);
        }
        if lockd.tick && !lockd.machine.is_locked() {
            lockd.reload(&event_queue_handle);
        }
        if lockd.tick && lockd.machine.is_locked() {
            lockd.screens_off_if_due();
        }
        if std::mem::take(&mut lockd.tick) && lockd.machine.is_locked() {
            lockd.handle(&event_queue_handle, LockEvent::Tick(Instant::now()));
            let (time, date) = lockd.clock_text();
            if lockd.last_drawn != format!("{time}{date}") {
                lockd.draw_all();
            }
        }
        event_loop
            .dispatch(Duration::from_millis(100), &mut lockd)
            .expect("dispatch");
        let _ = &mut event_queue_handle;
    }
}
