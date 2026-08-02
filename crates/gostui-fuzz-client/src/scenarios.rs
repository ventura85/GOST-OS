//! The attacks.
//!
//! They fall into two families, and the split is not cosmetic. The raw ones are
//! **unrepresentable** through `wayland-client`: a typed client library exists
//! so that a request to a non-existent object or an opcode outside the
//! interface cannot be written down. Those are precisely the messages a broken
//! or hostile client sends, so they go out as bytes.
//!
//! The typed ones do the opposite. They use the library correctly to reach the
//! state where the compositor stops merely routing requests and starts
//! *believing numbers* — a buffer's width, stride and offset, a window's
//! geometry. That is where `render/cpu.rs` reads client memory, and it is the
//! only place in this compositor where a wrong number could reach past the end
//! of a mapping.

use std::fs::File;
use std::io::Write as _;
use std::os::fd::AsFd;

use wayland_client::protocol::{wl_shm, wl_surface};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel};

use crate::wire::{self, DISPLAY};
use crate::{Fate, Globals, Session};

pub struct Scenario {
    pub name: &'static str,
    pub what: &'static str,
    pub run: fn(&mut Session) -> Fate,
}

pub fn all() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "bad-object",
            what: "żądanie do obiektu, którego nigdy nie stworzono",
            run: bad_object,
        },
        Scenario {
            name: "bad-opcode",
            what: "opcode spoza interfejsu wl_display",
            run: bad_opcode,
        },
        Scenario {
            name: "long-header",
            what: "nagłówek deklaruje 64 B, wysłano 8",
            run: long_header,
        },
        Scenario {
            name: "short-header",
            what: "deklarowana długość mniejsza niż nagłówek",
            run: short_header,
        },
        Scenario {
            name: "zero-object",
            what: "żądanie do obiektu 0",
            run: zero_object,
        },
        Scenario {
            name: "server-range-id",
            what: "new_id z zakresu zarezerwowanego dla serwera",
            run: server_range_id,
        },
        Scenario {
            name: "lying-string",
            what: "napis deklarujący 4 GiB w krótkiej wiadomości",
            run: lying_string,
        },
        Scenario {
            name: "unknown-interface",
            what: "bind do interfejsu, którego nikt nie ogłosił",
            run: unknown_interface,
        },
        Scenario {
            name: "pool-overrun",
            what: "bufor w granicach poola, ale poza plikiem (SIGBUS)",
            run: pool_overrun,
        },
        Scenario {
            name: "buffer-oversize",
            what: "wymiary i stride równe i32::MAX",
            run: buffer_oversize,
        },
        Scenario {
            name: "negative-buffer",
            what: "ujemne width, height, stride i offset",
            run: negative_buffer,
        },
        Scenario {
            name: "stride-below-width",
            what: "stride mniejszy niż width * 4",
            run: stride_below_width,
        },
        Scenario {
            name: "bad-format",
            what: "format pikseli, którego wl_shm nie ogłosił",
            run: bad_format,
        },
        Scenario {
            name: "double-role",
            what: "get_toplevel dwa razy na tej samej powierzchni",
            run: double_role,
        },
        Scenario {
            name: "commit-without-ack",
            what: "commit po configure, bez ack_configure",
            run: commit_without_ack,
        },
        Scenario {
            name: "garbage-geometry",
            what: "set_window_geometry z ujemnym rozmiarem",
            run: garbage_geometry,
        },
        Scenario {
            name: "flood",
            what: "2000 powierzchni, potem twarde rozłączenie",
            run: flood,
        },
    ]
}

// ---------------------------------------------------------------------------
// Raw bytes
// ---------------------------------------------------------------------------

/// Sends bytes the library never saw.
///
/// The flush first matters: the library has its own outgoing buffer, and
/// without draining it the garbage would arrive *before* the requests that set
/// the connection up. The point is a client that was behaving and then stopped.
fn raw(session: &mut Session, bytes: &[u8]) -> Fate {
    if session.conn.flush().is_err() {
        return Fate::Disconnected;
    }
    if wire::send(&mut session.raw, bytes).is_err() {
        // The compositor closed the socket before we finished writing. It
        // refused; that is a pass, and the survival check decides the rest.
        return Fate::Disconnected;
    }
    session.settle_raw()
}

fn bad_object(session: &mut Session) -> Fate {
    // 0xdeadbeef is in the client's half of the id space, so the compositor
    // cannot dismiss it on range alone — it has to look the object up and find
    // nothing.
    raw(session, &wire::message(0xdead_beef, 0, &[]))
}

