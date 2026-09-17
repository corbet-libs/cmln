//! Public API smoke: the wire and feed halves compose through the crate root.

use cmln::{decode_frame_header, decode_hello, encode_frame_header, encode_hello, Geometry};
use cmln::{feed, HostFeedFile, STALE_AFTER_SECS};

#[test]
fn wire_hello_and_frame_header_roundtrip_through_the_root() {
    let geometry = Geometry {
        width: 1920,
        height: 1080,
        scale: 1,
    };
    let hello = encode_hello(geometry);
    assert_eq!(decode_hello(&hello), Some(geometry));
    let header = encode_frame_header(64, 32);
    assert_eq!(decode_frame_header(&header), Some((64, 32, 64 * 32 * 4)));
}

#[test]
fn feed_file_survives_a_json_roundtrip_and_staleness_rule() {
    let feed = HostFeedFile {
        updated_at: 1_700_000_000,
        net: vec![],
        pools: vec![],
    };
    let bytes = serde_json::to_vec(&feed).unwrap();
    assert!(feed::parse_feed_bytes(&bytes, 1_700_000_000 + 10).is_some());
    assert!(feed::parse_feed_bytes(&bytes, 1_700_000_000 + STALE_AFTER_SECS + 1).is_none());
}
