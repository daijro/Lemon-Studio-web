// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! The shared song of a live session as JSON, and back: one canonical value per song, so equal songs give equal bytes
//! on every peer.
//!
//! What is shared: the project's channels, patterns (notes, control points, markers, loops), mixer, arrangements (clips,
//! tracks, markers), automation clips and song settings, the tempo, and the rack's locks, filter groups and loop
//! lengths. What stays with each person: the current pattern, arrangement and rack filter, the playlist's stored view,
//! which file block each channel came from, the rack's zipped rows and who saved the file.
//!
//! Lists merge item by item. Channels go by their index, patterns and arrangements by number, mixer tracks by index,
//! automation clips by channel and playlist tracks by number. Notes, clips, markers and control points are named by
//! where they sit (moving one removes it and adds another) and listed in one order, whatever order the editors left
//! them in.

use crate::rack::{Group, RackUi};
use os_model::*;
use serde_json::{Map, Number, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The shared song: the project (with this person's views left at their defaults), the tempo and the rack's song
/// settings.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Song {
    pub project: Project,
    pub tempo: f64,
    pub rack: Rack,
}

/// What the channel rack keeps of the song outside the project until a save (`rack::to_model`), by rack row.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Rack {
    pub locked: BTreeSet<usize>,
    pub groups: Vec<Group>,
    pub unsorted_at: Option<usize>,
    pub loop_mode: bool,
    pub loop_type: u8,
    pub loop_len: BTreeMap<usize, u32>,
}

impl Rack {
    pub fn from_ui(r: &RackUi) -> Rack {
        Rack {
            locked: r.locked.clone(),
            groups: r.groups.clone(),
            unsorted_at: r.unsorted_at,
            loop_mode: r.loop_mode,
            loop_type: r.loop_type,
            loop_len: r.loop_len.clone(),
        }
    }

    pub fn put(self, r: &mut RackUi) {
        (r.locked, r.groups, r.unsorted_at) = (self.locked, self.groups, self.unsorted_at);
        (r.loop_mode, r.loop_type, r.loop_len) = (self.loop_mode, self.loop_type, self.loop_len);
    }
}

/// A value as shared JSON and back. Decoding refuses anything a canonical document would not hold.
pub(crate) trait Js: Sized {
    fn js(&self) -> Value;
    fn read(v: &Value) -> Option<Self>;
    /// A list of them: an array, except bytes (hex text).
    fn list(items: &[Self]) -> Value {
        Value::Array(items.iter().map(Js::js).collect())
    }
    fn unlist(v: &Value) -> Option<Vec<Self>> {
        v.as_array()?.iter().map(Js::read).collect()
    }
}

macro_rules! numbers {
    ($($t:ty),*) => {$(
        impl Js for $t {
            fn js(&self) -> Value {
                Value::from(*self)
            }
            fn read(v: &Value) -> Option<Self> {
                match v.as_i64() {
                    Some(n) => n.try_into().ok(),
                    None => v.as_u64()?.try_into().ok(),
                }
            }
        }
    )*};
}
numbers!(u16, u32, i16, i32, i64, usize);

impl Js for u8 {
    fn js(&self) -> Value {
        Value::from(*self)
    }
    fn read(v: &Value) -> Option<Self> {
        v.as_u64()?.try_into().ok()
    }
    fn list(items: &[u8]) -> Value {
        Value::String(items.iter().map(|b| format!("{b:02x}")).collect())
    }
    fn unlist(v: &Value) -> Option<Vec<u8>> {
        let s = v.as_str()?;
        (s.len() % 2 == 0 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect())
            .flatten()
    }
}

/// Bit patterns (stored doubles) as decimal text: JSON numbers stop being exact at 2^53.
impl Js for u64 {
    fn js(&self) -> Value {
        Value::String(self.to_string())
    }
    fn read(v: &Value) -> Option<Self> {
        let s = v.as_str()?;
        s.parse().ok().filter(|n: &u64| n.to_string() == s)
    }
}

