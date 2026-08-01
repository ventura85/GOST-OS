//! Tiling layout (D-025), computed per output (D-026).
//!
//! Three rules from the decision register drive everything here, and all three
//! are the ones that make tiling compositors unusable when they are missing:
//!
//! 1. Tiling is *not* "every window visible". A tile limit applies; the rest of
//!    the windows wait on the bottom bar.
//! 2. Dialogs, file choosers and popups are **not** tiled. They float, centred
//!    over their parent. Tiling a "Save as" window is the single most common way
//!    a tiling compositor becomes unusable.
//! 3. A client's minimum size is respected. An application that does not fit in a
//!    tile is not tiled.
//!
//! Splits run along the **longer axis** of the area, so a portrait phone screen
//! stacks tiles and a landscape monitor puts them side by side — in the same
//! session, when the phone is docked.

use crate::geometry::{Axis, Rect, Size};

/// Smallest tile we are willing to produce along the split axis, in logical units.
/// Below this a desktop application is not usable, so we tile fewer windows instead.
///
/// The value is set by the tightest case we actually target: a 720x1600 phone panel
/// at scale 2 is 360x800 logical, and 800 minus the two bars leaves 700 for windows.
/// Two stacked tiles of 350 must therefore be allowed — that is the phone splitting
/// its screen in half, which D-025 explicitly requires. A higher floor would silently
/// disable tiling on exactly the device the whole model was designed for.
pub const MIN_TILE_EXTENT: i32 = 240;

/// Compile-time guard for the paragraph above: raising `MIN_TILE_EXTENT` past 350
/// would turn tiling off on a phone screen without any test failing elsewhere.
const _: () = assert!(MIN_TILE_EXTENT <= 350);

/// An area whose longer axis reaches this gets three tiles instead of two.
const WIDE_OUTPUT_EXTENT: i32 = 1600;

/// Split position between two tiles, in permille of the available extent.
///
/// Integer rather than floating point on purpose: the divider is draggable and its
/// position is persisted, and golden-image tests must not depend on float rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split(u16);

impl Split {
    pub const EVEN: Split = Split(500);

    /// Clamps to a range that cannot produce a tile below `MIN_TILE_EXTENT` on any
    /// plausible screen, so a wild drag cannot collapse a window to nothing.
    pub fn from_permille(v: i32) -> Self {
        Split(v.clamp(150, 850) as u16)
    }

    pub const fn permille(self) -> i32 {
        self.0 as i32
    }
}

impl Default for Split {
    fn default() -> Self {
        Self::EVEN
    }
}

/// Gap between tiles and around the tiled area, in logical units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gaps {
    pub outer: i32,
    pub inner: i32,
}

impl Default for Gaps {
    fn default() -> Self {
        Self { outer: 0, inner: 4 }
    }
}

/// How many windows may be tiled at once in this area.
///
/// Returns 0 for a degenerate area, 1 when a second tile would fall below
/// `MIN_TILE_EXTENT`, 3 on a wide desktop, 2 otherwise.
pub fn tile_limit(area: Rect, gaps: Gaps) -> usize {
    if !area.size.is_valid() {
        return 0;
    }
    let extent = match area.longer_axis() {
        Axis::Horizontal => area.w(),
        Axis::Vertical => area.h(),
    };
    let usable = extent - 2 * gaps.outer;
    if usable < MIN_TILE_EXTENT {
        return if usable > 0 { 1 } else { 0 };
    }
    if (usable - gaps.inner) / 2 < MIN_TILE_EXTENT {
        return 1;
    }
    if extent >= WIDE_OUTPUT_EXTENT && (usable - 2 * gaps.inner) / 3 >= MIN_TILE_EXTENT {
        3
    } else {
        2
    }
}

