//! A wayland client that sends the compositor things it must refuse.
//!
//! M2 asks for one thing of this program: *a fuzzing client sending malformed
//! requests kills only itself*. That criterion is about the resilience rule in
//! the repository guidelines — a protocol error must kill the client, never the
//! compositor — and a rule nobody exercises is a wish. Every scenario here aims
//! at a place where the compositor takes a number from a client and believes it.
//!
//! # How a scenario passes
//!
//! Not by the client surviving — most scenarios are supposed to get the client
//! disconnected, and that is the correct outcome. A scenario passes when, right
//! after it, a **fresh, well-behaved connection** completes a full `wl_registry`
//! round trip. That is the difference between a compositor that defended itself
//! and one that died quietly holding the socket open.
//!
//! The check lives in this binary rather than in a shell script beside it so
//! that the exit code means something on its own: 0 every scenario left the
//! compositor answering, 1 one of them did not.
//!
//! # Why it is not a `cargo test`
//!
//! It needs a running compositor, and CI has neither a screen nor a session.
//! CI compiles it — that much comes free from `--workspace` — and it is run by
//! hand when closing out a milestone (`docs/01-strategia-dev-test.md`, Tier 2).

mod scenarios;
mod wire;

use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use wayland_client::protocol::{wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::protocol::{wl_callback, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

/// What became of the client. Every one of these is an acceptable result — the
/// compositor decides how to refuse, and refusing quietly is as valid as a
/// protocol error. Only the compositor's own survival is judged.
#[derive(Debug)]
pub enum Fate {
    /// The compositor named the offence and closed the connection. The best
    /// outcome: it recognised the request as wrong rather than absorbing it.
    ProtocolError(String),
    /// The socket was closed without a protocol error.
    Disconnected,
    /// The request went through. Fine when the compositor sanitises the value
    /// instead of rejecting it — the buffer bounds checks work exactly this way.
    Accepted,
    /// No answer, and no disconnection either.
    ///
    /// This is the **correct** answer to a truncated message, and the reason
    /// this variant exists at all. A compositor told that 64 bytes are coming
    /// and given 8 cannot know the rest will never arrive; keeping the fragment
    /// and moving on to other clients is exactly right. Measured 2026-08-02:
    /// during a scenario left hanging for four minutes, `wayland-info` bound
    /// every global normally.
    ///
    /// It is also a limit on what this program may assume. Waiting for a reply
    /// to an unfinished message would hang the fuzzer on the compositor doing
    /// the right thing — which is how the first run of this scenario went.
    Silent,
}

impl Fate {
    fn describe(&self) -> String {
        match self {
            Fate::ProtocolError(e) => format!("błąd protokołu: {e}"),
            Fate::Disconnected => "rozłączony".into(),
            Fate::Accepted => "przyjęte".into(),
            Fate::Silent => "bez odpowiedzi (fragment zatrzymany)".into(),
        }
    }
}

/// The globals a scenario needs, plus whatever the compositor said back.
#[derive(Default)]
pub struct Globals {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub wm_base: Option<xdg_wm_base::XdgWmBase>,
    /// The serial from the last `xdg_surface.configure`. A scenario either
    /// acknowledges it or deliberately does not.
    pub last_configure: Option<u32>,
}

/// One connection: the library's view of it, and the raw socket underneath.
///
/// Both are needed and neither is enough. The library gets the client into a
/// state worth attacking from — bound globals, a real `wl_shm` pool backed by a
/// real file descriptor — which a hand-rolled client would have to reimplement.
/// The raw socket sends what the library's types make unrepresentable. Because
/// the socket is cloned before the `Connection` takes it, both write into the
/// same connection, so the garbage arrives as coming from a client the
/// compositor already knows.
pub struct Session {
    pub conn: Connection,
    pub queue: EventQueue<Globals>,
    pub globals: Globals,
    pub raw: UnixStream,
}

impl Session {
    /// Opens a connection and binds the globals every scenario starts from.
    pub fn open() -> Result<Self, String> {
        let stream = UnixStream::connect(socket_path()?)
            .map_err(|e| format!("nie udało się połączyć z gniazdem: {e}"))?;
        let raw = stream
            .try_clone()
            .map_err(|e| format!("nie udało się zduplikować gniazda: {e}"))?;

        let conn = Connection::from_socket(stream)
            .map_err(|e| format!("uzgodnienie nie doszło do skutku: {e}"))?;
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());

        let mut globals = Globals::default();
        queue
            .roundtrip(&mut globals)
            .map_err(|e| format!("registry bez odpowiedzi: {e}"))?;
        // A second round trip: `wl_shm` announces its formats only after the
        // bind, and a scenario that picks a format needs them to have arrived.
        queue
            .roundtrip(&mut globals)
            .map_err(|e| format!("globale bez odpowiedzi: {e}"))?;

        Ok(Session {
            conn,
            queue,
            globals,
            raw,
        })
    }

    /// Pushes everything queued and reads what came back, turning a dead
    /// connection into a [`Fate`] rather than an error.
    ///
    /// A scenario failing to reach the compositor is not a failure of this
    /// program — being thrown out is the expected ending for most of them.
    pub fn settle(&mut self) -> Fate {
        match self.queue.roundtrip(&mut self.globals) {
            Ok(_) => Fate::Accepted,
            Err(_) => self.why_dead(),
        }
    }

    /// Same, but for the raw path — and **without ever blocking**.
    ///
    /// The difference is not a refinement, it is the whole correctness of the
    /// raw scenarios. Half of them send a message the compositor cannot finish
    /// parsing, and a compositor that keeps the fragment and waits for the rest
    /// is behaving correctly. A blocking round trip here waits for a reply that
    /// correct behaviour guarantees will never come, and the fuzzer hangs on
    /// the compositor passing the test.
    ///
    /// So the reply is read the same way the garbage was sent: straight off the
    /// socket, with a timeout. Going through the library is not an option here
    /// — it waits in `poll()`, which no socket flag shortens, so the first
    /// version of this function hung for the full four minutes on a compositor
    /// that was answering other clients the whole time.
    pub fn settle_raw(&mut self) -> Fate {
        let _ = self.conn.flush();
        // Long enough for a local socket to have carried a reply, short enough
        // that seventeen scenarios stay a few seconds in total.
        let _ = self
            .raw
            .set_read_timeout(Some(std::time::Duration::from_millis(300)));

        let mut buf = [0u8; 8192];
        match std::io::Read::read(&mut self.raw, &mut buf) {
            Ok(0) => Fate::Disconnected,
            Ok(n) => match protocol_error_in(&buf[..n]) {
                Some(e) => Fate::ProtocolError(e),
                // Events arrived, none of them an error: the compositor carried
                // on talking to us.
                None => Fate::Accepted,
            },
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Fate::Silent
            }
            Err(_) => Fate::Disconnected,
        }
    }

    fn why_dead(&self) -> Fate {
        match self.conn.protocol_error() {
            Some(e) => Fate::ProtocolError(format!(
                "{}@{} kod {}: {}",
                e.object_interface, e.object_id, e.code, e.message
            )),
            None => Fate::Disconnected,
        }
    }

    /// The object id the compositor knows a proxy by. Raw messages need it:
    /// a request aimed at an object that was never created is only interesting
    /// next to one aimed at an object that was.
    pub fn id_of<P: Proxy>(proxy: &P) -> u32 {
        proxy.id().protocol_id()
    }
}