/// Numbers, or `NaN` / `inf` / `-inf` as text.
impl Js for f64 {
    fn js(&self) -> Value {
        Number::from_f64(*self).map_or_else(|| Value::String(self.to_string()), Value::Number)
    }
    fn read(v: &Value) -> Option<Self> {
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse().ok().filter(|x: &f64| !x.is_finite()),
            _ => None,
        }
    }
}

/// Exactly, as the double that holds it.
impl Js for f32 {
    fn js(&self) -> Value {
        f64::from(*self).js()
    }
    fn read(v: &Value) -> Option<Self> {
        let x = f64::read(v)?;
        (x.is_nan() || f64::from(x as f32) == x).then_some(x as f32)
    }
}

impl Js for bool {
    fn js(&self) -> Value {
        Value::Bool(*self)
    }
    fn read(v: &Value) -> Option<Self> {
        v.as_bool()
    }
}

impl Js for String {
    fn js(&self) -> Value {
        Value::String(self.clone())
    }
    fn read(v: &Value) -> Option<Self> {
        v.as_str().map(str::to_owned)
    }
}

impl<T: Js> Js for Option<T> {
    fn js(&self) -> Value {
        self.as_ref().map_or(Value::Null, Js::js)
    }
    fn read(v: &Value) -> Option<Self> {
        match v {
            Value::Null => Some(None),
            v => T::read(v).map(Some),
        }
    }
}

impl<T: Js> Js for Vec<T> {
    fn js(&self) -> Value {
        T::list(self)
    }
    fn read(v: &Value) -> Option<Self> {
        T::unlist(v)
    }
}

impl<T: Js, const N: usize> Js for [T; N] {
    fn js(&self) -> Value {
        T::list(self)
    }
    fn read(v: &Value) -> Option<Self> {
        T::unlist(v)?.try_into().ok()
    }
}

impl<A: Js, B: Js> Js for (A, B) {
    fn js(&self) -> Value {
        Value::Array(vec![self.0.js(), self.1.js()])
    }
    fn read(v: &Value) -> Option<Self> {
        match v.as_array()?.as_slice() {
            [a, b] => Some((A::read(a)?, B::read(b)?)),
            _ => None,
        }
    }
}

impl Js for BTreeSet<usize> {
    fn js(&self) -> Value {
        self.iter().copied().collect::<Vec<_>>().js()
    }
    fn read(v: &Value) -> Option<Self> {
        let rows = Vec::<usize>::read(v)?;
        // Strictly ascending: the order a set gives back.
        rows.windows(2).all(|w| w[0] < w[1]).then(|| rows.into_iter().collect())
    }
}

/// Rows as object keys, so two rows' lengths merge apart.
impl Js for BTreeMap<usize, u32> {
    fn js(&self) -> Value {
        Value::Object(self.iter().map(|(r, n)| (r.to_string(), n.js())).collect())
    }
    fn read(v: &Value) -> Option<Self> {
        let row = |k: &String| k.parse().ok().filter(|r: &usize| r.to_string() == *k);
        v.as_object()?.iter().map(|(k, n)| Some((row(k)?, u32::read(n)?))).collect()
    }
}

impl Js for Rgb {
    fn js(&self) -> Value {
        Value::String(format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b))
    }
    fn read(v: &Value) -> Option<Self> {
        let s = v.as_str()?.strip_prefix('#')?;
        let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        let rgb = Rgb::new(byte(0)?, byte(2)?, byte(4)?);
        (s.len() == 6 && rgb.js().as_str() == v.as_str()).then_some(rgb)
    }
}

