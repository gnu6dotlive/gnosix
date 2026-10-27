//! Read-only local clock qualification command.
//!
//! Prints the locale time format, the derived cycle and `count` observations
//! (default 1) as the minute observer delivers them.
fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    println!(
        "GNOS_CLOCK_LOCALE t_fmt={:?} cycle={}",
        gnos_clock::locale_time_format(),
        gnos_clock::locale_clock_cycle().as_str()
    );
    let mut seen = 0usize;
    let result = gnos_clock::run_minute_observer(|observation| {
        println!(
            "GNOS_CLOCK_SAMPLE wall={} hour={:02} minute={:02} cycle={}",
            chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z"),
            observation.time.hour_24(),
            observation.time.minute(),
            observation.cycle.as_str()
        );
        seen += 1;
        if seen >= count {
            std::process::exit(0);
        }
    });
    if let Err(error) = result {
        eprintln!("GNOS_CLOCK_OBSERVER_FAIL error={error}");
        std::process::exit(1);
    }
}
