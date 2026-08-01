//! Deterministic software rasteriser.
//!
//! This is the CPU path, and on a machine without a usable GPU it is the *fast*
//! path, not a fallback (D-027). It has a second job that matters just as much:
//! because it is deterministic, the same shell state always produces the same
//! bytes, which is what makes golden-image tests possible without a GPU or a
//! screen.
//!
//! Everything here works on straight-alpha RGBA8, the layout in logical units
//! multiplied by an output scale only at the moment of rasterisation (D-011).

#![forbid(unsafe_code)]

pub mod paint;
pub mod text;

pub use paint::{display_list, paint, Fill, Palette, ShellView};
pub use text::{only_fills, Align, Image, Painted, Primitive, SurfaceSlot, TextRenderer, TextRun};

use gostui_core::geometry::Rect;
use std::path::Path;

/// An RGBA8 colour.
///
/// Defined in `gostui-core` and re-exported here: a colour is part of the theme,
/// and the theme is data the layout reasons about, not a detail of rasterising
/// (D-016, D-032).
pub use gostui_core::theme::Rgba;

/// A drawable surface in device pixels.
#[derive(Debug, Clone)]
pub struct Canvas {
    width: u32,
    height: u32,
    /// Scale applied to every logical coordinate on the way in.
    scale: i32,
    pixels: Vec<u8>,
}

impl Canvas {
    /// Create a canvas for a logical size at a given integer scale.
    ///
    /// Returns `None` for a degenerate size rather than panicking: sizes reach
    /// this code from output modes, which come from hardware.
    pub fn new(logical_w: i32, logical_h: i32, scale: i32) -> Option<Self> {
        let scale = scale.max(1);
        let width = u32::try_from(logical_w.checked_mul(scale)?).ok()?;
        let height = u32::try_from(logical_h.checked_mul(scale)?).ok()?;
        if width == 0 || height == 0 {
            return None;
        }
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        Some(Self {
            width,
            height,
            scale,
            pixels: vec![0; len],
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Fill the whole canvas.
    pub fn clear(&mut self, colour: Rgba) {
        for px in self.pixels.chunks_exact_mut(4) {
            px[0] = colour.0;
            px[1] = colour.1;
            px[2] = colour.2;
            px[3] = colour.3;
        }
    }

    /// Fill a rectangle given in **logical** units.
    ///
    /// Clipped to the canvas, so a rectangle that hangs off the edge draws its
    /// visible part instead of erroring. Layout can legitimately produce those
    /// while a window is being dragged between outputs.
    pub fn fill_rect(&mut self, rect: Rect, colour: Rgba) {
        let s = self.scale;
        let x0 = (rect.x() * s).max(0) as u32;
        let y0 = (rect.y() * s).max(0) as u32;
        let x1 = ((rect.right() * s).max(0) as u32).min(self.width);
        let y1 = ((rect.bottom() * s).max(0) as u32).min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for y in y0..y1 {
            let row = (y as usize) * (self.width as usize) * 4;
            for x in x0..x1 {
                let i = row + (x as usize) * 4;
                self.pixels[i] = colour.0;
                self.pixels[i + 1] = colour.1;
                self.pixels[i + 2] = colour.2;
                self.pixels[i + 3] = colour.3;
            }
        }
    }

    /// Draw a rectangle outline `thickness` logical units wide, inside `rect`.
    pub fn stroke_rect(&mut self, rect: Rect, thickness: i32, colour: Rgba) {
        let t = thickness.max(1);
        if rect.w() <= 0 || rect.h() <= 0 {
            return;
        }
        self.fill_rect(Rect::new(rect.x(), rect.y(), rect.w(), t), colour);
        self.fill_rect(Rect::new(rect.x(), rect.bottom() - t, rect.w(), t), colour);
        self.fill_rect(Rect::new(rect.x(), rect.y(), t, rect.h()), colour);
        self.fill_rect(Rect::new(rect.right() - t, rect.y(), t, rect.h()), colour);
    }

    /// Write the canvas as a PNG.
    pub fn write_png(&self, path: &Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let w = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        writer
            .write_image_data(&self.pixels)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_multiplies_the_pixel_buffer_not_the_layout() {
        let c = Canvas::new(360, 800, 2).unwrap();
        assert_eq!((c.width(), c.height()), (720, 1600));
    }

    #[test]
    fn a_degenerate_size_yields_none_rather_than_panicking() {
        assert!(Canvas::new(0, 100, 1).is_none());
        assert!(Canvas::new(100, 0, 1).is_none());
    }

    #[test]
    fn fill_writes_exactly_the_requested_area() {
        let mut c = Canvas::new(10, 10, 1).unwrap();
        c.fill_rect(Rect::new(2, 2, 3, 3), Rgba::rgb(255, 0, 0));
        let at = |x: usize, y: usize| c.pixels()[(y * 10 + x) * 4];
        assert_eq!(at(2, 2), 255);
        assert_eq!(at(4, 4), 255);
        assert_eq!(at(5, 5), 0, "one past the far edge must stay clear");
        assert_eq!(at(1, 1), 0);
    }

    #[test]
    fn a_rectangle_hanging_off_the_edge_is_clipped_not_rejected() {
        let mut c = Canvas::new(10, 10, 1).unwrap();
        c.fill_rect(Rect::new(-5, -5, 100, 100), Rgba::rgb(1, 2, 3));
        assert_eq!(c.pixels()[0], 1);
        let last = (10 * 10 - 1) * 4;
        assert_eq!(c.pixels()[last], 1);
    }

    #[test]
    fn rendering_is_deterministic() {
        // The property golden-image testing rests on.
        let draw = || {
            let mut c = Canvas::new(64, 32, 1).unwrap();
            c.clear(Rgba::rgb(10, 20, 30));
            c.fill_rect(Rect::new(4, 4, 20, 10), Rgba::rgb(200, 100, 50));
            c.stroke_rect(Rect::new(30, 4, 20, 20), 2, Rgba::rgb(0, 255, 0));
            c
        };
        assert_eq!(draw().pixels(), draw().pixels());
    }
}
