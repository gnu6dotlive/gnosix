//! UXE-04: read-only Bar popouts for network, Bluetooth and battery.
//!
//! Each popout is derived only from an observed snapshot (NETWORK-01,
//! BLUETOOTH-01, the power observer) and offers no control. Where the
//! reference has a switch, the observed state is drawn as a switch that does
//! not react (W02); where the reference has a button that mutates (connect,
//! rescan, forget, profile switch, open settings), nothing is drawn (W03).
//! Facts the observers do not collect are not invented: the network popout
//! lists the networks in use, not a scan (W04); the battery popout says
//! whether it charges, not how long remains (W05).
//!
//! Rows are laid out top to bottom with one gap between them, and the view
//! paints them at exactly these rectangles.

use crate::product_status_glyph::{BluetoothDeviceClass, PowerProfileGlyph, StatusGlyph};
use gnos_bluetooth::{BluetoothProviderState, BluetoothSnapshot};
use gnos_i18n::t;
use gnos_network::{NetworkDeviceKind, NetworkProviderState, NetworkSnapshot};
use gnos_power::{PowerProfile, PowerSnapshot};

/// Reference `sizes.bar.networkWidth`.
pub const NETWORK_POPOUT_WIDTH: f32 = 320.0;
/// Reference Bluetooth popout width.
pub const BLUETOOTH_POPOUT_WIDTH: f32 = 300.0;
/// Reference `sizes.bar.batteryWidth`.
pub const BATTERY_POPOUT_WIDTH: f32 = 250.0;
/// Same inset as the audio popout.
const PADDING: f32 = 12.0;
/// Reference `spacing.small`.
const GAP: f32 = 8.0;
const HEADER_HEIGHT: f32 = 20.0;
const CAPTION_HEIGHT: f32 = 16.0;
const STATE_HEIGHT: f32 = 28.0;
const ITEM_HEIGHT: f32 = 32.0;
const TEXT_HEIGHT: f32 = 20.0;
const PROFILES_HEIGHT: f32 = 40.0;
/// Reference row caps.
const MAX_NETWORKS: usize = 8;
const MAX_BLUETOOTH_DEVICES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoPopoutKind {
    Network,
    Bluetooth,
    Battery,
}

impl InfoPopoutKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Battery => "battery",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoRowKind {
    Header,
    Caption,
    Text,
    /// An observed on/off fact drawn as a switch that never reacts; `None`
    /// when the fact was not observed.
    State {
        on: Option<bool>,
    },
    Wifi {
        glyph: StatusGlyph,
        secure: bool,
        active: bool,
    },
    Wired {
        connected: bool,
    },
    BluetoothDevice {
        glyph: StatusGlyph,
        connected: bool,
    },
    /// The three profiles, the observed one marked; `None` when unobserved.
    Profiles {
        current: Option<PowerProfileGlyph>,
    },
    /// Opens Settings on this popout's page (the reference's "Open
    /// settings"). It opens a surface; it writes nothing.
    OpenSettings,
}

impl InfoRowKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::Caption => "caption",
            Self::Text => "text",
            Self::State { .. } => "state",
            Self::Wifi { .. } => "wifi",
            Self::Wired { .. } => "wired",
            Self::BluetoothDevice { .. } => "device",
            Self::Profiles { .. } => "profiles",
            Self::OpenSettings => "open_settings",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfoRow {
    pub kind: InfoRowKind,
    pub label: String,
    /// Popout-local rectangle.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfoPopout {
    pub kind: InfoPopoutKind,
    pub width: f32,
    pub height: f32,
    pub rows: Vec<InfoRow>,
}

struct Rows {
    width: f32,
    y: f32,
    rows: Vec<InfoRow>,
}

impl Rows {
    fn new(width: f32) -> Self {
        Self {
            width,
            y: PADDING,
            rows: Vec::new(),
        }
    }

    fn push(&mut self, kind: InfoRowKind, label: impl Into<String>, height: f32) {
        if !self.rows.is_empty() {
            self.y += GAP;
        }
        self.rows.push(InfoRow {
            kind,
            label: label.into(),
            x: PADDING,
            y: self.y,
            width: self.width - 2.0 * PADDING,
            height,
        });
        self.y += height;
    }

