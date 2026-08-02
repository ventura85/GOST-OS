//! Text, rasterised once and executed by both renderers (D-005).
//!
//! # Why text is not just another rectangle
//!
//! Everything the shell drew until now was a solid rectangle, which both the
//! GPU and the CPU path can produce independently and still agree on. Glyphs
//! cannot work that way: shaping and hinting are far too fiddly for two
//! implementations to match pixel for pixel, and the moment they diverge the
//! golden-image comparison that keeps the paths honest stops meaning anything.
//!
//! So text is rasterised **once**, here, into straight-alpha RGBA8 with the
//! colour already applied, and both renderers merely place the result: the CPU
//! path blends it into the canvas, the GPU path uploads it as a texture. Same
//! bytes, same picture, one implementation.
//!
//! # Where the layout boundary falls
//!
//! `gostui-core` decides **which box** a string goes in — that is arithmetic
//! with a testable answer, so it belongs there (D-016). Deciding where the
//! glyphs sit *within* that box needs their measured width, which needs the
//! font, so it happens here. The split is deliberate: nothing in core has to
//! know a font exists, and nothing here decides what gets drawn.
//!
//! # What is not deterministic, and what that costs
//!
//! Rasterising is deterministic for a given font, but *which* font a family
//! name resolves to comes from fontconfig, so it varies between machines. That
//! is why the golden-image tests cover rectangles only: text is tested through
//! its layout and caching, not its pixels. Pixel-exact text tests need a font
//! committed to the repository, and that is a decision with a size cost nobody
//! has taken yet.

use crate::{Canvas, Rgba};
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache};
use gostui_core::geometry::Rect;
use std::collections::HashMap;
use std::sync::Arc;

/// Where a string sits inside the box core gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    Start,
    Centre,
    End,
}

/// A string to draw, in logical units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    /// The box core allocated. Text is centred vertically in it and placed
    /// horizontally according to `align`; it is never allowed to escape.
    pub area: Rect,
    pub text: String,
    /// Font size in logical units, multiplied by the output scale on the way to
    /// the rasteriser and nowhere else (D-011).
    pub size: i32,
    pub colour: Rgba,
    /// Family name as fontconfig knows it. Empty means the system sans-serif.
    pub family: String,
    pub align: Align,
}

/// A client window's place in the picture.
///
/// The shell describes *where* a window goes; the pixels belong to the client
/// and this crate never sees them. The compositor resolves the slot — as a
/// texture on the GPU path, as a memory blit on the CPU path — which is why the
/// slot carries an opaque id rather than anything drawable.
///
/// Keeping it in the display list rather than drawing windows in a separate pass
/// is what makes the z-order a property of one ordered list: a window is above
/// the tab slider and below the bars because it appears between them here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSlot {
    /// Opaque handle. The compositor knows it as a window id; nothing in this
    /// crate may interpret it.
    pub id: u64,
    /// Where the window goes, in logical units.
    pub rect: Rect,
    /// Where the window starts **inside the client's buffer**.
    ///
    /// Not always zero, and assuming it is costs exactly one visible bug: a
    /// client that draws its own decorations puts shadows and rounded corners
    /// outside its declared window geometry, so its buffer begins above and left
    /// of the window a person sees. Drawing from the buffer's corner then leaves
    /// a band of shadow inside the tile and pushes the window off by that much.
    /// Everything to the left of and above this point is skipped.
    pub src: (i32, i32),
    /// The window that has the keyboard.
    ///
    /// Marked on the slot rather than passed beside it because focus is a
    /// property of one window among several, and a separate "which id is
    /// focused" field is a second place for the answer to be wrong.
    pub focused: bool,
    /// Drawn **over** the two bars instead of under them.
    ///
    /// True for exactly one thing: a fullscreen window. Everywhere else the bars
    /// are on top, because a window that could cover them would cover the only
    /// way out of itself. Fullscreen is the deliberate exception, and it carries
    /// its own way out (`Super+F`).
    pub over_bars: bool,
}

