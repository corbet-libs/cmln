#![forbid(unsafe_code)]

//! cmln: shared helper library for the cksk/clck kiosk family.
//!
//! v1 covers exactly the two things cksk and clck must agree on byte-for-byte:
//! the display-socket [`wire`] protocol and the host-collector [`feed`] schema.
//! Everything else (rendering, locking, polling) stays in the products.
//!
//! Both products keep their compat invariants; this crate only names them once:
//! the wire magic stays `NIXLOCK1`, the server socket default stays
//! `nixlock.sock`, and a missing/stale feed reads as pending rows, never as an
//! error screen.

pub mod feed;
pub mod wire;

pub use feed::{HostFeedFile, NetIf, PoolHealth, STALE_AFTER_SECS};
pub use wire::{
    decode_frame_header, decode_hello, encode_frame_header, encode_hello, Geometry, HELLO_LEN,
    MAGIC, MAGIC_BYTES, MAX_FRAME_BYTES,
};
