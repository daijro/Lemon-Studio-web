# Signalsmith browser build

Sources copied from the crates.io `signalsmith-stretch` 0.1.3 package. `UPSTREAM.sha256` records their original hashes. Preserve the adjacent MIT licenses.

The DSP headers are unchanged. The browser wrapper initializes the same algorithm using a seed supplied by JavaScript's Math.random instead of an OS random device. This is audio phase decorrelation, not cryptography. `runtime.cpp` connects C++ allocation to Rust's allocator so all DSP objects use one WASM memory and can be processed by an AudioWorklet. Desktop builds retain the original crates.io dependency.
