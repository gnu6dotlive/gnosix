//! UPower and power-profiles-daemon facts, and one power: the active
//! profile (SETTINGS-02, [`request_profile`]).
//!
//! Unavailable and unsupported states are explicit. No battery commands,
//! service activation or unbounded polling are performed.

use futures::StreamExt;
use std::{fmt, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}
/// Exact validated UPower Percentage fact, in UPower's 0..=100 domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryPercentage(u64);

impl BatteryPercentage {
    pub fn new(percent: f64) -> Option<Self> {
        if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
            return None;
        }
        // Canonicalise signed zero so observational equality does not invent a
        // change that the donor could never display.
        let percent = if percent == 0.0 { 0.0 } else { percent };
        Some(Self(percent.to_bits()))
    }

    pub fn as_percent(self) -> f64 {
        f64::from_bits(self.0)
    }

    pub fn as_fraction(self) -> f64 {
        self.as_percent() / 100.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSnapshot {
    Unavailable,
    /// UPower observed no laptop battery, and the active profile was observed.
    Profile(PowerProfile),
    /// UPower observed no laptop battery but no usable profile was available.
    NoBatteryProfileUnavailable,
    Battery {
        percentage: BatteryPercentage,
        charging: bool,
        on_battery: bool,
    },
    /// The display device is a present laptop battery with unsupported state.
    BatteryStateUnsupported,
}
pub fn parse_profile(raw: &str) -> Option<PowerProfile> {
    match raw {
        "power-saver" => Some(PowerProfile::PowerSaver),
        "balanced" => Some(PowerProfile::Balanced),
        "performance" => Some(PowerProfile::Performance),
        _ => None,
    }
}
/// The profile's name on the bus.
pub fn profile_name(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::PowerSaver => "power-saver",
        PowerProfile::Balanced => "balanced",
        PowerProfile::Performance => "performance",
    }
}

/// The next profile, in the order power-saver, balanced, performance.
pub fn next_profile(profile: PowerProfile) -> PowerProfile {
    match profile {
        PowerProfile::PowerSaver => PowerProfile::Balanced,
        PowerProfile::Balanced => PowerProfile::Performance,
        PowerProfile::Performance => PowerProfile::PowerSaver,
    }
}

/// SETTINGS-02: ask power-profiles-daemon (on `bus`, the system bus) for
/// `profile`. The change is seen by observation, never assumed.
pub async fn request_profile(bus: &zbus::Connection, profile: PowerProfile) -> Result<(), String> {
    bus.call_method(
        Some(PROFILES_BUS_NAME),
        PROFILES_PATH,
        Some("org.freedesktop.DBus.Properties"),
        "Set",
        &(
            PROFILES_BUS_NAME,
            "ActiveProfile",
            zbus::zvariant::Value::from(profile_name(profile)),
        ),
    )
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

/// Pure reduction of UPower Type/IsPresent/Percentage/State/OnBattery and
/// power-profiles-daemon ActiveProfile. A missing fact is never false.
pub fn project_power(
    kind: Option<u32>,
    present: Option<bool>,
    percentage: Option<f64>,
    state: Option<u32>,
    on_battery: Option<bool>,
    profile: Option<&str>,
) -> PowerSnapshot {
    match (kind, present) {
        (Some(2), Some(true)) => {
            let (Some(percentage), Some(state), Some(on_battery)) = (percentage, state, on_battery)
            else {
                return PowerSnapshot::Unavailable;
            };
            let Some(percentage) = BatteryPercentage::new(percentage) else {
                return PowerSnapshot::Unavailable;
            };
            // UPower state: Charging=1, Discharging=2, Empty=3,
            // FullyCharged=4, PendingCharge=5, PendingDischarge=6.
            let charging = match state {
                1 | 4 | 5 => true,
                2 | 3 | 6 => false,
                _ => return PowerSnapshot::BatteryStateUnsupported,
            };
            PowerSnapshot::Battery {
                percentage,
                charging,
                on_battery,
            }
        }
        (Some(0), Some(false)) | (Some(2), Some(false)) => profile.and_then(parse_profile).map_or(
            PowerSnapshot::NoBatteryProfileUnavailable,
            PowerSnapshot::Profile,
        ),
        _ => PowerSnapshot::Unavailable,
    }
}

pub const UPOWER_BUS_NAME: &str = "org.freedesktop.UPower";
pub const UPOWER_PATH: &str = "/org/freedesktop/UPower";
pub const UPOWER_DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
pub const PROFILES_BUS_NAME: &str = "net.hadess.PowerProfiles";
pub const PROFILES_PATH: &str = "/net/hadess/PowerProfiles";

#[derive(Debug)]
pub enum PowerObserverError {
    Connect(String),
    Subscribe(String),
}

impl fmt::Display for PowerObserverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(error) => write!(f, "connect: {error}"),
            Self::Subscribe(error) => write!(f, "subscribe: {error}"),
        }
    }
}

