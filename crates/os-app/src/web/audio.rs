// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Browser audio resources are created on the page; sequencing and feeding stay on their existing workers.
use std::sync::{LazyLock, mpsc};
static CALLS: LazyLock<crossbeam::queue::SegQueue<Box<dyn FnOnce() + Send>>> = LazyLock::new(Default::default);

pub fn dispatch<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> T {
    if web_sys::window().is_some() { return call(); }
    let (tx, rx) = mpsc::channel();
    CALLS.push(Box::new(move || { let _ = tx.send(call()); }));
    crate::wake_ui();
    // Only the control worker waits; the page's event loop remains free to initialize AudioWorklets.
    rx.recv().expect("browser audio dispatcher closed")
}

pub fn poll() { while let Some(call) = CALLS.pop() { call(); } }

#[wasm_bindgen::prelude::wasm_bindgen(module = "/web/audio.mjs")]
extern "C" { fn watch_context(context: &wasm_bindgen::JsValue); }

pub fn watch(stream: &cpal::Stream) {
    if let cpal::platform::StreamInner::AudioWorklet(stream) = stream.as_inner() {
        if let Some(context) = stream.audio_context() { watch_context(context.as_ref()); }
    }
}
