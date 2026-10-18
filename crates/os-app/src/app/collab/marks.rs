// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Where each person in a live session is, for the others: their pointer, their selected clips and notes, and the
//! clips or notes they are dragging, the box they are selecting with, the steps they are painting in the channel rack
//! and the fader or knob they are turning, as it happens (the song itself changes only when a gesture ends). All in
//! the song's terms (ticks, tracks, keys, shared names, rack rows and steps, mixer tracks and controls), so each person
//! draws the others through their own scroll, zoom and window layout, eased toward where the others last were.

use super::doc;
use crate::app::{Action, App, Drag, Hit, State, Ui, Win};
use crate::knobs::KnobId;
use crate::mixer::MHit;
use crate::piano::{self, Geom};
use crate::playlist::{self as pl, PGeom, PlDrag};
use crate::views::piano::TOP_KEY;
use crate::views::{mixer, rack};
use os_ui::{Color, Font, Rect};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};

/// The editor a pointer, a drag or a selecting box is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum View {
    Playlist,
    Piano,
    Rack,
    Mixer,
}

/// A control in the mixer or the rack: a track's fader, a knob, a mixer button.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Control {
    Fader(u16),
    Knob(KnobId),
    Mix(MHit),
}

impl Control {
    /// The control a hit is (a strip's background is none).
    fn of(a: &Action) -> Option<Control> {
        match *a {
            Action::Fader(n) => Some(Control::Fader(n)),
            Action::Knob(id) => Some(Control::Knob(id)),
            Action::Mix(MHit::Strip(_)) => None,
            Action::Mix(h) => Some(Control::Mix(h)),
            _ => None,
        }
    }

    fn action(self) -> Action {
        match self {
            Control::Fader(n) => Action::Fader(n),
            Control::Knob(id) => Action::Knob(id),
            Control::Mix(h) => Action::Mix(h),
        }
    }
}

/// One person's presence. Rows are a playlist track's index, a piano-roll key's row from the top (131 minus the key)
/// or a rack row, plus how far down it (0 its top, 1 its bottom). Selections are the shared names of the items; while
/// a drag runs they name the items where it found them, and `ghosts` has them (tick, row, length in ticks) where they
/// are now. In the rack a pointer is (step, row) and `ghosts` the steps painted (step, row, 1 on or 0 off); in the
/// mixer it is (track plus how far across its strip, the strip's height as measured at 1080p, see
/// `views::mixer::strip_point`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Presence {
    /// The arrangement the playlist shows, and the clips selected in it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrangement: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<String>,
    /// The pattern and channel the piano roll edits, and the notes selected there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<View>,
    /// (tick, row).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ghosts: Vec<[f64; 3]>,
    /// A selecting box: (tick, row) of two corners.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<[f64; 4]>,
    /// The fader or knob dragged in the mixer or the rack, and its value (0..1) now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<(Control, f64)>,
    /// The control under the pointer in the mixer or the rack, and where on it (0..1 across and down): it may sit
    /// elsewhere against the pointer's point in a window of another size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over: Option<(Control, [f64; 2])>,
}

/// The most items a selection or a drag carries.
const MAX_ITEMS: usize = 50;
/// The most characters of a name tag.
const MAX_NAME: usize = 24;
/// How long the others' marks take to go most (1 - 1/e) of the way to where they last were, in seconds.
const EASE: f64 = 0.08;

/// Ticks in hundredths and rows in thousandths: finer than a pixel at any zoom, and short.
fn fine(x: f64, k: f64) -> f64 {
    (x * k).round() / k
}

/// A playlist point (tick, row) of `at`, on geometry `g`.
fn playlist_at(g: &PGeom, (x, y): (f32, f32)) -> [f64; 2] {
    let t = g.track_at(y);
    let h = g.rows.height(t).max(1.0);
    let down = ((y - g.y_of(t)) / h).clamp(0.0, 1.0);
    [fine(f64::from(g.tick_at(x)), 100.0), fine(f64::from(t) + f64::from(down), 1000.0)]
}

/// A piano-roll point (tick, row) of `at`, on geometry `g`.
fn piano_at(g: &Geom, (x, y): (f32, f32)) -> [f64; 2] {
    let row = (y - g.grid.y + g.scroll_y) / g.row.max(1.0);
    [fine(f64::from(g.tick_at(x)), 100.0), fine(f64::from(row), 1000.0)]
}

/// The mixer strips painted (`hits`), topmost first: track and hit rect.
fn strips(hits: &[Hit]) -> impl Iterator<Item = (u16, Rect)> + '_ {
    hits.iter().rev().filter_map(|h| match h.action {
        Action::Mix(MHit::Strip(n)) => Some((n, h.rect)),
        _ => None,
    })
}