impl std::error::Error for PowerObserverError {}

/// One uncached read of every fact `project_power` needs, over `bus`.
/// A service that is absent, or a property it does not have, is `None`.
pub async fn read_power(bus: &zbus::Connection) -> PowerSnapshot {
    async fn property<T>(
        bus: &zbus::Connection,
        destination: &'static str,
        path: &'static str,
        interface: &'static str,
        name: &'static str,
    ) -> Option<T>
    where
        T: TryFrom<zbus::zvariant::OwnedValue>,
        T::Error: Into<zbus::Error>,
    {
        let proxy = zbus::proxy::Builder::<zbus::Proxy<'_>>::new(bus)
            .destination(destination)
            .ok()?
            .path(path)
            .ok()?
            .interface(interface)
            .ok()?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .ok()?;
        proxy.get_property::<T>(name).await.ok()
    }
    const DEVICE: &str = "org.freedesktop.UPower.Device";
    let kind = property::<u32>(
        bus,
        UPOWER_BUS_NAME,
        UPOWER_DISPLAY_DEVICE_PATH,
        DEVICE,
        "Type",
    )
    .await;
    let present = property::<bool>(
        bus,
        UPOWER_BUS_NAME,
        UPOWER_DISPLAY_DEVICE_PATH,
        DEVICE,
        "IsPresent",
    )
    .await;
    let percentage = property::<f64>(
        bus,
        UPOWER_BUS_NAME,
        UPOWER_DISPLAY_DEVICE_PATH,
        DEVICE,
        "Percentage",
    )
    .await;
    let state = property::<u32>(
        bus,
        UPOWER_BUS_NAME,
        UPOWER_DISPLAY_DEVICE_PATH,
        DEVICE,
        "State",
    )
    .await;
    let on_battery = property::<bool>(
        bus,
        UPOWER_BUS_NAME,
        UPOWER_PATH,
        UPOWER_BUS_NAME,
        "OnBattery",
    )
    .await;
    let profile = property::<String>(
        bus,
        PROFILES_BUS_NAME,
        PROFILES_PATH,
        PROFILES_BUS_NAME,
        "ActiveProfile",
    )
    .await;
    project_power(
        kind,
        present,
        percentage,
        state,
        on_battery,
        profile.as_deref(),
    )
}

/// One observational sample on the system bus.
pub async fn observe_once() -> PowerSnapshot {
    match zbus::Connection::system().await {
        Ok(bus) => read_power(&bus).await,
        Err(_) => PowerSnapshot::Unavailable,
    }
}

/// Delivered to the caller on the first read and on every change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerObservation {
    pub snapshot: PowerSnapshot,
    /// Which wakeup produced it: qualification receipts only.
    pub cause: PowerWakeCause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerWakeCause {
    Baseline,
    Properties,
    Owner,
}

impl PowerWakeCause {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Properties => "properties",
            Self::Owner => "owner",
        }
    }
}

