// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Live editing in a Playground session (web only; the logic also builds for tests). Playground's shell drives it
//! through `playground_live`, and nothing here runs until the shell connects. The song is shared as JSON ([`doc`]);
//! a remote change merges into it at the start of a quiet frame, keeping this person's views and unsent edits, and is
//! no step of the project's undo history. While connected, Undo and Redo go to the session, which undoes only this
//! person's changes. The others' pointers, selections and drags show in the playlist, the piano roll, the channel rack
//! and the mixer ([`marks`]).
//! User samples the song plays travel to the people who lack them.

// Natively only the tests reach it: the shell's calls are the web's.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod doc;
mod marks;

use super::*;
use crate::playground_live::{self as live, Live};
use serde_json::Value;
use std::cell::RefCell;

pub(crate) use marks::{draw_mixer, draw_piano, draw_playlist, draw_rack};

/// The shared document's format: its number goes up when the song's JSON changes form.
pub(crate) const FORMAT: &str = concat!("lemon-studio:1:", env!("CARGO_PKG_VERSION"));

thread_local! {
    /// Sessions ended so far, and per undo history (by where it lives) the count it was last checked at.
    static ENDED: RefCell<(u64, Vec<(usize, u64)>)> = RefCell::default();
    /// The shared parts of the app as last compared, and their generation (see `live_generation`).
    static SEEN: RefCell<(Option<doc::Song>, u64)> = RefCell::default();
}

/// The top of the project history's Undo and Redo (`redo`; `steps`: the history). In a session the session undoes or
/// redoes this person's last change instead: true. After a session the history starts again from the song it left
/// (its steps may hold the others' work as it was before).
pub(crate) fn undo<T: Default>(steps: &mut T, redo: bool) -> bool {
    if live::undo(redo) {
        return true;
    }
    ENDED.with_borrow_mut(|(ended, seen)| {
        let at = steps as *const T as usize;
        match seen.iter_mut().find(|s| s.0 == at) {
            Some(s) if s.1 == *ended => {}
            Some(s) => {
                s.1 = *ended;
                *steps = T::default();
            }
            None => {
                seen.push((at, *ended));
                if *ended > 0 {
                    *steps = T::default();
                }
            }
        }
    });
    false
}

/// Each frame, before the app's tick: the remote changes waiting and the report of this person's, then the others'
/// marks.
pub(crate) fn frame(app: &mut App) {
    live::frame(app);
    app.ease_marks();
}

/// A sample path that names a user's file: relative, inside the imported files. Placeholders of bundled content
/// (`%...%`) and paths on someone's disk are not anyone's to send.
#[cfg(target_arch = "wasm32")]
fn user_sample(path: &str) -> bool {
    !path.starts_with('%') && os_paths::fs::relative_path(path).is_ok()
}

/// The user samples among sample `paths`, each once, in order.
#[cfg(target_arch = "wasm32")]
fn samples<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut ids: Vec<String> = paths.filter(|p| user_sample(p)).map(str::to_owned).collect();
    ids.sort();
    ids.dedup();
    ids
}

impl Live for App {
    fn live_document(&self) -> Value {
        doc::encode(&self.state.project, self.state.tempo, &doc::Rack::from_ui(&self.state.rack))
    }

    fn live_adopt(&mut self, document: Value) -> Result<(), String> {
        let song = doc::decode(&document).ok_or("Invalid shared song")?;
        self.adopt_song(song);
        Ok(())
    }

    /// No button or drag held, no text box, menu or confirmation open (they hold rows and indices), no project
    /// loading or export, no typed notes held, no plugin or sample request waiting for the next tick.
    fn live_quiet(&self) -> bool {
        let s = &self.state;
        self.drag.is_none()
            && self.pressed.is_none()
            && !self.loading()
            && self.typing_held.is_empty()
            && s.menu_open.is_none()
            && !s.lane_menu
            && s.playlist.edit.is_none()
            && s.playlist.popup.is_none()
            && s.pat_ui.name_box.is_none()
            && s.pat_ui.menu_at.is_none()
            && !s.pat_ui.confirm
            && !s.browser_search_focus
            && s.rack.pending_delete.is_none()
            && s.rack.delete_now.is_none()
            && s.rack.fill.is_none()
            && s.pending_plugin.is_none()
            && s.pending_open.is_none()
            && s.sample_loads.is_empty()
            && s.mixer.pending.is_empty()
    }

