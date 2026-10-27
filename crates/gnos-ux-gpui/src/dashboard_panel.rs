//! SURF-03B (H01–H04): the dashboard, out of the Bar's clock.
//!
//! Hovering the clock extrudes it from the Bar like a status popout; a click
//! on the clock pins it open (`bar_popout_host` owns that). This module is
//! its content, pure: three tabs as in Caelestia, whose pages slide sideways
//! as the tab changes (position only, no fade):
//! - **Dash:** the month's calendar (navigable, today marked), the date and
//!   time, the system (user and host, kernel, uptime), the playing media in
//!   brief.
//! - **Media:** the effective MPRIS player: title, artists, player, progress
//!   and its transport controls, each as the player permits.
//! - **Resources:** CPU (with its recent history), memory, disk and
//!   temperature, read only.
//!
//! The scene gives it observed facts (`DashboardModel`) and turns its only
//! effect, a media control, into the existing MPRIS transport.

use crate::product_mpris::MprisOperation;
use gnos_i18n::{t, tf};
use std::time::Instant;

/// The default width; the operator's is `dashboard_width()` (SET-02D).
pub const DASHBOARD_WIDTH: f32 = 520.0;
pub const DASHBOARD_HEIGHT: f32 = 300.0;
const PAD: f32 = 14.0;
const TAB_TOP: f32 = 12.0;
const TAB_HEIGHT: f32 = 30.0;
const TAB_WIDTH: f32 = 104.0;
const BODY_TOP: f32 = TAB_TOP + TAB_HEIGHT + 12.0;
const CELL_W: f32 = 32.0;
const CELL_H: f32 = 25.0;
/// How fast the pages follow the tab (per second).
const SLIDE_RATE: f32 = 16.0;
/// CPU samples kept for its history.
pub const CPU_HISTORY: usize = 60;

/// SET-02D: the dashboard's width, as the operator set it.
pub fn dashboard_width() -> f32 {
    #[cfg(feature = "component-lab")]
    {
        crate::surface_prefs::current().dashboard_width as f32
    }
    #[cfg(not(feature = "component-lab"))]
    {
        DASHBOARD_WIDTH
    }
}

