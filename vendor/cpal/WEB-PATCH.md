# Browser audio context access

Source: crates.io cpal 0.18.2, copied from the locked registry source. Original Apache-2.0 license retained.

Only `src/host/audioworklet/mod.rs` changes: a page-thread registry exposes a stream's AudioContext for user-gesture resume and suspension reporting. Stream retains only an integer ID and stays Send + Sync. The async close/error paths remove the registry entry. Native backends and default features are unchanged.
