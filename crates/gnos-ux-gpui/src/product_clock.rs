//! Clock facts are supplied by a time provider, never inferred by Bar widgets.
//!
//! Observations come from `gnos-clock`, already in local civil time with the
//! locale's cycle. This module owns only formatting; timezone, daylight-saving
//! and sampling belong to the observer.
pub use gnos_clock::{ClockCycle, ClockObservation, ClockSampleError, LocalClockSample};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meridiem {
    Am,
    Pm,
}

impl Meridiem {
    /// Lower-case, as the reference Bar shows it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Am => "am",
            Self::Pm => "pm",
        }
    }
}

/// A formatted clock. A 24-hour clock has no meridiem at all: absence is
/// `None`, never an empty or placeholder string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockDisplay {
    pub hour: String,
    pub minute: String,
    pub meridiem: Option<Meridiem>,
    /// The local date ("7 Oct"), when the sample carries one.
    pub date: Option<String>,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec", // i18n
];

pub fn format_clock(sample: LocalClockSample, cycle: ClockCycle) -> ClockDisplay {
    let hour_24 = sample.hour_24();
    let (hour, meridiem) = match cycle {
        ClockCycle::TwentyFourHour => (hour_24, None),
        ClockCycle::TwelveHour => (
            match hour_24 % 12 {
                0 => 12,
                n => n,
            },
            Some(if hour_24 < 12 {
                Meridiem::Am
            } else {
                Meridiem::Pm
            }),
        ),
    };
    ClockDisplay {
        hour: format!("{hour:02}"),
        minute: format!("{:02}", sample.minute()),
        meridiem,
        date: sample
            .date()
            .map(|(day, month)| format!("{day} {}", gnos_i18n::t(MONTHS[usize::from(month - 1)]))),
    }
}

impl From<ClockObservation> for ClockDisplay {
    fn from(observation: ClockObservation) -> Self {
        format_clock(observation.time, observation.cycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u8, minute: u8) -> LocalClockSample {
        LocalClockSample::new(hour, minute).unwrap()
    }

    #[test]
    fn twelve_hour_boundaries_are_derived_without_wall_clock_access() {
        for (hour, display, meridiem) in [
            (0, "12", Meridiem::Am),
            (1, "01", Meridiem::Am),
            (11, "11", Meridiem::Am),
            (12, "12", Meridiem::Pm),
            (13, "01", Meridiem::Pm),
            (22, "10", Meridiem::Pm),
            (23, "11", Meridiem::Pm),
        ] {
            let actual = format_clock(at(hour, 7), ClockCycle::TwelveHour);
            assert_eq!(actual.hour, display, "hour={hour}");
            assert_eq!(actual.minute, "07");
            assert_eq!(actual.meridiem, Some(meridiem), "hour={hour}");
        }
    }

    #[test]
    fn twenty_four_hour_keeps_midnight_and_noon_and_has_no_meridiem() {
        for (hour, display) in [(0, "00"), (12, "12"), (22, "22"), (23, "23")] {
            let value = format_clock(at(hour, 59), ClockCycle::TwentyFourHour);
            assert_eq!((&*value.hour, &*value.minute), (display, "59"));
            assert_eq!(value.meridiem, None);
        }
    }

    #[test]
    fn observations_format_with_their_own_cycle() {
        let observation = ClockObservation {
            time: at(22, 7),
            cycle: ClockCycle::TwelveHour,
        };
        assert_eq!(
            ClockDisplay::from(observation),
            ClockDisplay {
                hour: "10".into(),
                minute: "07".into(),
                meridiem: Some(Meridiem::Pm),
                date: None,
            }
        );
    }
}
