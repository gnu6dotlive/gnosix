//! Brightness (U02): laptop backlights and external displays.
//!
//! - **Backlights** are observed in `/sys/class/backlight/<name>/`
//!   (`brightness`, `max_brightness`) and set through logind
//!   `Session.SetBrightness("backlight", name, value)` on the caller's
//!   session, which needs no privilege.
//! - **Displays** speak DDC/CI through `ddcutil` (VCP 0x10). A display is
//!   identified by its I2C bus, manufacturer, model and serial, from
//!   `ddcutil detect --brief`; a request is refused unless the display is
//!   still detected on the same bus with the same serial. DDC/CI is slow
//!   (about a second a call), so callers run it off the UI thread.
//!
//! The program is `/usr/bin/ddcutil`, never looked up in `PATH`. Development
//! builds honour `GNOS_UX_DDCUTIL` so the qualification can stand one in.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One external display, as `ddcutil detect --brief` names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DisplayRef {
    pub bus: u32,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub connector: Option<String>,
}

/// A level and its maximum, as the device reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrightnessLevel {
    pub current: u32,
    pub max: u32,
}

impl BrightnessLevel {
    /// The level in percent, rounded.
    pub fn percent(self) -> u8 {
        if self.max == 0 {
            return 0;
        }
        ((self.current.min(self.max) as f64 * 100.0 / self.max as f64).round()) as u8
    }

    /// The raw value for `percent` on this device.
    pub fn value_for(self, percent: u8) -> u32 {
        ((percent.min(100) as f64 * self.max as f64 / 100.0).round()) as u32
    }
}

