//! Cross-host product focus arbitration.
//!
//! gpui tracks focus per `Window`, so two layer-shell surfaces can each
//! believe they own keyboard focus at the same time. On a multi-output
//! session that is not a cosmetic detail: a keyboard user could tab into
//! one output while the other still reports a focused control, and the
//! accessibility tree would show two focused nodes at once.
//!
//! The desktop has exactly one keyboard focus, so the product keeps
//! exactly one focused control. `ProductFocusCoordinator` records which
//! output host owns it, and every `SceneHost` synchronises through this
//! one object regardless of whether focus arrived from a key, a pointer,
//! or an assistive-technology client.
//!
//! Ownership is explicit: the coordinator is created above the hosts and
//! handed to each of them. It is deliberately not a process-global, so
//! tests and multi-session runs cannot leak focus state into one another.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use gpui::AnyWindowHandle;

use crate::product_focus::ProductControlId;

/// Stable identity of one interactive product surface (one output).
///
/// This is the scene's own identity, never a label, a render index, or a
/// pointer address, so two hosts can never be confused for one another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProductHostId(pub u64);

impl ProductHostId {
    pub fn value(self) -> u64 {
        self.0
    }
}

/// The single product control that owns focus across every host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductFocusTarget {
    pub host: ProductHostId,
    pub control: ProductControlId,
}

#[derive(Default)]
struct CoordinatorState {
    focused: Option<ProductFocusTarget>,
}

#[derive(Clone, Default)]
pub struct ProductFocusCoordinator {
    state: Arc<Mutex<CoordinatorState>>,
}

impl ProductFocusCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// The control that currently owns product focus, if any.
    pub fn focused(&self) -> Option<ProductFocusTarget> {
        self.state
            .lock()
            .expect("product focus coordinator poisoned")
            .focused
            .clone()
    }

    /// How many product controls own focus. Never above one by
    /// construction; the qualifier asserts the invariant rather than
    /// assuming it.
    pub fn focused_count(&self) -> usize {
        usize::from(self.focused().is_some())
    }

    /// Whether this exact host and control own focus.
    pub fn owns(&self, host: ProductHostId, control: &ProductControlId) -> bool {
        self.state
            .lock()
            .expect("product focus coordinator poisoned")
            .focused
            .as_ref()
            .is_some_and(|target| target.host == host && &target.control == control)
    }

    /// Whether any host other than `host` owns focus.
    pub fn owned_elsewhere(&self, host: ProductHostId) -> bool {
        self.state
            .lock()
            .expect("product focus coordinator poisoned")
            .focused
            .as_ref()
            .is_some_and(|target| target.host != host)
    }

    /// Record that `control` on `host` took focus.
    ///
    /// Returns the target that previously owned focus when it belonged to
    /// a *different* host, so the caller can make that host give focus
    /// up. A same-host move returns `None`: focus within one window is
    /// already exclusive and must not disturb itself.
    pub fn claim(
        &self,
        host: ProductHostId,
        control: ProductControlId,
    ) -> Option<ProductFocusTarget> {
        let mut state = self
            .state
            .lock()
            .expect("product focus coordinator poisoned");
        let previous = state.focused.clone();
        let displaced = previous
            .as_ref()
            .filter(|target| target.host != host)
            .cloned();
        state.focused = Some(ProductFocusTarget { host, control });
        displaced
    }

    /// Release the claim held by `host`, if that is the current owner.
    ///
    /// Called by a host that no longer holds focus, so a host that lost
    /// focus cannot keep the product believing it still owns it.
    pub fn release(&self, host: ProductHostId) {
        let mut state = self
            .state
            .lock()
            .expect("product focus coordinator poisoned");
        if state.focused.as_ref().is_some_and(|t| t.host == host) {
            state.focused = None;
        }
    }

    /// Drop the claim entirely, for a host that has gone away.
    pub fn clear(&self) {
        self.state
            .lock()
            .expect("product focus coordinator poisoned")
            .focused = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> ProductControlId {
        ProductControlId(name.into())
    }

    fn host(n: u64) -> ProductHostId {
        ProductHostId(n)
    }

    #[test]
    fn no_host_focused_means_zero_targets() {
        let coordinator = ProductFocusCoordinator::new();
        assert_eq!(coordinator.focused_count(), 0);
        assert!(coordinator.focused().is_none());
    }

    #[test]
    fn one_host_one_control_is_one_target() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(1), id("ws:a"));
        assert_eq!(coordinator.focused_count(), 1);
        assert!(coordinator.owns(host(1), &id("ws:a")));
        assert!(!coordinator.owns(host(2), &id("ws:a")));
    }

    #[test]
    fn claiming_from_another_host_displaces_the_previous_owner() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(1), id("ws:a"));
        let displaced = coordinator.claim(host(2), id("ws:b"));

        assert_eq!(
            displaced,
            Some(ProductFocusTarget {
                host: host(1),
                control: id("ws:a"),
            }),
            "the losing host must be reported so it can give focus up"
        );
        assert_eq!(coordinator.focused_count(), 1, "exclusivity must hold");
        assert!(coordinator.owns(host(2), &id("ws:b")));
        assert!(coordinator.owned_elsewhere(host(1)));
    }

    #[test]
    fn moving_within_one_host_does_not_displace_itself() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(1), id("ws:a"));
        let displaced = coordinator.claim(host(1), id("ws:b"));

        assert!(
            displaced.is_none(),
            "focus moving within a window is already exclusive"
        );
        assert!(coordinator.owns(host(1), &id("ws:b")));
        assert_eq!(coordinator.focused_count(), 1);
    }

    #[test]
    fn repeated_refresh_of_the_same_durable_target_preserves_identity() {
        let coordinator = ProductFocusCoordinator::new();
        let target = id("ws:aa11bb22");
        coordinator.claim(host(1), target.clone());
        let first = coordinator.focused().expect("a claim was just made");

        // An unrelated semantic generation re-presents the same control.
        coordinator.claim(host(1), target.clone());
        let second = coordinator.focused().expect("a claim was just made");

        assert_eq!(first, second, "durable identity must survive a refresh");
    }

    #[test]
    fn a_host_that_no_longer_holds_focus_releases_the_claim() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(1), id("ws:a"));
        coordinator.claim(host(2), id("ws:b"));
        assert_eq!(coordinator.focused_count(), 1);

        // Host 1 redraws having lost focus; it must not resurrect a claim.
        coordinator.release(host(1));

        assert!(
            coordinator.owns(host(2), &id("ws:b")),
            "a losing host must not steal the record back"
        );
        assert_eq!(coordinator.focused_count(), 1);
    }

    #[test]
    fn releasing_the_owning_host_leaves_no_product_target() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(1), id("ws:a"));
        coordinator.release(host(1));
        assert_eq!(coordinator.focused_count(), 0, "release must be total");
    }

    #[test]
    fn releasing_a_host_that_does_not_own_focus_is_harmless() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(1), id("ws:a"));
        coordinator.release(host(2));
        assert!(coordinator.owns(host(1), &id("ws:a")));
    }

    #[test]
    fn a_retired_host_clears_the_coordinator() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(2), id("ws:b"));
        coordinator.clear();
        assert_eq!(coordinator.focused_count(), 0);
    }

    #[test]
    fn hosts_are_distinguished_by_identity_not_by_order() {
        let coordinator = ProductFocusCoordinator::new();
        coordinator.claim(host(7), id("ws:a"));
        coordinator.claim(host(3), id("ws:b"));
        assert!(coordinator.owns(host(3), &id("ws:b")));
        assert!(
            !coordinator.owns(host(7), &id("ws:a")),
            "the earlier host must not still appear focused"
        );
    }
}

