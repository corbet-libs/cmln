# cmln

Shared helper library for the cksk/clck kiosk family: display-socket wire protocol and host-feed schema. No nix required.

```console
$ cargo build
$ cargo test
```

Library (`src/lib.rs`): `wire` (HELLO magic + geometry + RGBA framing, `NIXLOCK1` wire name) and `feed` (host collector JSON with link rates and pool aggregates; missing/stale reads as pending rows). See `src/wire.rs` and `src/feed.rs` for the exact contracts.

License: FSL-1.1-ALv2, see `LICENSE.md`.