/// A mixer point (track and how far across its strip, measured y) of `at`, over the strips in `hits`.
fn mixer_at(hits: &[Hit], (x, y): (f32, f32)) -> Option<[f64; 2]> {
    let (n, strip) = strips(hits).find(|(_, r)| x >= r.x && x < r.right())?;
    let (across, abs) = mixer::strip_point(strip, (x, y));
    Some([fine(f64::from(n) + f64::from(across), 1000.0), fine(f64::from(abs), 100.0)])
}

/// The names of `selected` items of a list whose `dragged` items a drag has moved: named as the drag found them.
fn names_before<T: Copy>(items: &[T], dragged: &[(usize, T)], selected: &[usize], ids: fn(&[T]) -> Vec<String>) -> Vec<String> {
    let mut found = items.to_vec();
    for &(i, x) in dragged {
        if let Some(slot) = found.get_mut(i) {
            *slot = x;
        }
    }
    let names = ids(&found);
    selected.iter().filter_map(|&i| names.get(i).cloned()).take(MAX_ITEMS).collect()
}

impl App {
    /// This person's presence, for the others.
    pub(crate) fn presence(&self) -> Presence {
        let s = &self.state;
        let mut me = Presence {
            arrangement: s.project.arrangement().map(|a| a.number),
            pattern: Some(s.pattern),
            channel: piano::channel(s),
            ..Presence::default()
        };
        // The playlist's selection and drag.
        let clips = pl::clips(s);
        let clip_drag = match &self.drag {
            Some(Drag::Pl(PlDrag::Move { orig, .. } | PlDrag::ResizeL { orig, .. } | PlDrag::ResizeR { orig, .. })) => Some(orig),
            _ => None,
        };
        match clip_drag {
            Some(orig) => {
                let at: Vec<usize> = orig.iter().map(|o| o.0).collect();
                me.clips = names_before(clips, orig, &at, doc::clip_ids);
                me.ghosts = (at.iter().filter_map(|&i| clips.get(i)))
                    .map(|c| [f64::from(c.position), f64::from(c.track), f64::from(c.length)])
                    .take(MAX_ITEMS)
                    .collect();
                me.view = Some(View::Playlist);
            }
            None => me.clips = names_before(clips, &[], &s.playlist.sel, doc::clip_ids),
        }
        // The piano roll's.
        let notes = piano::notes(s);
        let note_drag = match &self.drag {
            Some(Drag::NoteMove { orig, .. } | Drag::NoteResize { orig, .. }) => Some(orig),
            _ => None,
        };
        match note_drag {
            Some(orig) => {
                let at: Vec<usize> = orig.iter().map(|o| o.0).collect();
                me.notes = names_before(notes, orig, &at, doc::note_ids);
                me.ghosts = (at.iter().filter_map(|&i| notes.get(i)))
                    .map(|n| [f64::from(n.position), f64::from(TOP_KEY - i32::from(n.key)), f64::from(piano::shown_len(s, n))])
                    .take(MAX_ITEMS)
                    .collect();
                me.view = Some(View::Piano);
            }
            None => me.notes = names_before(notes, &[], &s.piano.selection(s.pattern, me.channel), doc::note_ids),
        }
        // The rack's step painting: the steps it crossed as they are now, those nearest the pointer when there are many.
        if let Some(Drag::RackPaint { row, last, span, .. }) = self.drag
            && let Some(c) = s.project.channels.get(row)
        {
            let notes = s.project.pattern(s.pattern).map_or(&[][..], |p| &p.notes);
            let ps = crate::rack::per_step(s);
            let near = MAX_ITEMS as u32;
            let (from, to) = (span.0.max(last.saturating_sub(near)), span.1.min(last.saturating_add(near)));
            let mut steps: Vec<u32> = (from..=to).collect();
            steps.sort_by_key(|k| k.abs_diff(last));
            me.ghosts = (steps.into_iter().take(MAX_ITEMS))
                .map(|k| [f64::from(k), row as f64, f64::from(u8::from(crate::rack::step_on(notes, c.index, k, ps)))])
                .collect();
            me.view = Some(View::Rack);
        }
        // A fader or knob turned, in the window its hit is in. A knob holds the cursor on it while it turns
        // (`cursor_locked`): the pointer waits at its lower right corner, clear of the needle.
        let mut held = None;
        match self.drag {
            Some(Drag::Fader { track, .. }) => {
                let v = s.project.mixer.iter().find(|m| m.index == track).and_then(|m| m.volume).unwrap_or(0.8);
                me.control = Some((Control::Fader(track), v));
                me.view = Some(View::Mixer);
            }
            Some(Drag::Knob { id, .. }) => {
                let hit = self.hits.iter().rev().find(|h| h.action == Action::Knob(id));
                me.view = match hit.and_then(|h| h.win) {
                    Some(Win::Mixer) => Some(View::Mixer),
                    Some(Win::ChannelRack) => Some(View::Rack),
                    _ => None,
                };
                me.control = me.view.map(|_| (Control::Knob(id), f64::from(s.knob(id))));
                held = hit.map(|h| (h.rect.right() - 1.0, h.rect.bottom() - 1.0));
            }
            _ => {}
        }
        // A selecting box, in the editor it is drawn in.
        let pg = self.pl_grid.map(|grid| PGeom::new(s, grid));
        let rg = self.piano_grid.map(|grid| Geom::new(s, grid));
        let corners = |a: [f64; 2], b: [f64; 2]| [a[0], a[1], b[0], b[1]];
        match (&self.drag, s.playlist.overlay, s.piano.overlay, &pg, &rg) {
            (Some(Drag::Pl(PlDrag::Marquee { .. })), Some(pl::Overlay::Marquee(r)), _, Some(g), _) => {
                me.band = Some(corners(playlist_at(g, (r.x, r.y)), playlist_at(g, (r.right(), r.bottom()))));
                me.view = Some(View::Playlist);
            }
            (Some(Drag::Marquee { .. }), _, Some(piano::Overlay::Marquee(r)), _, Some(g)) => {
                me.band = Some(corners(piano_at(g, (r.x, r.y)), piano_at(g, (r.right(), r.bottom()))));
                me.view = Some(View::Piano);
            }
            _ => {}
        }
        // The pointer, over the editor on top under it (or the one a gesture runs in).
        let (x, y) = held.unwrap_or(self.mouse);
        let over = self.top_hit(x, y).and_then(|h| h.win);
        let view = me.view.or(match over {
            Some(Win::Playlist) if self.pl_grid.is_some_and(|g| g.contains(x, y)) => Some(View::Playlist),
            Some(Win::PianoRoll) if self.piano_grid.is_some_and(|g| g.contains(x, y)) => Some(View::Piano),
            Some(Win::ChannelRack) if self.rack_grid().is_some_and(|g| g.contains(x, y)) => Some(View::Rack),
            Some(Win::Mixer) if self.mixer_grid().is_some_and(|g| g.contains(x, y)) => Some(View::Mixer),
            _ => None,
        });
        me.pointer = match (view, &pg, &rg) {
            (Some(View::Playlist), Some(g), _) => Some(playlist_at(g, (x, y))),
            (Some(View::Piano), _, Some(g)) => Some(piano_at(g, (x, y))),
            (Some(View::Rack), ..) => rack::point_at(s, (x, y)).map(|[k, row]| [fine(k, 100.0), fine(row, 1000.0)]),
            (Some(View::Mixer), ..) => mixer_at(&self.hits, (x, y)),
            _ => None,
        };
        let win = match view {
            Some(View::Rack) => Some(Win::ChannelRack),
            Some(View::Mixer) => Some(Win::Mixer),
            _ => None,
        };
        let on = |r: Rect| [fine(f64::from((x - r.x) / r.w.max(1.0)), 1000.0), fine(f64::from((y - r.y) / r.h.max(1.0)), 1000.0)];
        me.over = (self.top_hit(x, y).filter(|h| win.is_some() && h.win == win && me.pointer.is_some()))
            .and_then(|h| Some((Control::of(&h.action)?, on(h.rect))));
        me.view = view.filter(|_| me.pointer.is_some() || !me.ghosts.is_empty() || me.band.is_some() || me.control.is_some());
        me
    }

