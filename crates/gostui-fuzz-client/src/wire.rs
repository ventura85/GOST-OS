//! Wayland's wire format, written by hand.
//!
//! A typed client library exists so that malformed messages are impossible to
//! send. That is exactly the wrong property here: half of what a compositor must
//! survive cannot be expressed through `wayland-client` at all — a request
//! addressed to an object that was never created, an opcode the interface does
//! not have, a header whose declared length disagrees with the bytes that
//! follow. Those are the messages a hostile or simply broken client sends, so
//! they are the ones worth aiming at the socket.
//!
//! The format is small enough to write out. Every message is:
//!
//! ```text
//! u32  object id
//! u32  (length << 16) | opcode      // length counts the 8-byte header
//! ...  arguments, each padded to 4 bytes
//! ```
//!
//! Integers are host byte order — the protocol is local-only, so there is no
//! endianness to negotiate.

use std::io::Write;
use std::os::unix::net::UnixStream;

/// `wl_display` is object 1 in every connection, created before the client says
/// anything. It is the only id a client may assume.
pub const DISPLAY: u32 = 1;

/// Builds one message. `body` is the already-encoded arguments.
///
/// The length in the header is computed from `body`, so this function alone
/// cannot produce a lying header — [`raw_header`] exists for that.
pub fn message(object: u32, opcode: u16, body: &[u8]) -> Vec<u8> {
    let len = 8 + body.len();
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(&object.to_ne_bytes());
    out.extend_from_slice(&(((len as u32) << 16) | opcode as u32).to_ne_bytes());
    out.extend_from_slice(body);
    out
}

/// Builds a header that claims `declared_len` regardless of what follows it.
///
/// The interesting values are a length larger than the bytes actually sent (the
/// server waits for a remainder that never arrives, and must not block forever
/// on it) and a length below the 8-byte header (a parser that advances by the
/// declared length loops on the same bytes for ever).
pub fn raw_header(object: u32, opcode: u16, declared_len: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&object.to_ne_bytes());
    out.extend_from_slice(&(((declared_len as u32) << 16) | opcode as u32).to_ne_bytes());
    out
}

/// Encodes a `new_id` / `uint` / `int` argument: a bare 32-bit word.
pub fn word(v: u32) -> [u8; 4] {
    v.to_ne_bytes()
}

/// Encodes a `string`: length including the trailing NUL, the bytes, the NUL,
/// then padding to a 4-byte boundary.
pub fn string(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let len = s.len() as u32 + 1;
    out.extend_from_slice(&len.to_ne_bytes());
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// Encodes a `string` header claiming `claimed` bytes while sending `sent`.
///
/// A server that trusts the count reads past the end of the message, and one
/// that allocates before checking it can be asked for 4 GiB by an 8-byte
/// message.
pub fn lying_string(claimed: u32, sent: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&claimed.to_ne_bytes());
    out.extend_from_slice(sent.as_bytes());
    out.push(0);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// Pushes bytes straight onto the connection, behind the library's back.
///
/// Errors are returned rather than logged: the compositor closing the socket on
/// us is a pass, not a failure, and the caller is the one that knows which.
pub fn send(socket: &mut UnixStream, bytes: &[u8]) -> std::io::Result<()> {
    socket.write_all(bytes)?;
    socket.flush()
}
