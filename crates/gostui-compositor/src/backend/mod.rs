//! Backends — the only code in the workspace that touches a window system.
//!
//! Everything above this module works on `gostui-core` types and can be tested
//! with `cargo test` on a machine with no screen (D-016). A backend translates
//! events into core calls and draws the state core hands back; it decides
//! nothing on its own.

#[cfg(feature = "winit")]
pub mod winit;

/// Which backend the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// A window inside an existing X11/Wayland session. The everyday mode.
    Winit,
    /// DRM/KMS on a tty. M4; not built yet.
    Udev,
}

impl Backend {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "winit" | "nested" => Some(Self::Winit),
            "udev" | "drm" | "tty" => Some(Self::Udev),
            _ => None,
        }
    }
}

/// Which renderer draws the shell (D-001).
///
/// Declared here rather than next to the renderers themselves so that choosing
/// one costs no dependency on smithay: a build without the `winit` feature still
/// parses the flag and still explains itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererKind {
    /// GPU. The default where there is a GPU worth using.
    #[default]
    Gles,
    /// Our own rasteriser. Not a fallback — on a machine without a usable GPU
    /// it is the faster path (D-027).
    Cpu,
}

impl RendererKind {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "gles" | "gles2" | "gpu" => Some(Self::Gles),
            "pixman" | "cpu" | "software" => Some(Self::Cpu),
            _ => None,
        }
    }
}

/// Run a backend, or explain why this build cannot.
///
/// Returns the process exit code. A backend that fails to start is a normal
/// error path, not a panic: on a machine without EGL this is the first thing
/// the user will see.
///
/// `frames` stops after a given number of frames — the smoke test of "the
/// window opens and closes cleanly", which is otherwise unassertable.
///
/// `idle_test` stops after a duration instead and fails if the session asked
/// for a repaint in the meantime: the "zero rendering at rest" criterion with
/// an exit code (docs/01 §3.5).
#[cfg_attr(not(feature = "winit"), allow(unused_variables))]
pub fn run(
    backend: Backend,
    renderer: RendererKind,
    frames: Option<u64>,
    idle_test: Option<std::time::Duration>,
) -> i32 {
    match backend {
        #[cfg(feature = "winit")]
        Backend::Winit => match winit::run(renderer, frames, idle_test) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: backend winit nie wystartował: {e}");
                1
            }
        },
        #[cfg(not(feature = "winit"))]
        Backend::Winit => {
            eprintln!("error: ta binarka jest zbudowana bez cechy `winit`");
            eprintln!("       cargo run -p gostui-compositor --features winit -- --backend winit");
            1
        }
        Backend::Udev => {
            eprintln!("error: backend udev (DRM/KMS) przychodzi w M4, jeszcze go nie ma");
            eprintln!("       do codziennej pracy używaj `--backend winit`");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_names_are_recognised_with_their_aliases() {
        assert_eq!(Backend::parse("winit"), Some(Backend::Winit));
        assert_eq!(Backend::parse("nested"), Some(Backend::Winit));
        assert_eq!(Backend::parse("tty"), Some(Backend::Udev));
        assert_eq!(Backend::parse("Winit"), None, "names are case sensitive");
        assert_eq!(Backend::parse(""), None);
    }

    #[test]
    fn renderer_names_are_recognised_with_their_aliases() {
        assert_eq!(RendererKind::parse("gles2"), Some(RendererKind::Gles));
        assert_eq!(RendererKind::parse("pixman"), Some(RendererKind::Cpu));
        assert_eq!(RendererKind::parse("cpu"), Some(RendererKind::Cpu));
        assert_eq!(RendererKind::parse("vulkan"), None);
    }
}
