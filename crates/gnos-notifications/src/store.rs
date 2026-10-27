//! Pure notification store: history, toast lifetime, close and action rules.
//!
//! Time is an explicit monotonic millisecond value supplied by the caller, so
//! every rule is deterministic and testable without a clock. The store never
//! performs I/O; it returns the bus signals a transition requires.
//!
//! Lifetime model (reference behaviour, see UXE_02 facts N01–N09):
//! - a notification lives in the history until it is closed;
//! - its toast is a separate, shorter life: when the toast times out the
//!   notification stays in the history and nothing is sent on the bus;
//! - the toast timer pauses while the pointer holds it and restarts from the
//!   full interval when released.

use crate::model::{
    Action, CloseReason, ExpireTimeout, ImageSource, NotificationId, NotifyRequest, Urgency,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreConfig {
    /// Toast life when the client asks for the server default (N03).
    pub default_toast_ms: u64,
    /// Toast life for server-default notifications while a fullscreen window
    /// is focused (N03).
    pub fullscreen_toast_ms: u64,
    /// Oldest notifications beyond this are closed as expired.
    pub history_limit: usize,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            default_toast_ms: 5000,
            fullscreen_toast_ms: 2000,
            history_limit: 200,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toast {
    Hidden,
    /// `deadline_ms` is `None` while the toast is held by the pointer.
    Shown {
        interval_ms: u64,
        deadline_ms: Option<u64>,
    },
}

impl Toast {
    pub fn is_shown(self) -> bool {
        matches!(self, Self::Shown { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub id: NotificationId,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub actions: Vec<Action>,
    pub image: Option<ImageSource>,
    pub progress: Option<u8>,
    pub category: Option<String>,
    pub desktop_entry: Option<String>,
    pub resident: bool,
    pub transient: bool,
    pub expire_timeout: ExpireTimeout,
    /// First arrival; a replacement keeps it (N05).
    pub received_ms: u64,
    pub toast: Toast,
}

/// A `NotificationClosed(id, reason)` the bus must emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closed {
    pub id: NotificationId,
    pub reason: CloseReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyReceipt {
    pub id: NotificationId,
    pub replaced: bool,
    pub evicted: Vec<Closed>,
}

/// An `ActionInvoked(id, key)` the bus must emit, then the close it implies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvokeReceipt {
    pub id: NotificationId,
    pub key: String,
    pub closed: Option<Closed>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpiryOutcome {
    pub changed: bool,
    pub closed: Vec<Closed>,
}

/// Immutable view handed to surfaces. Newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationSnapshot {
    pub revision: u64,
    pub notifications: Vec<Notification>,
    pub do_not_disturb: bool,
}

impl NotificationSnapshot {
    pub fn toasts(&self) -> impl Iterator<Item = &Notification> {
        self.notifications.iter().filter(|n| n.toast.is_shown())
    }

    pub fn get(&self, id: NotificationId) -> Option<&Notification> {
        self.notifications.iter().find(|n| n.id == id)
    }
}

#[derive(Debug)]
pub struct NotificationStore {
    config: StoreConfig,
    /// Newest first (N01).
    notifications: Vec<Notification>,
    next_id: u32,
    revision: u64,
    do_not_disturb: bool,
    centre_open: bool,
    fullscreen: bool,
    /// SETTINGS-03B: the operator's preferences.
    prefs: crate::prefs::NotificationPrefs,
}

impl NotificationStore {
    pub fn new(config: StoreConfig) -> Self {
        Self {
            config,
            notifications: Vec::new(),
            next_id: 1,
            revision: 0,
            do_not_disturb: false,
            centre_open: false,
            fullscreen: false,
            prefs: crate::prefs::NotificationPrefs::default(),
        }
    }

    /// SETTINGS-03B: the operator's preferences apply to what arrives next
    /// (the toast life, the rules); what is already shown stays.
    pub fn set_prefs(&mut self, prefs: crate::prefs::NotificationPrefs) -> bool {
        if self.prefs == prefs || !prefs.is_valid() {
            return false;
        }
        self.config.default_toast_ms = u64::from(prefs.toast_secs) * 1000;
        self.prefs = prefs;
        self.bump();
        true
    }

    fn allocate_id(&mut self) -> NotificationId {
        loop {
            let raw = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if let Some(id) = NotificationId::new(raw)
                && self.position(id).is_none()
            {
                return id;
            }
        }
    }

    fn position(&self, id: NotificationId) -> Option<usize> {
        self.notifications.iter().position(|n| n.id == id)
    }

    fn bump(&mut self) {
        self.revision += 1;
    }

    /// N02: a new toast is suppressed by do-not-disturb or an open centre;
    /// SETTINGS-03B: and by the application's rule, while a critical one may
    /// pass do-not-disturb.
    fn toast_allowed(&self, app_name: &str, urgency: Urgency) -> bool {
        !self.centre_open
            && self
                .prefs
                .toast_through(app_name, urgency, self.do_not_disturb)
    }

    /// N03: an explicit client timeout wins; otherwise the server default,
    /// shortened while a fullscreen window is focused. "Never" (0) is treated
    /// as the server default for the toast: the notification itself is
    /// persistent anyway.
    fn toast_interval(&self, timeout: ExpireTimeout) -> u64 {
        match timeout {
            ExpireTimeout::Millis(ms) => u64::from(ms),
            ExpireTimeout::ServerDefault | ExpireTimeout::Never if self.fullscreen => {
                self.config.fullscreen_toast_ms
            }
            ExpireTimeout::ServerDefault | ExpireTimeout::Never => self.config.default_toast_ms,
        }
    }

    fn fresh_toast(
        &self,
        timeout: ExpireTimeout,
        now_ms: u64,
        app_name: &str,
        urgency: Urgency,
    ) -> Toast {
        if !self.toast_allowed(app_name, urgency) {
            return Toast::Hidden;
        }
        let interval_ms = self.toast_interval(timeout);
        Toast::Shown {
            interval_ms,
            deadline_ms: Some(now_ms + interval_ms),
        }
    }

    pub fn notify(&mut self, request: NotifyRequest, now_ms: u64) -> NotifyReceipt {
        let hints = request.hints;
        let toast = self.fresh_toast(
            request.expire_timeout,
            now_ms,
            &request.app_name,
            hints.urgency.unwrap_or_default(),
        );
        let off = self.prefs.rule(&request.app_name) == crate::prefs::AppRule::Off;
        let replace_at = NotificationId::new(request.replaces_id).and_then(|id| self.position(id));
        let (id, replaced, received_ms) = match replace_at {
            // N05: replacement keeps identity, history position and arrival
            // time; its toast is re-armed (Gnosix decision G02).
            Some(index) => {
                let existing = &self.notifications[index];
                (existing.id, true, existing.received_ms)
            }
            // A stale or unknown replaces_id is a new notification (spec).
            None => (self.allocate_id(), false, now_ms),
        };
        let notification = Notification {
            id,
            app_name: request.app_name,
            app_icon: request.app_icon,
            summary: request.summary,
            body: request.body,
            urgency: hints.urgency.unwrap_or_default(),
            actions: request.actions,
            image: hints.image,
            progress: hints.progress,
            category: hints.category,
            desktop_entry: hints.desktop_entry,
            resident: hints.resident,
            transient: hints.transient,
            expire_timeout: request.expire_timeout,
            received_ms,
            toast,
        };
        match replace_at {
            Some(index) => self.notifications[index] = notification,
            None => self.notifications.insert(0, notification),
        }
        let mut evicted = Vec::new();
        // SETTINGS-03B: an application the operator turned off is not kept.
        if off && let Some(index) = self.position(id) {
            self.notifications.remove(index);
            evicted.push(Closed {
                id,
                reason: CloseReason::Dismissed,
            });
        }
        while self.notifications.len() > self.config.history_limit.max(1) {
            let oldest = self.notifications.pop().expect("non-empty");
            evicted.push(Closed {
                id: oldest.id,
                reason: CloseReason::Expired,
            });
        }
        self.bump();
        NotifyReceipt {
            id,
            replaced,
            evicted,
        }
    }

    /// Removes the notification. `None` when it no longer exists, in which
    /// case nothing is emitted.
    pub fn close(&mut self, id: NotificationId, reason: CloseReason) -> Option<Closed> {
        let index = self.position(id)?;
        self.notifications.remove(index);
        self.bump();
        Some(Closed { id, reason })
    }

    /// N08: every notification closed by the user.
    pub fn clear_all(&mut self) -> Vec<Closed> {
        let closed = self
            .notifications
            .drain(..)
            .map(|n| Closed {
                id: n.id,
                reason: CloseReason::Dismissed,
            })
            .collect::<Vec<_>>();
        if !closed.is_empty() {
            self.bump();
        }
        closed
    }

    /// N06: invoking an action closes the notification unless it is resident.
    pub fn invoke(&mut self, id: NotificationId, key: &str) -> Option<InvokeReceipt> {
        let index = self.position(id)?;
        if !self.notifications[index]
            .actions
            .iter()
            .any(|a| a.key == key)
        {
            return None;
        }
        let closed = if self.notifications[index].resident {
            None
        } else {
            self.close(id, CloseReason::Dismissed)
        };
        Some(InvokeReceipt {
            id,
            key: key.to_owned(),
            closed,
        })
    }

    /// N07: swiping a toast away only hides the toast.
    pub fn hide_toast(&mut self, id: NotificationId) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };
        if !self.notifications[index].toast.is_shown() {
            return false;
        }
        self.notifications[index].toast = Toast::Hidden;
        self.bump();
        true
    }

    /// N04: the pointer holds the toast; its timer stops.
    pub fn hold_toast(&mut self, id: NotificationId) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };
        match self.notifications[index].toast {
            Toast::Shown {
                interval_ms,
                deadline_ms: Some(_),
            } => {
                self.notifications[index].toast = Toast::Shown {
                    interval_ms,
                    deadline_ms: None,
                };
                self.bump();
                true
            }
            _ => false,
        }
    }

    /// N04: released; the timer restarts from the full interval.
    pub fn release_toast(&mut self, id: NotificationId, now_ms: u64) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };
        match self.notifications[index].toast {
            Toast::Shown {
                interval_ms,
                deadline_ms: None,
            } => {
                self.notifications[index].toast = Toast::Shown {
                    interval_ms,
                    deadline_ms: Some(now_ms + interval_ms),
                };
                self.bump();
                true
            }
            _ => false,
        }
    }

    /// Hides due toasts. A transient notification leaves with its toast and
    /// is closed as expired (Gnosix decision G03).
    pub fn expire_due(&mut self, now_ms: u64) -> ExpiryOutcome {
        let mut outcome = ExpiryOutcome::default();
        let mut index = 0;
        while index < self.notifications.len() {
            let due = matches!(
                self.notifications[index].toast,
                Toast::Shown { deadline_ms: Some(deadline), .. } if deadline <= now_ms
            );
            if !due {
                index += 1;
                continue;
            }
            outcome.changed = true;
            if self.notifications[index].transient {
                let gone = self.notifications.remove(index);
                outcome.closed.push(Closed {
                    id: gone.id,
                    reason: CloseReason::Expired,
                });
            } else {
                self.notifications[index].toast = Toast::Hidden;
                index += 1;
            }
        }
        if outcome.changed {
            self.bump();
        }
        outcome
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.notifications
            .iter()
            .filter_map(|n| match n.toast {
                Toast::Shown { deadline_ms, .. } => deadline_ms,
                Toast::Hidden => None,
            })
            .min()
    }

    pub fn set_do_not_disturb(&mut self, enabled: bool) -> bool {
        if self.do_not_disturb == enabled {
            return false;
        }
        self.do_not_disturb = enabled;
        self.bump();
        true
    }

    /// N02/N14: while the centre is open no new toast appears, and opening it
    /// hides every shown toast (the notifications stay in the history).
    pub fn set_centre_open(&mut self, open: bool) -> bool {
        self.centre_open = open;
        if !open {
            return false;
        }
        let mut changed = false;
        for notification in &mut self.notifications {
            if notification.toast.is_shown() {
                notification.toast = Toast::Hidden;
                changed = true;
            }
        }
        if changed {
            self.bump();
        }
        changed
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    pub fn snapshot(&self) -> NotificationSnapshot {
        NotificationSnapshot {
            revision: self.revision,
            notifications: self.notifications.clone(),
            do_not_disturb: self.do_not_disturb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Hints, pair_actions};

    fn request(summary: &str) -> NotifyRequest {
        NotifyRequest {
            app_name: "app".into(),
            replaces_id: 0,
            app_icon: String::new(),
            summary: summary.into(),
            body: String::new(),
            actions: Vec::new(),
            hints: Hints::default(),
            expire_timeout: ExpireTimeout::ServerDefault,
        }
    }

    fn id(raw: u32) -> NotificationId {
        NotificationId::new(raw).unwrap()
    }

    fn store() -> NotificationStore {
        NotificationStore::new(StoreConfig::default())
    }

    #[test]
    fn preferences_silence_or_drop_an_application_and_let_critical_through() {
        use crate::prefs::{AppRule, NotificationPrefs};
        let mut s = store();
        let mut prefs = NotificationPrefs {
            toast_secs: 8,
            ..NotificationPrefs::default()
        };
        prefs.apps.insert("quiet".into(), AppRule::Silent);
        prefs.apps.insert("gone".into(), AppRule::Off);
        assert!(s.set_prefs(prefs.clone()));
        assert!(!s.set_prefs(prefs));
        let from = |app: &str| NotifyRequest {
            app_name: app.into(),
            ..request(app)
        };
        // The toast life is the operator's.
        let shown = s.notify(from("app"), 0).id;
        assert!(matches!(
            s.snapshot().get(shown).unwrap().toast,
            Toast::Shown {
                interval_ms: 8000,
                ..
            }
        ));
        // Silent: kept, no toast.
        let quiet = s.notify(from("quiet"), 0).id;
        assert_eq!(s.snapshot().get(quiet).unwrap().toast, Toast::Hidden);
        // Off: closed at once, never kept.
        let receipt = s.notify(from("gone"), 0);
        assert!(s.snapshot().get(receipt.id).is_none());
        assert_eq!(
            receipt.evicted,
            [Closed {
                id: receipt.id,
                reason: CloseReason::Dismissed
            }]
        );
        // Do not disturb: only a critical one shows its toast.
        s.set_do_not_disturb(true);
        let normal = s.notify(from("app"), 0).id;
        assert_eq!(s.snapshot().get(normal).unwrap().toast, Toast::Hidden);
        let mut urgent = from("app");
        urgent.hints.urgency = Some(Urgency::Critical);
        let urgent = s.notify(urgent, 0).id;
        assert!(s.snapshot().get(urgent).unwrap().toast.is_shown());
        // An invalid set is refused.
        assert!(!s.set_prefs(NotificationPrefs {
            toast_secs: 0,
            ..NotificationPrefs::default()
        }));
    }

    #[test]
    fn newest_first_and_ids_start_at_one() {
        let mut s = store();
        assert_eq!(s.notify(request("a"), 0).id, id(1));
        assert_eq!(s.notify(request("b"), 1).id, id(2));
        let summaries: Vec<_> = s
            .snapshot()
            .notifications
            .iter()
            .map(|n| n.summary.clone())
            .collect();
        assert_eq!(summaries, ["b", "a"]);
    }

    #[test]
    fn toast_interval_follows_client_default_and_fullscreen() {
        let mut s = store();
        s.notify(request("default"), 100);
        let mut explicit = request("explicit");
        explicit.expire_timeout = ExpireTimeout::Millis(1500);
        s.notify(explicit, 100);
        let mut never = request("never");
        never.expire_timeout = ExpireTimeout::Never;
        s.notify(never, 100);
        s.set_fullscreen(true);
        s.notify(request("fullscreen"), 100);
        let deadline = |summary: &str| match s
            .snapshot()
            .notifications
            .iter()
            .find(|n| n.summary == summary)
            .unwrap()
            .toast
        {
            Toast::Shown { deadline_ms, .. } => deadline_ms,
            Toast::Hidden => None,
        };
        assert_eq!(deadline("default"), Some(5100));
        assert_eq!(deadline("explicit"), Some(1600));
        assert_eq!(deadline("never"), Some(5100));
        assert_eq!(deadline("fullscreen"), Some(2100));
    }

    #[test]
    fn toast_expiry_keeps_history_and_emits_nothing() {
        let mut s = store();
        s.notify(request("a"), 0);
        assert_eq!(s.next_deadline(), Some(5000));
        assert_eq!(s.expire_due(4999), ExpiryOutcome::default());
        let outcome = s.expire_due(5000);
        assert!(outcome.changed);
        assert!(outcome.closed.is_empty());
        let snapshot = s.snapshot();
        assert_eq!(snapshot.notifications.len(), 1);
        assert_eq!(snapshot.toasts().count(), 0);
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn transient_leaves_with_its_toast() {
        let mut s = store();
        let mut transient = request("volume");
        transient.hints.transient = true;
        s.notify(transient, 0);
        let outcome = s.expire_due(5000);
        assert_eq!(
            outcome.closed,
            vec![Closed {
                id: id(1),
                reason: CloseReason::Expired
            }]
        );
        assert!(s.snapshot().notifications.is_empty());
    }

    #[test]
    fn hold_pauses_and_release_restarts_full_interval() {
        let mut s = store();
        s.notify(request("a"), 0);
        assert!(s.hold_toast(id(1)));
        assert!(!s.hold_toast(id(1)));
        assert_eq!(s.next_deadline(), None);
        assert!(!s.expire_due(60_000).changed);
        assert!(s.release_toast(id(1), 60_000));
        assert_eq!(s.next_deadline(), Some(65_000));
    }

    #[test]
    fn opening_the_centre_hides_shown_toasts_silently() {
        let mut s = store();
        s.notify(request("a"), 0);
        s.notify(request("b"), 0);
        assert!(s.set_centre_open(true));
        let snapshot = s.snapshot();
        assert_eq!(snapshot.notifications.len(), 2);
        assert_eq!(snapshot.toasts().count(), 0);
        assert!(!s.set_centre_open(true), "nothing left to hide");
        assert!(!s.set_centre_open(false));
        assert!(s.expire_due(u64::MAX).closed.is_empty());
    }

    #[test]
    fn dnd_and_open_centre_suppress_new_toasts() {
        let mut s = store();
        s.notify(request("shown"), 0);
        s.set_centre_open(false);
        s.centre_open = true;
        s.notify(request("quiet"), 0);
        s.set_centre_open(false);
        assert!(s.set_do_not_disturb(true));
        s.notify(request("dnd"), 0);
        let snapshot = s.snapshot();
        assert_eq!(snapshot.notifications.len(), 3);
        let toasts: Vec<_> = snapshot.toasts().map(|n| n.summary.as_str()).collect();
        assert_eq!(toasts, ["shown"]);
        assert!(snapshot.do_not_disturb);
    }

    #[test]
    fn replacement_keeps_identity_position_and_arrival() {
        let mut s = store();
        s.notify(request("first"), 10);
        s.notify(request("second"), 20);
        s.expire_due(10_000);
        let mut update = request("first, updated");
        update.replaces_id = 1;
        update.hints.progress = Some(40);
        let receipt = s.notify(update, 30);
        assert_eq!(
            receipt,
            NotifyReceipt {
                id: id(1),
                replaced: true,
                evicted: vec![]
            }
        );
        let snapshot = s.snapshot();
        let first = &snapshot.notifications[1];
        assert_eq!(
            (first.id, first.summary.as_str(), first.received_ms),
            (id(1), "first, updated", 10)
        );
        assert_eq!(first.progress, Some(40));
        assert!(first.toast.is_shown(), "replacement re-arms the toast");
        // Unknown replaces_id becomes a new notification.
        let mut stale = request("stale");
        stale.replaces_id = 99;
        assert_eq!(s.notify(stale, 40).id, id(3));
    }

    #[test]
    fn close_reports_reason_once() {
        let mut s = store();
        s.notify(request("a"), 0);
        assert_eq!(
            s.close(id(1), CloseReason::Closed),
            Some(Closed {
                id: id(1),
                reason: CloseReason::Closed
            })
        );
        assert_eq!(s.close(id(1), CloseReason::Closed), None);
    }

    #[test]
    fn invoke_closes_unless_resident_and_rejects_unknown_keys() {
        let mut s = store();
        let mut with_actions = request("a");
        with_actions.actions = pair_actions(&["default".into(), "Open".into()]);
        s.notify(with_actions.clone(), 0);
        assert_eq!(s.invoke(id(1), "missing"), None);
        let receipt = s.invoke(id(1), "default").unwrap();
        assert_eq!(
            receipt.closed,
            Some(Closed {
                id: id(1),
                reason: CloseReason::Dismissed
            })
        );
        assert!(s.snapshot().notifications.is_empty());

        with_actions.hints.resident = true;
        s.notify(with_actions, 0);
        let receipt = s.invoke(id(2), "default").unwrap();
        assert_eq!(receipt.closed, None);
        assert_eq!(s.snapshot().notifications.len(), 1);
    }

    #[test]
    fn hide_toast_keeps_notification_and_clear_all_dismisses() {
        let mut s = store();
        s.notify(request("a"), 0);
        s.notify(request("b"), 0);
        assert!(s.hide_toast(id(2)));
        assert!(!s.hide_toast(id(2)));
        assert_eq!(s.snapshot().notifications.len(), 2);
        let closed = s.clear_all();
        assert_eq!(closed.len(), 2);
        assert!(closed.iter().all(|c| c.reason == CloseReason::Dismissed));
    }

    #[test]
    fn history_limit_evicts_oldest_as_expired() {
        let mut s = NotificationStore::new(StoreConfig {
            history_limit: 2,
            ..StoreConfig::default()
        });
        s.notify(request("a"), 0);
        s.notify(request("b"), 0);
        let receipt = s.notify(request("c"), 0);
        assert_eq!(
            receipt.evicted,
            vec![Closed {
                id: id(1),
                reason: CloseReason::Expired
            }]
        );
    }

    #[test]
    fn ids_skip_zero_and_live_ids_on_wrap() {
        let mut s = store();
        s.notify(request("one"), 0);
        s.next_id = u32::MAX;
        assert_eq!(s.notify(request("max"), 0).id.get(), u32::MAX);
        // Wraps past 0 and past the still-live id 1.
        assert_eq!(s.notify(request("wrapped"), 0).id, id(2));
    }

    #[test]
    fn revision_moves_on_every_visible_change() {
        let mut s = store();
        let r0 = s.snapshot().revision;
        s.notify(request("a"), 0);
        let r1 = s.snapshot().revision;
        assert!(r1 > r0);
        s.expire_due(1);
        assert_eq!(s.snapshot().revision, r1, "nothing due, no change");
        s.hold_toast(id(1));
        assert!(s.snapshot().revision > r1);
    }
}
