//! The nested backend: GostUI as a window inside the session already running.
//!
//! This is the everyday development mode (docs/01 §2.2, Tier 1). A crash costs
//! one window instead of a session full of open programs, which is why it comes
//! before the tty backend and not after it.
//!
//! This is also where the wayland socket lives (M2). The backend owns the event
//! loop, so it owns the `Display` and the listening socket; what a client's
//! requests *mean* is in `crate::wayland`, and what they do to the layout is in
//! `gostui-core` (D-016).
//!
//! Note what is *absent*: a render loop. The window is redrawn when something
//! happens (a resize, an expose from the X server, a client mapping a window)
//! and at no other time. The "zero rendering at rest" rule is architectural; a
//! loop written here now would be copied into every backend that follows.

use crate::backend::RendererKind;
use crate::render::{self, ShellRenderer, SurfaceSource};
use crate::stats::{Cause, Stats};
use crate::wayland::{ClientState, Wayland};
use gostui_core::layout::tile_limit;
use gostui_core::shell::zones;
use gostui_core::{OutputId, Outputs, Rect, Size, Split, TabStrip, WindowModel};
use gostui_render::{display_list, ShellView, SurfaceSlot, TextRenderer};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{Frame, Renderer};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Display;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::window::WindowAttributes;
use smithay::utils::Transform;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Logical size of the nested window at start-up.
///
/// 1280×800 is not a target resolution — it is a window on somebody's desktop.
/// The layout is computed from the size that actually arrives, never from this.
const INITIAL_SIZE: (f64, f64) = (1280.0, 800.0);

/// How long `--idle-test` waits before it starts counting.
///
/// Opening a window is not resting. The host window manager maps it, hands it
/// its real size and asks for one repaint — on this station all of that lands
/// within ten milliseconds of start-up. Counting those frames as a failure
/// would measure X11 window management rather than our render policy, so the
/// measurement begins once the window has stopped being new.
const SETTLE: Duration = Duration::from_secs(1);

type Error = Box<dyn std::error::Error>;

/// Everything the nested backend owns while it runs.
///
/// `pub(crate)` because smithay's handler traits are implemented for it in
/// `crate::wayland::handlers`: the event loop's data type *is* the compositor as
/// far as the protocol is concerned.
pub(crate) struct State {
    backend: WinitGraphicsBackend<GlesRenderer>,
    renderer: Box<dyn ShellRenderer>,
    signal: LoopSignal,
    /// The user's appearance, loaded once at startup (D-032).
    theme: gostui_core::Theme,
    /// Placeholder tab strip. Real tabs come from the configuration in M3.
    tabs: TabStrip,
    /// Real windows, from real clients (D-025). Which one holds which tile is
    /// decided here and nowhere else.
    pub(crate) windows: WindowModel,
    /// The output collection. One entry in the nested window, but a collection
    /// from the first day: a session with a phone panel and a monitor is the
    /// target, not an extension (D-026).
    outputs: Outputs,
    output: OutputId,
    /// Where the divider between two tiles sits. Draggable in a later step;
    /// stored per session already so that dragging it has somewhere to write.
    split: Split,
    /// The protocol side: globals, the socket's clients, and the surface map.
    pub(crate) wayland: Wayland,
    /// The seat. It lives here rather than in `Wayland` because smithay's seat
    /// types are parameterised by the type that handles input — which is this
    /// one. Nothing is routed through it yet; it exists because a client that
    /// finds no `wl_seat` decides it has no keyboard.
    pub(crate) seat_state: SeatState<State>,
    #[allow(
        dead_code,
        reason = "the handle is used when input lands, in the next step"
    )]
    seat: Seat<State>,
    /// Something changed and the screen no longer matches the state.
    ///
    /// Handlers set this instead of drawing: a client can commit several times
    /// in one dispatch cycle, and drawing inside the handler would turn one
    /// state change into three frames. The flag is consumed once per loop pass.
    dirty: bool,
    /// The font system and glyph cache. One per process: it holds the whole
    /// font database, and a second copy would double the largest single cost
    /// text adds to the budget (D-029).
    text: TextRenderer,
    /// Last wall-clock reading, kept so a timer that fires early can be told
    /// apart from one that has real work to do.
    clock: gostui_core::clock::Wall,
    /// The string currently on screen. Re-formatted only when the minute turns.
    clock_text: String,
    /// Frame counts, causes and render timings — the measurement behind the
    /// "zero rendering at rest" criterion (docs/01 §4, step 6). Counting is
    /// always on; `GOSTUI_STATS` only decides whether it is printed.
    stats: Stats,
    /// When the window opened, and when the last frame was drawn. The two
    /// `Instant`s live here rather than in `Stats` so that the accounting stays
    /// testable without a clock.
    started: Instant,
    last_frame: Option<Instant>,
    /// Stop after this many frames. `None` outside of smoke tests.
    ///
    exit_after: Option<u64>,
    /// Frame counts as they stood when the measurement window opened.
    /// `None` until the settle timer fires, and always `None` without
    /// `--idle-test`.
    idle_watch: Option<IdleWatch>,
}

