//! Which windows may be shown as a live thumbnail.
//!
//! The operator chose a deny-list (2026-10-20): every window may be shown,
//! except the applications listed. A first list of sensitive applications
//! (password managers, authenticators, secret stores, encryption, the
//! prompts that ask for a password) is the default, and the operator edits
//! it in Settings › Behaviour.
//!
//! An entry is an application's class (its Wayland app id), compared without
//! case; an entry ending in `*` covers every class that starts with what
//! comes before it. A window is excluded when its class or its initial class
//! matches.

/// The sensitive applications excluded until the operator says otherwise.
pub const DEFAULT_EXCLUDED: &[&str] = &[
    // Password managers.
    "org.keepassxc.KeePassXC",
    "keepassxc",
    "Bitwarden",
    "1Password",
    "Proton Pass",
    "Enpass",
    // Secret stores and their managers.
    "org.gnome.World.Secrets",
    "org.gnome.seahorse.Application",
    "org.kde.kwalletmanager*",
    "org.kde.kwalletd*",
    "org.kde.ksecretd",
    // Authenticators.
    "com.belmoussaoui.Authenticator",
    "Authy Desktop",
    // Encryption.
    "veracrypt",
    // What asks for a password: pinentry, keyring and polkit prompts.
    "org.gnupg.pinentry*",
    "pinentry*",
    "gcr-prompter",
    "hyprpolkitagent",
    "polkit-*",
    "org.kde.polkit-kde-authentication-agent-1",
    "lxqt-policykit-agent",
];

/// Whether `entry` names `class`.
pub fn entry_matches(entry: &str, class: &str) -> bool {
    let entry = entry.trim();
    if class.is_empty() || entry.is_empty() {
        return false;
    }
    match entry.strip_suffix('*') {
        Some(prefix) => {
            !prefix.is_empty()
                && class.len() >= prefix.len()
                && class.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
        }
        None => class.eq_ignore_ascii_case(entry),
    }
}

/// The first entry that excludes a window of this class (or initial
/// class), if any.
pub fn excluded_by<'a>(
    excluded: &'a [String],
    class: &str,
    initial_class: &str,
) -> Option<&'a str> {
    excluded
        .iter()
        .find(|e| entry_matches(e, class) || entry_matches(e, initial_class))
        .map(String::as_str)
}

/// The default list, owned.
pub fn default_excluded() -> Vec<String> {
    DEFAULT_EXCLUDED.iter().map(|s| (*s).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_names_its_class_whatever_the_case() {
        assert!(entry_matches("keepassxc", "KeePassXC"));
        assert!(entry_matches(
            "org.keepassxc.KeePassXC",
            "org.keepassxc.keepassxc"
        ));
        assert!(!entry_matches("keepassxc", "keepassxc-cli"));
        assert!(!entry_matches("", "kitty"));
        assert!(!entry_matches("kitty", ""));
    }

    #[test]
    fn a_star_covers_a_prefix_and_only_a_prefix() {
        assert!(entry_matches("pinentry*", "pinentry-qt"));
        assert!(entry_matches("pinentry*", "Pinentry"));
        assert!(!entry_matches("pinentry*", "my-pinentry"));
        // A lone star excludes nothing: a slip must not hide every window.
        assert!(!entry_matches("*", "kitty"));
    }

    #[test]
    fn the_default_list_excludes_what_is_installed_here() {
        let list = default_excluded();
        // Classes of the sensitive applications installed on the operator's
        // workstation (their .desktop files, 2026-10-20).
        for class in [
            "org.keepassxc.KeePassXC",
            "keepassxc",
            "hyprpolkitagent",
            "org.gnupg.pinentry-qt",
            "pinentry-qt",
            "org.kde.ksecretd",
            "org.kde.kwalletd6",
            "veracrypt",
        ] {
            assert!(excluded_by(&list, class, "").is_some(), "{class}");
        }
        for class in ["kitty", "google-chrome-beta", "org.kde.dolphin"] {
            assert!(excluded_by(&list, class, class).is_none(), "{class}");
        }
    }

    #[test]
    fn the_initial_class_counts_too() {
        let list = vec!["veracrypt".to_owned()];
        assert_eq!(excluded_by(&list, "", "VeraCrypt"), Some("veracrypt"));
    }
}
