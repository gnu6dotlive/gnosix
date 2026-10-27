//! Read-only XDG installed-application catalog.
//!
//! Discovery follows the ordered XDG data roots supplied by the caller.
//! Higher-precedence roots shadow lower-precedence desktop-file IDs. The
//! catalog validates Desktop Entry syntax needed by GNOS.UX but never launches
//! an application and never mutates XDG state.

// Re-exported: this type appears throughout the catalog's public API
// (`ApplicationCatalog::get`, `InstalledApplication::id`), so a consumer
// cannot name what it must pass without reaching into another crate.
pub use in_gnu_os_settings_core::DesktopEntryId;

pub mod defaults;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalePreferences {
    candidates: Vec<String>,
}

impl Default for LocalePreferences {
    fn default() -> Self {
        Self::from_environment()
    }
}

impl LocalePreferences {
    pub fn neutral() -> Self {
        Self {
            candidates: Vec::new(),
        }
    }

    pub fn from_locale(locale: &str) -> Self {
        let locale = locale.trim();
        if locale.is_empty() || locale == "C" || locale == "POSIX" {
            return Self::neutral();
        }

        let (locale_without_modifier, modifier) = locale
            .split_once('@')
            .map_or((locale, None), |(base, modifier)| (base, Some(modifier)));
        let base = locale_without_modifier
            .split_once('.')
            .map_or(locale_without_modifier, |(base, _encoding)| base);
        let (language, territory) = base
            .split_once('_')
            .map_or((base, None), |(language, territory)| {
                (language, Some(territory))
            });

        let mut candidates = Vec::new();
        let mut push = |value: String| {
            if !value.is_empty() && !candidates.contains(&value) {
                candidates.push(value);
            }
        };
        if let (Some(territory), Some(modifier)) = (territory, modifier) {
            push(format!("{language}_{territory}@{modifier}"));
        }
        if let Some(territory) = territory {
            push(format!("{language}_{territory}"));
        }
        if let Some(modifier) = modifier {
            push(format!("{language}@{modifier}"));
        }
        push(language.to_owned());

        Self { candidates }
    }