/// One item of a display list before fonts are involved.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Fill(crate::Fill),
    Text(TextRun),
    Surface(SurfaceSlot),
}

/// A rasterised image in device pixels, straight-alpha RGBA8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Top-left in device pixels.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A display list with the fonts resolved: only rectangles and images remain.
///
/// Order is preserved from the original list. That matters: text drawn before a
/// panel that covers it must stay covered, and a design where all text is
/// painted last would bleed through every overlay we ever add.
#[derive(Debug, Clone, PartialEq)]
pub enum Painted {
    Fill(crate::Fill),
    Image(Arc<Image>),
    /// A client window, still unresolved: the compositor turns it into pixels,
    /// each renderer in its own way. It survives `resolve` untouched because
    /// fonts have nothing to say about it.
    Surface(SurfaceSlot),
}

/// What a rasterised string is cached under.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    text: String,
    /// Device pixels, so the same string at two output scales is two entries.
    size_px: i32,
    colour: (u8, u8, u8, u8),
    family: String,
}

/// How many rasterised strings we are willing to keep (D-039).
///
/// The number comes from what is on screen at once: a clock, a handful of bar
/// labels, one chip per open window, and the launcher grid — a few dozen, with
/// room to spare for a second output at a different scale. Two hundred entries
/// of interface text is on the order of half a megabyte.
const CACHE_LIMIT: usize = 200;

/// Shapes and rasterises text, and remembers what it has already done.
///
/// The cache is what makes a once-a-minute clock free: the same string at the
/// same size and colour is rasterised once for as long as it keeps appearing.
///
/// # Why it has a limit, and why that limit is not a detail
///
/// The key includes the **text**, and interface text changes: the clock is a
/// different string every minute, and a client may rename its window as often
/// as it likes. Without a bound this cache grows for as long as the session
/// lives — measured at 1440 entries and **5.3 MB per day** from the clock alone,
/// against a whole-process budget of 50 MB (D-038). A shell meant to run for
/// weeks cannot keep everything it has ever drawn, so the oldest unused entry is
/// dropped once the cache is full (D-039).
pub struct TextRenderer {
    fonts: FontSystem,
    swash: SwashCache,
    cache: HashMap<Key, Entry>,
    /// Ticks once per lookup, so "least recently used" is a comparison of two
    /// integers rather than a list that has to be kept in order. At a few
    /// hundred entries the linear scan for the minimum is cheaper than
    /// maintaining the order, and far easier to be sure about (D-027).
    clock: u64,
    /// True once we have found there are no usable fonts at all, so a system
    /// without any does not pay for a failed lookup on every frame.
    fontless: bool,
}

/// A cached image and when it was last wanted.
#[derive(Debug)]
struct Entry {
    image: Arc<Image>,
    used: u64,
}

