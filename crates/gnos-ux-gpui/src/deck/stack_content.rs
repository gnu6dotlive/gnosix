//! DECK-02A: what the AppStack holds, laid out (A01–A04).
//!
//! - **Tabs** (A01): All, then the categories that hold an application, in
//!   a fixed order. A category is the first of an entry's freedesktop
//!   categories that maps to one; an entry with none is Other.
//! - **Sections** (A01): with an empty search, All shows one row of the
//!   applications used most (favourites, then launches), then every
//!   application A–Z under letter headings; a category shows its own A–Z.
//!   Consecutive letters share a heading ("A–G") until they fill a row, so
//!   a sparse letter never leaves a row with one tile.
//! - **The search fit** (A02): while searching there are no headings and
//!   the grid takes only as many columns as there are results, so the
//!   envelope holds exactly the results.
//! - **Chips** (A03): the search modes by name, laid out in lines that wrap
//!   to the width they are given.
//! - **Marked letters** (A02): the letters of a name the query matched.
//!
//! Everything here is pure: rows in, rects out, relative to the content's
//! own origin.

use super::stack::{AppRow, LaunchFrequency, TILE, TILE_GAP};
use crate::product_application_search::ApplicationSearchMode;
use blobin_ui_core::layout::Rect;

/// A letter heading's height, and the gap under it.
pub const HEADING: f32 = 22.0;
const HEADING_GAP: f32 = 4.0;
/// Between two sections.
const SECTION_GAP: f32 = 10.0;
/// The tab strip and one chip line.
pub const TABS_HEIGHT: f32 = 30.0;
pub const CHIP_HEIGHT: f32 = 26.0;
const CHIP_GAP: f32 = 6.0;
const CHIP_PAD: f32 = 9.0;
/// A label's advance at 12 px, estimated per character class: wide (m, w),
/// narrow (i, l, t, f, r, j, I) and the rest.
fn label_width(label: &str) -> f32 {
    label
        .chars()
        .map(|c| match c {
            'm' | 'w' | 'M' | 'W' => 10.0,
            'i' | 'l' | 't' | 'f' | 'r' | 'j' | 'I' | ' ' => 4.0,
            c if c.is_uppercase() => 8.0,
            _ => 6.6,
        })
        .sum()
}
const TAB_PAD: f32 = 14.0;
const TAB_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppCategory {
    Internet,
    Office,
    Graphics,
    Media,
    Development,
    Games,
    System,
    Utilities,
    Other,
}

impl AppCategory {
    pub const ALL: [AppCategory; 9] = [
        AppCategory::Internet,
        AppCategory::Office,
        AppCategory::Graphics,
        AppCategory::Media,
        AppCategory::Development,
        AppCategory::Games,
        AppCategory::System,
        AppCategory::Utilities,
        AppCategory::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Internet => gnos_i18n::t("Internet"),
            Self::Office => gnos_i18n::t("Office"),
            Self::Graphics => gnos_i18n::t("Graphics"),
            Self::Media => gnos_i18n::t("Media"),
            Self::Development => gnos_i18n::t("Development"),
            Self::Games => gnos_i18n::t("Games"),
            Self::System => gnos_i18n::t("System"),
            Self::Utilities => gnos_i18n::t("Utilities"),
            Self::Other => gnos_i18n::t("Other"),
        }
    }

    /// The first of an entry's freedesktop categories that names one.
    pub fn of(categories: &[String]) -> Self {
        categories
            .iter()
            .find_map(|c| match c.as_str() {
                "Network" | "WebBrowser" | "Email" | "Chat" | "InstantMessaging" => {
                    Some(Self::Internet)
                }
                "Office" | "WordProcessor" | "Spreadsheet" | "Presentation" | "Calendar" => {
                    Some(Self::Office)
                }
                "Graphics" | "Photography" | "2DGraphics" | "3DGraphics" | "RasterGraphics"
                | "VectorGraphics" => Some(Self::Graphics),
                "AudioVideo" | "Audio" | "Video" | "Music" | "Player" => Some(Self::Media),
                "Development" | "IDE" | "Debugger" => Some(Self::Development),
                "Game" => Some(Self::Games),
                "System" | "Settings" | "TerminalEmulator" | "Monitor" | "FileManager" => {
                    Some(Self::System)
                }
                "Utility" | "Accessories" | "TextEditor" | "Archiving" | "Calculator" | "Clock" => {
                    Some(Self::Utilities)
                }
                _ => None,
            })
            .unwrap_or(Self::Other)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StackTab {
    #[default]
    All,
    Category(AppCategory),
}

impl StackTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => gnos_i18n::t("All"),
            Self::Category(c) => c.label(),
        }
    }

    pub fn slug(self) -> String {
        self.label().to_lowercase()
    }
}