    /// The rack's rows panel, when the rack shows.
    fn rack_grid(&self) -> Option<Rect> {
        self.state.shown(Win::ChannelRack).then(|| rack::panel(rack::rect(&self.state)))
    }

    /// The mixer's strips as last painted.
    fn mixer_grid(&self) -> Option<Rect> {
        self.hits.iter().find(|h| h.action == Action::MixerArea).map(|h| h.rect)
    }
}

/// Another person, and their marks as last drawn: eased toward their latest presence.
struct Peer {
    id: String,
    name: String,
    color: Color,
    state: Presence,
    shown: Shown,
    /// Their name tag's size as last painted (unknown before).
    tag: Cell<Option<(f32, f32)>>,
}

#[derive(Clone, Default, PartialEq)]
struct Shown {
    pointer: Option<[f64; 2]>,
    ghosts: Vec<[f64; 3]>,
    band: Option<[f64; 4]>,
    value: Option<f64>,
}

#[derive(Default)]
struct Peers {
    list: Vec<Peer>,
    /// The others as the editors last drew them, when the list changed since.
    drawn: Option<Vec<Peer>>,
    last: Option<crate::time::Instant>,
}

thread_local! {
    static PEERS: RefCell<Peers> = RefCell::default();
}

/// Whether two presences are in the same place: the same editor on the same arrangement, or pattern and channel.
fn same_place(a: &Presence, b: &Presence) -> bool {
    a.view == b.view
        && match a.view {
            Some(View::Playlist) => a.arrangement == b.arrangement,
            Some(View::Piano) => (a.pattern, a.channel) == (b.pattern, b.channel),
            Some(View::Rack) => a.pattern == b.pattern,
            Some(View::Mixer) | None => true,
        }
}