impl Js for ChannelKind {
    fn js(&self) -> Value {
        match self {
            ChannelKind::Sampler => "sampler".into(),
            ChannelKind::Plugin => "plugin".into(),
            ChannelKind::Layer => "layer".into(),
            ChannelKind::AudioClip => "audio_clip".into(),
            ChannelKind::Automation => "automation".into(),
            ChannelKind::Other(n) => Value::from(*n),
        }
    }
    fn read(v: &Value) -> Option<Self> {
        Some(match v.as_str() {
            Some("sampler") => ChannelKind::Sampler,
            Some("plugin") => ChannelKind::Plugin,
            Some("layer") => ChannelKind::Layer,
            Some("audio_clip") => ChannelKind::AudioClip,
            Some("automation") => ChannelKind::Automation,
            Some(_) => return None,
            None => ChannelKind::Other(<u8 as Js>::read(v)?),
        })
    }
}

impl Js for ClipSource {
    fn js(&self) -> Value {
        match *self {
            ClipSource::Pattern(n) => ("pattern".to_string(), n).js(),
            ClipSource::Channel(n) => ("channel".to_string(), n).js(),
        }
    }
    fn read(v: &Value) -> Option<Self> {
        match <(String, u16)>::read(v)? {
            (kind, n) if kind == "pattern" => Some(ClipSource::Pattern(n)),
            (kind, n) if kind == "channel" => Some(ClipSource::Channel(n)),
            _ => None,
        }
    }
}

impl Js for AutoMode {
    fn js(&self) -> Value {
        self.byte().js()
    }
    fn read(v: &Value) -> Option<Self> {
        <u8 as Js>::read(v).map(AutoMode::from_byte)
    }
}

/// A struct as an object of its fields, all of them: a field added to the model breaks the build here until it is
/// shared or left out on purpose.
macro_rules! records {
    ($($t:ty { $($f:ident),* $(,)? })*) => {$(
        impl Js for $t {
            fn js(&self) -> Value {
                let mut m = Map::new();
                $(m.insert(stringify!($f).into(), self.$f.js());)*
                Value::Object(m)
            }
            fn read(v: &Value) -> Option<Self> {
                Some(Self { $($f: Js::read(v.get(stringify!($f))?)?),* })
            }
        }
    )*};
}

records! {
    ProjectInfo { title, author, genre, comments, url }
    Envelope {
        enabled, delay, attack, hold, decay, sustain, release, amount, attack_tension, decay_tension, release_tension,
        flags, lfo_delay, lfo_attack, lfo_amount, lfo_speed, lfo_shape,
    }
    Tracking { mid, pan, mod_x, mod_y }
    Arpeggio { direction, time, gate, range, repeat, slide, chord }
    Echo { feed, time, pan, mod_x, mod_y, pitch, echoes, ping_pong, fat }
    LayerSettings { flags, crossfade }
    ChannelMisc {
        levels, max_poly, slide, porta, mono, gate, shift, swing, full_porta, tracking, cut, cut_by, arp, echo, root,
        main_pitch, add_key, fine, key_range, layer,
    }
    SampleFx {
        flags, boost, fade_out, fade_in, stereo_delay, pogo, feedback, cutoff, resonance, ring_mix, ring_freq, reverb,
        reverb_type, ping_pong, remove_dc, normalize, polarity, crossfade, trim, start_bits, length_bits,
        loop_follows_reverse,
    }
    ChannelSound { cutoff, resonance, filter_type, sampler_flags, declick, stretch, fx, loop_points, start_offset_bits }
    Channel {
        index, kind, name, plugin, plugin_state, color, enabled, volume, pan, pitch_cents, mixer_track, sample_path,
        layer_children, envelopes, rack, misc, sound,
    }
    Note { position, length, channel, key, fine_pitch, release, pan, velocity, mod_x, mod_y, flags, group, unk14, repeat }
    ControlPoint { position, kind, value }
    TimeMarker { position, kind, name, numerator, denominator }
    SlotPlugin { name, state }
    MixerTrack {
        index, name, color, enabled, slots, routes, send_levels, volume, pan, stereo_separation, polarity, swap,
        slots_enabled, slot_on, slot_mix, eq,
    }
    Clip { position, length, track, source, muted, window, raw }
    PlaylistTrack { number, name, color, enabled, grouped, collapsed, locked }
    AutoPoint { beat, value, tension, mode, flag }
    Automation { channel, targets, points, header, tail }
    Group { name, members }
    Rack { locked, groups, unsorted_at, loop_mode, loop_type, loop_len }
}