/// All, then every category that holds a row.
pub fn tabs(rows: &[AppRow]) -> Vec<StackTab> {
    let mut tabs = vec![StackTab::All];
    for category in AppCategory::ALL {
        if rows
            .iter()
            .any(|r| AppCategory::of(&r.categories) == category)
        {
            tabs.push(StackTab::Category(category));
        }
    }
    tabs
}

/// The tab strip from `x`, each tab sized to its label, wrapping within
/// `width`, line under line from `y`. Returns the rects and the height.
pub fn tab_rects(tabs: &[StackTab], x: f32, y: f32, width: f32) -> (Vec<(StackTab, Rect)>, f32) {
    let (mut at, mut line) = (x, 0.0_f32);
    let rects = tabs
        .iter()
        .map(|tab| {
            let w = label_width(tab.label()) + 2.0 * TAB_PAD;
            if at > x && at + w > x + width {
                at = x;
                line += TABS_HEIGHT + TAB_GAP;
            }
            let rect = Rect::new(at, y + line, w, TABS_HEIGHT);
            at += w + TAB_GAP;
            (*tab, rect)
        })
        .collect();
    (rects, line + TABS_HEIGHT)
}

/// One laid-out item of the content.
#[derive(Clone, Debug, PartialEq)]
pub enum Laid {
    Heading {
        label: String,
        rect: Rect,
    },
    /// `row` indexes the rows given to `lay_out`.
    Tile {
        row: usize,
        rect: Rect,
    },
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ContentLayout {
    pub items: Vec<Laid>,
    /// The grid's columns (fewer than the room allows while searching).
    pub columns: usize,
    /// The whole content's height (it scrolls inside the viewport).
    pub height: f32,
    /// The tiles in reading order: indices into `items`.
    pub tiles: Vec<usize>,
}

impl ContentLayout {
    pub fn tile(&self, n: usize) -> Option<(usize, Rect)> {
        match self.items.get(*self.tiles.get(n)?)? {
            Laid::Tile { row, rect } => Some((*row, *rect)),
            Laid::Heading { .. } => None,
        }
    }