/// The others, each time the session's list changes (empty when it ends). It comes from other people: an entry whose
/// presence does not read is left out, lists are cut short. Someone still in the same place keeps their marks where
/// they were drawn, to ease on from there.
pub(crate) fn show(others: Vec<crate::playground_live::Presence>) {
    let mut list: Vec<Peer> = (others.into_iter())
        .filter_map(|o| {
            let mut state: Presence = serde_json::from_value(o.state).ok()?;
            state.clips.truncate(MAX_ITEMS);
            state.notes.truncate(MAX_ITEMS);
            state.ghosts.truncate(MAX_ITEMS);
            let mut name: String = o.name.chars().take(MAX_NAME).collect();
            if o.name.chars().count() > MAX_NAME {
                name.pop();
                name.push('\u{2026}');
            }
            let [r, g, b] = o.color;
            let color = Color::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b));
            Some(Peer { id: o.id, name, color, state, shown: Shown::default(), tag: Cell::default() })
        })
        .collect();
    PEERS.with_borrow_mut(|peers| {
        for peer in &mut list {
            let Some(old) = peers.list.iter().find(|o| o.id == peer.id) else { continue };
            if old.name == peer.name {
                peer.tag.set(old.tag.get());
            }
            if same_place(&old.state, &peer.state) {
                peer.shown = old.shown.clone();
            }
        }
        let old = std::mem::replace(&mut peers.list, list);
        peers.drawn.get_or_insert(old);
    });
}

/// Pixels per tick and per row of an editor, and its size: how far a mark moves on screen.
#[derive(Clone, Copy)]
struct Scale {
    tick: f64,
    row: f64,
    view: (f64, f64),
}

/// A remote number worth drawing.
fn sane(x: f64) -> bool {
    x.is_finite() && x.abs() < 1e9
}

/// `shown` moved `k` of the way to `target`, all of it after a jump of more than a quarter of the view or once under
/// half a pixel is left. `unit`: which scale each part is in (0 ticks, 1 rows).
fn ease<const N: usize>(shown: &mut [f64; N], target: [f64; N], unit: [usize; N], k: f64, scale: Scale) {
    let px = |i: usize, d: f64| d.abs() * [scale.tick, scale.row][unit[i]];
    let far = (0..N).any(|i| px(i, target[i] - shown[i]) > [scale.view.0, scale.view.1][unit[i]] / 4.0);
    if far || target.iter().any(|x| !sane(*x)) {
        *shown = target;
        return;
    }
    for i in 0..N {
        shown[i] += (target[i] - shown[i]) * k;
    }
    if (0..N).all(|i| px(i, target[i] - shown[i]) < 0.5) {
        *shown = target;
    }
}

/// `shown` a step of `k` on toward `state`, in the editor it is in at that editor's scale (`scales`, by view); at once
/// when that editor is not painted.
fn step(state: &Presence, shown: &mut Shown, k: f64, scales: [Option<Scale>; 4]) {
    let value = state.control.map(|c| c.1).filter(|v| sane(*v));
    let Some(scale) = state.view.and_then(|v| scales[v as usize]) else {
        (shown.pointer, shown.ghosts, shown.band, shown.value) = (state.pointer, state.ghosts.clone(), state.band, value);
        return;
    };
    match (&mut shown.pointer, state.pointer) {
        (Some(at), Some(to)) => ease(at, to, [0, 1], k, scale),
        (at, to) => *at = to,
    }
    // Painted steps stay on their steps.
    if shown.ghosts.len() == state.ghosts.len() && state.view != Some(View::Rack) {
        for (at, &to) in shown.ghosts.iter_mut().zip(&state.ghosts) {
            ease(at, to, [0, 1, 0], k, scale);
        }
    } else {
        shown.ghosts = state.ghosts.clone();
    }
    match (&mut shown.band, state.band) {
        (Some(at), Some(to)) => ease(at, to, [0, 1, 0, 1], k, scale),
        (at, to) => *at = to,
    }
    match (&mut shown.value, value) {
        (Some(at), Some(to)) if (to - *at).abs() > 0.002 => *at += (to - *at) * k,
        (at, to) => *at = to,
    }
}

impl App {
    /// The scale of an editor as last painted.
    fn scale(&self, view: View) -> Option<Scale> {
        let s = &self.state;
        let ppq = f64::from(s.project.ppq.max(4));
        let (grid, tick, row) = match view {
            View::Playlist => (self.pl_grid?, f64::from(s.playlist_bar_w) / (ppq * 4.0), f64::from(s.playlist.row_h)),
            View::Piano => (self.piano_grid?, f64::from(s.piano.px_per_beat) / ppq, f64::from(s.piano.row_h)),
            View::Rack => (self.rack_grid()?, f64::from(rack::STEP_W), f64::from(rack::ROW_H)),
            View::Mixer => (self.mixer_grid()?, f64::from(mixer::STRIP_W), 1.0),
        };
        Some(Scale { tick, row, view: (f64::from(grid.w), f64::from(grid.h)) })
    }