/// Change suppression: the first read is always delivered, later reads only
/// when the projected snapshot differs.
#[derive(Debug, Default)]
pub struct PowerReducer {
    last: Option<PowerSnapshot>,
}

impl PowerReducer {
    pub fn observe(
        &mut self,
        snapshot: PowerSnapshot,
        cause: PowerWakeCause,
    ) -> Option<PowerObservation> {
        if self.last == Some(snapshot) {
            return None;
        }
        self.last = Some(snapshot);
        Some(PowerObservation { snapshot, cause })
    }
}

/// Event-driven observation on the system bus, for the life of the process.
pub fn run_observer<F>(on_observation: F) -> Result<(), PowerObserverError>
where
    F: FnMut(PowerObservation) + Send + 'static,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| PowerObserverError::Connect(error.to_string()))?;
    runtime.block_on(async move {
        let bus = zbus::Connection::system()
            .await
            .map_err(|error| PowerObserverError::Connect(error.to_string()))?;
        observe_on(&bus, on_observation).await
    })
}

/// Observe on an existing connection. Wakeups are the services' own
/// `PropertiesChanged` signals on the three observed objects and owner changes
/// of either well-known name; every wakeup re-reads all facts uncached, so a
/// signal is only a trigger and never a source of truth. There is no timer.
pub async fn observe_on<F>(
    bus: &zbus::Connection,
    mut on_observation: F,
) -> Result<(), PowerObserverError>
where
    F: FnMut(PowerObservation),
{
    let subscribe = |error: zbus::Error| PowerObserverError::Subscribe(error.to_string());
    let dbus = zbus::fdo::DBusProxy::new(bus).await.map_err(subscribe)?;
    let upower_owner = dbus
        .receive_name_owner_changed_with_args(&[(0, UPOWER_BUS_NAME)])
        .await
        .map_err(subscribe)?;
    let profiles_owner = dbus
        .receive_name_owner_changed_with_args(&[(0, PROFILES_BUS_NAME)])
        .await
        .map_err(subscribe)?;
    let mut owners = futures::stream::select(upower_owner, profiles_owner);

    let mut properties = Vec::new();
    for path in [UPOWER_DISPLAY_DEVICE_PATH, UPOWER_PATH, PROFILES_PATH] {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .map_err(subscribe)?
            .member("PropertiesChanged")
            .map_err(subscribe)?
            .path(path)
            .map_err(subscribe)?
            .build();
        properties.push(
            zbus::MessageStream::for_match_rule(rule, bus, None)
                .await
                .map_err(subscribe)?,
        );
    }
    let mut properties = futures::stream::select_all(properties);

    let mut reducer = PowerReducer::default();
    if let Some(observation) = reducer.observe(read_power(bus).await, PowerWakeCause::Baseline) {
        on_observation(observation);
    }
    loop {
        let cause = tokio::select! {
            Some(_) = owners.next() => PowerWakeCause::Owner,
            Some(_) = properties.next() => PowerWakeCause::Properties,
            else => return Ok(()),
        };
        // A service that just appeared may publish its objects a moment after
        // taking its name; re-reading once after a short settle keeps the
        // first post-restart facts from being a transient partial read.
        if cause == PowerWakeCause::Owner {
            if let Some(observation) = reducer.observe(read_power(bus).await, cause) {
                on_observation(observation);
            }
            tokio::time::sleep(OWNER_SETTLE).await;
        }
        if let Some(observation) = reducer.observe(read_power(bus).await, cause) {
            on_observation(observation);
        }
    }
}

