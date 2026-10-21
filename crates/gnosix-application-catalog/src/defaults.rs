//! APPS-01: the default application for each role, in the user's
//! `mimeapps.list` (the freedesktop "MIME applications associations" file
//! every desktop and `xdg-open` read).
//!
//! A role (web browser, files, images…) is a set of MIME types and URI
//! schemes; choosing an application for a role makes it the default for all
//! of them. Writing keeps everything else in the file: other sections, other
//! types, comments and their order.

use crate::{ApplicationCatalog, InstalledApplication};

/// One role the Settings page offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultRole {
    WebBrowser,
    Mail,
    Files,
    TextEditor,
    Images,
    Video,
    Music,
    Pdf,
    Archives,
}

impl DefaultRole {
    pub const ALL: [DefaultRole; 9] = [
        Self::WebBrowser,
        Self::Mail,
        Self::Files,
        Self::TextEditor,
        Self::Images,
        Self::Video,
        Self::Music,
        Self::Pdf,
        Self::Archives,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::WebBrowser => "Web browser", // i18n
            Self::Mail => "Mail",              // i18n
            Self::Files => "Files",            // i18n
            Self::TextEditor => "Text editor", // i18n
            Self::Images => "Images",          // i18n
            Self::Video => "Video",            // i18n
            Self::Music => "Music",            // i18n
            Self::Pdf => "PDF",                // i18n
            Self::Archives => "Archives",      // i18n
        }
    }

    /// The types the role covers; the first is the one candidates must
    /// declare.
    pub fn types(self) -> &'static [&'static str] {
        match self {
            Self::WebBrowser => &[
                "x-scheme-handler/http",
                "x-scheme-handler/https",
                "text/html",
            ],
            Self::Mail => &["x-scheme-handler/mailto"],
            Self::Files => &["inode/directory"],
            Self::TextEditor => &["text/plain"],
            Self::Images => &["image/png", "image/jpeg", "image/gif", "image/webp"],
            Self::Video => &["video/mp4", "video/x-matroska", "video/webm"],
            Self::Music => &["audio/mpeg", "audio/flac", "audio/ogg"],
            Self::Pdf => &["application/pdf"],
            Self::Archives => &[
                "application/zip",
                "application/x-tar",
                "application/x-compressed-tar",
            ],
        }
    }
}

const SECTION: &str = "[Default Applications]";

/// The default desktop id for `mime` in `text` (the first listed).
pub fn mimeapps_default(text: &str, mime: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == SECTION;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == mime
        {
            return value
                .split(';')
                .map(str::trim)
                .find(|v| !v.is_empty())
                .map(str::to_owned);
        }
    }
    None
}

/// `text` with `desktop_id` the default for every one of `mimes`; every
/// other line is kept as it was.
pub fn mimeapps_set<'a>(text: &str, mimes: &[&'a str], desktop_id: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut seen_section = false;
    let mut written: Vec<&'a str> = Vec::new();
    let flush = |out: &mut Vec<String>, written: &mut Vec<&'a str>| {
        for mime in mimes {
            if !written.contains(mime) {
                out.push(format!("{mime}={desktop_id};"));
                written.push(mime);
            }
        }
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_section {
                // Leaving the section: what it did not have goes at its end,
                // before any blank lines that separate it from the next one.
                let mut blanks = 0;
                while out.last().is_some_and(|l| l.trim().is_empty()) {
                    out.pop();
                    blanks += 1;
                }
                flush(&mut out, &mut written);
                out.extend(std::iter::repeat_n(String::new(), blanks));
            }
            in_section = trimmed == SECTION;
            seen_section |= in_section;
            out.push(line.to_owned());
            continue;
        }
        if in_section
            && let Some((key, _)) = trimmed.split_once('=')
            && let Some(mime) = mimes.iter().find(|m| key.trim() == **m)
        {
            if !written.contains(mime) {
                out.push(format!("{mime}={desktop_id};"));
                written.push(mime);
            }
            continue;
        }
        out.push(line.to_owned());
    }
    if in_section {
        flush(&mut out, &mut written);
    } else if !seen_section {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(SECTION.to_owned());
        flush(&mut out, &mut written);
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// The visible applications that declare the role's main type, by name.
pub fn candidates(catalog: &ApplicationCatalog, role: DefaultRole) -> Vec<&InstalledApplication> {
    let main = role.types()[0];
    let mut apps: Vec<_> = catalog
        .visible_entries()
        .filter(|app| app.mime_types.iter().any(|m| m == main))
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// `$XDG_CONFIG_HOME/mimeapps.list`, else `~/.config/mimeapps.list`.
pub fn mimeapps_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))?;
    Some(base.join("mimeapps.list"))
}

