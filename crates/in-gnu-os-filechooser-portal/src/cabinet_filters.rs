//! Conservative XDG FileChooser filter model, independent of the native UI.
//! Matching is an interface hint, not filesystem authorization.
use serde::{Deserialize, Serialize};
use std::path::Path;
use zbus::zvariant::OwnedValue;

pub const MAX_FILTERS: usize = 16;
pub const MAX_RULES: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CabinetFilter {
    pub name: String,
    pub rules: Vec<(u32, String)>,
}

impl CabinetFilter {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
            || self.rules.is_empty()
            || self.rules.len() > MAX_RULES
        {
            return Err("filter name or number of rules invalid");
        }
        for (kind, value) in &self.rules {
            if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err("filter rule invalid");
            }
            match kind {
                0 => {
                    // Restrict this iteration to filename globs.
                    if value.contains('/') || glob::Pattern::new(value).is_err() {
                        return Err("unsupported filename glob");
                    }
                }
                1 => {
                    let Some((category, subtype)) = value.split_once('/') else {
                        return Err("invalid MIME rule");
                    };
                    if category.is_empty()
                        || subtype.is_empty()
                        || !category
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b == b'-' || b == b'.' || b == b'+')
                        || !(subtype == "*"
                            || subtype.bytes().all(|b| {
                                b.is_ascii_lowercase()
                                    || b.is_ascii_digit()
                                    || matches!(b, b'.' | b'-' | b'+' | b'_')
                            }))
                    {
                        return Err("unsupported MIME format");
                    }
                }
                _ => return Err("unsupported filter type"),
            }
        }
        Ok(())
    }

    pub fn matches_path(&self, path: &Path) -> bool {
        self.rules.iter().any(|(kind, pattern)| match *kind {
            0 => path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| {
                    glob::Pattern::new(pattern)
                        .ok()
                        .map(|glob| glob.matches(name))
                })
                .unwrap_or(false),
            1 => mime_guess::from_path(path).iter_raw().any(|guess| {
                if pattern.ends_with("/*") {
                    guess
                        .split_once('/')
                        .is_some_and(|(group, _)| group == &pattern[..pattern.len() - 2])
                } else {
                    guess == pattern
                }
            }),
            _ => false,
        })
    }
}

pub fn parse_filters(
    filters: Option<&OwnedValue>,
    current_filter: Option<&OwnedValue>,
) -> Result<(Vec<CabinetFilter>, Option<usize>), &'static str> {
    let mut available = match filters {
        Some(value) => {
            let typed: Vec<(String, Vec<(u32, String)>)> =
                Vec::try_from(value.try_clone().map_err(|_| "cannot clone filters")?)
                    .map_err(|_| "invalid XDG filters signature")?;
            typed
                .into_iter()
                .map(|(name, rules)| CabinetFilter { name, rules })
                .collect::<Vec<_>>()
        }
        None => Vec::new(),
    };
    if available.len() > MAX_FILTERS {
        return Err("too many filters");
    }
    for filter in &available {
        filter.validate()?;
    }

    let selected = match current_filter {
        Some(raw) => {
            let typed: (String, Vec<(u32, String)>) = <(String, Vec<(u32, String)>)>::try_from(
                raw.try_clone()
                    .map_err(|_| "cannot clone selected filter")?,
            )
            .map_err(|_| "invalid selected filter signature")?;
            let filter = CabinetFilter {
                name: typed.0,
                rules: typed.1,
            };
            filter.validate()?;
            if available.is_empty() {
                available.push(filter);
                Some(0)
            } else {
                Some(
                    available
                        .iter()
                        .position(|item| item == &filter)
                        .ok_or("selected filter not present")?,
                )
            }
        }
        None if available.is_empty() => None,
        None => Some(0),
    };
    Ok((available, selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glob_matching_is_case_sensitive_and_local_to_name() {
        let rule = CabinetFilter {
            name: "Text".into(),
            rules: vec![(0, "*.[tT][xX][tT]".into())],
        };
        assert!(rule.validate().is_ok());
        assert!(rule.matches_path(Path::new("/tmp/Été.TXT")));
        assert!(!rule.matches_path(Path::new("/tmp/notes.md")));
    }
    #[test]
    fn mime_wildcard_is_extension_heuristic_not_content_authority() {
        let rule = CabinetFilter {
            name: "Images".into(),
            rules: vec![(1, "image/*".into())],
        };
        assert!(rule.matches_path(Path::new("/tmp/photo.png")));
        assert!(!rule.matches_path(Path::new("/tmp/script.rs")));
    }
    #[test]
    fn xdg_structured_filters_roundtrip_and_selected_filter() {
        use zbus::zvariant::{OwnedValue, Value};
        let items = vec![
            ("Text".to_string(), vec![(0u32, "*.txt".to_string())]),
            ("Images".to_string(), vec![(1u32, "image/*".to_string())]),
        ];
        let typed = OwnedValue::try_from(Value::new(items.clone())).unwrap();
        let active = OwnedValue::try_from(Value::new(items[1].clone())).unwrap();
        let (filters, selected) = parse_filters(Some(&typed), Some(&active)).unwrap();
        assert_eq!(filters.len(), 2);
        assert_eq!(selected, Some(1));
        assert!(filters[1].matches_path(Path::new("/tmp/cat.png")));
        assert!(!filters[1].matches_path(Path::new("/tmp/notes.txt")));
        let (only, selection) = parse_filters(None, Some(&active)).unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(selection, Some(0));
    }

    #[test]
    fn invalid_and_unhandled_rules_fail_closed() {
        for kind in [2, 50] {
            assert!(CabinetFilter {
                name: "Bad".into(),
                rules: vec![(kind, "*.png".into())]
            }
            .validate()
            .is_err());
        }
        assert!(CabinetFilter {
            name: "Bad".into(),
            rules: vec![(0, "dir/*.png".into())]
        }
        .validate()
        .is_err());
        assert!(CabinetFilter {
            name: "Bad".into(),
            rules: vec![(0, "[".into())]
        }
        .validate()
        .is_err());
        assert!(CabinetFilter {
            name: "Bad".into(),
            rules: vec![(1, "text/*+xml".into())]
        }
        .validate()
        .is_err());
    }
}
