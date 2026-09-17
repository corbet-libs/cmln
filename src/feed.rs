//! Host-collector feed schema: privileged host facts the kiosk cannot see
//! from its own namespace (host link rates, pool aggregates, disk temps).
//!
//! Extracted from cksk's system board (`src/system.rs`) and its
//! `contrib/host-collector/cksk-host-feed.sh` writer, so a future Rust
//! collector and every board consumer share one definition:
//! `{"updated_at": <unix>, "net": [{"iface","rx_bps","tx_bps",
//! "speed_bps"|null,"state"}], "pools": [{"name","health","used_pct"|null,
//! "read_bps","write_bps","util_pct"|null,"temp_c"|null}]}`.
//!
//! Read rule (never an error screen): a missing file, bad JSON, or a feed
//! older than [`STALE_AFTER_SECS`] reads as absent (pending rows).

/// Maximum feed age in seconds before it reads as absent (pending rows).
pub const STALE_AFTER_SECS: u64 = 120;

/// One network interface as the host collector saw it (rates already per second).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct NetIf {
    pub iface: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    /// Link speed in bit/s, when known. Without it only absolute rates render.
    pub speed_bps: Option<f64>,
    /// "up" or "down". Down links render dimmed with no rates: a missing link
    /// must read as broken, never as absent.
    #[serde(default = "net_up")]
    pub state: String,
}

fn net_up() -> String {
    "up".to_string()
}

/// One pool, aggregated by the host collector (consumers never map disks).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct PoolHealth {
    pub name: String,
    /// e.g. ONLINE / DEGRADED / FAULTED / UNKNOWN (collector degrades gracefully).
    pub health: String,
    pub used_pct: Option<f64>,
    pub read_bps: f64,
    pub write_bps: f64,
    pub util_pct: Option<f64>,
    pub temp_c: Option<f32>,
}

/// The collector JSON file. Field defaults keep old writers forward-readable.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct HostFeedFile {
    pub updated_at: u64,
    #[serde(default)]
    pub net: Vec<NetIf>,
    #[serde(default)]
    pub pools: Vec<PoolHealth>,
}

impl HostFeedFile {
    /// Age in seconds relative to `now_unix`; saturates at 0 for future stamps.
    pub fn age_secs(&self, now_unix: u64) -> u64 {
        now_unix.saturating_sub(self.updated_at)
    }

    /// True when the feed is older than [`STALE_AFTER_SECS`] (or from the future
    /// by clock skew, which reads as fresh, never as an error).
    pub fn is_stale(&self, now_unix: u64) -> bool {
        self.age_secs(now_unix) > STALE_AFTER_SECS
    }
}

/// Parse feed bytes with the production read rule: bad JSON is `None` (the
/// caller keeps its previous rows and logs once, as cksk does).
pub fn parse_feed_bytes(bytes: &[u8], now_unix: u64) -> Option<HostFeedFile> {
    let feed: HostFeedFile = serde_json::from_slice(bytes).ok()?;
    if feed.is_stale(now_unix) {
        return None;
    }
    Some(feed)
}

/// Read the host feed if configured. Never fails the board: missing file, bad
/// JSON, or stale data all degrade to `None` (pending rows), never to an error.
pub fn read_feed_file(path: Option<&str>, now_unix: u64) -> Option<HostFeedFile> {
    let bytes = std::fs::read(path?).ok()?;
    parse_feed_bytes(&bytes, now_unix)
}

/// Current Unix time in seconds; `0` when the clock is unavailable (the feed
/// then reads as fresh only when `updated_at` is also 0).
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_json(updated_at: u64) -> Vec<u8> {
        format!(
            r#"{{"updated_at":{updated_at},"net":[{{"iface":"br0","rx_bps":8000000.0,"tx_bps":1000000.0,"speed_bps":1000000000.0,"state":"up"}}],"pools":[{{"name":"solid","health":"ONLINE","used_pct":42.5,"read_bps":100.0,"write_bps":200.0,"util_pct":null,"temp_c":38.0}}]}}"#
        )
        .into_bytes()
    }

    #[test]
    fn fresh_feed_parses_with_net_and_pool_rows() {
        let feed = parse_feed_bytes(&feed_json(1_700_000_000), 1_700_000_010).unwrap();
        assert_eq!(feed.net.len(), 1);
        assert_eq!(feed.net[0].iface, "br0");
        assert_eq!(feed.pools.len(), 1);
        assert_eq!(feed.pools[0].name, "solid");
    }

    #[test]
    fn stale_feed_reads_as_absent() {
        assert!(parse_feed_bytes(
            &feed_json(1_700_000_000),
            1_700_000_000 + STALE_AFTER_SECS + 1
        )
        .is_none());
    }

    #[test]
    fn bad_json_reads_as_absent() {
        assert!(parse_feed_bytes(b"not json", 1_700_000_000).is_none());
    }

    #[test]
    fn missing_state_defaults_to_up() {
        let raw = r#"{"updated_at":1700000000,"net":[{"iface":"eth0","rx_bps":0.0,"tx_bps":0.0,"speed_bps":null}],"pools":[]}"#;
        let feed = parse_feed_bytes(raw.as_bytes(), 1_700_000_010).unwrap();
        assert_eq!(feed.net[0].state, "up");
    }

    #[test]
    fn missing_file_reads_as_absent() {
        assert!(read_feed_file(Some("/nonexistent/cmln-feed-test.json"), 1_700_000_000).is_none());
        assert!(read_feed_file(None, 1_700_000_000).is_none());
    }
}
