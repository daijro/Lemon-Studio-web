// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

//! Browser I/O stays here; the app still parses and serializes the same FLP bytes as desktop.
use crate::{
    app::{App, PreparedSave},
    audio_cfg::AudioCfg,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::{Arc, LazyLock},
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

#[wasm_bindgen(module = "/web/files.mjs")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn pick_files(kind: &str) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch)]
    fn download(name: &str, bytes: &[u8], mime: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn persist(snapshot: &JsValue) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch)]
    fn restore() -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch)]
    fn save_preferences(preferences: &str) -> Result<js_sys::Promise, JsValue>;
    fn dirty(value: bool);
}
#[derive(Clone, Copy)]
pub enum Import {
    Project,
    Folder,
    Sample(usize),
    Theme,
}
struct Imported {
    path: PathBuf,
    bytes: Vec<u8>,
}
enum Completion {
    Import(
        u64,
        Import,
        Option<os_model::Channel>,
        Result<Vec<Imported>, String>,
    ),
    Sample(
        u64,
        usize,
        os_model::Channel,
        PathBuf,
        Result<Arc<os_engine::Sample>, String>,
    ),
    Save(
        u64,
        PreparedSave,
        os_model::Project,
        f64,
        Vec<(usize, Option<Vec<u8>>)>,
        PathBuf,
        Result<(), String>,
    ),
    Message(String),
}
thread_local! {
    static GENERATION:Cell<u64>=const{Cell::new(0)};
    static PREFS_CHECK:Cell<f64>=const{Cell::new(0.0)};
    static RESTORED:RefCell<Option<JsValue>>=const{RefCell::new(None)};
    static SAVING:Cell<bool>=const{Cell::new(false)};
    static EXPORT_PENDING:Cell<bool>=const{Cell::new(false)};
    static ONLINE_PENDING:Cell<bool>=const{Cell::new(false)};
    static SAVE_PENDING:Cell<bool>=const{Cell::new(false)};
    static SAVE_PLUGINS:RefCell<Option<std::collections::BTreeMap<usize, (os_host::Descriptor, Option<Vec<u8>>)>>> = const { RefCell::new(None) };
    static PREFS:RefCell<String>=const{RefCell::new(String::new())};
}
static COMPLETED: LazyLock<crossbeam::queue::SegQueue<Completion>> =
    LazyLock::new(Default::default);