    /// Each frame: the others' marks a step on toward where they are. Only where marks were and are now is painted
    /// again, not the editors around them.
    pub(crate) fn ease_marks(&mut self) {
        let now = crate::time::Instant::now();
        let scales = [View::Playlist, View::Piano, View::Rack, View::Mixer].map(|v| self.scale(v));
        // The list as drawn when it changed since, and the people whose marks moved, with where they were.
        let (drawn, moved) = PEERS.with_borrow_mut(|peers| {
            let dt = peers.last.map_or(0.0, |t| now.duration_since(t).as_secs_f64()).min(0.1);
            peers.last = Some(now);
            let k = 1.0 - (-dt / EASE).exp();
            let drawn = peers.drawn.take();
            let mut moved = Vec::new();
            for (i, peer) in peers.list.iter_mut().enumerate() {
                let was = peer.shown.clone();
                step(&peer.state, &mut peer.shown, k, scales);
                if drawn.is_some() || peer.shown != was {
                    moved.push((i, was));
                }
            }
            (drawn, moved)
        });
        if drawn.is_none() && moved.is_empty() {
            return;
        }
        let s = &self.state;
        let pl = self.pl_grid.map(|grid| (PGeom::new(s, grid), doc::clip_ids(pl::clips(s))));
        let rg = self.piano_grid.map(|grid| (Geom::new(s, grid), doc::note_ids(piano::notes(s))));
        let (rk, mx, hits) = (self.rack_grid(), self.mixer_grid(), &self.hits);
        // Outlines follow the selections, which change only with the list.
        let outlines = drawn.is_some();
        let mut areas = Vec::new();
        let mut area = |peer: &Peer, shown: &Shown| {
            let marks = [
                pl.as_ref().and_then(|(g, ids)| Some((playlist_marks(s, g, ids, &peer.state, shown)?, g.grid))),
                rg.as_ref().and_then(|(g, ids)| Some((piano_marks(s, g, ids, &peer.state, shown)?, g.grid))),
                rk.and_then(|g| Some((rack_marks(s, hits, &peer.state, shown)?, g))),
                mx.and_then(|g| Some((mixer_marks(hits, g, &peer.state, shown)?, g))),
            ];
            for (mut m, grid) in marks.into_iter().flatten() {
                if !outlines {
                    m.outlines.clear();
                }
                m.area(peer.tag.get(), grid, &mut areas);
            }
        };
        PEERS.with_borrow(|peers| {
            for peer in drawn.iter().flatten() {
                area(peer, &peer.shown);
            }
            for (i, was) in &moved {
                let peer = &peers.list[*i];
                area(peer, was);
                area(peer, &peer.shown);
            }
        });
        for r in areas {
            self.damage_rect(r);
        }
    }
}

/// One person's marks in an editor, on screen: the ghosts of what they drag (and of the steps they clear), outlines
/// around what they selected, their selecting box, the control they turn with its knob's needle (from its centre),
/// and the tip of their pointer.
#[derive(Default)]
struct Marks {
    ghosts: Vec<Rect>,
    hollow: Vec<Rect>,
    outlines: Vec<Rect>,
    band: Option<Rect>,
    ring: Option<(Rect, f32)>,
    needle: Option<[(f32, f32); 2]>,
    tip: Option<(f32, f32)>,
}

impl Marks {
    /// What `paint` draws over, inside the editor's `grid` (`tag`: the name tag's size; unknown, the whole grid).
    fn area(&self, tag: Option<(f32, f32)>, grid: Rect, out: &mut Vec<Rect>) {
        let mut add = |r: Rect| out.push(r.intersect(&grid.outset(2.0)));
        // Ghosts' and boxes' edges are 1.5 px lines on their rects, outlines 2 px lines 2 px outside theirs.
        self.ghosts.iter().chain(&self.hollow).for_each(|r| add(r.outset(2.0)));
        self.outlines.iter().for_each(|r| add(r.outset(4.0)));
        self.band.into_iter().for_each(|r| add(r.outset(2.0)));
        self.ring.into_iter().for_each(|(r, _)| add(r.outset(2.0)));
        if let Some([(x0, y0), (x1, y1)]) = self.needle {
            add(Rect::span(x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1)).outset(3.0));
        }
        if let Some((x, y)) = self.tip {
            // The arrow with its white edge and shadow, then the tag.
            add(Rect::new(x - 2.0, y - 2.0, 17.0, 24.0));
            add(tag.map_or(grid, |(w, h)| Rect::new((x + 12.0).round(), (y + 15.0).round(), w, h).outset(1.0)));
        }
    }
}

/// A selecting box, from its corners through an editor's `x` and `y`.
fn band_rect([t0, r0, t1, r1]: [f64; 4], x: impl Fn(f64) -> f32, y: impl Fn(f64) -> f32) -> Rect {
    let (x0, x1, y0, y1) = (x(t0), x(t1), y(r0), y(r1));
    Rect::span(x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1))
}

