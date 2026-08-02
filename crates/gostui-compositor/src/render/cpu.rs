//! The CPU path: our own rasteriser, presented as a single texture.
//!
//! This is the same code that writes the golden PNGs — `gostui_render::Canvas`
//! — so what you see in the window and what the test suite compares byte for
//! byte are produced by one rasteriser. That is the whole value of running it
//! here: divergence between the two paths shows up as a picture, not as a
//! silent difference nobody looks at.
//!
//! The upload at the end is scaffolding, not the destination. On DRM (M4) the
//! same canvas goes straight into a dumb buffer and no GPU is involved at all;
//! the nested backend has no such route, because smithay's winit backend is
//! GLES-only (D-028).

use super::{Error, ShellRenderer, SurfaceSource};
use gostui_render::{Canvas, Painted, SurfaceSlot};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::{GlesFrame, GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::backend::renderer::{Frame, ImportMem};
use smithay::reexports::wayland_server::protocol::wl_shm::Format;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Physical, Rectangle, Size, Transform};
use smithay::wayland::shm::with_buffer_contents;

#[derive(Default)]
pub struct Cpu {
    texture: Option<GlesTexture>,
    /// Kept between frames so a redraw at an unchanged size reuses the
    /// allocation. A full-screen RGBA canvas at 1920×1080 is 8 MB — not a thing
    /// to allocate on every event (D-029).
    canvas: Option<Canvas>,
    /// Size of the texture prepared for this frame, in physical pixels.
    size: Size<i32, Physical>,
}

/// Blend one client's shared-memory buffer into the canvas.
///
/// # Why this exists at all
///
/// On the GPU path smithay imports the buffer as a texture and the driver does
/// the rest. There is no driver here, and that is the point (D-027): on a
/// machine whose GPU is `llvmpipe`, copying the bytes ourselves beats asking a
/// software GL implementation to do it. This function is the reason a window can
/// appear on a machine with no usable graphics hardware.
///
/// # What it is not
///
/// It handles `wl_shm` and nothing else. A client using `linux-dmabuf` has its
/// pixels in GPU memory, which the CPU path cannot read without an import path
/// that does not exist yet — such a window is skipped rather than drawn wrong.
/// That is an honest limit of this step, not a bug to paper over: `wl_shm` is
/// the baseline every client supports, and dmabuf clients are the ones that have
/// a GPU by definition.
fn blend_client(canvas: &mut Canvas, slot: &SurfaceSlot, surface: &WlSurface, scale: i32) {
    let Some(Some(buffer)) = with_renderer_surface_state(surface, |s| s.buffer().cloned()) else {
        return;
    };
    let scale = scale.max(1);
    let clip = (
        slot.rect.x() * scale,
        slot.rect.y() * scale,
        slot.rect.w() * scale,
        slot.rect.h() * scale,
    );
    let origin = (clip.0, clip.1);

    let result = with_buffer_contents(&buffer, |ptr, len, data| {
        // Every one of these came from a client and none may be trusted: a
        // negative stride, a height that overruns the pool, a format we never
        // advertised. Anything that does not add up means we draw nothing.
        if data.width <= 0 || data.height <= 0 || data.stride <= 0 || data.offset < 0 {
            return;
        }
        let (w, h) = (data.width as usize, data.height as usize);
        let stride = data.stride as usize;
        let offset = data.offset as usize;
        if w.saturating_mul(4) > stride {
            return;
        }
        let Some(needed) = h.checked_mul(stride).and_then(|n| n.checked_add(offset)) else {
            return;
        };
        if needed > len {
            return;
        }
        // SAFETY: smithay keeps the pool mapped for the duration of this
        // callback and installs a SIGBUS handler for a client that shrinks the
        // file under us; the bounds above keep every read inside `len`.
        let pool = unsafe { std::slice::from_raw_parts(ptr, len) };

        // The alpha channel is meaningful only for Argb8888. Xrgb8888 carries
        // rubbish there, and honouring it draws a fully transparent window —
        // which looks exactly like a compositor that failed to draw anything.
        let opaque = !matches!(data.format, Format::Argb8888);

        // The client's own decorations live outside its window geometry, so the
        // window starts this far into the buffer (see `SurfaceSlot::src`).
        let skip_x = (slot.src.0.max(0) as usize).min(w);
        let skip_y = (slot.src.1.max(0) as usize).min(h);
        let visible_w = w - skip_x;
        if visible_w == 0 {
            return;
        }

        let mut row = vec![0u8; visible_w * 4];
        for y in skip_y..h {
            let line = offset + y * stride;
            for x in skip_x..w {
                let s = line + x * 4;
                // wl_shm's ARGB8888 is a little-endian 32-bit word, so in memory
                // it reads B, G, R, A — and it is **premultiplied**. The
                // rasteriser works in straight alpha throughout, so undo the
                // premultiplication here rather than teaching the shared blend a
                // second convention (the same reasoning as `premultiply` on the
                // GPU side, in the other direction).
                let (b, g, r) = (pool[s], pool[s + 1], pool[s + 2]);
                let a = if opaque { 255 } else { pool[s + 3] };
                let d = (x - skip_x) * 4;
                match a {
                    0 => row[d..d + 4].copy_from_slice(&[0, 0, 0, 0]),
                    255 => row[d..d + 4].copy_from_slice(&[r, g, b, 255]),
                    a => {
                        let un =
                            |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                        row[d..d + 4].copy_from_slice(&[un(r), un(g), un(b), a]);
                    }
                }
            }
            canvas.blend_rgba(
                (origin.0, origin.1 + (y - skip_y) as i32),
                visible_w as u32,
                1,
                visible_w * 4,
                &row,
                clip,
            );
        }
    });
    if let Err(e) = result {
        // A buffer we cannot read is one window missing, not a dead compositor.
        tracing::warn!("nie udało się odczytać bufora klienta: {e}");
    }
}