/// SET-02D: the tabs the operator shows, in order (at least one).
pub fn shown_tabs() -> Vec<DashTab> {
    #[cfg(feature = "component-lab")]
    let hidden = crate::surface_prefs::current().dashboard_hidden_tabs;
    #[cfg(not(feature = "component-lab"))]
    let hidden = std::collections::BTreeSet::<String>::new();
    let shown: Vec<DashTab> = DashTab::ALL
        .into_iter()
        .filter(|t| !hidden.contains(t.slug()))
        .collect();
    if shown.is_empty() {
        DashTab::ALL.to_vec()
    } else {
        shown
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DashTab {
    Dash,
    Media,
    Resources,
}

impl DashTab {
    pub const ALL: [DashTab; 3] = [DashTab::Dash, DashTab::Media, DashTab::Resources];

    pub fn index(self) -> usize {
        match self {
            Self::Dash => 0,
            Self::Media => 1,
            Self::Resources => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dash => t("Dash"),
            Self::Media => t("Media"),
            Self::Resources => t("Resources"),
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Dash => "dash",
            Self::Media => "media",
            Self::Resources => "resources",
        }
    }
}

/// A local civil date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CivilDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Day of the week, Monday = 0 … Sunday = 6 (Sakamoto).
pub fn weekday(year: i32, month: u8, day: u8) -> u8 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let sunday_first =
        (y + y / 4 - y / 100 + y / 400 + T[usize::from(month - 1)] + i32::from(day)).rem_euclid(7);
    ((sunday_first + 6) % 7) as u8
}

/// `(year, month)` moved by `offset` months.
pub fn add_months(year: i32, month: u8, offset: i32) -> (i32, u8) {
    let index = year * 12 + i32::from(month) - 1 + offset;
    (index.div_euclid(12), (index.rem_euclid(12) + 1) as u8)
}

pub const MONTHS: [&str; 12] = [
    "January",   // i18n
    "February",  // i18n
    "March",     // i18n
    "April",     // i18n
    "May",       // i18n
    "June",      // i18n
    "July",      // i18n
    "August",    // i18n
    "September", // i18n
    "October",   // i18n
    "November",  // i18n
    "December",  // i18n
];
pub const WEEKDAYS: [&str; 7] = [
    "Monday",    // i18n
    "Tuesday",   // i18n
    "Wednesday", // i18n
    "Thursday",  // i18n
    "Friday",    // i18n
    "Saturday",  // i18n
    "Sunday",    // i18n
];

/// The month as six weeks of seven days (Monday first); `None` outside it.
pub fn month_grid(year: i32, month: u8) -> [[Option<u8>; 7]; 6] {
    let mut grid = [[None; 7]; 6];
    let first = usize::from(weekday(year, month, 1));
    for day in 1..=days_in_month(year, month) {
        let i = first + usize::from(day) - 1;
        grid[i / 7][i % 7] = Some(day);
    }
    grid
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileView {
    pub user_host: Option<String>,
    pub kernel: Option<String>,
    pub uptime: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaControl {
    pub operation: MprisOperation,
    pub glyph: &'static str,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaView {
    pub title: String,
    pub artists: String,
    pub player: String,
    pub playback: &'static str,
    /// Only when both a length and a position were observed.
    pub progress: Option<f32>,
    pub controls: Vec<MediaControl>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourcesView {
    pub cpu: Option<f32>,
    pub cpu_history: Vec<f32>,
    /// (used, total) bytes.
    pub memory: Option<(u64, u64)>,
    pub disk: Option<(u64, u64)>,
    pub temperature: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DashboardModel {
    pub date: Option<CivilDate>,
    pub time: Option<String>,
    pub profile: ProfileView,
    pub media: Option<MediaView>,
    pub resources: ResourcesView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashButton {
    Tab(DashTab),
    PrevMonth,
    NextMonth,
    Media(MprisOperation),
}

impl DashButton {
    pub fn slug(self) -> String {
        match self {
            Self::Tab(t) => format!("tab_{}", t.slug()),
            Self::PrevMonth => "month_prev".to_owned(),
            Self::NextMonth => "month_next".to_owned(),
            Self::Media(op) => format!("media_{}", op.name().to_lowercase()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Strong,
    Normal,
    Muted,
    Accent,
}

/// The parts, in panel coordinates (pages already slid into place).
#[derive(Clone, Debug, PartialEq)]
pub enum DashRow {
    Text {
        text: String,
        size: f32,
        tone: Tone,
        right: bool,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Button {
        button: DashButton,
        text: String,
        active: bool,
        enabled: bool,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Day {
        day: u8,
        today: bool,
        weekend: bool,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Meter {
        fraction: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Spark {
        values: Vec<f32>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

impl DashRow {
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Text { x, y, w, h, .. }
            | Self::Button { x, y, w, h, .. }
            | Self::Day { x, y, w, h, .. }
            | Self::Meter { x, y, w, h, .. }
            | Self::Spark { x, y, w, h, .. } => (*x, *y, *w, *h),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DashOutcome {
    pub changed: bool,
    pub effects: Vec<MprisOperation>,
}

fn bytes(n: u64) -> String {
    let gib = n as f64 / (1024.0 * 1024.0 * 1024.0);
    if gib >= 100.0 {
        format!("{gib:.0} GiB")
    } else {
        format!("{gib:.1} GiB")
    }
}

pub struct DashboardPanel {
    tab: DashTab,
    /// The pages' position, following the tab (0 = Dash … 2 = Resources).
    slide: f32,
    month_offset: i32,
    model: DashboardModel,
    press: Option<DashButton>,
    last_tick: Option<Instant>,
}

impl Default for DashboardPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardPanel {
    pub fn new() -> Self {
        Self {
            tab: DashTab::Dash,
            slide: 0.0,
            month_offset: 0,
            model: DashboardModel::default(),
            press: None,
            last_tick: None,
        }
    }

    /// The tab shown: the chosen one, or the first shown when the operator
    /// hid it.
    pub fn tab(&self) -> DashTab {
        let shown = shown_tabs();
        if shown.contains(&self.tab) {
            self.tab
        } else {
            shown[0]
        }
    }

    /// A tab's place among the shown tabs.
    fn position(tab: DashTab) -> f32 {
        shown_tabs().iter().position(|t| *t == tab).unwrap_or(0) as f32
    }

    pub fn month_offset(&self) -> i32 {
        self.month_offset
    }

    pub fn model(&self) -> &DashboardModel {
        &self.model
    }

    pub fn set_model(&mut self, model: DashboardModel) -> bool {
        if model == self.model {
            return false;
        }
        self.model = model;
        true
    }

    /// The dashboard closed: next time it opens on this month.
    pub fn reset(&mut self) {
        self.month_offset = 0;
        self.press = None;
    }

    pub fn needs_frame(&self) -> bool {
        (self.slide - Self::position(self.tab())).abs() > 0.001
    }

    pub fn tick(&mut self, now: Instant, reduced: bool) -> bool {
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |p| now.duration_since(p).as_secs_f32())
            .clamp(0.0, 0.1);
        self.last_tick = Some(now);
        let target = Self::position(self.tab());
        if reduced {
            self.slide = target;
        } else {
            self.slide += (target - self.slide) * (1.0 - (-SLIDE_RATE * dt).exp());
            if (self.slide - target).abs() <= 0.001 {
                self.slide = target;
            }
        }
        let moving = self.needs_frame();
        if !moving {
            self.last_tick = None;
        }
        moving
    }

    fn shown_month(&self) -> Option<(i32, u8)> {
        let d = self.model.date?;
        Some(add_months(d.year, d.month, self.month_offset))
    }

    pub fn rows(&self) -> Vec<DashRow> {
        let mut rows = Vec::new();
        // The tabs stay; the pages slide under them.
        let width = dashboard_width();
        let shown = shown_tabs();
        let current = self.tab();
        let n = shown.len() as f32;
        let tabs_left = (width - n * TAB_WIDTH - (n - 1.0) * 6.0) / 2.0;
        for (i, tab) in shown.iter().enumerate() {
            rows.push(DashRow::Button {
                button: DashButton::Tab(*tab),
                text: tab.label().to_owned(),
                active: *tab == current,
                enabled: true,
                x: tabs_left + i as f32 * (TAB_WIDTH + 6.0),
                y: TAB_TOP,
                w: TAB_WIDTH,
                h: TAB_HEIGHT,
            });
        }
        for (i, tab) in shown.into_iter().enumerate() {
            let dx = (i as f32 - self.slide) * width;
            if dx.abs() >= width {
                continue;
            }
            let page = match tab {
                DashTab::Dash => self.dash_rows(),
                DashTab::Media => self.media_rows(),
                DashTab::Resources => self.resource_rows(),
            };
            rows.extend(page.into_iter().map(|r| shift(r, dx)));
        }
        rows
    }

    fn text(text: impl Into<String>, size: f32, tone: Tone, x: f32, y: f32, w: f32) -> DashRow {
        DashRow::Text {
            text: text.into(),
            size,
            tone,
            right: false,
            x,
            y,
            w,
            h: size + 6.0,
        }
    }

    fn dash_rows(&self) -> Vec<DashRow> {
        let mut rows = Vec::new();
        let left = PAD;
        let mut y = BODY_TOP;
        match self.shown_month() {
            Some((year, month)) => {
                rows.push(DashRow::Button {
                    button: DashButton::PrevMonth,
                    text: "‹".to_owned(),
                    active: false,
                    enabled: true,
                    x: left,
                    y,
                    w: 26.0,
                    h: 24.0,
                });
                rows.push(DashRow::Text {
                    text: format!("{} {year}", t(MONTHS[usize::from(month - 1)])),
                    size: 13.0,
                    tone: Tone::Strong,
                    right: false,
                    x: left + 34.0,
                    y,
                    w: 7.0 * CELL_W - 68.0,
                    h: 24.0,
                });
                rows.push(DashRow::Button {
                    button: DashButton::NextMonth,
                    text: "›".to_owned(),
                    active: false,
                    enabled: true,
                    x: left + 7.0 * CELL_W - 26.0,
                    y,
                    w: 26.0,
                    h: 24.0,
                });
                y += 28.0;
                for (i, name) in WEEKDAYS.iter().enumerate() {
                    rows.push(Self::text(
                        t(name).chars().take(2).collect::<String>(),
                        10.0,
                        Tone::Muted,
                        left + i as f32 * CELL_W + 9.0,
                        y,
                        CELL_W,
                    ));
                }
                y += 18.0;
                let today = self
                    .model
                    .date
                    .filter(|_| self.month_offset == 0)
                    .map(|d| d.day);
                for (week, days) in month_grid(year, month).iter().enumerate() {
                    for (i, day) in days.iter().enumerate() {
                        if let Some(day) = day {
                            rows.push(DashRow::Day {
                                day: *day,
                                today: today == Some(*day),
                                weekend: i >= 5,
                                x: left + i as f32 * CELL_W,
                                y: y + week as f32 * CELL_H,
                                w: CELL_W,
                                h: CELL_H,
                            });
                        }
                    }
                }
            }
            None => rows.push(Self::text(
                t("No date observed"),
                12.0,
                Tone::Muted,
                left,
                y,
                7.0 * CELL_W,
            )),
        }
        // The right column: the date, the system, the media in brief.
        let x = PAD + 7.0 * CELL_W + 22.0;
        let w = dashboard_width() - x - PAD;
        let mut y = BODY_TOP;
        if let Some(d) = self.model.date {
            rows.push(Self::text(
                t(WEEKDAYS[usize::from(weekday(d.year, d.month, d.day))]),
                12.0,
                Tone::Muted,
                x,
                y,
                w,
            ));
            y += 18.0;
            rows.push(Self::text(
                format!("{} {}", d.day, t(MONTHS[usize::from(d.month - 1)])),
                20.0,
                Tone::Strong,
                x,
                y,
                w,
            ));
            y += 28.0;
        }
        if let Some(time) = &self.model.time {
            rows.push(Self::text(time.clone(), 13.0, Tone::Accent, x, y, w));
            y += 26.0;
        }
        let p = &self.model.profile;
        for line in [&p.user_host, &p.kernel, &p.uptime].into_iter().flatten() {
            rows.push(Self::text(line.clone(), 11.0, Tone::Normal, x, y, w));
            y += 18.0;
        }
        y = y.max(BODY_TOP + 160.0);
        match &self.model.media {
            Some(m) => {
                rows.push(Self::text(
                    m.title.clone(),
                    12.0,
                    Tone::Strong,
                    x,
                    y,
                    w - 40.0,
                ));
                rows.push(Self::text(
                    m.artists.clone(),
                    10.0,
                    Tone::Muted,
                    x,
                    y + 18.0,
                    w - 40.0,
                ));
                if let Some(c) = m
                    .controls
                    .iter()
                    .find(|c| c.operation == MprisOperation::PlayPause)
                {
                    rows.push(DashRow::Button {
                        button: DashButton::Media(c.operation),
                        text: c.glyph.to_owned(),
                        active: false,
                        enabled: c.enabled,
                        x: x + w - 32.0,
                        y: y + 2.0,
                        w: 32.0,
                        h: 32.0,
                    });
                }
            }
            None => rows.push(Self::text("Nothing playing", 11.0, Tone::Muted, x, y, w)),
        }
        rows
    }

    fn media_rows(&self) -> Vec<DashRow> {
        let mut rows = Vec::new();
        let (x, w) = (PAD + 8.0, dashboard_width() - 2.0 * PAD - 16.0);
        let mut y = BODY_TOP + 8.0;
        let Some(m) = &self.model.media else {
            rows.push(Self::text("No player", 14.0, Tone::Muted, x, y, w));
            return rows;
        };
        rows.push(Self::text(m.title.clone(), 18.0, Tone::Strong, x, y, w));
        y += 30.0;
        rows.push(Self::text(m.artists.clone(), 12.0, Tone::Normal, x, y, w));
        y += 22.0;
        rows.push(Self::text(
            format!("{} · {}", m.player, m.playback),
            11.0,
            Tone::Muted,
            x,
            y,
            w,
        ));
        y += 30.0;
        if let Some(p) = m.progress {
            rows.push(DashRow::Meter {
                fraction: p,
                x,
                y,
                w,
                h: 6.0,
            });
        }
        y += 26.0;
        let n = m.controls.len() as f32;
        let size = 44.0;
        let gap = 18.0;
        let start = x + (w - n * size - (n - 1.0).max(0.0) * gap) / 2.0;
        for (i, c) in m.controls.iter().enumerate() {
            rows.push(DashRow::Button {
                button: DashButton::Media(c.operation),
                text: c.glyph.to_owned(),
                active: c.operation == MprisOperation::PlayPause,
                enabled: c.enabled,
                x: start + i as f32 * (size + gap),
                y,
                w: size,
                h: size,
            });
        }
        rows
    }

    fn resource_rows(&self) -> Vec<DashRow> {
        let mut rows = Vec::new();
        let r = &self.model.resources;
        let (x, w) = (PAD + 8.0, dashboard_width() - 2.0 * PAD - 16.0);
        let mut y = BODY_TOP + 2.0;
        let line = |rows: &mut Vec<DashRow>,
                    y: &mut f32,
                    label: &str,
                    value: String,
                    fraction: Option<f32>| {
            rows.push(Self::text(label, 12.0, Tone::Strong, x, *y, 120.0));
            rows.push(DashRow::Text {
                text: value,
                size: 11.0,
                tone: Tone::Muted,
                right: true,
                x: x + 120.0,
                y: *y,
                w: w - 120.0,
                h: 17.0,
            });
            *y += 20.0;
            if let Some(f) = fraction {
                rows.push(DashRow::Meter {
                    fraction: f,
                    x,
                    y: *y,
                    w,
                    h: 6.0,
                });
            }
            *y += 18.0;
        };
        line(
            &mut rows,
            &mut y,
            t("CPU"),
            r.cpu.map_or("—".to_owned(), |c| format!("{c:.0} %")),
            r.cpu.map(|c| c / 100.0),
        );
        if r.cpu_history.len() > 1 {
            rows.push(DashRow::Spark {
                values: r.cpu_history.clone(),
                x,
                y: y - 4.0,
                w,
                h: 28.0,
            });
            y += 30.0;
        }
        let usage = |u: Option<(u64, u64)>| {
            u.map_or("—".to_owned(), |(used, total)| {
                tf(
                    "{used} of {total}",
                    &[("used", &bytes(used)), ("total", &bytes(total))],
                )
            })
        };
        let frac =
            |u: Option<(u64, u64)>| u.map(|(a, b)| if b == 0 { 0.0 } else { a as f32 / b as f32 });
        line(
            &mut rows,
            &mut y,
            t("Memory"),
            usage(r.memory),
            frac(r.memory),
        );
        line(&mut rows, &mut y, t("Disk"), usage(r.disk), frac(r.disk));
        line(
            &mut rows,
            &mut y,
            t("Temperature"),
            r.temperature
                .map_or("—".to_owned(), |t| format!("{t:.0} °C")),
            r.temperature.map(|t| (t / 100.0).clamp(0.0, 1.0)),
        );
        rows
    }

    /// CURSOR-01: whether a press at this point acts (an enabled button).
    pub fn presses_at(&self, lx: f32, ly: f32) -> bool {
        self.button_at(lx, ly).is_some()
    }

    fn button_at(&self, lx: f32, ly: f32) -> Option<DashButton> {
        self.rows().into_iter().find_map(|r| match r {
            DashRow::Button {
                button,
                enabled: true,
                x,
                y,
                w,
                h,
                ..
            } if lx >= x
                && lx < x + w
                && ly >= y
                && ly < y + h
                && x >= 0.0
                && x + w <= dashboard_width() + 0.5 =>
            {
                Some(button)
            }
            _ => None,
        })
    }

    /// A press at panel coordinates.
    pub fn pointer_down(&mut self, lx: f32, ly: f32) -> DashOutcome {
        self.press = self.button_at(lx, ly);
        DashOutcome::default()
    }

    /// A release at panel coordinates: a button pressed and released fires.
    pub fn pointer_up(&mut self, lx: f32, ly: f32) -> DashOutcome {
        let mut out = DashOutcome::default();
        let Some(press) = self.press.take() else {
            return out;
        };
        if self.button_at(lx, ly) != Some(press) {
            return out;
        }
        out.changed = true;
        match press {
            DashButton::Tab(tab) => self.tab = tab,
            DashButton::PrevMonth => self.month_offset -= 1,
            DashButton::NextMonth => self.month_offset += 1,
            DashButton::Media(op) => out.effects.push(op),
        }
        out
    }
}

fn shift(row: DashRow, dx: f32) -> DashRow {
    let mut row = row;
    match &mut row {
        DashRow::Text { x, .. }
        | DashRow::Button { x, .. }
        | DashRow::Day { x, .. }
        | DashRow::Meter { x, .. }
        | DashRow::Spark { x, .. } => *x += dx,
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> DashboardModel {
        DashboardModel {
            date: Some(CivilDate {
                year: 2026,
                month: 10,
                day: 17,
            }),
            time: Some("08:51".to_owned()),
            profile: ProfileView {
                user_host: Some("user@host".to_owned()),
                kernel: Some("7.2".to_owned()),
                uptime: Some("3 h".to_owned()),
            },
            media: Some(MediaView {
                title: "Song".to_owned(),
                artists: "Band".to_owned(),
                player: "Player".to_owned(),
                playback: "Playing",
                progress: Some(0.25),
                controls: vec![
                    MediaControl {
                        operation: MprisOperation::Previous,
                        glyph: "«",
                        enabled: false,
                    },
                    MediaControl {
                        operation: MprisOperation::PlayPause,
                        glyph: "❚❚",
                        enabled: true,
                    },
                    MediaControl {
                        operation: MprisOperation::Next,
                        glyph: "»",
                        enabled: true,
                    },
                ],
            }),
            resources: ResourcesView {
                cpu: Some(12.0),
                cpu_history: vec![10.0, 12.0],
                memory: Some((4, 16)),
                disk: Some((50, 100)),
                temperature: Some(52.0),
            },
        }
    }

    fn centre(p: &DashboardPanel, b: DashButton) -> (f32, f32) {
        let r = p
            .rows()
            .into_iter()
            .find(|r| matches!(r, DashRow::Button { button, .. } if *button == b))
            .unwrap();
        let (x, y, w, h) = r.rect();
        (x + w / 2.0, y + h / 2.0)
    }

    fn click(p: &mut DashboardPanel, b: DashButton) -> DashOutcome {
        let (x, y) = centre(p, b);
        p.pointer_down(x, y);
        p.pointer_up(x, y)
    }

    #[test]
    fn calendar_arithmetic() {
        // 17 October 2026 is a Saturday; 1 October 2026 a Thursday.
        assert_eq!(weekday(2026, 10, 17), 5);
        assert_eq!(weekday(2026, 10, 1), 3);
        assert_eq!(weekday(2000, 1, 1), 5);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(add_months(2026, 12, 1), (2027, 1));
        assert_eq!(add_months(2026, 1, -1), (2025, 12));
        let g = month_grid(2026, 10);
        assert_eq!(g[0][3], Some(1));
        assert_eq!(g[0][2], None);
        assert_eq!(g[4][5], Some(31));
    }

    #[test]
    fn the_dash_marks_today_and_the_month_navigates() {
        let mut p = DashboardPanel::new();
        p.set_model(model());
        let today = |p: &DashboardPanel| {
            p.rows()
                .into_iter()
                .filter(|r| matches!(r, DashRow::Day { today: true, .. }))
                .count()
        };
        assert_eq!(today(&p), 1);
        assert!(click(&mut p, DashButton::NextMonth).changed);
        assert_eq!(p.month_offset(), 1);
        assert!(
            p.rows()
                .iter()
                .any(|r| matches!(r, DashRow::Text { text, .. } if text == "November 2026"))
        );
        assert_eq!(today(&p), 0, "today is not in November");
        p.reset();
        assert_eq!(p.month_offset(), 0);
    }

    #[test]
    fn tabs_slide_the_pages_and_media_controls_follow_the_player() {
        let mut p = DashboardPanel::new();
        p.set_model(model());
        assert!(click(&mut p, DashButton::Tab(DashTab::Media)).changed);
        assert_eq!(p.tab(), DashTab::Media);
        // Mid-slide the media page is still partly off to the right.
        let now = Instant::now();
        p.tick(now, false);
        p.tick(now + std::time::Duration::from_millis(16), false);
        assert!(p.needs_frame());
        let mut t = now;
        while p.needs_frame() {
            t += std::time::Duration::from_millis(16);
            p.tick(t, false);
        }
        // Settled: a disabled control fires nothing, an enabled one asks.
        let (x, y) = centre(&p, DashButton::Media(MprisOperation::Previous));
        p.pointer_down(x, y);
        assert!(
            p.pointer_up(x, y).effects.is_empty(),
            "the player forbids it"
        );
        assert_eq!(
            click(&mut p, DashButton::Media(MprisOperation::Next)).effects,
            vec![MprisOperation::Next]
        );
        // Reduced motion snaps.
        click(&mut p, DashButton::Tab(DashTab::Resources));
        p.tick(t + std::time::Duration::from_millis(16), true);
        assert!(!p.needs_frame());
        assert!(
            p.rows()
                .iter()
                .any(|r| matches!(r, DashRow::Text { text, .. } if text == "52 °C"))
        );
    }
}
