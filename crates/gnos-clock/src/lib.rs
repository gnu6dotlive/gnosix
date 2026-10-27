//! Typed, read-only local civil time observation.
//!
//! Ownership is explicit:
//!
//! - Timezone and daylight-saving resolution belong to this observer, through
//!   `chrono::Local`, which reads `TZ` / `/etc/localtime` and notices when the
//!   zone file changes. Display code never converts time.
//! - The 12/24-hour cycle is the process locale's `LC_TIME` time format, read
//!   without mutating the process-global locale.
//! - Wakeups are absolute `CLOCK_REALTIME` deadlines at the next local minute,
//!   recomputed from a fresh sample every time, so error never accumulates. The
//!   timer is cancelled when the realtime clock is set discontinuously (manual
//!   set, NTP step), and an absolute deadline that passed during suspend fires
//!   on resume; both cases resample immediately.

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};

/// One local civil time, to the minute, with its local date when the
/// observer knows it (BAR-01D: the Bar's clock can show the date).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalClockSample {
    hour_24: u8,
    minute: u8,
    /// (day of the month 1..=31, month 1..=12).
    date: Option<(u8, u8)>,
    /// SURF-03: the local year, with the date, when known (the dashboard's
    /// calendar needs it; the Bar never shows it).
    year: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockSampleError {
    InvalidHour,
    InvalidMinute,
}

impl LocalClockSample {
    pub fn new(hour_24: u8, minute: u8) -> Result<Self, ClockSampleError> {
        if hour_24 >= 24 {
            return Err(ClockSampleError::InvalidHour);
        }
        if minute >= 60 {
            return Err(ClockSampleError::InvalidMinute);
        }
        Ok(Self {
            hour_24,
            minute,
            date: None,
            year: None,
        })
    }

    /// The same time on a local date; an impossible day or month leaves it
    /// without one.
    pub fn with_date(mut self, day: u8, month: u8) -> Self {
        self.date = ((1..=31).contains(&day) && (1..=12).contains(&month)).then_some((day, month));
        self
    }

    /// (day of the month, month), when known.
    pub const fn date(self) -> Option<(u8, u8)> {
        self.date
    }

    /// The same sample in a known local year (only with a date).
    pub fn with_year(mut self, year: i32) -> Self {
        self.year = self.date.map(|_| year);
        self
    }

    /// The local year, when known with the date.
    pub const fn year(self) -> Option<i32> {
        self.year
    }

    pub const fn hour_24(self) -> u8 {
        self.hour_24
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockCycle {
    TwelveHour,
    TwentyFourHour,
}

impl ClockCycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TwelveHour => "12h",
            Self::TwentyFourHour => "24h",
        }
    }
}

/// What the product needs to show a clock: the local time and the cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockObservation {
    pub time: LocalClockSample,
    pub cycle: ClockCycle,
}

/// A POSIX `strftime` time format uses a 12-hour cycle when it contains a
/// 12-hour conversion: `%I`, `%l`, `%r` (= `%I:%M:%S %p`) or a meridiem
/// `%p`/`%P`. `%%` is a literal percent and is skipped.
pub fn cycle_from_time_format(format: &str) -> ClockCycle {
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            continue;
        }
        // Skip glibc flags/width (`%-I`, `%_I`, `%0I`, `%^p`, `%#p`, `%2I`).
        let mut conversion = chars.next();
        while let Some(flag) = conversion
            && (matches!(flag, '-' | '_' | '0' | '^' | '#') || flag.is_ascii_digit())
        {
            conversion = chars.next();
        }
        // `E`/`O` modifiers precede an ordinary conversion.
        if matches!(conversion, Some('E' | 'O')) {
            conversion = chars.next();
        }
        if matches!(conversion, Some('I' | 'l' | 'r' | 'p' | 'P')) {
            return ClockCycle::TwelveHour;
        }
    }
    ClockCycle::TwentyFourHour
}

