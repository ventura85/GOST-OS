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
use crate::theme::Metrics;

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
/// The Start Menu button is square: it holds an icon and no text, and a wide
/// button around a small mark reads as a coloured block rather than as a thing
/// to press. Square also makes it exactly a touch target (D-020) at bar height.
const MENU_W: i32 = MIN_TOUCH_TARGET;
const CLOCK_W: i32 = 160;
const STATUS_W: i32 = 116;

/// One window's chip on the bottom bar.
const CHIP_W: i32 = 180;
const CHIP_H: i32 = 32;
const CHIP_GAP: i32 = 8;

/// The Start Menu icon: four squares in a 2×2 grid, the whole thing this wide.
const MENU_ICON_SIDE: i32 = 24;
/// The cross of background showing between the four squares.
const MENU_ICON_GAP: i32 = 4;

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

/// Where the card columns of the middle zone go.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardLayout {
    /// One rectangle per drawn card, in strip order, starting at [`first`].
    ///
    /// Every card is the **same width**, including the ones at the ends, and a
    /// card that does not fit **hangs off the edge** — its rectangle starts
    /// before the zone or ends after it. That overhang is the specification's
    /// "sliver of the neighbouring card", and keeping it as a whole rectangle is
    /// what makes it one: the tiles inside it are placed by the same arithmetic
    /// as every other card's and then cut by the rasteriser, so the sliver shows
    /// part of a card rather than an empty column.
    ///
    /// Narrowing the rectangle instead — which is what this did until the strip
    /// was centred — hands [`layout_tiles`] a box too small for a single tile
    /// column, and it answers correctly for the box it was given: no tiles at
    /// all. The card was not being clipped, it was being **re-laid-out**.
    ///
    /// [`first`]: CardLayout::first
    pub cards: Vec<Rect>,
    /// Index into the tab strip of the first drawn card.
    pub first: usize,
    /// The `[+] Nowa karta` slot, when any of it is on screen.
    ///
    /// It is **the last column of the strip**, the same width and height as a
    /// card, and it takes part in the strip's arithmetic like one: the offset
    /// that centres the active card and the clamp that stops the strip at its
    /// ends both count it. A button of its own size beside the strip would be a
    /// second case in [`card_columns`], in `hit_test`, and in the offset — three
    /// places to keep agreeing about one rectangle.
    ///
    /// Never the active card and never reachable with `Super+←/→`: it makes a
    /// card, it is not one.
    pub add: Option<Rect>,
}

