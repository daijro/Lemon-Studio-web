# Lemon Studio browser port

## Goal and scope

Build the existing Lemon Studio interface as a standalone browser application in this fork, suitable for Playground to download as a web release ZIP. Preserve the upstream Rust implementation and desktop targets. The baseline is upstream commit `39a74ac1cf5d17f6ce3e949f071e070ffed66e39`.

The proposed first release is a usable editor: channel rack, piano roll, playlist, mixer, built-in sampler, three-oscillator synth, parametric EQ, playback, project import/export, sample import and WAV export. This assumes working audio is preferred over an interface-only preview; the owner has been asked to choose. External native VST3, CLAP and FL Studio plugin binaries cannot run directly in the browser. Preserve their project state and report unavailable plugins; never silently discard or overwrite it.

Keep upstream branding, attribution and license notices. This fork remains a separate application. Adding it to Playground and publishing releases are later deployment steps, not part of the initial local build.

## Approach

Use `wasm32-unknown-unknown`, the browser target supported by the existing winit 0.30 backend. Retain the existing softbuffer/tiny-skia rendering and portable font engine. An Emscripten application build would require changes to winit itself; rewriting the interface in JavaScript would duplicate the editors. Neither is the preferred route.

Add browser implementations at the existing platform boundaries, selected by target configuration. Do not replace desktop behavior or spread silent no-op stubs through shared editing logic. New browser dependencies belong to the browser target.

### Window and input

Adapt `crates/os-app/src/main.rs` to attach its window to a canvas and use the browser event loop. Retain existing editor event handling. Handle canvas resizing, keyboard focus, pointer capture and display scaling. Disable desktop window chrome and plugin-window operations where they have no browser equivalent. Report startup errors visibly rather than leaving an empty canvas.

### Audio and built-in instruments

Adapt `crates/os-app/src/transport.rs` and `loading.rs` for browser audio and asynchronous loading. CPAL already offers browser backends; select an AudioWorklet path after verifying the pinned crate's requirements. Ordinary `std::thread::spawn` and blocking waits cannot be left on the browser main thread.

Keep `os-engine` sequencing, mixing, voice behavior and `os-dsp` algorithms shared. Adapt the existing linked built-in instruments without loading native dynamic libraries. Separate the NIH-plug/native editor dependencies where required. Existing `os-host` and `os-engine::Instrument` interfaces are preferable to introducing another plugin abstraction.

The Signalsmith C++ dependency is a technical gate: demonstrate a browser-compatible build and compare its output with native fixtures before claiming time stretching works. Do not silently substitute a different algorithm. If this cannot be preserved within the first release, bring the scope change back to the owner before shipping.

Start audio after a user gesture. Surface denied or suspended audio and initialization failures. Stop or suspend processing when the application closes, and prevent a loading task from replacing a newer project.

### Files and persistence

Replace native dialogs and direct host filesystem assumptions at the app and engine boundaries with asynchronous file import and download/export. Reuse `os-flp` parsing and serialization. Decode imported sample bytes through the existing engine decoder.

Keep imported relative sample paths distinct. Same-named files in different folders must not silently replace each other. Show missing samples and preserve references. Cancellation or malformed input must leave the current project intact. A failed save/export must not clear the unsaved flag.

Use browser storage for preferences and saved local working data, with explicit error handling for denied storage and quota exhaustion. Keep downloads available even when persistent storage is unavailable. Opening an imported project must not overwrite its original file.

### Build and distribution

Add a reproducible web build command, a small HTML bootstrap, and a separate web CI job alongside the existing desktop CI. Pin the chosen Rust/wasm tooling after the dependency probe succeeds. The web artifact must include the application, fonts, worklet/worker files and any required DSP module; it must run under a subdirectory such as `/fl-studio/`.

If shared memory is required, supply and document the required isolation headers in the local server and deployment package. Detect missing isolation support before starting audio, with a useful error message. Do not rely on arbitrary remote executable scripts at runtime.

## Implementation order and completion evidence

1. Prove the target and dependency boundaries. Record the unmodified wasm build failure, verify the existing core tests, and demonstrate the browser route for Signalsmith and built-in instrument code. Use temporary build output rather than filling the source checkout; the host disk currently has only about 7 GB free.
2. Bring up the real interface and editing paths. Verify the canvas, resize, input, note creation/deletion and undo. This is an intermediate milestone, not the promised usable release.
3. Connect built-in sound generation and playback. Check deterministic synthetic notes and silence, transport start/pause/stop, sustained notes, repeated project changes and mixer processing. Compare offline/native and browser output with justified numerical tolerances.
4. Complete file import/export and persistence. Test project round trips, unknown plugin-state preservation, missing samples, duplicate basenames, cancelled imports, malformed files, export failures and storage failures.
5. Build the distributable ZIP and run browser integration checks from a subdirectory with the required headers. Verify a clean browser session can create a song, hear it, save it, reopen it and export non-silent WAV audio. Run the affected native tests and desktop compilation checks to catch regressions.

A compiled `.wasm` file alone is not completion. The first release is complete only when the functional workflow above passes. Any unavailable browser verification must be reported explicitly rather than inferred from compilation.