    /// A new number whenever the shared parts differ from the last frame's (a compare, no JSON).
    fn live_generation(&self) -> Option<u64> {
        let s = &self.state;
        let rack = doc::Rack::from_ui(&s.rack);
        SEEN.with_borrow_mut(|(seen, n)| {
            let same = seen.as_ref().is_some_and(|x| x.tempo == s.tempo && x.rack == rack && x.project == s.project);
            if !same {
                *seen = Some(doc::Song { project: s.project.clone(), tempo: s.tempo, rack });
                *n += 1;
            }
            Some(*n)
        })
    }

    fn live_disconnected(&mut self) {
        ENDED.with_borrow_mut(|e| e.0 += 1);
    }

    #[cfg(target_arch = "wasm32")]
    fn live_assets(&self) -> Vec<(String, String, bool)> {
        let paths = self.state.project.channels.iter().filter_map(|c| c.sample_path.as_deref());
        (samples(paths).into_iter())
            .map(|id| {
                let name = id.rsplit(['/', '\\']).next().unwrap_or(&id).to_owned();
                let have = os_paths::fs::exists(os_paths::fs::resolve(&id));
                (id, name, have)
            })
            .collect()
    }

    #[cfg(target_arch = "wasm32")]
    fn live_assets_of(document: &Value) -> Vec<String> {
        let channels = document["channels"].as_array().into_iter().flatten();
        samples(channels.filter_map(|c| c["sample_path"].as_str()))
    }

    #[cfg(target_arch = "wasm32")]
    fn live_read_asset(&self, id: &str) -> Option<Vec<u8>> {
        let played = self.state.project.channels.iter().any(|c| c.sample_path.as_deref() == Some(id));
        (played && user_sample(id)).then(|| os_paths::fs::read(os_paths::fs::resolve(id)).ok())?
    }

    /// Keeps a sample that arrived as an import keeps one (this tab's imported files, stored with the project on
    /// the next save), never over a file already here: two people's different files under one name each keep their
    /// own. It counts toward the tab's 256 MiB of imports like any import, so a save stays restorable. The channels
    /// playing it start again with it. No song change, no undo step.
    #[cfg(target_arch = "wasm32")]
    fn live_write_asset(&mut self, id: &str, _name: &str, bytes: Vec<u8>) {
        if !user_sample(id) || os_paths::fs::exists(os_paths::fs::resolve(id)) || os_paths::fs::import(id, bytes).is_err() {
            return;
        }
        let rows: Vec<usize> = (self.state.project.channels.iter().enumerate())
            .filter(|(_, c)| c.kind.is_sampler() && c.sample_path.as_deref() == Some(id))
            .map(|(r, _)| r)
            .collect();
        self.restart(&rows, &[]);
        self.state.playlist.waves.borrow_mut().remove(id);
        self.state.piano.dirty = true;
        self.invalidate();
    }

    fn live_presence(&self) -> Value {
        serde_json::to_value(self.presence()).unwrap_or(Value::Null)
    }

    fn live_show_presence(&mut self, others: Vec<live::Presence>) {
        marks::show(others);
    }

    fn live_merge(base: &Value, current: &Value, edited: &Value) -> Result<Value, String> {
        doc::merge(base, current, edited)
    }

    fn live_check(document: &Value) -> Result<(), String> {
        doc::check(document)
    }
}

/// Channels that are the same wherever they sit: their shared JSON without the index.
fn moved_key(c: &Channel) -> Value {
    let mut v = doc::channel(c);
    v["index"] = Value::Null;
    v
}

/// What starts a hosted instrument: a change re-creates it from the song.
type Instrument<'a> = (ChannelKind, &'a Option<String>, &'a Option<Vec<u8>>, &'a Option<String>, &'a [os_model::Envelope; 5], &'a os_model::ChannelSound, &'a os_model::ChannelMisc);

fn instrument(c: Option<&Channel>) -> Option<Instrument<'_>> {
    c.map(|c| (c.kind, &c.plugin, &c.plugin_state, &c.sample_path, &c.envelopes, &c.sound, &c.misc))
}

