//! The window model: which windows exist, which of them hold a tile, and which
//! wait on the bottom bar (D-025, per output — D-026).
//!
//! This module exists *before* any wayland code so that the answer to "what
//! happens when a third window opens on a two-tile screen" is decided by a test
//! and not by the shape of a protocol handler. [`layout`](crate::layout) already
//! answers "where do N tiles go"; this module answers the questions that come
//! before it:
//!
//! - which windows are in those N tiles, and which ones are queued;
//! - what a new window does when the tiles are full (it takes the focused tile,
//!   and the window that was there goes to the bottom bar — D-025, trap 1);
//! - what happens when a tiled window closes (the longest-waiting one moves in);
//! - what happens when an output disappears with windows standing on it (D-026:
//!   they move to a remaining output, they do not vanish and they do not panic).
//!
//! # Capacity is pushed in, not computed here
//!
//! How many tiles fit depends on the area, the gaps and the screen's longer
//! axis, all of which live in [`layout::tile_limit`](crate::layout::tile_limit).
//! The compositor calls [`WindowModel::set_capacity`] whenever an output's
//! geometry changes. Keeping the number as state rather than recomputing it here
//! is what lets the queue react to a resize — shrinking the capacity has to spill
//! windows onto the bottom bar, and that spill is a decision with a test.

use crate::geometry::{Rect, Size};
use crate::layout::{self, centred, placement, Gaps, Placement, Split, SurfaceRole};
use crate::output::OutputId;

/// Stable handle for a window. An index would not survive a window closing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u32);

/// Everything the shell knows about one client surface.
///
/// Deliberately no protocol objects, no buffers and no surface handles: those
/// belong to the compositor crate (D-016). What is here is what layout and the
/// bottom bar need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub id: WindowId,
    /// `xdg_toplevel.set_title`. Shown on the bottom bar.
    pub title: String,
    /// `xdg_toplevel.set_app_id`. Used to find an icon, so it is kept separate
    /// from the title, which the client changes freely.
    pub app_id: String,
    pub role: SurfaceRole,
    /// The client's `set_min_size`. A window that does not fit its tile floats
    /// instead of being squeezed (D-025, trap 3).
    pub min_size: Size,
    /// Size the window last asked for, used when it floats. Never used for a
    /// tiled window: there the layout decides and the client obeys.
    pub size: Size,
    /// Dialogs and popups are positioned over their parent. `None` for a
    /// toplevel — and for a parentless dialog, which is a client bug we tolerate
    /// by centring it on the output.
    pub parent: Option<WindowId>,
    pub fullscreen: bool,
    pub output: OutputId,
}

impl Window {
    /// True for windows that take part in tiling and appear on the bottom bar.
    /// Dialogs and popups do neither: they belong to their parent.
    pub fn is_toplevel(&self) -> bool {
        matches!(self.role, SurfaceRole::Toplevel)
    }
}

/// A window resolved to a place on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub window: WindowId,
    pub rect: Rect,
    pub placement: Placement,
}

/// The windows of one output: the bottom bar's order, the tile assignment, and
/// how many tiles the current geometry allows.
#[derive(Debug, Clone)]
struct OutputWindows {
    output: OutputId,
    /// Every toplevel on this output, in the order it opened. This is the bottom
    /// bar, and it is deliberately *not* reordered when tiles change: a chip that
    /// moves when you swap windows is a chip you cannot hit twice in a row.
    order: Vec<WindowId>,
    /// The windows currently holding a tile, in tile order. Always a subset of
    /// `order`, never longer than `capacity`.
    tiled: Vec<WindowId>,
    capacity: usize,
}

impl OutputWindows {
    fn new(output: OutputId) -> Self {
        Self {
            output,
            order: Vec::new(),
            tiled: Vec::new(),
            capacity: 1,
        }
    }

    fn waiting(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.order.iter().copied().filter(|w| !self.is_tiled(*w))
    }

    fn is_tiled(&self, w: WindowId) -> bool {
        self.tiled.contains(&w)
    }

    /// Pull waiting windows into free tiles, oldest first. Called after anything
    /// that can leave a tile empty.
    fn fill_free_tiles(&mut self) {
        while self.tiled.len() < self.capacity {
            let Some(next) = self.waiting().next() else {
                return;
            };
            self.tiled.push(next);
        }
    }
}

