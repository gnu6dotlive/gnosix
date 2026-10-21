//! ICONS-01: Gnosix's icons, Tabler Icons and Tabler Flags.
//!
//! The operator chose Tabler as Gnosix's icon set (2026-10-19). Every icon
//! ships with the shell, imported from the upstream archives by
//! `tools/tabler_import.py`: its path data on Tabler's 24 grid, its category
//! and its search tags. Nothing here paints; the scene strokes the segments
//! natively (2 units wide on the grid), so an icon takes any colour, size and
//! weight, and can be drawn on progressively. Flags are full-colour SVG
//! documents, handed over as bytes.
//!
//! Licences: `LICENSE-tabler-icons`, `LICENSE-tabler-flags` (MIT).

use std::sync::OnceLock;

mod flags_index;
mod path;

pub use path::{ParseError, Point, Segment, length, parse};

/// The grid every icon is drawn on.
pub const GRID: f32 = 24.0;
/// Tabler's stroke on that grid.
pub const STROKE: f32 = 2.0;

const OUTLINE: &str = include_str!("../data/outline.tsv");
const FILLED: &str = include_str!("../data/filled.tsv");
const FLAGS_TSV: &str = include_str!("../data/flags.tsv");
/// The imported upstream versions.
pub const VERSION: &str = include_str!("../data/VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    /// Stroked outlines (the default set).
    Outline,
    /// Filled shapes.
    Filled,
}

/// One icon: its name, Tabler category, search tags and path data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Icon {
    pub name: &'static str,
    pub style: Style,
    pub category: &'static str,
    tags: &'static str,
    pub path: &'static str,
}

impl Icon {
    pub fn tags(&self) -> impl Iterator<Item = &'static str> {
        self.tags.split(',').filter(|t| !t.is_empty())
    }

    /// The path as absolute segments on the grid.
    pub fn segments(&self) -> Vec<Segment> {
        parse(self.path).expect("imported icons parse (checked at import and in tests)")
    }
}

fn table(style: Style) -> &'static [Icon] {
    static OUT: OnceLock<Vec<Icon>> = OnceLock::new();
    static FILL: OnceLock<Vec<Icon>> = OnceLock::new();
    let (cell, data) = match style {
        Style::Outline => (&OUT, OUTLINE),
        Style::Filled => (&FILL, FILLED),
    };
    cell.get_or_init(|| {
        data.lines()
            .filter_map(|line| {
                let mut f = line.splitn(4, '\t');
                Some(Icon {
                    name: f.next()?,
                    style,
                    category: f.next()?,
                    tags: f.next()?,
                    path: f.next()?,
                })
            })
            .collect()
    })
}

/// The icon `name` in `style`, if Tabler has it.
pub fn icon(style: Style, name: &str) -> Option<Icon> {
    let t = table(style);
    t.binary_search_by(|i| i.name.cmp(name)).ok().map(|i| t[i])
}

/// Every icon of a style, by name.
pub fn all(style: Style) -> &'static [Icon] {
    table(style)
}

/// Icons whose name or tags contain `query` (for a picker).
pub fn search(style: Style, query: &str) -> impl Iterator<Item = Icon> + '_ {
    let q = query.to_lowercase();
    table(style)
        .iter()
        .copied()
        .filter(move |i| i.name.contains(&q) || i.tags().any(|t| t.contains(&q)))
}

/// A flag: its name, ISO 3166 code when it has one, category and English
/// display name, and its SVG document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flag {
    pub name: &'static str,
    pub iso: &'static str,
    pub category: &'static str,
    pub display_name: &'static str,
    pub svg: &'static [u8],
}

fn flags() -> &'static [Flag] {
    static FLAGS: OnceLock<Vec<Flag>> = OnceLock::new();
    FLAGS.get_or_init(|| {
        FLAGS_TSV
            .lines()
            .filter_map(|line| {
                let mut f = line.splitn(4, '\t');
                let name = f.next()?;
                let svg = flags_index::FLAGS
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, b)| *b)?;
                Some(Flag {
                    name,
                    iso: f.next()?,
                    category: f.next()?,
                    display_name: f.next()?,
                    svg,
                })
            })
            .collect()
    })
}

/// The flag by Tabler name (`france`) or ISO code (`FR`, any case).
pub fn flag(key: &str) -> Option<Flag> {
    flags()
        .iter()
        .find(|f| f.name == key || (!f.iso.is_empty() && f.iso.eq_ignore_ascii_case(key)))
        .copied()
}

pub fn all_flags() -> &'static [Flag] {
    flags()
}

#[cfg(test)]
mod tests;