/// The `LC_TIME` time format (`T_FMT`) of the locale this process was started
/// with. `newlocale` builds a private locale object from the environment, so
/// the process-global locale is neither read through `setlocale` nor changed.
/// When the environment names no usable locale, glibc's answer is the C
/// locale, which is itself a real 24-hour fact (`%H:%M:%S`).
pub fn locale_time_format() -> String {
    let empty = c"";
    let fallback = c"C";
    // SAFETY: both names are valid NUL-terminated strings; a null base asks for
    // a fresh object. The returned handle is checked before use and freed once.
    let mut locale =
        unsafe { libc::newlocale(libc::LC_TIME_MASK, empty.as_ptr(), std::ptr::null_mut()) };
    if locale.is_null() {
        // SAFETY: as above.
        locale =
            unsafe { libc::newlocale(libc::LC_TIME_MASK, fallback.as_ptr(), std::ptr::null_mut()) };
    }
    if locale.is_null() {
        return "%H:%M:%S".to_owned();
    }
    // SAFETY: `locale` is a live locale object. The returned pointer is owned
    // by it and copied before the object is freed.
    unsafe {
        let raw = libc::nl_langinfo_l(libc::T_FMT, locale);
        let format = if raw.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned()
        };
        libc::freelocale(locale);
        format
    }
}

pub fn locale_clock_cycle() -> ClockCycle {
    cycle_from_time_format(&locale_time_format())
}

/// The local second of the minute now (0-59), for a clock that shows
/// seconds. Only asked for while the operator turns seconds on.
pub fn local_second() -> u8 {
    Local::now().second() as u8
}

/// Truncate a zoned instant to its local minute.
pub fn sample_at<Tz: TimeZone>(instant: &DateTime<Tz>) -> LocalClockSample {
    LocalClockSample {
        hour_24: instant.hour() as u8,
        minute: instant.minute() as u8,
        date: Some((instant.day() as u8, instant.month() as u8)),
        year: Some(instant.year()),
    }
}

/// Unix seconds of the next local minute boundary after `instant`.
///
/// Computed from the local seconds-of-minute, not from UTC, so a zone whose
/// offset is not a whole minute still wakes on its own minute.
pub fn next_minute_deadline<Tz: TimeZone>(instant: &DateTime<Tz>) -> i64 {
    instant.timestamp() - i64::from(instant.second()) + 60
}

/// Observe local time for as long as the process lives, calling `on_sample`
/// once immediately and then once per local minute, plus after every
/// discontinuous realtime clock change. Never returns on success.
pub fn run_minute_observer<F>(mut on_sample: F) -> std::io::Result<()>
where
    F: FnMut(ClockObservation),
{
    let cycle = locale_clock_cycle();
    let timer = RealtimeTimer::new()?;
    loop {
        let now = Local::now();
        on_sample(ClockObservation {
            time: sample_at(&now),
            cycle,
        });
        timer.wait_until(next_minute_deadline(&now))?;
    }
}

struct RealtimeTimer(std::os::fd::OwnedFd);

impl RealtimeTimer {
    fn new() -> std::io::Result<Self> {
        use std::os::fd::FromRawFd;
        // SAFETY: plain syscall; the result is checked before it is owned.
        let fd = unsafe { libc::timerfd_create(libc::CLOCK_REALTIME, libc::TFD_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `fd` is a fresh descriptor owned by nobody else.
        Ok(Self(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) }))
    }