/// Lay the middle zone out as card columns, as many as the output has room for.
///
/// A card is a **column of fixed width** running the full height of the zone,
/// not a page that fills the screen one at a time (D-046). How many are visible
/// therefore follows from the width of the output — seven on a 1920 monitor, two
/// and a sliver on a phone held sideways, one on a phone held upright — while a
/// single card stays the same size everywhere. That is the property worth
/// keeping: a card is one object, not one object per form factor.
///
/// **The active card sits in the middle of the zone**, so the strip reads the
/// same on every output: what you are looking at is centred, and what is on
/// either side of it is visible as a sliver. On a phone held upright that is the
/// whole interaction — one card and a hint of its neighbours — and it is
/// `gostos.md` §B taken literally, which says the neighbouring cards show *at
/// the sides*, plural.
///
/// **Except at the ends, where the strip is clamped.** The first card does not
/// float into the middle leaving nothing to its left: empty space where the eye
/// expects a card reads as a fault, not as a layout, and a shell that puts
/// density above room (D-044) has no use for it. So the strip stops when its
/// edge reaches the zone's edge, exactly as it did before it was centred, and
/// `Super+←` on the first card still moves nothing and draws no frame (D-007).
///
/// **When every card fits, the whole strip is centred instead.** Four cards on a
/// 1920 monitor leave 844 units over; spent on one side they are a hole, split
/// between the two they are a margin.
///
/// `first` is **derived from `active`, never stored**. Scrolling the strip has
/// exactly one cause — the focused card has to be on screen — so keeping it as
/// state would be keeping a second answer to a question that already has one,
/// and `Super+←/→` (D-007) would have to remember to update it.
///
/// Lives in core, like [`bottom_bar_layout`] and [`top_bar_layout`], because a
/// click has to land on the card that was drawn (D-016).
pub fn card_columns(area: Rect, m: &Metrics, count: usize, active: usize) -> CardLayout {
    if area.w() <= 0 || area.h() <= 0 {
        return CardLayout::default();
    }
    // The `[+]` slot is one more column, so a strip of no cards is still a strip
    // of one thing — which is the state a fresh session starts in and the one
    // where being told how to make a card is worth most.
    let slots = count.saturating_add(1);
    // An output narrower than one card gets one card as wide as the output,
    // rather than a column hanging off both edges at once and never showing a
    // whole one.
    let width = m.card_width.min(area.w());
    let step = width + m.card_gap;

    // Saturating, because the strip length is the one place where a tab count
    // multiplies: a caller with an absurd number of cards should get a wrong
    // picture, not a wrapped one.
    let n = i32::try_from(slots).unwrap_or(i32::MAX);
    let strip = n.saturating_mul(step).saturating_sub(m.card_gap);

    // How far the strip has slid left, in units. Positive means scrolled;
    // negative means it is narrower than the zone and has been centred in it.
    let offset = if strip <= area.w() {
        -(area.w() - strip) / 2
    } else {
        let centred = i32::try_from(active)
            .unwrap_or(i32::MAX)
            .saturating_mul(step)
            .saturating_add(width / 2 - area.w() / 2);
        centred.clamp(0, strip - area.w())
    };

    // Start from the last card that could still reach into the zone from the
    // left, so a strip of a thousand cards costs the same as a strip of ten.
    let start = if offset > width {
        ((offset - width) / step) as usize
    } else {
        0
    };

    let mut cards = Vec::new();
    let mut add = None;
    let mut first = start;
    for i in start..slots {
        let x = area.x() + (i as i32).saturating_mul(step) - offset;
        if x + width <= area.x() {
            // Entirely past the left edge — `start` overshoots by at most one.
            first = i + 1;
            continue;
        }
        if x >= area.right() {
            break;
        }
        // Full width even when it hangs off: the sliver is a card seen partly,
        // and the rasteriser is what cuts it. See [`CardLayout::cards`].
        let slot = Rect::new(x, area.y(), width, area.h());
        if i == count {
            add = Some(slot);
        } else {
            cards.push(slot);
        }
    }
    // `first` counts cards, so an empty run of them past the left edge must not
    // leave it pointing at the `[+]` slot, which is not a card index.
    CardLayout {
        first: first.min(count),
        cards,
        add,
    }
}

/// Place the tiles inside one card column.
///
/// The number of columns is **not configured**: it follows from the width left
/// after the padding, the same way [`bottom_bar_layout`] and
/// [`crate::layout::tile_limit`] derive their counts from the room available.
/// With the default metrics that comes to two — but nothing in the code says
/// "two", so narrowing a card in the theme produces one column instead of a
/// contradiction between two numbers that were both meant to be true.
///
/// Tiles that would fall below the card are dropped, not shrunk.
pub fn layout_tiles(card: Rect, m: &Metrics, count: usize) -> Vec<Rect> {
    let inner_w = card.w() - 2 * m.card_pad;
    let bottom = card.bottom() - m.card_pad;
    if inner_w < m.tile_unit || card.h() <= 0 {
        return Vec::new();
    }
    let cols = ((inner_w + m.tile_gap) / (m.tile_unit + m.tile_gap)).max(1);
    let x0 = card.x() + m.card_pad;
    let y0 = card.y() + m.card_header + m.card_pad;

    let mut out = Vec::new();
    for n in 0..count as i32 {
        let tile = Rect::new(
            x0 + (n % cols) * (m.tile_unit + m.tile_gap),
            y0 + (n / cols) * (m.tile_unit + m.tile_gap),
            m.tile_unit,
            m.tile_unit,
        );
        if tile.bottom() > bottom {
            break;
        }
        out.push(tile);
    }
    out
}

/// The two halves of a dead tile: the mark, and the name under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileFace {
    /// Square left for the icon, centred in what the caption did not take.
    pub icon: Rect,
    /// Strip along the bottom of the tile for the caption, or `None` when the
    /// tile is too small to hold a name and a mark both.
    pub caption: Option<Rect>,
}