/// A channel's rack state is this person's (the file block it came from, the lock, zip and group the rack keeps until
/// a save) except the colour the playlist draws its clips with.
impl Js for ChannelRack {
    fn js(&self) -> Value {
        let mut m = Map::new();
        m.insert("stored_color".into(), self.stored_color.js());
        Value::Object(m)
    }
    fn read(v: &Value) -> Option<Self> {
        Some(ChannelRack { stored_color: Js::read(v.get("stored_color")?)?, ..ChannelRack::default() })
    }
}

/// `v` with an `id`.
fn with_id(mut v: Value, id: Value) -> Value {
    if let Value::Object(m) = &mut v {
        m.insert("id".into(), id);
    }
    v
}

/// Items with their own number as `id`, in their order.
fn numbered<T: Js>(items: &[T], id: impl Fn(&T) -> u32) -> Value {
    Value::Array(items.iter().map(|x| with_id(x.js(), id(x).into())).collect())
}

/// The order a named list is shared in, and each item's name: where it sits, then `#2`, `#3`... for items that sit
/// in the same place. `order` must start with what `name` reads, so items of one name are next to each other.
fn names<T, K: Ord>(items: &[T], order: impl Fn(&T) -> K, name: impl Fn(&T) -> String) -> (Vec<usize>, Vec<String>) {
    let mut sorted: Vec<usize> = (0..items.len()).collect();
    sorted.sort_by_key(|&i| order(&items[i]));
    let mut ids = vec![String::new(); items.len()];
    let mut last: Option<(String, usize)> = None;
    for &i in &sorted {
        let base = name(&items[i]);
        let n = match &last {
            Some((b, n)) if *b == base => n + 1,
            _ => 1,
        };
        ids[i] = if n == 1 { base.clone() } else { format!("{base}#{n}") };
        last = Some((base, n));
    }
    (sorted, ids)
}

fn named<T: Js, K: Ord>(items: &[T], order: impl Fn(&T) -> K, name: impl Fn(&T) -> String) -> Value {
    let (sorted, ids) = names(items, order, name);
    Value::Array(sorted.into_iter().map(|i| with_id(items[i].js(), Value::String(ids[i].clone()))).collect())
}

type NoteOrder = ((u32, u16, u16), (u32, u8, u8, u8, u8, u8, u8, u16, u8, u16, u8));

fn note_order(n: &Note) -> NoteOrder {
    let rest = (n.length, n.velocity, n.fine_pitch, n.release, n.pan, n.mod_x, n.mod_y, n.flags, n.group, n.unk14, n.repeat);
    ((n.position, n.key, n.channel), rest)
}

fn note_name(n: &Note) -> String {
    format!("{}:{}:{}", n.position, n.key, n.channel)
}

/// The shared names of `notes`, in their order (the piano roll's selection, for the others).
pub(crate) fn note_ids(notes: &[Note]) -> Vec<String> {
    names(notes, note_order, note_name).1
}

fn source_key(s: ClipSource) -> (u8, u16) {
    match s {
        ClipSource::Pattern(n) => (0, n),
        ClipSource::Channel(n) => (1, n),
    }
}

type ClipOrder = ((u32, u16, (u8, u16)), (u32, bool, Option<(u64, u64)>, [u8; CLIP_RECORD_LEN]));

fn clip_order(c: &Clip) -> ClipOrder {
    let window = c.window.map(|(a, b)| (a.to_bits(), b.to_bits()));
    ((c.position, c.track, source_key(c.source)), (c.length, c.muted, window, c.raw))
}

fn clip_name(c: &Clip) -> String {
    let (kind, n) = source_key(c.source);
    format!("{}:{}:{}{n}", c.position, c.track, ["p", "c"][usize::from(kind)])
}