/// Looks for a `wl_display.error` among whatever the compositor sent back.
///
/// Scanning rather than reading the first message: a scenario that used the
/// library to reach a live object has registry events queued ahead of the
/// answer, so the error is somewhere in the buffer, not at the front of it.
///
/// `wl_display.error` is object 1, opcode 0, carrying the offending object id,
/// a code and a message.
fn protocol_error_in(buf: &[u8]) -> Option<String> {
    let word_at =
        |i: usize| -> Option<u32> { Some(u32::from_ne_bytes(buf.get(i..i + 4)?.try_into().ok()?)) };

    let mut i = 0;
    while i + 8 <= buf.len() {
        let object = word_at(i)?;
        let header = word_at(i + 4)?;
        let size = (header >> 16) as usize;
        let opcode = (header & 0xffff) as u16;
        // A size below the header would mean walking the same bytes for ever —
        // the very thing `short-header` asks the compositor about.
        if size < 8 || i + size > buf.len() {
            return None;
        }
        if object == wire::DISPLAY && opcode == 0 {
            let body = &buf[i + 8..i + size];
            if body.len() >= 12 {
                let bad = u32::from_ne_bytes(body[0..4].try_into().ok()?);
                let code = u32::from_ne_bytes(body[4..8].try_into().ok()?);
                let len = u32::from_ne_bytes(body[8..12].try_into().ok()?) as usize;
                let text = body
                    .get(12..12 + len.saturating_sub(1))
                    .map(String::from_utf8_lossy)
                    .unwrap_or_default();
                return Some(format!("obiekt {bad}, kod {code}: {text}"));
            }
        }
        i += size;
    }
    None
}