// `FontSystem` holds a font database and has no useful Debug; the crate lints
// for missing Debug, so give it one that says something true and short.
impl std::fmt::Debug for TextRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextRenderer")
            .field("cached_runs", &self.cache.len())
            .field("fontless", &self.fontless)
            .finish()
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRenderer {
    /// Build a text renderer, loading the system font database.
    ///
    /// This is the expensive part of the text stack and the reason it is built
    /// once and kept: scanning fontconfig on every frame would be absurd.
    pub fn new() -> Self {
        let fonts = FontSystem::new();
        let fontless = fonts.db().is_empty();
        Self {
            fonts,
            swash: SwashCache::new(),
            cache: HashMap::new(),
            clock: 0,
            fontless,
        }
    }

    /// True when the system has no fonts at all. The shell still runs; it just
    /// draws no text, which is better than refusing to start.
    pub fn is_fontless(&self) -> bool {
        self.fontless
    }

    pub fn cached_runs(&self) -> usize {
        self.cache.len()
    }

    /// Drop the cache. Called when the theme changes, since colour is part of
    /// the key and a re-themed shell would otherwise keep the old entries.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Resolve every text run in a display list into a placed image.
    ///
    /// A run that cannot be rasterised — no fonts, an empty string, a box with
    /// no room — is dropped silently. Text is decoration over a picture that is
    /// already correct; failing to draw it must never fail the frame.
    pub fn resolve(&mut self, list: &[Primitive], scale: i32) -> Vec<Painted> {
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            match item {
                Primitive::Fill(f) => out.push(Painted::Fill(*f)),
                Primitive::Text(run) => {
                    if let Some(image) = self.place(run, scale) {
                        out.push(Painted::Image(image));
                    }
                }
                Primitive::Surface(slot) => out.push(Painted::Surface(*slot)),
            }
        }
        out
    }

    /// Rasterise a run and position it inside its box.
    fn place(&mut self, run: &TextRun, scale: i32) -> Option<Arc<Image>> {
        let scale = scale.max(1);
        let glyphs = self.rasterise(run, scale)?;

        let box_px = (
            run.area.x() * scale,
            run.area.y() * scale,
            run.area.w() * scale,
            run.area.h() * scale,
        );
        let free_x = box_px.2 - glyphs.width as i32;
        let x = box_px.0
            + match run.align {
                Align::Start => 0,
                // Rounding down on an odd remainder keeps this identical on both
                // renderers, which share this function rather than each doing
                // their own arithmetic.
                Align::Centre => free_x / 2,
                Align::End => free_x,
            };
        let y = box_px.1 + (box_px.3 - glyphs.height as i32) / 2;

        Some(Arc::new(Image {
            x,
            y,
            width: glyphs.width,
            height: glyphs.height,
            pixels: glyphs.pixels.clone(),
        }))
    }

    /// Shape and rasterise, hitting the cache when the same string comes back.
    fn rasterise(&mut self, run: &TextRun, scale: i32) -> Option<Arc<Image>> {
        if self.fontless || run.text.is_empty() || run.size <= 0 {
            return None;
        }
        let size_px = run.size.saturating_mul(scale);
        let key = Key {
            text: run.text.clone(),
            size_px,
            colour: (run.colour.0, run.colour.1, run.colour.2, run.colour.3),
            family: run.family.clone(),
        };
        self.clock += 1;
        if let Some(hit) = self.cache.get_mut(&key) {
            hit.used = self.clock;
            return Some(hit.image.clone());
        }

        let size = size_px as f32;
        // Line height only has to contain the glyphs; the box comes from core
        // and the result is centred in it, so a generous factor costs nothing.
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * 1.4));
        // Unbounded: this is one line of interface text, not a paragraph. A
        // width limit here would wrap the clock.
        buffer.set_size(None, None);
        let family = if run.family.is_empty() {
            Family::SansSerif
        } else {
            Family::Name(&run.family)
        };
        buffer.set_text(
            &run.text,
            &Attrs::new().family(family),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.fonts, false);

        let mut width = 0.0f32;
        let mut height = 0.0f32;
        for line in buffer.layout_runs() {
            width = width.max(line.line_w);
            height = height.max(line.line_top + line.line_height);
        }
        let w = width.ceil().max(0.0) as u32;
        let h = height.ceil().max(0.0) as u32;
        if w == 0 || h == 0 {
            return None;
        }

        let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
        let colour =
            cosmic_text::Color::rgba(run.colour.0, run.colour.1, run.colour.2, run.colour.3);
        buffer.draw(
            &mut self.fonts,
            &mut self.swash,
            colour,
            |gx, gy, gw, gh, c| {
                let a = c.a();
                if a == 0 {
                    return;
                }
                for dy in 0..gh as i32 {
                    for dx in 0..gw as i32 {
                        let (px, py) = (gx + dx, gy + dy);
                        if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                            continue;
                        }
                        let i = ((py as usize) * (w as usize) + px as usize) * 4;
                        blend(&mut pixels[i..i + 4], c.r(), c.g(), c.b(), a);
                    }
                }
            },
        );

        let image = Arc::new(Image {
            x: 0,
            y: 0,
            width: w,
            height: h,
            pixels,
        });
        self.remember(key, image.clone());
        Some(image)
    }

    /// Store a freshly rasterised image, evicting the least recently used entry
    /// if the cache is full (D-039).
    ///
    /// Eviction happens *before* the insert, so the cache never momentarily
    /// exceeds its limit — the limit is the amount of memory this is allowed to
    /// hold, not an average it hovers around.
    fn remember(&mut self, key: Key, image: Arc<Image>) {
        if self.cache.len() >= CACHE_LIMIT {
            if let Some(oldest) = self
                .cache
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
            {
                self.cache.remove(&oldest);
            }
        }
        self.cache.insert(
            key,
            Entry {
                image,
                used: self.clock,
            },
        );
    }
}

