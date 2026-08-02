//! The server side of the protocol: the socket, the globals, and the map from
//! wayland surfaces to the window model in core.
//!
//! Everything here is translation. What a new window does to the layout is
//! decided by [`gostui_core::window::WindowModel`] and tested without a
//! compositor (D-016); this module's job is to turn `xdg_toplevel` requests
//! into calls on that model, and the model's answers back into `configure`
//! events.
//!
//! # Why the globals live behind a generic parameter
//!
//! smithay's state constructors are generic over the type that implements the
//! protocol dispatch — in practice the event loop's data type, which belongs to
//! whichever backend is running. Naming it here would make the protocol depend
//! on the backend, which is exactly backwards: the socket is the same on a
//! nested window and on a tty. Hence the `D` parameter and its long list of
//! bounds. The bounds are smithay's, not ours, and the delegate macros in
//! [`handlers`] are what satisfy them.

pub mod handlers;

use gostui_core::layout::{Gaps, Split};
use gostui_core::{OutputId, Rect, Size, WindowId, WindowModel};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_wm_base::XdgWmBase;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_protocols::wp::primary_selection::zv1::server::zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1 as PrimaryDeviceManager;
use smithay::reexports::wayland_server::protocol::wl_compositor::WlCompositor;
use smithay::reexports::wayland_server::protocol::wl_data_device_manager::WlDataDeviceManager;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_shm::WlShm;
use smithay::reexports::wayland_server::protocol::wl_shm_pool::WlShmPool;
use smithay::reexports::wayland_server::protocol::wl_subcompositor::WlSubcompositor;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Dispatch, DisplayHandle, GlobalDispatch};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::output::WlOutputData;
use smithay::wayland::selection::data_device::{DataDeviceHandler, DataDeviceState};
use smithay::wayland::selection::primary_selection::{
    PrimaryDeviceManagerGlobalData, PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::reexports::wayland_protocols::wp::pointer_constraints::zv1::server::{
    zwp_confined_pointer_v1::ZwpConfinedPointerV1, zwp_locked_pointer_v1::ZwpLockedPointerV1,
    zwp_pointer_constraints_v1::ZwpPointerConstraintsV1,
};
use smithay::reexports::wayland_protocols::wp::relative_pointer::zv1::server::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::ZwpRelativePointerV1,
};
use smithay::wayland::pointer_constraints::{
    PointerConstraintUserData, PointerConstraintsHandler, PointerConstraintsState,
};
use smithay::wayland::relative_pointer::{RelativePointerManagerState, RelativePointerUserData};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use smithay::wayland::shell::xdg::decoration::{XdgDecorationManagerGlobalData, XdgDecorationState};
use smithay::wayland::shell::xdg::{PopupSurface, PositionerState, ToplevelSurface, XdgShellState};
use smithay::wayland::shm::{ShmHandler, ShmPoolUserData, ShmState};

/// The socket name we ask for, so that the documented development command line
/// (`WAYLAND_DISPLAY=wayland-gostui foot`) is the one that works. If the name is
/// taken — a second compositor already running — we fall back to an automatic
/// one rather than refusing to start.
const PREFERRED_SOCKET: &str = "wayland-gostui";

/// Per-client state. smithay stores this alongside the connection and drops it
/// when the client goes away, which is what makes surface cleanup automatic.
#[derive(Debug, Default)]
pub struct ClientState {
    pub compositor: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, id: ClientId) {
        tracing::debug!(?id, "client connected");
    }

    fn disconnected(&self, id: ClientId, reason: DisconnectReason) {
        // A client dying is normal traffic, not an error. It is logged at debug
        // because a protocol error kills the *client* and must never look like
        // a compositor fault (resilience rule).
        tracing::debug!(?id, ?reason, "client disconnected");
    }
}

/// One mapped `xdg_toplevel` and the window it became in the model.
///
/// The pair is kept here rather than in the model because a `ToplevelSurface` is
/// a protocol object, and core does not get to see those.
#[derive(Debug)]
struct Mapped {
    toplevel: ToplevelSurface,
    window: WindowId,
}

