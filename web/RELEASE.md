Browser build of Lemon Studio, with the shared editor, built-in instruments and effects, AudioWorklet playback, project/sample import, manual browser Save, and FLP/WAV downloads.

Download `lemon-studio-web.zip` and extract it on a static web server. Use HTTPS and these response headers:

- `Cross-Origin-Opener-Policy: same-origin`
- `Cross-Origin-Embedder-Policy: require-corp`

The package includes gzip sidecars. Configure precompressed gzip serving to use them; the original files remain available for other hosts. WebAssembly SIMD and shared memory are required. All runtime assets are included and paths support subdirectory hosting.

Native plugin binaries and browser MIDI are unavailable. Keep downloaded projects and original sample files; browser storage can be cleared. This is a prerelease tested in Chromium and Firefox. Safari/WebKit is unverified.
