//! The three screen zones.
//!
//! The organising rule of the whole interface: the screen splits into three
//! **non-overlapping** zones, so the user never confuses system UI with
//! application UI.
//!
//! ```text
//! ┌──────────────────────────────┐
//! │  top bar — system only       │
//! ├──────────────────────────────┤
//! │  applications, or the tab    │
//! │  slider when nothing covers  │
//! │  it (D-003, Model A)         │
//! ├──────────────────────────────┤
//! │  bottom bar — open windows   │
//! └──────────────────────────────┘
//! ```
//!
//! Zones are computed per output (D-026), in logical units (D-011). The bars
//! never overlap the application area, which is why tiling can treat that area
//! as the whole world.

use crate::geometry::Rect;

/// Smallest comfortable touch target, in logical units.
///
/// Everything the finger must hit is at least this big (D-020). It is not a
/// style preference: below roughly this size a fingertip cannot reliably pick
/// one target over its neighbour, and the phone is the target platform.
pub const MIN_TOUCH_TARGET: i32 = 48;

/// Heights of the two bars, in logical units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarHeights {
    pub top: i32,
    pub bottom: i32,
}

impl Default for BarHeights {
    fn default() -> Self {
        Self {
            top: MIN_TOUCH_TARGET,
            bottom: MIN_TOUCH_TARGET,
        }
    }
}

/// The three zones of one output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zones {
    /// System only: Start Menu, search, clock, status.
    pub top_bar: Rect,
    /// Applications — and the tab slider underneath them.
    pub apps: Rect,
    /// Open-window switcher only.
    pub bottom_bar: Rect,
}

impl Zones {
    /// True when the three zones exactly tile `area` with no gap and no overlap.
    /// Used by tests; cheap enough to assert in debug builds too.
    pub fn covers(&self, area: Rect) -> bool {
        self.top_bar.y() == area.y()
            && self.apps.y() == self.top_bar.bottom()
            && self.bottom_bar.y() == self.apps.bottom()
            && self.bottom_bar.bottom() == area.bottom()
    }
}

/// Split an output's logical area into the three zones.
///
/// Bar heights are raised to [`MIN_TOUCH_TARGET`] and then, if the screen is too
/// short for both bars plus a usable application area, shrunk proportionally.
/// The application area is never negative — a pathological output produces thin
/// bars and an empty middle rather than a panic.
pub fn zones(area: Rect, bars: BarHeights) -> Zones {
    let mut top = bars.top.max(MIN_TOUCH_TARGET);
    let mut bottom = bars.bottom.max(MIN_TOUCH_TARGET);

    // Never let the bars eat more than half the screen: on a very short output
    // the application area matters more than a comfortable bar.
    let budget = area.h() / 2;
    if top + bottom > budget {
        let total = top + bottom;
        if total > 0 && budget > 0 {
            top = (top * budget / total).max(1);
            bottom = (budget - top).max(1);
        } else {
            top = 0;
            bottom = 0;
        }
    }

    let middle = (area.h() - top - bottom).max(0);
    Zones {
        top_bar: Rect::new(area.x(), area.y(), area.w(), top),
        apps: Rect::new(area.x(), area.y() + top, area.w(), middle),
        bottom_bar: Rect::new(area.x(), area.y() + top + middle, area.w(), bottom),
    }
}

/// Where the top bar's elements sit.
///
/// Every field is optional because the bar has to work on a 360-unit phone
/// screen as well as a 1920-unit monitor, and on the narrow one there is simply
/// no room for all four. Dropping an element is a decision the layout makes
/// explicitly; letting them overlap is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TopBarLayout {
    /// The Start Menu button. Never dropped — it is the anchor of the bar.
    pub menu: Option<Rect>,
    pub search: Option<Rect>,
    pub clock: Option<Rect>,
    pub status: Option<Rect>,
}

impl TopBarLayout {
    /// Every element that got a place, in left-to-right order.
    pub fn placed(&self) -> Vec<Rect> {
        [self.menu, self.search, self.clock, self.status]
            .into_iter()
            .flatten()
            .collect()
    }
}

const BAR_MARGIN: i32 = 12;
const BAR_GAP: i32 = 12;
const MENU_W: i32 = 132;
const CLOCK_W: i32 = 160;
const STATUS_W: i32 = 116;

/// One window's chip on the bottom bar.
const CHIP_W: i32 = 180;
const CHIP_H: i32 = 32;
const CHIP_GAP: i32 = 8;