/// Where the compositor's socket is. The same rule the documented development
/// command line follows: the client is told through `WAYLAND_DISPLAY`, never
/// the compositor.
fn socket_path() -> Result<PathBuf, String> {
    let display = std::env::var("WAYLAND_DISPLAY").map_err(|_| {
        "WAYLAND_DISPLAY nie jest ustawione — ustaw je klientowi, nie kompozytorowi".to_string()
    })?;
    let path = PathBuf::from(&display);
    if path.is_absolute() {
        return Ok(path);
    }
    let dir = std::env::var("XDG_RUNTIME_DIR").map_err(|_| "brak XDG_RUNTIME_DIR".to_string())?;
    Ok(PathBuf::from(dir).join(display))
}

/// Does the compositor still answer? The whole verdict rests on this.
///
/// A fresh connection on purpose: reusing the one the scenario just poisoned
/// would prove nothing, since the compositor is entitled to have closed it. The
/// question is whether the *next* client can still work — which is what a user
/// with other applications open would notice.
fn compositor_alive() -> Result<(), String> {
    let session = Session::open()?;
    // Binding the globals means the compositor is not merely holding a socket
    // open: it parsed a request, allocated objects and answered.
    if session.globals.compositor.is_none() || session.globals.wm_base.is_none() {
        return Err("kompozytor odpowiada, ale nie ogłasza globali".into());
    }
    Ok(())
}

/// One line of the report. Column widths live here alone so that the header and
/// the rows cannot drift apart.
fn row(scenario: &str, fate: &str, verdict: &str) {
    println!("{scenario:<20} {fate:<44} {verdict}");
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let all = scenarios::all();

    if args.iter().any(|a| a == "--list" || a == "-l") {
        for s in &all {
            println!("{:<20} {}", s.name, s.what);
        }
        return std::process::ExitCode::SUCCESS;
    }

    let chosen: Vec<_> = if args.is_empty() {
        all.iter().collect()
    } else {
        let picked: Vec<_> = all
            .iter()
            .filter(|s| args.contains(&s.name.to_string()))
            .collect();
        if picked.is_empty() {
            eprintln!("nie znam żadnego z podanych scenariuszy; `--list` pokazuje wszystkie");
            return std::process::ExitCode::FAILURE;
        }
        picked
    };

    // Before anything else: if the compositor is not answering now, every
    // result below would be noise blamed on the last scenario that ran.
    if let Err(e) = compositor_alive() {
        eprintln!("kompozytor nie odpowiada jeszcze zanim cokolwiek wysłano: {e}");
        return std::process::ExitCode::FAILURE;
    }

    row("SCENARIUSZ", "LOS KLIENTA", "KOMPOZYTOR");
    let mut killed_by = None;
    for scenario in &chosen {
        let fate = match Session::open() {
            Ok(mut session) => (scenario.run)(&mut session),
            // Failing to connect before the scenario even ran means an earlier
            // one left the compositor unable to accept clients.
            Err(e) => {
                row(scenario.name, &format!("brak połączenia: {e}"), "MARTWY");
                killed_by = Some(scenario.name);
                break;
            }
        };
        match compositor_alive() {
            Ok(()) => row(scenario.name, &fate.describe(), "żyje"),
            Err(e) => {
                row(scenario.name, &fate.describe(), &format!("MARTWY: {e}"));
                killed_by = Some(scenario.name);
                break;
            }
        }
    }

    match killed_by {
        None => {
            println!("\n{} scenariuszy, kompozytor żyje po każdym.", chosen.len());
            std::process::ExitCode::SUCCESS
        }
        Some(name) => {
            eprintln!("\nkompozytor przestał odpowiadać po scenariuszu `{name}`.");
            std::process::ExitCode::FAILURE
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                state.compositor = Some(registry.bind(name, version.min(6), qh, ()));
            }
            "wl_shm" => {
                state.shm = Some(registry.bind(name, version.min(1), qh, ()));
            }
            "xdg_wm_base" => {
                state.wm_base = Some(registry.bind(name, version.min(5), qh, ()));
            }
            _ => {}
        }
    }
}

/// The one event that must be answered rather than ignored: a compositor is
/// entitled to kill a client that does not reply to `ping`, and a scenario
/// killed for being slow would be indistinguishable from one killed for being
/// malformed.
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Globals {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

/// Recorded, not acknowledged. Acknowledging is a scenario's decision — one of
/// them exists precisely to commit without it.
impl Dispatch<xdg_surface::XdgSurface, ()> for Globals {
    fn event(
        state: &mut Self,
        _: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.last_configure = Some(serial);
        }
    }
}

delegate_noop!(Globals: ignore wl_compositor::WlCompositor);
delegate_noop!(Globals: ignore wl_shm::WlShm);
delegate_noop!(Globals: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Globals: ignore wl_surface::WlSurface);
delegate_noop!(Globals: ignore wl_buffer::WlBuffer);
delegate_noop!(Globals: ignore wl_callback::WlCallback);
delegate_noop!(Globals: ignore xdg_toplevel::XdgToplevel);