/// The same for a popup: a menu, a tooltip, a combo box drop-down.
///
/// Kept in its own list rather than folded into [`Mapped`] because a popup is a
/// different protocol object with a different lifetime — it appears and vanishes
/// several times a minute while its parent stays put — and because a popup never
/// appears on the bottom bar, never takes a tile and never survives its parent.
#[derive(Debug)]
struct MappedPopup {
    popup: PopupSurface,
    window: WindowId,
}

/// The protocol side of the compositor: globals, the surface map, and the
/// output clients are told about.
#[derive(Debug)]
pub struct Wayland {
    pub display: DisplayHandle,
    pub compositor: CompositorState,
    pub shm: ShmState,
    pub xdg_shell: XdgShellState,
    /// The clipboard and the primary (middle-click) selection. Copy-paste
    /// between two clients is an M2 criterion, not a later nicety.
    pub data_device: DataDeviceState,
    pub primary_selection: PrimarySelectionState,
    /// Relative motion and pointer locking. Core input rather than a nicety for
    /// games (D-022): the pointer mode that makes a phone usable with desktop
    /// applications is built out of exactly these two.
    ///
    /// Held only to keep the globals alive — clients reach them through the
    /// seat, and nothing here is called after construction.
    #[allow(dead_code, reason = "the globals live as long as these values do")]
    pub relative_pointer: RelativePointerManagerState,
    #[allow(dead_code, reason = "the globals live as long as these values do")]
    pub pointer_constraints: PointerConstraintsState,
    /// The `wl_output` global. Clients position menus and pick scale factors
    /// from this, so it exists even in the nested window where "the output" is
    /// somebody else's window.
    pub output: Output,
    /// Which output in core's collection the above corresponds to.
    pub output_id: OutputId,
    /// Server-side decorations. Tiles do not draw their own frames (D-025), so
    /// a client that offers to draw one is told not to.
    #[allow(dead_code, reason = "the global lives as long as this value does")]
    pub decoration: XdgDecorationState,
    toplevels: Vec<Mapped>,
    popups: Vec<MappedPopup>,
}

/// The display list's side of the surface map: a slot's opaque id is a
/// [`WindowId`], and this is the one place that knows it.
impl crate::render::SurfaceSource for Wayland {
    fn surface(&self, id: u64) -> Option<&WlSurface> {
        let id = u32::try_from(id).ok()?;
        self.surface_of(WindowId(id))
    }
}

impl Wayland {
    /// Create the globals. The socket is added separately by the backend, which
    /// owns the event loop.
    pub fn new<D>(display: &DisplayHandle, output_id: OutputId, size: Size, name: &str) -> Self
    where
        D: GlobalDispatch<WlCompositor, ()>
            + GlobalDispatch<WlSubcompositor, ()>
            + GlobalDispatch<WlShm, ()>
            + Dispatch<WlShm, ()>
            + Dispatch<WlShmPool, ShmPoolUserData>
            + GlobalDispatch<XdgWmBase, ()>
            + GlobalDispatch<WlOutput, WlOutputData>
            + GlobalDispatch<WlDataDeviceManager, ()>
            + GlobalDispatch<PrimaryDeviceManager, PrimaryDeviceManagerGlobalData>
            + BufferHandler
            + ShmHandler
            + DataDeviceHandler
            + PrimarySelectionHandler
            + PointerConstraintsHandler
            + GlobalDispatch<ZwpRelativePointerManagerV1, ()>
            + Dispatch<ZwpRelativePointerManagerV1, ()>
            + Dispatch<ZwpRelativePointerV1, RelativePointerUserData<D>>
            + GlobalDispatch<ZwpPointerConstraintsV1, ()>
            + Dispatch<ZwpPointerConstraintsV1, ()>
            + Dispatch<ZwpConfinedPointerV1, PointerConstraintUserData<D>>
            + Dispatch<ZwpLockedPointerV1, PointerConstraintUserData<D>>
            + GlobalDispatch<ZxdgDecorationManagerV1, XdgDecorationManagerGlobalData>
            + Dispatch<ZxdgDecorationManagerV1, ()>
            + 'static,
    {
        let compositor = CompositorState::new::<D>(display);
        // No extra formats beyond the two the protocol requires. Adding formats
        // we cannot actually convert would be advertising a lie.
        let shm = ShmState::new::<D>(display, []);
        let xdg_shell = XdgShellState::new::<D>(display);
        // The clipboard, and the middle-click one next to it. Not a late
        // refinement: `foot` refuses to start without `wl_data_device_manager`,
        // which is the protocol telling us what the M2 criterion already says —
        // a compositor without a clipboard is not usable, it is a demo.
        let data_device = DataDeviceState::new::<D>(display);
        let primary_selection = PrimarySelectionState::new::<D>(display);
        let relative_pointer = RelativePointerManagerState::new::<D>(display);
        let pointer_constraints = PointerConstraintsState::new::<D>(display);
        let decoration = XdgDecorationState::new::<D>(display);

        let output = Output::new(
            name.to_string(),
            PhysicalProperties {
                // A nested window has no physical size; zero is the honest
                // answer and clients treat it as "unknown".
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "GOST".into(),
                model: "GostUI".into(),
            },
        );
        output.create_global::<D>(display);
        let mut wayland = Self {
            display: display.clone(),
            compositor,
            shm,
            xdg_shell,
            data_device,
            primary_selection,
            relative_pointer,
            pointer_constraints,
            output,
            output_id,
            decoration,
            toplevels: Vec::new(),
            popups: Vec::new(),
        };
        wayland.resize_output(size, 1);
        wayland
    }