/// Where the frame counters stood when `--idle-test` started measuring.
///
/// All three numbers are needed, not just the total: a measurement window longer
/// than a minute contains a clock frame, and one with a client connected
/// contains client frames. Both are the system working, not a fault.
#[derive(Debug)]
struct IdleWatch {
    frames: u64,
    clock_frames: u64,
    client_frames: u64,
}

/// Open the window and run until it is closed.
///
/// `exit_after` closes the window after a given number of frames instead of
/// waiting for the user. It exists so that "the window opens and closes
/// cleanly" is something CI can assert rather than something somebody looked
/// at once.
///
/// `idle_test` closes it after a given duration instead, and is the other half
/// of the same idea: it turns "leave it alone for ten seconds and no frame is
/// drawn" from something to eyeball into a command with an exit code.
pub fn run(
    kind: RendererKind,
    exit_after: Option<u64>,
    idle_test: Option<Duration>,
) -> Result<(), Error> {
    init_tracing();

    let attributes = WindowAttributes::default()
        .with_title("GOST OS")
        .with_inner_size(LogicalSize::new(INITIAL_SIZE.0, INITIAL_SIZE.1))
        .with_visible(true);

    let (backend, source) = winit::init_from_attributes::<GlesRenderer>(attributes)?;
    let mut event_loop: EventLoop<State> = EventLoop::try_new()?;

    let size = backend.window_size();
    let mut tabs = TabStrip::new();
    for name in ["Pliki", "Praca", "Rozrywka"] {
        tabs.add(name);
    }
    let renderer = render::build(kind);
    let label = renderer.label();
    let now = crate::clock::now_local();

    // The output exists in core's collection before any client hears about it:
    // the nested window is an output like any other, and code that treated it as
    // "the screen" would have to be rewritten for the dock (D-026, D-035).
    let mut outputs = Outputs::new();
    let output = outputs.add("winit", Size::new(size.w, size.h));

    let display: Display<State> = Display::new()?;
    let dh = display.handle();
    let wayland = Wayland::new::<State>(&dh, output, Size::new(size.w, size.h), "GostUI-winit");

    let mut seat_state = SeatState::<State>::new();
    let mut seat = seat_state.new_wl_seat(&dh, "seat0");
    // 600 ms before a held key repeats, then 25 a second — the values X11 and
    // GNOME both settle on. The keymap itself comes from the user's environment
    // through libxkbcommon, so a Polish layout stays a Polish layout. A failure
    // here is not fatal: a shell without a keyboard is crippled, but a shell
    // that refuses to start is worse, and the message says which one happened.
    if let Err(e) = seat.add_keyboard(Default::default(), 600, 25) {
        tracing::error!("nie udało się utworzyć klawiatury (xkb): {e}");
    }
    seat.add_pointer();

    let mut state = State {
        backend,
        renderer,
        signal: event_loop.get_signal(),
        theme: crate::load_theme(),
        tabs,
        windows: WindowModel::new(),
        outputs,
        output,
        split: Split::EVEN,
        wayland,
        seat_state,
        seat,
        dirty: false,
        text: TextRenderer::new(),
        clock: now,
        clock_text: gostui_core::clock::format(now, gostui_core::ClockFormat::H24),
        stats: Stats::from_env(),
        started: Instant::now(),
        last_frame: None,
        exit_after,
        idle_watch: None,
    };

    if state.text.is_fontless() {
        tracing::warn!("no fonts found on this system; the shell will draw no text");
    }

    tracing::info!(
        width = size.w,
        height = size.h,
        scale = state.backend.scale_factor(),
        renderer = label,
        "nested window open"
    );

    // The winit event loop is a calloop source, so the process sleeps in
    // `poll` between events instead of spinning. That is the whole reason for
    // running calloop here before there is anything else to dispatch.
    event_loop
        .handle()
        .insert_source(source, |event, (), state| state.handle(event))
        .map_err(|e| format!("nie udało się wpiąć źródła winit do pętli: {e}"))?;

    // The socket. From here a client can connect, which is the whole of M2's
    // first step: `WAYLAND_DISPLAY=wayland-gostui wayland-info` now answers.
    let (socket_name, socket) = Wayland::socket()?;
    event_loop
        .handle()
        .insert_source(socket, move |stream, (), state: &mut State| {
            // A client that cannot be inserted is a client that goes away; the
            // compositor keeps running. This is the first place where "a broken
            // client must not be a broken session" is tested for real.
            if let Err(e) = state
                .wayland
                .display
                .insert_client(stream, Arc::new(ClientState::default()))
            {
                tracing::warn!("odrzucono klienta: {e}");
            }
        })
        .map_err(|e| format!("nie udało się wpiąć gniazda do pętli: {e}"))?;

    // Client requests. The `Display` lives inside the source rather than in the
    // state because dispatching needs `&mut` to both at once, and calloop hands
    // them to the callback separately for exactly this reason.
    event_loop
        .handle()
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state: &mut State| {
                // SAFETY: the display is not dropped or replaced here; only
                // dispatched. This is the borrow split described above.
                let result = unsafe { display.get_mut().dispatch_clients(state) };
                if let Err(e) = result {
                    // Never fatal. A dispatch error is one client misbehaving,
                    // and the compositor outliving its clients is the point.
                    tracing::warn!("błąd obsługi klienta: {e}");
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| format!("nie udało się wpiąć displayu wayland do pętli: {e}"))?;

    eprintln!("gniazdo wayland: WAYLAND_DISPLAY={socket_name}");
    tracing::info!(socket = %socket_name, "wayland socket listening");

    // The clock is the first thing in this shell that changes on its own, and
    // the timer is what keeps that from becoming a render loop. It sleeps until
    // the displayed minute is actually wrong — never a one-second poll asking
    // whether anything happened (docs/01 §4, step 6).
    let first_wait = Duration::from_secs(state.clock.until_next_minute());
    event_loop
        .handle()
        .insert_source(Timer::from_duration(first_wait), |_, (), state| {
            state.tick_clock();
            TimeoutAction::ToDuration(Duration::from_secs(state.clock.until_next_minute()))
        })
        .map_err(|e| format!("nie udało się wpiąć zegara do pętli: {e}"))?;

    // `--idle-test n`: two one-shot timers, not a poll. The first opens the
    // measurement window once the window has settled, the second ends the run.
    // Neither observes anything — everything they report was already counted.
    if let Some(window) = idle_test {
        tracing::info!(
            seconds = window.as_secs_f64(),
            settle = SETTLE.as_secs_f64(),
            "idle test: settling, then measuring"
        );
        event_loop
            .handle()
            .insert_source(Timer::from_duration(SETTLE), |_, (), state: &mut State| {
                state.begin_idle_watch();
                TimeoutAction::Drop
            })
            .map_err(|e| format!("nie udało się wpiąć testu spoczynku do pętli: {e}"))?;
        event_loop
            .handle()
            .insert_source(
                Timer::from_duration(SETTLE + window),
                |_, (), state: &mut State| {
                    state.signal.stop();
                    TimeoutAction::Drop
                },
            )
            .map_err(|e| format!("nie udało się wpiąć końca testu spoczynku do pętli: {e}"))?;
    }

    // The first frame: nothing has asked for a redraw yet, and an unfilled EGL
    // window shows whatever happened to be in the buffer.
    state.sync_layout();
    state.draw(Cause::Initial);

    // `EventLoop::run` clears the stop flag on entry, so a budget spent by the
    // frame above has to be caught here — signalling it would be swallowed.
    if !state.budget_spent() {
        // The callback runs once per loop pass, after everything dispatched.
        // This is where a state change becomes at most one frame, no matter how
        // many commits produced it — and where clients get their answers.
        event_loop.run(None, &mut state, |state| {
            if std::mem::take(&mut state.dirty) {
                state.draw(Cause::Client);
            }
            if let Err(e) = state.wayland.display.flush_clients() {
                tracing::warn!("nie udało się wysłać zdarzeń do klientów: {e}");
            }
        })?;
    }

    let uptime = state.started.elapsed();
    tracing::info!(frames = state.stats.frames(), "closing cleanly");
    if state.stats.enabled() {
        // Straight to stderr, not through `tracing`: this is a report somebody
        // asked for, and it should not disappear behind a log filter.
        eprintln!("{}", state.stats.report(uptime));
    }

    // The criterion, as an exit code.
    //
    // Only frames inside the measurement window count, and two kinds inside it
    // do not count against us: the specification's "leave it alone and the
    // counter stays at zero" is about frames drawn *for no reason*.
    //
    // - A minute turning is a reason (step 5).
    // - A client changing what is on screen is a reason (M2). This one is worth
    //   stating plainly, because it looks like a loophole and is not: the shell
    //   draws when an application maps, closes or renames a window, and a
    //   terminal with a blinking cursor will therefore produce frames for as
    //   long as it blinks. That is the client's wakeup, not ours. It is
    //   reported separately so that a client waking us up *too often* is still
    //   visible instead of being averaged into a total.
    //
    // Anything left over means either the window was touched or we are drawing
    // without being asked — the architectural break D-027 is about.
    if let Some(window) = idle_test {
        let Some(watch) = &state.idle_watch else {
            return Err(
                "test spoczynku nie wystartował: okno zamknięto przed końcem stabilizacji".into(),
            );
        };
        let drawn = state.stats.frames() - watch.frames;
        let from_clock = state.stats.count(Cause::Clock) - watch.clock_frames;
        let from_clients = state.stats.count(Cause::Client) - watch.client_frames;
        let unexplained = drawn - from_clock - from_clients;
        eprintln!(
            "test spoczynku: okno pomiarowe {:.1} s · klatek: {drawn} \
             (od zegara: {from_clock}, od klientów: {from_clients}, bez powodu: {unexplained})",
            window.as_secs_f64()
        );
        if unexplained > 0 {
            return Err(format!(
                "test spoczynku nieudany — klatek bez powodu: {unexplained} \
                 w ciągu {:.1} s. Okno było ruszane albo rysujemy bez potrzeby",
                window.as_secs_f64()
            )
            .into());
        }
    }
    Ok(())
}

impl State {
    /// Open the measurement window of `--idle-test`.
    ///
    /// Everything drawn up to this point was start-up; from here on a frame
    /// needs a reason.
    fn begin_idle_watch(&mut self) {
        self.idle_watch = Some(IdleWatch {
            frames: self.stats.frames(),
            clock_frames: self.stats.count(Cause::Clock),
            client_frames: self.stats.count(Cause::Client),
        });
        tracing::info!(
            frames_so_far = self.stats.frames(),
            "idle test: window settled, measuring from here"
        );
    }

    /// Re-read the clock and redraw only if the minute actually turned.
    ///
    /// The guard is not paranoia: a timer can fire a hair early, and redrawing
    /// the identical string would be a frame drawn for no reason — exactly what
    /// the frame counter is there to catch.
    fn tick_clock(&mut self) {
        let now = crate::clock::now_local();
        if now.same_minute(self.clock) {
            self.clock = now;
            return;
        }
        self.clock = now;
        self.clock_text = gostui_core::clock::format(now, gostui_core::ClockFormat::H24);
        self.draw(Cause::Clock);
    }

    /// Ask for one frame at the end of this loop pass.
    ///
    /// Never draws on the spot. Three commits from one client in one dispatch
    /// cycle are one change to look at, and turning them into three frames would
    /// break the rule the whole `Stats` module exists to police.
    pub(crate) fn request_redraw(&mut self) {
        self.dirty = true;
    }

    /// The logical area windows may use: the screen minus the two bars.
    fn app_zone(&self) -> Rect {
        let size = self.backend.window_size();
        let area = Rect::new(0, 0, size.w, size.h);
        zones(area, self.theme.metrics.bar_heights()).apps
    }

    /// Recompute how many tiles fit, then tell every client its size.
    ///
    /// Called after anything that can change the geometry or the set of
    /// windows. Both halves matter: the capacity is what pushes a window onto
    /// the bottom bar when the zone shrinks, and the configure is what makes the
    /// client actually resize.
    pub(crate) fn sync_layout(&mut self) {
        let area = self.app_zone();
        let gaps = self.theme.metrics.gaps();
        // The limit is core's arithmetic (D-025): three tiles on a wide
        // monitor, two normally, one when a second would be unusably narrow.
        self.windows
            .set_capacity(self.output, tile_limit(area, gaps));
        self.wayland
            .configure(&self.windows, area, self.split, gaps);
    }

    fn handle(&mut self, event: WinitEvent) {
        match event {
            WinitEvent::Resized { size, scale_factor } => {
                tracing::debug!(width = size.w, height = size.h, scale_factor, "resized");
                // A resized nested window is a monitor changing mode as far as
                // a client is concerned, and clients have to survive that
                // anyway (D-026). Telling them is not optional: a client that
                // never hears the new size keeps its old buffer.
                if let Some(o) = self.outputs.get_mut(self.output) {
                    o.mode_px = Size::new(size.w, size.h);
                }
                self.wayland.resize_output(Size::new(size.w, size.h), 1);
                self.sync_layout();
                self.draw(Cause::Resized);
            }
            WinitEvent::Redraw => self.draw(Cause::Redraw),
            WinitEvent::CloseRequested => {
                tracing::info!("window closed by the user");
                self.signal.stop();
            }
            // Input arrives in M2, together with `wl_seat` and xkbcommon.
            WinitEvent::Input(_) | WinitEvent::Focus(_) => {}
        }
    }

    /// Fill the window.
    ///
    /// A failed frame is logged and dropped, never propagated as a panic: on a
    /// nested backend a lost EGL context means the parent session took the GPU
    /// away, and the right answer is to survive until the next event.
    fn draw(&mut self, cause: Cause) {
        let began = Instant::now();
        if let Err(e) = self.try_draw() {
            // A dropped frame is not counted: the statistics describe frames
            // that reached the screen, and counting failures as renders would
            // quietly inflate the very number the criterion reads.
            tracing::warn!(cause = cause.label(), "frame dropped: {e}");
            return;
        }
        let render = began.elapsed();
        let since_previous = self.last_frame.map(|t| began.duration_since(t));
        self.last_frame = Some(began);
        self.stats.record(cause, render, since_previous);

        if self.stats.enabled() {
            tracing::info!(
                frame = self.stats.frames(),
                cause = cause.label(),
                render_us = render.as_micros() as u64,
                since_previous_ms = since_previous.map(|d| d.as_millis() as u64),
                "frame"
            );
        }

        if self.budget_spent() {
            tracing::info!(frames = self.stats.frames(), "frame budget spent, closing");
            self.signal.stop();
        }
    }

    fn budget_spent(&self) -> bool {
        self.exit_after.is_some_and(|n| self.stats.frames() >= n)
    }

    fn try_draw(&mut self) -> Result<(), Error> {
        let size = self.backend.window_size();
        let damage = smithay::utils::Rectangle::from_size(size);

        // Scale 1 in the nested window on purpose: the parent session's scale
        // factor is its business, and a fractional one (this station reports
        // 1.0625) would only blur a picture whose point is to be inspected.
        // Per-output scale is a property of a real output, and that is M4.
        const SCALE: i32 = 1;
        let area = Rect::new(0, 0, size.w / SCALE, size.h / SCALE);
        // The bottom bar is now the model's, not a hardcoded pair of names: one
        // chip per real client, in the order they opened.
        let titles = self.wayland.bar_titles(&self.windows);
        let zones = zones(area, self.theme.metrics.bar_heights());
        // Where the windows go — decided by core, back to front (D-025).
        let placed = self.windows.layout(
            self.output,
            zones.apps,
            self.split,
            self.theme.metrics.gaps(),
        );
        let slots: Vec<SurfaceSlot> = placed
            .iter()
            .map(|p| SurfaceSlot {
                id: p.window.0 as u64,
                rect: p.rect,
            })
            .collect();
        let view = ShellView {
            // Bar heights are the theme's, not a default (D-032).
            zones,
            tabs: &self.tabs,
            windows: &titles,
            focused_window: self.wayland.focused_bar_index(&self.windows),
            clock: Some(&self.clock_text),
            surfaces: &slots,
        };
        // Fonts are resolved here, before the frame opens, so both renderers
        // receive the same already-rasterised glyphs (D-005).
        let list = display_list(&view, &self.theme);
        let list = self.text.resolve(&list, SCALE);

        {
            let (gl, mut framebuffer) = self.backend.bind()?;
            // Texture uploads and client-buffer imports happen before the frame
            // opens: a frame borrows the renderer for as long as it lives.
            self.renderer
                .prepare(gl, &list, size, SCALE, &self.wayland)?;
            let mut frame = gl.render(&mut framebuffer, size, Transform::Flipped180)?;
            self.renderer.draw(&mut frame, &list, SCALE)?;
            // The nested compositor synchronises for us; the sync point is of
            // interest only on the DRM path (M4).
            let _sync = frame.finish()?;
        }

        // Full-window damage every frame. Honest about where we are: partial
        // damage is the optimisation D-027 promotes to a requirement, and it
        // arrives with clients in M2 — until then there is nothing to damage
        // partially, since the whole shell is redrawn from state anyway.
        self.backend.submit(Some(&[damage]))?;

        // The frame callback is the compositor's half of the bargain: a client
        // that asked "tell me when to draw again" gets told, and only now, after
        // its previous buffer actually reached the screen.
        //
        // Only the windows in `placed` are told, and that is the whole throttle:
        // a window waiting on the bottom bar is never asked to draw, so an
        // animation in a window nobody can see costs nothing. Doing this before
        // the frame reached the screen would let a fast client run the loop as
        // hard as it likes.
        let now = self.started.elapsed().as_millis() as u32;
        for p in &placed {
            if let Some(surface) = self.wayland.surface(p.window.0 as u64) {
                send_frame_callbacks(surface, now);
            }
        }
        Ok(())
    }
}

/// Answer the `wl_surface.frame` requests of a surface and its subsurfaces.
///
/// Written out rather than taken from `smithay::desktop`: that module also
/// carries a window model of its own — `Space`, `Window`, a stacking order — and
/// enabling it would put a second answer to "where does this window go" one
/// import away from anybody working here. Ours is in `gostui-core` and there is
/// only room for one (D-016).
fn send_frame_callbacks(surface: &WlSurface, time_ms: u32) {
    use smithay::wayland::compositor::{
        with_surface_tree_downward, SurfaceAttributes, TraversalAction,
    };
    with_surface_tree_downward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |_, states, _| {
            let mut attrs = states.cached_state.get::<SurfaceAttributes>();
            for callback in attrs.current().frame_callbacks.drain(..) {
                callback.done(time_ms);
            }
        },
        |_, _, _| true,
    );
}

/// Route `tracing` output to the terminal.
///
/// Quiet by default: our own `info` and nothing else. smithay logs the full EGL
/// and GL extension list at `info`, which is a wall of text nobody starting a
/// shell asked for. `RUST_LOG=gostui=debug,smithay=info` when something breaks.
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,gostui=info"));
    // A second call would fail; the compositor calls this once, and a failure
    // to set up logging must not stop a shell from starting.
    let _ = fmt().with_env_filter(filter).try_init();
}