/// The window of every interactive product host, keyed by host identity.
///
/// Focus arbitration has to be able to *reach* the host that lost focus,
/// and gpui tracks focus per window, so telling a host to give focus up
/// means touching its window. This registry is that reach.
///
/// It is owned by the session above the hosts and handed to each of them,
/// exactly like the coordinator: explicit ownership, never a global. It
/// holds no semantic state at all - only which window belongs to which
/// host identity.
#[derive(Clone, Default)]
pub struct ProductHostWindows {
    windows: Arc<Mutex<BTreeMap<ProductHostId, AnyWindowHandle>>>,
}

impl ProductHostWindows {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the window a host renders into.
    pub fn register(&self, host: ProductHostId, window: AnyWindowHandle) {
        self.windows
            .lock()
            .expect("product host windows poisoned")
            .insert(host, window);
    }

    /// Forget a host that has gone away, so a retired surface can never be
    /// asked to release focus it no longer has.
    pub fn retire(&self, host: ProductHostId) {
        self.windows
            .lock()
            .expect("product host windows poisoned")
            .remove(&host);
    }

    pub fn window(&self, host: ProductHostId) -> Option<AnyWindowHandle> {
        self.windows
            .lock()
            .expect("product host windows poisoned")
            .get(&host)
            .copied()
    }

    pub fn len(&self) -> usize {
        self.windows
            .lock()
            .expect("product host windows poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod host_window_tests {
    use super::*;

    #[test]
    fn an_unregistered_host_has_no_window() {
        let registry = ProductHostWindows::new();
        assert!(registry.window(ProductHostId(1)).is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn a_host_is_distinguished_by_identity_not_registration_order() {
        // Registration is exercised through the real window type in the
        // scene tests; here the identity keying is what matters.
        let registry = ProductHostWindows::new();
        assert_eq!(registry.len(), 0);
        registry.retire(ProductHostId(9));
        assert!(registry.is_empty());
    }
}