/// Split a tile into the icon square and the caption strip below it.
///
/// A dead tile is a shortcut: an icon and a name (D-033). Both live **inside**
/// the tile rather than the name hanging below it, which is what keeps the grid
/// a grid — a caption outside the square would make the row height depend on
/// whether anything in that row had a name.
///
/// The caption is dropped, not shrunk, when the tile is small: a two-line-tall
/// strip of half-height glyphs is not a name, it is a smudge that costs the icon
/// its room. Same rule as the top bar, which drops elements rather than
/// overlapping them, and as [`layout_tiles`], which drops tiles rather than
/// squeezing them.
///
/// `line` is the height one line of caption needs, which the caller derives from
/// the font size — core does not know what a font is (D-016), only that
/// something asked it to reserve that many units.
pub fn tile_face(tile: Rect, line: i32) -> TileFace {
    let line = line.max(0);
    // The icon needs to stay square and big enough to read: below half the tile
    // it is a dot with a label, not a shortcut.
    let fits = line > 0 && tile.h() - line >= tile.h() / 2 && tile.w() > 0;
    if !fits {
        return TileFace {
            icon: tile,
            caption: None,
        };
    }
    let icon_h = tile.h() - line;
    let side = icon_h.min(tile.w());
    // A caption running edge to edge reads as text the tile cut off by accident,
    // which is exactly what an ellipsis is supposed to say on purpose. The inset
    // is small — this is a dense interface (D-044), not an airy one — but it has
    // to be there, and it shrinks rather than disappears on a tiny tile.
    let pad = CAPTION_PAD.min(tile.w() / 8);
    TileFace {
        icon: Rect::new(
            tile.x() + (tile.w() - side) / 2,
            tile.y() + (icon_h - side) / 2,
            side,
            side,
        ),
        caption: Some(Rect::new(
            tile.x() + pad,
            tile.bottom() - line,
            tile.w() - 2 * pad,
            line,
        )),
    }
}

/// Side of the plus mark on the `[+]` slot, and the thickness of its bars.
const PLUS_SIDE: i32 = 40;
const PLUS_BAR: i32 = 8;

/// The plus on the `[+] Nowa karta` slot: two crossed bars, or `None` when the
/// slot is too small to hold the mark whole.
///
/// **Two rectangles, not a glyph.** The same reasoning as [`menu_icon`]'s four
/// squares: a mark made of fills needs no font, no texture and no cache, both
/// renderer paths execute it identically, and the golden images keep their
/// property of containing no text at all. A `+` set in the UI font would cost
/// all three for a shape that is two rectangles.
///
/// Placed in the **upper third** of the slot, where a card keeps its tiles — the
/// eye and then the finger go there (measured for D-046), and a mark centred in
/// a full-height column would sit halfway down an empty panel.
pub fn plus_mark(slot: Rect, m: &Metrics) -> Option<[Rect; 2]> {
    let head = card_header(slot, m);
    let body = Rect::new(
        slot.x(),
        head.bottom(),
        slot.w(),
        slot.bottom() - head.bottom(),
    );
    if body.w() < PLUS_SIDE || body.h() < PLUS_SIDE || PLUS_BAR <= 0 {
        return None;
    }
    let x = body.x() + (body.w() - PLUS_SIDE) / 2;
    // A third of the way down rather than halfway, and measured from the body so
    // the header does not push the mark off centre with respect to the tiles a
    // real card would show beside it.
    let y = body.y() + (body.h() / 3 - PLUS_SIDE / 2).max(0);
    let off = (PLUS_SIDE - PLUS_BAR) / 2;
    Some([
        Rect::new(x, y + off, PLUS_SIDE, PLUS_BAR),
        Rect::new(x + off, y, PLUS_BAR, PLUS_SIDE),
    ])
}

/// Breathing room on each side of text set inside a box, in logical units.
///
/// Shared by the tile caption and the card's header for one reason: text that
/// runs edge to edge is indistinguishable from text the box cut off by
/// accident, and saying "there is more of this name" on purpose is the whole
/// job of the ellipsis.
const CAPTION_PAD: i32 = 4;

/// The bar across the top of a card, where its name and its function icons go.
///
/// Full width and full [`Metrics::card_header`] height, including on a card
/// that hangs off the zone (D-047) — the rasteriser cuts it, so a sliver shows
/// part of a header rather than a header of its own size.
pub fn card_header(card: Rect, m: &Metrics) -> Rect {
    Rect::new(card.x(), card.y(), card.w(), m.card_header.max(0))
}

