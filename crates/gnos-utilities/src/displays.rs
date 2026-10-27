//! DISPLAY-01: the outputs, and changing them.
//!
//! Hyprland is the authority: the outputs are read from `hyprctl -j
//! monitors all`, and a change is one `hl.monitor({...})` evaluated by
//! `hyprctl eval`. The Lua is built here from typed, checked values only (an
//! output name of letters, digits and `-`/`_`; numbers), so nothing a caller
//! passes is ever evaluated as code.
//!
//! A change takes effect at once and is observed by reading the outputs
//! again. Keeping it (the product asks, and reverts on its own when nobody
//! answers) is the caller's; [`layout_lua`] writes what is kept as a Lua
//! file Hyprland's own config can load.

use serde::{Deserialize, Serialize};
use std::process::Command;

/// One mode an output offers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    /// Refresh rate in millihertz (74960 for 74.96 Hz).
    pub refresh_mhz: u32,
}

impl DisplayMode {
    /// "2560x1440@74.96", as Hyprland takes it.
    pub fn spec(self) -> String {
        format!(
            "{}x{}@{}",
            self.width,
            self.height,
            format_hz(self.refresh_mhz)
        )
    }
}

fn format_hz(mhz: u32) -> String {
    format!("{:.2}", f64::from(mhz) / 1000.0)
}

/// One output as Hyprland reports it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayOutput {
    pub name: String,
    pub description: String,
    pub mode: DisplayMode,
    pub modes: Vec<DisplayMode>,
    /// Logical position (top-left), in layout pixels.
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    /// 0..=7, Hyprland's transform (1 is 90°).
    pub transform: u8,
    pub disabled: bool,
}

impl DisplayOutput {
    /// The output's size in layout pixels (rotated and scaled).
    pub fn logical_size(&self) -> (i32, i32) {
        let (w, h) = if self.transform % 2 == 1 {
            (self.mode.height, self.mode.width)
        } else {
            (self.mode.width, self.mode.height)
        };
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        (
            (w as f32 / scale).round() as i32,
            (h as f32 / scale).round() as i32,
        )
    }

    /// The distinct sizes it offers, largest first.
    pub fn sizes(&self) -> Vec<(u32, u32)> {
        let mut sizes: Vec<(u32, u32)> = self.modes.iter().map(|m| (m.width, m.height)).collect();
        sizes.sort_by(|a, b| (b.0 * b.1).cmp(&(a.0 * a.1)).then(b.0.cmp(&a.0)));
        sizes.dedup();
        sizes
    }

    /// The rates it offers at `size`, highest first.
    pub fn rates(&self, size: (u32, u32)) -> Vec<u32> {
        let mut rates: Vec<u32> = self
            .modes
            .iter()
            .filter(|m| (m.width, m.height) == size)
            .map(|m| m.refresh_mhz)
            .collect();
        rates.sort_unstable_by(|a, b| b.cmp(a));
        rates.dedup();
        rates
    }
}

/// What one output should become: every field Hyprland needs to place it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplaySpec {
    pub output: String,
    pub mode: DisplayMode,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub transform: u8,
}