    pub fn from_environment() -> Self {
        if let Ok(language) = env::var("LANGUAGE") {
            for locale in language.split(':') {
                let preferences = Self::from_locale(locale);
                if !preferences.candidates.is_empty() {
                    return preferences;
                }
            }
        }
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(locale) = env::var(key) {
                let preferences = Self::from_locale(&locale);
                if !preferences.candidates.is_empty() {
                    return preferences;
                }
            }
        }
        Self::neutral()
    }

    fn localized_key_candidates(&self, key: &str) -> Vec<String> {
        self.candidates
            .iter()
            .map(|locale| format!("{key}[{locale}]"))
            .chain(std::iter::once(key.to_owned()))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationCatalogRoots {
    application_dirs: Vec<PathBuf>,
}

impl ApplicationCatalogRoots {
    pub fn from_application_dirs(application_dirs: Vec<PathBuf>) -> Self {
        Self { application_dirs }
    }

    pub fn from_environment() -> Result<Self, CatalogEnvironmentError> {
        let home = env::var_os("HOME").ok_or(CatalogEnvironmentError::MissingHome)?;
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&home).join(".local/share"));
        let data_dirs = env::var_os("XDG_DATA_DIRS")
            .map(|value| env::split_paths(&value).collect::<Vec<_>>())
            .unwrap_or_else(|| {
                vec![
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
            });

        let application_dirs = std::iter::once(data_home.join("applications"))
            .chain(data_dirs.into_iter().map(|dir| dir.join("applications")))
            .collect();
        Ok(Self { application_dirs })
    }

    pub fn application_dirs(&self) -> &[PathBuf] {
        &self.application_dirs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogEnvironmentError {
    MissingHome,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopEnvironment {
    names: Vec<String>,
}

impl DesktopEnvironment {
    pub fn from_environment() -> Self {
        Self::from_names(
            env::var("XDG_CURRENT_DESKTOP")
                .ok()
                .into_iter()
                .flat_map(|value| {
                    value
                        .split(':')
                        .filter(|name| !name.is_empty())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                }),
        )
    }

    pub fn from_names(names: impl IntoIterator<Item = String>) -> Self {
        let mut ordered = Vec::new();
        for name in names {
            if !name.is_empty() && !ordered.contains(&name) {
                ordered.push(name);
            }
        }
        Self { names: ordered }
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogGeneration([u8; 32]);

impl CatalogGeneration {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecTemplate {
    pub argv: Vec<String>,
    pub field_codes: BTreeSet<char>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecValidationError {
    Empty,
    UnterminatedQuote,
    TrailingEscape,
    TruncatedFieldCode,
    UnsupportedFieldCode(char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApplication {
    pub id: DesktopEntryId,
    pub source_path: PathBuf,
    pub source_precedence: usize,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub dbus_activatable: bool,
    pub exec: Option<ExecTemplate>,
    pub try_exec: Option<String>,
    pub startup_wm_class: Option<String>,
    /// Desktop-entry search metadata. These are presentation/indexing facts,
    /// never execution authority.
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub mime_types: Vec<String>,
    pub actions: Vec<String>,
    /// The `[Desktop Action <id>]` groups `Actions=` lists, in that order.
    /// An action without a group, a name or a valid `Exec` is left out: the
    /// entry itself stays launchable.
    pub desktop_actions: Vec<DesktopAction>,
}

/// One additional way to start an entry (freedesktop "Desktop Action"),
/// such as a new window or a private window. Like the entry's own `Exec`,
/// the template is execution data for the launch capability only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub exec: ExecTemplate,
}

impl InstalledApplication {
    pub fn desktop_action(&self, id: &str) -> Option<&DesktopAction> {
        self.desktop_actions.iter().find(|action| action.id == id)
    }

    pub fn normally_visible(&self) -> bool {
        !self.hidden && !self.no_display
    }

    pub fn visible_in(&self, desktop: &DesktopEnvironment) -> bool {
        if !self.normally_visible() {
            return false;
        }
        for current in desktop.names() {
            if self.only_show_in.iter().any(|name| name == current) {
                return true;
            }
            if self.not_show_in.iter().any(|name| name == current) {
                return false;
            }
        }
        self.only_show_in.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogIssueKind {
    ReadDirectory,
    ReadFile,
    InvalidUtf8,
    InvalidDesktopEntryId,
    MissingDesktopEntrySection,
    NonApplicationType,
    MissingName,
    MissingExec,
    InvalidExec(ExecValidationError),
    DuplicateDesktopIdWithinRoot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogIssue {
    pub path: PathBuf,
    pub kind: CatalogIssueKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationCatalog {
    generation: CatalogGeneration,
    desktop: DesktopEnvironment,
    entries: Vec<InstalledApplication>,
    issues: Vec<CatalogIssue>,
}

impl ApplicationCatalog {
    pub fn empty() -> Self {
        Self {
            generation: CatalogGeneration(Sha256::digest([]).into()),
            desktop: DesktopEnvironment::default(),
            entries: Vec::new(),
            issues: Vec::new(),
        }
    }

    pub fn discover(
        roots: &ApplicationCatalogRoots,
        locale: &LocalePreferences,
        desktop: DesktopEnvironment,
    ) -> Self {
        let mut selected = BTreeMap::<String, InstalledApplication>::new();
        let mut issues = Vec::new();
        let mut hasher = Sha256::new();

        for (precedence, root) in roots.application_dirs.iter().enumerate() {
            let mut files = Vec::new();
            collect_desktop_files(root, root, &mut files, &mut issues);
            files.sort();
            let mut seen_this_root = BTreeSet::new();

            for path in files {
                let Some(relative) = path.strip_prefix(root).ok() else {
                    continue;
                };
                let Some(id_string) = desktop_file_id(relative) else {
                    issues.push(CatalogIssue {
                        path: path.clone(),
                        kind: CatalogIssueKind::InvalidDesktopEntryId,
                    });
                    continue;
                };
                if !seen_this_root.insert(id_string.clone()) {
                    issues.push(CatalogIssue {
                        path: path.clone(),
                        kind: CatalogIssueKind::DuplicateDesktopIdWithinRoot,
                    });
                    continue;
                }

                let bytes = match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        issues.push(CatalogIssue {
                            path: path.clone(),
                            kind: CatalogIssueKind::ReadFile,
                        });
                        continue;
                    }
                };
                hasher.update(precedence.to_le_bytes());
                hasher.update(id_string.as_bytes());
                hasher.update(&bytes);

                if selected.contains_key(&id_string) {
                    continue;
                }

                let text = match std::str::from_utf8(&bytes) {
                    Ok(text) => text,
                    Err(_) => {
                        issues.push(CatalogIssue {
                            path: path.clone(),
                            kind: CatalogIssueKind::InvalidUtf8,
                        });
                        continue;
                    }
                };
                match parse_desktop_entry(&id_string, &path, precedence, text, locale) {
                    Ok(entry) => {
                        selected.insert(id_string, entry);
                    }
                    Err(kind) => issues.push(CatalogIssue {
                        path: path.clone(),
                        kind,
                    }),
                }
            }
        }

        let mut entries = selected.into_values().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        let generation = CatalogGeneration(hasher.finalize().into());
        Self {
            generation,
            desktop,
            entries,
            issues,
        }
    }

    pub fn generation(&self) -> &CatalogGeneration {
        &self.generation
    }

    pub fn entries(&self) -> &[InstalledApplication] {
        &self.entries
    }

    pub fn issues(&self) -> &[CatalogIssue] {
        &self.issues
    }

    pub fn issue_counts(&self) -> BTreeMap<CatalogIssueKind, usize> {
        let mut counts = BTreeMap::new();
        for issue in &self.issues {
            *counts.entry(issue.kind.clone()).or_insert(0) += 1;
        }
        counts
    }

    pub fn get(&self, id: &DesktopEntryId) -> Option<&InstalledApplication> {
        self.entries.iter().find(|entry| &entry.id == id)
    }

    pub fn visible_entries(&self) -> impl Iterator<Item = &InstalledApplication> {
        self.entries
            .iter()
            .filter(|entry| entry.visible_in(&self.desktop))
    }

    pub fn desktop_environment(&self) -> &DesktopEnvironment {
        &self.desktop
    }

    pub fn match_window_class(
        &self,
        class: &str,
        initial_class: &str,
    ) -> Vec<&InstalledApplication> {
        let mut matches = self
            .entries
            .iter()
            .filter(|entry| {
                entry.startup_wm_class.as_deref().is_some_and(|wm_class| {
                    wm_class.eq_ignore_ascii_case(class)
                        || wm_class.eq_ignore_ascii_case(initial_class)
                }) || desktop_id_stem(entry.id.as_str()).eq_ignore_ascii_case(class)
                    || desktop_id_stem(entry.id.as_str()).eq_ignore_ascii_case(initial_class)
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        matches
    }
}

fn desktop_id_stem(id: &str) -> &str {
    id.strip_suffix(".desktop").unwrap_or(id)
}

fn collect_desktop_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
    issues: &mut Vec<CatalogIssue>,
) {
    let read_dir = match fs::read_dir(current) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && current == root => return,
        Err(_) => {
            issues.push(CatalogIssue {
                path: current.to_path_buf(),
                kind: CatalogIssueKind::ReadDirectory,
            });
            return;
        }
    };

    let mut children = read_dir.filter_map(Result::ok).collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let file_type = match child.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            collect_desktop_files(root, &path, files, issues);
        } else if path.extension() == Some(OsStr::new("desktop")) {
            files.push(path);
        }
    }
}

fn desktop_file_id(relative: &Path) -> Option<String> {
    let components = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?;
    let id = components.join("-");
    DesktopEntryId::parse(id.clone()).ok().map(|_| id)
}

fn parse_desktop_entry(
    id: &str,
    path: &Path,
    precedence: usize,
    text: &str,
    locale: &LocalePreferences,
) -> Result<InstalledApplication, CatalogIssueKind> {
    let section =
        desktop_entry_section(text).ok_or(CatalogIssueKind::MissingDesktopEntrySection)?;
    if section.get("Type").map(String::as_str) != Some("Application") {
        return Err(CatalogIssueKind::NonApplicationType);
    }

    let name = localized_value(&section, "Name", locale)
        .filter(|value| !value.trim().is_empty())
        .ok_or(CatalogIssueKind::MissingName)?;
    let dbus_activatable = parse_bool(section.get("DBusActivatable"));
    let exec = match section.get("Exec").filter(|value| !value.trim().is_empty()) {
        Some(exec_raw) => Some(parse_exec(exec_raw).map_err(CatalogIssueKind::InvalidExec)?),
        None if dbus_activatable => None,
        None => return Err(CatalogIssueKind::MissingExec),
    };
    let id = DesktopEntryId::parse(id.to_owned())
        .map_err(|_| CatalogIssueKind::InvalidDesktopEntryId)?;
    let actions = parse_list(section.get("Actions"));
    let groups = desktop_action_sections(text);
    let desktop_actions = actions
        .iter()
        .filter_map(|action| {
            let group = groups.get(action)?;
            let name =
                localized_value(group, "Name", locale).filter(|value| !value.trim().is_empty())?;
            let exec = group
                .get("Exec")
                .filter(|value| !value.trim().is_empty())
                .and_then(|raw| parse_exec(raw).ok())?;
            Some(DesktopAction {
                id: action.clone(),
                name,
                icon: group.get("Icon").cloned().filter(|value| !value.is_empty()),
                exec,
            })
        })
        .collect();

    Ok(InstalledApplication {
        id,
        source_path: path.to_path_buf(),
        source_precedence: precedence,
        name,
        generic_name: localized_value(&section, "GenericName", locale),
        comment: localized_value(&section, "Comment", locale),
        icon: section
            .get("Icon")
            .cloned()
            .filter(|value| !value.is_empty()),
        dbus_activatable,
        exec,
        try_exec: section
            .get("TryExec")
            .cloned()
            .filter(|value| !value.is_empty()),
        startup_wm_class: section
            .get("StartupWMClass")
            .cloned()
            .filter(|value| !value.is_empty()),
        categories: parse_list(section.get("Categories")),
        keywords: parse_list(section.get("Keywords")),
        terminal: parse_bool(section.get("Terminal")),
        no_display: parse_bool(section.get("NoDisplay")),
        hidden: parse_bool(section.get("Hidden")),
        only_show_in: parse_list(section.get("OnlyShowIn")),
        not_show_in: parse_list(section.get("NotShowIn")),
        mime_types: parse_list(section.get("MimeType")),
        actions,
        desktop_actions,
    })
}

/// Every `[Desktop Action <id>]` group, by id. A repeated group keeps its
/// first occurrence.
fn desktop_action_sections(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut groups = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut current: Option<String> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches(char::from(13_u8));
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            current = section
                .strip_prefix("Desktop Action ")
                .filter(|id| !id.is_empty() && !groups.contains_key(*id))
                .map(ToOwned::to_owned);
            if let Some(id) = &current {
                groups.insert(id.clone(), BTreeMap::new());
            }
            continue;
        }
        let Some(group) = current.as_ref().and_then(|id| groups.get_mut(id)) else {
            continue;
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            continue;
        }
        group.insert(key.to_owned(), unescape_value(value));
    }
    groups
}

fn desktop_entry_section(text: &str) -> Option<BTreeMap<String, String>> {
    let mut in_desktop_entry = false;
    let mut found = false;
    let mut values = BTreeMap::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches(char::from(13_u8));
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            if found && section != "Desktop Entry" {
                break;
            }
            in_desktop_entry = section == "Desktop Entry";
            found |= in_desktop_entry;
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            continue;
        }
        values.insert(key.to_owned(), unescape_value(value));
    }
    found.then_some(values)
}

fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn localized_value(
    values: &BTreeMap<String, String>,
    key: &str,
    locale: &LocalePreferences,
) -> Option<String> {
    locale
        .localized_key_candidates(key)
        .into_iter()
        .find_map(|candidate| values.get(&candidate).cloned())
}

fn parse_bool(value: Option<&String>) -> bool {
    value.is_some_and(|value| value == "true")
}

fn parse_list(value: Option<&String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .split(';')
                .filter(|part| !part.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_exec(raw: &str) -> Result<ExecTemplate, ExecValidationError> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut token_started = false;

    for ch in raw.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            token_started = true;
            continue;
        }
        match ch {
            '\\' => {
                escaped = true;
                token_started = true;
            }
            '"' => {
                quoted = !quoted;
                token_started = true;
            }
            ch if ch.is_whitespace() && !quoted => {
                if token_started {
                    argv.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            _ => {
                current.push(ch);
                token_started = true;
            }
        }
    }
    if escaped {
        return Err(ExecValidationError::TrailingEscape);
    }
    if quoted {
        return Err(ExecValidationError::UnterminatedQuote);
    }
    if token_started {
        argv.push(current);
    }
    if argv.is_empty() || argv[0].is_empty() {
        return Err(ExecValidationError::Empty);
    }

    let mut field_codes = BTreeSet::new();
    for token in &argv {
        let mut chars = token.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                continue;
            }
            let code = chars
                .next()
                .ok_or(ExecValidationError::TruncatedFieldCode)?;
            if !matches!(code, 'f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k' | '%') {
                return Err(ExecValidationError::UnsupportedFieldCode(code));
            }
            if code != '%' {
                field_codes.insert(code);
            }
        }
    }

    Ok(ExecTemplate { argv, field_codes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(1);

    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new() -> Self {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let root =
                env::temp_dir().join(format!("gnosix-app-catalog-{}-{id}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn applications(&self, name: &str) -> PathBuf {
            let path = self.root.join(name).join("applications");
            fs::create_dir_all(&path).unwrap();
            path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    const APP: &str = "[Desktop Entry]\nType=Application\nName=Browser\nExec=browser %U\nIcon=browser\nMimeType=text/html;x-scheme-handler/http;\n";

    #[test]
    fn higher_precedence_root_shadows_lower_desktop_id() {
        let tree = TempTree::new();
        let high = tree.applications("high");
        let low = tree.applications("low");
        write(
            &low.join("browser.desktop"),
            &APP.replace("Name=Browser", "Name=Low"),
        );
        write(
            &high.join("browser.desktop"),
            &APP.replace("Name=Browser", "Name=High"),
        );

        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![high, low]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );
        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].name, "High");
        assert_eq!(catalog.entries()[0].source_precedence, 0);
    }

    #[test]
    fn nested_paths_form_desktop_file_ids_and_hidden_entry_shadows_lower() {
        let tree = TempTree::new();
        let high = tree.applications("high");
        let low = tree.applications("low");
        write(&low.join("vendor/app.desktop"), APP);
        write(
            &high.join("vendor/app.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=browser\nHidden=true\n",
        );

        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![high, low]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );
        assert_eq!(catalog.entries()[0].id.as_str(), "vendor-app.desktop");
        assert!(catalog.entries()[0].hidden);
        assert_eq!(catalog.visible_entries().count(), 0);
    }

    #[test]
    fn locale_resolution_prefers_specific_then_language_then_base() {
        let tree = TempTree::new();
        let apps = tree.applications("one");
        write(
            &apps.join("localized.desktop"),
            "[Desktop Entry]\nType=Application\nName=Base\nName[fr]=Français\nName[fr_CA]=Québécois\nExec=app\n",
        );
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![apps]),
            &LocalePreferences::from_locale("fr_CA.UTF-8"),
            DesktopEnvironment::default(),
        );
        assert_eq!(catalog.entries()[0].name, "Québécois");
    }

    #[test]
    fn desktop_actions_follow_the_actions_key_and_drop_what_they_cannot_honour() {
        let tree = TempTree::new();
        let apps = tree.applications("one");
        write(
            &apps.join("term.desktop"),
            "[Desktop Entry]\nType=Application\nName=Term\nExec=term\nActions=new-tab;unlisted-name;bad-exec;missing;window;\n\
             \n[Desktop Action window]\nName=New Window\nName[fr]=Nouvelle fenêtre\nExec=term --window\nIcon=window-new\n\
             \n[Desktop Action new-tab]\nName=New Tab\nExec=term --tab %u\n\
             \n[Desktop Action unlisted-name]\nExec=term --x\n\
             \n[Desktop Action bad-exec]\nName=Bad\nExec=term %Z\n\
             \n[Desktop Action stray]\nName=Stray\nExec=term --stray\n",
        );
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![apps]),
            &LocalePreferences::from_locale("fr_FR.UTF-8"),
            DesktopEnvironment::default(),
        );
        let entry = &catalog.entries()[0];
        assert_eq!(entry.name, "Term", "the entry group is not polluted");
        let ids: Vec<_> = entry
            .desktop_actions
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["new-tab", "window"],
            "Actions= order; the rest dropped"
        );
        let window = entry.desktop_action("window").unwrap();
        assert_eq!(window.name, "Nouvelle fenêtre");
        assert_eq!(window.icon.as_deref(), Some("window-new"));
        assert_eq!(window.exec.argv, ["term", "--window"]);
        assert!(entry.desktop_action("stray").is_none());
        assert_eq!(entry.exec.as_ref().unwrap().argv, ["term"]);
    }

    #[test]
    fn exec_is_tokenized_without_shell_and_unknown_field_codes_fail() {
        let parsed = parse_exec(r#"browser --profile "Work Profile" %U %%done"#).unwrap();
        assert_eq!(
            parsed.argv,
            ["browser", "--profile", "Work Profile", "%U", "%%done"]
        );
        assert_eq!(parsed.field_codes, BTreeSet::from(['U']));
        assert_eq!(
            parse_exec("browser %Z"),
            Err(ExecValidationError::UnsupportedFieldCode('Z'))
        );
        assert_eq!(
            parse_exec("browser \"unterminated"),
            Err(ExecValidationError::UnterminatedQuote)
        );
    }

    #[test]
    fn dbus_activatable_application_may_omit_exec() {
        let tree = TempTree::new();
        let apps = tree.applications("one");
        write(
            &apps.join("org.example.DBusApp.desktop"),
            "[Desktop Entry]\nType=Application\nName=D-Bus App\nDBusActivatable=true\n",
        );
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![apps]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );
        assert_eq!(catalog.issues(), &[]);
        let entry = &catalog.entries()[0];
        assert!(entry.dbus_activatable);
        assert_eq!(entry.exec, None);
    }

    #[test]
    fn visibility_obeys_current_desktop_only_show_in_and_not_show_in() {
        let tree = TempTree::new();
        let apps = tree.applications("one");
        write(
            &apps.join("kde.desktop"),
            "[Desktop Entry]\nType=Application\nName=KDE only\nExec=kde-only\nOnlyShowIn=KDE;\n",
        );
        write(
            &apps.join("not-hyprland.desktop"),
            "[Desktop Entry]\nType=Application\nName=Not Hyprland\nExec=no-hypr\nNotShowIn=Hyprland;\n",
        );
        write(
            &apps.join("generic.desktop"),
            "[Desktop Entry]\nType=Application\nName=Generic\nExec=generic\n",
        );
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![apps]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::from_names(["Hyprland".to_owned()]),
        );
        let visible = catalog
            .visible_entries()
            .map(|entry| entry.id.as_str().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(visible, vec!["generic.desktop"]);
    }

    #[test]
    fn categories_and_keywords_are_preserved_as_search_metadata() {
        let tree = TempTree::new();
        let apps = tree.applications("search-metadata");
        write(
            &apps.join("org.example.Editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\nCategories=Development;IDE;\nKeywords=code;rust;workspace;\n",
        );
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![apps]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );
        let entry = &catalog.entries()[0];
        assert_eq!(entry.categories, ["Development", "IDE"]);
        assert_eq!(entry.keywords, ["code", "rust", "workspace"]);
    }

    #[test]
    fn startup_wm_class_and_desktop_id_support_presentation_matching() {
        let tree = TempTree::new();
        let apps = tree.applications("one");
        write(
            &apps.join("org.example.Editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\nStartupWMClass=EditorWindow\n",
        );
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![apps]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );
        assert_eq!(
            catalog
                .match_window_class("EditorWindow", "irrelevant")
                .len(),
            1
        );
        assert_eq!(
            catalog
                .match_window_class("org.example.Editor", "irrelevant")
                .len(),
            1
        );
    }

    #[test]
    fn generation_changes_when_effective_source_bytes_change() {
        let tree = TempTree::new();
        let apps = tree.applications("one");
        let path = apps.join("browser.desktop");
        write(&path, APP);
        let roots = ApplicationCatalogRoots::from_application_dirs(vec![apps]);
        let first = ApplicationCatalog::discover(
            &roots,
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );
        write(&path, &APP.replace("Name=Browser", "Name=Changed"));
        let second = ApplicationCatalog::discover(
            &roots,
            &LocalePreferences::neutral(),
            DesktopEnvironment::default(),
        );
        assert_ne!(first.generation(), second.generation());
    }
}
