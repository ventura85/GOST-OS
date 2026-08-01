//! The top bar clock: what it says, and when it needs redrawing.
//!
//! No system time is read here, on purpose. This module takes a wall-clock
//! reading and answers two questions — what string to draw, and how long until
//! that string changes — which makes both testable without a clock, a timezone
//! or a screen (D-016). Fetching the time and knowing the timezone is the
//! compositor's job, because that is where the platform lives.
//!
//! **The second question is the load-bearing one.** A clock is the first thing
//! in this shell that changes without the user doing anything, and the obvious
//! implementation — redraw every second and see if it differs — would break the
//! zero-rendering-at-rest rule on day one. [`Wall::until_next_minute`] exists so
//! the compositor can sleep exactly until the display is wrong, and not a
//! moment sooner.

/// A wall-clock reading, already converted to the user's local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wall {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Wall {
    /// Clamps rather than rejects: this comes from a platform time library, and
    /// a leap second reported as `:60` should shift the clock by a second, not
    /// take the shell down.
    pub const fn new(hour: u8, minute: u8, second: u8) -> Self {
        Self {
            hour: if hour > 23 { 23 } else { hour },
            minute: if minute > 59 { 59 } else { minute },
            second: if second > 59 { 59 } else { second },
        }
    }

    /// Seconds until the displayed minute becomes wrong.
    ///
    /// Never zero: a timer armed for zero fires immediately and spins. At the
    /// exact top of a minute the answer is a whole minute, not nothing.
    pub const fn until_next_minute(self) -> u64 {
        60 - self.second as u64
    }

    /// True when these two readings would draw the same string. The compositor
    /// uses this to drop a redraw the timer asked for but nothing needs.
    pub const fn same_minute(self, other: Self) -> bool {
        self.hour == other.hour && self.minute == other.minute
    }
}

/// How the hour is written. A user setting, not a locale guess: the guess is
/// wrong often enough to be irritating, and the setting is one line of TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockFormat {
    /// `14:05`
    #[default]
    H24,
    /// `2:05 PM`
    H12,
}

impl ClockFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "24h" | "h24" => Some(Self::H24),
            "12h" | "h12" => Some(Self::H12),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::H24 => "24h",
            Self::H12 => "12h",
        }
    }
}

/// The string the top bar shows.
pub fn format(at: Wall, fmt: ClockFormat) -> String {
    match fmt {
        ClockFormat::H24 => format!("{:02}:{:02}", at.hour, at.minute),
        ClockFormat::H12 => {
            // 0 and 12 both display as 12, which is the part everyone gets
            // wrong: `hour % 12` alone turns midnight into "0:00 AM".
            let h = match at.hour % 12 {
                0 => 12,
                h => h,
            };
            let suffix = if at.hour < 12 { "AM" } else { "PM" };
            format!("{h}:{:02} {suffix}", at.minute)
        }
    }
}

/// The widest string this format can produce.
///
/// Used to reserve space so the bar does not twitch when 09:59 becomes 10:00.
/// Layout that depends on the current time is layout that moves under the
/// user's finger.
pub const fn widest(fmt: ClockFormat) -> &'static str {
    match fmt {
        ClockFormat::H24 => "00:00",
        // Two-digit hours are the wide case: "12:00 PM" beats "9:00 AM".
        ClockFormat::H12 => "12:00 PM",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_four_hour_pads_the_hour() {
        assert_eq!(format(Wall::new(9, 5, 0), ClockFormat::H24), "09:05");
        assert_eq!(format(Wall::new(14, 5, 0), ClockFormat::H24), "14:05");
        assert_eq!(format(Wall::new(0, 0, 0), ClockFormat::H24), "00:00");
        assert_eq!(format(Wall::new(23, 59, 0), ClockFormat::H24), "23:59");
    }

    #[test]
    fn twelve_hour_shows_midnight_and_noon_as_twelve() {
        // The bug this test exists for: `hour % 12` gives "0:00 AM".
        assert_eq!(format(Wall::new(0, 0, 0), ClockFormat::H12), "12:00 AM");
        assert_eq!(format(Wall::new(12, 0, 0), ClockFormat::H12), "12:00 PM");
        assert_eq!(format(Wall::new(13, 7, 0), ClockFormat::H12), "1:07 PM");
        assert_eq!(format(Wall::new(11, 59, 0), ClockFormat::H12), "11:59 AM");
    }

    #[test]
    fn the_seconds_never_reach_the_string() {
        // Drawing seconds would mean redrawing every second forever.
        let a = format(Wall::new(10, 30, 0), ClockFormat::H24);
        let b = format(Wall::new(10, 30, 59), ClockFormat::H24);
        assert_eq!(a, b);
    }

    #[test]
    fn the_wait_is_never_zero() {
        // A calloop timer armed for zero fires immediately, and the shell spins
        // at 100% CPU drawing the same minute forever.
        for second in 0..=59u8 {
            let w = Wall::new(12, 0, second).until_next_minute();
            assert!((1..=60).contains(&w), "second {second} gave {w}");
        }
        assert_eq!(Wall::new(12, 0, 0).until_next_minute(), 60);
        assert_eq!(Wall::new(12, 0, 59).until_next_minute(), 1);
    }

    #[test]
    fn a_leap_second_shifts_the_clock_rather_than_killing_it() {
        let w = Wall::new(23, 59, 60);
        assert_eq!(w.second, 59);
        assert_eq!(w.until_next_minute(), 1);
    }

    #[test]
    fn out_of_range_input_is_clamped_not_wrapped() {
        // Wrapping would turn a bad reading into a plausible wrong time, which
        // is worse than an obviously pinned one.
        assert_eq!(Wall::new(99, 99, 99), Wall::new(23, 59, 59));
    }

    #[test]
    fn same_minute_ignores_seconds_only() {
        let a = Wall::new(10, 30, 0);
        assert!(a.same_minute(Wall::new(10, 30, 59)));
        assert!(!a.same_minute(Wall::new(10, 31, 0)));
        assert!(!a.same_minute(Wall::new(11, 30, 0)));
    }

    #[test]
    fn the_reserved_string_is_at_least_as_wide_as_anything_drawn() {
        // If this fails the bar twitches when the time rolls over.
        for fmt in [ClockFormat::H24, ClockFormat::H12] {
            let reserved = widest(fmt).chars().count();
            for hour in 0..24u8 {
                for minute in [0u8, 9, 10, 59] {
                    let drawn = format(Wall::new(hour, minute, 0), fmt).chars().count();
                    assert!(
                        drawn <= reserved,
                        "{fmt:?} {hour}:{minute} draws {drawn} > reserved {reserved}"
                    );
                }
            }
        }
    }

    #[test]
    fn format_names_round_trip() {
        for fmt in [ClockFormat::H24, ClockFormat::H12] {
            assert_eq!(ClockFormat::parse(fmt.as_str()), Some(fmt));
        }
        assert_eq!(ClockFormat::parse("swiss"), None);
    }
}