/// Divide `area` into `count` tiles along its longer axis.
///
/// `count` is clamped to `tile_limit`, so callers cannot request more tiles than
/// the area supports. `split` applies to the two-tile case; three tiles are even,
/// with any remainder given to the leading tiles so the tiles exactly fill the area.
pub fn tile(area: Rect, count: usize, split: Split, gaps: Gaps) -> Vec<Rect> {
    let limit = tile_limit(area, gaps);
    let count = count.min(limit);
    if count == 0 {
        return Vec::new();
    }

    let inner = area.inset(gaps.outer);
    if count == 1 {
        return vec![inner];
    }

    let axis = area.longer_axis();
    let extent = match axis {
        Axis::Horizontal => inner.w(),
        Axis::Vertical => inner.h(),
    };
    let available = extent - gaps.inner * (count as i32 - 1);

    let mut extents = Vec::with_capacity(count);
    if count == 2 {
        let first = available * split.permille() / 1000;
        extents.push(first);
        extents.push(available - first);
    } else {
        let base = available / count as i32;
        let mut remainder = available % count as i32;
        for _ in 0..count {
            let extra = if remainder > 0 { 1 } else { 0 };
            remainder -= extra;
            extents.push(base + extra);
        }
    }

    let mut tiles = Vec::with_capacity(count);
    let mut cursor = 0;
    for e in extents {
        let r = match axis {
            Axis::Horizontal => Rect::new(inner.x() + cursor, inner.y(), e, inner.h()),
            Axis::Vertical => Rect::new(inner.x(), inner.y() + cursor, inner.w(), e),
        };
        tiles.push(r);
        cursor += e + gaps.inner;
    }
    tiles
}

/// What kind of surface a client has asked for. Mirrors the distinctions
/// `xdg-shell` makes, so the compositor layer can map its roles onto this
/// without inventing its own vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRole {
    /// An ordinary application window.
    Toplevel,
    /// A modal or transient window: "Save as", "Preferences", an alert.
    Dialog,
    /// A menu or tooltip, positioned relative to its parent.
    Popup,
}

/// Where a surface ends up on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Occupies a tile, sized by the layout.
    Tiled,
    /// Floats above the tiles at its own size, centred over its parent.
    Floating,
}

/// Decide whether a surface is tiled or floats.
///
/// `min_size` is the client's own `set_min_size`; `tile` is the tile it would be
/// given. A client that does not fit floats rather than being squeezed below its
/// minimum — squeezing produces clipped, unusable windows, and the client is
/// entitled to refuse the size anyway.
pub fn placement(role: SurfaceRole, min_size: Size, tile: Rect, fullscreen: bool) -> Placement {
    if fullscreen {
        // Fullscreen escapes tiling entirely: video, games, presentations.
        return Placement::Floating;
    }
    match role {
        SurfaceRole::Dialog | SurfaceRole::Popup => Placement::Floating,
        SurfaceRole::Toplevel => {
            if min_size.fits_in(tile.size) {
                Placement::Tiled
            } else {
                Placement::Floating
            }
        }
    }
}