    fn finish(mut self, kind: InfoPopoutKind) -> InfoPopout {
        self.push(InfoRowKind::OpenSettings, t("Open settings"), ITEM_HEIGHT);
        InfoPopout {
            kind,
            width: self.width,
            height: self.y + PADDING,
            rows: self.rows,
        }
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl InfoPopout {
    /// Network: the Ethernet view while a wired connection is active, the
    /// wireless view otherwise (reference `Content.qml`).
    pub fn network(snapshot: &NetworkSnapshot) -> Self {
        let mut rows = Rows::new(NETWORK_POPOUT_WIDTH);
        if snapshot.provider != NetworkProviderState::Available {
            rows.push(InfoRowKind::Header, t("Network"), HEADER_HEIGHT);
            rows.push(
                InfoRowKind::Caption,
                t("Network status unavailable"),
                CAPTION_HEIGHT,
            );
            return rows.finish(InfoPopoutKind::Network);
        }
        let user_facing = || {
            snapshot
                .devices
                .values()
                .filter(|d| d.relevance.is_user_facing())
        };
        let mut wired: Vec<_> = user_facing()
            .filter(|d| d.kind == NetworkDeviceKind::Ethernet)
            .collect();
        if wired.iter().any(|d| d.state.is_connected()) {
            wired.sort_by(|a, b| {
                b.state
                    .is_connected()
                    .cmp(&a.state.is_connected())
                    .then_with(|| a.interface.cmp(&b.interface))
            });
            rows.push(InfoRowKind::Header, t("Ethernet"), HEADER_HEIGHT);
            rows.push(
                InfoRowKind::Caption,
                plural(wired.len(), "device available", "devices available"),
                CAPTION_HEIGHT,
            );
            for device in wired.into_iter().take(MAX_NETWORKS) {
                rows.push(
                    InfoRowKind::Wired {
                        connected: device.state.is_connected(),
                    },
                    device.interface.clone(),
                    ITEM_HEIGHT,
                );
            }
            return rows.finish(InfoPopoutKind::Network);
        }
        rows.push(InfoRowKind::Header, t("Wireless"), HEADER_HEIGHT);
        rows.push(
            InfoRowKind::State {
                on: snapshot.globals.as_ref().map(|g| g.wireless_enabled),
            },
            t("Enabled"),
            STATE_HEIGHT,
        );
        let mut networks: Vec<_> = user_facing()
            .filter(|d| d.kind == NetworkDeviceKind::Wifi)
            .filter_map(|d| d.wifi.as_ref().map(|w| (d.state.is_connected(), w)))
            .collect();
        networks.sort_by(|(a_on, a), (b_on, b)| {
            b_on.cmp(a_on)
                .then_with(|| b.strength.cmp(&a.strength))
                .then_with(|| a.ssid.cmp(&b.ssid))
        });
        let connected = networks.iter().filter(|(on, _)| *on).count();
        rows.push(
            InfoRowKind::Caption,
            if connected == 0 {
                t("Not connected").to_owned()
            } else {
                plural(connected, "network in use", "networks in use")
            },
            CAPTION_HEIGHT,
        );
        for (active, wifi) in networks.into_iter().take(MAX_NETWORKS) {
            rows.push(
                InfoRowKind::Wifi {
                    glyph: StatusGlyph::wifi_from_strength(wifi.strength),
                    secure: wifi.security.authenticated,
                    active,
                },
                wifi.ssid.clone(),
                ITEM_HEIGHT,
            );
        }
        rows.finish(InfoPopoutKind::Network)
    }

    /// Bluetooth: adapter state, then devices connected first, then paired,
    /// then by name, at most five (reference `Bluetooth.qml`).
    pub fn bluetooth(snapshot: &BluetoothSnapshot) -> Self {
        let mut rows = Rows::new(BLUETOOTH_POPOUT_WIDTH);
        rows.push(InfoRowKind::Header, t("Bluetooth"), HEADER_HEIGHT);
        if snapshot.provider != BluetoothProviderState::Available {
            rows.push(
                InfoRowKind::Caption,
                t("Bluetooth status unavailable"),
                CAPTION_HEIGHT,
            );
            return rows.finish(InfoPopoutKind::Bluetooth);
        }
        // BlueZ has no default adapter; aggregate as the Bar entry does.
        let observed = !snapshot.adapters.is_empty();
        rows.push(
            InfoRowKind::State {
                on: observed.then(|| snapshot.adapters.values().any(|a| a.powered)),
            },
            t("Enabled"),
            STATE_HEIGHT,
        );
        rows.push(
            InfoRowKind::State {
                on: observed.then(|| snapshot.adapters.values().any(|a| a.discovering)),
            },
            t("Discovering"),
            STATE_HEIGHT,
        );
        let mut devices: Vec<_> = snapshot.devices.values().collect();
        let name = |d: &gnos_bluetooth::BluetoothDeviceSnapshot| {
            d.alias
                .clone()
                .unwrap_or_else(|| t("Unnamed device").to_owned())
        };
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| b.paired.cmp(&a.paired))
                .then_with(|| name(a).cmp(&name(b)))
        });
        let connected = devices.iter().filter(|d| d.connected).count();
        let available = plural(devices.len(), "device available", "devices available");
        rows.push(
            InfoRowKind::Caption,
            if connected > 0 {
                gnos_i18n::tf(
                    "{available} ({connected} connected)",
                    &[
                        ("available", &available),
                        ("connected", &connected.to_string()),
                    ],
                )
            } else {
                available
            },
            CAPTION_HEIGHT,
        );
        for device in devices.into_iter().take(MAX_BLUETOOTH_DEVICES) {
            rows.push(
                InfoRowKind::BluetoothDevice {
                    glyph: StatusGlyph::BluetoothDevice(BluetoothDeviceClass::from_bluez_icon(
                        device.icon.as_deref(),
                    )),
                    connected: device.connected,
                },
                name(device),
                ITEM_HEIGHT,
            );
        }
        rows.finish(InfoPopoutKind::Bluetooth)
    }

    /// Battery: remaining charge or "No battery detected", one status line,
    /// then the power profiles (reference `Battery.qml`).
    pub fn battery(power: PowerSnapshot) -> Self {
        let mut rows = Rows::new(BATTERY_POPOUT_WIDTH);
        let profile = |p: PowerProfile| match p {
            PowerProfile::PowerSaver => PowerProfileGlyph::PowerSaver,
            PowerProfile::Balanced => PowerProfileGlyph::Balanced,
            PowerProfile::Performance => PowerProfileGlyph::Performance,
        };
        let (first, second, current) = match power {
            PowerSnapshot::Battery {
                percentage,
                charging,
                on_battery,
            } => {
                let percent = percentage.as_percent().round() as u32;
                let status = if on_battery {
                    t("On battery")
                } else if percent == 100 {
                    t("Fully charged!")
                } else if charging {
                    t("Charging")
                } else {
                    t("Plugged in, not charging")
                };
                (
                    gnos_i18n::tf(
                        "Remaining: {percent}%",
                        &[("percent", &percent.to_string())],
                    ),
                    status.to_owned(),
                    None,
                )
            }
            PowerSnapshot::Profile(p) => (
                t("No battery detected").to_owned(),
                gnos_i18n::tf("Power profile: {profile}", &[("profile", profile_name(p))]),
                Some(profile(p)),
            ),
            PowerSnapshot::NoBatteryProfileUnavailable => (
                t("No battery detected").to_owned(),
                t("Power profile: Unknown").to_owned(),
                None,
            ),
            PowerSnapshot::BatteryStateUnsupported => (
                t("Battery state not recognised").to_owned(),
                t("Power profile: Unknown").to_owned(),
                None,
            ),
            PowerSnapshot::Unavailable => (
                t("Power status unavailable").to_owned(),
                t("Power profile: Unknown").to_owned(),
                None,
            ),
        };
        rows.push(InfoRowKind::Text, first, TEXT_HEIGHT);
        rows.push(InfoRowKind::Text, second, TEXT_HEIGHT);
        rows.push(InfoRowKind::Profiles { current }, "", PROFILES_HEIGHT);
        rows.finish(InfoPopoutKind::Battery)
    }
}

