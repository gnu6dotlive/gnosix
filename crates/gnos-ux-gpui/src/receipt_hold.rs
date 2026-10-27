//! LIVE-03: receipts held while a topology cycle decides whether it matters.
//!
//! The topology loop reconciles on every Hyprland event and on a safety
//! resync every two seconds. Most cycles change nothing, and printing their
//! ten receipts each time flooded the journal (about 650 lines a minute),
//! so older receipts were rotated away before anyone read them. A cycle now
//! holds its receipts; when it changed something they are printed as before,
//! and when it did not they are dropped and counted, and
//! `GNOS_UX_TOPOLOGY_QUIET` reports the count once a minute.
//!
//! Only `println!` receipts go through here; errors (`eprintln!`) are never
//! held.

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// How often the quiet cycles are summed up.
pub const QUIET_REPORT: Duration = Duration::from_secs(60);

thread_local! {
    static HELD: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Hold the receipts printed from now on (on this thread).
pub fn hold() {
    HELD.with(|h| *h.borrow_mut() = Some(Vec::new()));
}

/// Print a receipt, or keep it while a cycle holds them.
pub fn line(text: String) {
    HELD.with(|h| match h.borrow_mut().as_mut() {
        Some(held) => held.push(text),
        None => println!("{text}"),
    });
}

/// Stop holding: print what was held (`publish`) or drop it. Returns how
/// many receipts were held.
pub fn release(publish: bool) -> usize {
    let held = HELD.with(|h| h.borrow_mut().take()).unwrap_or_default();
    if publish {
        for text in &held {
            println!("{text}");
        }
    }
    held.len()
}

/// The quiet cycles since the last report.
#[derive(Debug)]
pub struct Quiet {
    cycles: u64,
    receipts: u64,
    since: Instant,
}

impl Default for Quiet {
    fn default() -> Self {
        Self {
            cycles: 0,
            receipts: 0,
            since: Instant::now(),
        }
    }
}

impl Quiet {
    /// Count one quiet cycle; the report line once `QUIET_REPORT` has passed.
    pub fn count(&mut self, receipts: usize, generation: u64) -> Option<String> {
        self.cycles += 1;
        self.receipts += receipts as u64;
        (self.since.elapsed() >= QUIET_REPORT).then(|| {
            let report = format!(
                "GNOS_UX_TOPOLOGY_QUIET cycles={} receipts_dropped={} seconds={} semantic_generation={generation}",
                self.cycles,
                self.receipts,
                self.since.elapsed().as_secs()
            );
            *self = Self::default();
            report
        })
    }
}

/// `println!` through the hold.
#[macro_export]
macro_rules! receipt {
    ($($arg:tt)*) => {
        $crate::receipt_hold::line(format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_receipts_are_kept_or_dropped_and_quiet_cycles_counted() {
        hold();
        line("a".into());
        line("b".into());
        assert_eq!(release(false), 2);
        assert_eq!(release(false), 0, "nothing held any more");
        let mut q = Quiet::default();
        assert_eq!(q.count(10, 7), None);
        q.since -= QUIET_REPORT;
        let report = q.count(10, 7).unwrap();
        assert!(report.contains("cycles=2 receipts_dropped=20"));
        assert!(report.ends_with("semantic_generation=7"));
        assert_eq!(q.cycles, 0);
    }
}