/// Where `state`'s marks are in the playlist on `g` when shown at `shown` (`ids`: its clips' shared names); None on
/// another arrangement.
fn playlist_marks(s: &State, g: &PGeom, ids: &[String], state: &Presence, shown: &Shown) -> Option<Marks> {
    if state.arrangement != s.project.arrangement().map(|a| a.number) {
        return None;
    }
    let clips = pl::clips(s);
    // A row's top on screen: a track's top plus the share of its height.
    let y = |row: f64| {
        let t = row.floor().clamp(0.0, f64::from(pl::TRACKS - 1));
        g.y_of(t as u16) + ((row - t) as f32) * g.rows.height(t as u16)
    };
    let x = |tick: f64| g.x64(tick) as f32;
    let here = state.view == Some(View::Playlist);
    Some(Marks {
        ghosts: (shown.ghosts.iter())
            .filter(|v| here && v.iter().all(|x| sane(*x)))
            .map(|&[t, row, len]| Rect::new(x(t), y(row), (x(t + len.max(0.0)) - x(t)).max(3.0), g.rows.clip_h()))
            .collect(),
        outlines: (state.clips.iter())
            .filter_map(|id| ids.iter().position(|i| i == id))
            .map(|i| g.clip_rect(&clips[i]))
            .collect(),
        band: shown.band.filter(|b| here && b.iter().all(|x| sane(*x))).map(|b| band_rect(b, x, y)),
        tip: shown.pointer.filter(|p| here && p.iter().all(|x| sane(*x))).map(|[t, row]| (x(t), y(row))),
        ..Marks::default()
    })
}

/// Where `state`'s marks are in the piano roll on `g` when shown at `shown` (`ids`: its notes' shared names); None
/// on another pattern or channel.
fn piano_marks(s: &State, g: &Geom, ids: &[String], state: &Presence, shown: &Shown) -> Option<Marks> {
    if (state.pattern, state.channel) != (Some(s.pattern), piano::channel(s)) {
        return None;
    }
    let notes = piano::notes(s);
    let y = |row: f64| g.grid.y + row as f32 * g.row - g.scroll_y;
    let x = |tick: f64| g.x_of(tick as f32);
    let here = state.view == Some(View::Piano);
    Some(Marks {
        ghosts: (shown.ghosts.iter())
            .filter(|v| here && v.iter().all(|x| sane(*x)))
            .map(|&[t, row, len]| Rect::new(x(t), y(row) - 1.0, (x(t + len.max(0.0)) - x(t)).max(3.0), g.row + 1.0))
            .collect(),
        outlines: (state.notes.iter())
            .filter_map(|id| ids.iter().position(|i| i == id))
            .filter_map(|i| g.note_rect(&notes[i]))
            .collect(),
        band: shown.band.filter(|b| here && b.iter().all(|x| sane(*x))).map(|b| band_rect(b, x, y)),
        tip: shown.pointer.filter(|p| here && p.iter().all(|x| sane(*x))).map(|[t, row]| (x(t), y(row))),
        ..Marks::default()
    })
}

/// The hit rect of `action` in `win` as last painted (`hits`).
fn hit(hits: &[Hit], action: &Action, win: Win) -> Option<Rect> {
    hits.iter().rev().find(|h| h.win == Some(win) && h.action == *action).map(|h| h.rect)
}

type Xy = (f32, f32);

/// `tip`, the pointer of `state` mapped by `at`, put on the control it is over when that control is in `win` here
/// (`hits`): by as much as the control sits elsewhere against `at`'s mapping.
fn on_control(tip: Option<Xy>, state: &Presence, hits: &[Hit], win: Win, at: impl Fn([f64; 2]) -> Option<Xy>) -> Option<Xy> {
    let (Some((x, y)), Some((c, [fx, fy])), Some(p)) = (tip, state.over, state.pointer) else { return tip };
    let (Some(r), Some((px, py)), true) = (hit(hits, &c.action(), win), at(p), sane(fx) && sane(fy)) else { return tip };
    Some((x + r.x + fx.clamp(0.0, 1.0) as f32 * r.w - px, y + r.y + fy.clamp(0.0, 1.0) as f32 * r.h - py))
}

/// Knob `id` lit in `win` (`hits`), with its needle at value `v` (0..1): from 135° on through 270°, as the knobs draw.
fn knob(m: &mut Marks, hits: &[Hit], id: KnobId, v: f64, win: Win) {
    let Some(r) = hit(hits, &Action::Knob(id), win) else { return };
    let a = std::f32::consts::PI * (0.75 + 1.5 * v.clamp(0.0, 1.0) as f32);
    let (cx, cy, len) = (r.cx(), r.cy(), r.w.min(r.h) / 2.0);
    m.ring = Some((r.outset(1.0), len + 1.0));
    m.needle = Some([(cx, cy), (cx + len * a.cos(), cy + len * a.sin())]);
}