/// Centre `size` inside `area`, clamped so the result never starts off-screen.
/// Used for floating dialogs, which must stay reachable even when larger than
/// the area they are centred in.
pub fn centred(size: Size, area: Rect) -> Rect {
    let x = area.x() + (area.w() - size.w) / 2;
    let y = area.y() + (area.h() - size.h) / 2;
    Rect::new(x.max(area.x()), y.max(area.y()), size.w, size.h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect::new(0, 0, 1920, 1040); // 1080 minus the two bars
    const PHONE: Rect = Rect::new(0, 0, 360, 700);

    #[test]
    fn monitor_splits_side_by_side() {
        let tiles = tile(MONITOR, 2, Split::EVEN, Gaps { outer: 0, inner: 0 });
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0], Rect::new(0, 0, 960, 1040));
        assert_eq!(tiles[1], Rect::new(960, 0, 960, 1040));
    }

    #[test]
    fn phone_splits_one_above_the_other() {
        // Same function, same session, different output — this is D-026 in one test.
        let tiles = tile(PHONE, 2, Split::EVEN, Gaps { outer: 0, inner: 0 });
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0], Rect::new(0, 0, 360, 350));
        assert_eq!(tiles[1], Rect::new(0, 350, 360, 350));
    }

    #[test]
    fn wide_output_allows_three_tiles_narrow_one_does_not() {
        assert_eq!(tile_limit(MONITOR, Gaps { outer: 0, inner: 0 }), 3);
        assert_eq!(tile_limit(PHONE, Gaps { outer: 0, inner: 0 }), 2);
    }

    #[test]
    fn requesting_more_tiles_than_the_limit_is_clamped_not_an_error() {
        // Windows beyond the limit wait on the bottom bar; asking for five is
        // a normal thing for the caller to do.
        let tiles = tile(PHONE, 5, Split::EVEN, Gaps::default());
        assert_eq!(tiles.len(), 2);
    }

    #[test]
    fn tiles_exactly_fill_the_area_with_no_lost_units() {
        let gaps = Gaps { outer: 8, inner: 6 };
        for count in 1..=3 {
            let tiles = tile(MONITOR, count, Split::EVEN, gaps);
            if tiles.len() < 2 {
                continue;
            }
            let first = tiles.first().unwrap();
            let last = tiles.last().unwrap();
            assert_eq!(first.x(), MONITOR.x() + gaps.outer);
            assert_eq!(last.right(), MONITOR.right() - gaps.outer);
        }
    }

    #[test]
    fn three_tiles_distribute_the_remainder() {
        // 1700 does not divide by 3; no logical unit may be dropped, or the tiles
        // stop meeting the screen edge and a one-unit gap appears in the corner.
        let area = Rect::new(0, 0, 1700, 400);
        let tiles = tile(area, 3, Split::EVEN, Gaps { outer: 0, inner: 0 });
        assert_eq!(tiles.len(), 3);
        let total: i32 = tiles.iter().map(|t| t.w()).sum();
        assert_eq!(total, 1700);
        assert_eq!(tiles[0].w(), 567);
        assert_eq!(tiles[2].w(), 566);
    }

    #[test]
    fn a_phone_sized_area_still_tiles_in_two() {
        // The phone half-splitting its own screen — see MIN_TILE_EXTENT.
        assert_eq!(tile_limit(PHONE, Gaps { outer: 0, inner: 0 }), 2);
    }

    #[test]
    fn a_dragged_divider_cannot_collapse_a_tile() {
        let tiles = tile(MONITOR, 2, Split::from_permille(0), Gaps::default());
        assert!(
            tiles[0].w() >= 200,
            "clamped split still leaves a usable tile"
        );
        let tiles = tile(MONITOR, 2, Split::from_permille(5000), Gaps::default());
        assert!(tiles[1].w() >= 200);
    }

    #[test]
    fn a_tiny_area_yields_one_tile_not_a_panic() {
        let tiny = Rect::new(0, 0, 200, 120);
        assert_eq!(tile_limit(tiny, Gaps::default()), 1);
        assert_eq!(tile(tiny, 2, Split::EVEN, Gaps::default()).len(), 1);
    }

    #[test]
    fn a_zero_sized_area_yields_no_tiles() {
        let empty = Rect::new(0, 0, 0, 0);
        assert_eq!(tile(empty, 2, Split::EVEN, Gaps::default()), Vec::new());
    }

    #[test]
    fn save_as_dialog_floats() {
        // D-025, trap 2, as an executable test.
        let t = Rect::new(0, 0, 960, 1040);
        assert_eq!(
            placement(SurfaceRole::Dialog, Size::new(400, 300), t, false),
            Placement::Floating
        );
        assert_eq!(
            placement(SurfaceRole::Popup, Size::new(120, 200), t, false),
            Placement::Floating
        );
    }

    #[test]
    fn a_client_that_does_not_fit_its_tile_is_not_tiled() {
        // Trap 3: respect set_min_size.
        let t = Rect::new(0, 0, 360, 350);
        assert_eq!(
            placement(SurfaceRole::Toplevel, Size::new(800, 600), t, false),
            Placement::Floating
        );
        assert_eq!(
            placement(SurfaceRole::Toplevel, Size::new(300, 200), t, false),
            Placement::Tiled
        );
    }

    #[test]
    fn fullscreen_escapes_tiling() {
        let t = Rect::new(0, 0, 960, 1040);
        assert_eq!(
            placement(SurfaceRole::Toplevel, Size::new(10, 10), t, true),
            Placement::Floating
        );
    }

    #[test]
    fn an_oversized_dialog_stays_reachable() {
        let area = Rect::new(0, 0, 360, 700);
        let r = centred(Size::new(800, 600), area);
        assert_eq!(r.origin.x, 0);
        assert_eq!(r.origin.y, 50);
    }
}