/// Place the bottom bar's window chips, left to right.
///
/// The returned vector is **shorter than `count`** when the bar runs out of room.
/// That is a real limitation and not a rounding detail: a window whose chip did
/// not fit cannot be reached by pointer or finger at all. It is left this way on
/// purpose for now — the alternatives (scrolling the bar, shrinking chips below
/// a touch target, grouping by application) are decisions the specification has
/// not made, and quietly picking one here would be picking it for good.
///
/// This lives in core rather than in the painter for the same reason
/// [`top_bar_layout`] does: a click has to land on the chip that was drawn, and
/// two copies of the arithmetic are two answers waiting to disagree (D-016).
pub fn bottom_bar_layout(bar: Rect, count: usize) -> Vec<Rect> {
    if bar.w() <= 0 || bar.h() <= 0 {
        return Vec::new();
    }
    let h = CHIP_H.min(bar.h());
    let y = bar.y() + (bar.h() - h) / 2;
    let mut out = Vec::new();
    let mut x = bar.x() + BAR_MARGIN;
    for _ in 0..count {
        let chip = Rect::new(x, y, CHIP_W, h);
        if chip.right() > bar.right() - BAR_MARGIN {
            break;
        }
        out.push(chip);
        x += CHIP_W + CHIP_GAP;
    }
    out
}