impl DisplaySpec {
    /// The output as it is, its mode snapped to the nearest one it offers
    /// (Hyprland reports the measured rate, 164.917, for the 164.92 mode).
    pub fn of(output: &DisplayOutput) -> Self {
        let mode = output
            .modes
            .iter()
            .filter(|m| (m.width, m.height) == (output.mode.width, output.mode.height))
            .min_by_key(|m| m.refresh_mhz.abs_diff(output.mode.refresh_mhz))
            .copied()
            .unwrap_or(output.mode);
        Self {
            output: output.name.clone(),
            mode,
            x: output.x,
            y: output.y,
            scale: output.scale,
            transform: output.transform,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayError {
    /// The output's name is not a connector name.
    BadName,
    /// A value out of what Hyprland takes.
    BadValue(&'static str),
    Hyprctl(String),
}

/// The scales the product offers.
pub const SCALES: [f32; 8] = [1.0, 1.25, 1.333_333, 1.5, 1.6, 1.75, 2.0, 2.5];

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The one line of Lua that applies `spec`; checked values only.
pub fn monitor_lua(spec: &DisplaySpec) -> Result<String, DisplayError> {
    if !valid_name(&spec.output) {
        return Err(DisplayError::BadName);
    }
    if !(0.25..=4.0).contains(&spec.scale) || !spec.scale.is_finite() {
        return Err(DisplayError::BadValue("scale"));
    }
    if spec.transform > 7 {
        return Err(DisplayError::BadValue("transform"));
    }
    let m = spec.mode;
    if m.width == 0 || m.height == 0 || m.width > 16_384 || m.height > 16_384 {
        return Err(DisplayError::BadValue("mode"));
    }
    if spec.x.abs() > 100_000 || spec.y.abs() > 100_000 {
        return Err(DisplayError::BadValue("position"));
    }
    Ok(format!(
        "hl.monitor({{ output = \"{}\", mode = \"{}\", position = \"{}x{}\", scale = {:.6}, transform = {} }})",
        spec.output,
        m.spec(),
        spec.x,
        spec.y,
        spec.scale,
        spec.transform
    ))
}

/// The kept layout as a Lua file Hyprland's config can `dofile`.
pub fn layout_lua(specs: &[DisplaySpec]) -> Result<String, DisplayError> {
    let mut out = String::from(
        "-- Written by Gnosix (Settings > Display). Load it from hyprland.lua, after\n\
         -- your own hl.monitor lines: dofile(os.getenv(\"HOME\") .. \"/.config/gnosix/displays.lua\")\n",
    );
    for spec in specs {
        out.push_str(&monitor_lua(spec)?);
        out.push('\n');
    }
    Ok(out)
}

/// Apply `spec` now (`hyprctl eval`).
pub fn apply(spec: &DisplaySpec) -> Result<(), DisplayError> {
    let lua = monitor_lua(spec)?;
    let output = Command::new("hyprctl")
        .args(["eval", &lua])
        .output()
        .map_err(|e| DisplayError::Hyprctl(e.to_string()))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() && (text.is_empty() || text == "ok") {
        Ok(())
    } else {
        Err(DisplayError::Hyprctl(if text.is_empty() {
            String::from_utf8_lossy(&output.stderr).trim().to_owned()
        } else {
            text
        }))
    }
}

#[derive(Deserialize)]
struct RawMonitor {
    name: String,
    #[serde(default)]
    description: String,
    width: u32,
    height: u32,
    #[serde(rename = "refreshRate")]
    refresh_rate: f64,
    x: i32,
    y: i32,
    scale: f64,
    transform: u8,
    #[serde(default)]
    disabled: bool,
    #[serde(rename = "availableModes", default)]
    available_modes: Vec<String>,
}

fn parse_mode(text: &str) -> Option<DisplayMode> {
    // "2560x1440@74.96Hz"
    let (size, rate) = text.trim().trim_end_matches("Hz").split_once('@')?;
    let (w, h) = size.split_once('x')?;
    let rate: f64 = rate.parse().ok()?;
    Some(DisplayMode {
        width: w.parse().ok()?,
        height: h.parse().ok()?,
        refresh_mhz: (rate * 1000.0).round() as u32,
    })
}

/// Parse `hyprctl -j monitors all`.
pub fn parse_outputs(json: &str) -> Vec<DisplayOutput> {
    let Ok(raw) = serde_json::from_str::<Vec<RawMonitor>>(json) else {
        return Vec::new();
    };
    raw.into_iter()
        .map(|m| DisplayOutput {
            name: m.name,
            description: m.description,
            mode: DisplayMode {
                width: m.width,
                height: m.height,
                refresh_mhz: (m.refresh_rate * 1000.0).round() as u32,
            },
            modes: m
                .available_modes
                .iter()
                .filter_map(|s| parse_mode(s))
                .collect(),
            x: m.x,
            y: m.y,
            scale: m.scale as f32,
            transform: m.transform,
            disabled: m.disabled,
        })
        .collect()
}

/// The outputs now.
pub fn read_outputs() -> Result<Vec<DisplayOutput>, DisplayError> {
    let output = Command::new("hyprctl")
        .args(["-j", "monitors", "all"])
        .output()
        .map_err(|e| DisplayError::Hyprctl(e.to_string()))?;
    if !output.status.success() {
        return Err(DisplayError::Hyprctl(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(parse_outputs(&String::from_utf8_lossy(&output.stdout)))
}

/// Where an output stands against another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Above,
    Below,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Left, Side::Right, Side::Above, Side::Below];

    pub fn label(self) -> &'static str {
        match self {
            Side::Left => "Left of",   // i18n
            Side::Right => "Right of", // i18n
            Side::Above => "Above",    // i18n
            Side::Below => "Below",    // i18n
        }
    }
}

/// Which side of `other` `this` stands on now (by the centres).
pub fn side_of(this: &DisplayOutput, other: &DisplayOutput) -> Side {
    let (tw, th) = this.logical_size();
    let (ow, oh) = other.logical_size();
    let dx = (this.x + tw / 2) - (other.x + ow / 2);
    let dy = (this.y + th / 2) - (other.y + oh / 2);
    if dx.abs() >= dy.abs() {
        if dx < 0 { Side::Left } else { Side::Right }
    } else if dy < 0 {
        Side::Above
    } else {
        Side::Below
    }
}

/// The position that puts `this` against `other` on `side`, edge to edge,
/// centred along the shared edge.
pub fn place(this: &DisplayOutput, other: &DisplayOutput, side: Side) -> (i32, i32) {
    let (tw, th) = this.logical_size();
    let (ow, oh) = other.logical_size();
    match side {
        Side::Left => (other.x - tw, other.y + (oh - th) / 2),
        Side::Right => (other.x + ow, other.y + (oh - th) / 2),
        Side::Above => (other.x + (ow - tw) / 2, other.y - th),
        Side::Below => (other.x + (ow - tw) / 2, other.y + oh),
    }
}

/// One setting a Settings control steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplaySetting {
    Size,
    Rate,
    Scale,
    Rotation,
    /// Its side against the first output.
    Position,
}

/// The spec one step of `setting` makes of output `index`; `None` when the
/// step would leave what the output offers (its smallest size, its lowest
/// rate…) or the output is not there.
pub fn next_spec(
    outputs: &[DisplayOutput],
    index: usize,
    setting: DisplaySetting,
    dir: i8,
) -> Option<DisplaySpec> {
    let output = outputs.get(index)?;
    let mut spec = DisplaySpec::of(output);
    let step = |len: usize, at: usize| -> Option<usize> {
        let next = at as isize + isize::from(dir.signum());
        (0..len as isize).contains(&next).then_some(next as usize)
    };
    match setting {
        DisplaySetting::Size => {
            // The list is largest first: "up" is a larger size.
            let sizes = output.sizes();
            let at = sizes
                .iter()
                .position(|s| *s == (spec.mode.width, spec.mode.height))?;
            let next = at as isize - isize::from(dir.signum());
            let size = *sizes.get(usize::try_from(next).ok()?)?;
            let rate = *output.rates(size).first()?;
            spec.mode = DisplayMode {
                width: size.0,
                height: size.1,
                refresh_mhz: rate,
            };
        }
        DisplaySetting::Rate => {
            // Highest first: "up" is a higher rate.
            let size = (spec.mode.width, spec.mode.height);
            let rates = output.rates(size);
            let at = rates.iter().position(|r| *r == spec.mode.refresh_mhz)?;
            let next = at as isize - isize::from(dir.signum());
            spec.mode.refresh_mhz = *rates.get(usize::try_from(next).ok()?)?;
        }
        DisplaySetting::Scale => {
            let at = SCALES
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    (a.1 - spec.scale)
                        .abs()
                        .total_cmp(&(b.1 - spec.scale).abs())
                })?
                .0;
            spec.scale = SCALES[step(SCALES.len(), at)?];
        }
        DisplaySetting::Rotation => {
            // 0°, 90°, 180°, 270° (flips are kept as they are).
            let flipped = spec.transform & 4;
            spec.transform = flipped | (((spec.transform & 3) + 1) % 4);
        }
        DisplaySetting::Position => {
            let first = outputs.first()?;
            if index == 0 {
                return None;
            }
            let side = side_of(output, first);
            let at = Side::ALL.iter().position(|s| *s == side)?;
            let next = Side::ALL[(at + 1) % Side::ALL.len()];
            (spec.x, spec.y) = place(output, first, next);
        }
    }
    Some(spec)
}

/// "90°".
pub fn rotation_label(transform: u8) -> String {
    let degrees = u16::from(transform & 3) * 90;
    if transform & 4 != 0 {
        format!("{degrees}°, flipped")
    } else {
        format!("{degrees}°")
    }
}

/// "74.96 Hz".
pub fn rate_label(mhz: u32) -> String {
    format!("{} Hz", format_hz(mhz))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[
      {"name":"HDMI-A-1","description":"LG QHD","width":2560,"height":1440,"refreshRate":74.96400,
       "x":-630,"y":590,"scale":1.0,"transform":0,"disabled":false,
       "availableModes":["2560x1440@59.95Hz","2560x1440@74.96Hz","1920x1080@75.00Hz","1920x1080@60.00Hz"]},
      {"name":"DP-2","description":"Acer","width":1920,"height":1080,"refreshRate":164.91701,
       "x":1930,"y":390,"scale":1.0,"transform":1,"disabled":false,
       "availableModes":["1920x1080@164.92Hz","1920x1080@60.00Hz"]}
    ]"#;

    #[test]
    fn outputs_parse_with_their_modes_sizes_and_rates() {
        let outputs = parse_outputs(JSON);
        assert_eq!(outputs.len(), 2);
        let hdmi = &outputs[0];
        assert_eq!(
            hdmi.mode,
            DisplayMode {
                width: 2560,
                height: 1440,
                refresh_mhz: 74964
            }
        );
        assert_eq!(hdmi.sizes(), vec![(2560, 1440), (1920, 1080)]);
        assert_eq!(hdmi.rates((2560, 1440)), vec![74960, 59950]);
        assert_eq!(outputs[1].logical_size(), (1080, 1920));
        assert!(parse_outputs("not json").is_empty());
    }

    #[test]
    fn the_lua_is_built_from_checked_values_only() {
        let outputs = parse_outputs(JSON);
        let spec = DisplaySpec::of(&outputs[0]);
        assert_eq!(DisplaySpec::of(&outputs[1]).mode.refresh_mhz, 164920);
        assert_eq!(
            monitor_lua(&spec).unwrap(),
            r#"hl.monitor({ output = "HDMI-A-1", mode = "2560x1440@74.96", position = "-630x590", scale = 1.000000, transform = 0 })"#
        );
        for bad in ["", "HDMI\"; os.exit()", "a b", "x)"] {
            let mut s = spec.clone();
            s.output = bad.into();
            assert_eq!(monitor_lua(&s), Err(DisplayError::BadName), "{bad:?}");
        }
        let mut s = spec.clone();
        s.scale = f32::NAN;
        assert_eq!(monitor_lua(&s), Err(DisplayError::BadValue("scale")));
        let mut s = spec.clone();
        s.transform = 9;
        assert_eq!(monitor_lua(&s), Err(DisplayError::BadValue("transform")));
        let lua = layout_lua(&[spec]).unwrap();
        assert!(
            lua.contains("dofile")
                && lua.lines().filter(|l| l.starts_with("hl.monitor")).count() == 1
        );
    }

    #[test]
    fn placing_an_output_puts_it_edge_to_edge_and_reads_back_as_that_side() {
        let outputs = parse_outputs(JSON);
        let (hdmi, dp) = (&outputs[0], &outputs[1]);
        assert_eq!(side_of(dp, hdmi), Side::Right);
        for side in Side::ALL {
            let (x, y) = place(dp, hdmi, side);
            let moved = DisplayOutput { x, y, ..dp.clone() };
            assert_eq!(side_of(&moved, hdmi), side, "{side:?}");
        }
        // Right of HDMI (2560 wide at x -630): x = 1930, centred vertically.
        assert_eq!(
            place(dp, hdmi, Side::Right),
            (1930, 590 + (1440 - 1920) / 2)
        );
    }

    #[test]
    fn a_step_moves_through_what_the_output_offers_and_stops_at_its_ends() {
        let outputs = parse_outputs(JSON);
        let size_down = next_spec(&outputs, 0, DisplaySetting::Size, -1).unwrap();
        assert_eq!(
            (
                size_down.mode.width,
                size_down.mode.height,
                size_down.mode.refresh_mhz
            ),
            (1920, 1080, 75000)
        );
        assert_eq!(next_spec(&outputs, 0, DisplaySetting::Size, 1), None);
        let rate_down = next_spec(&outputs, 0, DisplaySetting::Rate, -1).unwrap();
        assert_eq!(rate_down.mode.refresh_mhz, 59950);
        assert_eq!(next_spec(&outputs, 0, DisplaySetting::Rate, 1), None);
        assert_eq!(
            next_spec(&outputs, 0, DisplaySetting::Scale, 1)
                .unwrap()
                .scale,
            1.25
        );
        assert_eq!(next_spec(&outputs, 0, DisplaySetting::Scale, -1), None);
        assert_eq!(
            next_spec(&outputs, 1, DisplaySetting::Rotation, 1)
                .unwrap()
                .transform,
            2
        );
        let flipped = DisplayOutput {
            transform: 7,
            ..outputs[1].clone()
        };
        assert_eq!(
            next_spec(
                &[outputs[0].clone(), flipped],
                1,
                DisplaySetting::Rotation,
                1
            )
            .unwrap()
            .transform,
            4
        );
        // DP-2 is right of HDMI-A-1; the next side is Above.
        let moved = next_spec(&outputs, 1, DisplaySetting::Position, 1).unwrap();
        let after = DisplayOutput {
            x: moved.x,
            y: moved.y,
            ..outputs[1].clone()
        };
        assert_eq!(side_of(&after, &outputs[0]), Side::Above);
        assert_eq!(next_spec(&outputs, 0, DisplaySetting::Position, 1), None);
        assert_eq!(next_spec(&outputs, 5, DisplaySetting::Scale, 1), None);
        for setting in [
            DisplaySetting::Size,
            DisplaySetting::Rate,
            DisplaySetting::Scale,
            DisplaySetting::Rotation,
            DisplaySetting::Position,
        ] {
            for dir in [-1, 1] {
                if let Some(spec) = next_spec(&outputs, 1, setting, dir) {
                    assert!(monitor_lua(&spec).is_ok(), "{setting:?} {dir}");
                }
            }
        }
        assert_eq!(rotation_label(1), "90°");
        assert_eq!(rate_label(74960), "74.96 Hz");
    }

    #[test]
    fn refresh_rates_format_as_hyprland_reads_them() {
        assert_eq!(format_hz(74960), "74.96");
        assert_eq!(format_hz(60000), "60.00");
        assert_eq!(format_hz(164917), "164.92");
        assert_eq!(
            parse_mode("1920x1080@164.92Hz").unwrap().refresh_mhz,
            164920
        );
    }
}