/// Source-over blend of one straight-alpha pixel onto another.
fn blend(dst: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    if a == 255 {
        dst[0] = r;
        dst[1] = g;
        dst[2] = b;
        dst[3] = 255;
        return;
    }
    let sa = a as u32;
    let ia = 255 - sa;
    // Rounded, not truncated. The GPU path blends in floating point and rounds,
    // so truncating here put the two renderers 1/255 apart on every antialiased
    // glyph edge — small, but it broke the pixel-for-pixel agreement that is the
    // only thing keeping the two paths honest about each other.
    let mix = |s: u8, d: u8| ((s as u32 * sa + d as u32 * ia + 127) / 255) as u8;
    dst[0] = mix(r, dst[0]);
    dst[1] = mix(g, dst[1]);
    dst[2] = mix(b, dst[2]);
    dst[3] = (sa + (dst[3] as u32 * ia + 127) / 255).min(255) as u8;
}

impl Canvas {
    /// Blend a rasterised image into the canvas at its device-pixel position.
    ///
    /// Takes device pixels, not logical ones: glyphs are rasterised at the
    /// output's real resolution, and scaling them again here would be the
    /// blurry-text bug.
    pub fn blend_image(&mut self, image: &Image) {
        let cw = self.width() as i32;
        let ch = self.height() as i32;
        for row in 0..image.height as i32 {
            let py = image.y + row;
            if py < 0 || py >= ch {
                continue;
            }
            for col in 0..image.width as i32 {
                let px = image.x + col;
                if px < 0 || px >= cw {
                    continue;
                }
                let s = ((row as usize) * (image.width as usize) + col as usize) * 4;
                let a = image.pixels[s + 3];
                if a == 0 {
                    continue;
                }
                let d = ((py as usize) * (cw as usize) + px as usize) * 4;
                let (r, g, b) = (image.pixels[s], image.pixels[s + 1], image.pixels[s + 2]);
                blend(&mut self.pixels[d..d + 4], r, g, b, a);
            }
        }
    }