/// `new` takes over what is this person's from `old`: the parts a channel keeps of this browser's file and rack (by
/// the channel it was: the same one wherever it sits, else the one on its row with the same kind, name, plugin and
/// sample), and this person's own copies of patterns, arrangements, automation clips and mixer tracks the song has
/// unchanged (their notes and clips in this person's order).
fn keep_local(old: &Project, new: &mut Project) {
    let olds: Vec<Value> = old.channels.iter().map(moved_key).collect();
    let mut used = vec![false; olds.len()];
    for (row, c) in new.channels.iter_mut().enumerate() {
        let key = moved_key(c);
        let free = |i: usize, used: &[bool]| !used[i];
        let similar = |o: &Channel| (o.kind, &o.name, &o.plugin, &o.sample_path) == (c.kind, &c.name, &c.plugin, &c.sample_path);
        let found = (olds.get(row) == Some(&key) && free(row, &used)).then_some(row)
            .or_else(|| (0..olds.len()).find(|&i| free(i, &used) && olds[i] == key))
            .or_else(|| old.channels.get(row).filter(|o| similar(o) && free(row, &used)).map(|_| row));
        if let Some(i) = found {
            used[i] = true;
            let o = old.channels[i].rack;
            (c.rack.origin, c.rack.locked, c.rack.zipped, c.rack.group) = (o.origin, o.locked, o.zipped, o.group);
        }
    }
    for p in &mut new.patterns {
        if let Some(o) = old.patterns.iter().find(|o| o.number == p.number && doc::pattern_value(o) == doc::pattern_value(p)) {
            p.clone_from(o);
        }
    }
    for a in &mut new.arrangements {
        if let Some(o) = old.arrangements.iter().find(|o| o.number == a.number && doc::arrangement_value(o) == doc::arrangement_value(a)) {
            a.clone_from(o);
        }
    }
}

impl App {
    /// Installs a shared song as an undo step's restore would, with no step of its own: this person's views stay,
    /// their selections follow what they select, and the instruments and effects whose settings changed start again
    /// from the song on the next tick.
    fn adopt_song(&mut self, song: doc::Song) {
        let doc::Song { project: mut p, tempo, rack } = song;
        let s = &mut self.state;
        let old = &s.project;
        (p.saved_by, p.tempo, p.current_pattern, p.current_arrangement) =
            (old.saved_by.clone(), old.tempo, old.current_pattern, old.current_arrangement);
        (p.groups, p.current_group, p.playlist_view) = (old.groups.clone(), old.current_group, old.playlist_view);
        // The file's own records: the mode it was saved in, the working time, Show info on opening, the data folder
        // and the initial values.
        (p.song_mode, p.work_time, p.show_info) = (old.song_mode, old.work_time, old.show_info);
        (p.data_folder, p.init_controls) = (old.data_folder.clone(), old.init_controls.clone());
        keep_local(old, &mut p);
        // What changed for the hosted instruments (by rack row) and effects (by mixer track and slot).
        let rows: Vec<usize> = (0..old.channels.len().max(p.channels.len()))
            .filter(|&r| instrument(old.channels.get(r)) != instrument(p.channels.get(r)))
            .collect();
        fn slot(m: &[MixerTrack], t: u16, k: usize) -> Option<&os_model::SlotPlugin> {
            m.iter().find(|x| x.index == t)?.slots[k].as_ref()
        }
        let tracks: std::collections::BTreeSet<u16> = old.mixer.iter().chain(&p.mixer).map(|m| m.index).collect();
        let slots: Vec<(u16, u8)> = (tracks.into_iter())
            .flat_map(|t| (0..10).map(move |k| (t, k)))
            .filter(|&(t, k)| slot(&old.mixer, t, usize::from(k)) != slot(&p.mixer, t, usize::from(k)))
            .collect();
        // Selections by name, before and after.
        let channel = piano::channel(s);
        let note_ids = doc::note_ids(piano::notes(s));
        let notes: Vec<String> = s.piano.selection(s.pattern, channel).iter().filter_map(|&i| note_ids.get(i).cloned()).collect();
        let clip_ids = doc::clip_ids(crate::playlist::clips(s));
        let clips: Vec<String> = s.playlist.sel.iter().filter_map(|&i| clip_ids.get(i).cloned()).collect();
        s.project = p;
        s.tempo = tempo;
        rack.put(&mut s.rack);
        let at = |ids: Vec<String>, names: &[String]| -> Vec<usize> { names.iter().filter_map(|n| ids.iter().position(|i| i == n)).collect() };
        if let (false, Some(ch)) = (notes.is_empty(), piano::channel(s)) {
            let range = s.piano.range;
            s.piano.select(s.pattern, ch, at(doc::note_ids(piano::notes(s)), &notes));
            s.piano.range = range;
        }
        s.playlist.sel = at(doc::clip_ids(crate::playlist::clips(s)), &clips);
        (s.playlist.hover_clip, s.playlist.hover_handle, s.playlist.last_clip) = (None, None, None);
        // Views on rack rows that are gone.
        let n = s.project.channels.len();
        s.rack.sel.retain(|&r| r < n);
        s.rack.zipped.retain(|&r| r < n);
        s.rack.soloed = s.rack.soloed.filter(|&r| r < n);
        s.selected_channel = s.selected_channel.min(n.saturating_sub(1));
        if s.plugin_channel >= n {
            s.hide(Win::Plugin);
            s.plugin_channel = s.selected_channel;
        }
        if s.rack.sel.is_empty() && n > 0 {
            s.rack.select_only(s.selected_channel);
        }
        self.restart(&rows, &slots);
        self.state.piano.dirty = true;
        self.invalidate();
    }