    /// Tell clients the output's size and scale.
    ///
    /// Called on every resize of the nested window: to a client this looks like
    /// a monitor changing mode, which is a thing clients must handle anyway
    /// (D-026) and which we therefore exercise from the first day.
    pub fn resize_output(&mut self, size: Size, scale: i32) {
        let mode = Mode {
            size: (size.w, size.h).into(),
            // Refresh is in mHz. A nested window has no refresh rate of its own;
            // 60 Hz is what the host almost certainly runs at and clients only
            // use it for pacing hints.
            refresh: 60_000,
        };
        self.output.change_current_state(
            Some(mode),
            Some(smithay::utils::Transform::Normal),
            Some(smithay::output::Scale::Integer(scale.max(1))),
            None,
        );
        self.output.set_preferred(mode);
    }

    /// Register a newly mapped toplevel and open the matching window in the
    /// model. Returns the window id so the caller can configure it.
    ///
    /// A toplevel that names a parent is a **dialog**, not a second application
    /// window: "Save as", "Preferences", an alert. It floats over its parent and
    /// takes no tile (D-025, trap 2) — tiling a file chooser is the single most
    /// common way a tiling compositor becomes unusable, so the distinction is
    /// made here, at the only moment the parent is known, rather than guessed
    /// later from the size or the title.
    pub fn map_toplevel(&mut self, model: &mut WindowModel, toplevel: ToplevelSurface) -> WindowId {
        let (app_id, title) = identity(&toplevel);
        let parent = toplevel
            .parent()
            .and_then(|surface| self.window_of(&surface));
        let window = match parent {
            Some(parent) => model
                .open_child(
                    parent,
                    gostui_core::SurfaceRole::Dialog,
                    // The dialog's own idea of its size. A client that has not
                    // said yet gets something reasonable to be centred at, and
                    // corrects it on its first commit.
                    Size::new(480, 320),
                    title.clone(),
                )
                // A parent that vanished between the request and here: fall back
                // to an ordinary window rather than dropping the surface, which
                // would leave the client waiting for a configure forever.
                .unwrap_or_else(|| model.open_toplevel(self.output_id, app_id.clone(), title)),
            None => model.open_toplevel(self.output_id, app_id, title),
        };
        self.toplevels.push(Mapped { toplevel, window });
        window
    }