fn error(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
fn get(object: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(object, &key.into()).unwrap_or(JsValue::NULL)
}
fn set(object: &JsValue, key: &str, value: &JsValue) {
    js_sys::Reflect::set(object, &key.into(), value).expect("plain object");
}
fn complete(value: Completion) {
    COMPLETED.push(value);
    crate::wake_ui();
}
pub fn retry_export() { EXPORT_PENDING.with(|p|p.set(true)); }
pub fn retry_online() { ONLINE_PENDING.with(|p|p.set(true)); }
pub fn changed_project() {
    EXPORT_PENDING.with(|p|p.set(false));
    ONLINE_PENDING.with(|p|p.set(false));
    SAVE_PENDING.with(|p|p.set(false));
    GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
}
fn current() -> u64 {
    GENERATION.with(Cell::get)
}
pub fn saved_plugin(key: usize) -> Option<(os_host::Descriptor, Option<Vec<u8>>)> {
    SAVE_PLUGINS.with_borrow(|p|p.as_ref()?.get(&key).cloned())
}
fn states(app: &App) -> Option<Vec<(usize, Option<Vec<u8>>)>> {
    app.plugins.hosted.iter().map(|(&row, h)| {
        Some((row, h.plugin.try_lock().ok()?.save_state()))
    }).collect()
}

pub async fn initialize() {
    match restore().map_err(error) {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(value) => RESTORED.with_borrow_mut(|v| *v = Some(value)),
            Err(e) => complete(Completion::Message(format!(
                "Browser storage unavailable: {}. Downloads still work.",
                error(e)
            ))),
        },
        Err(e) => complete(Completion::Message(format!(
            "Browser storage unavailable: {e}. Downloads still work."
        ))),
    }
}
fn imported(value: JsValue, stored: bool) -> Result<Vec<Imported>, String> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    if !js_sys::Array::is_array(&value) {
        return Err("Invalid imported file list".into());
    }
    let mut files = Vec::new();
    let mut size = 0usize;
    let mut names = std::collections::HashSet::new();
    for item in js_sys::Array::from(&value) {
        let name = get(&item, "path")
            .as_string()
            .ok_or("Missing imported filename")?;
        let relative = if stored {
            name.strip_prefix("/imports/")
                .ok_or("Invalid stored sample path")?
        } else {
            &name
        };
        let path = os_paths::fs::relative_path(relative).map_err(|e| e.to_string())?;
        let path = if stored {
            PathBuf::from("/imports").join(path)
        } else {
            path
        };
        if !names.insert(path.clone()) {
            return Err(format!("Duplicate file path: {name}"));
        }
        let bytes = get(&item, "bytes");
        if !bytes.is_instance_of::<js_sys::Uint8Array>() {
            return Err("Invalid imported bytes".into());
        }
        let array = js_sys::Uint8Array::new(&bytes);
        size += array.length() as usize;
        if size > os_paths::fs::MAX_IMPORT_BYTES {
            return Err("Import exceeds the 256 MiB browser limit".into());
        }
        files.push(Imported {
            path,
            bytes: array.to_vec(),
        });
    }
    Ok(files)
}
pub fn pick(app: &App, purpose: Import) {
    let channel = match purpose {
        Import::Sample(row) => app.state.project.channels.get(row).cloned(),
        _ => None,
    };
    changed_project();
    let generation = current();
    let kind = match purpose {
        Import::Project => "project",
        Import::Folder => "folder",
        Import::Sample(_) => "sample",
        Import::Theme => "theme",
    };
    let request = pick_files(kind);
    spawn_local(async move {
        let result = match request {
            Ok(p) => JsFuture::from(p)
                .await
                .map_err(error)
                .and_then(|v| imported(v, false)),
            Err(e) => Err(error(e)),
        };
        complete(Completion::Import(generation, purpose, channel, result));
    });
}
pub fn download_bytes(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    download(name, bytes, mime).map_err(error)
}
fn preferences(app: &App) -> String {
    serde_json::json!({"audio":AudioCfg::current().text(),"theme":app.theme.to_text(),"design":app.design().name(),"plugins":app.state.plugdb.prefs,"monitor":app.state.monitor.cfg.dump()}).to_string()
}
fn apply_preferences(app: &mut App, text: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if let Some(text) = value["audio"].as_str() {
        AudioCfg::parse(text).apply();
    }
    if let Some(text) = value["theme"].as_str() {
        if let Ok(theme) = os_theme::Theme::parse("Browser theme", text) {
            app.theme = theme;
        }
    }
    if value["design"].as_str() == Some("Lemon") {
        app.set_design(os_theme::Design::Lemon, false);
    } else {
        os_theme::set_current(&app.theme.colors);
    }
    if let Ok(prefs) = serde_json::from_value(value["plugins"].clone()) {
        app.state.plugdb.prefs = prefs;
        app.state.plugdb.rebuild(&app.plugins);
    }
    if let Some(text) = value["monitor"].as_str() {
        app.state.monitor.cfg.load(text);
    }
}
pub fn save(app: &mut App) {
    if SAVING.with(Cell::get) {
        app.state.message = Some("A browser save is still in progress".into());
        return;
    }
    let captured: Option<std::collections::BTreeMap<_,_>> = app.plugins.hosted.iter().map(|(&key,h)| {
        let p=h.plugin.try_lock().ok()?;
        Some((key,(p.desc().clone(),p.save_state())))
    }).collect();
    let Some(captured)=captured else {
        SAVE_PENDING.with(|p|p.set(true));
        app.state.message=Some("Waiting for instruments before saving…".into());
        return;
    };
    SAVE_PENDING.with(|p|p.set(false));
    let old_states=captured.iter().map(|(&key,(_,state))|(key,state.clone())).collect();
    // Serialize one captured set of plugin states without locking the audio worker again.
    SAVE_PLUGINS.with_borrow_mut(|p|*p=Some(captured));
    let result=app.project_bytes(&mut |_| {});
    SAVE_PLUGINS.with_borrow_mut(|p|*p=None);
    let save = match result {
        Ok(save) => save,
        Err(e) => {
            app.state.message = Some(format!("Save failed: {e}"));
            return;
        }
    };
    let target = app
        .state
        .path
        .clone()
        .unwrap_or_else(|| os_paths::fs::root().join("Untitled.flp"));
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    if let Err(e) = download_bytes(&name, &save.bytes, "application/octet-stream") {
        app.state.message = Some(format!("Download failed: {e}"));
        return;
    }
    let snapshot = js_sys::Object::new();
    set(&snapshot, "version", &1.into());
    set(&snapshot, "path", &target.to_string_lossy().as_ref().into());
    set(
        &snapshot,
        "project",
        &js_sys::Uint8Array::from(save.bytes.as_slice()),
    );
    set(&snapshot, "preferences", &preferences(app).into());
    let files = js_sys::Array::new();
    for path in os_paths::fs::paths() {
        if let Ok(bytes) = os_paths::fs::read(&path) {
            let f = js_sys::Object::new();
            set(&f, "path", &path.to_string_lossy().as_ref().into());
            set(&f, "bytes", &js_sys::Uint8Array::from(bytes.as_slice()));
            files.push(&f);
        }
    }
    set(&snapshot, "files", &files);
    let (generation, project, tempo, states) = (
        current(),
        app.state.project.clone(),
        app.state.tempo,
        old_states,
    );
    let request = persist(&snapshot);
    SAVING.with(|s| s.set(true));
    app.state.message = Some("Download started; saving a copy in this browser…".into());
    spawn_local(async move {
        let result = match request {
            Ok(p) => JsFuture::from(p).await.map(|_| ()).map_err(error),
            Err(e) => Err(error(e)),
        };
        complete(Completion::Save(
            generation, save, project, tempo, states, target, result,
        ));
    });
}

