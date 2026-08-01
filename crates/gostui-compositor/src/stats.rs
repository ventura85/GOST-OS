//! Frame accounting for the "zero rendering at rest" rule.
//!
//! The specification requires that a shell nobody is touching draws nothing.
//! Without a number that claim is an opinion, so this module turns it into a
//! measurement: how many frames were drawn, why each one happened, and how much
//! of the process's lifetime was spent rendering (docs/01 §3.5, §4 step 6).
//!
//! Everything here is arithmetic over `Duration`s handed in by the caller —
//! there is no clock and no smithay type in this file. That is deliberate and
//! follows the same rule as D-016: a measurement that needs a running
//! compositor to test is a measurement nobody runs. The backend owns the
//! `Instant`s and passes the differences in.
//!
//! Why a per-frame line and a closing report, and *not* the one-second log
//! §3.5 originally described: a timer firing every second is a wakeup every
//! second. On the phone that is battery and on an old CPU that is a fan (D-027),
//! and it would show up in `top` as CPU burned by the instrumentation itself —
//! the measurement disturbing what it measures. Zero frames now produces zero
//! lines, which is exactly the signal we are after.

use std::time::Duration;

/// Why a frame was drawn.
///
/// This is the field that actually catches a stray render loop. A count alone
/// says "47 frames"; the cause says "47 frames, all of them Redraw", which
/// names the bug.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cause {
    /// The first frame, drawn before any event: an unfilled window shows
    /// whatever happened to be in the buffer.
    Initial,
    /// The window changed size or scale.
    Resized,
    /// The host session asked for the window to be repainted.
    Redraw,
    /// The displayed minute stopped being true (step 5).
    Clock,
    /// A client changed something we draw: a window opened, closed, or renamed
    /// itself (M2). Kept apart from `Redraw` because "the shell drew because an
    /// application did something" and "the host asked us to repaint" fail in
    /// completely different ways — a client that redraws us in a loop is a bug
    /// this count names immediately.
    Client,
}

impl Cause {
    const COUNT: usize = 5;

    pub const ALL: [Cause; Self::COUNT] = [
        Cause::Initial,
        Cause::Resized,
        Cause::Redraw,
        Cause::Clock,
        Cause::Client,
    ];

    fn index(self) -> usize {
        match self {
            Cause::Initial => 0,
            Cause::Resized => 1,
            Cause::Redraw => 2,
            Cause::Clock => 3,
            Cause::Client => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Cause::Initial => "initial",
            Cause::Resized => "resized",
            Cause::Redraw => "redraw",
            Cause::Clock => "clock",
            Cause::Client => "client",
        }
    }
}

/// The environment variable that turns the per-frame log on.
const ENV: &str = "GOSTUI_STATS";

/// Frame counts and render timings for one run.
///
/// Counting happens whether or not the log is enabled — the counter is cheap
/// and `--idle-test` needs it regardless. `enabled` gates only the output.
#[derive(Debug)]
pub struct Stats {
    enabled: bool,
    frames: u64,
    by_cause: [u64; Cause::COUNT],
    render_total: Duration,
    render_min: Option<Duration>,
    render_max: Duration,
    /// The longest stretch between two frames. In a healthy idle shell this
    /// grows without bound; if it stays near zero, something is spinning.
    idle_max: Duration,
}

impl Stats {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            frames: 0,
            by_cause: [0; Cause::COUNT],
            render_total: Duration::ZERO,
            render_min: None,
            render_max: Duration::ZERO,
            idle_max: Duration::ZERO,
        }
    }

    /// Read `GOSTUI_STATS` from the environment.
    ///
    /// Anything set and not `0` counts as on, so `GOSTUI_STATS=1` from the docs
    /// works and so does a bare `GOSTUI_STATS=yes` from muscle memory.
    pub fn from_env() -> Self {
        Self::new(match std::env::var(ENV) {
            Ok(v) => !v.is_empty() && v != "0",
            Err(_) => false,
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// How many frames one particular cause accounts for.
    ///
    /// The idle criterion is built from this rather than from the total,
    /// because not every frame in an untouched run is a fault: the clock is
    /// *supposed* to redraw when the minute turns (step 5), so a run longer
    /// than a minute legitimately draws more than once. What must stay at zero
    /// is frames the host session asked for while nobody was touching it.
    pub fn count(&self, cause: Cause) -> u64 {
        self.by_cause[cause.index()]
    }

    /// Account for one frame.
    ///
    /// `render` is how long the frame took; `since_previous` is the gap since
    /// the last one, or `None` for the first frame of the run.
    pub fn record(&mut self, cause: Cause, render: Duration, since_previous: Option<Duration>) {
        self.frames += 1;
        self.by_cause[cause.index()] += 1;
        self.render_total += render;
        self.render_min = Some(match self.render_min {
            Some(min) if min <= render => min,
            _ => render,
        });
        if render > self.render_max {
            self.render_max = render;
        }
        if let Some(gap) = since_previous {
            if gap > self.idle_max {
                self.idle_max = gap;
            }
        }
    }

    /// The share of the run spent inside `draw`, as a percentage.
    ///
    /// This is the number the "zero rendering at rest" criterion reduces to: a
    /// shell that redraws only on events spends a rounding error here, and a
    /// shell with a loop in it does not.
    pub fn render_share(&self, uptime: Duration) -> f64 {
        if uptime.is_zero() {
            return 0.0;
        }
        100.0 * self.render_total.as_secs_f64() / uptime.as_secs_f64()
    }

    /// The closing summary, one line per topic.
    ///
    /// Returned as a string rather than logged directly so the formatting is
    /// testable — the whole point of keeping this module free of clocks.
    pub fn report(&self, uptime: Duration) -> String {
        if self.frames == 0 {
            return format!("GOSTUI_STATS — no frames drawn in {}", secs(uptime));
        }

        let causes: Vec<String> = Cause::ALL
            .iter()
            .filter(|c| self.by_cause[c.index()] > 0)
            .map(|c| format!("{} {}", c.label(), self.by_cause[c.index()]))
            .collect();

        let mean = self.render_total / self.frames as u32;
        let min = self.render_min.unwrap_or(Duration::ZERO);

        format!(
            "GOSTUI_STATS — {} frame(s) in {}\n  \
             render: min {} · mean {} · max {} · total {} ({:.4}% of uptime)\n  \
             causes: {}\n  \
             longest gap between frames: {}\n  \
             damage: full window every frame (partial damage is M2)",
            self.frames,
            secs(uptime),
            ms(min),
            ms(mean),
            ms(self.render_max),
            ms(self.render_total),
            self.render_share(uptime),
            causes.join(" · "),
            secs(self.idle_max),
        )
    }
}

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1000.0)
}

