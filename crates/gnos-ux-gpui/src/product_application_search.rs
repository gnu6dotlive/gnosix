//! Pure application-search index for AppStack.
//!
//! This module may inspect presentation metadata from the XDG catalogue, but it
//! returns only catalog identity, generation, visible name and preference
//! state. It cannot launch anything and the AppStack view never receives a
//! command-shaped value.

use std::collections::BTreeSet;

use gnosix_application_catalog::{CatalogGeneration, DesktopEntryId};

use crate::product_runtime::ProductInstalledApplicationContent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationSearchMode {
    Name,
    Id,
    Categories,
    Description,
    Exec,
    StartupClass,
    GenericName,
    Keywords,
    TerminalOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationSearchQuery {
    pub mode: ApplicationSearchMode,
    pub text: String,
}

impl ApplicationSearchQuery {
    pub fn parse(input: &str) -> Self {
        let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
        let (mode, text) = [
            ("@i ", ApplicationSearchMode::Id),
            ("@c ", ApplicationSearchMode::Categories),
            ("@d ", ApplicationSearchMode::Description),
            ("@e ", ApplicationSearchMode::Exec),
            ("@w ", ApplicationSearchMode::StartupClass),
            ("@g ", ApplicationSearchMode::GenericName),
            ("@k ", ApplicationSearchMode::Keywords),
            ("@t ", ApplicationSearchMode::TerminalOnly),
        ]
        .into_iter()
        .find_map(|(prefix, mode)| {
            // A mode prefix alone (a chip just pressed, DECK-02) is that
            // mode with nothing typed yet.
            if normalized == prefix.trim_end() {
                return Some((mode, ""));
            }
            normalized.strip_prefix(prefix).map(|text| (mode, text))
        })
        .unwrap_or((ApplicationSearchMode::Name, normalized.as_str()));
        Self {
            mode,
            text: text.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplicationSearchPreferences {
    pub favourites: BTreeSet<DesktopEntryId>,
    pub hidden: BTreeSet<DesktopEntryId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationSearchHit {
    pub entry: DesktopEntryId,
    pub name: String,
    pub generation: CatalogGeneration,
    pub favourite: bool,
}

pub fn search_installed_applications(
    content: Vec<ProductInstalledApplicationContent>,
    query: &ApplicationSearchQuery,
    preferences: &ApplicationSearchPreferences,
) -> Vec<ApplicationSearchHit> {
    let mut ranked = content
        .into_iter()
        .filter(|item| !preferences.hidden.contains(item.reference.entry()))
        .filter_map(|item| {
            let score = search_score(&item, query)?;
            let favourite = preferences.favourites.contains(item.reference.entry());
            Some((
                ApplicationSearchHit {
                    entry: item.reference.entry().clone(),
                    name: item.name,
                    generation: item.reference.generation().clone(),
                    favourite,
                },
                score,
            ))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|(left, left_score), (right, right_score)| {
        right
            .favourite
            .cmp(&left.favourite)
            .then_with(|| right_score.cmp(left_score))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.entry.as_str().cmp(right.entry.as_str()))
    });
    ranked.into_iter().map(|(hit, _)| hit).collect()
}

fn search_score(
    item: &ProductInstalledApplicationContent,
    query: &ApplicationSearchQuery,
) -> Option<u32> {
    if query.mode == ApplicationSearchMode::TerminalOnly && !item.terminal {
        return None;
    }
    if query.text.is_empty() {
        return Some(0);
    }

    let name = || fuzzy_score(&item.name, &query.text);
    match query.mode {
        ApplicationSearchMode::Name | ApplicationSearchMode::TerminalOnly => name(),
        ApplicationSearchMode::Id => specialized_score(
            [Some(item.reference.entry().as_str())],
            &item.name,
            &query.text,
        ),
        ApplicationSearchMode::Categories => specialized_score(
            item.categories.iter().map(String::as_str).map(Some),
            &item.name,
            &query.text,
        ),
        ApplicationSearchMode::Description => {
            specialized_score([item.comment.as_deref()], &item.name, &query.text)
        }
        ApplicationSearchMode::Exec => {
            specialized_score([item.exec_search_text.as_deref()], &item.name, &query.text)
        }
        ApplicationSearchMode::StartupClass => {
            specialized_score([item.startup_wm_class.as_deref()], &item.name, &query.text)
        }
        ApplicationSearchMode::GenericName => {
            specialized_score([item.generic_name.as_deref()], &item.name, &query.text)
        }
        ApplicationSearchMode::Keywords => specialized_score(
            item.keywords.iter().map(String::as_str).map(Some),
            &item.name,
            &query.text,
        ),
    }
}

fn specialized_score<'a>(
    primary: impl IntoIterator<Item = Option<&'a str>>,
    name: &str,
    query: &str,
) -> Option<u32> {
    let primary = primary
        .into_iter()
        .flatten()
        .filter_map(|value| fuzzy_score(value, query))
        .max();
    let name = fuzzy_score(name, query);
    match (primary, name) {
        (Some(primary), Some(name)) => Some(primary.saturating_mul(10).saturating_add(name)),
        (Some(primary), None) => Some(primary.saturating_mul(10)),
        (None, name) => name,
    }
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<u32> {
    let candidate = candidate.to_lowercase();
    let terms = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Some(0);
    }
    let mut total = 0_u32;
    for term in terms {
        total = total.saturating_add(fuzzy_term_score(&candidate, &term)?);
    }
    Some(total)
}

fn fuzzy_term_score(candidate: &str, term: &str) -> Option<u32> {
    if candidate == term {
        return Some(10_000);
    }
    if candidate.starts_with(term) {
        return Some(9_000_u32.saturating_sub((candidate.len() - term.len()) as u32));
    }
    if let Some(position) = candidate.find(term) {
        return Some(
            8_000_u32
                .saturating_sub(position as u32 * 10)
                .saturating_sub((candidate.len() - term.len()) as u32),
        );
    }

    let haystack = candidate.chars().collect::<Vec<_>>();
    let needle = term.chars().collect::<Vec<_>>();
    let mut cursor = 0_usize;
    let mut first = None;
    let mut last = 0_usize;
    for wanted in &needle {
        let relative = haystack[cursor..]
            .iter()
            .position(|candidate| candidate == wanted)?;
        let found = cursor + relative;
        first.get_or_insert(found);
        last = found;
        cursor = found + 1;
    }
    let span = last.saturating_sub(first.unwrap_or(0)).saturating_add(1);
    let gaps = span.saturating_sub(needle.len());
    Some(
        5_000_u32
            .saturating_sub(gaps as u32 * 10)
            .saturating_sub(first.unwrap_or(0) as u32),
    )
}

#[cfg(test)]
mod prefix_tests {
    use super::*;

    #[test]
    fn a_mode_prefix_alone_selects_the_mode() {
        let q = ApplicationSearchQuery::parse("@c ");
        assert_eq!(
            (q.mode, q.text.as_str()),
            (ApplicationSearchMode::Categories, "")
        );
        let q = ApplicationSearchQuery::parse("@c graphics");
        assert_eq!(
            (q.mode, q.text.as_str()),
            (ApplicationSearchMode::Categories, "graphics")
        );
        let q = ApplicationSearchQuery::parse("@cx");
        assert_eq!(q.mode, ApplicationSearchMode::Name);
    }
}
