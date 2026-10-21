use crate::LockConfig;
use std::time::{Duration, Instant};

/// The longest passphrase the prompt takes.
pub const MAX_TYPED: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockPhase {
    /// No lock: the session is open.
    Unlocked,
    /// The lock was asked for; the compositor has not confirmed it.
    Locking,
    /// The compositor holds the lock; the prompt takes input.
    Locked,
    /// A password is with PAM; input is ignored until it answers.
    Authenticating,
    /// Too many failures: the prompt takes nothing until the wait ends.
    Waiting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockEvent {
    /// logind's `Lock`, the idle timer, the sleep inhibitor, or the Bar.
    LockRequested,
    /// `ext_session_lock_v1.locked`.
    CompositorLocked,
    /// `ext_session_lock_v1.finished`: the compositor ended the lock
    /// (another client holds it, or it refused). Never an unlock.
    CompositorFinished,
    Char(char),
    Backspace,
    /// Clear what is typed (Escape).
    Clear,
    Submit,
    /// PAM answered the attempt `attempt`.
    AuthResult {
        attempt: u64,
        success: bool,
    },
    Tick(Instant),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockEffect {
    /// Ask the compositor for the lock.
    RequestCompositorLock,
    /// Hand the password to PAM for attempt `attempt`. The only effect that
    /// carries the typed text.
    Authenticate {
        attempt: u64,
        password: String,
    },
    /// Authentication succeeded: unlock the compositor, then tell logind.
    Unlock,
    /// Tell logind the session is (not) locked.
    SetLockedHint(bool),
    Redraw,
}

#[derive(Clone, Debug)]
pub struct Machine {
    config: LockConfig,
    phase: LockPhase,
    typed: String,
    failures: u8,
    attempt: u64,
    wait_until: Option<Instant>,
    /// What the screen says about the last attempt.
    notice: Option<Notice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Wrong,
    Waiting { secs_left: u32 },
}

impl Machine {
    pub fn new(config: LockConfig) -> Self {
        Self {
            config: config.or_default(),
            phase: LockPhase::Unlocked,
            typed: String::new(),
            failures: 0,
            attempt: 0,
            wait_until: None,
            notice: None,
        }
    }

    pub fn phase(&self) -> LockPhase {
        self.phase
    }

    pub fn config(&self) -> &LockConfig {
        &self.config
    }

    /// New settings apply from the next lock; one in progress keeps its own.
    pub fn set_config(&mut self, config: LockConfig) {
        if self.phase == LockPhase::Unlocked {
            self.config = config.or_default();
        }
    }

    /// How many characters to show as dots (zero when the setting hides it).
    pub fn dots(&self) -> usize {
        if self.config.show_typed_length {
            self.typed.chars().count()
        } else {
            0
        }
    }

    pub fn notice(&self) -> Option<Notice> {
        self.notice
    }

    pub fn is_locked(&self) -> bool {
        self.phase != LockPhase::Unlocked
    }

    fn wipe(&mut self) {
        // Overwrite before dropping, so the text does not linger in memory.
        let len = self.typed.len();
        self.typed.clear();
        self.typed.extend(std::iter::repeat_n('\0', len));
        self.typed.clear();
    }

    pub fn handle(&mut self, event: LockEvent) -> Vec<LockEffect> {
        use LockEffect::*;
        use LockPhase::*;
        match (self.phase, event) {
            (Unlocked, LockEvent::LockRequested) => {
                self.phase = Locking;
                vec![RequestCompositorLock]
            }
            (_, LockEvent::LockRequested) => Vec::new(),
            (Locking, LockEvent::CompositorLocked) => {
                self.phase = Locked;
                self.failures = 0;
                self.notice = None;
                vec![SetLockedHint(true), Redraw]
            }
            // The compositor ended the lock before it was ever held: the
            // session never locked, so there is nothing to unlock.
            (Locking, LockEvent::CompositorFinished) => {
                self.phase = Unlocked;
                Vec::new()
            }
            // Once held, a finished lock means the compositor dropped it
            // (or crashed). The screen is the compositor's now: report it
            // as unlocked, never as a successful authentication.
            (Locked | Authenticating | Waiting, LockEvent::CompositorFinished) => {
                self.wipe();
                self.phase = Unlocked;
                vec![SetLockedHint(false)]
            }
            (Locked, LockEvent::Char(c)) => {
                if !c.is_control() && self.typed.chars().count() < MAX_TYPED {
                    self.typed.push(c);
                    self.notice = None;
                    return vec![Redraw];
                }
                Vec::new()
            }
            (Locked, LockEvent::Backspace) => {
                if self.typed.pop().is_some() {
                    return vec![Redraw];
                }
                Vec::new()
            }
            (Locked, LockEvent::Clear) => {
                self.wipe();
                vec![Redraw]
            }
            (Locked, LockEvent::Submit) => {
                if self.typed.is_empty() {
                    return Vec::new();
                }
                self.attempt += 1;
                self.phase = Authenticating;
                let password = std::mem::take(&mut self.typed);
                vec![
                    Authenticate {
                        attempt: self.attempt,
                        password,
                    },
                    Redraw,
                ]
            }
            (Authenticating, LockEvent::AuthResult { attempt, success })
                if attempt == self.attempt =>
            {
                if success {
                    self.failures = 0;
                    self.notice = None;
                    self.phase = Unlocked;
                    return vec![Unlock, SetLockedHint(false)];
                }
                self.failures += 1;
                if self.failures >= self.config.attempts_before_wait {
                    self.failures = 0;
                    self.phase = Waiting;
                    self.wait_until = None;
                    self.notice = Some(Notice::Waiting {
                        secs_left: self.config.wait_secs,
                    });
                    // The wait starts at the next tick, which carries a time.
                } else {
                    self.phase = Locked;
                    self.notice = Some(Notice::Wrong);
                }
                vec![Redraw]
            }
            // An answer for another attempt, or none in flight, is ignored:
            // it can never unlock.
            (_, LockEvent::AuthResult { .. }) => Vec::new(),
            (Waiting, LockEvent::Tick(now)) => {
                let until = *self
                    .wait_until
                    .get_or_insert(now + Duration::from_secs(self.config.wait_secs.into()));
                if now >= until {
                    self.wait_until = None;
                    self.phase = Locked;
                    self.notice = None;
                } else {
                    let secs_left = (until - now).as_secs_f32().ceil() as u32;
                    self.notice = Some(Notice::Waiting { secs_left });
                }
                vec![Redraw]
            }
            (_, LockEvent::Tick(_)) => Vec::new(),
            // Input outside the prompt's phase is dropped.
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locked() -> Machine {
        let mut m = Machine::new(LockConfig::default());
        assert_eq!(
            m.handle(LockEvent::LockRequested),
            vec![LockEffect::RequestCompositorLock]
        );
        assert_eq!(
            m.handle(LockEvent::CompositorLocked),
            vec![LockEffect::SetLockedHint(true), LockEffect::Redraw]
        );
        m
    }

    fn type_in(m: &mut Machine, text: &str) {
        for c in text.chars() {
            m.handle(LockEvent::Char(c));
        }
    }

    fn submit(m: &mut Machine) -> u64 {
        match m.handle(LockEvent::Submit).first() {
            Some(LockEffect::Authenticate { attempt, .. }) => *attempt,
            other => panic!("expected an authentication, got {other:?}"),
        }
    }

    #[test]
    fn it_unlocks_only_on_a_successful_answer_for_the_attempt_in_flight() {
        let mut m = locked();
        type_in(&mut m, "hunter2");
        let attempt = submit(&mut m);
        assert_eq!(m.phase(), LockPhase::Authenticating);
        // Another attempt's success, and a success with nothing in flight,
        // never unlock.
        assert!(
            m.handle(LockEvent::AuthResult {
                attempt: attempt + 1,
                success: true
            })
            .is_empty()
        );
        assert_eq!(m.phase(), LockPhase::Authenticating);
        assert_eq!(
            m.handle(LockEvent::AuthResult {
                attempt,
                success: true
            }),
            vec![LockEffect::Unlock, LockEffect::SetLockedHint(false)]
        );
        assert_eq!(m.phase(), LockPhase::Unlocked);
        assert!(
            m.handle(LockEvent::AuthResult {
                attempt,
                success: true
            })
            .is_empty()
        );
    }

    #[test]
    fn a_success_without_a_submission_does_nothing() {
        let mut m = locked();
        assert!(
            m.handle(LockEvent::AuthResult {
                attempt: 1,
                success: true
            })
            .is_empty()
        );
        assert_eq!(m.phase(), LockPhase::Locked);
    }

    #[test]
    fn the_password_is_in_exactly_one_effect_and_is_cleared_when_sent() {
        let mut m = locked();
        type_in(&mut m, "s3cret");
        assert_eq!(m.dots(), 6);
        let effects = m.handle(LockEvent::Submit);
        assert!(
            matches!(&effects[0], LockEffect::Authenticate { password, .. } if password == "s3cret")
        );
        assert_eq!(m.dots(), 0);
        assert!(!format!("{:?}", &effects[1..]).contains("s3cret"));
        // Input is ignored while PAM has it.
        assert!(m.handle(LockEvent::Char('x')).is_empty());
        assert_eq!(m.dots(), 0);
    }

    #[test]
    fn a_wrong_password_returns_to_the_prompt_and_says_so() {
        let mut m = locked();
        type_in(&mut m, "nope");
        let attempt = submit(&mut m);
        m.handle(LockEvent::AuthResult {
            attempt,
            success: false,
        });
        assert_eq!(m.phase(), LockPhase::Locked);
        assert_eq!(m.notice(), Some(Notice::Wrong));
        type_in(&mut m, "a");
        assert_eq!(m.notice(), None);
    }

    #[test]
    fn enough_failures_make_it_wait_then_it_takes_input_again() {
        let mut m = locked();
        let t0 = Instant::now();
        for _ in 0..3 {
            type_in(&mut m, "bad");
            let attempt = submit(&mut m);
            m.handle(LockEvent::AuthResult {
                attempt,
                success: false,
            });
        }
        assert_eq!(m.phase(), LockPhase::Waiting);
        // Nothing is taken while it waits.
        assert!(m.handle(LockEvent::Char('x')).is_empty());
        assert!(m.handle(LockEvent::Submit).is_empty());
        m.handle(LockEvent::Tick(t0));
        assert_eq!(m.notice(), Some(Notice::Waiting { secs_left: 30 }));
        m.handle(LockEvent::Tick(t0 + Duration::from_secs(10)));
        assert_eq!(m.notice(), Some(Notice::Waiting { secs_left: 20 }));
        assert_eq!(m.phase(), LockPhase::Waiting);
        m.handle(LockEvent::Tick(t0 + Duration::from_secs(31)));
        assert_eq!(m.phase(), LockPhase::Locked);
        assert_eq!(m.notice(), None);
    }

    #[test]
    fn the_compositor_ending_the_lock_is_never_an_authentication() {
        let mut m = Machine::new(LockConfig::default());
        m.handle(LockEvent::LockRequested);
        // Refused before it was held: nothing to undo.
        assert!(m.handle(LockEvent::CompositorFinished).is_empty());
        assert_eq!(m.phase(), LockPhase::Unlocked);
        let mut m = locked();
        type_in(&mut m, "abc");
        assert_eq!(
            m.handle(LockEvent::CompositorFinished),
            vec![LockEffect::SetLockedHint(false)]
        );
        assert_eq!(m.phase(), LockPhase::Unlocked);
        assert_eq!(m.dots(), 0);
    }

    #[test]
    fn typing_is_bounded_ignores_controls_and_an_empty_submit_does_nothing() {
        let mut m = locked();
        assert!(m.handle(LockEvent::Submit).is_empty());
        assert!(m.handle(LockEvent::Char('\n')).is_empty());
        for _ in 0..MAX_TYPED + 10 {
            m.handle(LockEvent::Char('a'));
        }
        assert_eq!(m.dots(), MAX_TYPED);
        m.handle(LockEvent::Backspace);
        assert_eq!(m.dots(), MAX_TYPED - 1);
        m.handle(LockEvent::Clear);
        assert_eq!(m.dots(), 0);
    }

    #[test]
    fn a_lock_request_while_locked_changes_nothing_and_settings_wait_for_unlock() {
        let mut m = locked();
        assert!(m.handle(LockEvent::LockRequested).is_empty());
        let changed = LockConfig {
            wait_secs: 99,
            ..LockConfig::default()
        };
        m.set_config(changed);
        assert_eq!(m.config().wait_secs, 30);
        let mut m = Machine::new(LockConfig::default());
        m.set_config(LockConfig {
            wait_secs: 99,
            ..LockConfig::default()
        });
        assert_eq!(m.config().wait_secs, 99);
    }

    #[test]
    fn the_typed_length_can_be_hidden() {
        let mut m = Machine::new(LockConfig {
            show_typed_length: false,
            ..LockConfig::default()
        });
        m.handle(LockEvent::LockRequested);
        m.handle(LockEvent::CompositorLocked);
        type_in(&mut m, "abc");
        assert_eq!(m.dots(), 0);
    }
}
