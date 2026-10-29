// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

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
    let work = Some(os_model::WorkTime { started: 1.0, spent: 2.0 });
    b.state.project.work_time = work;
    // This app's ids, which the song does not carry (the edited pattern's too).
    b.state.control.settle(&mut b.state.project);
    let ids = |p: &os_model::Project| (p.channels.iter().map(|c| c.id.0).collect::<Vec<_>>(), p.patterns[0].id.0, p.mixer[1].id.0);
    let before = ids(&b.state.project);
    assert!(before.0.iter().chain([&before.1, &before.2]).all(|&i| i != 0));
    let song = edited(|a| {
        // A note in front of the selected one, another tempo and automatic crossfades.
        a.state.project.patterns[0].notes = vec![note(96, 60), note(0, 64)];
        a.state.tempo = 140.0;
        a.state.project.auto_crossfades = Some(true);
    });
    b.live_adopt(song.clone()).unwrap();
    assert_eq!(b.live_document(), song, "nothing of its own to report");
    assert_eq!((b.state.piano.scroll_x, b.state.tempo), (300.0, 140.0));
    assert_eq!((b.state.project.auto_crossfades, b.state.project.work_time), (Some(true), work), "the song's setting, the file's own record");
    let sel = b.state.piano.selection(1, Some(0));
    assert_eq!(sel.iter().map(|&i| piano::notes(&b.state)[i].position).collect::<Vec<_>>(), [96], "the selection follows its note");
    assert!(!piano::undo(&mut b.state), "no undo step");
    assert_eq!(ids(&b.state.project), before, "the ids stay with their things");
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
    crate::history::ctrl_z(&mut b.state);
    crate::history::ctrl_shift_z(&mut b.state);
    assert_eq!(live::activity().1, [false, true]);
    live::disconnect();
    b.live_disconnected();
}

#[test]
fn presence_names_the_pointer_selection_and_drag_in_song_terms() {
    let mut a = App::new(None);
    let grid = Rect::new(300.0, 200.0, 800.0, 400.0);
    a.pl_grid = Some(grid);
    a.hits.push(Hit { rect: grid, action: Action::PlaylistArea, hint: "".into(), win: Some(Win::Playlist), menu: 0, reveal: None });
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

#[test]
fn a_remote_pointer_moving_repaints_around_it_and_a_shared_change_repaints_everything() {
    let (Ok(mut part), Ok(mut full)) = (os_ui::Canvas::new(1920, 1080, 1.0), os_ui::Canvas::new(1920, 1080, 1.0)) else { return };
    let mut a = App::new(None);
    a.paint(&mut part, None);
    let grid = a.pl_grid.expect("the playlist shows");
    let g = PGeom::new(&a.state, grid);
    let tick = |share: f32| f64::from(g.tick_at(grid.x + grid.w * share));
    let arrangement = a.presence().arrangement;
    let show = |tick: f64| {
        let state = marks::Presence { arrangement, view: Some(View::Playlist), pointer: Some([tick, 1.5]), ..Default::default() };
        let state = serde_json::to_value(state).unwrap();
        marks::show(vec![live::Presence { id: "b".into(), name: "Bob".into(), color: [0xf5, 0x82, 0x31], state }]);
    };
    // Someone new: their name tag is measured as they are first drawn.
    show(tick(0.4));
    a.ease_marks();
    let _ = a.take_damage();
    a.paint(&mut part, None);
    // They move: their pointer and tag ease along, painted again where they were and are, not the grid around them.
    show(tick(0.64));
    a.ease_marks();
    let d = a.take_damage().flatten();
    a.paint_region(&mut part, d.as_deref());
    std::thread::sleep(std::time::Duration::from_millis(100));
    a.ease_marks();
    let d = a.take_damage();
    let Some(Some(rects)) = &d else { panic!("repaints regions, not the frame: {d:?}") };
    let area: f32 = rects.iter().map(|r| r.w * r.h).sum();
    assert!(area < grid.w * grid.h / 20.0, "{area} of {}", grid.w * grid.h);
    a.paint_region(&mut part, Some(rects));
    a.paint(&mut full, None);
    let differ = part.pixmap.data().iter().zip(full.pixmap.data()).filter(|(x, y)| x.abs_diff(**y) > 3).count();
    assert_eq!(differ, 0, "the partial paint is the full one");
    // A shared change repaints everything.
    a.live_adopt(edited(|b| b.state.tempo = 99.0)).unwrap();
    assert_eq!(a.take_damage(), Some(None));
}

#[test]
fn a_mixer_point_stays_on_its_control_in_a_mixer_of_another_height() {
    use crate::{knobs::KnobId, mixer::MHit, views::mixer};
    let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
    let mut a = App::new(None);
    a.state.z = vec![Win::Mixer];
    let mut at_height = |a: &mut App, h: f32| {
        let rect = os_ui::Rect::new(100.0, 120.0, 1200.0, h);
        a.state.wins[Win::Mixer.slot().unwrap()] = crate::app::WinState { rect, maxed: false, mini: false };
        a.paint(&mut c, None);
        let rect = |action: Action| a.hits.iter().rev().find(|h| h.action == action).map(|h| h.rect).unwrap();
        let (pan, led, fader) = (rect(Action::Knob(KnobId::MixPan(2))), rect(Action::Mix(MHit::Led(2))), rect(Action::Fader(2)));
        // Fixed-size controls between the meter and the fader, and the fader's travel, which scales.
        let spots = [(pan.cx(), pan.cy()), (led.cx(), led.cy()), (fader.cx(), fader.y + 5.0), (fader.cx(), fader.bottom() - 5.0)];
        (rect(Action::Mix(MHit::Strip(2))), spots)
    };
    let (tall, spots) = at_height(&mut a, 900.0);
    let points = spots.map(|p| mixer::strip_point(tall, p));
    let (short, spots) = at_height(&mut a, 450.0);
    for (p, (x, y)) in points.into_iter().zip(spots) {
        let (px, py) = mixer::strip_xy(short, p);
        assert!((px - x).abs() < 0.01 && (py - y).abs() < 0.01, "{p:?} lands at {px}, {py}, not {x}, {y}");
    }
}
