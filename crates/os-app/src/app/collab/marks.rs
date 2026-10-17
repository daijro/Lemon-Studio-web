// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

//! Where each person in a live session is, for the others: their pointer, their selected clips and notes, and the
//! clips or notes they are dragging, the box they are selecting with, as it happens (the song itself changes only when
//! a gesture ends). All in the song's terms (ticks, tracks, keys, shared names), so each person draws the others
//! through their own scroll and zoom, eased toward where the others last were.

use super::doc;
use crate::app::{App, Drag, State, Ui, Win};
use crate::piano::{self, Geom};
use crate::playlist::{self as pl, PGeom, PlDrag};
use crate::views::piano::TOP_KEY;
use os_ui::{Color, Font, Rect};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

/// The editor a pointer, a drag or a selecting box is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum View {
    Playlist,
    Piano,
}

/// One person's presence. Rows are a playlist track's index or a piano-roll key's row from the top (131 minus the
/// key), plus how far down it (0 its top, 1 its bottom). Selections are the shared names of the items; while a drag
/// runs they name the items where it found them, and `ghosts` has them (tick, row, length in ticks) where they are now.
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
        let (x, y) = self.mouse;
        let over = self.top_hit(x, y).and_then(|h| h.win);
        let view = me.view.or(match over {
            Some(Win::Playlist) if self.pl_grid.is_some_and(|g| g.contains(x, y)) => Some(View::Playlist),
            Some(Win::PianoRoll) if self.piano_grid.is_some_and(|g| g.contains(x, y)) => Some(View::Piano),
            _ => None,
        });
        me.pointer = match (view, &pg, &rg) {
            (Some(View::Playlist), Some(g), _) => Some(playlist_at(g, (x, y))),
            (Some(View::Piano), _, Some(g)) => Some(piano_at(g, (x, y))),
            _ => None,
        };
        me.view = view.filter(|_| me.pointer.is_some() || !me.ghosts.is_empty() || me.band.is_some());
        me
    }
}

/// Another person, and their marks as last drawn: eased toward their latest presence.
struct Peer {
    id: String,
    name: String,
    color: Color,
    state: Presence,
    shown: Shown,
}

#[derive(Default)]
struct Shown {
    pointer: Option<[f64; 2]>,
    ghosts: Vec<[f64; 3]>,
    band: Option<[f64; 4]>,
}