    /// Block until the absolute realtime `deadline` (Unix seconds) or until
    /// the realtime clock is set, whichever comes first.
    fn wait_until(&self, deadline: i64) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;
        let spec = libc::itimerspec {
            it_interval: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: libc::timespec {
                tv_sec: deadline as libc::time_t,
                tv_nsec: 0,
            },
        };
        // SAFETY: `spec` is a valid itimerspec and the old value is not wanted.
        let armed = unsafe {
            libc::timerfd_settime(
                self.0.as_raw_fd(),
                libc::TFD_TIMER_ABSTIME | libc::TFD_TIMER_CANCEL_ON_SET,
                &spec,
                std::ptr::null_mut(),
            )
        };
        if armed < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut expirations = [0u8; 8];
        loop {
            // SAFETY: reading 8 bytes into an 8-byte buffer.
            let read = unsafe {
                libc::read(
                    self.0.as_raw_fd(),
                    expirations.as_mut_ptr().cast(),
                    expirations.len(),
                )
            };
            if read == 8 {
                return Ok(());
            }
            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EINTR) => continue,
                // The realtime clock was set: resample now.
                Some(libc::ECANCELED) => return Ok(()),
                _ => return Err(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    #[test]
    fn locale_time_formats_decide_the_cycle() {
        for (format, cycle) in [
            ("%r", ClockCycle::TwelveHour),
            ("%I:%M:%S %p", ClockCycle::TwelveHour),
            ("%l:%M %P", ClockCycle::TwelveHour),
            ("%-I:%M", ClockCycle::TwelveHour),
            ("%H:%M:%S", ClockCycle::TwentyFourHour),
            ("%T", ClockCycle::TwentyFourHour),
            ("%OH:%OM", ClockCycle::TwentyFourHour),
            ("%%I %H", ClockCycle::TwentyFourHour),
            ("", ClockCycle::TwentyFourHour),
        ] {
            assert_eq!(cycle_from_time_format(format), cycle, "{format:?}");
        }
    }

    #[test]
    fn samples_are_local_and_truncated_to_the_minute() {
        let zone = FixedOffset::west_opt(4 * 3600).unwrap();
        let instant = zone.with_ymd_and_hms(2026, 10, 7, 22, 7, 59).unwrap();
        assert_eq!(
            sample_at(&instant),
            LocalClockSample::new(22, 7)
                .unwrap()
                .with_date(7, 10)
                .with_year(2026)
        );
        let midnight = zone.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap();
        // The local date turns over with the local midnight.
        assert_eq!(
            sample_at(&midnight),
            LocalClockSample::new(0, 0)
                .unwrap()
                .with_date(8, 10)
                .with_year(2026)
        );
        assert_eq!(
            LocalClockSample::new(0, 0).unwrap().with_date(32, 1).date(),
            None
        );
        // A year needs a date.
        assert_eq!(
            LocalClockSample::new(0, 0).unwrap().with_year(2026).year(),
            None
        );
    }

    #[test]
    fn the_next_deadline_is_the_next_local_minute_without_drift() {
        let zone = FixedOffset::west_opt(4 * 3600).unwrap();
        let instant = zone.with_ymd_and_hms(2026, 10, 7, 22, 7, 59).unwrap();
        let deadline = next_minute_deadline(&instant);
        assert_eq!(deadline - instant.timestamp(), 1);
        let boundary = zone.timestamp_opt(deadline, 0).unwrap();
        assert_eq!((boundary.minute(), boundary.second()), (8, 0));
        // Exactly on a boundary, the next one is a full minute away: a timer
        // that fired on time never re-arms for the same minute.
        assert_eq!(next_minute_deadline(&boundary) - deadline, 60);
    }

    #[test]
    fn a_zone_offset_with_seconds_still_wakes_on_its_own_minute() {
        // Historic Monrovia time was UTC-0:44:30.
        let zone = FixedOffset::west_opt(44 * 60 + 30).unwrap();
        let instant = zone.with_ymd_and_hms(1970, 6, 1, 10, 0, 10).unwrap();
        let boundary = zone
            .timestamp_opt(next_minute_deadline(&instant), 0)
            .unwrap();
        assert_eq!((boundary.minute(), boundary.second()), (1, 0));
    }

    #[test]
    fn invalid_samples_fail_instead_of_wrapping() {
        assert_eq!(
            LocalClockSample::new(24, 0),
            Err(ClockSampleError::InvalidHour)
        );
        assert_eq!(
            LocalClockSample::new(7, 60),
            Err(ClockSampleError::InvalidMinute)
        );
    }
}