/// Where `state`'s marks are in the channel rack when shown at `shown` (`hits`: as last painted); None on another
/// pattern.
fn rack_marks(s: &State, hits: &[Hit], state: &Presence, shown: &Shown) -> Option<Marks> {
    if state.view != Some(View::Rack) || state.pattern != Some(s.pattern) {
        return None;
    }
    let at = |p: [f64; 2]| rack::point_xy(s, p).filter(|_| p.iter().all(|x| sane(*x)));
    let mut m = Marks { tip: on_control(shown.pointer.and_then(at), state, hits, Win::ChannelRack, at), ..Marks::default() };
    for &[k, row, on] in shown.ghosts.iter().filter(|v| v.iter().all(|x| sane(*x) && *x >= 0.0)) {
        if let Some(r) = rack::step_rect(s, row as usize, k as u32) {
            if on >= 0.5 { m.ghosts.push(r) } else { m.hollow.push(r) }
        }
    }
    if let (Some((Control::Knob(id), _)), Some(v)) = (state.control, shown.value) {
        knob(&mut m, hits, id, v, Win::ChannelRack);
    }
    Some(m)
}

/// Where `state`'s marks are over the mixer's strips (`grid`) when shown at `shown` (`hits`: as last painted). A strip
/// scrolled under the level scale and the master strip shows none.
fn mixer_marks(hits: &[Hit], grid: Rect, state: &Presence, shown: &Shown) -> Option<Marks> {
    if state.view != Some(View::Mixer) {
        return None;
    }
    let master = strips(hits).find(|s| s.0 == 0).map(|s| s.1);
    let seen = |x: f32| x >= grid.x + mixer::INSERTS_X || master.is_some_and(|r| x >= r.x && x < r.right());
    let strip = |n: u16| strips(hits).find(|s| s.0 == n).map(|s| s.1);
    let at = |[t, y]: [f64; 2]| {
        let n = t.floor();
        let ok = sane(y) && (0.0..65536.0).contains(&t);
        Some(mixer::strip_xy(strip(n as u16).filter(|_| ok)?, ((t - n) as f32, y as f32)))
    };
    let tip = on_control(shown.pointer.and_then(at), state, hits, Win::Mixer, at).filter(|p| seen(p.0));
    let mut m = Marks { tip, ..Marks::default() };
    match (state.control, shown.value) {
        (Some((Control::Fader(n), _)), Some(v)) => {
            if let (Some(st), Some(fr)) = (strip(n), hit(hits, &Action::Fader(n), Win::Mixer).filter(|r| seen(r.cx()))) {
                let y = mixer::fader_y(st, v as f32);
                m.ring = Some((fr.outset(1.0), 4.0));
                m.ghosts.push(Rect::new(fr.cx() - 7.0, y - 11.0, 14.0, 23.0));
            }
        }
        (Some((Control::Knob(id), _)), Some(v)) => knob(&mut m, hits, id, v, Win::Mixer),
        _ => {}
    }
    m.ring = m.ring.filter(|(r, _)| seen(r.cx()));
    m.needle = m.needle.filter(|n| seen(n[0].0));
    Some(m)
}

/// The others in the channel rack on the same pattern, after its rows: the steps they paint, the knob they turn and
/// their pointers.
pub(crate) fn draw_rack(ui: &mut Ui, s: &State) {
    PEERS.with_borrow(|peers| {
        if peers.list.is_empty() {
            return;
        }
        let radius = if crate::views::lemon(ui) { 3.0 } else { 2.0 };
        for peer in &peers.list {
            if let Some(marks) = rack_marks(s, &ui.hits[..], &peer.state, &peer.shown) {
                paint(ui, peer, marks, radius);
            }
        }
    });
}

/// The others over the mixer's strips (`grid`), after its cables: the fader or knob they turn and their pointers.
pub(crate) fn draw_mixer(ui: &mut Ui, grid: Rect) {
    PEERS.with_borrow(|peers| {
        if peers.list.is_empty() {
            return;
        }
        let radius = if crate::views::lemon(ui) { 5.0 } else { 3.0 };
        ui.c.push_clip(grid);
        for peer in &peers.list {
            if let Some(marks) = mixer_marks(&ui.hits[..], grid, &peer.state, &peer.shown) {
                paint(ui, peer, marks, radius);
            }
        }
        ui.c.pop_clip();
    });
}

/// The others in the playlist, after its clips: the ghosts of their drags, outlines around their selected clips,
/// their selecting boxes, then their pointers.
pub(crate) fn draw_playlist(ui: &mut Ui, s: &State, g: &PGeom) {
    PEERS.with_borrow(|peers| {
        if peers.list.is_empty() {
            return;
        }
        let ids = doc::clip_ids(pl::clips(s));
        // The clips' own corners.
        let radius = if crate::views::lemon(ui) { os_ui::kit::lemon::r_btn(ui.t) } else { 1.5 };
        for peer in &peers.list {
            if let Some(marks) = playlist_marks(s, g, &ids, &peer.state, &peer.shown) {
                paint(ui, peer, marks, radius);
            }
        }
    });
}