    /// The tile nearest in x on the visual row before (`-1`) or after
    /// (`+1`) the tile `n`'s; `n` itself at either end.
    pub fn vertical(&self, n: usize, direction: i32) -> usize {
        let Some((_, here)) = self.tile(n) else {
            return n;
        };
        let rows: Vec<(usize, Rect)> = (0..self.tiles.len())
            .filter_map(|i| self.tile(i).map(|(_, r)| (i, r)))
            .collect();
        let next_y = rows
            .iter()
            .map(|(_, r)| r.y)
            .filter(|y| {
                if direction < 0 {
                    *y < here.y - 0.5
                } else {
                    *y > here.y + 0.5
                }
            })
            .fold(None::<f32>, |best, y| match best {
                None => Some(y),
                Some(b) if (direction < 0 && y > b) || (direction > 0 && y < b) => Some(y),
                Some(b) => Some(b),
            });
        let Some(y) = next_y else {
            return n;
        };
        rows.iter()
            .filter(|(_, r)| (r.y - y).abs() < 0.5)
            .min_by(|(_, a), (_, b)| (a.x - here.x).abs().total_cmp(&(b.x - here.x).abs()))
            .map_or(n, |(i, _)| *i)
    }
}

/// The section key of a name: its first letter, upper-cased, or `#`.
fn initial(name: &str) -> String {
    match name.chars().find(|c| c.is_alphanumeric()) {
        Some(c) if c.is_alphabetic() => c.to_uppercase().collect(),
        _ => "#".to_owned(),
    }
}

/// A heading for the initials `first` to `last`.
fn range(first: &str, last: &str) -> String {
    if first == last {
        first.to_owned()
    } else {
        format!("{first}–{last}")
    }
}

/// Lay the content out for `columns` columns of tiles (A01, A02).
pub fn lay_out(
    rows: &[AppRow],
    frequency: &LaunchFrequency,
    tab: StackTab,
    searching: bool,
    columns: usize,
) -> ContentLayout {
    let columns = columns.max(1);
    let pitch = TILE + TILE_GAP;
    let mut layout = ContentLayout::default();
    let mut y = 0.0_f32;
    let place_tiles = |layout: &mut ContentLayout, indices: &[usize], y: &mut f32, cols: usize| {
        for (k, &row) in indices.iter().enumerate() {
            let rect = Rect::new(
                (k % cols) as f32 * pitch,
                *y + (k / cols) as f32 * pitch,
                TILE,
                TILE,
            );
            layout.tiles.push(layout.items.len());
            layout.items.push(Laid::Tile { row, rect });
        }
        if !indices.is_empty() {
            *y += indices.len().div_ceil(cols) as f32 * pitch - TILE_GAP;
        }
    };
    if searching {
        // A02: the search's own order, as many columns as results.
        let indices: Vec<usize> = (0..rows.len()).collect();
        let cols = columns.min(rows.len()).max(1);
        layout.columns = cols;
        place_tiles(&mut layout, &indices, &mut y, cols);
        layout.height = y.max(0.0);
        return layout;
    }
    layout.columns = columns;
    let in_tab: Vec<usize> = (0..rows.len())
        .filter(|&i| match tab {
            StackTab::All => true,
            StackTab::Category(c) => AppCategory::of(&rows[i].categories) == c,
        })
        .collect();
    let mut sections: Vec<(String, Vec<usize>)> = Vec::new();
    if tab == StackTab::All {
        let mut frequent: Vec<usize> = in_tab
            .iter()
            .copied()
            .filter(|&i| rows[i].favourite || frequency.get(&rows[i].entry) > 0)
            .collect();
        frequent.sort_by(|&a, &b| {
            rows[b]
                .favourite
                .cmp(&rows[a].favourite)
                .then_with(|| {
                    frequency
                        .get(&rows[b].entry)
                        .cmp(&frequency.get(&rows[a].entry))
                })
                .then_with(|| {
                    rows[a]
                        .name
                        .to_lowercase()
                        .cmp(&rows[b].name.to_lowercase())
                })
        });
        frequent.truncate(columns);
        if !frequent.is_empty() {
            sections.push(("Frequent".to_owned(), frequent));
        }
    }
    let mut alphabetical = in_tab;
    alphabetical.sort_by(|&a, &b| {
        rows[a]
            .name
            .to_lowercase()
            .cmp(&rows[b].name.to_lowercase())
    });
    // One group per initial, then consecutive initials merged until they
    // fill a row ("A–G").
    let mut letters: Vec<(String, Vec<usize>)> = Vec::new();
    for i in alphabetical {
        let key = initial(&rows[i].name);
        match letters.last_mut() {
            Some((label, members)) if *label == key => members.push(i),
            _ => letters.push((key, vec![i])),
        }
    }
    let mut group: Option<(String, String, Vec<usize>)> = None;
    for (letter, members) in letters {
        let (first, _, mut acc) =
            group
                .take()
                .unwrap_or((letter.clone(), letter.clone(), Vec::new()));
        acc.extend(members);
        if acc.len() >= columns {
            sections.push((range(&first, &letter), acc));
        } else {
            group = Some((first, letter, acc));
        }
    }
    if let Some((first, last, acc)) = group {
        sections.push((range(&first, &last), acc));
    }
    let width = columns as f32 * pitch - TILE_GAP;
    // SET-03: a category that holds one row may go without its heading.
    let one_row = tab != StackTab::All && sections.len() == 1 && sections[0].1.len() <= columns;
    let headed = !one_row || crate::surface_prefs::advanced("one_row_heading") == 1;
    for (n, (label, members)) in sections.iter().enumerate() {
        if n > 0 {
            y += SECTION_GAP;
        }
        if headed {
            layout.items.push(Laid::Heading {
                label: label.clone(),
                rect: Rect::new(0.0, y, width, HEADING),
            });
            y += HEADING + HEADING_GAP;
        }
        place_tiles(&mut layout, members, &mut y, columns);
    }
    layout.height = y.max(0.0);
    layout
}

/// The chips (A03): each search mode by name, and the prefix that selects
/// it (`None` for Name, the default).
pub const CHIPS: [(ApplicationSearchMode, &str, Option<&str>); 9] = [
    (ApplicationSearchMode::Name, "Name", None),
    (ApplicationSearchMode::Categories, "Category", Some("@c ")),
    (ApplicationSearchMode::Keywords, "Keywords", Some("@k ")),
    (
        ApplicationSearchMode::Description,
        "Description",
        Some("@d "),
    ),
    (ApplicationSearchMode::Exec, "Command", Some("@e ")),
    (ApplicationSearchMode::TerminalOnly, "Terminal", Some("@t ")),
    (ApplicationSearchMode::StartupClass, "Window", Some("@w ")),
    (ApplicationSearchMode::GenericName, "Generic", Some("@g ")),
    (ApplicationSearchMode::Id, "ID", Some("@i ")),
];

/// The chips laid out from `x`, wrapping within `width`, line under line
/// from `y`. Returns each chip's mode, label and rect, and the height used.
pub fn chip_rects(
    x: f32,
    y: f32,
    width: f32,
) -> (Vec<(ApplicationSearchMode, &'static str, Rect)>, f32) {
    let mut out = Vec::new();
    let (mut at, mut line) = (x, 0.0_f32);
    for (mode, label, _) in CHIPS {
        let w = label_width(label) + 2.0 * CHIP_PAD;
        if at > x && at + w > x + width {
            at = x;
            line += CHIP_HEIGHT + CHIP_GAP;
        }
        out.push((mode, label, Rect::new(at, y + line, w, CHIP_HEIGHT)));
        at += w + CHIP_GAP;
    }
    (out, line + CHIP_HEIGHT)
}

/// The text a chip makes of the field's `input`: its mode's prefix and the
/// input without any former prefix.
pub fn with_mode(input: &str, mode: ApplicationSearchMode) -> String {
    let bare = CHIPS
        .iter()
        .filter_map(|(_, _, p)| *p)
        .find_map(|p| input.strip_prefix(p))
        .unwrap_or(input);
    match CHIPS
        .iter()
        .find(|(m, _, _)| *m == mode)
        .and_then(|(_, _, p)| *p)
    {
        Some(prefix) => format!("{prefix}{bare}"),
        None => bare.to_owned(),
    }
}

/// The characters of `name` the query matched (A02): each query character,
/// in order, at its first case-insensitive occurrence after the previous
/// one. Empty when the query does not match in order.
pub fn marked(name: &str, query: &str) -> Vec<usize> {
    let chars: Vec<char> = name.chars().collect();
    let mut out = Vec::new();
    let mut from = 0;
    for q in query.chars().filter(|c| !c.is_whitespace()) {
        let q = q.to_lowercase().collect::<String>();
        let Some(i) =
            (from..chars.len()).find(|&i| chars[i].to_lowercase().collect::<String>() == q)
        else {
            return Vec::new();
        };
        out.push(i);
        from = i + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnosix_application_catalog::DesktopEntryId;

    fn row(id: &str, name: &str, categories: &[&str]) -> AppRow {
        AppRow {
            entry: DesktopEntryId::parse(format!("{id}.desktop")).unwrap(),
            generation: gnosix_application_catalog::ApplicationCatalog::empty()
                .generation()
                .clone(),
            name: name.to_owned(),
            detail: String::new(),
            icon: None,
            favourite: false,
            categories: categories.iter().map(|c| (*c).to_owned()).collect(),
            running: false,
            pinned: false,
        }
    }

    fn sample() -> Vec<AppRow> {
        vec![
            row("files", "Files", &["System", "FileManager"]),
            row("firefox", "Firefox", &["Network", "WebBrowser"]),
            row("gimp", "GIMP", &["Graphics"]),
            row("geany", "Geany", &["Development", "IDE"]),
            row("calc", "Calculator", &["Utility", "Calculator"]),
            row("x", "xterm", &["System", "TerminalEmulator"]),
            row("odd", "2048", &[]),
        ]
    }

    #[test]
    fn categories_map_and_tabs_list_only_what_holds_an_application() {
        assert_eq!(AppCategory::of(&["Network".into()]), AppCategory::Internet);
        assert_eq!(
            AppCategory::of(&["GTK".into(), "Graphics".into()]),
            AppCategory::Graphics
        );
        assert_eq!(AppCategory::of(&[]), AppCategory::Other);
        let tabs = tabs(&sample());
        let labels: Vec<_> = tabs.iter().map(|t| t.label()).collect();
        assert_eq!(
            labels,
            [
                "All",
                "Internet",
                "Graphics",
                "Development",
                "System",
                "Utilities",
                "Other"
            ]
        );
        let (rects, height) = tab_rects(&tabs, 10.0, 0.0, 2000.0);
        assert!(rects.windows(2).all(|w| w[1].1.x > w[0].1.x + w[0].1.width));
        assert_eq!(height, TABS_HEIGHT);
        let (narrow, height) = tab_rects(&tabs, 10.0, 0.0, 300.0);
        assert!(height > TABS_HEIGHT);
        assert!(narrow.iter().all(|(_, r)| r.x + r.width <= 310.5));
    }

    #[test]
    fn all_shows_the_frequent_row_then_letters_and_a_category_its_own() {
        let rows = sample();
        let mut frequency = LaunchFrequency::default();
        frequency.record(&rows[2].entry);
        frequency.record(&rows[2].entry);
        frequency.record(&rows[1].entry);
        let all = lay_out(&rows, &frequency, StackTab::All, false, 6);
        let headings: Vec<_> = all
            .items
            .iter()
            .filter_map(|i| match i {
                Laid::Heading { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        // #, C, F (2) and G (2) fill a row of six; X is left.
        assert_eq!(headings, ["Frequent", "#–G", "X"]);
        // The frequent row: GIMP (2 launches) then Firefox.
        assert_eq!(all.tile(0).unwrap().0, 2);
        assert_eq!(all.tile(1).unwrap().0, 1);
        // Every application once A–Z after it, so seven plus two tiles.
        assert_eq!(all.tiles.len(), 9);
        let system = lay_out(
            &rows,
            &frequency,
            StackTab::Category(AppCategory::System),
            false,
            6,
        );
        let names: Vec<_> = (0..system.tiles.len())
            .map(|n| rows[system.tile(n).unwrap().0].name.as_str())
            .collect();
        assert_eq!(names, ["Files", "xterm"]);
        // Headings and tiles never overlap, top to bottom.
        let mut last = -1.0;
        for item in &all.items {
            let r = match item {
                Laid::Heading { rect, .. } | Laid::Tile { rect, .. } => *rect,
            };
            assert!(r.y + 0.1 >= last || r.x > 0.0);
            last = r.y;
        }
        assert!(all.height > 6.0 * HEADING);
    }

    #[test]
    fn a_search_fits_its_results_without_headings() {
        let rows = sample();
        let two = lay_out(
            &rows[..2],
            &LaunchFrequency::default(),
            StackTab::All,
            true,
            6,
        );
        assert_eq!(two.columns, 2);
        assert!(two.items.iter().all(|i| matches!(i, Laid::Tile { .. })));
        assert_eq!(two.height, TILE);
        let seven = lay_out(&rows, &LaunchFrequency::default(), StackTab::All, true, 6);
        assert_eq!(seven.columns, 6);
        assert_eq!(seven.height, 2.0 * TILE + TILE_GAP);
    }

    #[test]
    fn the_cursor_moves_by_visual_rows_across_headings() {
        let rows = sample();
        let all = lay_out(&rows, &LaunchFrequency::default(), StackTab::All, false, 2);
        // Sections of two columns: #–C (2048, Calculator), F (Files,
        // Firefox), G (Geany, GIMP), X (xterm). Down from either of the
        // first row lands below it.
        assert_eq!(all.vertical(0, 1), 2);
        assert_eq!(all.vertical(1, 1), 3);
        // From Firefox (second column) down to GIMP (second column).
        assert_eq!(all.vertical(3, 1), 5);
        assert_eq!(all.vertical(5, -1), 3);
        assert_eq!(all.vertical(0, -1), 0);
        let last = all.tiles.len() - 1;
        assert_eq!(all.vertical(last, 1), last);
    }

    #[test]
    fn chips_wrap_and_rewrite_the_mode_prefix() {
        let (chips, height) = chip_rects(0.0, 0.0, 280.0);
        assert_eq!(chips.len(), 9);
        assert!(height > CHIP_HEIGHT, "nine chips wrap in 280 px");
        let (_, height) = chip_rects(0.0, 0.0, 616.0);
        assert_eq!(
            height, CHIP_HEIGHT,
            "nine chips on one line in a six-column AppStack"
        );
        assert!(chips.iter().all(|(_, _, r)| r.x + r.width <= 280.0 + 0.5));
        assert_eq!(
            with_mode("term", ApplicationSearchMode::TerminalOnly),
            "@t term"
        );
        assert_eq!(
            with_mode("@t term", ApplicationSearchMode::Keywords),
            "@k term"
        );
        assert_eq!(with_mode("@k term", ApplicationSearchMode::Name), "term");
    }

    #[test]
    fn marked_letters_follow_the_query_in_order() {
        assert_eq!(marked("Gamma Terminal", "term"), vec![6, 7, 8, 9]);
        assert_eq!(marked("Firefox", "ffx"), vec![0, 4, 6]);
        assert!(marked("Files", "xf").is_empty());
        assert!(marked("Files", "").is_empty());
    }
}