/// Make `desktop_id` the default for `role` in the file at `path`
/// (written atomically).
pub fn set_default(
    path: &std::path::Path,
    role: DefaultRole,
    desktop_id: &str,
) -> std::io::Result<()> {
    if desktop_id.is_empty()
        || !desktop_id.ends_with(".desktop")
        || desktop_id.contains(['\n', ';', '=', '/'])
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a desktop id",
        ));
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let next = mimeapps_set(&text, role.types(), desktop_id);
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no directory"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".mimeapps.list.{}.tmp", std::process::id()));
    std::fs::write(&tmp, next)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "[Added Associations]\ntext/html=firefox.desktop;\n\n[Default Applications]\n# mine\naudio/mpeg=vlc.desktop\ntext/html=firefox.desktop;chromium.desktop;\n\n[Removed Associations]\nimage/png=gimp.desktop;\n";

    #[test]
    fn the_default_is_the_first_listed_in_the_defaults_section_only() {
        assert_eq!(
            mimeapps_default(LIST, "text/html").as_deref(),
            Some("firefox.desktop")
        );
        assert_eq!(
            mimeapps_default(LIST, "audio/mpeg").as_deref(),
            Some("vlc.desktop")
        );
        assert_eq!(mimeapps_default(LIST, "image/png"), None);
        assert_eq!(mimeapps_default("", "text/html"), None);
    }

    #[test]
    fn setting_a_role_rewrites_its_types_and_keeps_everything_else() {
        let out = mimeapps_set(LIST, DefaultRole::WebBrowser.types(), "brave.desktop");
        for mime in DefaultRole::WebBrowser.types() {
            assert_eq!(
                mimeapps_default(&out, mime).as_deref(),
                Some("brave.desktop"),
                "{mime}"
            );
        }
        assert!(out.contains("# mine"));
        assert!(out.contains("audio/mpeg=vlc.desktop"));
        assert!(out.contains("[Added Associations]\ntext/html=firefox.desktop;"));
        assert!(out.contains("[Removed Associations]\nimage/png=gimp.desktop;"));
        assert_eq!(
            out.matches("text/html=").count(),
            2,
            "one in Added, one in Default"
        );
        // New types land inside the section, before the blank separator.
        let section = out.split("[Removed Associations]").next().unwrap();
        assert!(section.contains("x-scheme-handler/https=brave.desktop;"));
        // Setting again changes nothing more.
        assert_eq!(
            mimeapps_set(&out, DefaultRole::WebBrowser.types(), "brave.desktop"),
            out
        );
    }

    #[test]
    fn a_file_without_the_section_gains_one() {
        let out = mimeapps_set(
            "[Added Associations]\na/b=x.desktop;\n",
            &["inode/directory"],
            "files.desktop",
        );
        assert_eq!(
            mimeapps_default(&out, "inode/directory").as_deref(),
            Some("files.desktop")
        );
        assert!(
            out.starts_with("[Added Associations]\na/b=x.desktop;\n\n[Default Applications]\n")
        );
        let empty = mimeapps_set("", &["text/plain"], "edit.desktop");
        assert_eq!(empty, "[Default Applications]\ntext/plain=edit.desktop;\n");
    }

    #[test]
    fn only_a_plain_desktop_id_is_written() {
        let dir = std::env::temp_dir().join(format!("gnosix-defaults-{}", std::process::id()));
        let path = dir.join("mimeapps.list");
        for bad in ["", "x", "a;b.desktop", "a\n.desktop", "../x.desktop"] {
            assert!(
                set_default(&path, DefaultRole::Pdf, bad).is_err(),
                "{bad:?}"
            );
        }
        set_default(&path, DefaultRole::Pdf, "okular.desktop").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            mimeapps_default(&text, "application/pdf").as_deref(),
            Some("okular.desktop")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
