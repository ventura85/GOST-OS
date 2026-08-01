//! Two renderers behind one trait (D-001).
//!
//! The shell is described once — `gostui_render::display_list` turns the state
//! into rectangles in logical units — and rasterised twice: on the GPU with
//! `draw_solid`, or on the CPU with our own rasteriser. Neither path is the
//! fallback. On a machine with no usable GPU the CPU path is the *fast* one
//! (D-027), and on a phone the GPU path is the only one that keeps the battery.
//!
//! **What is honest about the current shape, and what is not.** The trait takes
//! a `GlesFrame`, because in the nested backend even the CPU path presents its
//! result through GLES: it paints into a buffer and uploads it as one texture
//! (D-028, wariant 2). That makes the nested CPU path a *rasterisation* test,
//! not a *no-GPU* test. The no-GPU path — CPU straight into a dumb buffer with
//! no EGL anywhere — arrives with DRM/KMS in M4. Until then what keeps the CPU
//! path alive is the golden PNG, which needs neither.

pub mod cpu;
pub mod gles;

use crate::backend::RendererKind;
use gostui_core::geometry::Rect;
use gostui_render::{Painted, Rgba};
use smithay::backend::renderer::gles::{GlesFrame, GlesRenderer};
use smithay::backend::renderer::Color32F;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Physical, Rectangle, Size};

pub type Error = Box<dyn std::error::Error>;

/// Build the renderer the user asked for. Runtime choice, not a build flag:
/// both paths ship in every binary, because both are supported (D-001, D-027).
pub fn build(kind: RendererKind) -> Box<dyn ShellRenderer> {
    match kind {
        RendererKind::Gles => Box::new(gles::Gles::default()),
        RendererKind::Cpu => Box::new(cpu::Cpu::default()),
    }
}

/// Turns a display-list surface slot into the client surface behind it.
///
/// The display list carries only an opaque id, because `gostui-render` must not
/// know that wayland exists (D-016). Both renderers need the real surface — one
/// to import a texture from it, the other to copy its shared memory — so the
/// lookup is a trait the backend implements and hands to whichever renderer is
/// running, rather than a field either of them owns.
pub trait SurfaceSource {
    fn surface(&self, id: u64) -> Option<&WlSurface>;
}

/// Rasterise a display list into the frame that is currently bound.
pub trait ShellRenderer {
    /// Name for logs and for `--help`. Which path ran must never be a guess.
    fn label(&self) -> &'static str;

    /// Work that needs the renderer itself rather than a frame — uploading a
    /// texture, for instance. Runs before the frame is opened, because a frame
    /// borrows the renderer for as long as it lives.
    fn prepare(
        &mut self,
        _renderer: &mut GlesRenderer,
        _list: &[Painted],
        _size: Size<i32, Physical>,
        _scale: i32,
        _surfaces: &dyn SurfaceSource,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Draw into the open frame.
    fn draw(
        &mut self,
        frame: &mut GlesFrame<'_, '_>,
        list: &[Painted],
        scale: i32,
    ) -> Result<(), Error>;
}

/// Logical rectangle → physical, the one place the scale is applied (D-011).
pub fn to_physical(rect: Rect, scale: i32) -> Rectangle<i32, Physical> {
    let s = scale.max(1);
    Rectangle::new(
        (rect.x() * s, rect.y() * s).into(),
        (rect.w() * s, rect.h() * s).into(),
    )
}

/// straight-alpha RGBA8 (what the rasteriser speaks) → GL's four floats.
pub fn to_color32f(c: Rgba) -> Color32F {
    const M: f32 = 255.0;
    Color32F::new(
        c.0 as f32 / M,
        c.1 as f32 / M,
        c.2 as f32 / M,
        c.3 as f32 / M,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_multiplies_position_as_well_as_size() {
        // Multiplying only the size is the classic scaling bug: everything is
        // the right size and in the wrong place.
        let r = to_physical(Rect::new(10, 20, 30, 40), 2);
        assert_eq!((r.loc.x, r.loc.y), (20, 40));
        assert_eq!((r.size.w, r.size.h), (60, 80));
    }

    #[test]
    fn colours_convert_without_touching_the_palette() {
        let c = to_color32f(Rgba(0, 128, 255, 255));
        assert_eq!(c.r(), 0.0);
        assert!((c.g() - 128.0 / 255.0).abs() < f32::EPSILON);
        assert_eq!(c.b(), 1.0);
        assert_eq!(c.a(), 1.0);
    }
}