pub fn poll(app: &mut App) {
    if ONLINE_PENDING.with(Cell::get) && app.plugins.try_set_offline(false) { ONLINE_PENDING.with(|p|p.set(false)); }
    if EXPORT_PENDING.with(|p|p.replace(false)) { app.open_export(false); }
    if SAVE_PENDING.with(Cell::get) { save(app); app.invalidate(); }
    if let Some(snapshot) = RESTORED.with_borrow_mut(Option::take) {
        if let Some(prefs) = get(&snapshot, "preferences").as_string() {
            apply_preferences(app, &prefs);
            PREFS.with_borrow_mut(|p| *p = preferences(app));
        }
        if !get(&snapshot, "version").is_null() && !get(&snapshot, "version").is_undefined() {
            if let Err(e) = restore_project(app, &snapshot) {
                app.state.message = Some(format!(
                    "Could not restore browser project: {e}. The stored copy has been kept."
                ));
            }
        }
        app.invalidate();
    }
    while let Some(completion) = COMPLETED.pop() {
        match completion {
            Completion::Message(message) => {
                app.state.message = Some(message);
                app.invalidate();
            }
            Completion::Save(generation, save, project, tempo, old_states, target, result) => {
                // The browser UI cannot block while the audio worker owns an instrument.
                let Some(current_states) = states(app) else {
                    COMPLETED.push(Completion::Save(generation, save, project, tempo, old_states, target, result));
                    break;
                };
                SAVING.with(|s| s.set(false));
                match result {
                    Ok(())
                        if generation == current()
                            && app.state.project == project
                            && app.state.tempo == tempo
                            && current_states == old_states =>
                    {
                        let message = if save.lost.is_empty() {
                            "Saved in this browser; download started".into()
                        } else {
                            format!(
                                "Saved in this browser; not stored yet: {}",
                                save.lost.join(", ")
                            )
                        };
                        app.adopt_save(save, target);
                        app.state.message = Some(message);
                    }
                    Ok(()) => {
                        app.state.message = Some(
                            "Earlier version saved in this browser; current edits remain unsaved"
                                .into(),
                        )
                    }
                    Err(e) => {
                        app.state.message = Some(format!(
                            "Browser save failed: {e}. Download was started; edits remain unsaved."
                        ))
                    }
                }
                app.invalidate();
            }
            Completion::Sample(generation, row, channel, path, result) => {
                if generation != current() || app.state.project.channels.get(row) != Some(&channel) {
                    continue;
                }
                let result = result.and_then(|sample| {
                    // Keep the worker's decode alive until the sampler takes the cached copy.
                    let loaded = app.plugins.load_sample(row, &path.to_string_lossy());
                    drop(sample);
                    if !loaded {
                        return Err("Lemon Sampler could not load the sample".into());
                    }
                    app.state.project.channels[row].sample_path =
                        Some(path.to_string_lossy().into_owned());
                    app.state.piano.dirty = true;
                    Ok(())
                });
                if let Err(e) = result {
                    os_paths::fs::remove(&os_paths::fs::resolve(&path.to_string_lossy()));
                    app.state.message = Some(format!("Import failed: {e}"));
                }
                app.invalidate();
            }
            Completion::Import(generation, purpose, channel, result) => {
                if generation != current() {
                    continue;
                }
                match result {
                    Ok(files) if !files.is_empty() => {
                        if let Err(e) = apply_import(app, purpose, channel, files) {
                            app.state.message = Some(format!("Import failed: {e}"));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => app.state.message = Some(format!("Import failed: {e}")),
                }
                app.invalidate();
            }
        }
    }
    dirty(app.unsaved());
    let now = js_sys::Date::now();
    if PREFS_CHECK.with(|t| {
        if now - t.get() < 500.0 {
            true
        } else {
            t.set(now);
            false
        }
    }) {
        return;
    }
    let prefs = preferences(app);
    let changed = PREFS.with_borrow_mut(|old| {
        if *old == prefs {
            false
        } else {
            *old = prefs.clone();
            true
        }
    });
    if changed {
        spawn_local(async move {
            let result = match save_preferences(&prefs) {
                Ok(p) => JsFuture::from(p).await.map(|_| ()).map_err(error),
                Err(e) => Err(error(e)),
            };
            if let Err(e) = result {
                complete(Completion::Message(format!(
                    "Preferences could not be saved in this browser: {e}"
                )));
            }
        });
    }
}
fn apply_import(
    app: &mut App,
    purpose: Import,
    channel: Option<os_model::Channel>,
    files: Vec<Imported>,
) -> Result<(), String> {
    let prefix = PathBuf::from(format!("/imports/{:.0}-{}", js_sys::Date::now(), current()));
    match purpose {
        Import::Project | Import::Folder => {
            let projects: Vec<_> = files
                .iter()
                .filter(|f| {
                    f.path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("flp"))
                })
                .collect();
            if projects.len() != 1 {
                return Err("Select exactly one FLP project, with its sample folders".into());
            }
            let project = projects[0];
            let parsed = os_flp::FlpFile::parse(&project.bytes).map_err(|e| e.to_string())?;
            os_flp::project::load(&parsed).map_err(|e| e.to_string())?;
            if !app.confirm_discard("opening another project") {
                return Ok(());
            }
            let path = prefix.join(&project.path);
            let bytes = project.bytes.clone();
            os_paths::fs::clear();
            for file in files {
                if prefix.join(&file.path) != path {
                    os_paths::fs::insert(prefix.join(file.path), file.bytes);
                }
            }
            app.open_bytes(path, Ok(bytes));
        }
        Import::Sample(row) => {
            let file = files.into_iter().next().unwrap();
            let name = file.path;
            let channel = channel.ok_or("The destination channel no longer exists")?;
            if app.state.project.channels.get(row) != Some(&channel) {
                return Err("The destination channel changed during import".into());
            }
            if !channel.kind.is_sampler() {
                return Err("This channel is not a sampler".into());
            }
            let path = os_paths::fs::import(&name.to_string_lossy(), file.bytes).map_err(|e| e.to_string())?;
            let generation = current();
            let physical = os_paths::fs::resolve(&path.to_string_lossy());
            wasm_thread::spawn(move || {
                let result = os_engine::sample::load_shared(&physical).map_err(|e| e.to_string());
                complete(Completion::Sample(generation, row, channel, path, result));
            });
        }
        Import::Theme => {
            let file = files.into_iter().next().unwrap();
            let theme = crate::app::theme_from_bytes(&file.path, &file.bytes)?;
            app.theme = theme;
            os_theme::set_current(&app.theme.colors);
            app.state.message = Some("Theme imported".into());
        }
    }
    Ok(())
}

fn restore_project(app: &mut App, snapshot: &JsValue) -> Result<(), String> {
    if get(snapshot, "version").as_f64() != Some(1.0) {
        return Err("Unsupported storage version".into());
    }
    let project = get(snapshot, "project");
    if !project.is_instance_of::<js_sys::Uint8Array>() {
        return Err("Missing project bytes".into());
    }
    let array = js_sys::Uint8Array::new(&project);
    if array.length() as usize > os_paths::fs::MAX_IMPORT_BYTES {
        return Err("Stored project is too large".into());
    }
    let bytes = array.to_vec();
    let parsed = os_flp::FlpFile::parse(&bytes).map_err(|e| e.to_string())?;
    os_flp::project::load(&parsed).map_err(|e| e.to_string())?;
    let path = get(snapshot, "path")
        .as_string()
        .ok_or("Missing project name")?;
    os_paths::fs::relative_path(path.strip_prefix("/imports/").unwrap_or(&path))
        .map_err(|e| e.to_string())?;
    let files = imported(get(snapshot, "files"), true)?;
    os_paths::fs::clear();
    for file in files {
        os_paths::fs::insert(file.path, file.bytes);
    }
    app.open_bytes(path.into(), Ok(bytes));
    Ok(())
}
