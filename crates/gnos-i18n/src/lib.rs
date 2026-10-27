//! I18N-01: the shell's texts in the operator's language.
//!
//! The English text is the key (`t("Do not disturb")`), so a text with no
//! translation shows in English rather than as a key. The catalogues
//! (`locales/<language>.json`, English text → translation) are built into
//! the binary; `tools/i18n.py` lists the texts the code uses and reports
//! those a catalogue lacks.
//!
//! Nothing is translated until a binary calls [`init`] or [`set_language`]:
//! tests and libraries see the English source.
//!
//! A text with values uses named slots: `tf("Wait {secs} s", &[("secs",
//! &n.to_string())])`; a translation keeps the slots it needs.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// The languages Gnosix ships, as (code, name in that language).
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("fr", "Français")];

const CATALOGUES: &[(&str, &str)] = &[("fr", include_str!("../locales/fr.json"))];

type Catalogue = HashMap<String, String>;

fn catalogues() -> &'static HashMap<&'static str, Catalogue> {
    static ALL: OnceLock<HashMap<&'static str, Catalogue>> = OnceLock::new();
    ALL.get_or_init(|| {
        CATALOGUES
            .iter()
            .map(|(code, text)| {
                let catalogue: Catalogue = serde_json::from_str(text).unwrap_or_default();
                (*code, catalogue)
            })
            .collect()
    })
}

/// The active language's catalogue; `None` is English.
static ACTIVE: RwLock<Option<&'static str>> = RwLock::new(None);

/// What the operator chose: a language code, or `None` for the system's.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LanguageChoice {
    pub language: Option<String>,
}

/// The shipped language for a locale string (`fr_CA.UTF-8` → `fr`).
pub fn language_of_locale(locale: &str) -> Option<&'static str> {
    let code = locale
        .split(['_', '.', '@', '-'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    LANGUAGES.iter().map(|(c, _)| *c).find(|c| *c == code)
}

/// The system's language: `LC_ALL`, then `LC_MESSAGES`, then `LANG`;
/// English when none names a shipped language.
pub fn system_language() -> &'static str {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .and_then(|v| language_of_locale(&v))
        .unwrap_or("en")
}

/// Make `code` the active language (an unshipped code is English).
pub fn set_language(code: &str) {
    let active: Option<&'static str> = LANGUAGES
        .iter()
        .position(|(c, _)| *c == code && *c != "en")
        .map(|i| LANGUAGES[i].0);
    if let Ok(mut slot) = ACTIVE.write() {
        *slot = active;
    }
}

/// The active language's code.
pub fn language() -> &'static str {
    ACTIVE.read().ok().and_then(|a| *a).unwrap_or("en")
}

/// `$XDG_CONFIG_HOME/gnosix/language.json`, else under `~/.config`.
pub fn choice_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))?;
    Some(base.join("gnosix").join("language.json"))
}

/// The stored choice (the system's when absent or unreadable).
pub fn load_choice() -> LanguageChoice {
    choice_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Store the choice atomically and make it active.
pub fn save_choice(choice: &LanguageChoice) -> std::io::Result<()> {
    if let Some(code) = &choice.language
        && !LANGUAGES.iter().any(|(c, _)| c == code)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unknown language",
        ));
    }
    let path = choice_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config directory"))?;
    let dir = path.parent().expect("a file in a directory");
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".language.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(choice)?)?;
    std::fs::rename(&tmp, &path)?;
    apply(choice);
    Ok(())
}

fn apply(choice: &LanguageChoice) {
    match choice.language.as_deref() {
        Some(code) => set_language(code),
        None => set_language(system_language()),
    }
}

/// For a binary at start: the stored choice, else the system's language.
pub fn init() {
    apply(&load_choice());
}

/// `source` in the active language (`source` itself when untranslated).
pub fn t(source: &'static str) -> &'static str {
    let Some(code) = ACTIVE.read().ok().and_then(|a| *a) else {
        return source;
    };
    catalogues()
        .get(code)
        .and_then(|c| c.get(source))
        .map_or(source, String::as_str)
}

/// `source` translated, then each `{name}` slot filled.
pub fn tf(source: &'static str, values: &[(&str, &str)]) -> String {
    let mut text = t(source).to_owned();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// Every source text a catalogue holds, for the checks.
pub fn catalogue_keys(code: &str) -> Vec<String> {
    let mut keys: Vec<String> = catalogues()
        .get(code)
        .map(|c| c.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_name_their_shipped_language() {
        assert_eq!(language_of_locale("fr_CA.UTF-8"), Some("fr"));
        assert_eq!(language_of_locale("fr"), Some("fr"));
        assert_eq!(language_of_locale("en_US.UTF-8"), Some("en"));
        assert_eq!(language_of_locale("de_DE.UTF-8"), None);
        assert_eq!(language_of_locale(""), None);
    }

    #[test]
    fn texts_translate_once_a_language_is_set_and_fall_back_to_english() {
        // One test owns the global language, so it never races another.
        assert_eq!(t("Do not disturb"), "Do not disturb");
        set_language("fr");
        assert_eq!(language(), "fr");
        assert_eq!(t("Do not disturb"), "Ne pas déranger");
        assert_eq!(t("A text nobody translated"), "A text nobody translated");
        assert_eq!(
            tf("Password for {network}", &[("network", "Maison")]),
            "Mot de passe pour Maison"
        );
        set_language("xx");
        assert_eq!(language(), "en");
        assert_eq!(t("Do not disturb"), "Do not disturb");
    }

    #[test]
    fn every_translation_keeps_the_slots_of_its_source() {
        for (code, _) in CATALOGUES {
            for (source, text) in catalogues().get(code).unwrap() {
                let slots = |s: &str| {
                    let mut v: Vec<String> = s
                        .split('{')
                        .skip(1)
                        .filter_map(|p| p.split_once('}').map(|(n, _)| n.to_owned()))
                        .collect();
                    v.sort();
                    v
                };
                assert_eq!(slots(source), slots(text), "{code}: {source:?}");
            }
        }
    }
}