fn bad_opcode(session: &mut Session) -> Fate {
    // wl_display has two requests: sync (0) and get_registry (1).
    raw(session, &wire::message(DISPLAY, 99, &wire::word(2)))
}

fn long_header(session: &mut Session) -> Fate {
    // The compositor is told 64 bytes are coming and gets 8. It must not block
    // waiting for the rest — a client that hangs the compositor by saying
    // nothing is a denial of service that costs the attacker nothing.
    raw(session, &wire::raw_header(DISPLAY, 0, 64))
}

fn short_header(session: &mut Session) -> Fate {
    // A length below the 8-byte header. A parser that advances by the declared
    // length never moves past this message and spins on it for ever.
    raw(session, &wire::raw_header(DISPLAY, 0, 4))
}

fn zero_object(session: &mut Session) -> Fate {
    // Object 0 is the protocol's null. Nothing may be addressed to it.
    raw(session, &wire::message(0, 0, &[]))
}

fn server_range_id(session: &mut Session) -> Fate {
    // Ids at or above 0xff000000 belong to the server. A client creating one
    // there could collide with an object the compositor made for itself.
    raw(
        session,
        &wire::message(DISPLAY, 0, &wire::word(0xff00_0001)),
    )
}

fn lying_string(session: &mut Session) -> Fate {
    let qh = session.queue.handle();
    // A registry we own the id of, so the message is aimed at a live object and
    // gets as far as argument parsing.
    let registry = session.conn.display().get_registry(&qh, ());
    let id = Session::id_of(&registry);

    // wl_registry.bind: uint name, string interface, uint version, new_id id.
    // The string claims 4 GiB. A compositor that allocates before checking the
    // length against the bytes left in the message asks the kernel for 4 GiB on
    // behalf of a client that sent 24 bytes.
    let mut body = Vec::new();
    body.extend_from_slice(&wire::word(1));
    body.extend_from_slice(&wire::lying_string(u32::MAX, "wl_compositor"));
    body.extend_from_slice(&wire::word(1));
    body.extend_from_slice(&wire::word(0xf000_0001));
    raw(session, &wire::message(id, 0, &body))
}

fn unknown_interface(session: &mut Session) -> Fate {
    let qh = session.queue.handle();
    let registry = session.conn.display().get_registry(&qh, ());
    let id = Session::id_of(&registry);

    // A well-formed bind — correct string encoding, plausible version — naming
    // a global that was never advertised. The message is valid; only its
    // meaning is wrong, which is the harder of the two to reject and the one a
    // compositor is most likely to answer by indexing a table with whatever it
    // was handed.
    let mut body = Vec::new();
    body.extend_from_slice(&wire::word(9999)); // a name never announced either
    body.extend_from_slice(&wire::string("wl_nonexistent"));
    body.extend_from_slice(&wire::word(1));
    body.extend_from_slice(&wire::word(0xf000_0003));
    raw(session, &wire::message(id, 0, &body))
}

// ---------------------------------------------------------------------------
// Typed: reaching the code that reads client memory
// ---------------------------------------------------------------------------

/// A file in `/dev/shm`, unlinked immediately. The descriptor stays valid, so
/// the pool is backed by real memory that no longer has a name — nothing to
/// clean up if this program is killed mid-scenario.
fn shm_file(len: u64, fill: bool) -> Result<File, String> {
    let path = format!("/dev/shm/gostui-fuzz-{}", std::process::id());
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| format!("{path}: {e}"))?;
    let _ = std::fs::remove_file(&path);
    file.set_len(len).map_err(|e| e.to_string())?;
    if fill {
        // Written rather than left sparse so that a compositor reading inside
        // the file sees something, and only reads past the end fault.
        let mut f = &file;
        f.write_all(&vec![0x80u8; len as usize])
            .map_err(|e| e.to_string())?;
    }
    Ok(file)
}

/// Surface, `xdg_surface`, toplevel, committed and acknowledged — a window the
/// compositor has tiled and is ready to draw. Every buffer scenario starts
/// here, because a buffer attached to a surface nobody draws is never read, and
/// an unread buffer proves nothing.
fn mapped_window(
    session: &mut Session,
) -> Result<
    (
        wl_surface::WlSurface,
        xdg_surface::XdgSurface,
        xdg_toplevel::XdgToplevel,
    ),
    Fate,