/// The shared names of an arrangement's `clips`, in their order.
pub(crate) fn clip_ids(clips: &[Clip]) -> Vec<String> {
    names(clips, clip_order, clip_name).1
}

fn markers(m: &[TimeMarker]) -> Value {
    let order = |m: &TimeMarker| ((m.position, m.kind), (m.name.clone(), m.numerator, m.denominator));
    named(m, order, |m| format!("{}:{}", m.position, m.kind))
}

fn pattern(p: &Pattern) -> Value {
    let controls = named(&p.controls, |c| (c.position, c.kind, c.value), |c| format!("{}:{}", c.position, c.kind));
    let mut m = Map::new();
    m.insert("number".into(), p.number.js());
    m.insert("name".into(), p.name.js());
    m.insert("color".into(), p.color.js());
    m.insert("notes".into(), named(&p.notes, note_order, note_name));
    m.insert("controls".into(), controls);
    m.insert("length".into(), p.length.js());
    m.insert("loops".into(), p.loops.js());
    m.insert("loop_off".into(), p.loop_off.js());
    m.insert("markers".into(), markers(&p.markers));
    Value::Object(m)
}

fn unpattern(v: &Value) -> Option<Pattern> {
    let get = |k: &str| v.get(k);
    Some(Pattern {
        number: Js::read(get("number")?)?,
        name: Js::read(get("name")?)?,
        color: Js::read(get("color")?)?,
        notes: Js::read(get("notes")?)?,
        controls: Js::read(get("controls")?)?,
        length: Js::read(get("length")?)?,
        loops: Js::read(get("loops")?)?,
        loop_off: Js::read(get("loop_off")?)?,
        markers: Js::read(get("markers")?)?,
    })
}

/// An arrangement. Its playlist tracks are mostly as a new project has them: only the others are listed, with the
/// count, when they are numbered from 1 in order (always, as the app makes them).
fn arrangement(a: &Arrangement) -> Value {
    let in_order = a.tracks.iter().enumerate().all(|(i, t)| t.number as usize == i + 1);
    let edited: Vec<PlaylistTrack> = if in_order {
        a.tracks.iter().filter(|t| **t != PlaylistTrack::new(t.number)).cloned().collect()
    } else {
        a.tracks.clone()
    };
    let mut m = Map::new();
    m.insert("number".into(), a.number.js());
    m.insert("name".into(), a.name.js());
    m.insert("clips".into(), named(&a.clips, clip_order, clip_name));
    m.insert("markers".into(), markers(&a.markers));
    m.insert("tracks".into(), numbered(&edited, |t| t.number));
    m.insert("track_count".into(), in_order.then_some(a.tracks.len()).js());
    Value::Object(m)
}

fn unarrangement(v: &Value) -> Option<Arrangement> {
    let get = |k: &str| v.get(k);
    let listed: Vec<PlaylistTrack> = Js::read(get("tracks")?)?;
    let tracks = match <Option<usize>>::read(get("track_count")?)? {
        Some(n) if n <= usize::from(crate::playlist::TRACKS) * 64 => {
            let mut all: Vec<PlaylistTrack> = (1..=n as u32).map(PlaylistTrack::new).collect();
            for t in listed {
                let i = (t.number as usize).checked_sub(1)?;
                *all.get_mut(i)? = t;
            }
            all
        }
        Some(_) => return None,
        None => listed,
    };
    Some(Arrangement {
        number: Js::read(get("number")?)?,
        name: Js::read(get("name")?)?,
        clips: Js::read(get("clips")?)?,
        tracks,
        markers: Js::read(get("markers")?)?,
    })
}

/// The shared JSON of a channel, a pattern, an arrangement: what [`encode`] lists for it.
pub(crate) fn channel(c: &Channel) -> Value {
    c.js()
}

pub(crate) fn pattern_value(p: &Pattern) -> Value {
    pattern(p)
}

