//! Read-only Hyprland layer geometry and reservation adapter for GNOS.UX qualification.
//!
//! Hyprland remains configuration/compositor authority. This module observes
//! compositor geometry; it does not observe desktop semantics, mutate state,
//! or persist anything.

use std::process::Command;

const RECEIPT_PREFIX: &str = "GNOS_HYPRLAND_GEOMETRY|";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorRectObservation {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerRectObservation {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReservedEdgesObservation {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HyprlandLayerGeometryObservation {
    pub version: String,
    pub monitor: String,
    pub monitor_rect: MonitorRectObservation,
    pub reserved: ReservedEdgesObservation,
    pub layer_rect: LayerRectObservation,
    pub scale: f32,
    pub transform: i32,
}

fn parse_monitor_rect(value: &str) -> Option<MonitorRectObservation> {
    let values = value
        .split(',')
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 4 {
        return None;
    }
    Some(MonitorRectObservation {
        x: values[0],
        y: values[1],
        width: values[2],
        height: values[3],
    })
}

fn parse_layer_rect(value: &str) -> Option<LayerRectObservation> {
    let values = value
        .split(',')
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 4 {
        return None;
    }
    Some(LayerRectObservation {
        x: values[0],
        y: values[1],
        width: values[2],
        height: values[3],
    })
}

fn parse_reserved_edges(value: &str) -> Option<ReservedEdgesObservation> {
    let values = value
        .split(',')
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 4 {
        return None;
    }
    Some(ReservedEdgesObservation {
        top: values[0],
        right: values[1],
        bottom: values[2],
        left: values[3],
    })
}

impl HyprlandLayerGeometryObservation {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        let payload = line.strip_prefix(RECEIPT_PREFIX)?;
        if payload == "none" {
            return None;
        }

        let mut version = None;
        let mut monitor = None;
        let mut monitor_rect = None;
        let mut reserved = None;
        let mut layer_rect = None;
        let mut scale = None;
        let mut transform = None;

        for field in payload.split('|') {
            let (key, value) = field.split_once('=')?;
            match key {
                "version" => version = Some(value.to_owned()),
                "monitor" => monitor = Some(value.to_owned()),
                "monitor_rect" => monitor_rect = parse_monitor_rect(value),
                "reserved" => reserved = parse_reserved_edges(value),
                "layer_rect" => layer_rect = parse_layer_rect(value),
                "scale" => scale = value.parse::<f32>().ok(),
                "transform" => transform = value.parse::<i32>().ok(),
                "workspace" | "active_workspace" | "activeworkspace" | "activewindow"
                | "clients" | "workspaces" | "focus" | "visibility" => return None,
                _ => {}
            }
        }

        Some(Self {
            version: version?,
            monitor: monitor?,
            monitor_rect: monitor_rect?,
            reserved: reserved?,
            layer_rect: layer_rect?,
            scale: scale?,
            transform: transform?,
        })
    }
}

fn layer_geometry_lua(namespace: &str, pid: u32) -> String {
    let escaped_namespace = namespace.replace('\\', "\\\\").replace('\'', "\\'");
    format!(
        concat!(
            "local pid={pid}; local ns='{namespace}'; ",
            "for _,l in ipairs(hl.get_layers({{namespace=ns}})) do ",
            "if l.pid == pid then local m=l.monitor; ",
            "if not m then return 'GNOS_HYPRLAND_GEOMETRY|none' end; ",
            "local r=m.reserved; ",
            "return string.format(",
            "'GNOS_HYPRLAND_GEOMETRY|version=%s|monitor=%s|monitor_rect=%d,%d,%d,%d|",
            "reserved=%.2f,%.2f,%.2f,%.2f|layer_rect=%d,%d,%d,%d|",
            "scale=%.3f|transform=%d', ",
            "hl.version(), m.name, m.x, m.y, m.width, m.height, ",
            "r.top, r.right, r.bottom, r.left, l.x, l.y, l.w, l.h, m.scale, m.transform) end end; ",
            "return 'GNOS_HYPRLAND_GEOMETRY|none'"
        ),
        pid = pid,
        namespace = escaped_namespace,
    )
}

/// Query current Hyprland layer geometry and reservation evidence.
///
/// This function does not read desktop semantic state or call mutation APIs.
pub fn query_layer_geometry(namespace: &str, pid: u32) -> Option<HyprlandLayerGeometryObservation> {
    let lua = layer_geometry_lua(namespace, pid);
    let output = Command::new("hyprctl").args(["repl", &lua]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    HyprlandLayerGeometryObservation::parse(String::from_utf8_lossy(&output.stdout).trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_geometry_observation() {
        let parsed = HyprlandLayerGeometryObservation::parse(
            "GNOS_HYPRLAND_GEOMETRY|version=0.56.2|monitor=HDMI-A-1|monitor_rect=-630,590,2560,1440|reserved=36.00,12.00,8.00,4.00|layer_rect=350,1610,600,420|scale=1.000|transform=0",
        )
        .expect("valid geometry receipt");

        assert_eq!(parsed.version, "0.56.2");
        assert_eq!(parsed.monitor, "HDMI-A-1");
        assert_eq!(
            parsed.monitor_rect,
            MonitorRectObservation {
                x: -630,
                y: 590,
                width: 2560,
                height: 1440,
            }
        );
        assert_eq!(
            parsed.reserved,
            ReservedEdgesObservation {
                top: 36.0,
                right: 12.0,
                bottom: 8.0,
                left: 4.0,
            }
        );
        assert_eq!(
            parsed.layer_rect,
            LayerRectObservation {
                x: 350,
                y: 1610,
                width: 600,
                height: 420,
            }
        );
        assert_eq!(parsed.scale, 1.0);
        assert_eq!(parsed.transform, 0);
        assert!(!format!("{parsed:?}").contains("workspace"));
    }

    #[test]
    fn rejects_semantic_workspace_receipts() {
        assert!(HyprlandLayerGeometryObservation::parse(
            "GNOS_HYPRLAND_GEOMETRY|version=0.56.2|monitor=HDMI-A-1|monitor_rect=0,0,1920,1080|reserved=0,0,0,0|workspace=1|layer_rect=0,0,100,100|scale=1|transform=0",
        )
        .is_none());
    }

    #[test]
    fn geometry_query_omits_semantic_reads() {
        let lua = layer_geometry_lua("gnos.ux:dock:qualification", 42);
        for forbidden in ["active_workspace", "activeworkspace", "workspace"] {
            assert!(!lua.contains(forbidden));
        }
        for required in [
            "monitor_rect",
            "reserved",
            "layer_rect",
            "scale",
            "transform",
        ] {
            assert!(lua.contains(required));
        }
    }

    #[test]
    fn rejects_missing_or_non_receipts() {
        assert!(HyprlandLayerGeometryObservation::parse("GNOS_HYPRLAND_GEOMETRY|none").is_none());
        assert!(HyprlandLayerGeometryObservation::parse("not-a-receipt").is_none());
    }
}