    /// Register a popup and place it where its positioner says.
    ///
    /// `area` is the zone the popup must stay inside — the application zone, not
    /// the whole output, because a menu that slid under the top bar would be a
    /// menu with unreachable entries.
    ///
    /// The placement itself is the protocol's, computed by smithay: anchor,
    /// gravity, and the prescribed order of adjustments when the result does not
    /// fit (flip, then slide, then resize). We supply the parent's rectangle and
    /// keep the answer (D-041's reasoning applied to geometry: one stored answer,
    /// so the picture and the hit test cannot disagree).
    pub fn map_popup(
        &mut self,
        model: &mut WindowModel,
        popup: PopupSurface,
        positioner: PositionerState,
        parent_rect: Option<Rect>,
    ) -> Option<WindowId> {
        let parent_surface = popup.get_parent_surface()?;
        let parent = self.window_of(&parent_surface)?;
        let geometry = positioner.get_geometry();
        let size = Size::new(geometry.size.w.max(1), geometry.size.h.max(1));
        let window = model.open_child(parent, gostui_core::SurfaceRole::Popup, size, "")?;
        self.popups.push(MappedPopup { popup, window });
        self.position_popup(model, window, positioner, parent_rect);
        Some(window)
    }

    /// Work out where a popup goes and tell both the model and the client.
    pub fn position_popup(
        &self,
        model: &mut WindowModel,
        window: WindowId,
        positioner: PositionerState,
        parent_rect: Option<Rect>,
    ) {
        let Some(mapped) = self.popups.iter().find(|p| p.window == window) else {
            return;
        };
        let geometry = match parent_rect {
            // The area the popup may occupy, expressed relative to its parent —
            // which is the coordinate space `xdg_positioner` works in.
            Some(rect) => positioner.get_unconstrained_geometry(smithay::utils::Rectangle::new(
                (-rect.x(), -rect.y()).into(),
                (rect.w().max(1), rect.h().max(1)).into(),
            )),
            None => positioner.get_geometry(),
        };
        mapped.popup.with_pending_state(|state| {
            state.geometry = geometry;
            state.positioner = positioner;
        });
        model.set_position(
            window,
            Rect::new(
                geometry.loc.x,
                geometry.loc.y,
                geometry.size.w.max(1),
                geometry.size.h.max(1),
            ),
        );
    }

    /// Forget a popup and close its window.
    pub fn unmap_popup(&mut self, model: &mut WindowModel, surface: &WlSurface) -> Vec<WindowId> {
        let Some(i) = self
            .popups
            .iter()
            .position(|p| p.popup.wl_surface() == surface)
        else {
            return Vec::new();
        };
        let gone = model.close(self.popups[i].window);
        self.popups.retain(|p| !gone.contains(&p.window));
        self.toplevels.retain(|m| !gone.contains(&m.window));
        gone
    }

    /// Forget a toplevel and close its window. Returns every window the model
    /// removed — a parent takes its dialogs with it.
    pub fn unmap_toplevel(
        &mut self,
        model: &mut WindowModel,
        surface: &WlSurface,
    ) -> Vec<WindowId> {
        let Some(i) = self
            .toplevels
            .iter()
            .position(|m| m.toplevel.wl_surface() == surface)
        else {
            return Vec::new();
        };
        let gone = model.close(self.toplevels[i].window);
        self.toplevels.retain(|m| !gone.contains(&m.window));
        // A parent takes its dialogs *and* its menus with it: a popup outliving
        // the window it belongs to is a menu floating over nothing.
        self.popups.retain(|p| !gone.contains(&p.window));
        gone
    }

    pub fn window_of(&self, surface: &WlSurface) -> Option<WindowId> {
        if let Some(m) = self
            .toplevels
            .iter()
            .find(|m| m.toplevel.wl_surface() == surface)
        {
            return Some(m.window);
        }
        self.popups
            .iter()
            .find(|p| p.popup.wl_surface() == surface)
            .map(|p| p.window)
    }

    /// The `wl_surface` a window is drawn from, for the code that routes input
    /// to it and for the renderer that looks for its pixels.
    ///
    /// Popups are searched too: a menu is a surface like any other, and one that
    /// the renderer could not resolve would be a menu nobody can see.
    pub fn surface_of(&self, window: WindowId) -> Option<&WlSurface> {
        if let Some(t) = self.toplevel_of(window) {
            return Some(t.wl_surface());
        }
        self.popups
            .iter()
            .find(|p| p.window == window)
            .map(|p| p.popup.wl_surface())
    }