> {
    let qh = session.queue.handle();
    let (Some(compositor), Some(wm_base)) = (
        session.globals.compositor.clone(),
        session.globals.wm_base.clone(),
    ) else {
        return Err(Fate::Disconnected);
    };

    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("gostui-fuzz".into());
    surface.commit();

    // The compositor answers the first commit with a configure carrying the
    // tile size. Acknowledging it is what makes the surface mappable.
    if session.queue.roundtrip(&mut session.globals).is_err() {
        return Err(session.settle());
    }
    if let Some(serial) = session.globals.last_configure.take() {
        xdg.ack_configure(serial);
    }
    Ok((surface, xdg, toplevel))
}

/// Attaches a buffer, damages the whole surface and gives the compositor room
/// to draw it.
///
/// The extra round trips are the point of the scenario, not politeness: the
/// read that could fault happens while the compositor renders, which is a
/// different turn of its event loop from the commit that asked for it.
fn present(
    session: &mut Session,
    surface: &wl_surface::WlSurface,
    buffer: &wayland_client::protocol::wl_buffer::WlBuffer,
) -> Fate {
    surface.attach(Some(buffer), 0, 0);
    surface.damage(0, 0, i32::MAX, i32::MAX);
    surface.commit();
    for _ in 0..3 {
        if session.queue.roundtrip(&mut session.globals).is_err() {
            return session.settle();
        }
    }
    Fate::Accepted
}

fn pool_overrun(session: &mut Session) -> Fate {
    // The heart of the whole program. The pool claims a megabyte; the file
    // behind it is one page. mmap of a megabyte succeeds — the kernel is happy
    // to map past the end of a file — and the pages beyond the first fault with
    // SIGBUS *when read*, not when mapped. So the compositor accepts the pool,
    // accepts a buffer that fits inside it, and dies at the moment it draws.
    //
    // Nothing in `render/cpu.rs` can catch this: its bounds are checked against
    // the pool size, which is honest. Only a SIGBUS handler around the mapping
    // saves the compositor here, which is exactly what this scenario asks about.
    const PAGE: u64 = 4096;
    const POOL: i32 = 1024 * 1024;

    let file = match shm_file(PAGE, true) {
        Ok(f) => f,
        Err(_) => return Fate::Disconnected,
    };
    let qh = session.queue.handle();
    let Some(shm) = session.globals.shm.clone() else {
        return Fate::Disconnected;
    };
    let pool = shm.create_pool(file.as_fd(), POOL, &qh, ());
    // 256 x 256 at 1024 bytes per row is 256 KiB: inside the pool, far outside
    // the 4 KiB file.
    let buffer = pool.create_buffer(0, 256, 256, 1024, wl_shm::Format::Argb8888, &qh, ());

    let (surface, _xdg, _toplevel) = match mapped_window(session) {
        Ok(w) => w,
        Err(fate) => return fate,
    };
    present(session, &surface, &buffer)
}

fn buffer_oversize(session: &mut Session) -> Fate {
    // Every dimension at i32::MAX. width * 4 overflows, height * stride
    // overflows, and a compositor multiplying before checking wraps to a small
    // number that passes its own bounds test.
    let file = match shm_file(4096, true) {
        Ok(f) => f,
        Err(_) => return Fate::Disconnected,
    };
    let qh = session.queue.handle();
    let Some(shm) = session.globals.shm.clone() else {
        return Fate::Disconnected;
    };
    let pool = shm.create_pool(file.as_fd(), 4096, &qh, ());
    let buffer = pool.create_buffer(
        0,
        i32::MAX,
        i32::MAX,
        i32::MAX,
        wl_shm::Format::Argb8888,
        &qh,
        (),
    );

    let (surface, _xdg, _toplevel) = match mapped_window(session) {
        Ok(w) => w,
        Err(fate) => return fate,
    };
    present(session, &surface, &buffer)
}

fn negative_buffer(session: &mut Session) -> Fate {
    // Negative offset, width, height and stride. The protocol types them as
    // signed, so this is a legal message carrying an illegal meaning — the case
    // a compositor casting straight to `usize` turns into an enormous positive
    // number.
    let file = match shm_file(4096, true) {
        Ok(f) => f,
        Err(_) => return Fate::Disconnected,
    };
    let qh = session.queue.handle();
    let Some(shm) = session.globals.shm.clone() else {
        return Fate::Disconnected;
    };
    let pool = shm.create_pool(file.as_fd(), 4096, &qh, ());
    let buffer = pool.create_buffer(-1, -16, -16, -64, wl_shm::Format::Argb8888, &qh, ());

    let (surface, _xdg, _toplevel) = match mapped_window(session) {
        Ok(w) => w,
        Err(fate) => return fate,
    };
    present(session, &surface, &buffer)
}

