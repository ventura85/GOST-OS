//! smithay's handler traits, implemented for the running state.
//!
//! Every function here is short on purpose. A handler receives a protocol event,
//! calls the model, and asks for a redraw; anything longer than that is logic
//! that belongs in `gostui-core` where it can be tested without a client
//! (D-016). If a handler in this file ever grows a branch about *where* a window
//! goes, it is in the wrong file.
//!
//! **No `unwrap` on anything a client sent.** A malformed request must kill the
//! client and leave the compositor running — losing a text editor is an
//! annoyance, losing the compositor is losing every open program at once.
//!
//! The impls name the winit backend's state because that is the only event loop
//! there is. When the DRM backend lands (M4) the two loop states get a shared
//! trait and these impls follow it; nothing about the protocol side changes.

use crate::backend::winit::State;
use crate::wayland::ClientState;
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::input::pointer::{CursorImageStatus, PointerHandle};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Client;
use smithay::utils::{Logical, Point, Serial};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::pointer_constraints::{
    with_pointer_constraint, PointerConstraint, PointerConstraintsHandler,
};
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_output, delegate_pointer_constraints,
    delegate_primary_selection, delegate_relative_pointer, delegate_seat, delegate_shm,
    delegate_xdg_decoration, delegate_xdg_shell,
};

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.wayland.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // The per-client state is created when the client is inserted, so a
        // missing one would be our bug and not the client's. Falling back to a
        // fresh default would silently break surface tracking, so this is one of
        // the few places where a panic is the honest answer — and it cannot be
        // triggered from the far side of the socket.
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with a ClientState")
            .compositor
    }

    fn commit(&mut self, surface: &WlSurface) {
        // Hand the newly attached buffer to smithay's surface state before
        // anything else looks at it. Both renderers read what this records —
        // the GPU path imports a texture from it, the CPU path copies its
        // memory — so a commit that skipped this is a window that never appears.
        on_commit_buffer_handler::<Self>(surface);

        let Some(window) = self.wayland.window_of(surface) else {
            // Subsurfaces and not-yet-mapped surfaces land here. They are not
            // windows and there is nothing to lay out.
            return;
        };
        // Titles and minimum sizes arrive whenever the client feels like it,
        // so they are re-read on commit rather than trusted from map time.
        self.wayland.refresh_metadata(&mut self.windows, window);
        self.sync_layout();
        self.request_redraw();
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {
        // Nothing to release yet: no buffer is imported until surfaces are
        // actually drawn. When they are, the texture cache is dropped here.
    }
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.wayland.shm
    }
}

impl OutputHandler for State {}

/// The seat: one keyboard, one pointer, one touch device, three separate focus
/// types.
///
/// They stay three types rather than one because touch must never be a renamed
/// pointer (D-020, D-022) — the pointer mode that makes desktop applications
/// usable with a finger is a *translation* between two of them, and it can only
/// be written cleanly if both exist.
impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {
        // The client is telling us what the cursor should look like over its
        // window. In the nested backend the cursor on screen belongs to the host
        // session and we cannot change it, so this is dropped rather than
        // half-honoured. It becomes real on the tty (M4), where nobody else is
        // drawing a cursor, and it is a requirement of the pointer mode (D-022).
    }
}

/// Pointer locking and confinement.
///
/// A client asks for these; the compositor decides. We grant a **lock** — the
/// cursor stands still and the client is told the movement instead, which is
/// what a game and the virtual trackpad of D-022 both want — and refuse
/// **confinement**, because clamping the pointer to a client-supplied region is
/// not implemented and a constraint that is activated but not enforced is worse
/// than one that was never granted.
impl PointerConstraintsHandler for State {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        // Only for the surface the pointer is actually on: granting a lock to a
        // window the user is not pointing at would let a background application
        // capture the cursor.
        if pointer.current_focus().as_ref() != Some(surface) {
            return;
        }
        with_pointer_constraint(surface, pointer, |constraint| {
            let Some(constraint) = constraint else { return };
            if matches!(&*constraint, PointerConstraint::Locked(_)) {
                constraint.activate();
            }
        });
    }

    fn cursor_position_hint(
        &mut self,
        _surface: &WlSurface,
        _pointer: &PointerHandle<Self>,
        _location: Point<f64, Logical>,
    ) {
        // Where the client would like the cursor to reappear when the lock ends.
        // Acting on it means moving a cursor we do not draw yet (M4).
    }
}

/// Decorations: ours, always.
///
/// A tiled window may not draw its own title bar, border or shadow (D-025) — the
/// frame is the compositor's business, the contents are the client's, and a
/// window that draws both ends up with two title bars in a tile that has room
/// for none. So every client is told `ServerSide`, whatever it asked for.
///
/// **This works for Qt and is ignored by GTK, and that is not a bug we can fix
/// here.** GTK does not implement `xdg-decoration` at all: it draws client-side
/// decorations unconditionally. The consequence is visible rather than harmful —
/// a GTK window in a tile carries a header bar it did not need — and the answer
/// is a decoration policy per application in the theme (M3), not an argument
/// with a toolkit.
impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.decorate(&toplevel);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: DecorationMode) {
        // The client is allowed to prefer; it is not allowed to decide.
        self.decorate(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.decorate(&toplevel);
    }
}

