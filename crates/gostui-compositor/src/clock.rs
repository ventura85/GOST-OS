//! Reading the local wall clock.
//!
//! The whole platform half of the clock, and deliberately tiny: what the clock
//! *says* and when it next changes are decided in `gostui_core::clock`, where
//! they can be tested without a timezone or a screen (D-016). All that is left
//! here is asking the operating system what time it is.

use gostui_core::clock::Wall;

/// The current local time.
///
/// Falls back to UTC when the system has no timezone database — a stripped
/// container, or a phone image built without `/usr/share/zoneinfo`. A clock an
/// hour or two out is a nuisance; refusing to start over it would not be.
pub fn now_local() -> Wall {
    let now = jiff::Zoned::now();
    Wall::new(now.hour() as u8, now.minute() as u8, now.second() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_clock_reads_as_a_valid_wall_time() {
        // Not asserting a value — asserting that whatever the platform hands
        // back has already been through the clamp in core, so nothing downstream
        // has to defend against a 25th hour.
        let w = now_local();
        assert!(w.hour <= 23 && w.minute <= 59 && w.second <= 59, "{w:?}");
        assert!((1..=60).contains(&w.until_next_minute()));
    }
}
