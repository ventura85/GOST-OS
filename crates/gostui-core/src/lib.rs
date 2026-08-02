//! Pure logic of the GostUI shell.
//!
//! # The boundary this crate exists to enforce (D-016)
//!
//! Nothing here depends on `smithay` or any `wayland-*` crate, and nothing here
//! performs graphics I/O. The compositor translates protocol events into calls on
//! this crate and draws the state it returns — that is the whole of its job.
//!
//! The practical test: if a piece of logic needs a running compositor to be
//! tested, it is in the wrong place. Move it here and test it with `cargo test`.
//!
//! # What lives where
//!
//! - [`geometry`] — sizes and rectangles in logical units (D-011).
//! - [`output`] — the output collection: scale, rotation, hot-unplug (D-026).
//! - [`shell`] — the three screen zones the whole interface is built on.
//! - [`input`] — what a point on screen means and what a shortcut does.
//! - [`layout`] — tiling, tile limits, and what floats instead (D-025).
//! - [`window`] — which window holds which tile and which ones wait (D-025).
//! - [`tab`] — the tab slider that replaces the desktop (D-003).
//! - [`clock`] — what the top bar clock says and when it changes.
//! - [`theme`] — colours, sizes and fonts as data the user owns (D-032).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod clock;
pub mod geometry;
pub mod input;
pub mod layout;
pub mod output;
pub mod shell;
pub mod tab;
pub mod theme;
pub mod window;

pub use clock::{ClockFormat, Wall};
pub use geometry::{Axis, Point, Rect, Size};
pub use input::{hit_test, Action, Binding, Hit, Keymap, Keysym, Mods, TopBarItem};
pub use layout::{Gaps, Placement, Split, SurfaceRole};
pub use output::{Output, OutputId, Outputs, Transform};
pub use shell::{zones, BarHeights, Zones, MIN_TOUCH_TARGET};
pub use tab::{LauncherItem, Tab, TabId, TabStrip};
pub use theme::{Fonts, Metrics, Palette, Pointing, Rgba, Theme};
pub use window::{Placed, Window, WindowId, WindowModel};