/// Bounded single re-read after an owner change; not a polling interval.
pub const OWNER_SETTLE: Duration = Duration::from_millis(250);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_names_itself_as_the_daemon_does_and_cycles_through_all_three() {
        for profile in [
            PowerProfile::PowerSaver,
            PowerProfile::Balanced,
            PowerProfile::Performance,
        ] {
            assert_eq!(parse_profile(profile_name(profile)), Some(profile));
            assert_ne!(next_profile(profile), profile);
            assert_eq!(next_profile(next_profile(next_profile(profile))), profile);
        }
    }
    #[test]
    fn desktop_prefers_observed_profile_over_fabricated_battery() {
        assert_eq!(
            project_power(
                Some(0),
                Some(false),
                Some(0.0),
                Some(0),
                None,
                Some("performance")
            ),
            PowerSnapshot::Profile(PowerProfile::Performance)
        );
        assert_eq!(
            project_power(Some(0), Some(false), None, None, None, None),
            PowerSnapshot::NoBatteryProfileUnavailable
        );
    }
    #[test]
    fn missing_power_facts_are_not_treated_as_false() {
        assert_eq!(
            project_power(None, None, None, None, None, Some("performance")),
            PowerSnapshot::Unavailable
        );
        assert_eq!(
            project_power(Some(2), Some(true), Some(80.0), Some(2), None, None),
            PowerSnapshot::Unavailable
        );
        assert_eq!(parse_profile("turbo"), None);
    }
    #[test]
    fn battery_states_and_percent_are_exactly_bounded() {
        assert_eq!(
            project_power(Some(2), Some(true), Some(19.6), Some(2), Some(true), None),
            PowerSnapshot::Battery {
                percentage: BatteryPercentage::new(19.6).unwrap(),
                charging: false,
                on_battery: true
            }
        );
        assert_eq!(
            project_power(Some(2), Some(true), Some(98.0), Some(4), Some(false), None),
            PowerSnapshot::Battery {
                percentage: BatteryPercentage::new(98.0).unwrap(),
                charging: true,
                on_battery: false
            }
        );
        assert_eq!(
            project_power(Some(2), Some(true), Some(30.0), Some(99), Some(true), None),
            PowerSnapshot::BatteryStateUnsupported
        );
        assert_eq!(
            project_power(
                Some(2),
                Some(true),
                Some(f64::NAN),
                Some(1),
                Some(true),
                None
            ),
            PowerSnapshot::Unavailable
        );
    }

    #[test]
    fn reducer_delivers_the_baseline_and_only_real_changes() {
        let mut reducer = PowerReducer::default();
        let profile = PowerSnapshot::Profile(PowerProfile::Performance);
        assert_eq!(
            reducer.observe(PowerSnapshot::Unavailable, PowerWakeCause::Baseline),
            Some(PowerObservation {
                snapshot: PowerSnapshot::Unavailable,
                cause: PowerWakeCause::Baseline
            })
        );
        assert_eq!(
            reducer.observe(PowerSnapshot::Unavailable, PowerWakeCause::Properties),
            None
        );
        assert!(reducer.observe(profile, PowerWakeCause::Owner).is_some());
        assert_eq!(reducer.observe(profile, PowerWakeCause::Properties), None);
        // Losing the provider is a change, and so is its return.
        assert!(
            reducer
                .observe(PowerSnapshot::Unavailable, PowerWakeCause::Owner)
                .is_some()
        );
        assert!(reducer.observe(profile, PowerWakeCause::Owner).is_some());
    }

    #[test]
    fn battery_precision_is_a_change_even_inside_one_rounded_percent() {
        let battery = |percent| PowerSnapshot::Battery {
            percentage: BatteryPercentage::new(percent).unwrap(),
            charging: false,
            on_battery: true,
        };
        let mut reducer = PowerReducer::default();
        assert!(
            reducer
                .observe(battery(19.6), PowerWakeCause::Baseline)
                .is_some()
        );
        assert!(
            reducer
                .observe(battery(19.8), PowerWakeCause::Properties)
                .is_some()
        );
        assert_eq!(BatteryPercentage::new(-0.0), BatteryPercentage::new(0.0));
        assert_eq!(BatteryPercentage::new(100.5), None);
    }
}
