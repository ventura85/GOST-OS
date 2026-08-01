//! The output model (D-026).
//!
//! Outputs live in a **collection**, never in a single field, even though v1 drives
//! one screen. The target scenario is a phone in a dock: a portrait 720x1600 panel
//! at scale 2.0 and a landscape 1920x1080 monitor at scale 1.0, in the same session,
//! at the same time. Code that assumes "the output" cannot express that, and
//! retrofitting the assumption away means rewriting layout.
//!
//! Scale and transform are fields from day one (D-011, D-026), even while every
//! real output here reports scale 1.0 and no rotation.

use crate::geometry::{Rect, Size};

/// Stable handle for an output. Wraps an integer so callers cannot accidentally
/// mix it up with an index into some vector — outputs come and go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputId(pub u32);

/// How the compositor rotates and flips its rendering for this output.
///
/// The variants match `wl_output.transform`, so the compositor layer can map
/// them across without a translation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transform {
    #[default]
    Normal,
    _90,
    _180,
    _270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl Transform {
    /// True when this transform swaps the width and height of the visible area.
    pub const fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::_90 | Self::_270 | Self::Flipped90 | Self::Flipped270
        )
    }
}

/// A single screen: a physical panel, an HDMI monitor, or a USB-C dock output.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub id: OutputId,
    /// Connector name as the backend reports it, e.g. `HDMI-A-1`, `DSI-1`, `winit`.
    pub name: String,
    /// Resolution of the physical framebuffer, in device pixels, before transform.
    pub mode_px: Size,
    /// Integer scale factor. Layout is computed in logical units and multiplied
    /// by this only at rasterisation (D-011).
    pub scale: i32,
    pub transform: Transform,
    /// Where this output's logical area starts in the global layout. With one
    /// output it is always (0, 0); with a dock it is how the screens sit relative
    /// to each other.
    pub position: crate::geometry::Point,
}

impl Output {
    /// Create an untransformed output at scale 1.0, positioned at the origin.
    pub fn new(id: OutputId, name: impl Into<String>, mode_px: Size) -> Self {
        Self {
            id,
            name: name.into(),
            mode_px,
            scale: 1,
            transform: Transform::default(),
            position: crate::geometry::Point::new(0, 0),
        }
    }

    pub fn with_scale(mut self, scale: i32) -> Self {
        self.scale = scale.max(1);
        self
    }

    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    pub fn at(mut self, x: i32, y: i32) -> Self {
        self.position = crate::geometry::Point::new(x, y);
        self
    }

    /// The size of this output in logical units: rotation applied, then scale
    /// divided out. This is the number every layout calculation works with.
    pub fn logical_size(&self) -> Size {
        let scale = self.scale.max(1);
        let rotated = if self.transform.swaps_axes() {
            self.mode_px.transposed()
        } else {
            self.mode_px
        };
        Size::new((rotated.w / scale).max(1), (rotated.h / scale).max(1))
    }

    /// The whole logical area of this output, positioned in the global layout.
    pub fn logical_rect(&self) -> Rect {
        Rect {
            origin: self.position,
            size: self.logical_size(),
        }
    }

    /// True when this output is taller than it is wide — a phone held upright.
    /// Tiles stack on such an output and sit side by side on a monitor (D-025).
    pub fn is_portrait(&self) -> bool {
        let s = self.logical_size();
        s.h > s.w
    }
}

/// The set of outputs currently present.
///
/// Deliberately a `Vec` and not a map: the count is single digit, order is what
/// the user sees, and linear scans beat hashing at this size — which matters on
/// the old hardware this targets (D-027).
#[derive(Debug, Clone, Default)]
pub struct Outputs {
    outputs: Vec<Output>,
    next_id: u32,
}