/// Seconds, or milliseconds when seconds would round to nothing.
///
/// A gap between frames of 18 ms printed as "0.0 s" reads as "no gap", which is
/// the opposite of what it says — and this figure is the whole evidence for
/// the shell being at rest.
fn secs(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        ms(d)
    } else {
        format!("{:.1} s", d.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_fresh_counter_has_drawn_nothing() {
        let stats = Stats::new(true);
        assert_eq!(stats.frames(), 0);
        assert_eq!(stats.render_share(Duration::from_secs(10)), 0.0);
        assert!(stats.report(Duration::from_secs(10)).contains("no frames"));
    }

    #[test]
    fn frames_are_counted_per_cause() {
        let mut stats = Stats::new(false);
        stats.record(Cause::Initial, millis(2), None);
        stats.record(Cause::Clock, millis(1), Some(Duration::from_secs(60)));
        stats.record(Cause::Clock, millis(1), Some(Duration::from_secs(60)));

        assert_eq!(stats.frames(), 3);
        let report = stats.report(Duration::from_secs(120));
        assert!(report.contains("initial 1"), "{report}");
        assert!(report.contains("clock 2"), "{report}");
        // A cause that never happened must not appear as a zero — noise in a
        // diagnostic is how a diagnostic stops being read.
        assert!(!report.contains("redraw"), "{report}");
    }

    #[test]
    fn counting_works_with_the_log_switched_off() {
        // `--idle-test` reads the counter regardless of GOSTUI_STATS, so the
        // two must not be wired together.
        let mut stats = Stats::new(false);
        stats.record(Cause::Redraw, millis(1), None);
        assert!(!stats.enabled());
        assert_eq!(stats.frames(), 1);
    }

    #[test]
    fn min_and_max_track_the_extremes() {
        let mut stats = Stats::new(true);
        stats.record(Cause::Initial, millis(5), None);
        stats.record(Cause::Redraw, millis(1), Some(millis(10)));
        stats.record(Cause::Redraw, millis(9), Some(millis(10)));

        let report = stats.report(Duration::from_secs(1));
        assert!(report.contains("min 1.00 ms"), "{report}");
        assert!(report.contains("max 9.00 ms"), "{report}");
        assert!(report.contains("mean 5.00 ms"), "{report}");
    }

    #[test]
    fn the_longest_gap_is_the_idle_evidence() {
        let mut stats = Stats::new(true);
        stats.record(Cause::Initial, millis(1), None);
        stats.record(Cause::Redraw, millis(1), Some(millis(200)));
        stats.record(Cause::Clock, millis(1), Some(Duration::from_secs(60)));

        assert!(stats.report(Duration::from_secs(61)).contains("60.0 s"));
    }

    #[test]
    fn a_sub_second_gap_is_reported_in_milliseconds() {
        // "0.0 s" reads as "no gap at all", which is the opposite of an 18 ms
        // one — and this is the number the idle evidence rests on.
        let mut stats = Stats::new(true);
        stats.record(Cause::Initial, millis(1), None);
        stats.record(Cause::Redraw, millis(1), Some(millis(18)));

        let report = stats.report(Duration::from_secs(1));
        assert!(
            report.contains("longest gap between frames: 18.00 ms"),
            "{report}"
        );
    }

    #[test]
    fn an_idle_shell_spends_a_rounding_error_on_rendering() {
        // One 2 ms frame in ten minutes. This is the shape of the number the
        // criterion is about; a render loop would put it near 100.
        let mut stats = Stats::new(true);
        stats.record(Cause::Initial, millis(2), None);

        let share = stats.render_share(Duration::from_secs(600));
        assert!(share < 0.001, "{share}");
        assert!(stats.report(Duration::from_secs(600)).contains("0.0003%"));
    }

    #[test]
    fn a_zero_length_run_does_not_divide_by_zero() {
        let mut stats = Stats::new(true);
        stats.record(Cause::Initial, millis(2), None);
        assert_eq!(stats.render_share(Duration::ZERO), 0.0);
    }

    #[test]
    fn every_cause_has_its_own_slot_and_label() {
        // Guards the hand-written `index()`: two causes sharing a slot would
        // silently merge their counts.
        let mut seen = std::collections::BTreeSet::new();
        for cause in Cause::ALL {
            assert!(seen.insert(cause.index()), "duplicate slot for {cause:?}");
            assert!(!cause.label().is_empty());
        }
        assert_eq!(seen.len(), Cause::ALL.len());
    }
}