    /// Drops the hosted instruments of rack `rows` and the effects in mixer `slots`: the next tick hosts them again
    /// from the song, as it hosts those of an opened project.
    fn restart(&mut self, rows: &[usize], slots: &[(u16, u8)]) {
        use crate::plugins::{fx_key, fx_of};
        if rows.is_empty() && slots.is_empty() {
            return;
        }
        for key in rows.iter().copied().chain(slots.iter().map(|&(t, k)| fx_key(t, k))) {
            self.plugins.close_if_open(key);
        }
        self.plugins.remap(&|r| (!rows.contains(&r)).then_some(r));
        self.plugins.remap_fx(&|t, k| (!slots.contains(&(t, k))).then_some((t, k)));
        let gone = |key: usize| match fx_of(key) {
            Some(tk) => slots.contains(&tk),
            None => rows.contains(&key),
        };
        self.state.hosted_rows.retain(|&r| !gone(r));
        self.sampler_failed.retain(|&r| !gone(r));
        self.fx_failed.retain(|&k| !gone(k));
        self.wrap_failed.retain(|&k| !gone(k));
        self.native_failed.retain(|&k| !gone(k));
        self.transport.set_instruments(self.plugins.instruments());
        self.transport.set_effects(self.plugins.effects());
    }
}

/// The app as winit runs it on the web, with the session's turns: the shared song is merged and reported (and the
/// others' marks eased) before each tick, and while connected Ctrl+Y and Ctrl+Shift+Z redo through the session.
#[cfg(target_arch = "wasm32")]
pub(crate) mod web {
    use super::{App, live};
    use crate::Shell;
    use std::cell::{Cell, RefCell};
    use std::rc::{Rc, Weak};
    use winit::application::ApplicationHandler;
    use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
    use winit::event_loop::ActiveEventLoop;
    use winit::keyboard::Key;
    use winit::window::WindowId;

    thread_local! {
        static SHELL: RefCell<Weak<RefCell<Shell>>> = RefCell::default();
        /// The app ran a tick: the song this browser restored at start is in, and nothing replaces a session's song
        /// with it later.
        static READY: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) struct Handle(Rc<RefCell<Shell>>);

    impl Handle {
        /// Wraps the app and offers it to Playground's shell (a page without the shell never connects).
        pub(crate) fn new(shell: Shell) -> Handle {
            let shell = Rc::new(RefCell::new(shell));
            SHELL.set(Rc::downgrade(&shell));
            // The shell calls between frames; a frame under way, or the app not started yet, is busy.
            let with_app = |f: &mut dyn FnMut(&mut App)| -> bool {
                let Some(shell) = SHELL.with_borrow(Weak::upgrade).filter(|_| READY.get()) else { return false };
                let Ok(mut shell) = shell.try_borrow_mut() else { return false };
                f(&mut shell.app);
                true
            };
            if let Err(e) = live::web::attach::<App>(super::FORMAT, with_app, crate::wake_ui) {
                web_sys::console::error_2(&"Live editing is unavailable:".into(), &e);
            }
            Handle(shell)
        }
    }

    impl ApplicationHandler for Handle {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            self.0.borrow_mut().resumed(el);
        }

        fn user_event(&mut self, el: &ActiveEventLoop, e: ()) {
            self.0.borrow_mut().user_event(el, e);
        }

        fn about_to_wait(&mut self, el: &ActiveEventLoop) {
            let mut shell = self.0.borrow_mut();
            if READY.get() {
                super::frame(&mut shell.app);
            }
            shell.about_to_wait(el);
            READY.set(true);
        }

        fn device_event(&mut self, el: &ActiveEventLoop, id: DeviceId, e: DeviceEvent) {
            self.0.borrow_mut().device_event(el, id, e);
        }

        fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            let mut shell = self.0.borrow_mut();
            if let WindowEvent::KeyboardInput { event: key, .. } = &event
                && key.state == ElementState::Pressed
                && !key.repeat
                && crate::ctrl(shell.mods)
                && live::connected()
                && let Key::Character(c) = &key.logical_key
                && (c.eq_ignore_ascii_case("y") || shell.mods.shift_key() && c.eq_ignore_ascii_case("z"))
            {
                live::undo(true);
                return;
            }
            shell.window_event(el, id, event);
        }
    }
}

#[cfg(test)]
mod tests;