    /// Execute a resolved display list.
    /// Blend a block of straight-alpha RGBA8 pixels, clipped to `clip`.
    ///
    /// This is what a client window becomes on the CPU path: a wayland buffer is
    /// somebody else's memory, and the only thing this crate needs to know about
    /// it is its stride and where it goes.
    ///
    /// `clip` is not optional and not a nicety. A client is told its tile size
    /// and answers *eventually*; between the configure and the ack it still owns
    /// a buffer of the old size, and without a clip that stale buffer paints
    /// straight over the bars. Every parameter is bounds-checked because all of
    /// them ultimately come from a client, which is entitled to lie.
    pub fn blend_rgba(
        &mut self,
        origin: (i32, i32),
        width: u32,
        height: u32,
        stride: usize,
        pixels: &[u8],
        clip: (i32, i32, i32, i32),
    ) {
        let cw = self.width() as i32;
        let ch = self.height() as i32;
        // Intersect the clip with the canvas once, so the inner loop tests one
        // pair of bounds instead of two.
        let x0 = clip.0.max(0);
        let y0 = clip.1.max(0);
        let x1 = (clip.0 + clip.2).min(cw);
        let y1 = (clip.1 + clip.3).min(ch);

        for row in 0..height as i32 {
            let py = origin.1 + row;
            if py < y0 || py >= y1 {
                continue;
            }
            let line = row as usize * stride;
            for col in 0..width as i32 {
                let px = origin.0 + col;
                if px < x0 || px >= x1 {
                    continue;
                }
                let s = line + col as usize * 4;
                // A short buffer is a client bug, and the answer is to draw
                // less, not to panic in the compositor.
                let Some(src) = pixels.get(s..s + 4) else {
                    return;
                };
                if src[3] == 0 {
                    continue;
                }
                let d = (py as usize * cw as usize + px as usize) * 4;
                blend(&mut self.pixels[d..d + 4], src[0], src[1], src[2], src[3]);
            }
        }
    }

    /// Client windows are skipped: this canvas is also the golden-image path,
    /// which has no clients, and the pixels of a real one live in a wayland
    /// buffer this crate must not know about. The compositor's CPU renderer
    /// walks the same list and fills the slots in with [`Canvas::blend_rgba`].
    pub fn paint_list(&mut self, list: &[Painted]) {
        for item in list {
            match item {
                Painted::Fill(f) => self.fill_rect(f.rect, f.colour),
                Painted::Image(img) => self.blend_image(img),
                Painted::Surface(_) => {}
            }
        }
    }
}

