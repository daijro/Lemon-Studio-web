// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

pub mod audio;
pub mod files;
pub use files::{pick,Import};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn start() {
    console_error_panic_hook::set_once();
    files::initialize().await;
    crate::run();
}

#[cfg(feature = "web-test")]
thread_local! { static SNAPSHOT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }
#[cfg(feature = "web-test")]
pub fn record(app: &crate::App) {
    let hits: Vec<_> = app.hits.iter().map(|h| serde_json::json!({"action":format!("{:?}",h.action),"hint":h.hint,"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]})).collect();
    let parameters: Vec<_> = app.plugins.hosted.iter().filter_map(|(row,h)| {
        let p = h.plugin.try_lock().ok()?;
        Some(serde_json::json!({"row":row,"values":(0..p.param_count()).filter_map(|i| { let info=p.param_info(i)?; Some(serde_json::json!({"name":info.name,"value":p.param_value(info.id)})) }).collect::<Vec<_>>()}))
    }).collect();
    let mut audio=vec![0.0f32;4096];
    let rendered=app.transport.tap(&mut audio);
    let peak=audio.iter().copied().map(f32::abs).fold(0.0f32,f32::max);
    let notes: usize = app.state.project.patterns.iter().map(|p| p.notes.len()).sum();
    SNAPSHOT.with_borrow_mut(|s| *s = serde_json::json!({"notes":notes,"loading":app.loading(),"dirty":app.unsaved(),"audio":{"state":format!("{:?}",app.transport.state()),"tick":app.transport.tick(),"rendered":rendered,"peak":peak,"underruns":app.transport.underruns()},"parameters":parameters,"hits":hits,"screen":app.state.screen,"message":app.state.message,"channels":app.state.project.channels.len()}).to_string());
}
#[cfg(feature = "web-test")]
#[wasm_bindgen]
pub fn snapshot() -> String { SNAPSHOT.with_borrow(Clone::clone) }