/// Parse `ddcutil detect --brief`: valid displays only.
pub fn parse_detect(text: &str) -> Vec<DisplayRef> {
    let mut out = Vec::new();
    let mut bus = None;
    let mut connector = None;
    let mut monitor = None;
    let mut valid = false;
    let flush = |out: &mut Vec<DisplayRef>,
                 valid: bool,
                 bus: Option<u32>,
                 connector: Option<String>,
                 monitor: Option<(String, String, String)>| {
        if let (true, Some(bus), Some((manufacturer, model, serial))) = (valid, bus, monitor) {
            out.push(DisplayRef {
                bus,
                manufacturer,
                model,
                serial,
                connector,
            });
        }
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if line.starts_with("Display ") || line.starts_with("Invalid display") {
            flush(
                &mut out,
                valid,
                bus.take(),
                connector.take(),
                monitor.take(),
            );
            valid = line.starts_with("Display ");
        } else if let Some(rest) = trimmed.strip_prefix("I2C bus:") {
            bus = rest
                .trim()
                .strip_prefix("/dev/i2c-")
                .and_then(|n| n.parse().ok());
        } else if let Some(rest) = trimmed.strip_prefix("DRM connector:") {
            connector = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("Monitor:") {
            let rest = rest.trim();
            // MFG:MODEL:SERIAL; the model may itself hold colons.
            if let (Some(first), Some(last)) = (rest.find(':'), rest.rfind(':'))
                && first < last
            {
                monitor = Some((
                    rest[..first].to_owned(),
                    rest[first + 1..last].to_owned(),
                    rest[last + 1..].to_owned(),
                ));
            }
        }
    }
    flush(&mut out, valid, bus, connector, monitor);
    out
}

/// Parse `ddcutil getvcp 10 --brief`: `VCP 10 C <current> <max>`.
pub fn parse_getvcp(text: &str) -> Option<BrightnessLevel> {
    let line = text.lines().find(|l| l.starts_with("VCP 10 C "))?;
    let mut parts = line.split_whitespace().skip(3);
    let current = parts.next()?.parse().ok()?;
    let max = parts.next()?.parse().ok()?;
    (max > 0).then_some(BrightnessLevel { current, max })
}

/// Why a display brightness request was not sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrightnessRefusal {
    /// Not detected on that bus with that serial any more.
    DisplayGone,
    AlreadySatisfied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrightnessOutcome {
    Accepted,
    Refused(BrightnessRefusal),
    Failed(String),
}

/// Whether a request for `percent` on `display` may be sent, given the
/// displays detected now and the level last observed.
pub fn revalidate_display(
    detected: &[DisplayRef],
    display: &DisplayRef,
    observed: Option<BrightnessLevel>,
    percent: u8,
) -> Result<u32, BrightnessRefusal> {
    if !detected
        .iter()
        .any(|d| d.bus == display.bus && d.serial == display.serial)
    {
        return Err(BrightnessRefusal::DisplayGone);
    }
    let level = observed.unwrap_or(BrightnessLevel {
        current: u32::MAX,
        max: 100,
    });
    let value = level.value_for(percent);
    if value == level.current {
        return Err(BrightnessRefusal::AlreadySatisfied);
    }
    Ok(value)
}

/// The `ddcutil` program.
pub fn ddcutil_program() -> PathBuf {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("GNOS_UX_DDCUTIL").filter(|p| Path::new(p).is_absolute()) {
        return PathBuf::from(path);
    }
    PathBuf::from("/usr/bin/ddcutil")
}

fn run(args: &[&str]) -> Result<String, String> {
    let output = Command::new(ddcutil_program())
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Detect the displays that answer DDC/CI. Blocking.
pub fn detect_displays() -> Result<Vec<DisplayRef>, String> {
    run(&["detect", "--brief"]).map(|text| parse_detect(&text))
}

/// Read a display's brightness. Blocking.
pub fn read_display(display: &DisplayRef) -> Result<BrightnessLevel, String> {
    let bus = display.bus.to_string();
    let text = run(&["--bus", &bus, "getvcp", "10", "--brief"])?;
    parse_getvcp(&text).ok_or_else(|| format!("unreadable reply: {}", text.trim()))
}

/// Revalidate against a fresh detection, then set a display's brightness.
/// Blocking.
pub fn request_display(
    display: &DisplayRef,
    observed: Option<BrightnessLevel>,
    percent: u8,
) -> BrightnessOutcome {
    let detected = match detect_displays() {
        Ok(d) => d,
        Err(e) => return BrightnessOutcome::Failed(e),
    };
    let value = match revalidate_display(&detected, display, observed, percent) {
        Ok(v) => v,
        Err(r) => return BrightnessOutcome::Refused(r),
    };
    let bus = display.bus.to_string();
    match run(&["--bus", &bus, "setvcp", "10", &value.to_string()]) {
        Ok(_) => BrightnessOutcome::Accepted,
        Err(e) => BrightnessOutcome::Failed(e),
    }
}

/// One laptop backlight, observed in sysfs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlight {
    pub name: String,
    pub level: BrightnessLevel,
}

/// Read the backlights under `root` (normally `/sys/class/backlight`).
pub fn read_backlights(root: &Path) -> Vec<Backlight> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let read = |p: PathBuf| -> Option<u32> { std::fs::read_to_string(p).ok()?.trim().parse().ok() };
    let mut out: Vec<Backlight> = entries
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let current = read(dir.join("brightness"))?;
            let max = read(dir.join("max_brightness"))?;
            (max > 0).then_some(Backlight {
                name,
                level: BrightnessLevel { current, max },
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Ask logind to set a backlight, on the caller's own session.
pub async fn request_backlight(
    connection: &zbus::Connection,
    backlight: &Backlight,
    percent: u8,
) -> BrightnessOutcome {
    let value = backlight.level.value_for(percent);
    if value == backlight.level.current {
        return BrightnessOutcome::Refused(BrightnessRefusal::AlreadySatisfied);
    }
    let reply = connection
        .call_method(
            Some(crate::LOGIND_BUS_NAME),
            "/org/freedesktop/login1/session/auto",
            Some("org.freedesktop.login1.Session"),
            "SetBrightness",
            &("backlight", backlight.name.as_str(), value),
        )
        .await;
    match reply {
        Ok(_) => BrightnessOutcome::Accepted,
        Err(error) => BrightnessOutcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DETECT: &str = "Display 1
   I2C bus:          /dev/i2c-1
   DRM connector:    card1-HDMI-A-1
   drm_connector_id: 828
   Monitor:          GSM:LG QHD:005KMHWY2601

Invalid display
   I2C bus:          /dev/i2c-2
   Monitor:          XXX:Broken:000

Display 2
   I2C bus:          /dev/i2c-3
   DRM connector:    card1-DP-2
   drm_connector_id: 837
   Monitor:          ACR:ED320QR S:TJNAA0033W01
";

    #[test]
    fn detect_names_each_valid_display_by_bus_model_and_serial() {
        let displays = parse_detect(DETECT);
        assert_eq!(displays.len(), 2);
        assert_eq!(displays[0].bus, 1);
        assert_eq!(displays[0].manufacturer, "GSM");
        assert_eq!(displays[0].model, "LG QHD");
        assert_eq!(displays[0].serial, "005KMHWY2601");
        assert_eq!(displays[0].connector.as_deref(), Some("card1-HDMI-A-1"));
        assert_eq!(displays[1].bus, 3);
        assert_eq!(displays[1].model, "ED320QR S");
        assert!(parse_detect("No displays found.").is_empty());
    }

    #[test]
    fn getvcp_reads_the_level_and_its_maximum() {
        assert_eq!(
            parse_getvcp("VCP 10 C 90 100\n"),
            Some(BrightnessLevel {
                current: 90,
                max: 100
            })
        );
        assert_eq!(parse_getvcp("VCP 10 ERR\n"), None);
        assert_eq!(parse_getvcp("VCP 10 C 5 0\n"), None);
        let level = BrightnessLevel {
            current: 937,
            max: 1874,
        };
        assert_eq!(level.percent(), 50);
        assert_eq!(level.value_for(100), 1874);
        assert_eq!(level.value_for(0), 0);
    }

    #[test]
    fn a_display_request_needs_the_same_display_still_there() {
        let displays = parse_detect(DETECT);
        let lg = displays[0].clone();
        let level = Some(BrightnessLevel {
            current: 100,
            max: 100,
        });
        assert_eq!(revalidate_display(&displays, &lg, level, 60), Ok(60));
        assert_eq!(
            revalidate_display(&displays, &lg, level, 100),
            Err(BrightnessRefusal::AlreadySatisfied)
        );
        let mut moved = lg.clone();
        moved.serial = "OTHER".into();
        assert_eq!(
            revalidate_display(&displays, &moved, level, 60),
            Err(BrightnessRefusal::DisplayGone)
        );
        assert_eq!(
            revalidate_display(&displays[1..], &lg, level, 60),
            Err(BrightnessRefusal::DisplayGone)
        );
    }

    #[test]
    fn backlights_are_read_from_sysfs() {
        let root = std::env::temp_dir().join(format!("gnosix-backlight-{}", std::process::id()));
        let dir = root.join("intel_backlight");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("brightness"), "600\n").unwrap();
        std::fs::write(dir.join("max_brightness"), "1200\n").unwrap();
        let found = read_backlights(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "intel_backlight");
        assert_eq!(found[0].level.percent(), 50);
        assert!(read_backlights(&root.join("absent")).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
