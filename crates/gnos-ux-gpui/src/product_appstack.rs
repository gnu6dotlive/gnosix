//! The AppStack surface's model: what it shows, in what order, and what it says
//! after a launch attempt.
//!
//! This module holds the decisions that are worth testing and are awkward to
//! test through a rendered view: the ordering rule, the identity rule, and the
//! feedback rule. The view renders what these produce and attaches the one
//! action; it does not decide any of it.
//!
//! Three rules earn their own explanation.
//!
//! **Ordering is derived, never inherited.** The catalog already returns entries
//! in a stable order, but the AppStack's product order is by visible name,
//! because a user looking for a AppStack is looking for a name. Both keys are
//! used: name first, so the list reads alphabetically, and the desktop entry id
//! as a tie-break, so two applications called "Terminal" have a defined order
//! rather than whatever order they happened to arrive in. Nothing here reads a
//! hash map, so the result cannot vary between runs.
//!
//! **Identity is the entry id, never the name.** Two installed applications may
//! share a visible title. If a control were identified by its title, activating
//! "the second Terminal" would be ambiguous, a focus handle would jump between
//! rows when the catalog re-sorted, and two rows would collide. The entry id is
//! stable across sorts, unique within a catalog, and is already the thing the
//! launch capability is fenced to.
//!
//! **The generation is part of the selection, not a lookup detail.** Content is
//! built against one catalog snapshot. If the catalog changes between render
//! and activation, the entry the user picked may no longer be the entry that
//! would run, so the capability refuses rather than resolving against a catalog
//! the user never saw. The AppStack therefore never re-resolves a selection by
//! name or by id against the current catalog to "fix up" a stale reference; that
//! would silently retarget the user's intent.

use gnosix_application_catalog::{CatalogGeneration, DesktopEntryId};
use gpui::SharedString;

pub use crate::product_application_search::{
    ApplicationSearchMode as AppStackSearchMode,
    ApplicationSearchPreferences as AppStackPreferenceProjection,
    ApplicationSearchQuery as AppStackQuery,
};
use crate::{
    product_application_search::search_installed_applications,
    product_runtime::{
        ProductInstalledApplicationContent, ProductLaunchOutcome, ProductLaunchState,
        ProductRuntime,
    },
};

/// One row in the AppStack, resolved from installed application content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppStackRow {
    /// The durable product identity. Never the display name.
    pub entry: DesktopEntryId,
    /// The visible name, as the catalog localized it.
    pub name: String,
    /// The catalog generation this row was presented from.
    pub generation: CatalogGeneration,
    /// Explicit user preference only. Usage ranking is a separate M13 concern.
    pub favourite: bool,
}

impl AppStackRow {
    /// Build rows from installed application content, in product order.
    ///
    /// Takes the content rather than the runtime so the ordering rule is a
    /// pure function of what was presented, and so a test can drive it with
    /// content it constructed rather than a catalog on disk.
    pub fn from_content(content: Vec<ProductInstalledApplicationContent>) -> Vec<AppStackRow> {
        let mut rows: Vec<AppStackRow> = content
            .into_iter()
            .map(|item| AppStackRow {
                entry: item.reference.entry().clone(),
                name: item.name,
                generation: item.reference.generation().clone(),
                favourite: false,
            })
            .collect();
        rows.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                // A stable tie-break, so equal names never reorder between runs.
                .then_with(|| left.entry.as_str().cmp(right.entry.as_str()))
        });
        rows
    }

    /// The id under which this row is rendered.
    ///
    /// Derived from the entry id rather than from the position in the list, so
    /// that a re-sort does not leave a stale element id pointing at a different
    /// application. GPUI element ids are what the accessibility tree and the
    /// automation harness address controls by.
    pub fn element_id(&self) -> String {
        format!("appstack-app-{}", self.entry.as_str())
    }
}

/// The installed applications the AppStack offers, in product order.
pub fn appstack_rows(runtime: &ProductRuntime) -> Vec<AppStackRow> {
    appstack_rows_for(
        runtime,
        AppStackQuery::parse(""),
        &AppStackPreferenceProjection::default(),
    )
}

/// Resolve one query against the bounded search index.
///
/// The search service returns only product identity, generation, visible name
/// and preference state. Command-shaped catalogue metadata never enters this
/// AppStack model.
pub fn appstack_rows_for(
    runtime: &ProductRuntime,
    query: AppStackQuery,
    preferences: &AppStackPreferenceProjection,
) -> Vec<AppStackRow> {
    search_installed_applications(runtime.installed_applications(), &query, preferences)
        .into_iter()
        .map(|hit| AppStackRow {
            entry: hit.entry,
            name: hit.name,
            generation: hit.generation,
            favourite: hit.favourite,
        })
        .collect()
}

/// What the AppStack says after a launch attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppStackFeedback {
    /// The typed outcome code, retained verbatim: `LAUNCHED`, `SPAWN_FAILED`,
    /// or a refusal code such as `CATALOG_GENERATION_CHANGED`.
    pub code: &'static str,
    /// The application the attempt was for.
    pub entry: DesktopEntryId,
    /// The concise line a user reads.
    pub message: SharedString,
    /// Whether the attempt actually started the application.
    pub launched: bool,
}

/// Build the feedback for the runtime's current launch state.
///
/// Returns `None` when nothing has been attempted, so a AppStack that has never
/// been used shows no stale result from a previous session's last attempt.
///
/// The refusal code is carried through rather than flattened into "failed". A
/// user who activated an application and saw nothing happen deserves to be told
/// *why*, and the reason is a product decision the capability already made.
pub fn launch_feedback(state: &ProductLaunchState) -> Option<AppStackFeedback> {
    let record = match state {
        ProductLaunchState::Idle => return None,
        ProductLaunchState::Pending(_) => return None,
        ProductLaunchState::Terminal(record) => record,
    };
    let terminal = record.terminal.as_ref()?;
    let (message, launched) = match terminal {
        ProductLaunchOutcome::Launched { .. } => (format!("{} lancé", record.label), true),
        ProductLaunchOutcome::SpawnFailed { .. } => {
            (format!("{} n’a pas pu démarrer", record.label), false)
        }
        ProductLaunchOutcome::Refused { code, .. } => (
            format!("{} : lancement refusé ({code})", record.label),
            false,
        ),
    };
    Some(AppStackFeedback {
        code: terminal.code(),
        entry: record.entry.clone(),
        message: message.into(),
        launched,
    })
}

/// Whether a successful launch should close the AppStack.
///
/// v1 policy: yes. The user asked for an application, the application is
/// running, and leaving a panel open over a desktop the user wants to use is
/// not what they asked for. The receipt retains the outcome, so nothing is lost
/// by dismissing the surface.
pub const fn closes_on_successful_launch(feedback: &AppStackFeedback) -> bool {
    feedback.launched
}

#[cfg(test)]
mod tests;