/// Place the top bar's elements, dropping what does not fit.
///
/// Order of sacrifice, least useful first: search (reachable by keyboard and
/// from the menu), then the clock (the phone draws its own), then status.
/// The menu always stays.
pub fn top_bar_layout(bar: Rect) -> TopBarLayout {
    if bar.w() <= 0 || bar.h() <= 0 {
        return TopBarLayout::default();
    }
    let h = bar.h().clamp(1, MIN_TOUCH_TARGET);
    let y = bar.y() + (bar.h() - h) / 2;
    let chip = |x: i32, w: i32| Rect::new(x, y, w, h);

    let mut out = TopBarLayout::default();
    let left = bar.x() + BAR_MARGIN;
    let right = bar.right() - BAR_MARGIN;

    let menu_w = MENU_W.min(right - left);
    if menu_w <= 0 {
        return out;
    }
    out.menu = Some(chip(left, menu_w));
    let mut cursor = left + menu_w;

    // Status first, from the right, because it is anchored there.
    let status_x = right - STATUS_W;
    if status_x >= cursor + BAR_GAP {
        out.status = Some(chip(status_x, STATUS_W));
    }
    let right_edge = out.status.map_or(right, |s| s.x());

    // Clock, centred if it fits between the two groups without touching them.
    let clock_x = bar.x() + (bar.w() - CLOCK_W) / 2;
    if clock_x >= cursor + BAR_GAP && clock_x + CLOCK_W <= right_edge - BAR_GAP {
        out.clock = Some(chip(clock_x, CLOCK_W));
    }

    // Search last, tucked against the menu, only if it collides with nothing.
    let search_x = cursor + BAR_GAP;
    let search_limit = out.clock.map_or(right_edge, |c| c.x());
    if search_x + h <= search_limit - BAR_GAP {
        out.search = Some(chip(search_x, h));
        cursor = search_x + h;
    }
    let _ = cursor;

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;

    #[test]
    fn monitor_splits_into_three_stacked_zones() {
        let area = Rect::from_size(Size::new(1920, 1080));
        let z = zones(area, BarHeights::default());
        assert_eq!(z.top_bar, Rect::new(0, 0, 1920, 48));
        assert_eq!(z.apps, Rect::new(0, 48, 1920, 984));
        assert_eq!(z.bottom_bar, Rect::new(0, 1032, 1920, 48));
    }

    #[test]
    fn the_three_zones_tile_the_output_exactly() {
        // No gap, no overlap — the property the whole layout depends on.
        for size in [
            Size::new(1920, 1080),
            Size::new(360, 800),
            Size::new(640, 480),
        ] {
            let area = Rect::from_size(size);
            let z = zones(area, BarHeights::default());
            assert!(z.covers(area), "zones do not tile {size:?}");
            assert_eq!(z.top_bar.h() + z.apps.h() + z.bottom_bar.h(), size.h);
        }
    }

    #[test]
    fn bars_are_never_smaller_than_a_touch_target() {
        let area = Rect::from_size(Size::new(1920, 1080));
        let z = zones(area, BarHeights { top: 10, bottom: 4 });
        assert_eq!(z.top_bar.h(), MIN_TOUCH_TARGET);
        assert_eq!(z.bottom_bar.h(), MIN_TOUCH_TARGET);
    }

    #[test]
    fn bars_never_eat_more_than_half_the_screen() {
        let area = Rect::from_size(Size::new(800, 200));
        let z = zones(
            area,
            BarHeights {
                top: 300,
                bottom: 300,
            },
        );
        assert!(z.top_bar.h() + z.bottom_bar.h() <= 100);
        assert!(z.apps.h() > 0);
        assert!(z.covers(area));
    }

    #[test]
    fn a_degenerate_output_yields_no_negative_zone() {
        let area = Rect::from_size(Size::new(100, 1));
        let z = zones(area, BarHeights::default());
        assert!(z.top_bar.h() >= 0 && z.apps.h() >= 0 && z.bottom_bar.h() >= 0);
        assert!(z.covers(area));
    }

    #[test]
    fn top_bar_elements_never_overlap_at_any_width() {
        // The bug this test exists for: on a 360-unit phone bar the centred clock
        // sat on top of both neighbours, and the bar rendered as one solid block.
        for w in [200, 320, 360, 480, 720, 1080, 1920, 3840] {
            let bar = Rect::new(0, 0, w, MIN_TOUCH_TARGET);
            let placed = top_bar_layout(bar).placed();
            for pair in placed.windows(2) {
                assert!(
                    pair[0].right() <= pair[1].x(),
                    "overlap at width {w}: {:?} then {:?}",
                    pair[0],
                    pair[1]
                );
            }
            for r in &placed {
                assert!(
                    r.x() >= bar.x() && r.right() <= bar.right(),
                    "chip escapes bar at {w}"
                );
            }
        }
    }

    #[test]
    fn the_start_menu_survives_every_width_that_can_hold_anything() {
        for w in [160, 360, 1920] {
            let l = top_bar_layout(Rect::new(0, 0, w, MIN_TOUCH_TARGET));
            assert!(l.menu.is_some(), "menu dropped at width {w}");
        }
    }

    #[test]
    fn a_wide_bar_gets_everything_and_a_narrow_one_does_not() {
        let wide = top_bar_layout(Rect::new(0, 0, 1920, MIN_TOUCH_TARGET));
        assert_eq!(wide.placed().len(), 4);

        let phone = top_bar_layout(Rect::new(0, 0, 360, MIN_TOUCH_TARGET));
        assert!(
            phone.placed().len() < 4,
            "a 360-unit bar cannot hold all four"
        );
    }

    #[test]
    fn a_bar_narrower_than_its_own_margins_places_nothing() {
        assert_eq!(
            top_bar_layout(Rect::new(0, 0, 0, 0)),
            TopBarLayout::default()
        );
        // 10 units cannot hold two 12-unit margins, so there is nowhere to put
        // even the menu. Drawing nothing beats drawing something off the edge.
        assert!(top_bar_layout(Rect::new(0, 0, 10, 48)).placed().is_empty());
        // 100 units fits a shrunken menu and nothing else.
        assert_eq!(top_bar_layout(Rect::new(0, 0, 100, 48)).placed().len(), 1);
    }

    #[test]
    fn window_chips_sit_inside_the_bottom_bar_and_never_overlap() {
        let bar = Rect::new(0, 1032, 1920, 48);
        let chips = bottom_bar_layout(bar, 4);
        assert_eq!(chips.len(), 4);
        for pair in chips.windows(2) {
            assert!(pair[0].right() < pair[1].x());
        }
        for c in &chips {
            assert!(c.y() >= bar.y() && c.bottom() <= bar.bottom());
            assert!(c.right() <= bar.right());
        }
    }

    #[test]
    fn a_bar_too_narrow_for_every_chip_drops_the_ones_that_do_not_fit() {
        // The phone case, and the reason the caller must not assume it got back
        // as many rectangles as it asked for.
        let bar = Rect::new(0, 0, 360, MIN_TOUCH_TARGET);
        let chips = bottom_bar_layout(bar, 6);
        assert!(chips.len() < 6);
        for c in &chips {
            assert!(c.right() <= bar.right() - 12);
        }
    }

    #[test]
    fn chips_of_a_degenerate_bar_are_empty_rather_than_negative() {
        assert!(bottom_bar_layout(Rect::new(0, 0, 0, 0), 3).is_empty());
        for c in bottom_bar_layout(Rect::new(0, 0, 1920, 4), 3) {
            assert!(c.h() > 0 && c.h() <= 4);
        }
    }

    #[test]
    fn zones_are_positioned_relative_to_the_output_not_the_origin() {
        // A second output in a dock does not start at (0, 0).
        let area = Rect::new(360, 0, 1920, 1080);
        let z = zones(area, BarHeights::default());
        assert_eq!(z.top_bar.x(), 360);
        assert_eq!(z.apps.x(), 360);
        assert!(z.covers(area));
    }
}