fn profile_name(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::PowerSaver => t("Power saver"),
        PowerProfile::Balanced => t("Balanced"),
        PowerProfile::Performance => t("Performance"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_bluetooth::{
        BluetoothAdapterSnapshot, BluetoothDeviceSnapshot, BluetoothObjectRef, BluetoothPowerState,
    };
    use std::collections::BTreeMap;

    /// Content rows; the trailing "Open settings" row is checked on its own.
    fn labels(popout: &InfoPopout) -> Vec<(&'static str, &str)> {
        let last = popout.rows.last().expect("rows");
        assert_eq!(
            (last.kind, last.label.as_str()),
            (InfoRowKind::OpenSettings, "Open settings"),
            "every popout ends with Open settings"
        );
        popout.rows[..popout.rows.len() - 1]
            .iter()
            .map(|r| (r.kind.name(), r.label.as_str()))
            .collect()
    }

    fn assert_stacked(popout: &InfoPopout) {
        let mut y = PADDING;
        for (i, row) in popout.rows.iter().enumerate() {
            if i > 0 {
                y += GAP;
            }
            assert_eq!(row.y, y, "{row:?}");
            assert_eq!(row.x, PADDING);
            assert_eq!(row.width, popout.width - 2.0 * PADDING);
            y += row.height;
        }
        assert_eq!(popout.height, y + PADDING);
    }

    fn device(
        name: &str,
        connected: bool,
        paired: bool,
        icon: Option<&str>,
    ) -> BluetoothDeviceSnapshot {
        BluetoothDeviceSnapshot {
            id: BluetoothObjectRef::new(":1.1", format!("/org/bluez/hci0/dev_{name}")),
            adapter: BluetoothObjectRef::new(":1.1", "/org/bluez/hci0"),
            alias: Some(name.to_owned()),
            paired,
            bonded: paired,
            connected,
            trusted: false,
            blocked: false,
            services_resolved: connected,
            rssi: None,
            icon: icon.map(str::to_owned),
        }
    }

    fn bluetooth(devices: Vec<BluetoothDeviceSnapshot>, powered: bool) -> BluetoothSnapshot {
        let adapter = BluetoothObjectRef::new(":1.1", "/org/bluez/hci0");
        BluetoothSnapshot {
            provider: BluetoothProviderState::Available,
            generation: 1,
            provider_owner: Some(":1.1".into()),
            adapters: BTreeMap::from([(
                adapter.clone(),
                BluetoothAdapterSnapshot {
                    id: adapter,
                    powered,
                    power_state: if powered {
                        BluetoothPowerState::On
                    } else {
                        BluetoothPowerState::Off
                    },
                    discovering: false,
                    pairable: false,
                    discoverable: false,
                },
            )]),
            devices: devices.into_iter().map(|d| (d.id.clone(), d)).collect(),
        }
    }

    #[test]
    fn bluetooth_orders_connected_then_paired_then_name_and_caps_at_five() {
        let popout = InfoPopout::bluetooth(&bluetooth(
            vec![
                device("Zed", false, true, None),
                device("Alpha", false, false, None),
                device("Buds", true, true, Some("audio-headset")),
                device("Mouse", false, true, Some("input-mouse")),
                device("Kbd", false, true, Some("input-keyboard")),
                device("Phone", false, true, Some("phone")),
            ],
            true,
        ));
        assert_eq!(
            labels(&popout),
            [
                ("header", "Bluetooth"),
                ("state", "Enabled"),
                ("state", "Discovering"),
                ("caption", "6 devices available (1 connected)"),
                ("device", "Buds"),
                ("device", "Kbd"),
                ("device", "Mouse"),
                ("device", "Phone"),
                ("device", "Zed"),
            ]
        );
        assert_eq!(popout.rows[1].kind, InfoRowKind::State { on: Some(true) });
        assert_eq!(
            popout.rows[4].kind,
            InfoRowKind::BluetoothDevice {
                glyph: StatusGlyph::BluetoothDevice(BluetoothDeviceClass::Headphones),
                connected: true
            }
        );
        assert_eq!(popout.width, BLUETOOTH_POPOUT_WIDTH);
        assert_stacked(&popout);
    }

    #[test]
    fn bluetooth_without_provider_says_so_and_shows_no_state() {
        let mut snapshot = bluetooth(vec![], false);
        snapshot.provider = BluetoothProviderState::Unavailable;
        let popout = InfoPopout::bluetooth(&snapshot);
        assert_eq!(
            labels(&popout),
            [
                ("header", "Bluetooth"),
                ("caption", "Bluetooth status unavailable")
            ]
        );
    }

    #[test]
    fn bluetooth_without_adapter_leaves_switches_unobserved() {
        let mut snapshot = bluetooth(vec![], false);
        snapshot.adapters.clear();
        let popout = InfoPopout::bluetooth(&snapshot);
        assert_eq!(popout.rows[1].kind, InfoRowKind::State { on: None });
        assert_eq!(popout.rows[3].label, "0 devices available");
    }

    #[test]
    fn battery_rows_follow_the_observed_power_fact() {
        let pct = |p| gnos_power::BatteryPercentage::new(p).expect("valid");
        let popout = InfoPopout::battery(PowerSnapshot::Battery {
            percentage: pct(41.6),
            charging: false,
            on_battery: true,
        });
        assert_eq!(
            labels(&popout),
            [
                ("text", "Remaining: 42%"),
                ("text", "On battery"),
                ("profiles", "")
            ]
        );
        assert_eq!(popout.rows[2].kind, InfoRowKind::Profiles { current: None });
        let full = InfoPopout::battery(PowerSnapshot::Battery {
            percentage: pct(100.0),
            charging: true,
            on_battery: false,
        });
        assert_eq!(full.rows[1].label, "Fully charged!");
        let desktop = InfoPopout::battery(PowerSnapshot::Profile(PowerProfile::Performance));
        assert_eq!(
            labels(&desktop),
            [
                ("text", "No battery detected"),
                ("text", "Power profile: Performance"),
                ("profiles", "")
            ]
        );
        assert_eq!(
            desktop.rows[2].kind,
            InfoRowKind::Profiles {
                current: Some(PowerProfileGlyph::Performance)
            }
        );
        assert_eq!(desktop.width, BATTERY_POPOUT_WIDTH);
        assert_stacked(&desktop);
        let unknown = InfoPopout::battery(PowerSnapshot::Unavailable);
        assert_eq!(unknown.rows[0].label, "Power status unavailable");
    }

    fn net_device(
        path: &str,
        interface: &str,
        kind: NetworkDeviceKind,
        connected: bool,
        relevance: gnos_network::NetworkRelevance,
        wifi: Option<(&str, u8, bool)>,
    ) -> gnos_network::NetworkDeviceSnapshot {
        let id = |p: &str| gnos_network::NetworkObjectRef::new(":1.7", p);
        gnos_network::NetworkDeviceSnapshot {
            id: id(path),
            interface: interface.into(),
            kind,
            state: if connected {
                gnos_network::NetworkDeviceState::Connected
            } else {
                gnos_network::NetworkDeviceState::Disconnected
            },
            managed: true,
            primary: connected,
            active_connection: None,
            settings_connection: None,
            relevance,
            wifi: wifi.map(|(ssid, strength, secure)| gnos_network::NetworkWifiFacts {
                access_point: None,
                bssid: None,
                ssid: ssid.into(),
                strength,
                frequency_mhz: 5220,
                security: gnos_network::decode_rsn_security(
                    if secure { 1 } else { 0 },
                    0,
                    if secure { 392 } else { 0 },
                ),
            }),
        }
    }

    fn network(
        devices: Vec<gnos_network::NetworkDeviceSnapshot>,
        wireless_enabled: Option<bool>,
    ) -> NetworkSnapshot {
        NetworkSnapshot {
            provider: NetworkProviderState::Available,
            generation: 1,
            provider_owner: Some(":1.7".into()),
            globals: wireless_enabled.map(|enabled| gnos_network::NetworkGlobals {
                state: gnos_network::NetworkDeviceState::ConnectedGlobal,
                connectivity: gnos_network::NetworkConnectivityAssessment {
                    value: gnos_network::NetworkConnectivity::Full,
                    checked: true,
                },
                primary_connection: None,
                primary_connection_type: None,
                networking_enabled: true,
                wireless_enabled: enabled,
            }),
            devices: devices.into_iter().map(|d| (d.id.clone(), d)).collect(),
            active_connections: BTreeMap::new(),
            settings_connections: BTreeMap::new(),
        }
    }

    use gnos_network::NetworkRelevance::{InfrastructureVirtual, PhysicalPrimaryCandidate};

    #[test]
    fn wireless_view_lists_the_network_in_use_and_never_infrastructure() {
        let popout = InfoPopout::network(&network(
            vec![
                net_device(
                    "/d/3",
                    "wlan0",
                    NetworkDeviceKind::Wifi,
                    true,
                    PhysicalPrimaryCandidate,
                    Some(("Home", 68, true)),
                ),
                net_device(
                    "/d/2",
                    "enp3s0",
                    NetworkDeviceKind::Ethernet,
                    false,
                    PhysicalPrimaryCandidate,
                    None,
                ),
                net_device(
                    "/d/9",
                    "docker0",
                    NetworkDeviceKind::Bridge,
                    true,
                    InfrastructureVirtual,
                    None,
                ),
            ],
            Some(true),
        ));
        assert_eq!(
            labels(&popout),
            [
                ("header", "Wireless"),
                ("state", "Enabled"),
                ("caption", "1 network in use"),
                ("wifi", "Home"),
            ]
        );
        assert_eq!(popout.rows[1].kind, InfoRowKind::State { on: Some(true) });
        assert_eq!(
            popout.rows[3].kind,
            InfoRowKind::Wifi {
                glyph: StatusGlyph::Wifi { bars: 3 },
                secure: true,
                active: true
            }
        );
        assert_eq!(popout.width, NETWORK_POPOUT_WIDTH);
        assert_stacked(&popout);
    }

    #[test]
    fn wireless_view_without_globals_or_connection_says_so() {
        let popout = InfoPopout::network(&network(
            vec![net_device(
                "/d/3",
                "wlan0",
                NetworkDeviceKind::Wifi,
                false,
                PhysicalPrimaryCandidate,
                None,
            )],
            None,
        ));
        assert_eq!(popout.rows[1].kind, InfoRowKind::State { on: None });
        assert_eq!(popout.rows[2].label, "Not connected");
        assert_eq!(labels(&popout).len(), 3);
    }

    #[test]
    fn an_active_wired_connection_selects_the_ethernet_view() {
        let popout = InfoPopout::network(&network(
            vec![
                net_device(
                    "/d/3",
                    "wlan0",
                    NetworkDeviceKind::Wifi,
                    true,
                    PhysicalPrimaryCandidate,
                    Some(("Home", 90, false)),
                ),
                net_device(
                    "/d/4",
                    "enp4s0",
                    NetworkDeviceKind::Ethernet,
                    false,
                    PhysicalPrimaryCandidate,
                    None,
                ),
                net_device(
                    "/d/2",
                    "enp3s0",
                    NetworkDeviceKind::Ethernet,
                    true,
                    PhysicalPrimaryCandidate,
                    None,
                ),
            ],
            Some(true),
        ));
        assert_eq!(
            labels(&popout),
            [
                ("header", "Ethernet"),
                ("caption", "2 devices available"),
                ("wired", "enp3s0"),
                ("wired", "enp4s0"),
            ]
        );
        assert_eq!(popout.rows[2].kind, InfoRowKind::Wired { connected: true });
        assert_stacked(&popout);
    }

    #[test]
    fn network_unavailable_provider_says_so() {
        let mut snapshot = network(vec![], None);
        snapshot.provider = NetworkProviderState::Unavailable;
        let popout = InfoPopout::network(&snapshot);
        assert_eq!(
            labels(&popout),
            [
                ("header", "Network"),
                ("caption", "Network status unavailable")
            ]
        );
    }
}
