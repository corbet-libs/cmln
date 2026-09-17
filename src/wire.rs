//! Kiosk display-socket wire protocol v1 (all integers little-endian).
//!
//! Extracted byte-for-byte from the deployed behavior shared by cksk (client,
//! `src/bin/cksk.rs`) and clck (server, `src/socket.rs`):
//!   - on connect, the server writes a HELLO: 8-byte magic `NIXLOCK1`, then
//!     `width: u32`, `height: u32`, `scale: u32`. `0,0,0` means "no kiosk
//!     output" (none configured, or the compositor has not sent the first
//!     surface `configure` yet; the client idles/retries either way).
//!   - the client then sends repeated FRAMES: `width: u32`, `height: u32`, then
//!     exactly `width*height*4` bytes of premultiplied RGBA (row-major,
//!     top-down, stride = width*4).
//!   - a frame whose geometry does not match the kiosk output is dropped and
//!     the connection is kept; a truncated read ends the stream like a clean
//!     disconnect (there is no way to resynchronize without a valid length).
//!
//! Compat invariants (never break without a deploy plan): [`MAGIC`] names the
//! wire, not the program, and stays `NIXLOCK1`; the server socket default stays
//! `nixlock.sock` (clients dial `clck.sock` first, then fall back).

use std::path::{Path, PathBuf};

/// Deployed wire magic, kept byte-identical so today's cksk speaks to today's
/// clck (and yesterday's nixlock) without a flag day. Never rename this to
/// match either program: the magic names the wire, not the program.
pub const MAGIC: &str = "NIXLOCK1";
/// Same magic as bytes, for framing without a UTF-8 round trip.
pub const MAGIC_BYTES: &[u8; 8] = b"NIXLOCK1";
/// HELLO length: 8 magic + 3 u32 geometry.
pub const HELLO_LEN: usize = 20;
/// A frame this large is never legitimate for a real display. Guards against a
/// garbled or hostile length prefix turning into a multi-gigabyte allocation.
/// Purely a defensive backstop; the normal rejection path is the geometry check.
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Kiosk output geometry from a HELLO. `(0, 0, 0)` means "no kiosk output".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}

impl Geometry {
    /// True while there is nothing to render to (unconfigured output or the
    /// pre-configure startup race). The client reconnects with backoff.
    pub fn is_pending(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Encode a server HELLO (magic + geometry).
pub fn encode_hello(geometry: Geometry) -> [u8; HELLO_LEN] {
    let mut out = [0u8; HELLO_LEN];
    out[0..8].copy_from_slice(MAGIC_BYTES);
    out[8..12].copy_from_slice(&geometry.width.to_le_bytes());
    out[12..16].copy_from_slice(&geometry.height.to_le_bytes());
    out[16..20].copy_from_slice(&geometry.scale.to_le_bytes());
    out
}

/// Decode and validate a server HELLO. `None` on short input or magic mismatch.
pub fn decode_hello(bytes: &[u8]) -> Option<Geometry> {
    if bytes.len() < HELLO_LEN || &bytes[0..8] != MAGIC_BYTES {
        return None;
    }
    Some(Geometry {
        width: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
        height: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
        scale: u32::from_le_bytes(bytes[16..20].try_into().ok()?),
    })
}

/// Encode a client frame header (geometry prefix before the RGBA payload).
pub fn encode_frame_header(width: u32, height: u32) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[0..4].copy_from_slice(&width.to_le_bytes());
    out[4..8].copy_from_slice(&height.to_le_bytes());
    out
}

/// Decode a client frame header and validate the implied payload size.
/// Returns `(width, height, payload_len)` or `None` for an implausible size
/// (zero or above [`MAX_FRAME_BYTES`]); the caller must close the connection
/// without reading a payload in that case.
pub fn decode_frame_header(bytes: &[u8; 8]) -> Option<(u32, u32, usize)> {
    let width = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let expect = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if expect == 0 || expect > MAX_FRAME_BYTES {
        return None;
    }
    Some((width, height, expect))
}

/// Resolve the server socket path: the configured value, else
/// `$XDG_RUNTIME_DIR/nixlock.sock`. `None` when neither is available; the
/// caller then skips the socket server (a display feature must never block
/// the lock itself). The filename keeps the previous product name on purpose.
pub fn resolve_server_path(configured: Option<&Path>) -> Option<PathBuf> {
    configured.map(Path::to_path_buf).or_else(|| {
        std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("nixlock.sock"))
    })
}

