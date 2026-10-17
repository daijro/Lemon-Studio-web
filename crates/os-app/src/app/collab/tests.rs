// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

use super::marks::View;
use super::*;
use crate::app::Hit;
use crate::playlist::{self as pl, PGeom, PlDrag};
use os_model::{Clip, ClipSource};
use std::rc::Rc;

fn note(position: u32, key: u16) -> Note {
    Note { length: 48, key, ..Note::step(0, position) }
}

/// The song of a new app after `f`.
fn edited(f: impl FnOnce(&mut App)) -> Value {
    let mut app = App::new(None);
    f(&mut app);
    app.live_document()
}

#[test]
fn the_shared_song_leaves_views_out_and_merges_items_apart() {
    let mut a = App::new(None);
    let base = a.live_document();
    assert_eq!(doc::check(&base), Ok(()));
    let s = &mut a.state;
    (s.piano.scroll_x, s.playlist_scroll, s.pattern, s.project.current_pattern) = (480.0, (100.0, 40.0), 2, Some(2));
    s.project.playlist_view = Some(os_model::PlaylistView::new(96));
    (s.project.channels[1].rack.origin, s.project.saved_by) = (Some(7), "someone".into());
    s.rack.zipped.insert(1);
    assert_eq!(a.live_document(), base, "views and file blocks are this person's");
    // At once: one person adds a note and turns channel 1 down, the other adds a note and pans it.
    let c = edited(|a| {
        a.state.project.patterns[0].notes.push(note(96, 60));
        a.state.project.channels[0].volume = 0.5;
    });
    let e = edited(|a| {
        a.state.project.patterns[0].notes.push(note(0, 62));
        a.state.project.channels[0].pan = -0.5;
    });
    let merged = doc::merge(&base, &c, &e).unwrap();
    assert_eq!(doc::check(&merged), Ok(()));
    let p = doc::decode(&merged).unwrap().project;
    let notes: Vec<_> = p.patterns[0].notes.iter().map(|n| (n.position, n.key)).collect();
    assert_eq!(notes, [(0, 62), (96, 60)], "both notes, in the song's order");
    assert_eq!((p.channels[0].volume, p.channels[0].pan), (0.5, -0.5));
    // Both moving one note is a conflict.
    let moved = |to: u32| edited(|a| a.state.project.patterns[0].notes = vec![note(to, 60)]);
    let one = edited(|a| a.state.project.patterns[0].notes = vec![note(96, 60)]);
    assert!(doc::merge(&one, &moved(0), &moved(192)).is_err());
}

#[test]
fn adopting_keeps_views_and_selections_and_is_no_undo_step() {
    let mut b = App::new(None);
    b.state.project.patterns[0].notes = vec![note(96, 60)];
    b.state.piano.select(1, 0, vec![0]);
    b.state.piano.scroll_x = 300.0;
    let song = edited(|a| {
        // A note in front of the selected one, and another tempo.
        a.state.project.patterns[0].notes = vec![note(96, 60), note(0, 64)];
        a.state.tempo = 140.0;
    });
    b.live_adopt(song.clone()).unwrap();
    assert_eq!(b.live_document(), song, "nothing of its own to report");
    assert_eq!((b.state.piano.scroll_x, b.state.tempo), (300.0, 140.0));
    let sel = b.state.piano.selection(1, Some(0));
    assert_eq!(sel.iter().map(|&i| piano::notes(&b.state)[i].position).collect::<Vec<_>>(), [96], "the selection follows its note");
    assert!(!piano::undo(&mut b.state), "no undo step");
}

#[test]
fn a_remote_change_waits_for_a_quiet_frame_and_undo_goes_to_the_session() {
    let mut b = App::new(None);
    let tempo = b.state.tempo;
    live::connect();
    let base = live::canonical(&b.live_document());
    let done = Rc::new(RefCell::new(None));
    let to = done.clone();
    let song = edited(|a| a.state.tempo = 99.0);
    live::queue(Some(base), live::canonical(&song), Box::new(move |r| *to.borrow_mut() = Some(r)));
    b.drag = Some(Drag::NoteErase);
    frame(&mut b);
    assert!(done.borrow().is_none() && b.state.tempo == tempo, "a drag holds it");
    assert_eq!(live::checkpoint(&b), Err("busy".into()));
    b.drag = None;
    frame(&mut b);
    assert_eq!((done.borrow().clone(), b.state.tempo), (Some(Ok(None)), 99.0));
    assert!(piano::undo(&mut b.state) && pl::undo(&mut b.state));
    assert_eq!(live::activity().1, [false, false]);
    live::disconnect();
    b.live_disconnected();
}

#[test]
fn presence_names_the_pointer_selection_and_drag_in_song_terms() {
    let mut a = App::new(None);
    let grid = Rect::new(300.0, 200.0, 800.0, 400.0);
    a.pl_grid = Some(grid);
    a.hits.push(Hit { rect: grid, action: Action::PlaylistArea, hint: "".into(), win: Some(Win::Playlist) });
    pl::arr_mut(&mut a.state).clips.push(Clip::new(384, 384, 2, ClipSource::Pattern(1)));
    a.state.playlist.sel = vec![0];
    let g = PGeom::new(&a.state, grid);
    a.mouse = (g.x_of(192.0), g.y_of(1) + g.rows.height(1) / 2.0);
    let me = a.presence();
    assert_eq!((me.view, me.pointer, me.clips.as_slice()), (Some(View::Playlist), Some([192.0, 1.5]), ["384:2:p1".to_string()].as_slice()));
    // Mid-drag the clip is at tick 768 on track 3: a ghost there, the selection where the others still have it.
    let found = pl::clips(&a.state)[0];
    let clip = &mut pl::arr_mut(&mut a.state).clips[0];
    (clip.position, clip.track) = (768, 3);
    a.drag = Some(Drag::Pl(PlDrag::Move { x0: 0.0, y0: 0.0, orig: vec![(0, found)], fresh: false }));
    let me = a.presence();
    assert_eq!((me.ghosts.as_slice(), me.clips.as_slice()), ([[768.0, 3.0, 384.0]].as_slice(), ["384:2:p1".to_string()].as_slice()));
}