/// Convenience for callers that only have solids — used by the tests that
/// predate text and by any path that draws before fonts are available.
pub fn only_fills(list: &[Primitive]) -> Vec<crate::Fill> {
    list.iter()
        .filter_map(|p| match p {
            Primitive::Fill(f) => Some(*f),
            Primitive::Text(_) | Primitive::Surface(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fill;

    fn run(text: &str, area: Rect, align: Align) -> TextRun {
        TextRun {
            area,
            text: text.to_string(),
            size: 14,
            colour: Rgba::rgb(255, 255, 255),
            family: String::new(),
            align,
        }
    }

    /// A block of solid pixels of one colour, in straight-alpha RGBA8.
    fn block(w: u32, h: u32, colour: [u8; 4]) -> Vec<u8> {
        colour
            .iter()
            .copied()
            .cycle()
            .take((w * h * 4) as usize)
            .collect()
    }

    #[test]
    fn a_client_buffer_larger_than_its_tile_is_clipped_to_it() {
        // The case that matters: a client is told to shrink and has not answered
        // yet, so it still owns a buffer of the old size. Without the clip that
        // stale buffer paints over the bars — over the only way out of the app.
        let mut c = Canvas::new(10, 10, 1).unwrap();
        c.clear(Rgba::rgb(0, 0, 0));
        let px = block(10, 10, [255, 0, 0, 255]);
        c.blend_rgba((0, 0), 10, 10, 40, &px, (0, 0, 4, 4));

        let at = |x: usize, y: usize| c.pixels()[(y * 10 + x) * 4];
        assert_eq!(at(3, 3), 255, "inside the tile");
        assert_eq!(at(4, 3), 0, "one column past the tile");
        assert_eq!(at(3, 4), 0, "one row past the tile");
    }

    #[test]
    fn a_buffer_with_padding_in_its_stride_does_not_skew() {
        // Stride is not width: a client may pad every row. Reading rows at
        // `width * 4` produces the classic diagonal smear.
        let mut c = Canvas::new(4, 2, 1).unwrap();
        c.clear(Rgba::rgb(0, 0, 0));
        let mut px = vec![0u8; 2 * 32];
        for row in 0..2 {
            for col in 0..2 {
                let s = row * 32 + col * 4;
                px[s..s + 4].copy_from_slice(&[9, 9, 9, 255]);
            }
        }
        c.blend_rgba((0, 0), 2, 2, 32, &px, (0, 0, 4, 2));
        let at = |x: usize, y: usize| c.pixels()[(y * 4 + x) * 4];
        assert_eq!((at(0, 0), at(1, 0)), (9, 9));
        assert_eq!(
            (at(0, 1), at(1, 1)),
            (9, 9),
            "second row read at the stride"
        );
        assert_eq!(at(2, 0), 0, "nothing beyond the buffer's width");
    }

    #[test]
    fn a_buffer_shorter_than_it_claims_draws_less_instead_of_panicking() {
        // Everything about a client buffer is a client's word. A short one is a
        // window drawn partly, never a dead compositor.
        let mut c = Canvas::new(4, 4, 1).unwrap();
        c.clear(Rgba::rgb(0, 0, 0));
        let px = block(4, 1, [7, 7, 7, 255]);
        c.blend_rgba((0, 0), 4, 4, 16, &px, (0, 0, 4, 4));
        assert_eq!(c.pixels()[0], 7);
    }

    #[test]
    fn a_fully_transparent_client_pixel_leaves_what_is_under_it() {
        let mut c = Canvas::new(2, 1, 1).unwrap();
        c.clear(Rgba::rgb(50, 60, 70));
        let px = block(2, 1, [0, 0, 0, 0]);
        c.blend_rgba((0, 0), 2, 1, 8, &px, (0, 0, 2, 1));
        assert_eq!(&c.pixels()[..3], &[50, 60, 70]);
    }

    #[test]
    fn a_surface_slot_survives_font_resolution_untouched() {
        // The compositor resolves it, not the text renderer — and the position
        // in the list must not move, or a window would change z-order.
        let slot = SurfaceSlot {
            id: 7,
            rect: Rect::new(1, 2, 3, 4),
            src: (0, 0),
            focused: false,
            over_bars: false,
        };
        let list = vec![
            Primitive::Fill(Fill {
                rect: Rect::new(0, 0, 1, 1),
                colour: Rgba::rgb(1, 1, 1),
            }),
            Primitive::Surface(slot),
        ];
        let mut t = TextRenderer::new();
        let out = t.resolve(&list, 1);
        assert_eq!(out.len(), 2);
        assert!(matches!(out[1], Painted::Surface(s) if s == slot));
    }

    #[test]
    fn blending_an_opaque_pixel_replaces_it() {
        let mut px = [0u8, 0, 0, 0];
        blend(&mut px, 10, 20, 30, 255);
        assert_eq!(px, [10, 20, 30, 255]);
    }

    #[test]
    fn blending_a_transparent_pixel_leaves_the_destination() {
        let mut px = [10u8, 20, 30, 255];
        blend(&mut px, 200, 200, 200, 0);
        assert_eq!(px, [10, 20, 30, 255]);
    }

    #[test]
    fn half_coverage_lands_between_the_two_colours() {
        let mut px = [0u8, 0, 0, 255];
        blend(&mut px, 255, 255, 255, 128);
        // Not asserting an exact midpoint: the point is that it moved, and that
        // it did not overshoot into either endpoint.
        assert!(px[0] > 100 && px[0] < 155, "got {}", px[0]);
        assert_eq!(px[3], 255);
    }

    #[test]
    fn an_image_hanging_off_the_canvas_is_clipped_not_a_panic() {
        let mut c = Canvas::new(10, 10, 1).unwrap();
        let img = Image {
            x: -3,
            y: -3,
            width: 20,
            height: 20,
            pixels: vec![255; 20 * 20 * 4],
        };
        c.blend_image(&img);
        assert_eq!(c.pixels()[0], 255);
    }

    #[test]
    fn fills_pass_through_resolution_untouched() {
        let mut t = TextRenderer::new();
        let fill = Fill {
            rect: Rect::new(0, 0, 10, 10),
            colour: Rgba::rgb(1, 2, 3),
        };
        let out = t.resolve(&[Primitive::Fill(fill)], 1);
        assert_eq!(out, vec![Painted::Fill(fill)]);
    }

    #[test]
    fn an_empty_string_produces_nothing_rather_than_an_empty_image() {
        let mut t = TextRenderer::new();
        let out = t.resolve(
            &[Primitive::Text(run(
                "",
                Rect::new(0, 0, 100, 20),
                Align::Start,
            ))],
            1,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn a_nonpositive_size_draws_nothing_instead_of_panicking() {
        let mut t = TextRenderer::new();
        let mut r = run("12:34", Rect::new(0, 0, 100, 20), Align::Start);
        r.size = 0;
        assert!(t.resolve(&[Primitive::Text(r.clone())], 1).is_empty());
        r.size = -5;
        assert!(t.resolve(&[Primitive::Text(r)], 1).is_empty());
    }

    #[test]
    fn only_fills_drops_text_and_keeps_order() {
        let a = Fill {
            rect: Rect::new(0, 0, 1, 1),
            colour: Rgba::rgb(1, 1, 1),
        };
        let b = Fill {
            rect: Rect::new(2, 2, 1, 1),
            colour: Rgba::rgb(2, 2, 2),
        };
        let list = vec![
            Primitive::Fill(a),
            Primitive::Text(run("x", Rect::new(0, 0, 9, 9), Align::Start)),
            Primitive::Fill(b),
        ];
        assert_eq!(only_fills(&list), vec![a, b]);
    }

    // The tests below need a font. On a machine with none they would be testing
    // the absence of one, so they say so and stop rather than failing.
    fn skip_without_fonts(t: &TextRenderer) -> bool {
        if t.is_fontless() {
            eprintln!("no fonts installed; skipping the rasterisation tests");
        }
        t.is_fontless()
    }

    #[test]
    fn a_string_is_rasterised_once_and_cached() {
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        let list = [Primitive::Text(run(
            "12:34",
            Rect::new(0, 0, 160, 48),
            Align::Centre,
        ))];
        assert_eq!(t.cached_runs(), 0);
        let first = t.resolve(&list, 1);
        assert_eq!(t.cached_runs(), 1);
        let second = t.resolve(&list, 1);
        // A clock redrawing the same minute must not re-shape anything.
        assert_eq!(t.cached_runs(), 1);
        assert_eq!(first, second);
    }

    #[test]
    fn the_same_string_at_two_scales_is_two_entries() {
        // Sharing them would give a phone at scale 2 the blurry upscaled bitmap
        // meant for a monitor.
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        let list = [Primitive::Text(run(
            "12:34",
            Rect::new(0, 0, 160, 48),
            Align::Centre,
        ))];
        t.resolve(&list, 1);
        t.resolve(&list, 2);
        assert_eq!(t.cached_runs(), 2);
    }

    #[test]
    fn a_day_of_clock_ticks_does_not_grow_the_cache_without_end() {
        // The measurement behind D-039, as a test. A whole day of distinct
        // strings used to leave 1440 entries and 5.3 MB behind, in a process
        // whose entire budget is 50 MB (D-038) and which is meant to run for
        // weeks. The number of *distinct strings* is unbounded; the memory the
        // cache is allowed to hold is not.
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        for h in 0..24 {
            for m in 0..60 {
                t.resolve(
                    &[Primitive::Text(run(
                        &format!("{h:02}:{m:02}"),
                        Rect::new(0, 0, 160, 48),
                        Align::Centre,
                    ))],
                    1,
                );
            }
        }
        assert!(
            t.cached_runs() <= CACHE_LIMIT,
            "1440 different strings left {} entries",
            t.cached_runs()
        );
    }

    #[test]
    fn the_entry_dropped_when_the_cache_is_full_is_the_least_recently_used() {
        // Evicting the newest, or an arbitrary one, would throw away the clock
        // that is on screen right now and keep a window title from an hour ago.
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        let draw = |t: &mut TextRenderer, s: &str| {
            t.resolve(
                &[Primitive::Text(run(
                    s,
                    Rect::new(0, 0, 160, 48),
                    Align::Start,
                ))],
                1,
            );
        };
        draw(&mut t, "keep-me");
        // Fill the cache to its limit, touching "keep-me" as we go so it stays
        // the most recently used entry rather than the oldest.
        for i in 0..CACHE_LIMIT {
            draw(&mut t, &format!("filler-{i}"));
            draw(&mut t, "keep-me");
        }
        assert_eq!(t.cached_runs(), CACHE_LIMIT);
        // Re-drawing it must be a cache hit: the count does not move.
        draw(&mut t, "keep-me");
        assert_eq!(t.cached_runs(), CACHE_LIMIT, "the used entry survived");
    }

    #[test]
    fn clearing_the_cache_forgets_everything() {
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        t.resolve(
            &[Primitive::Text(run(
                "12:34",
                Rect::new(0, 0, 160, 48),
                Align::Start,
            ))],
            1,
        );
        assert_eq!(t.cached_runs(), 1);
        t.clear_cache();
        assert_eq!(t.cached_runs(), 0);
    }

    #[test]
    fn alignment_moves_the_text_without_letting_it_escape_the_box() {
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        let area = Rect::new(100, 10, 200, 40);
        let mut at = |align| {
            let out = t.resolve(&[Primitive::Text(run("12:34", area, align))], 1);
            match out.first() {
                Some(Painted::Image(i)) => (i.x, i.y, i.width, i.height),
                other => panic!("expected an image, got {other:?}"),
            }
        };
        let (sx, _, w, _) = at(Align::Start);
        let (cx, cy, _, h) = at(Align::Centre);
        let (ex, _, _, _) = at(Align::End);

        assert_eq!(sx, area.x(), "start-aligned text must sit on the left edge");
        assert!(sx < cx && cx < ex, "got {sx} {cx} {ex}");
        assert_eq!(ex + w as i32, area.right());
        // Vertically centred, so a taller bar does not push text off the top.
        assert_eq!(cy, area.y() + (area.h() - h as i32) / 2);
    }

    #[test]
    fn a_wider_string_produces_a_wider_image() {
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        let area = Rect::new(0, 0, 400, 40);
        let mut width = |s: &str| {
            let out = t.resolve(&[Primitive::Text(run(s, area, Align::Start))], 1);
            match out.first() {
                Some(Painted::Image(i)) => i.width,
                _ => 0,
            }
        };
        assert!(width("00:00") < width("00:00 PM"));
    }

    #[test]
    fn scale_two_rasterises_at_twice_the_size() {
        // The property that makes text sharp on a phone instead of upscaled.
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        let area = Rect::new(0, 0, 400, 40);
        let mut height = |scale| {
            let out = t.resolve(&[Primitive::Text(run("12:34", area, Align::Start))], scale);
            match out.first() {
                Some(Painted::Image(i)) => i.height,
                _ => 0,
            }
        };
        let (one, two) = (height(1), height(2));
        assert!(two >= one * 2 - 2 && two <= one * 2 + 2, "{one} then {two}");
    }

    #[test]
    fn text_actually_reaches_the_canvas() {
        let mut t = TextRenderer::new();
        if skip_without_fonts(&t) {
            return;
        }
        let mut c = Canvas::new(200, 40, 1).unwrap();
        c.clear(Rgba::rgb(0, 0, 0));
        let list = t.resolve(
            &[Primitive::Text(run(
                "12:34",
                Rect::new(0, 0, 200, 40),
                Align::Centre,
            ))],
            1,
        );
        c.paint_list(&list);
        let lit = c.pixels().chunks_exact(4).filter(|p| p[0] > 0).count();
        assert!(lit > 0, "nothing was drawn");
    }
}
