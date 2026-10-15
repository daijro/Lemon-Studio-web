# Browser support patch

Source: https://github.com/robbert-vdh/nih-plug/tree/de421011f41a6d10fc8c7a6084e4f4dee0143683

The core crate and derive macros are vendored at the exact revision used by Lemon Studio. The upstream ISC license is retained. Workspace examples and unrelated UI integrations are omitted. Browser changes are target-gated; desktop builds retain the upstream implementation and VST3 feature.

Browser-only patches use `wasm_thread` for background tasks and thread identities, and detach on shutdown instead of blocking the UI on a join. Native shutdown and threading code is unchanged.