/// All windows in the session, across all outputs.
///
/// One model rather than one per output, because windows move between outputs —
/// that is the whole point of D-026, and a per-output model would make the move
/// a special case instead of a list operation.
#[derive(Debug, Clone, Default)]
pub struct WindowModel {
    windows: Vec<Window>,
    outputs: Vec<OutputWindows>,
    focus: Option<WindowId>,
    next_id: u32,
}

impl WindowModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many tiles this output currently has room for.
    ///
    /// The compositor recomputes this with [`layout::tile_limit`] whenever the
    /// output's application zone changes, and pushes it in here. Lowering it
    /// spills the excess windows onto the bottom bar; raising it pulls the
    /// longest-waiting ones back in.
    pub fn set_capacity(&mut self, output: OutputId, capacity: usize) {
        let slot = self.slot_mut(output);
        slot.capacity = capacity;
        if slot.tiled.len() > capacity {
            // Truncating from the end keeps the leading tiles stable, so a resize
            // does not shuffle the windows the user is looking at.
            slot.tiled.truncate(capacity);
        }
        slot.fill_free_tiles();
    }

    pub fn capacity(&self, output: OutputId) -> usize {
        self.slot(output).map_or(0, |s| s.capacity)
    }

    /// Open a toplevel on `output`.
    ///
    /// When every tile is taken the new window replaces the **focused** tile and
    /// the window that was there moves to the bottom bar — it is not closed and
    /// not resized (D-025, trap 1). With no focus the last tile gives way,
    /// because the newest window is the one the user is least attached to.
    pub fn open_toplevel(
        &mut self,
        output: OutputId,
        app_id: impl Into<String>,
        title: impl Into<String>,
    ) -> WindowId {
        let id = self.push_window(Window {
            id: WindowId(self.next_id),
            title: title.into(),
            app_id: app_id.into(),
            role: SurfaceRole::Toplevel,
            min_size: Size::new(1, 1),
            size: Size::new(640, 480),
            parent: None,
            fullscreen: false,
            output,
        });

        let focus = self.focus;
        let slot = self.slot_mut(output);
        slot.order.push(id);
        if slot.tiled.len() < slot.capacity {
            slot.tiled.push(id);
        } else if slot.capacity > 0 {
            let victim = focus
                .and_then(|f| slot.tiled.iter().position(|w| *w == f))
                .unwrap_or(slot.tiled.len() - 1);
            slot.tiled[victim] = id;
        }
        self.focus = Some(id);
        id
    }

    /// Open a dialog or popup belonging to `parent`.
    ///
    /// It takes no tile and never appears on the bottom bar: it floats over its
    /// parent and closes with it. Tiling this is the single most common way a
    /// tiling compositor becomes unusable (D-025, trap 2), so there is no
    /// code path here that could.
    pub fn open_child(
        &mut self,
        parent: WindowId,
        role: SurfaceRole,
        size: Size,
        title: impl Into<String>,
    ) -> Option<WindowId> {
        let p = self.get(parent)?;
        let output = p.output;
        let app_id = p.app_id.clone();
        let id = self.push_window(Window {
            id: WindowId(self.next_id),
            title: title.into(),
            app_id,
            role,
            min_size: Size::new(1, 1),
            size,
            parent: Some(parent),
            fullscreen: false,
            output,
        });
        // A dialog takes focus; a popup does not, because a menu closing must
        // not leave focus dangling on a surface that lived for 200 ms.
        if role == SurfaceRole::Dialog {
            self.focus = Some(id);
        }
        Some(id)
    }

    fn push_window(&mut self, w: Window) -> WindowId {
        let id = w.id;
        self.next_id += 1;
        self.windows.push(w);
        id
    }

    /// Close a window and everything that belonged to it.
    ///
    /// A tiled window's tile is filled by the longest-waiting window, so the
    /// screen never keeps a hole while windows queue on the bar. Returns the
    /// windows actually removed, children included — the compositor needs the
    /// list to drop its own per-window state.
    pub fn close(&mut self, id: WindowId) -> Vec<WindowId> {
        let mut doomed = vec![id];
        // Children of children: a popup opened from a dialog is normal.
        let mut i = 0;
        while i < doomed.len() {
            let parent = doomed[i];
            for w in &self.windows {
                if w.parent == Some(parent) && !doomed.contains(&w.id) {
                    doomed.push(w.id);
                }
            }
            i += 1;
        }

        self.windows.retain(|w| !doomed.contains(&w.id));
        for slot in &mut self.outputs {
            slot.order.retain(|w| !doomed.contains(w));
            slot.tiled.retain(|w| !doomed.contains(w));
            slot.fill_free_tiles();
        }
        if self.focus.is_some_and(|f| doomed.contains(&f)) {
            // Focus goes to a tiled window on the same output where possible;
            // leaving focus at None would make the keyboard go nowhere.
            self.focus = self
                .outputs
                .iter()
                .flat_map(|s| s.tiled.iter())
                .next()
                .copied();
        }
        doomed
    }

    /// Bring a window to a tile and focus it — a click on its bottom-bar chip.
    ///
    /// A waiting window takes the focused tile, and the window that was there
    /// goes back to the bar. This is a swap, not a reorder: the bar's order
    /// never changes, so the chips stay where the user's finger last found them.
    pub fn activate(&mut self, id: WindowId) -> bool {
        let Some(w) = self.get(id) else {
            return false;
        };
        if !w.is_toplevel() {
            // Focusing a dialog is legitimate; it just has no tile to take.
            self.focus = Some(id);
            return true;
        }
        let output = w.output;
        let focus = self.focus;
        let slot = self.slot_mut(output);
        if !slot.is_tiled(id) {
            if slot.tiled.len() < slot.capacity {
                slot.tiled.push(id);
            } else if slot.capacity > 0 {
                let victim = focus
                    .and_then(|f| slot.tiled.iter().position(|w| *w == f))
                    .unwrap_or(0);
                slot.tiled[victim] = id;
            }
        }
        self.focus = Some(id);
        true
    }

    pub fn focus(&mut self, id: WindowId) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.focus = Some(id);
        true
    }

    pub fn focused(&self) -> Option<WindowId> {
        self.focus
    }

    /// Move every window from `from` onto `to`, then re-fill `to`'s tiles.
    ///
    /// This is a monitor being unplugged while windows stand on it (D-026), and
    /// also a headless remote-desktop output going away (D-035) — the model does
    /// not distinguish, because neither does anything else in core. The source
    /// output is left empty but present; removing it from the collection is
    /// [`Outputs`](crate::output::Outputs)' job.
    pub fn migrate(&mut self, from: OutputId, to: OutputId) {
        if from == to {
            return;
        }
        let moved: Vec<WindowId> = match self.slot(from) {
            Some(s) => s.order.clone(),
            None => return,
        };
        for w in &mut self.windows {
            if w.output == from {
                w.output = to;
            }
        }
        let source = self.slot_mut(from);
        source.order.clear();
        source.tiled.clear();
        let target = self.slot_mut(to);
        target.order.extend(moved);
        target.fill_free_tiles();
    }

    /// Windows holding a tile on this output, in tile order.
    pub fn tiled(&self, output: OutputId) -> &[WindowId] {
        self.slot(output).map_or(&[], |s| &s.tiled)
    }

    /// Toplevels on this output that are not currently tiled — the ones waiting
    /// on the bottom bar.
    pub fn waiting(&self, output: OutputId) -> Vec<WindowId> {
        self.slot(output)
            .map(|s| s.waiting().collect())
            .unwrap_or_default()
    }

    /// Every toplevel on this output in bottom-bar order (opening order).
    pub fn bar(&self, output: OutputId) -> &[WindowId] {
        self.slot(output).map_or(&[], |s| &s.order)
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Window> {
        self.windows.iter()
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Resolve this output's windows to rectangles, back to front.
    ///
    /// `area` is the application zone from [`shell::zones`](crate::shell::zones),
    /// never the whole output: the bars are not window space.
    ///
    /// Tiled windows come first, then the floating children of whatever is
    /// visible. A child of a waiting window is not placed — its parent is not on
    /// screen, so neither is it.
    ///
    /// Popups are returned centred over their parent, which is a placeholder:
    /// their real position comes from `xdg_positioner`, which is protocol data
    /// and therefore the compositor's to apply. What core decides is that a popup
    /// *floats*, never that it takes a tile.
    pub fn layout(&self, output: OutputId, area: Rect, split: Split, gaps: Gaps) -> Vec<Placed> {
        let Some(slot) = self.slot(output) else {
            return Vec::new();
        };
        let mut out = Vec::new();

        let rects = layout::tile(area, slot.tiled.len(), split, gaps);
        for (id, tile) in slot.tiled.iter().zip(rects.iter()) {
            let Some(w) = self.get(*id) else { continue };
            let p = placement(w.role, w.min_size, *tile, w.fullscreen);
            let rect = match p {
                Placement::Tiled => *tile,
                // A window that refuses its tile keeps the slot but is drawn at
                // its own size, centred: better a floating window than one
                // clipped below its minimum.
                Placement::Floating => {
                    let size = if w.fullscreen {
                        area.size
                    } else {
                        Size::new(w.size.w.max(w.min_size.w), w.size.h.max(w.min_size.h))
                    };
                    centred(size, area)
                }
            };
            out.push(Placed {
                window: *id,
                rect,
                placement: p,
            });
        }

        // Children float over the parent's rectangle. Collected after the tiles
        // so the returned order is already back-to-front for the renderer.
        let visible: Vec<Placed> = out.clone();
        for w in &self.windows {
            if w.output != output || w.is_toplevel() {
                continue;
            }
            let Some(parent) = w.parent else {
                // A parentless dialog is a client bug; centre it on the area
                // rather than dropping it, or the user gets an invisible modal.
                out.push(Placed {
                    window: w.id,
                    rect: centred(w.size, area),
                    placement: Placement::Floating,
                });
                continue;
            };
            let Some(anchor) = visible.iter().find(|p| p.window == parent) else {
                continue;
            };
            out.push(Placed {
                window: w.id,
                rect: centred(w.size, anchor.rect),
                placement: Placement::Floating,
            });
        }
        out
    }

    fn slot(&self, output: OutputId) -> Option<&OutputWindows> {
        self.outputs.iter().find(|s| s.output == output)
    }

    /// Outputs are created on first use rather than registered: the compositor
    /// learns about an output and its windows in one step, and a model that
    /// needed both to be announced in the right order would be a source of
    /// panics on hotplug.
    fn slot_mut(&mut self, output: OutputId) -> &mut OutputWindows {
        if let Some(i) = self.outputs.iter().position(|s| s.output == output) {
            return &mut self.outputs[i];
        }
        self.outputs.push(OutputWindows::new(output));
        self.outputs.last_mut().expect("just pushed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect::new(0, 40, 1920, 1000);
    const PHONE: Rect = Rect::new(0, 40, 360, 700);
    const A: OutputId = OutputId(0);
    const B: OutputId = OutputId(1);

    fn model(capacity: usize) -> WindowModel {
        let mut m = WindowModel::new();
        m.set_capacity(A, capacity);
        m
    }

    #[test]
    fn first_window_takes_a_tile_and_focus() {
        let mut m = model(2);
        let w = m.open_toplevel(A, "foot", "Terminal");
        assert_eq!(m.tiled(A), &[w]);
        assert_eq!(m.focused(), Some(w));
        assert!(m.waiting(A).is_empty());
    }

    #[test]
    fn a_window_over_the_limit_replaces_the_focused_tile() {
        // D-025, trap 1, as an executable test: tiling is not
        // "everything visible", and the newcomer takes the *focused* tile.
        let mut m = model(2);
        let a = m.open_toplevel(A, "foot", "Terminal");
        let b = m.open_toplevel(A, "firefox", "Firefox");
        m.focus(a);
        let c = m.open_toplevel(A, "gedit", "Notes");

        assert_eq!(m.tiled(A), &[c, b]);
        assert_eq!(m.waiting(A), vec![a]);
        assert_eq!(m.focused(), Some(c));
        // The displaced window still exists, on the bar, at its own size.
        assert!(m.get(a).is_some());
    }

    #[test]
    fn the_bottom_bar_keeps_its_order_when_tiles_swap() {
        // A chip that moves when windows swap is a chip you cannot hit twice.
        let mut m = model(1);
        let a = m.open_toplevel(A, "foot", "Terminal");
        let b = m.open_toplevel(A, "firefox", "Firefox");
        let c = m.open_toplevel(A, "gedit", "Notes");
        assert_eq!(m.bar(A), &[a, b, c]);
        m.activate(a);
        assert_eq!(m.bar(A), &[a, b, c]);
        assert_eq!(m.tiled(A), &[a]);
    }

    #[test]
    fn activating_a_waiting_window_swaps_it_into_the_focused_tile() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "foot", "Terminal");
        let b = m.open_toplevel(A, "firefox", "Firefox");
        let c = m.open_toplevel(A, "gedit", "Notes"); // takes b's tile (b focused)
        assert_eq!(m.tiled(A), &[a, c]);

        m.focus(a);
        assert!(m.activate(b));
        assert_eq!(m.tiled(A), &[b, c]);
        assert_eq!(m.waiting(A), vec![a]);
        assert_eq!(m.focused(), Some(b));
    }

    #[test]
    fn closing_a_tiled_window_promotes_the_longest_waiting_one() {
        let mut m = model(1);
        let a = m.open_toplevel(A, "foot", "Terminal");
        let b = m.open_toplevel(A, "firefox", "Firefox");
        assert_eq!(m.tiled(A), &[b]);
        m.close(b);
        assert_eq!(m.tiled(A), &[a], "the screen never keeps an empty tile");
        assert_eq!(m.focused(), Some(a));
    }

    #[test]
    fn closing_the_last_window_leaves_an_empty_model_not_a_panic() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "foot", "Terminal");
        assert_eq!(m.close(a), vec![a]);
        assert!(m.is_empty());
        assert_eq!(m.focused(), None);
        assert!(m.tiled(A).is_empty());
        assert!(m
            .layout(A, MONITOR, Split::EVEN, Gaps::default())
            .is_empty());
    }

    #[test]
    fn a_dialog_takes_no_tile_and_is_not_on_the_bar() {
        // Trap 2: the "Save as" window must never become a third tile.
        let mut m = model(2);
        let a = m.open_toplevel(A, "gedit", "Notes");
        let d = m
            .open_child(a, SurfaceRole::Dialog, Size::new(600, 400), "Zapisz jako")
            .expect("parent exists");
        assert_eq!(m.tiled(A), &[a]);
        assert_eq!(m.bar(A), &[a]);
        assert_eq!(m.focused(), Some(d));

        let placed = m.layout(A, MONITOR, Split::EVEN, Gaps::default());
        let dialog = placed.iter().find(|p| p.window == d).expect("placed");
        assert_eq!(dialog.placement, Placement::Floating);
        assert_eq!(dialog.rect.size, Size::new(600, 400));
    }

    #[test]
    fn a_popup_floats_but_does_not_steal_focus() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "gedit", "Notes");
        let p = m
            .open_child(a, SurfaceRole::Popup, Size::new(200, 300), "menu")
            .expect("parent exists");
        assert_eq!(m.focused(), Some(a), "a menu closing must not orphan focus");
        let placed = m.layout(A, MONITOR, Split::EVEN, Gaps::default());
        assert_eq!(
            placed.iter().find(|q| q.window == p).map(|q| q.placement),
            Some(Placement::Floating)
        );
    }

    #[test]
    fn closing_a_parent_closes_its_children() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "gedit", "Notes");
        let d = m
            .open_child(a, SurfaceRole::Dialog, Size::new(600, 400), "Zapisz jako")
            .unwrap();
        let p = m
            .open_child(d, SurfaceRole::Popup, Size::new(200, 200), "menu")
            .unwrap();
        let gone = m.close(a);
        assert_eq!(gone.len(), 3);
        assert!(gone.contains(&d) && gone.contains(&p));
        assert!(m.is_empty());
    }

    #[test]
    fn a_child_of_a_waiting_window_is_not_drawn() {
        let mut m = model(1);
        let a = m.open_toplevel(A, "gedit", "Notes");
        let d = m
            .open_child(a, SurfaceRole::Dialog, Size::new(300, 200), "Zapisz jako")
            .unwrap();
        m.open_toplevel(A, "foot", "Terminal"); // pushes a off the tile
        let placed = m.layout(A, MONITOR, Split::EVEN, Gaps::default());
        assert!(placed.iter().all(|p| p.window != d));
    }

    #[test]
    fn a_window_that_does_not_fit_its_tile_floats_but_keeps_its_slot() {
        // Trap 3: respect set_min_size instead of squeezing.
        let mut m = model(2);
        let a = m.open_toplevel(A, "big", "Wielkie");
        let b = m.open_toplevel(A, "foot", "Terminal");
        m.get_mut(a).unwrap().min_size = Size::new(1200, 900);

        let placed = m.layout(A, PHONE, Split::EVEN, Gaps::default());
        let big = placed.iter().find(|p| p.window == a).expect("still placed");
        assert_eq!(big.placement, Placement::Floating);
        assert!(big.rect.size.w >= 1200, "never squeezed below its minimum");
        assert!(placed.iter().any(|p| p.window == b));
    }

    #[test]
    fn a_fullscreen_window_covers_the_application_area() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "mpv", "Film");
        m.get_mut(a).unwrap().fullscreen = true;
        let placed = m.layout(A, MONITOR, Split::EVEN, Gaps::default());
        assert_eq!(placed[0].placement, Placement::Floating);
        assert_eq!(placed[0].rect.size, MONITOR.size);
    }

    #[test]
    fn shrinking_the_capacity_spills_windows_onto_the_bar() {
        // A monitor's zone getting narrower — a resize, not a hotplug.
        let mut m = model(3);
        let a = m.open_toplevel(A, "a", "A");
        let b = m.open_toplevel(A, "b", "B");
        let c = m.open_toplevel(A, "c", "C");
        assert_eq!(m.tiled(A), &[a, b, c]);
        m.set_capacity(A, 1);
        assert_eq!(m.tiled(A), &[a]);
        assert_eq!(m.waiting(A), vec![b, c]);
    }

    #[test]
    fn growing_the_capacity_pulls_waiting_windows_back_in() {
        let mut m = model(1);
        let a = m.open_toplevel(A, "a", "A");
        let b = m.open_toplevel(A, "b", "B");
        assert_eq!(m.waiting(A), vec![a]);
        m.set_capacity(A, 2);
        assert_eq!(m.tiled(A), &[b, a]);
        assert!(m.waiting(A).is_empty());
    }

    #[test]
    fn capacity_zero_keeps_windows_alive_with_no_tiles() {
        // A degenerate area must not lose windows — the geometry will come back.
        let mut m = model(0);
        let a = m.open_toplevel(A, "a", "A");
        assert!(m.tiled(A).is_empty());
        assert_eq!(m.waiting(A), vec![a]);
        m.set_capacity(A, 2);
        assert_eq!(m.tiled(A), &[a]);
    }

    #[test]
    fn unplugging_an_output_moves_its_windows_to_the_remaining_one() {
        // D-026: the most common place a compositor panics, and with a dock it
        // happens every day.
        let mut m = WindowModel::new();
        m.set_capacity(A, 2);
        m.set_capacity(B, 2);
        let a = m.open_toplevel(A, "a", "A");
        let b = m.open_toplevel(B, "b", "B");
        let c = m.open_toplevel(B, "c", "C");

        m.migrate(B, A);
        assert!(m.tiled(B).is_empty() && m.bar(B).is_empty());
        assert_eq!(m.bar(A), &[a, b, c]);
        assert_eq!(m.tiled(A), &[a, b], "capacity still applies after the move");
        assert_eq!(m.waiting(A), vec![c]);
        assert!(m.iter().all(|w| w.output == A));
    }

    #[test]
    fn migrating_to_the_same_output_or_from_an_unknown_one_is_a_no_op() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "a", "A");
        m.migrate(A, A);
        m.migrate(OutputId(99), A);
        assert_eq!(m.tiled(A), &[a]);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn a_dialog_migrates_with_its_parent() {
        let mut m = WindowModel::new();
        m.set_capacity(A, 2);
        m.set_capacity(B, 2);
        let a = m.open_toplevel(B, "gedit", "Notes");
        let d = m
            .open_child(a, SurfaceRole::Dialog, Size::new(300, 200), "Zapisz")
            .unwrap();
        m.migrate(B, A);
        assert_eq!(m.get(d).map(|w| w.output), Some(A));
        assert!(m
            .layout(A, MONITOR, Split::EVEN, Gaps::default())
            .iter()
            .any(|p| p.window == d));
    }

    #[test]
    fn tiles_come_back_from_layout_in_tile_order() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "a", "A");
        let b = m.open_toplevel(A, "b", "B");
        let placed = m.layout(A, MONITOR, Split::EVEN, Gaps { outer: 0, inner: 0 });
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].window, a);
        assert_eq!(placed[0].rect, Rect::new(0, 40, 960, 1000));
        assert_eq!(placed[1].window, b);
        assert_eq!(placed[1].rect, Rect::new(960, 40, 960, 1000));
    }

    #[test]
    fn an_unknown_window_cannot_be_focused_or_activated() {
        // Everything reachable from a protocol handler must reject a stale id
        // by returning false, never by panicking (docs/04, resilience).
        let mut m = model(2);
        assert!(!m.focus(WindowId(42)));
        assert!(!m.activate(WindowId(42)));
        assert!(m
            .open_child(WindowId(42), SurfaceRole::Dialog, Size::new(1, 1), "x")
            .is_none());
        assert_eq!(m.close(WindowId(42)), vec![WindowId(42)]);
    }

    #[test]
    fn ids_are_never_reused_after_a_close() {
        let mut m = model(2);
        let a = m.open_toplevel(A, "a", "A");
        m.close(a);
        let b = m.open_toplevel(A, "b", "B");
        assert_ne!(a, b, "a stale protocol reference must not hit a new window");
    }
}