/// The clipboard.
///
/// Copy-paste between two clients working in both directions is an M2
/// criterion, and this is the whole of what it takes: smithay moves the data,
/// we only say where the selection state lives. The empty grab handlers are
/// drag-and-drop, which needs a pointer to start — the next step.
impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.wayland.data_device
    }
}

impl ClientDndGrabHandler for State {}
impl ServerDndGrabHandler for State {}

/// The middle-click selection, kept separate from the clipboard because X11 and
/// wayland both keep them separate and users rely on the difference.
impl PrimarySelectionHandler for State {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.wayland.primary_selection
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.wayland.xdg_shell
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = self.wayland.map_toplevel(&mut self.windows, surface);
        tracing::info!(?window, "toplevel mapped");
        // No configure yet: the protocol wants it in answer to the client's
        // first commit, and `commit` sends it through `sync_layout`. Configuring
        // here would be a size sent before the client asked to exist.
        self.request_redraw();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let gone = self
            .wayland
            .unmap_toplevel(&mut self.windows, surface.wl_surface());
        tracing::info!(count = gone.len(), "toplevel destroyed");
        // The tile the window held is filled by whatever was waiting, so the
        // remaining windows have to be told their new size.
        self.sync_layout();
        self.request_redraw();
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // A popup floats (D-025, trap 2) exactly where `xdg_positioner` says,
        // constrained to the application zone — a menu that slid under the top
        // bar would have entries nobody can click.
        let parent = self.parent_rect(&surface);
        let window = self
            .wayland
            .map_popup(&mut self.windows, surface.clone(), positioner, parent);
        match window {
            Some(w) => tracing::debug!(?w, "popup mapped"),
            None => tracing::debug!("popup with no parent we know: ignored"),
        }
        if let Err(e) = surface.send_configure() {
            // A refused configure means the popup is already gone. The client
            // gets the protocol error, we keep running.
            tracing::debug!("popup configure refused: {e}");
        }
        self.request_redraw();
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        let gone = self
            .wayland
            .unmap_popup(&mut self.windows, surface.wl_surface());
        tracing::debug!(count = gone.len(), "popup destroyed");
        self.request_redraw();
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // An explicit grab would take the pointer and keyboard away from
        // everything else until the menu closes. Not implemented yet, and the
        // honest consequence is that a menu closes on the client's own logic
        // rather than on a click elsewhere — visible, but not broken.
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        // A menu being re-anchored, typically because a submenu opened near an
        // edge. Same placement path as the first time, so the two cannot drift.
        let parent = self.parent_rect(&surface);
        if let Some(window) = self.wayland.window_of(surface.wl_surface()) {
            self.wayland
                .position_popup(&mut self.windows, window, positioner, parent);
        }
        surface.send_repositioned(token);
        self.request_redraw();
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.wayland.window_of(surface.wl_surface()) {
            self.wayland.refresh_metadata(&mut self.windows, window);
            // The bottom bar shows titles, so a rename is a reason to draw —
            // and the only reason. Nothing else on screen changed.
            self.request_redraw();
        }
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.wayland.window_of(surface.wl_surface()) {
            self.wayland.refresh_metadata(&mut self.windows, window);
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        if let Some(window) = self.wayland.window_of(surface.wl_surface()) {
            if let Some(w) = self.windows.get_mut(window) {
                w.fullscreen = true;
            }
            self.sync_layout();
            self.request_redraw();
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.wayland.window_of(surface.wl_surface()) {
            if let Some(w) = self.windows.get_mut(window) {
                w.fullscreen = false;
            }
            self.sync_layout();
            self.request_redraw();
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        // Every window is already as large as the layout allows, so "maximise"
        // has nothing to do — but the protocol requires an answer, and a client
        // that never gets one waits forever.
        surface.send_configure();
    }

    fn move_request(&mut self, _surface: ToplevelSurface, _seat: WlSeat, _serial: Serial) {
        // Windows are not dragged (D-025). Ignoring the request is the correct
        // answer, not a missing feature.
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: WlSeat,
        _serial: Serial,
        _edges: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
    ) {
        // Same: the divider is dragged, the window is not.
    }
}

delegate_compositor!(State);
delegate_shm!(State);
delegate_xdg_shell!(State);
delegate_output!(State);
delegate_seat!(State);
delegate_data_device!(State);
delegate_primary_selection!(State);
delegate_relative_pointer!(State);
delegate_pointer_constraints!(State);
delegate_xdg_decoration!(State);