pub(crate) fn arrangement_value(a: &Arrangement) -> Value {
    arrangement(a)
}

/// The shared song as JSON.
pub(crate) fn encode(p: &Project, tempo: f64, rack: &Rack) -> Value {
    let mut m = Map::new();
    m.insert("ppq".into(), p.ppq.js());
    m.insert("tempo".into(), tempo.js());
    m.insert("info".into(), p.info.js());
    m.insert("channels".into(), numbered(&p.channels, |c| c.index.into()));
    let patterns = p.patterns.iter().map(|x| with_id(pattern(x), x.number.into())).collect();
    m.insert("patterns".into(), Value::Array(patterns));
    m.insert("mixer".into(), numbered(&p.mixer, |t| t.index.into()));
    let arrangements = p.arrangements.iter().map(|a| with_id(arrangement(a), a.number.into())).collect();
    m.insert("arrangements".into(), Value::Array(arrangements));
    m.insert("automations".into(), numbered(&p.automations, |a| a.channel.into()));
    m.insert("swing".into(), p.swing.js());
    m.insert("bar".into(), p.bar.js());
    m.insert("bar_as_signature".into(), p.bar_as_signature.js());
    m.insert("play_truncated".into(), p.play_truncated.js());
    m.insert("panning_law".into(), p.panning_law.js());
    m.insert("fast_declick".into(), p.fast_declick.js());
    m.insert("auto_crossfades".into(), p.auto_crossfades.js());
    m.insert("main_volume".into(), p.main_volume.js());
    m.insert("main_pitch".into(), p.main_pitch.js());
    m.insert("rack".into(), rack.js());
    Value::Object(m)
}

/// The song a shared document holds, this person's views at their defaults. `None`: not a song.
pub(crate) fn decode(v: &Value) -> Option<Song> {
    let get = |k: &str| v.get(k);
    fn list<T>(v: Option<&Value>, f: fn(&Value) -> Option<T>) -> Option<Vec<T>> {
        v?.as_array()?.iter().map(f).collect()
    }
    let project = Project {
        ppq: Js::read(get("ppq")?)?,
        info: Js::read(get("info")?)?,
        channels: Js::read(get("channels")?)?,
        patterns: list(get("patterns"), unpattern)?,
        mixer: Js::read(get("mixer")?)?,
        arrangements: list(get("arrangements"), unarrangement)?,
        automations: Js::read(get("automations")?)?,
        swing: Js::read(get("swing")?)?,
        bar: Js::read(get("bar")?)?,
        bar_as_signature: Js::read(get("bar_as_signature")?)?,
        play_truncated: Js::read(get("play_truncated")?)?,
        panning_law: Js::read(get("panning_law")?)?,
        fast_declick: Js::read(get("fast_declick")?)?,
        auto_crossfades: Js::read(get("auto_crossfades")?)?,
        main_volume: Js::read(get("main_volume")?)?,
        main_pitch: Js::read(get("main_pitch")?)?,
        ..Project::default()
    };
    Some(Song { project, tempo: Js::read(get("tempo")?)?, rack: Js::read(get("rack")?)? })
}

/// `v` as its song would share it: the one form a song has.
fn canonical(v: &Value) -> Result<Value, String> {
    let song = decode(v).ok_or("Conflict: not a song")?;
    Ok(encode(&song.project, song.tempo, &song.rack))
}

/// The three-way merge of shared songs: field by field and item by item (`playground_live::merge`), then in the
/// song's own form (named lists in their order, names as their items have them).
pub(crate) fn merge(base: &Value, current: &Value, edited: &Value) -> Result<Value, String> {
    canonical(&crate::playground_live::merge(base, current, edited, &[])?)
}

/// Whether `v` is a song every person can take as it is.
pub(crate) fn check(v: &Value) -> Result<(), String> {
    if canonical(v)? == *v {
        Ok(())
    } else {
        Err("Conflict: not a canonical song".into())
    }
}
