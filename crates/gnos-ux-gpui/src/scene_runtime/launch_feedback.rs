//! CURSOR-01B: an application starting, until its window is there.
//!
//! After a launch the shell is still usable, but what was asked for has not
//! appeared yet: the pointer says so (`progress`). It is the freedesktop
//! startup notification's feedback, read from what the shell already knows:
//! the launch ends when a window appears on the application's Deck card
//! that was not there when it was launched. A program that never opens a
//! window ends it at the operator's ceiling (Settings › Behaviour, 15 s by
//! default, as launchers do). Receipt: `GNOS_UX_LAUNCH_FEEDBACK`.

use super::*;
use gnosix_desktop_model::SurfaceId;

/// One application starting.
pub(super) struct Launching {
    key: String,
    /// Its windows when it was launched.
    seen: Vec<SurfaceId>,
    since: Instant,
}

impl SceneHost {
    /// The application's windows now (its Deck card's).
    fn launch_windows(&self, key: &str) -> Vec<SurfaceId> {
        self.deck_cards
            .iter()
            .find(|c| c.key == key)
            .map(|c| c.windows.iter().map(|w| w.reference.surface_id()).collect())
            .unwrap_or_default()
    }

    /// After a launch: when it started, follow it until its window is there.
    pub(super) fn note_launch(
        &mut self,
        record: &crate::product_runtime::ProductLaunchRecord,
        cx: &mut Context<Self>,
    ) {
        let ceiling = crate::surface_prefs::current().behaviour.launch_feedback_s;
        if ceiling == 0 || !record.terminal.as_ref().is_some_and(|t| t.is_launched()) {
            return;
        }
        let key = record.entry.as_str().to_owned();
        let seen = self.launch_windows(&key);
        println!(
            "GNOS_UX_LAUNCH_FEEDBACK state=started entry={key} windows={}",
            seen.len()
        );
        self.launching.retain(|l| l.key != key);
        self.launching.push(Launching {
            key,
            seen,
            since: Instant::now(),
        });
        // The ceiling's wake: the pointer is told even if nothing else moves.
        let wait = Duration::from_secs(ceiling.into());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        cx.notify();
    }

    /// Each frame, with the Deck's cards fresh: the launches whose window
    /// appeared, or that reached the ceiling, end. Returns whether any did.
    pub(super) fn settle_launches(&mut self) -> bool {
        if self.launching.is_empty() {
            return false;
        }
        let ceiling = Duration::from_secs(
            crate::surface_prefs::current()
                .behaviour
                .launch_feedback_s
                .into(),
        );
        let before = self.launching.len();
        let launching = std::mem::take(&mut self.launching);
        for l in launching {
            let now = self.launch_windows(&l.key);
            if let Some(window) = now.iter().find(|w| !l.seen.contains(w)) {
                println!(
                    "GNOS_UX_LAUNCH_FEEDBACK state=window entry={} after_ms={} surface={window:?}",
                    l.key,
                    l.since.elapsed().as_millis()
                );
            } else if ceiling.is_zero() || l.since.elapsed() >= ceiling {
                println!(
                    "GNOS_UX_LAUNCH_FEEDBACK state=ceiling entry={} after_ms={}",
                    l.key,
                    l.since.elapsed().as_millis()
                );
            } else {
                self.launching.push(l);
            }
        }
        self.launching.len() != before
    }

    /// An application starting (the pointer says `progress`).
    pub(super) fn launching(&self) -> bool {
        !self.launching.is_empty()
    }
}