#[derive(Default)]
struct Peers {
    list: Vec<Peer>,
    /// The others changed since the editors last drew them.
    changed: bool,
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
            None => true,
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
            Some(Peer { id: o.id, name, color, state, shown: Shown::default() })
        })
        .collect();
    PEERS.with_borrow_mut(|peers| {
        for peer in &mut list {
            if let Some(old) = peers.list.iter_mut().find(|o| o.id == peer.id && same_place(&o.state, &peer.state)) {
                peer.shown = std::mem::take(&mut old.shown);
            }
        }
        peers.list = list;
        peers.changed = true;
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
/// half a pixel is left. `unit`: which scale each part is in (0 ticks, 1 rows). Whether it is still on its way.
fn ease<const N: usize>(shown: &mut [f64; N], target: [f64; N], unit: [usize; N], k: f64, scale: Scale) -> bool {
    let px = |i: usize, d: f64| d.abs() * [scale.tick, scale.row][unit[i]];
    let far = (0..N).any(|i| px(i, target[i] - shown[i]) > [scale.view.0, scale.view.1][unit[i]] / 4.0);
    if far || target.iter().any(|x| !sane(*x)) {
        *shown = target;
        return false;
    }
    for i in 0..N {
        shown[i] += (target[i] - shown[i]) * k;
    }
    if (0..N).all(|i| px(i, target[i] - shown[i]) < 0.5) {
        *shown = target;
        return false;
    }
    true
}

impl App {
    /// The scale of an editor as last painted.
    fn scale(&self, view: View) -> Option<Scale> {
        let s = &self.state;
        let ppq = f64::from(s.project.ppq.max(4));
        let (grid, tick, row) = match view {
            View::Playlist => (self.pl_grid?, f64::from(s.playlist_bar_w) / (ppq * 4.0), f64::from(s.playlist.row_h)),
            View::Piano => (self.piano_grid?, f64::from(s.piano.px_per_beat) / ppq, f64::from(s.piano.row_h)),
        };
        Some(Scale { tick, row, view: (f64::from(grid.w), f64::from(grid.h)) })
    }

    /// Each frame: the others' marks a step on toward where they are, and the editors they show in painted again
    /// while any moves or the list changed.
    pub(crate) fn ease_marks(&mut self) {
        let now = crate::time::Instant::now();
        let scales = [self.scale(View::Playlist), self.scale(View::Piano)];
        let redraw = PEERS.with_borrow_mut(|peers| {
            let dt = peers.last.map_or(0.0, |t| now.duration_since(t).as_secs_f64()).min(0.1);
            peers.last = Some(now);
            let k = 1.0 - (-dt / EASE).exp();
            let mut moving = false;
            for peer in &mut peers.list {
                let (state, shown) = (&peer.state, &mut peer.shown);
                let scale = state.view.and_then(|v| scales[v as usize]);
                let Some(scale) = scale else {
                    (shown.pointer, shown.ghosts, shown.band) = (state.pointer, state.ghosts.clone(), state.band);
                    continue;
                };
                match (&mut shown.pointer, state.pointer) {
                    (Some(at), Some(to)) => moving |= ease(at, to, [0, 1], k, scale),
                    (at, to) => *at = to,
                }
                if shown.ghosts.len() == state.ghosts.len() {
                    for (at, &to) in shown.ghosts.iter_mut().zip(&state.ghosts) {
                        moving |= ease(at, to, [0, 1, 0], k, scale);
                    }
                } else {
                    shown.ghosts = state.ghosts.clone();
                }
                match (&mut shown.band, state.band) {
                    (Some(at), Some(to)) => moving |= ease(at, to, [0, 1, 0, 1], k, scale),
                    (at, to) => *at = to,
                }
            }
            let redraw = moving || peers.changed;
            peers.changed = false;
            redraw
        });
        if redraw {
            for grid in [self.pl_grid, self.piano_grid].into_iter().flatten() {
                self.damage_rect(grid.outset(2.0));
            }
        }
    }
}

/// The others in the playlist, after its clips: the ghosts of their drags, outlines around their selected clips,
/// their selecting boxes, then their pointers.
pub(crate) fn draw_playlist(ui: &mut Ui, s: &State, g: &PGeom) {
    PEERS.with_borrow(|peers| {
        if peers.list.is_empty() {
            return;
        }
        let here = s.project.arrangement().map(|a| a.number);
        let clips = pl::clips(s);
        let ids = doc::clip_ids(clips);
        // A row's top on screen: a track's top plus the share of its height.
        let y = |row: f64| {
            let t = row.floor().clamp(0.0, f64::from(pl::TRACKS - 1));
            g.y_of(t as u16) + ((row - t) as f32) * g.rows.height(t as u16)
        };
        let x = |tick: f64| g.x64(tick) as f32;
        // The clips' own corners.
        let radius = if crate::views::lemon(ui) { os_ui::kit::lemon::r_btn(ui.t) } else { 1.5 };
        for peer in peers.list.iter().filter(|p| p.state.arrangement == here) {
            let shown = peer.state.view == Some(View::Playlist);
            let ghosts = (peer.shown.ghosts.iter())
                .filter(|_| shown)
                .filter(|v| v.iter().all(|x| sane(*x)))
                .map(|&[t, row, len]| Rect::new(x(t), y(row), (x(t + len.max(0.0)) - x(t)).max(3.0), g.rows.clip_h()))
                .collect();
            let outlines = (peer.state.clips.iter())
                .filter_map(|id| ids.iter().position(|i| i == id))
                .map(|i| g.clip_rect(&clips[i]))
                .collect();
            let band = peer.shown.band.filter(|_| shown).filter(|b| b.iter().all(|x| sane(*x))).map(|[t0, r0, t1, r1]| {
                let (x0, x1, y0, y1) = (x(t0), x(t1), y(r0), y(r1));
                Rect::span(x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1))
            });
            let tip = (peer.shown.pointer.filter(|_| shown))
                .filter(|p| p.iter().all(|x| sane(*x)))
                .map(|[t, row]| (x(t), y(row)));
            paint(ui, peer, (ghosts, outlines, band, tip), radius);
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
        let place = (Some(s.pattern), piano::channel(s));
        let notes = piano::notes(s);
        let ids = doc::note_ids(notes);
        let y = |row: f64| g.grid.y + row as f32 * g.row - g.scroll_y;
        let x = |tick: f64| g.x_of(tick as f32);
        let radius = if crate::views::lemon(ui) { 3.5 } else { 2.0 };
        for peer in peers.list.iter().filter(|p| (p.state.pattern, p.state.channel) == place) {
            let shown = peer.state.view == Some(View::Piano);
            let ghosts = (peer.shown.ghosts.iter())
                .filter(|_| shown)
                .filter(|v| v.iter().all(|x| sane(*x)))
                .map(|&[t, row, len]| Rect::new(x(t), y(row) - 1.0, (x(t + len.max(0.0)) - x(t)).max(3.0), g.row + 1.0))
                .collect();
            let outlines = (peer.state.notes.iter())
                .filter_map(|id| ids.iter().position(|i| i == id))
                .filter_map(|i| g.note_rect(&notes[i]))
                .collect();
            let band = peer.shown.band.filter(|_| shown).filter(|b| b.iter().all(|x| sane(*x))).map(|[t0, r0, t1, r1]| {
                let (x0, x1, y0, y1) = (x(t0), x(t1), y(r0), y(r1));
                Rect::span(x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1))
            });
            let tip = (peer.shown.pointer.filter(|_| shown))
                .filter(|p| p.iter().all(|x| sane(*x)))
                .map(|[t, row]| (x(t), y(row)));
            paint(ui, peer, (ghosts, outlines, band, tip), radius);
        }
    });
}

/// The arrow of a pointer, its tip at (0, 0).
const ARROW: [(f32, f32); 7] = [(0.0, 0.0), (0.0, 16.0), (4.5, 12.2), (7.6, 18.6), (10.4, 17.3), (7.3, 11.0), (12.5, 11.0)];

/// One person's marks in their colour: ghosts (their colour at 28 %, the edge at 85 %), outlines (2 px), the selecting
/// box (dashed), then the pointer with a name tag.
fn paint(ui: &mut Ui, peer: &Peer, (ghosts, outlines, band, tip): (Vec<Rect>, Vec<Rect>, Option<Rect>, Option<(f32, f32)>), radius: f32) {
    let c = peer.color;
    // Far off-screen items are cut to near the view, keeping their edges there.
    let near = ui.c.clip_rect().outset(8.0);
    let cut = |r: Rect| Some(r.intersect(&near)).filter(|r| !r.is_empty());
    for r in ghosts.into_iter().filter_map(cut) {
        ui.c.fill_rrect(r, radius, c.with_alpha(71));
        ui.c.stroke_rrect(r, radius, 1.5, c.with_alpha(217));
    }
    for r in outlines.into_iter().filter_map(cut) {
        ui.c.stroke_rrect(r.outset(2.0), radius + 1.0, 2.0, c);
    }
    if let Some(r) = band.and_then(cut) {
        dashed(ui, r, c);
    }
    let Some((x, y)) = tip else { return };
    let at = |dx: f32, dy: f32| ARROW.iter().map(|&(px, py)| (x + px + dx, y + py + dy)).collect::<Vec<_>>();
    ui.c.fill_polygon(&at(0.0, 1.0), Color::rgb(0).with_alpha(64));
    ui.c.fill_polygon(&at(0.0, 0.0), c);
    let mut edge = at(0.0, 0.0);
    edge.push(edge[0]);
    ui.c.polyline(&edge, 1.5, Color::rgb(0xffffff));
    if peer.name.is_empty() {
        return;
    }
    let font = Font::new(12).bold();
    let (w, h) = (ui.c.text_width(font, &peer.name), ui.c.line_height(font));
    let tag = Rect::new((x + 12.0).round(), (y + 15.0).round(), (w + 14.0).ceil(), (h + 6.0).ceil());
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
