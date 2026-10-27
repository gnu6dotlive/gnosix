//! Process-global, read-only observation sessions for Clock and Status facts.
//!
//! Each source owns one observer thread per process, exactly as the network
//! session does: the observer blocks on its provider (a realtime timer, the
//! Hyprland socket, the system bus) off the compositor thread, and typed
//! values cross to GPUI over an unbounded channel. A session holds only the
//! latest value and notifies only when it changes; `ProductRuntime` compares
//! again. No session can mutate its provider.

use futures::{StreamExt, channel::mpsc};
use gpui::{AppContext, Context, Entity};
use std::marker::PhantomData;

/// One kind of observed fact and how to observe it.
pub trait StatusSource: 'static {
    type Value: Clone + PartialEq + Send + 'static;
    const THREAD: &'static str;
    /// The truth before the first observation. Never a fabricated default.
    fn unobserved() -> Self::Value;
    /// Run the observer on the current (dedicated) thread, sending every new
    /// value. Returning means the provider could not be observed at all.
    fn observe(send: &mut dyn FnMut(Self::Value)) -> Result<(), String>;
}

pub struct StatusSession<S: StatusSource> {
    value: S::Value,
    started: bool,
    _source: PhantomData<S>,
}

struct SessionGlobal<S: StatusSource>(Entity<StatusSession<S>>);
impl<S: StatusSource> gpui::Global for SessionGlobal<S> {}

impl<S: StatusSource> StatusSession<S> {
    fn new() -> Self {
        Self {
            value: S::unobserved(),
            started: false,
            _source: PhantomData,
        }
    }

    pub fn shared(cx: &mut gpui::App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<SessionGlobal<S>>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self::new());
        cx.set_global(SessionGlobal::<S>(entity.clone()));
        entity
    }

    pub fn value(&self) -> &S::Value {
        &self.value
    }

    pub const fn started(&self) -> bool {
        self.started
    }

    fn set_value(&mut self, value: S::Value, cx: &mut Context<Self>) {
        if self.value == value {
            return;
        }
        self.value = value;
        cx.notify();
    }
}

/// Start the source's observer once per process.
pub fn ensure_observation<S: StatusSource>(session: &Entity<StatusSession<S>>, cx: &mut gpui::App) {
    session.update(cx, |this, cx| {
        if this.started {
            return;
        }
        this.started = true;
        let entity = cx.entity();
        let (sender, mut receiver) = mpsc::unbounded::<S::Value>();
        let spawn = std::thread::Builder::new()
            .name(S::THREAD.into())
            .spawn(move || {
                let thread_sender = sender.clone();
                let mut send = move |value| {
                    let _ = thread_sender.unbounded_send(value);
                };
                if let Err(error) = S::observe(&mut send) {
                    // Could not observe at all: report that, never a default.
                    eprintln!(
                        "GNOS_UX_STATUS_OBSERVER_FAIL source={} error={error}",
                        S::THREAD
                    );
                    let _ = sender.unbounded_send(S::unobserved());
                }
            });
        if let Err(error) = spawn {
            this.started = false;
            eprintln!(
                "GNOS_UX_STATUS_THREAD_FAIL source={} error={error}",
                S::THREAD
            );
            return;
        }
        cx.spawn(async move |_this, task_cx| {
            while let Some(value) = receiver.next().await {
                entity.update(&mut *task_cx, |this, cx| this.set_value(value, cx));
            }
        })
        .detach();
    });
}

/// Local civil time, once per local minute.
pub struct ClockSource;
impl StatusSource for ClockSource {
    type Value = Option<gnos_clock::ClockObservation>;
    const THREAD: &'static str = "gnos-clock-observer";
    fn unobserved() -> Self::Value {
        None
    }
    fn observe(send: &mut dyn FnMut(Self::Value)) -> Result<(), String> {
        gnos_clock::run_minute_observer(|observation| send(Some(observation)))
            .map_err(|error| error.to_string())
    }
}

/// Primary-keyboard Caps/Num Lock from Hyprland.
pub struct KeyboardLockSource;
impl StatusSource for KeyboardLockSource {
    type Value = gnosix_hyprland::keyboard_lock::KeyboardLockObservation;
    const THREAD: &'static str = "gnos-keyboard-lock-observer";
    fn unobserved() -> Self::Value {
        gnosix_hyprland::keyboard_lock::KeyboardLockObservation::Unavailable
    }
    fn observe(send: &mut dyn FnMut(Self::Value)) -> Result<(), String> {
        gnosix_hyprland::keyboard_lock::run_keyboard_lock_observer(send)
    }
}

/// BlueZ adapters and devices.
pub struct BluetoothSource;
impl StatusSource for BluetoothSource {
    type Value = gnos_bluetooth::BluetoothSnapshot;
    const THREAD: &'static str = "gnos-bluetooth-observer";
    fn unobserved() -> Self::Value {
        gnos_bluetooth::BluetoothSnapshot {
            provider: gnos_bluetooth::BluetoothProviderState::Unavailable,
            generation: 0,
            provider_owner: None,
            adapters: Default::default(),
            devices: Default::default(),
        }
    }
    fn observe(send: &mut dyn FnMut(Self::Value)) -> Result<(), String> {
        // The BlueZ runner requires a `Send` callback; its snapshots cross
        // through a channel to this thread's `send`.
        let (sender, receiver) = std::sync::mpsc::channel();
        let runner = std::thread::Builder::new()
            .name("gnos-bluetooth-runner".into())
            .spawn(move || {
                gnos_bluetooth::run_observer(move |observation| {
                    let _ = sender.send(observation.snapshot);
                })
                .map_err(|error| error.to_string())
            })
            .map_err(|error| error.to_string())?;
        for snapshot in receiver {
            send(snapshot);
        }
        runner
            .join()
            .map_err(|_| "bluetooth runner panicked".to_owned())?
    }
}

/// UPower battery or power-profiles-daemon profile.
pub struct PowerSource;
impl StatusSource for PowerSource {
    type Value = gnos_power::PowerSnapshot;
    const THREAD: &'static str = "gnos-power-observer";
    fn unobserved() -> Self::Value {
        gnos_power::PowerSnapshot::Unavailable
    }
    fn observe(send: &mut dyn FnMut(Self::Value)) -> Result<(), String> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let runner = std::thread::Builder::new()
            .name("gnos-power-runner".into())
            .spawn(move || {
                gnos_power::run_observer(move |observation| {
                    let _ = sender.send(observation.snapshot);
                })
                .map_err(|error| error.to_string())
            })
            .map_err(|error| error.to_string())?;
        for snapshot in receiver {
            send(snapshot);
        }
        runner
            .join()
            .map_err(|_| "power runner panicked".to_owned())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_start_unobserved_and_not_started() {
        assert_eq!(*StatusSession::<ClockSource>::new().value(), None);
        assert_eq!(
            *StatusSession::<KeyboardLockSource>::new().value(),
            gnosix_hyprland::keyboard_lock::KeyboardLockObservation::Unavailable
        );
        assert_eq!(
            StatusSession::<BluetoothSource>::new().value().provider,
            gnos_bluetooth::BluetoothProviderState::Unavailable
        );
        assert_eq!(
            *StatusSession::<PowerSource>::new().value(),
            gnos_power::PowerSnapshot::Unavailable
        );
        assert!(!StatusSession::<PowerSource>::new().started());
    }
}