fn stride_below_width(session: &mut Session) -> Fate {
    // 64 pixels a row declared to occupy 16 bytes. Reading row by row at
    // `width * 4` walks off the end of the last row and past the mapping.
    let file = match shm_file(65536, true) {
        Ok(f) => f,
        Err(_) => return Fate::Disconnected,
    };
    let qh = session.queue.handle();
    let Some(shm) = session.globals.shm.clone() else {
        return Fate::Disconnected;
    };
    let pool = shm.create_pool(file.as_fd(), 65536, &qh, ());
    let buffer = pool.create_buffer(0, 64, 64, 16, wl_shm::Format::Argb8888, &qh, ());

    let (surface, _xdg, _toplevel) = match mapped_window(session) {
        Ok(w) => w,
        Err(fate) => return fate,
    };
    present(session, &surface, &buffer)
}

fn bad_format(session: &mut Session) -> Fate {
    // Unrepresentable through the typed API — `wl_shm::Format` is an enum — so
    // the buffer is created by hand. wl_shm_pool.create_buffer is opcode 0:
    // new_id, int offset, int width, int height, int stride, uint format.
    let file = match shm_file(4096, true) {
        Ok(f) => f,
        Err(_) => return Fate::Disconnected,
    };
    let qh = session.queue.handle();
    let Some(shm) = session.globals.shm.clone() else {
        return Fate::Disconnected;
    };
    let pool = shm.create_pool(file.as_fd(), 4096, &qh, ());
    let pool_id = Session::id_of(&pool);

    let mut body = Vec::new();
    body.extend_from_slice(&wire::word(0xf000_0002)); // our own new_id
    body.extend_from_slice(&wire::word(0)); // offset
    body.extend_from_slice(&wire::word(16)); // width
    body.extend_from_slice(&wire::word(16)); // height
    body.extend_from_slice(&wire::word(64)); // stride
    body.extend_from_slice(&wire::word(0xdead_f00d)); // a format nobody announced
    raw(session, &wire::message(pool_id, 0, &body))
}

fn double_role(session: &mut Session) -> Fate {
    let (_surface, xdg, _toplevel) = match mapped_window(session) {
        Ok(w) => w,
        Err(fate) => return fate,
    };
    let qh = session.queue.handle();
    // A second toplevel on a surface that already has a role. xdg-shell says a
    // surface has exactly one, for ever.
    let _second = xdg.get_toplevel(&qh, ());
    session.settle()
}

fn commit_without_ack(session: &mut Session) -> Fate {
    let qh = session.queue.handle();
    let (Some(compositor), Some(wm_base)) = (
        session.globals.compositor.clone(),
        session.globals.wm_base.clone(),
    ) else {
        return Fate::Disconnected;
    };

    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, ());
    let _toplevel = xdg.get_toplevel(&qh, ());
    surface.commit();
    if session.queue.roundtrip(&mut session.globals).is_err() {
        return session.settle();
    }

    // A configure arrived and is deliberately dropped. Committing now means
    // committing state the compositor never agreed to — the client claiming a
    // size of its own choosing, which in a tiling compositor is the request
    // that must not be honoured.
    session.globals.last_configure.take();
    surface.commit();
    session.settle()
}

fn garbage_geometry(session: &mut Session) -> Fate {
    let (surface, xdg, _toplevel) = match mapped_window(session) {
        Ok(w) => w,
        Err(fate) => return fate,
    };
    // A window geometry with a negative size. The renderer subtracts this
    // margin and the pointer adds it (the client-side-decoration rule), so a
    // value believed here moves drawing and clicking in opposite directions.
    xdg.set_window_geometry(0, 0, -1, -1);
    surface.commit();
    session.settle()
}

fn flood(session: &mut Session) -> Fate {
    let qh = session.queue.handle();
    let Some(compositor) = session.globals.compositor.clone() else {
        return Fate::Disconnected;
    };
    // Two thousand surfaces, each with a role, none of them ever destroyed.
    // Then the connection goes away without warning. Everything above is the
    // compositor's to free, and a leak here is the kind that only shows after
    // the shell has been running for a week (D-039).
    for _ in 0..2000 {
        let surface = compositor.create_surface(&qh, ());
        surface.commit();
    }
    let fate = session.settle();
    // Hard disconnect: no destructors, no goodbye.
    let _ = session.raw.shutdown(std::net::Shutdown::Both);
    fate
}

/// Kept so that `Globals` is not accidentally reduced to something a scenario
/// cannot use: every scenario reaches for at least one of these.
#[allow(dead_code, reason = "documents the invariant the scenarios rely on")]
fn globals_are_bound(g: &Globals) -> bool {
    g.compositor.is_some() && g.shm.is_some() && g.wm_base.is_some()
}