/// Where a card's name is set inside its header, or `None` when the header is
/// too short to hold a line of it.
///
/// Dropped rather than shrunk, which is the rule the whole shell already
/// follows: [`top_bar_layout`] drops elements, [`layout_tiles`] drops tiles and
/// [`tile_face`] drops the caption. Half-height glyphs are not a smaller name,
/// they are a smudge.
///
/// The line is **centred vertically** in the header rather than sat on its
/// baseline: the header is a touch target 48 units tall (D-020) holding one
/// short word, and text pinned to its top edge in a box that size reads as
/// having slipped.
///
/// `line` comes from the caller for the same reason it does in [`tile_face`] —
/// core does not know what a font is (D-016), only that something asked it to
/// reserve that many units.
pub fn card_title(card: Rect, m: &Metrics, line: i32) -> Option<Rect> {
    let header = card_header(card, m);
    if line <= 0 || line > header.h() || header.w() <= 0 {
        return None;
    }
    let pad = CAPTION_PAD.min(header.w() / 8);
    let w = header.w() - 2 * pad;
    if w <= 0 {
        return None;
    }
    Some(Rect::new(
        header.x() + pad,
        header.y() + (header.h() - line) / 2,
        w,
        line,
    ))
}

/// Place the top bar's elements, dropping what does not fit.
///
/// The menu always stays; everything else goes when the room runs out. What
/// goes **first** is decided by width rather than by a ranking, and the
/// difference matters enough to write down, because this comment used to claim
/// a ranking the code does not implement (found by the golden images, which
/// draw a 420-unit bar and showed the clock gone while search stayed).
///
/// Status is anchored right and placed first. The clock wants 160 units in the
/// middle and loses them to any bar narrow enough that the middle overlaps
/// either group. Search asks for a square beside the menu, so it survives
/// widths the clock cannot — which is the behaviour we want on a phone (the
/// clock is drawn by the phone's own bar, search by touch is not), but it is a
/// consequence of the arithmetic and not a rule stated anywhere.
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

