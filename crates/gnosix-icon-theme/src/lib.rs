//! Native, read-only freedesktop icon-theme lookup.
//!
//! Resolves an icon *name* at a requested size to a file on the local
//! filesystem, following the Icon Theme Specification: `index.theme` parsing,
//! `Fixed`/`Scalable`/`Threshold` directory matching, closest-size fallback,
//! theme inheritance, `hicolor` as the last theme, then the flat fallback
//! directories. Nothing is ever written, no network or daemon is involved, and
//! results are deterministic: only constructed paths are stat-ed, in a fixed
//! order, so the same filesystem always gives the same answer.
//!
//! An item may bring its own directory (SNI `IconThemePath`). Observed order of
//! the reference shell: the normal theme lookup first, then that directory as a
//! fallback; both a flat `<dir>/<name>.<ext>` and a theme tree inside it are
//! accepted.
//!
//! Resolution is explicit: [`IconLookup::Resolved`], [`IconLookup::NotFound`] or
//! [`IconLookup::Unsupported`]. A missing icon never turns into a stand-in.
#![forbid(unsafe_code)]

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// Formats this resolver can hand to a renderer, in specification preference
/// order. XPM is recognised only to report it as unsupported.
const SUPPORTED_EXTENSIONS: [(&str, IconFormat); 2] =
    [("png", IconFormat::Png), ("svg", IconFormat::Svg)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconFormat {
    Png,
    Svg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedIcon {
    pub path: PathBuf,
    pub format: IconFormat,
    /// Nominal size of the matched directory (0 for flat files).
    pub directory_size: u32,
    pub scalable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// The only match has a format this resolver does not decode (for example `xpm`).
    Format(String),
    /// The name is not a theme name and not an absolute path (separators, `..`).
    InvalidName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconLookup {
    Resolved(ResolvedIcon),
    NotFound,
    Unsupported(UnsupportedReason),
}

/// Where to search. Everything is injected: nothing is read from the process
/// environment here, so tests and callers own the inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconThemeSearch {
    /// Icon base directories, highest priority first (each holds theme folders).
    pub roots: Vec<PathBuf>,
    /// Flat fallback directories (`pixmaps`), searched last.
    pub fallback_dirs: Vec<PathBuf>,
    /// Name of the current theme; `hicolor` is always searched after it.
    pub theme: String,
}

impl IconThemeSearch {
    pub fn new(roots: Vec<PathBuf>, theme: impl Into<String>) -> Self {
        Self {
            roots,
            fallback_dirs: vec![PathBuf::from("/usr/share/pixmaps")],
            theme: theme.into(),
        }
    }

    /// Base directories the specification names, from injected environment
    /// values: `$HOME/.icons`, `$XDG_DATA_HOME/icons`, then each
    /// `$XDG_DATA_DIRS/icons` (default `/usr/local/share:/usr/share`).
    pub fn default_roots(
        home: Option<&Path>,
        xdg_data_home: Option<&Path>,
        xdg_data_dirs: Option<&str>,
    ) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(home) = home {
            roots.push(home.join(".icons"));
        }
        match (xdg_data_home, home) {
            (Some(dir), _) if !dir.as_os_str().is_empty() => roots.push(dir.join("icons")),
            (_, Some(home)) => roots.push(home.join(".local/share/icons")),
            _ => {}
        }
        let dirs = xdg_data_dirs
            .filter(|dirs| !dirs.is_empty())
            .unwrap_or("/usr/local/share:/usr/share");
        roots.extend(
            dirs.split(':')
                .filter(|dir| !dir.is_empty())
                .map(|dir| Path::new(dir).join("icons")),
        );
        roots
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IconRequest<'a> {
    pub name: &'a str,
    /// Logical size in pixels.
    pub size: u32,
    pub scale: u32,
    /// The item's own directory (SNI `IconThemePath`), used after the theme.
    pub item_theme_path: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirKind {
    Fixed,
    Scalable,
    Threshold,
}

#[derive(Clone, Debug)]
struct ThemeDirectory {
    name: String,
    size: u32,
    scale: u32,
    kind: DirKind,
    min_size: u32,
    max_size: u32,
    threshold: u32,
}

impl ThemeDirectory {
    fn distance(&self, size: u32, scale: u32) -> u32 {
        let wanted = size * scale;
        // Zero inside [low, high], otherwise the gap to the nearest edge.
        let span = |low: u32, high: u32| {
            (low * self.scale)
                .saturating_sub(wanted)
                .max(wanted.saturating_sub(high * self.scale))
        };
        match self.kind {
            DirKind::Fixed => (self.size * self.scale).abs_diff(wanted),
            DirKind::Scalable => span(self.min_size, self.max_size),
            DirKind::Threshold => span(
                self.size.saturating_sub(self.threshold),
                self.size + self.threshold,
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ThemeIndex {
    inherits: Vec<String>,
    directories: Vec<ThemeDirectory>,
}

fn parse_index(text: &str) -> ThemeIndex {
    use std::collections::HashMap;
    let mut sections: Vec<(String, HashMap<String, String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name.to_owned(), HashMap::new()));
        } else if let (Some((_, keys)), Some((key, value))) =
            (sections.last_mut(), line.split_once('='))
        {
            // Localised keys (`Name[fr]`) never matter for lookup.
            if !key.contains('[') {
                keys.insert(key.trim().to_owned(), value.trim().to_owned());
            }
        }
    }
    let list = |value: Option<&String>| -> Vec<String> {
        value
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let Some((_, header)) = sections.iter().find(|(name, _)| name == "Icon Theme") else {
        return ThemeIndex::default();
    };
    let mut names = list(header.get("Directories"));
    names.extend(list(header.get("ScaledDirectories")));
    let number = |keys: &HashMap<String, String>, key: &str| -> Option<u32> {
        keys.get(key).and_then(|v| v.parse().ok())
    };
    let directories = names
        .into_iter()
        .filter_map(|name| {
            let (_, keys) = sections.iter().find(|(section, _)| *section == name)?;
            let size = number(keys, "Size")?;
            let kind = match keys.get("Type").map(String::as_str) {
                Some("Fixed") => DirKind::Fixed,
                Some("Scalable") => DirKind::Scalable,
                // Threshold is the specification's default.
                _ => DirKind::Threshold,
            };
            Some(ThemeDirectory {
                size,
                scale: number(keys, "Scale").unwrap_or(1),
                kind,
                min_size: number(keys, "MinSize").unwrap_or(size),
                max_size: number(keys, "MaxSize").unwrap_or(size),
                threshold: number(keys, "Threshold").unwrap_or(2),
                name,
            })
        })
        .collect();
    ThemeIndex {
        inherits: list(header.get("Inherits")),
        directories,
    }
}

/// First `index.theme` found for `theme` across the base directories.
fn load_index(roots: &[PathBuf], theme: &str) -> Option<ThemeIndex> {
    roots.iter().find_map(|root| {
        let text = fs::read_to_string(root.join(theme).join("index.theme")).ok()?;
        Some(parse_index(&text))
    })
}

enum FileMatch {
    Found(ResolvedIcon),
    OnlyUnsupported(String),
    Missing,
}

fn find_file(base: &Path, name: &str, directory_size: u32, scalable: bool) -> FileMatch {
    for (extension, format) in SUPPORTED_EXTENSIONS {
        let path = base.join(format!("{name}.{extension}"));
        if path.is_file() {
            return FileMatch::Found(ResolvedIcon {
                path,
                format,
                directory_size,
                scalable,
            });
        }
    }
    if base.join(format!("{name}.xpm")).is_file() {
        return FileMatch::OnlyUnsupported("xpm".to_owned());
    }
    FileMatch::Missing
}

struct Walk<'a> {
    roots: &'a [PathBuf],
    name: &'a str,
    size: u32,
    scale: u32,
    visited: HashSet<String>,
    unsupported: Option<String>,
}

impl Walk<'_> {
    /// `FindIconHelper`: exact directory match first, then the closest size,
    /// then the parents, depth first. A theme already visited is skipped, so
    /// inheritance cycles terminate.
    fn in_theme(&mut self, theme: &str) -> Option<ResolvedIcon> {
        if !self.visited.insert(theme.to_owned()) {
            return None;
        }
        let index = load_index(self.roots, theme);
        if let Some(index) = &index
            && let Some(found) = self.in_directories(theme, index)
        {
            return Some(found);
        }
        let parents = index.map(|i| i.inherits).unwrap_or_default();
        for parent in parents {
            if let Some(found) = self.in_theme(&parent) {
                return Some(found);
            }
        }
        None
    }

    fn candidate(&mut self, theme: &str, dir: &ThemeDirectory) -> Option<ResolvedIcon> {
        for root in self.roots {
            let base = root.join(theme).join(&dir.name);
            match find_file(&base, self.name, dir.size, dir.kind == DirKind::Scalable) {
                FileMatch::Found(icon) => return Some(icon),
                FileMatch::OnlyUnsupported(format) => {
                    self.unsupported.get_or_insert(format);
                }
                FileMatch::Missing => {}
            }
        }
        None
    }

    /// `DirectoryMatchesSize` and `DirectorySizeDistance` collapse into one
    /// rule: a directory that matches the size has distance 0, so taking the
    /// smallest distance (the first listed on ties) returns the same file the
    /// specification's exact-match phase would, and the closest one otherwise.
    fn in_directories(&mut self, theme: &str, index: &ThemeIndex) -> Option<ResolvedIcon> {
        let mut closest: Option<(u32, ResolvedIcon)> = None;
        for dir in &index.directories {
            let distance = dir.distance(self.size, self.scale);
            if closest.as_ref().is_some_and(|(best, _)| distance >= *best) {
                continue;
            }
            if let Some(found) = self.candidate(theme, dir) {
                closest = Some((distance, found));
            }
        }
        closest.map(|(_, icon)| icon)
    }
}

fn lookup_in_search(search: &IconThemeSearch, name: &str, size: u32, scale: u32) -> IconLookup {
    let mut walk = Walk {
        roots: &search.roots,
        name,
        size,
        scale,
        visited: HashSet::new(),
        unsupported: None,
    };
    if let Some(icon) = walk.in_theme(&search.theme) {
        return IconLookup::Resolved(icon);
    }
    if let Some(icon) = walk.in_theme("hicolor") {
        return IconLookup::Resolved(icon);
    }
    for dir in &search.fallback_dirs {
        match find_file(dir, name, 0, false) {
            FileMatch::Found(icon) => return IconLookup::Resolved(icon),
            FileMatch::OnlyUnsupported(format) => {
                walk.unsupported.get_or_insert(format);
            }
            FileMatch::Missing => {}
        }
    }
    match walk.unsupported {
        Some(format) => IconLookup::Unsupported(UnsupportedReason::Format(format)),
        None => IconLookup::NotFound,
    }
}

/// A name may be a bare theme name or an absolute file path; anything else with
/// path syntax is refused so a provider can never make the resolver wander.
fn classify_name(name: &str) -> Result<NameKind<'_>, UnsupportedReason> {
    if name.is_empty() || name.contains('\0') {
        return Err(UnsupportedReason::InvalidName);
    }
    if Path::new(name).is_absolute() {
        return Ok(NameKind::Absolute(Path::new(name)));
    }
    if name.contains('/') || name.contains("..") {
        return Err(UnsupportedReason::InvalidName);
    }
    Ok(NameKind::Theme(name))
}

enum NameKind<'a> {
    Theme(&'a str),
    Absolute(&'a Path),
}

fn lookup_absolute(path: &Path) -> IconLookup {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !path.is_file() {
        return IconLookup::NotFound;
    }
    for (supported, format) in SUPPORTED_EXTENSIONS {
        if extension.as_deref() == Some(supported) {
            return IconLookup::Resolved(ResolvedIcon {
                path: path.to_owned(),
                format,
                directory_size: 0,
                scalable: format == IconFormat::Svg,
            });
        }
    }
    IconLookup::Unsupported(UnsupportedReason::Format(extension.unwrap_or_default()))
}

/// Resolve one icon request.
pub fn lookup(search: &IconThemeSearch, request: &IconRequest<'_>) -> IconLookup {
    let name = match classify_name(request.name) {
        Ok(NameKind::Absolute(path)) => return lookup_absolute(path),
        Ok(NameKind::Theme(name)) => name,
        Err(reason) => return IconLookup::Unsupported(reason),
    };
    let (size, scale) = (request.size.max(1), request.scale.max(1));
    let themed = lookup_in_search(search, name, size, scale);
    let Some(item_dir) = request.item_theme_path else {
        return themed;
    };
    if matches!(themed, IconLookup::Resolved(_)) {
        return themed;
    }
    // The item's own directory is a fallback: flat files first, then a theme
    // tree inside it (it is searched as one more icon base directory).
    match find_file(item_dir, name, 0, false) {
        FileMatch::Found(icon) => return IconLookup::Resolved(icon),
        FileMatch::OnlyUnsupported(format) => {
            if matches!(themed, IconLookup::NotFound) {
                return IconLookup::Unsupported(UnsupportedReason::Format(format));
            }
        }
        FileMatch::Missing => {}
    }
    let mut roots = vec![item_dir.to_owned()];
    roots.extend(search.roots.iter().cloned());
    let inside = IconThemeSearch {
        roots,
        fallback_dirs: Vec::new(),
        theme: search.theme.clone(),
    };
    match lookup_in_search(&inside, name, size, scale) {
        found @ IconLookup::Resolved(_) => found,
        _ => themed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A scratch directory tree that is removed on drop.
    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "gnosix-icon-theme-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn icon(&self, relative: &str) {
            self.write(relative, "icon");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    const FLAT_THEME: &str = "[Icon Theme]\nName=Flat\nDirectories=16x16/apps,32x32/apps,48x48/apps,scalable/apps\n\n\
[16x16/apps]\nSize=16\nType=Fixed\n\n[32x32/apps]\nSize=32\nType=Fixed\n\n[48x48/apps]\nSize=48\nType=Fixed\n\n\
[scalable/apps]\nSize=64\nType=Scalable\nMinSize=8\nMaxSize=512\n";

    fn search(fixture: &Fixture, theme: &str) -> IconThemeSearch {
        IconThemeSearch {
            roots: vec![fixture.path("icons")],
            fallback_dirs: vec![fixture.path("pixmaps")],
            theme: theme.into(),
        }
    }

    fn request<'a>(name: &'a str, size: u32) -> IconRequest<'a> {
        IconRequest {
            name,
            size,
            scale: 1,
            item_theme_path: None,
        }
    }

    fn resolved(lookup: IconLookup) -> ResolvedIcon {
        match lookup {
            IconLookup::Resolved(icon) => icon,
            other => panic!("expected Resolved, got {other:?}"),
        }
    }

    #[test]
    fn an_exact_fixed_size_directory_wins() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        for dir in ["16x16", "32x32", "48x48"] {
            f.icon(&format!("icons/Flat/{dir}/apps/mail.png"));
        }
        let icon = resolved(lookup(&search(&f, "Flat"), &request("mail", 32)));
        assert_eq!(icon.path, f.path("icons/Flat/32x32/apps/mail.png"));
        assert_eq!((icon.directory_size, icon.scalable), (32, false));
        assert_eq!(icon.format, IconFormat::Png);
    }

    #[test]
    fn the_closest_size_is_chosen_when_none_matches_exactly() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.icon("icons/Flat/16x16/apps/mail.png");
        f.icon("icons/Flat/48x48/apps/mail.png");
        let s = search(&f, "Flat");
        assert_eq!(
            resolved(lookup(&s, &request("mail", 24))).directory_size,
            16
        );
        assert_eq!(
            resolved(lookup(&s, &request("mail", 40))).directory_size,
            48
        );
        // A tie goes to the directory listed first.
        assert_eq!(
            resolved(lookup(&s, &request("mail", 32))).directory_size,
            16
        );
    }

    #[test]
    fn a_scalable_directory_matches_any_size_in_its_range() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.icon("icons/Flat/scalable/apps/vector.svg");
        let s = search(&f, "Flat");
        let icon = resolved(lookup(&s, &request("vector", 24)));
        assert_eq!(icon.format, IconFormat::Svg);
        assert!(icon.scalable);
        // Outside the range it is still the closest, not an error.
        assert!(matches!(
            lookup(&s, &request("vector", 2048)),
            IconLookup::Resolved(_)
        ));
    }

    #[test]
    fn threshold_directories_match_within_their_threshold() {
        let f = Fixture::new();
        f.write(
            "icons/T/index.theme",
            "[Icon Theme]\nDirectories=24\n[24]\nSize=24\nType=Threshold\nThreshold=4\n",
        );
        f.icon("icons/T/24/glyph.png");
        let s = search(&f, "T");
        assert_eq!(
            resolved(lookup(&s, &request("glyph", 28))).directory_size,
            24
        );
        // Beyond the threshold it is no longer a match but is still the closest.
        assert_eq!(
            resolved(lookup(&s, &request("glyph", 40))).directory_size,
            24
        );
    }

    #[test]
    fn scale_must_match_for_an_exact_directory() {
        let f = Fixture::new();
        f.write(
            "icons/S/index.theme",
            "[Icon Theme]\nDirectories=24,24@2\n[24]\nSize=24\nType=Fixed\n[24@2]\nSize=24\nScale=2\nType=Fixed\n",
        );
        f.icon("icons/S/24/glyph.png");
        f.icon("icons/S/24@2/glyph.png");
        let s = search(&f, "S");
        let at_two = IconRequest {
            scale: 2,
            ..request("glyph", 24)
        };
        assert_eq!(
            resolved(lookup(&s, &at_two)).path,
            f.path("icons/S/24@2/glyph.png")
        );
        assert_eq!(
            resolved(lookup(&s, &request("glyph", 24))).path,
            f.path("icons/S/24/glyph.png")
        );
    }

    #[test]
    fn inheritance_walks_the_parents_depth_first() {
        let f = Fixture::new();
        f.write(
            "icons/Child/index.theme",
            "[Icon Theme]\nInherits=Mid\nDirectories=16\n[16]\nSize=16\nType=Fixed\n",
        );
        f.write(
            "icons/Mid/index.theme",
            "[Icon Theme]\nInherits=Base\nDirectories=16\n[16]\nSize=16\nType=Fixed\n",
        );
        f.write(
            "icons/Base/index.theme",
            "[Icon Theme]\nDirectories=16\n[16]\nSize=16\nType=Fixed\n",
        );
        f.icon("icons/Base/16/deep.png");
        f.icon("icons/Mid/16/shared.png");
        f.icon("icons/Base/16/shared.png");
        let s = search(&f, "Child");
        assert_eq!(
            resolved(lookup(&s, &request("deep", 16))).path,
            f.path("icons/Base/16/deep.png")
        );
        // The nearer ancestor shadows the farther one.
        assert_eq!(
            resolved(lookup(&s, &request("shared", 16))).path,
            f.path("icons/Mid/16/shared.png")
        );
    }

    #[test]
    fn an_inheritance_cycle_terminates_with_not_found() {
        let f = Fixture::new();
        f.write(
            "icons/A/index.theme",
            "[Icon Theme]\nInherits=B\nDirectories=16\n[16]\nSize=16\nType=Fixed\n",
        );
        f.write(
            "icons/B/index.theme",
            "[Icon Theme]\nInherits=A\nDirectories=16\n[16]\nSize=16\nType=Fixed\n",
        );
        assert_eq!(
            lookup(&search(&f, "A"), &request("ghost", 16)),
            IconLookup::NotFound
        );
    }

    #[test]
    fn hicolor_is_searched_after_the_current_theme_and_its_parents() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.write(
            "icons/hicolor/index.theme",
            "[Icon Theme]\nDirectories=48x48/apps\n[48x48/apps]\nSize=48\nType=Fixed\n",
        );
        f.icon("icons/hicolor/48x48/apps/legacy.png");
        let icon = resolved(lookup(&search(&f, "Flat"), &request("legacy", 48)));
        assert_eq!(icon.path, f.path("icons/hicolor/48x48/apps/legacy.png"));
    }

    #[test]
    fn the_current_theme_beats_hicolor_for_the_same_name() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.write(
            "icons/hicolor/index.theme",
            "[Icon Theme]\nDirectories=48x48/apps\n[48x48/apps]\nSize=48\nType=Fixed\n",
        );
        f.icon("icons/Flat/16x16/apps/both.png");
        f.icon("icons/hicolor/48x48/apps/both.png");
        let icon = resolved(lookup(&search(&f, "Flat"), &request("both", 48)));
        assert_eq!(icon.path, f.path("icons/Flat/16x16/apps/both.png"));
    }

    #[test]
    fn flat_fallback_directories_are_searched_last() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.icon("pixmaps/loose.png");
        let icon = resolved(lookup(&search(&f, "Flat"), &request("loose", 24)));
        assert_eq!(icon.path, f.path("pixmaps/loose.png"));
        assert_eq!(icon.directory_size, 0);
    }

    #[test]
    fn the_first_root_that_has_the_file_wins_across_roots() {
        let f = Fixture::new();
        for root in ["first", "second"] {
            f.write(&format!("{root}/Flat/index.theme"), FLAT_THEME);
            f.icon(&format!("{root}/Flat/32x32/apps/dup.png"));
        }
        let s = IconThemeSearch {
            roots: vec![f.path("first"), f.path("second")],
            fallback_dirs: Vec::new(),
            theme: "Flat".into(),
        };
        assert_eq!(
            resolved(lookup(&s, &request("dup", 32))).path,
            f.path("first/Flat/32x32/apps/dup.png")
        );
    }

    #[test]
    fn png_is_preferred_over_svg_in_one_directory() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.icon("icons/Flat/32x32/apps/pair.svg");
        f.icon("icons/Flat/32x32/apps/pair.png");
        assert_eq!(
            resolved(lookup(&search(&f, "Flat"), &request("pair", 32))).format,
            IconFormat::Png
        );
    }

    #[test]
    fn an_item_theme_path_is_a_fallback_after_the_theme() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.icon("icons/Flat/32x32/apps/shared.png");
        f.icon("itemdir/shared.png");
        f.icon("itemdir/private.png");
        let s = search(&f, "Flat");
        let item = f.path("itemdir");
        let with = |name| IconRequest {
            item_theme_path: Some(&item),
            ..request(name, 32)
        };
        // A name the theme has resolves in the theme, as observed.
        assert_eq!(
            resolved(lookup(&s, &with("shared"))).path,
            f.path("icons/Flat/32x32/apps/shared.png")
        );
        // A name only the item has resolves from its own directory.
        assert_eq!(
            resolved(lookup(&s, &with("private"))).path,
            f.path("itemdir/private.png")
        );
        // Without the item path the private name does not exist.
        assert_eq!(lookup(&s, &request("private", 32)), IconLookup::NotFound);
    }

    #[test]
    fn an_item_theme_path_may_hold_a_whole_theme_tree() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.write("itemdir/Flat/index.theme", FLAT_THEME);
        f.icon("itemdir/Flat/32x32/apps/bundled.png");
        let item = f.path("itemdir");
        let req = IconRequest {
            item_theme_path: Some(&item),
            ..request("bundled", 32)
        };
        assert_eq!(
            resolved(lookup(&search(&f, "Flat"), &req)).path,
            f.path("itemdir/Flat/32x32/apps/bundled.png")
        );
    }

    #[test]
    fn not_found_is_deterministic_and_explicit() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        let s = search(&f, "Flat");
        for _ in 0..3 {
            assert_eq!(lookup(&s, &request("absent", 32)), IconLookup::NotFound);
        }
        // A theme that does not exist is also just NotFound for a missing icon.
        assert_eq!(
            lookup(&search(&f, "NoSuchTheme"), &request("absent", 32)),
            IconLookup::NotFound
        );
    }

    #[test]
    fn xpm_only_is_reported_unsupported_not_missing() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.icon("icons/Flat/32x32/apps/old.xpm");
        assert_eq!(
            lookup(&search(&f, "Flat"), &request("old", 32)),
            IconLookup::Unsupported(UnsupportedReason::Format("xpm".into()))
        );
    }

    #[test]
    fn absolute_paths_resolve_by_extension_and_nothing_else() {
        let f = Fixture::new();
        f.icon("abs/app.png");
        f.icon("abs/app.ico");
        let s = search(&f, "Flat");
        let abs = |name: &str| f.path(name).to_string_lossy().into_owned();
        assert_eq!(
            resolved(lookup(&s, &request(&abs("abs/app.png"), 24))).format,
            IconFormat::Png
        );
        assert_eq!(
            lookup(&s, &request(&abs("abs/app.ico"), 24)),
            IconLookup::Unsupported(UnsupportedReason::Format("ico".into()))
        );
        assert_eq!(
            lookup(&s, &request(&abs("abs/missing.png"), 24)),
            IconLookup::NotFound
        );
    }

    #[test]
    fn names_with_path_syntax_are_refused_never_traversed() {
        let f = Fixture::new();
        f.write("icons/Flat/index.theme", FLAT_THEME);
        f.icon("secret.png");
        let s = search(&f, "Flat");
        for name in ["../secret", "sub/dir", "a/../b", "", "x\0y"] {
            assert_eq!(
                lookup(&s, &request(name, 32)),
                IconLookup::Unsupported(UnsupportedReason::InvalidName),
                "{name:?}"
            );
        }
    }

    #[test]
    fn default_roots_follow_the_specification_from_injected_environment() {
        let roots = IconThemeSearch::default_roots(
            Some(Path::new("/home/u")),
            None,
            Some("/opt/share:/usr/share"),
        );
        assert_eq!(
            roots,
            [
                PathBuf::from("/home/u/.icons"),
                PathBuf::from("/home/u/.local/share/icons"),
                PathBuf::from("/opt/share/icons"),
                PathBuf::from("/usr/share/icons"),
            ]
        );
        let defaults = IconThemeSearch::default_roots(None, Some(Path::new("/data")), None);
        assert_eq!(
            defaults,
            [
                PathBuf::from("/data/icons"),
                PathBuf::from("/usr/local/share/icons"),
                PathBuf::from("/usr/share/icons"),
            ]
        );
    }

    #[test]
    fn index_parsing_ignores_comments_localised_keys_and_unknown_sections() {
        let index = parse_index(
            "# comment\n[Icon Theme]\nName[fr]=Plat\nInherits=Base, Other\nDirectories=a,missing\n\n[a]\nSize=16\nType=Scalable\nMinSize=8\nMaxSize=64\n[unrelated]\nSize=999\n",
        );
        assert_eq!(index.inherits, ["Base", "Other"]);
        // `missing` has no section and is dropped rather than guessed.
        assert_eq!(index.directories.len(), 1);
        assert_eq!(index.directories[0].kind, DirKind::Scalable);
        assert_eq!(
            (index.directories[0].min_size, index.directories[0].max_size),
            (8, 64)
        );
    }

    #[test]
    fn directory_types_compete_by_distance_not_by_listing_order() {
        let f = Fixture::new();
        // The Fixed directory is listed first but is 5 px away; the Threshold
        // and Scalable directories contain the requested size.
        f.write(
            "icons/D/index.theme",
            "[Icon Theme]\nDirectories=fixed,threshold,scalable\n[fixed]\nSize=32\nType=Fixed\n[threshold]\nSize=24\nType=Threshold\nThreshold=4\n[scalable]\nSize=64\nType=Scalable\nMinSize=40\nMaxSize=80\n",
        );
        for dir in ["fixed", "threshold", "scalable"] {
            f.icon(&format!("icons/D/{dir}/who.png"));
        }
        let s = search(&f, "D");
        assert_eq!(
            resolved(lookup(&s, &request("who", 27))).path,
            f.path("icons/D/threshold/who.png")
        );
        assert_eq!(
            resolved(lookup(&s, &request("who", 60))).path,
            f.path("icons/D/scalable/who.png")
        );
        // 36 is 4 from the Fixed size, 8 from Threshold's upper edge, 4 from Scalable's lower edge.
        assert_eq!(
            resolved(lookup(&s, &request("who", 36))).path,
            f.path("icons/D/fixed/who.png")
        );
    }
}
