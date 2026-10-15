// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

pub mod audio;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn start() {
    console_error_panic_hook::set_once();
    crate::run();
}

#[derive(Clone, Copy)]
pub enum Import { Project, Sample(usize), Theme }
thread_local! {
    static IMPORTS: std::cell::RefCell<Vec<(Import, String, Vec<u8>)>> = const { std::cell::RefCell::new(Vec::new()) };
}
pub fn pick(purpose: Import) {
    wasm_bindgen_futures::spawn_local(async move {
        let filter: &[&str] = match purpose { Import::Project => &["flp"], Import::Sample(_) => &["wav", "mp3", "flac", "ogg", "aif", "aiff"], Import::Theme => &["flstheme", "txt"] };
        if let Some(file) = rfd::AsyncFileDialog::new().add_filter("Import", filter).pick_file().await {
            let bytes = file.read().await;
            IMPORTS.with_borrow_mut(|q| q.push((purpose, file.file_name(), bytes)));
            crate::wake_ui();
        }
    });
}

#[cfg(feature = "web-test")]
thread_local! { static SNAPSHOT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }
#[cfg(feature = "web-test")]
pub fn record(app: &crate::App) {
    let hits: Vec<_> = app.hits.iter().map(|h| serde_json::json!({"action":format!("{:?}",h.action),"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]})).collect();
    let parameters: Vec<_> = app.plugins.hosted.iter().filter_map(|(row,h)| {
        let p = h.plugin.try_lock().ok()?;
        Some(serde_json::json!({"row":row,"values":(0..p.param_count()).filter_map(|i| { let info=p.param_info(i)?; Some(serde_json::json!({"name":info.name,"value":p.param_value(info.id)})) }).collect::<Vec<_>>()}))
    }).collect();
    let notes: usize = app.state.project.patterns.iter().map(|p| p.notes.len()).sum();
    SNAPSHOT.with_borrow_mut(|s| *s = serde_json::json!({"notes":notes,"parameters":parameters,"hits":hits,"screen":app.state.screen,"message":app.state.message,"channels":app.state.project.channels.len()}).to_string());
}
#[cfg(feature = "web-test")]
#[wasm_bindgen]
pub fn snapshot() -> String { SNAPSHOT.with_borrow(Clone::clone) }
