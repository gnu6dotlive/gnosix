//! Read-only primary-keyboard lock qualification command.
//!
//! Runs the production lock observer for `seconds` (default 2) and prints the
//! first observation and every change. Only `j/devices` is read.
fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    let start = std::time::Instant::now();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(seconds));
        std::process::exit(0);
    });
    gnosix_hyprland::keyboard_lock::run_keyboard_lock_observer(|observation| {
        println!(
            "GNOS_KEYBOARD_LOCK_OBSERVATION t_ms={} value={observation:?}",
            start.elapsed().as_millis()
        );
    });
}
