//! XDG FileChooser choices (a(ssa(ss)s)). No choices are silently dropped.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::OwnedValue;

pub const MAX_CHOICES: usize = 16;
pub const MAX_OPTIONS: usize = 32;

type DbusChoiceTuple = (String, String, Vec<(String, String)>, String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CabinetChoice {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub selected: String,
}

impl CabinetChoice {
    pub fn validate(&self) -> Result<(), &'static str> {
        fn valid(text: &str) -> bool {
            !text.is_empty() && text.len() <= 128 && !text.chars().any(char::is_control)
        }
        if !valid(&self.id) || !valid(&self.label) || self.options.len() > MAX_OPTIONS {
            return Err("bad choice id, label or option count");
        }
        let mut ids = HashSet::new();
        for (id, label) in &self.options {
            if !valid(id) || !valid(label) || !ids.insert(id) {
                return Err("invalid or duplicated choice option");
            }
        }
        if !self.accepts(&self.selected) {
            return Err("initial choice selection not offered");
        }
        Ok(())
    }
    pub fn accepts(&self, value: &str) -> bool {
        if self.options.is_empty() {
            matches!(value, "true" | "false")
        } else {
            self.options.iter().any(|(id, _)| id == value)
        }
    }
    pub fn cycle(&mut self) {
        if self.options.is_empty() {
            self.selected = if self.selected == "true" {
                "false"
            } else {
                "true"
            }
            .into();
        } else {
            let old = self
                .options
                .iter()
                .position(|(id, _)| id == &self.selected)
                .unwrap_or(0);
            self.selected = self.options[(old + 1) % self.options.len()].0.clone();
        }
    }
    pub fn selection_label(&self) -> &str {
        if self.options.is_empty() {
            if self.selected == "true" {
                "On"
            } else {
                "Off"
            }
        } else {
            self.options
                .iter()
                .find(|(id, _)| id == &self.selected)
                .map(|(_, label)| label.as_str())
                .unwrap_or("Invalid")
        }
    }
}

pub fn parse_choices(value: Option<&OwnedValue>) -> Result<Vec<CabinetChoice>, &'static str> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let typed: Vec<DbusChoiceTuple> =
        Vec::try_from(value.try_clone().map_err(|_| "choices not cloneable")?)
            .map_err(|_| "wrong choices ABI")?;
    if typed.len() > MAX_CHOICES {
        return Err("too many choices");
    }
    let mut ids = HashSet::new();
    let mut choices = Vec::with_capacity(typed.len());
    for (id, label, options, selected) in typed {
        if !ids.insert(id.clone()) {
            return Err("duplicate choice id");
        }
        let selected = if selected.is_empty() {
            options
                .first()
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| "false".into())
        } else {
            selected
        };
        let choice = CabinetChoice {
            id,
            label,
            options,
            selected,
        };
        choice.validate()?;
        choices.push(choice);
    }
    Ok(choices)
}

/// Exact round trip: same IDs, same order, only offered option values.
/// No caller-supplied labels returned as option identifiers.
pub fn validate_result(offered: &[CabinetChoice], returned: &[(String, String)]) -> bool {
    offered.len() == returned.len()
        && offered
            .iter()
            .zip(returned.iter())
            .all(|(choice, (id, value))| choice.id == *id && choice.accepts(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{OwnedValue, Value};
    fn input() -> OwnedValue {
        let entries = vec![
            (
                "encoding".to_string(),
                "Encoding".to_string(),
                vec![
                    ("utf8".to_string(), "Unicode".to_string()),
                    ("latin15".to_string(), "Western".to_string()),
                ],
                "latin15".to_string(),
            ),
            (
                "reencode".to_string(),
                "Reencode".to_string(),
                Vec::new(),
                "true".to_string(),
            ),
        ];
        OwnedValue::try_from(Value::new(entries)).unwrap()
    }
    #[test]
    fn parse_xdg_tuple_and_cycle_radio_boolean() {
        let mut choices = parse_choices(Some(&input())).unwrap();
        assert_eq!(choices[0].selected, "latin15");
        assert_eq!(choices[1].selected, "true");
        choices[0].cycle();
        choices[1].cycle();
        assert_eq!(choices[0].selected, "utf8");
        assert_eq!(choices[1].selected, "false");
        assert!(validate_result(
            &choices,
            &[
                ("encoding".into(), "utf8".into()),
                ("reencode".into(), "false".into())
            ]
        ));
        assert!(!validate_result(
            &choices,
            &[
                ("reencode".into(), "false".into()),
                ("encoding".into(), "utf8".into())
            ]
        ));
        assert!(!validate_result(
            &choices,
            &[
                ("encoding".into(), "arbitrary".into()),
                ("reencode".into(), "false".into())
            ]
        ));
    }
    #[test]
    fn invalid_duplicates_and_missing_selection_fail_closed() {
        let entries = vec![
            (
                "same".to_string(),
                "One".to_string(),
                Vec::<(String, String)>::new(),
                "true".to_string(),
            ),
            (
                "same".to_string(),
                "Two".to_string(),
                Vec::new(),
                "false".to_string(),
            ),
        ];
        let value = OwnedValue::try_from(Value::new(entries)).unwrap();
        assert!(parse_choices(Some(&value)).is_err());
        let invalid = CabinetChoice {
            id: "mode".into(),
            label: "Mode".into(),
            options: vec![("a".into(), "A".into())],
            selected: "b".into(),
        };
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn default_selected_is_first_option_or_false_boolean() {
        let items = vec![
            (
                "encoding".to_string(),
                "Encoding".to_string(),
                vec![("u".to_string(), "UTF8".to_string())],
                "".to_string(),
            ),
            (
                "toggle".to_string(),
                "Toggle".to_string(),
                Vec::new(),
                "".to_string(),
            ),
        ];
        let value = OwnedValue::try_from(Value::new(items)).unwrap();
        let parsed = parse_choices(Some(&value)).unwrap();
        assert_eq!(parsed[0].selected, "u");
        assert_eq!(parsed[1].selected, "false");
    }
}