/// Client dial order: explicit value first, then `CLCK_SOCKET`, then
/// `NIXLOCK_SOCKET` (migration fallback), then `$XDG_RUNTIME_DIR/clck.sock`
/// when it exists, else `$XDG_RUNTIME_DIR/nixlock.sock`. `None` only when no
/// candidate can be constructed (no configured path and no `XDG_RUNTIME_DIR`).
pub fn resolve_client_path(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = configured {
        return Some(p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("CLCK_SOCKET").map(PathBuf::from) {
        return Some(p);
    }
    if let Some(p) = std::env::var_os("NIXLOCK_SOCKET").map(PathBuf::from) {
        return Some(p);
    }
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| {
        let dir = PathBuf::from(d);
        let fresh = dir.join("clck.sock");
        if fresh.exists() {
            fresh
        } else {
            dir.join("nixlock.sock")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrips_magic_and_geometry() {
        let g = Geometry {
            width: 3840,
            height: 2160,
            scale: 2,
        };
        let bytes = encode_hello(g);
        assert_eq!(bytes.len(), HELLO_LEN);
        assert_eq!(&bytes[0..8], MAGIC_BYTES);
        assert_eq!(decode_hello(&bytes), Some(g));
    }

    #[test]
    fn hello_reports_zero_geometry_when_there_is_no_kiosk_output() {
        let g = Geometry {
            width: 0,
            height: 0,
            scale: 0,
        };
        assert!(g.is_pending());
        let bytes = encode_hello(g);
        assert_eq!(&bytes[8..20], &[0u8; 12]);
        assert_eq!(decode_hello(&bytes), Some(g));
    }

    #[test]
    fn hello_rejects_a_wrong_magic() {
        let mut bytes = encode_hello(Geometry {
            width: 4,
            height: 2,
            scale: 1,
        });
        bytes[0] = b'X';
        assert_eq!(decode_hello(&bytes), None);
    }

    #[test]
    fn hello_rejects_short_input() {
        assert_eq!(decode_hello(&[0u8; 7]), None);
    }

    #[test]
    fn frame_header_roundtrips_a_plausible_size() {
        let header = encode_frame_header(4, 2);
        assert_eq!(decode_frame_header(&header), Some((4, 2, 4 * 2 * 4)));
    }

    #[test]
    fn frame_header_rejects_zero_and_huge_sizes() {
        assert_eq!(decode_frame_header(&[0u8; 8]), None);
        // Just past the cap: 4097 x 4096 x 4 > 64 MiB.
        let mut huge = [0u8; 8];
        huge[0..4].copy_from_slice(&4097u32.to_le_bytes());
        huge[4..8].copy_from_slice(&4096u32.to_le_bytes());
        assert_eq!(decode_frame_header(&huge), None);
    }

    #[test]
    fn server_path_prefers_config_then_xdg_then_gives_up() {
        let configured = Path::new("/run/custom/nixlock.sock");
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        assert_eq!(resolve_server_path(Some(configured)).unwrap(), configured);
        assert_eq!(
            resolve_server_path(None).unwrap(),
            PathBuf::from("/run/user/1000/nixlock.sock")
        );
        std::env::remove_var("XDG_RUNTIME_DIR");
        assert!(resolve_server_path(None).is_none());
        assert_eq!(resolve_server_path(Some(configured)).unwrap(), configured);
    }
}