impl Outputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an output, assigning it a fresh id.
    pub fn add(&mut self, name: impl Into<String>, mode_px: Size) -> OutputId {
        let id = OutputId(self.next_id);
        self.next_id += 1;
        self.outputs.push(Output::new(id, name, mode_px));
        id
    }

    /// Register an already-built output, keeping its id.
    pub fn insert(&mut self, output: Output) {
        self.next_id = self.next_id.max(output.id.0 + 1);
        match self.outputs.iter_mut().find(|o| o.id == output.id) {
            Some(slot) => *slot = output,
            None => self.outputs.push(output),
        }
    }

    /// Remove an output — a monitor unplugged from the dock.
    ///
    /// Returns whether anything was removed. Callers must handle the case where
    /// the removed output held windows; surviving an unplug is an acceptance
    /// criterion for M2 (D-026), not a later refinement.
    pub fn remove(&mut self, id: OutputId) -> bool {
        let before = self.outputs.len();
        self.outputs.retain(|o| o.id != id);
        self.outputs.len() != before
    }

    pub fn get(&self, id: OutputId) -> Option<&Output> {
        self.outputs.iter().find(|o| o.id == id)
    }

    pub fn get_mut(&mut self, id: OutputId) -> Option<&mut Output> {
        self.outputs.iter_mut().find(|o| o.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Output> {
        self.outputs.iter()
    }

    pub fn len(&self) -> usize {
        self.outputs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }

    /// The output windows should fall back to when the one they were on
    /// disappears. `None` only when the last output is gone — a headless
    /// session, which must not be a crash.
    pub fn fallback(&self) -> Option<&Output> {
        self.outputs.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn scale_divides_logical_size() {
        let o = Output::new(OutputId(0), "DSI-1", Size::new(1080, 2400)).with_scale(2);
        assert_eq!(o.logical_size(), Size::new(540, 1200));
    }

    #[test]
    fn rotation_swaps_axes() {
        let o =
            Output::new(OutputId(0), "DSI-1", Size::new(1080, 2400)).with_transform(Transform::_90);
        assert_eq!(o.logical_size(), Size::new(2400, 1080));
        assert!(!o.is_portrait());
    }

    #[test]
    fn dock_scenario_two_outputs_differ_in_orientation_and_scale() {
        // The whole reason outputs are a collection (D-026).
        let mut outs = Outputs::new();
        outs.insert(
            Output::new(OutputId(0), "DSI-1", Size::new(720, 1600))
                .with_scale(2)
                .at(0, 0),
        );
        outs.insert(Output::new(OutputId(1), "DP-1", Size::new(1920, 1080)).at(360, 0));

        let phone = outs.get(OutputId(0)).unwrap();
        let monitor = outs.get(OutputId(1)).unwrap();

        assert!(phone.is_portrait());
        assert!(!monitor.is_portrait());
        assert_eq!(phone.logical_size(), Size::new(360, 800));
        assert_eq!(monitor.logical_rect().origin, Point::new(360, 0));
    }

    #[test]
    fn unplugging_an_output_leaves_a_fallback() {
        let mut outs = Outputs::new();
        let phone = outs.add("DSI-1", Size::new(720, 1600));
        let monitor = outs.add("DP-1", Size::new(1920, 1080));

        assert!(outs.remove(monitor));
        assert_eq!(outs.len(), 1);
        assert_eq!(outs.fallback().map(|o| o.id), Some(phone));
    }

    #[test]
    fn removing_the_last_output_is_not_a_panic() {
        let mut outs = Outputs::new();
        let only = outs.add("winit", Size::new(1280, 720));
        assert!(outs.remove(only));
        assert!(outs.is_empty());
        assert!(outs.fallback().is_none());
    }

    #[test]
    fn removing_an_unknown_output_reports_false() {
        let mut outs = Outputs::new();
        outs.add("winit", Size::new(1280, 720));
        assert!(!outs.remove(OutputId(99)));
    }

    #[test]
    fn scale_zero_from_a_broken_backend_does_not_divide_by_zero() {
        let o = Output::new(OutputId(0), "bad", Size::new(800, 600)).with_scale(0);
        assert_eq!(o.logical_size(), Size::new(800, 600));
    }
}
