//! Controlled Clock + Status qualification cases for the reference Bar lab.
//!
//! Every case is a set of typed provider snapshots written into a
//! `ProductRuntime`; the Clock and Status presentation is then derived by the
//! production adapter, `BarPresentation::from_product_runtime`. Fixtures stop
//! at the provider boundary, so each capture exercises the real projection,
//! geometry and native renderer. None of this is reachable from production.

use crate::{
    product_clock::{ClockCycle, ClockObservation, LocalClockSample},
    product_runtime::ProductRuntime,
    reference_bar::BarPresentation,
};
use gnos_bluetooth::{
    BluetoothAdapterSnapshot, BluetoothDeviceSnapshot, BluetoothObjectRef, BluetoothPowerState,
    BluetoothProviderState, BluetoothSnapshot,
};
use gnos_power::{BatteryPercentage, PowerProfile, PowerSnapshot};
use gnosix_hyprland::keyboard_lock::{KeyboardLockObservation, KeyboardLockSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusCase {
    AllUnavailable,
    Clock12h2207,
    Clock24hMidnight,
    Clock12hMidnight,
    Clock12hNoon,
    Clock24hNoon,
    LocksBoth,
    NetworkOnly,
    BluetoothDevice,
    BatteryNormal,
    BatteryCritical,
    NoBatteryProfile,
    Mixed,
}

impl StatusCase {
    pub const ALL: [Self; 13] = [
        Self::AllUnavailable,
        Self::Clock12h2207,
        Self::Clock24hMidnight,
        Self::Clock12hMidnight,
        Self::Clock12hNoon,
        Self::Clock24hNoon,
        Self::LocksBoth,
        Self::NetworkOnly,
        Self::BluetoothDevice,
        Self::BatteryNormal,
        Self::BatteryCritical,
        Self::NoBatteryProfile,
        Self::Mixed,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::AllUnavailable => "all-unavailable",
            Self::Clock12h2207 => "clock-12h-2207",
            Self::Clock24hMidnight => "clock-24h-midnight",
            Self::Clock12hMidnight => "clock-12h-midnight",
            Self::Clock12hNoon => "clock-12h-noon",
            Self::Clock24hNoon => "clock-24h-noon",
            Self::LocksBoth => "locks-both",
            Self::NetworkOnly => "network-only",
            Self::BluetoothDevice => "bluetooth-device",
            Self::BatteryNormal => "battery-normal",
            Self::BatteryCritical => "battery-critical",
            Self::NoBatteryProfile => "no-battery-profile",
            Self::Mixed => "mixed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|case| case.name() == value)
    }

    /// The typed provider facts of this case.
    pub fn runtime(self) -> ProductRuntime {
        let mut runtime = ProductRuntime::default();
        let clock = |hour, minute, cycle| {
            Some(ClockObservation {
                time: LocalClockSample::new(hour, minute).expect("valid fixture time"),
                cycle,
            })
        };
        let locks = KeyboardLockObservation::Observed(KeyboardLockSnapshot {
            caps_lock: true,
            num_lock: true,
        });
        match self {
            Self::AllUnavailable => {}
            Self::Clock12h2207 => {
                runtime.set_clock(clock(22, 7, ClockCycle::TwelveHour));
            }
            Self::Clock24hMidnight => {
                runtime.set_clock(clock(0, 0, ClockCycle::TwentyFourHour));
            }
            Self::Clock12hMidnight => {
                runtime.set_clock(clock(0, 0, ClockCycle::TwelveHour));
            }
            Self::Clock12hNoon => {
                runtime.set_clock(clock(12, 0, ClockCycle::TwelveHour));
            }
            Self::Clock24hNoon => {
                runtime.set_clock(clock(12, 0, ClockCycle::TwentyFourHour));
            }
            Self::LocksBoth => {
                runtime.set_keyboard_locks(locks);
            }
            Self::NetworkOnly => {
                runtime.set_network(crate::product_lab::wifi_ready_network(85));
            }
            Self::BluetoothDevice => {
                runtime.set_bluetooth(bluetooth_with_headset());
            }
            Self::BatteryNormal => {
                runtime.set_power(battery(60.0, false, true));
            }
            Self::BatteryCritical => {
                runtime.set_power(battery(15.0, false, true));
            }
            Self::NoBatteryProfile => {
                runtime.set_power(PowerSnapshot::Profile(PowerProfile::Performance));
            }
            Self::Mixed => {
                runtime.set_clock(clock(22, 7, ClockCycle::TwelveHour));
                runtime.set_keyboard_locks(locks);
                runtime.set_network(crate::product_lab::wifi_ready_network(85));
                runtime.set_bluetooth(bluetooth_with_headset());
                runtime.set_power(battery(60.0, true, false));
            }
        }
        runtime
    }

    /// The frozen reference presentation with Clock and Status replaced by
    /// what the production adapter derives from this case's runtime.
    pub fn apply(self, presentation: &mut BarPresentation) {
        let derived = BarPresentation::from_product_runtime(&self.runtime());
        presentation.clock = derived.clock;
        presentation.status = derived.status;
    }
}

fn battery(percent: f64, charging: bool, on_battery: bool) -> PowerSnapshot {
    PowerSnapshot::Battery {
        percentage: BatteryPercentage::new(percent).expect("valid fixture percentage"),
        charging,
        on_battery,
    }
}

fn bluetooth_with_headset() -> BluetoothSnapshot {
    const OWNER: &str = ":1.42";
    let adapter = BluetoothObjectRef::new(OWNER, "/org/bluez/hci0");
    let device = BluetoothObjectRef::new(OWNER, "/org/bluez/hci0/dev_00_1B_66_00_00_01");
    BluetoothSnapshot {
        provider: BluetoothProviderState::Available,
        generation: 1,
        provider_owner: Some(OWNER.into()),
        adapters: [(
            adapter.clone(),
            BluetoothAdapterSnapshot {
                id: adapter.clone(),
                powered: true,
                power_state: BluetoothPowerState::On,
                discovering: false,
                pairable: false,
                discoverable: false,
            },
        )]
        .into(),
        devices: [(
            device.clone(),
            BluetoothDeviceSnapshot {
                id: device,
                adapter,
                alias: None,
                paired: true,
                bonded: true,
                connected: true,
                trusted: true,
                blocked: false,
                services_resolved: true,
                rssi: None,
                icon: Some("audio-headset".into()),
            },
        )]
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_bar::{
        clock_module_height, describe_clock_and_status, status_module_height,
    };

    fn heights(case: StatusCase) -> (Option<f32>, Option<f32>) {
        let presentation = BarPresentation::from_product_runtime(&case.runtime());
        (
            presentation.clock.as_ref().map(clock_module_height),
            presentation.status.as_ref().map(status_module_height),
        )
    }

    #[test]
    fn geometry_follows_the_facts_actually_present() {
        use StatusCase as C;
        // No provider: no clock module, and the enabled Status module keeps
        // only its padding.
        assert_eq!(heights(C::AllUnavailable), (None, Some(24.0)));
        assert_eq!(heights(C::Clock12h2207), (Some(105.0), Some(24.0)));
        assert_eq!(heights(C::Clock12hMidnight), (Some(105.0), Some(24.0)));
        assert_eq!(heights(C::Clock24hMidnight), (Some(91.0), Some(24.0)));
        assert_eq!(heights(C::LocksBoth).1, Some(78.0));
        assert_eq!(heights(C::NetworkOnly).1, Some(48.0));
        assert_eq!(heights(C::BluetoothDevice).1, Some(78.0));
        assert_eq!(heights(C::BatteryNormal).1, Some(48.0));
        assert_eq!(heights(C::NoBatteryProfile).1, Some(48.0));
        // locks 54 + network 24 + bluetooth 54 + battery 24 + 3 gaps + padding.
        assert_eq!(heights(C::Mixed), (Some(105.0), Some(198.0)));
    }

    #[test]
    fn every_case_is_described_from_the_runtime() {
        let described = |case: StatusCase| {
            describe_clock_and_status(&BarPresentation::from_product_runtime(&case.runtime()))
        };
        assert!(described(StatusCase::AllUnavailable).starts_with("clock=absent "));
        assert!(described(StatusCase::Clock12h2207).starts_with("clock=10:07pm "));
        assert!(described(StatusCase::Clock24hMidnight).starts_with("clock=00:00 "));
        assert!(described(StatusCase::Clock12hMidnight).starts_with("clock=12:00am "));
        assert!(described(StatusCase::Clock12hNoon).starts_with("clock=12:00pm "));
        assert!(described(StatusCase::Clock24hNoon).starts_with("clock=12:00 "));
        assert!(described(StatusCase::BatteryCritical).contains("error=true"));
        assert!(described(StatusCase::BatteryNormal).contains("error=false"));
        assert!(
            described(StatusCase::BluetoothDevice)
                .contains("bluetooth(bluetooth-connected;bluetooth-device-headphones)")
        );
        for case in StatusCase::ALL {
            assert_eq!(StatusCase::parse(case.name()), Some(case));
        }
    }
}