    pub fn toplevel_of(&self, window: WindowId) -> Option<&ToplevelSurface> {
        self.toplevels
            .iter()
            .find(|m| m.window == window)
            .map(|m| &m.toplevel)
    }

    /// Copy the client-owned parts of a toplevel's state into the model.
    ///
    /// Titles, app ids, minimum sizes **and the parent** arrive as protocol
    /// requests at times of the client's choosing, so they are re-read on commit
    /// rather than trusted from map time.
    pub fn refresh_metadata(&self, model: &mut WindowModel, window: WindowId) {
        let Some(toplevel) = self.toplevel_of(window) else {
            return;
        };
        let (app_id, title) = identity(toplevel);
        let min = min_size(toplevel);
        if let Some(w) = model.get_mut(window) {
            w.app_id = app_id;
            w.title = title;
            w.min_size = min;
        }

        // The parent belongs in this list and used to be read only at map time,
        // which was wrong for a reason worth remembering: `set_parent` is sent
        // **after** `get_toplevel`, so at creation every window looks parentless.
        // A file chooser was therefore born as an ordinary window and tiled
        // beside the window that opened it — trap 2 of D-025, reached by a side
        // door. The model refuses a cycle, so a client naming nonsense here
        // changes nothing.
        if let Some(parent) = toplevel.parent().and_then(|s| self.window_of(&s)) {
            model.adopt_as_dialog(window, parent);
        }
    }

    /// Send every visible window the size the layout gave it.
    ///
    /// This is the whole of "the compositor decides, the client obeys" (D-025).
    /// `send_pending_configure` sends nothing when the state has not changed, so
    /// re-running this after an unrelated redraw costs no protocol traffic —
    /// which matters, because a configure the client answers is a round trip and
    /// a repaint.
    pub fn configure(
        &self,
        model: &WindowModel,
        area: Rect,
        screen: Rect,
        split: Split,
        gaps: Gaps,
    ) {
        let placed = model.layout(self.output_id, area, screen, split, gaps);
        let focus = model.focused();
        for p in &placed {
            let Some(toplevel) = self.toplevel_of(p.window) else {
                continue;
            };
            toplevel.with_pending_state(|state| {
                state.size = Some((p.rect.w(), p.rect.h()).into());
                // Bounds tell a client how big it may usefully ask to be. It is
                // the polite half of tiling: a client that knows the bound picks
                // a sane default size instead of asking for 1920x1080 on a phone.
                state.bounds = Some((area.w(), area.h()).into());
                set(state, XdgState::Activated, Some(p.window) == focus);
                // Tiled edges are how a client knows not to draw rounded corners
                // and shadows on sides that touch another window. GTK and Qt both
                // read them, and without them tiled windows grow useless margins.
                let tiled = p.placement == gostui_core::Placement::Tiled;
                for edge in [
                    XdgState::TiledLeft,
                    XdgState::TiledRight,
                    XdgState::TiledTop,
                    XdgState::TiledBottom,
                ] {
                    set(state, edge, tiled);
                }
                set(
                    state,
                    XdgState::Fullscreen,
                    model.get(p.window).is_some_and(|w| w.fullscreen),
                );
            });
            toplevel.send_pending_configure();
        }

        // A window that lost its tile is not drawn, so it must not think it is
        // focused: an activated window off screen keeps drawing a focus ring and,
        // worse, keeps a text cursor blinking — which is a wake-up per blink.
        for id in model.waiting(self.output_id) {
            let Some(toplevel) = self.toplevel_of(id) else {
                continue;
            };
            toplevel.with_pending_state(|state| set(state, XdgState::Activated, false));
            toplevel.send_pending_configure();
        }
    }

    /// Titles for the bottom bar, in the model's stable bar order.
    pub fn bar_titles(&self, model: &WindowModel) -> Vec<String> {
        model
            .bar(self.output_id)
            .iter()
            .filter_map(|id| model.get(*id))
            .map(|w| w.title.clone())
            .collect()
    }

