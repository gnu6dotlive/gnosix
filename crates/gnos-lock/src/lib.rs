//! LOCK-01: the Gnosix lock screen's pure core.
//!
//! [`LockConfig`] holds every setting, bounded, with a default; the daemon
//! reads it from the durable profile. [`Machine`] is the lock's state: it
//! takes events (the compositor, the keyboard, PAM, the clock) and returns
//! the effects to carry out. Nothing here touches Wayland, PAM or D-Bus, so
//! the security-relevant rules are tested directly:
//! - the session is unlocked only by a successful authentication, reported
//!   for the attempt that is in flight;
//! - a wrong password never unlocks, and after `attempts_before_wait`
//!   failures the lock waits `wait_secs` before taking another;
//! - the typed text is bounded, is never part of an effect except the one
//!   that hands it to PAM, and is cleared as soon as it is sent;
//! - a lock the compositor ends (`finished`) is not an unlock.

mod config;
mod machine;

pub use config::*;
pub use machine::*;