/// The others in the piano roll on the same pattern and channel, after its notes: ghosts, outlines around their
/// selected notes, selecting boxes and pointers.
pub(crate) fn draw_piano(ui: &mut Ui, s: &State, g: &Geom) {
    PEERS.with_borrow(|peers| {
        if peers.list.is_empty() {
            return;
        }
        let ids = doc::note_ids(piano::notes(s));
        let radius = if crate::views::lemon(ui) { 3.5 } else { 2.0 };
        for peer in &peers.list {
            if let Some(marks) = piano_marks(s, g, &ids, &peer.state, &peer.shown) {
                paint(ui, peer, marks, radius);
            }
        }
    });
}

/// The arrow of a pointer, its tip at (0, 0).
const ARROW: [(f32, f32); 7] = [(0.0, 0.0), (0.0, 16.0), (4.5, 12.2), (7.6, 18.6), (10.4, 17.3), (7.3, 11.0), (12.5, 11.0)];

/// One person's marks in their colour: ghosts (their colour at 28 %, the edge at 85 %; only the edge for cleared
/// steps), outlines (2 px), the selecting box (dashed), the control turned (2 px) with its knob's needle, then the
/// pointer with a name tag.
fn paint(ui: &mut Ui, peer: &Peer, Marks { ghosts, hollow, outlines, band, ring, needle, tip }: Marks, radius: f32) {
    let c = peer.color;
    // Far off-screen items are cut to near the view, keeping their edges there.
    let near = ui.c.clip_rect().outset(8.0);
    let cut = |r: Rect| Some(r.intersect(&near)).filter(|r| !r.is_empty());
    for r in ghosts.into_iter().filter_map(cut) {
        ui.c.fill_rrect(r, radius, c.with_alpha(71));
        ui.c.stroke_rrect(r, radius, 1.5, c.with_alpha(217));
    }
    for r in hollow.into_iter().filter_map(cut) {
        ui.c.stroke_rrect(r, radius, 1.5, c.with_alpha(217));
    }
    for r in outlines.into_iter().filter_map(cut) {
        ui.c.stroke_rrect(r.outset(2.0), radius + 1.0, 2.0, c);
    }
    if let Some(r) = band.and_then(cut) {
        dashed(ui, r, c);
    }
    if let Some((r, rr)) = ring {
        ui.c.stroke_rrect(r, rr, 2.0, c);
    }
    if let Some([(x0, y0), (x1, y1)]) = needle {
        ui.c.line(x0, y0, x1, y1, 2.5, c);
    }
    let Some((x, y)) = tip else { return };
    let at = |dx: f32, dy: f32| ARROW.iter().map(|&(px, py)| (x + px + dx, y + py + dy)).collect::<Vec<_>>();
    ui.c.fill_polygon(&at(0.0, 1.0), Color::rgb(0).with_alpha(64));
    ui.c.fill_polygon(&at(0.0, 0.0), c);
    let mut edge = at(0.0, 0.0);
    edge.push(edge[0]);
    ui.c.polyline(&edge, 1.5, Color::rgb(0xffffff));
    if peer.name.is_empty() {
        peer.tag.set(Some((0.0, 0.0)));
        return;
    }
    let font = Font::new(12).bold();
    let (w, h) = (ui.c.text_width(font, &peer.name), ui.c.line_height(font));
    let tag = Rect::new((x + 12.0).round(), (y + 15.0).round(), (w + 14.0).ceil(), (h + 6.0).ceil());
    peer.tag.set(Some((tag.w, tag.h)));
    ui.c.fill_rrect(tag, 6.0, c);
    ui.c.text(font, tag.x + 7.0, tag.y + 3.0, ink(c), &peer.name);
}

/// White on a dark colour, near black on a light one (relative luminance over 0.55).
fn ink(c: Color) -> Color {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
    if l > 0.55 { Color::rgb(0x111111) } else { Color::rgb(0xffffff) }
}

/// A 1.5 px outline of `r` in 4 px dashes 3 px apart.
fn dashed(ui: &mut Ui, r: Rect, c: Color) {
    let edges = [(r.x, r.y, r.right(), r.y), (r.right(), r.y, r.right(), r.bottom()), (r.right(), r.bottom(), r.x, r.bottom()), (r.x, r.bottom(), r.x, r.y)];
    for (x0, y0, x1, y1) in edges {
        let len = (x1 - x0).abs() + (y1 - y0).abs();
        let mut d = 0.0;
        while d < len {
            let (a, b) = (d / len.max(1.0), ((d + 4.0).min(len)) / len.max(1.0));
            ui.c.line(x0 + (x1 - x0) * a, y0 + (y1 - y0) * a, x0 + (x1 - x0) * b, y0 + (y1 - y0) * b, 1.5, c);
            d += 7.0;
        }
    }
}