    /// Index into [`bar_titles`](Self::bar_titles) of the focused window.
    pub fn focused_bar_index(&self, model: &WindowModel) -> Option<usize> {
        let focus = model.focused()?;
        model.bar(self.output_id).iter().position(|id| *id == focus)
    }

    /// The socket name to advertise, and the source that accepts connections.
    pub fn socket() -> Result<(String, smithay::wayland::socket::ListeningSocketSource), String> {
        use smithay::wayland::socket::ListeningSocketSource;
        match ListeningSocketSource::with_name(PREFERRED_SOCKET) {
            Ok(source) => Ok((PREFERRED_SOCKET.to_string(), source)),
            Err(e) => {
                tracing::warn!("gniazdo `{PREFERRED_SOCKET}` zajęte ({e}), biorę automatyczne");
                let source = ListeningSocketSource::new_auto()
                    .map_err(|e| format!("nie udało się otworzyć gniazda wayland: {e}"))?;
                let name = source.socket_name().to_string_lossy().into_owned();
                Ok((name, source))
            }
        }
    }
}

use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as XdgState;
use smithay::wayland::shell::xdg::ToplevelState;

/// Add or remove a state flag. smithay's set has separate calls for the two, and
/// every configure needs both halves — a flag that is only ever added is a
/// window that can never lose focus.
fn set(state: &mut ToplevelState, flag: XdgState, on: bool) {
    if on {
        state.states.set(flag);
    } else {
        state.states.unset(flag);
    }
}

/// The client's app id and title, with placeholders for a client that has sent
/// neither yet — which is the normal state during the first few milliseconds.
fn identity(toplevel: &ToplevelSurface) -> (String, String) {
    use smithay::wayland::compositor::with_states;
    use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
    with_states(toplevel.wl_surface(), |states| {
        let Some(data) = states.data_map.get::<XdgToplevelSurfaceData>() else {
            return (String::new(), String::from("Okno"));
        };
        // A client that dies mid-lock would poison this; treat it as unnamed
        // rather than propagating a panic into the compositor.
        let Ok(data) = data.lock() else {
            return (String::new(), String::from("Okno"));
        };
        (
            data.app_id.clone().unwrap_or_default(),
            data.title.clone().unwrap_or_else(|| String::from("Okno")),
        )
    })
}

/// The window geometry a client declared with `xdg_surface.set_window_geometry`.
///
/// **The offset that has to be honoured or nothing lines up.** A client's buffer
/// is not always its window: one that draws its own decorations puts shadows and
/// rounded corners outside the geometry, so the buffer starts *above and left of*
/// the window the user sees. Everything that translates between screen and
/// surface has to add this back — a picture drawn without it is shifted, and a
/// click routed without it lands somewhere the user did not click.
///
/// `None` when the client has not set one, which means "the whole buffer" and
/// therefore an offset of zero.
pub fn window_geometry(surface: &WlSurface) -> Option<Rect> {
    use smithay::wayland::compositor::with_states;
    use smithay::wayland::shell::xdg::SurfaceCachedState;
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
            .map(|g| Rect::new(g.loc.x, g.loc.y, g.size.w, g.size.h))
    })
}

/// The client's `set_min_size`, or 1x1 when it has not asked for one.
///
/// Never trusted blindly: a client may send a minimum larger than any screen,
/// and the answer to that is to float it (D-025 trap 3), which the model does.
fn min_size(toplevel: &ToplevelSurface) -> Size {
    use smithay::wayland::compositor::with_states;
    use smithay::wayland::shell::xdg::SurfaceCachedState;
    with_states(toplevel.wl_surface(), |states| {
        // The minimum is double-buffered like the rest of the surface state, so
        // it is read from the *current* half — the pending one may describe a
        // size the client has not committed to yet.
        let min = states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .min_size;
        // Zero on an axis means "unconstrained" in the protocol, not "zero".
        Size::new(min.w.max(1), min.h.max(1))
    })
}