impl ShellRenderer for Cpu {
    fn label(&self) -> &'static str {
        "pixman (CPU)"
    }

    fn prepare(
        &mut self,
        renderer: &mut GlesRenderer,
        list: &[Painted],
        size: Size<i32, Physical>,
        scale: i32,
        surfaces: &dyn SurfaceSource,
    ) -> Result<(), Error> {
        let scale = scale.max(1);
        let logical = (size.w / scale, size.h / scale);

        let fits = self.canvas.as_ref().is_some_and(|c| {
            c.width() == size.w.max(0) as u32 && c.height() == size.h.max(0) as u32
        });
        if !fits {
            self.canvas = Canvas::new(logical.0, logical.1, scale);
        }
        let Some(canvas) = self.canvas.as_mut() else {
            // A window with no area is not an error worth killing a shell over.
            return Ok(());
        };

        // Walked here rather than through `paint_list`, because a client window
        // is the one item the canvas cannot resolve on its own: its pixels live
        // in somebody else's shared memory. Order still comes from the list and
        // nowhere else — a window drawn after the slider and before the bars
        // because that is where it sits in the list.
        for item in list {
            match item {
                Painted::Fill(f) => canvas.fill_rect(f.rect, f.colour),
                Painted::Image(img) => canvas.blend_image(img),
                Painted::Surface(slot) => {
                    if let Some(surface) = surfaces.surface(slot.id) {
                        blend_client(canvas, slot, surface, scale);
                    }
                }
            }
        }

        // RGBA8 in memory order is `Abgr8888` in DRM's little-endian naming.
        // Getting this wrong swaps red and blue — and looks plausible until
        // somebody notices the brand colours are inverted.
        self.texture = Some(renderer.import_memory(
            canvas.pixels(),
            Fourcc::Abgr8888,
            (size.w, size.h).into(),
            false,
        )?);
        self.size = size;
        Ok(())
    }

    fn draw(
        &mut self,
        frame: &mut GlesFrame<'_, '_>,
        _list: &[Painted],
        _scale: i32,
    ) -> Result<(), Error> {
        let Some(texture) = self.texture.as_ref() else {
            return Ok(());
        };
        let damage = [Rectangle::from_size(self.size)];
        // Opaque: the display list starts with a background covering the whole
        // screen, so there is nothing underneath to blend with.
        frame.render_texture_at(
            texture,
            (0, 0).into(),
            1,
            1.0,
            Transform::Normal,
            &damage,
            &damage,
            1.0,
        )?;
        Ok(())
    }
}