/// The four squares of the Start Menu icon, inside the chip the layout gave it.
///
/// Four squares in a 2×2 grid, in reading order: top-left, top-right,
/// bottom-left, bottom-right. Returned in **logical units**, like every other
/// rectangle here — the scale is applied when rasterising (D-011).
///
/// # Why this is in core
///
/// It is arithmetic, and arithmetic that two places would have to agree on: the
/// renderer fills these rectangles and nothing else may re-derive them. The same
/// rule that keeps `top_bar_layout` here (D-016) — the renderer is handed
/// rectangles, it does not compute them.
///
/// `None` when the chip is too small to hold the icon whole. A half-drawn icon
/// is worse than a plain accented button, and dropping what does not fit is
/// what the rest of the bar does too.
pub fn menu_icon(chip: Rect) -> Option<[Rect; 4]> {
    if chip.w() < MENU_ICON_SIDE || chip.h() < MENU_ICON_SIDE {
        return None;
    }
    // An odd gap would make one square wider than its neighbour; the icon is
    // symmetrical or it is not this icon.
    let square = (MENU_ICON_SIDE - MENU_ICON_GAP) / 2;
    if square <= 0 {
        return None;
    }
    // Centred in the chip. The icon is the whole content of the button, so
    // anchoring it to an edge would only make the button look misprinted.
    let x = chip.x() + (chip.w() - MENU_ICON_SIDE) / 2;
    let y = chip.y() + (chip.h() - MENU_ICON_SIDE) / 2;
    let far = square + MENU_ICON_GAP;
    Some([
        Rect::new(x, y, square, square),
        Rect::new(x + far, y, square, square),
        Rect::new(x, y + far, square, square),
        Rect::new(x + far, y + far, square, square),
    ])
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

    #[test]
    fn the_menu_button_is_square_and_a_touch_target() {
        // It holds an icon and no text, so its width is not free-floating: a
        // wide button around a small mark reads as a coloured block. And every
        // control has to be reachable with a finger (D-020), which at bar height
        // makes square and "big enough" the same requirement.
        let bar = Rect::from_size(Size::new(1920, 48));
        let menu = top_bar_layout(bar).menu.expect("never dropped");
        assert_eq!(menu.w(), menu.h(), "square");
        assert!(menu.w() >= MIN_TOUCH_TARGET, "reachable with a finger");
    }

    #[test]
    fn the_menu_icon_is_four_equal_squares_in_a_square_grid() {
        let bar = Rect::from_size(Size::new(1920, 48));
        let chip = top_bar_layout(bar).menu.expect("the menu is never dropped");
        let squares = menu_icon(chip).expect("a full-size bar has room for it");

        for s in &squares {
            assert_eq!(s.w(), s.h(), "square, not merely rectangular");
            assert_eq!(s.w(), squares[0].w(), "all four the same size");
        }
        // Reading order, and a real gap between the columns and the rows.
        let [tl, tr, bl, br] = squares;
        assert_eq!(tl.y(), tr.y());
        assert_eq!(bl.y(), br.y());
        assert_eq!(tl.x(), bl.x());
        assert_eq!(tr.x(), br.x());
        assert!(tr.x() > tl.right(), "a gap, not two squares touching");
        assert!(bl.y() > tl.bottom());
    }

    #[test]
    fn the_menu_icon_stays_inside_its_chip_and_centred() {
        let bar = Rect::new(360, 0, 1920, 48);
        let chip = top_bar_layout(bar).menu.expect("the menu is never dropped");
        let squares = menu_icon(chip).expect("room for it");

        for s in &squares {
            assert!(
                s.x() >= chip.x()
                    && s.y() >= chip.y()
                    && s.right() <= chip.right()
                    && s.bottom() <= chip.bottom(),
                "the icon never leaves the button it belongs to"
            );
        }
        // Symmetrical margins: the same slack on the left as on the right, so
        // the button does not read as misprinted. The bar is offset here on
        // purpose — an icon centred on the origin instead of on its chip would
        // pass at (0, 0) and fail on a docked second output.
        let left = squares[0].x() - chip.x();
        let right = chip.right() - squares[1].right();
        assert!((left - right).abs() <= 1, "left {left}, right {right}");
        let top = squares[0].y() - chip.y();
        let bottom = chip.bottom() - squares[2].bottom();
        assert!((top - bottom).abs() <= 1, "top {top}, bottom {bottom}");
    }

    #[test]
    fn a_chip_too_small_for_the_icon_gets_none_rather_than_a_broken_one() {
        // The phone in portrait, or a bar squeezed by a tiny output: the button
        // stays (it is never dropped), the icon inside it does not get drawn
        // half-size.
        assert!(menu_icon(Rect::new(0, 0, 12, 48)).is_none());
        assert!(menu_icon(Rect::new(0, 0, 132, 8)).is_none());
        assert!(menu_icon(Rect::new(0, 0, 0, 0)).is_none());
    }

    /// The application zone of each output, as `zones` cuts it with default bars.
    fn apps(w: i32, h: i32) -> Rect {
        zones(Rect::new(0, 0, w, h), BarHeights::default()).apps
    }

    #[test]
    fn a_monitor_shows_seven_cards_and_a_landscape_phone_two() {
        // The number nobody configures: it falls out of the width, which is the
        // whole point of fixing the card and deriving the count (D-046).
        let m = Metrics::default();
        assert_eq!(card_columns(apps(1920, 1080), &m, 7, 0).cards.len(), 7);
        assert_eq!(card_columns(apps(780, 360), &m, 7, 0).cards.len(), 3);
        assert_eq!(card_columns(apps(360, 780), &m, 7, 0).cards.len(), 2);
    }

    #[test]
    fn the_card_that_does_not_fit_hangs_off_the_edge_and_that_is_the_sliver() {
        // `gostos.md` §B wants neighbouring cards partly visible. Nothing here
        // draws a sliver: it is a whole card whose rectangle leaves the zone,
        // and the rasteriser is what cuts it.
        let m = Metrics::default();
        let area = apps(780, 360);
        let l = card_columns(area, &m, 7, 0);
        assert!(l.cards.iter().all(|c| c.w() == m.card_width));
        let last = *l.cards.last().unwrap();
        assert!(last.x() < area.right() && last.right() > area.right());

        // The point of keeping it whole: the sliver shows part of a card. Narrow
        // the rectangle to the visible strip instead and `layout_tiles` answers
        // for the box it was handed — no tiles at all — so the sliver becomes an
        // empty column that hints at nothing.
        assert_eq!(
            layout_tiles(last, &m, 6).len(),
            layout_tiles(l.cards[0], &m, 6).len()
        );
    }

    #[test]
    fn every_column_is_full_size_even_when_it_hangs_off_the_zone() {
        let m = Metrics::default();
        for (w, h) in [(1920, 1080), (780, 360), (360, 780)] {
            let area = apps(w, h);
            for card in card_columns(area, &m, 9, 0).cards {
                assert_eq!((card.y(), card.h()), (area.y(), area.h()));
                assert_eq!(card.w(), m.card_width.min(area.w()));
                // Hanging off is allowed; being invisible is not. A card nobody
                // can see is one the painter and the hit test both walk for
                // nothing.
                assert!(card.right() > area.x() && card.x() < area.right());
            }
        }
    }

    #[test]
    fn the_active_card_sits_in_the_middle_and_the_strip_stops_at_the_ends() {
        let m = Metrics::default();
        let area = apps(1920, 1080);
        let offset_from_centre = |l: &CardLayout, active: usize| {
            let c = l.cards[active - l.first];
            c.x() + c.w() / 2 - (area.x() + area.w() / 2)
        };

        // Mid-strip: the card you are looking at is the one in the middle.
        let mid = card_columns(area, &m, 40, 20);
        assert!(offset_from_centre(&mid, 20).abs() <= 1);

        // At the ends the strip is clamped instead, so there is never empty
        // space where the eye expects a card.
        let head = card_columns(area, &m, 40, 0);
        assert_eq!(head.first, 0);
        assert_eq!(head.cards[0].x(), area.x());
        // The strip ends with the `[+]` slot, not with the last card, so that is
        // what the right-hand clamp brings to the edge.
        let tail = card_columns(area, &m, 40, 39);
        assert_eq!(tail.add.expect("the slot").right(), area.right());

        // And coming back releases it again — the offset is derived from
        // `active`, not remembered.
        assert_eq!(card_columns(area, &m, 40, 0), head);
    }

    #[test]
    fn a_strip_narrower_than_the_zone_is_centred_rather_than_left_aligned() {
        // Four cards on a monitor leave 844 units over. Spent on one side they
        // are a hole; split between the two they are a margin.
        let m = Metrics::default();
        let area = apps(1920, 1080);
        let l = card_columns(area, &m, 4, 1);
        assert_eq!(l.cards.len(), 4);
        // Measured across the whole strip, `[+]` slot included — it is the last
        // column, so it is part of what gets centred, not something beside it.
        let (left, right) = (
            l.cards[0].x() - area.x(),
            area.right() - l.add.expect("the slot").right(),
        );
        assert!(left > 0 && (left - right).abs() <= 1, "{left} vs {right}");

        // Nothing is off screen, so nothing scrolls: changing the active card
        // moves the frame, not the strip.
        for active in 0..4 {
            assert_eq!(card_columns(area, &m, 4, active).cards, l.cards);
        }
    }

    #[test]
    fn the_new_card_slot_is_the_last_column_of_the_strip() {
        let m = Metrics::default();
        let area = apps(1920, 1080);
        let l = card_columns(area, &m, 3, 0);
        let slot = l.add.expect("the slot");
        let last = *l.cards.last().expect("three cards");
        // One step past the last card, same size — it is a column, not a button
        // parked beside the strip.
        assert_eq!(slot.x() - last.x(), m.card_width + m.card_gap);
        assert_eq!((slot.w(), slot.h()), (last.w(), last.h()));
    }

    #[test]
    fn a_strip_with_no_cards_is_still_a_strip_of_one_thing() {
        // The state a fresh session starts in, and the one where being shown how
        // to make a card is worth most. Before the slot this drew nothing at all.
        let m = Metrics::default();
        let area = apps(1920, 1080);
        let l = card_columns(area, &m, 0, 0);
        assert!(l.cards.is_empty());
        let slot = l.add.expect("the slot");
        assert_eq!(slot.x() - area.x(), area.right() - slot.right());
    }

    #[test]
    fn the_slot_is_counted_when_the_strip_is_measured() {
        // The reason it is a column rather than a special case: one more column
        // is all the centring and the clamp need to know about it. A strip of
        // cards that just fits stops fitting once the slot is there.
        let m = Metrics::default();
        let step = m.card_width + m.card_gap;
        let area = apps(7 * step - m.card_gap, 1080);
        let l = card_columns(area, &m, 7, 0);
        assert_eq!(l.cards.len(), 7);
        assert_eq!(l.add, None, "the slot is past the edge until you scroll");

        // And the end of the strip is the slot, not the last card: activating
        // the last card scrolls far enough to bring it to the edge.
        let end = card_columns(area, &m, 7, 6);
        assert_eq!(end.add.expect("the slot").right(), area.right());
    }

    #[test]
    fn the_plus_is_two_crossed_bars_centred_in_the_slot() {
        let m = Metrics::default();
        let slot = card_columns(apps(1920, 1080), &m, 0, 0)
            .add
            .expect("the slot");
        let [across, down] = plus_mark(slot, &m).expect("room for the mark");
        // Same centre, crossed, and inside the slot rather than over its header.
        assert_eq!(across.x() + across.w() / 2, down.x() + down.w() / 2);
        assert_eq!(across.y() + across.h() / 2, down.y() + down.h() / 2);
        assert_eq!(across.w(), down.h());
        assert_eq!(across.h(), down.w());
        assert!(across.x() + across.w() / 2 - slot.x() == slot.w() / 2);
        assert!(down.y() >= card_header(slot, &m).bottom());

        // Dropped whole rather than drawn cramped, like every other mark here.
        assert_eq!(plus_mark(Rect::new(0, 0, 20, 400), &m), None);
        assert_eq!(plus_mark(Rect::new(0, 0, 0, 0), &m), None);
    }

    #[test]
    fn on_a_phone_held_upright_the_active_card_has_a_sliver_on_each_side() {
        // The case the centring is for. One card fits, so left-aligning the
        // strip shows a neighbour on the right and nothing on the left, while
        // `gostos.md` §B asks for both sides.
        let m = Metrics::default();
        let area = apps(360, 780);
        let l = card_columns(area, &m, 3, 1);
        assert_eq!((l.first, l.cards.len()), (0, 3));
        assert!(l.cards[0].x() < area.x());
        assert!(l.cards[2].right() > area.right());
        let active = l.cards[1];
        assert_eq!(active.x() - area.x(), area.right() - active.right());
    }

    #[test]
    fn an_output_narrower_than_a_card_gets_one_card_as_wide_as_it_is() {
        let m = Metrics::default();
        let area = Rect::new(0, 0, 140, 400);
        let l = card_columns(area, &m, 5, 0);
        assert_eq!(l.cards.len(), 1);
        assert_eq!(l.cards[0].w(), 140);
    }

    #[test]
    fn no_cards_and_no_room_produce_nothing_rather_than_a_panic() {
        let m = Metrics::default();
        assert!(card_columns(apps(1920, 1080), &m, 0, 0).cards.is_empty());
        assert!(card_columns(Rect::new(0, 0, 0, 0), &m, 4, 0)
            .cards
            .is_empty());
        assert!(card_columns(Rect::new(0, 0, 800, -10), &m, 4, 0)
            .cards
            .is_empty());
    }

    #[test]
    fn tile_columns_follow_from_the_width_and_are_never_written_down() {
        let m = Metrics::default();
        let card = card_columns(apps(1920, 1080), &m, 3, 0).cards[0];
        let tiles = layout_tiles(card, &m, 6);
        // Two columns with the default metrics: the tiles come in pairs by row.
        assert_eq!(tiles[0].y(), tiles[1].y());
        assert_ne!(tiles[1].y(), tiles[2].y());
        assert_eq!(tiles[0].x(), tiles[2].x());

        // Narrow the card and the second column goes, rather than overflowing.
        let narrow = Metrics {
            card_width: 140,
            ..m
        };
        let card = card_columns(apps(1920, 1080), &narrow, 3, 0).cards[0];
        let tiles = layout_tiles(card, &narrow, 4);
        assert!(tiles.windows(2).all(|p| p[0].x() == p[1].x()));
    }

    #[test]
    fn tiles_stay_inside_their_card_and_run_out_rather_than_overflow() {
        let m = Metrics::default();
        for (w, h) in [(1920, 1080), (780, 360), (360, 780)] {
            let card = card_columns(apps(w, h), &m, 3, 0).cards[0];
            let tiles = layout_tiles(card, &m, 64);
            assert!(tiles.len() < 64, "64 tiles cannot fit any of these cards");
            for t in tiles {
                assert!(t.x() >= card.x() && t.right() <= card.right());
                assert!(t.y() >= card.y() + m.card_header && t.bottom() <= card.bottom());
            }
        }
    }

    #[test]
    fn a_tile_keeps_its_icon_square_and_its_caption_at_the_bottom() {
        let m = Metrics::default();
        let card = card_columns(apps(1920, 1080), &m, 3, 0).cards[0];
        let tile = layout_tiles(card, &m, 1)[0];
        let face = tile_face(tile, 18);
        let caption = face.caption.expect("a 96-unit tile has room for a name");

        assert_eq!(face.icon.w(), face.icon.h(), "square, or it is not an icon");
        assert_eq!(caption.bottom(), tile.bottom(), "the name sits at the foot");
        // Inset, not flush: text touching the edge reads as accidentally cut,
        // which is the one thing the ellipsis is there to say deliberately.
        assert!(caption.x() > tile.x() && caption.right() < tile.right());
        assert_eq!(
            caption.x() - tile.x(),
            tile.right() - caption.right(),
            "the same margin on both sides"
        );
        assert!(
            face.icon.bottom() <= caption.y(),
            "the mark and the name must not overlap: {:?} into {caption:?}",
            face.icon
        );
        for r in [face.icon, caption] {
            assert!(
                r.x() >= tile.x()
                    && r.y() >= tile.y()
                    && r.right() <= tile.right()
                    && r.bottom() <= tile.bottom(),
                "{r:?} leaves {tile:?}"
            );
        }
    }

    #[test]
    fn the_card_name_is_centred_in_the_header_and_never_touches_its_edges() {
        let m = Metrics::default();
        let card = card_columns(apps(1920, 1080), &m, 3, 0).cards[0];
        let header = card_header(card, &m);
        assert_eq!((header.x(), header.y()), (card.x(), card.y()));
        assert_eq!((header.w(), header.h()), (card.w(), m.card_header));

        let title = card_title(card, &m, 18).expect("a 48-unit header holds a line");
        // Room on both sides, so a name cut by the text stack reads as cut on
        // purpose — the lesson the tile caption already paid for.
        assert!(title.x() > header.x());
        assert_eq!(title.x() - header.x(), header.right() - title.right());
        // And centred vertically rather than sat on the header's top edge.
        assert_eq!(title.y() - header.y(), header.bottom() - title.bottom());
        assert!(title.h() == 18 && title.bottom() <= header.bottom());
    }

    #[test]
    fn a_header_too_short_for_a_line_drops_the_name_rather_than_shrinking_it() {
        // Same rule as `tile_face`, `layout_tiles` and `top_bar_layout`: what
        // does not fit goes, because half-height glyphs are not a smaller name.
        let m = Metrics::default();
        let card = card_columns(apps(1920, 1080), &m, 3, 0).cards[0];
        assert_eq!(card_title(card, &m, m.card_header + 1), None);
        assert_eq!(card_title(card, &m, 0), None);
        // A degenerate card answers instead of producing a negative box.
        assert_eq!(card_title(Rect::new(0, 0, 0, 0), &m, 18), None);
    }

    #[test]
    fn the_header_of_a_card_hanging_off_the_zone_hangs_off_with_it() {
        // The sliver shows part of a header, not a header of its own size — the
        // same reason the card keeps its full width in D-047. A header measured
        // against the visible strip would put the neighbour's name in a place
        // the neighbour's name is not.
        let m = Metrics::default();
        let area = apps(360, 780);
        let l = card_columns(area, &m, 3, 1);
        let cut = l.cards[0];
        assert!(cut.x() < area.x());
        assert_eq!(card_header(cut, &m).w(), card_header(l.cards[1], &m).w());
        let (a, b) = (
            card_title(cut, &m, 18).expect("cut"),
            card_title(l.cards[1], &m, 18).expect("whole"),
        );
        assert_eq!((a.w(), a.h()), (b.w(), b.h()));
        assert_eq!(a.x() - cut.x(), b.x() - l.cards[1].x());
    }

    #[test]
    fn a_tile_too_small_for_both_keeps_the_icon_and_drops_the_name() {
        // Half the tile spent on a caption leaves a mark nobody can read, and
        // two unreadable things are worse than one readable one. The top bar
        // drops elements for the same reason rather than overlapping them.
        let tile = Rect::new(0, 0, 40, 40);
        assert_eq!(tile_face(tile, 30).caption, None);
        assert_eq!(tile_face(tile, 30).icon, tile);
        // A caption asking for nothing is not a caption.
        assert_eq!(tile_face(tile, 0).caption, None);
        // And a degenerate tile answers rather than producing a negative box.
        assert_eq!(tile_face(Rect::new(0, 0, 0, 0), 18).caption, None);
    }

    #[test]
    fn the_caption_takes_its_room_from_the_icon_and_not_from_the_tile() {
        // The grid is the thing being protected here: a name is drawn inside the
        // square, so a card of tiles with names and a card of tiles without them
        // have rows in the same places.
        let tile = Rect::new(10, 20, 96, 96);
        let with = tile_face(tile, 18);
        let without = tile_face(tile, 0);
        assert_eq!(without.icon, tile);
        assert!(with.icon.h() < without.icon.h());
        assert_eq!(with.icon.h(), 96 - 18);
    }

    #[test]
    fn a_sliver_too_narrow_for_a_tile_gets_none_rather_than_a_broken_one() {
        let m = Metrics::default();
        let sliver = Rect::new(0, 0, m.tile_unit + m.card_pad, 600);
        assert!(layout_tiles(sliver, &m, 4).is_empty());
    }
}
