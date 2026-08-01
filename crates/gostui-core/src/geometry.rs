//! Geometry in *logical* units (D-011).
//!
//! Nothing in this module knows about pixels. Output scale is applied once, at
//! rasterisation time, by the compositor. Keeping layout in logical units is what
//! makes a phone at scale 2.0 and a monitor at scale 1.0 shareable by one session.

/// A width/height pair in logical units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

impl Size {
    pub const fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }

    /// True when both dimensions are positive. A zero or negative size is not a
    /// usable surface, and several callers need to reject one without panicking.
    pub const fn is_valid(self) -> bool {
        self.w > 0 && self.h > 0
    }

    /// True when `self` fits inside `other` in both dimensions.
    pub const fn fits_in(self, other: Size) -> bool {
        self.w <= other.w && self.h <= other.h
    }

    /// Swap width and height. Used by 90/270 degree output transforms.
    pub const fn transposed(self) -> Self {
        Self {
            w: self.h,
            h: self.w,
        }
    }
}

/// A position in logical units, relative to the top-left of the output layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in logical units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(w, h),
        }
    }

    pub const fn from_size(size: Size) -> Self {
        Self {
            origin: Point::new(0, 0),
            size,
        }
    }

    pub const fn x(self) -> i32 {
        self.origin.x
    }

    pub const fn y(self) -> i32 {
        self.origin.y
    }

    pub const fn w(self) -> i32 {
        self.size.w
    }

    pub const fn h(self) -> i32 {
        self.size.h
    }

    pub const fn right(self) -> i32 {
        self.origin.x + self.size.w
    }

    pub const fn bottom(self) -> i32 {
        self.origin.y + self.size.h
    }

    pub const fn contains(self, p: Point) -> bool {
        p.x >= self.origin.x && p.x < self.right() && p.y >= self.origin.y && p.y < self.bottom()
    }

    /// The longer axis of this rectangle. Ties count as horizontal, because a
    /// square area splits more usefully side by side on a desktop.
    pub const fn longer_axis(self) -> Axis {
        if self.size.h > self.size.w {
            Axis::Vertical
        } else {
            Axis::Horizontal
        }
    }

    /// Shrink the rectangle by `margin` on every side. Never produces a negative
    /// size; an over-large margin collapses the rectangle to zero instead.
    pub fn inset(self, margin: i32) -> Self {
        let w = (self.size.w - 2 * margin).max(0);
        let h = (self.size.h - 2 * margin).max(0);
        Self {
            origin: Point::new(self.origin.x + margin, self.origin.y + margin),
            size: Size::new(w, h),
        }
    }
}

/// Which way an area is divided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Tiles sit side by side; the split line is vertical.
    Horizontal,
    /// Tiles sit one above the other; the split line is horizontal.
    Vertical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_axis_of_a_phone_screen_is_vertical() {
        // 720x1600 portrait: tiles must stack, not sit side by side (D-025, D-026).
        assert_eq!(Rect::new(0, 0, 720, 1600).longer_axis(), Axis::Vertical);
    }

    #[test]
    fn longer_axis_of_a_monitor_is_horizontal() {
        assert_eq!(Rect::new(0, 0, 1920, 1080).longer_axis(), Axis::Horizontal);
    }

    #[test]
    fn square_area_splits_side_by_side() {
        assert_eq!(Rect::new(0, 0, 800, 800).longer_axis(), Axis::Horizontal);
    }

    #[test]
    fn inset_never_goes_negative() {
        let r = Rect::new(0, 0, 10, 10).inset(50);
        assert_eq!(r.size, Size::new(0, 0));
    }

    #[test]
    fn fits_in_checks_both_dimensions() {
        assert!(Size::new(300, 200).fits_in(Size::new(300, 200)));
        assert!(!Size::new(301, 200).fits_in(Size::new(300, 200)));
        assert!(!Size::new(300, 201).fits_in(Size::new(300, 200)));
    }
}
