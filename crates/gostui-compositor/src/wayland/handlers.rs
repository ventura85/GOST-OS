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
use smithay::input::{SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Client;
use smithay::utils::Serial;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_output, delegate_primary_selection,
    delegate_seat, delegate_shm, delegate_xdg_shell,
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

/// The seat exists from the moment the socket does, even though nothing routes
/// input through it yet.
///
/// Not premature: `xdg_shell` requires a seat to compile at all — popup grabs
/// and interactive resize both take one — and a client that finds no `wl_seat`
/// concludes it has no keyboard and disables text entry entirely. Routing the
/// events is the next step; the three focus types are already separate because
/// touch must never be a renamed pointer (D-022).
impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
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

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        // Popups float (D-025 trap 2) and are positioned by their own
        // positioner, which is protocol data. Placing them properly is the next
        // step; sending the configure they are waiting for is what keeps a menu
        // from hanging in the meantime.
        if let Err(e) = surface.send_configure() {
            tracing::debug!("popup configure refused: {e}");
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // Pointer grabs need a seat, and the seat arrives with input. Until then
        // a menu simply does not grab — it still shows and still closes.
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
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
