//! The GPU path: one `draw_solid` per rectangle, one texture per string.
//!
//! Nothing clever on purpose. The shell is flat colour blocks (the spec bans
//! decorative effects), so the fast thing and the simple thing are the same
//! thing.
//!
//! Text arrived in step 5 and did not change that. The glyphs are rasterised
//! once in `gostui_render::text` — the same bytes the CPU path blends — and
//! this side only uploads them and puts them where they were told. Neither
//! renderer shapes text, so neither can disagree about it.

use super::{to_color32f, to_physical, Error, ShellRenderer, SurfaceSource};
use gostui_render::{Image, Painted, SurfaceSlot};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::{GlesFrame, GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::{import_surface_tree, with_renderer_surface_state};
use smithay::backend::renderer::{Frame, ImportMem, Renderer, Texture};
use smithay::utils::{Physical, Rectangle, Size, Transform};

#[derive(Default)]
pub struct Gles {
    /// One slot per display-list entry, `None` for the solids. Kept parallel to
    /// the list rather than compacted so `draw` can walk both together and
    /// preserve painting order — text that a later panel covers must stay
    /// covered.
    textures: Vec<Option<GlesTexture>>,
}

impl ShellRenderer for Gles {
    fn label(&self) -> &'static str {
        "gles2"
    }

    fn prepare(
        &mut self,
        renderer: &mut GlesRenderer,
        list: &[Painted],
        _size: Size<i32, Physical>,
        _scale: i32,
        surfaces: &dyn SurfaceSource,
    ) -> Result<(), Error> {
        // Uploads happen here because a frame borrows the renderer for as long
        // as it lives, and a texture cannot be created while one is open.
        self.textures.clear();
        self.textures.reserve(list.len());
        for item in list {
            match item {
                Painted::Fill(_) => self.textures.push(None),
                Painted::Image(img) => {
                    let premultiplied = premultiply(img);
                    self.textures.push(Some(renderer.import_memory(
                        &premultiplied,
                        Fourcc::Abgr8888,
                        (img.width as i32, img.height as i32).into(),
                        false,
                    )?));
                }
                Painted::Surface(slot) => {
                    self.textures.push(client_texture(renderer, slot, surfaces))
                }
            }
        }
        Ok(())
    }

    fn draw(
        &mut self,
        frame: &mut GlesFrame<'_, '_>,
        list: &[Painted],
        scale: i32,
    ) -> Result<(), Error> {
        for (item, texture) in list.iter().zip(self.textures.iter()) {
            match item {
                Painted::Fill(fill) => {
                    let dst = to_physical(fill.rect, scale);
                    // The damage rectangle is relative to `dst`, not to the
                    // output — passing output coordinates here draws the right
                    // colour in the wrong place, and only on some drivers.
                    let damage = [Rectangle::from_size(dst.size)];
                    frame.draw_solid(dst, &damage, to_color32f(fill.colour))?;
                }
                Painted::Image(img) => {
                    let Some(texture) = texture else { continue };
                    // Already in device pixels: glyphs were rasterised at the
                    // output's real resolution, so scaling here would undo the
                    // one thing that keeps small text sharp.
                    let size = Size::from((img.width as i32, img.height as i32));
                    let damage = [Rectangle::from_size(size)];
                    frame.render_texture_at(
                        texture,
                        (img.x, img.y).into(),
                        1,
                        1.0,
                        Transform::Normal,
                        &damage,
                        // **No opaque regions.** This argument is not a second
                        // damage rectangle: it tells smithay which parts need no
                        // blending, and it disables blending for them. A glyph
                        // box is almost entirely transparent, so claiming it is
                        // opaque draws the clock on a solid black tile — which is
                        // exactly what happened the first time this was written
                        // by copying the call from the CPU path, where the
                        // texture really is opaque.
                        &[],
                        1.0,
                    )?;
                }
                Painted::Surface(slot) => {
                    let Some(texture) = texture else { continue };
                    // The destination is the tile the layout gave the window.
                    // The source is the whole buffer, unscaled: a client that
                    // has not yet answered its configure still holds a buffer of
                    // the old size, and stretching it would make every resize a
                    // blurry smear. Drawing it at 1:1 and letting the tile clip
                    // it means a resize looks like a resize.
                    let dst = to_physical(slot.rect, scale);
                    let size = texture.size();
                    let visible = Rectangle::new(
                        dst.loc,
                        Size::from((size.w.min(dst.size.w), size.h.min(dst.size.h))),
                    );
                    if visible.size.w <= 0 || visible.size.h <= 0 {
                        continue;
                    }
                    let src = Rectangle::from_size(Size::from((
                        visible.size.w as f64,
                        visible.size.h as f64,
                    )));
                    let damage = [Rectangle::from_size(visible.size)];
                    frame.render_texture_from_to(
                        texture,
                        src,
                        visible,
                        &damage,
                        // Client buffers may be translucent (a terminal with
                        // transparency, a rounded GTK corner), so nothing here is
                        // claimed opaque. The cost is one blend over a picture
                        // that is already correct underneath.
                        &[],
                        Transform::Normal,
                        1.0,
                        // No custom shader and no extra uniforms: the shell has
                        // no effects to apply to a client's window, by design.
                        None,
                        &[],
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Fetch the texture smithay imported for this client's current buffer.
///
/// The import itself is smithay's: `on_commit_buffer_handler` records the buffer
/// on commit, and `import_surface_tree` turns it into a texture at most once per
/// commit, whatever kind of buffer it is. That last part is why this goes
/// through smithay instead of reading the memory ourselves — the same call
/// handles `linux-dmabuf` when it arrives, and a hand-rolled shm import would
/// have to be thrown away then.
///
/// Returns `None` for a window that has not committed a buffer yet, which is the
/// normal state of a window between mapping and its first frame.
fn client_texture(
    renderer: &mut GlesRenderer,
    slot: &SurfaceSlot,
    surfaces: &dyn SurfaceSource,
) -> Option<GlesTexture> {
    let surface = surfaces.surface(slot.id)?;
    if let Err(e) = import_surface_tree(renderer, surface) {
        // A buffer we cannot import is one window drawn as a hole, not a frame
        // lost for everybody.
        tracing::warn!("nie udało się zaimportować bufora klienta: {e}");
        return None;
    }
    let context = renderer.context_id();
    with_renderer_surface_state(surface, |state| {
        state.texture::<GlesTexture>(context).cloned()
    })?
}

/// Straight alpha → premultiplied, which is what GL blending expects.
///
/// The rasteriser speaks straight alpha throughout, because that is what makes
/// the CPU blend readable. Converting here rather than there keeps one format in
/// the shared code and confines the GPU's convention to the GPU path. The images
/// are glyph-sized — a clock is a few thousand pixels — so the copy costs
/// nothing worth measuring.
fn premultiply(img: &Image) -> Vec<u8> {
    let mut out = img.pixels.clone();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            // Rounded, to match the rounding the CPU blend does. Truncating here
            // biases every antialiased edge one step dark relative to the other
            // path, which is the difference the two-renderer comparison exists
            // to catch.
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pixels: Vec<u8>) -> Image {
        Image {
            x: 0,
            y: 0,
            width: (pixels.len() / 4) as u32,
            height: 1,
            pixels,
        }
    }

    #[test]
    fn opaque_pixels_pass_through_premultiplication_unchanged() {
        let img = image(vec![10, 20, 30, 255]);
        assert_eq!(premultiply(&img), vec![10, 20, 30, 255]);
    }

    #[test]
    fn transparent_pixels_lose_their_colour() {
        // Left as-is they show up as a coloured halo around every glyph.
        let img = image(vec![255, 255, 255, 0]);
        assert_eq!(premultiply(&img), vec![0, 0, 0, 0]);
    }

    #[test]
    fn half_covered_pixels_are_scaled_not_zeroed() {
        let img = image(vec![200, 100, 50, 128]);
        let out = premultiply(&img);
        assert_eq!(out[3], 128, "alpha itself must not be scaled");
        assert!(out[0] < 200 && out[0] > 80, "got {}", out[0]);
    }
}
