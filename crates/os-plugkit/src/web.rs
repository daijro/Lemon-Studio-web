// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

//! Browser windows for the same instrument editor loop used on Linux and macOS.
use std::{collections::{HashMap}, sync::{Arc, Mutex, LazyLock, mpsc, atomic::{AtomicBool, AtomicU8, Ordering}}};
use wasm_bindgen::prelude::*;
use web_time::{Duration, Instant};
pub use crate::input::{keysym, keysym_char, Button, Input};
pub type Parent = u32;
pub type Slot = Arc<Mutex<Option<Waker>>>;
type Reply = mpsc::Sender<Option<String>>;

#[wasm_bindgen(module = "/web/plugin-window.mjs")]
extern "C" {
    fn open(id: u32, w: u32, h: u32, scale: f64, callback: &js_sys::Function);
    fn draw(id: u32, pixels: &[u32], w: u32, h: u32);
    fn close(id: u32);
    fn menu(id: u32, items: &str);
    fn type_in(id: u32, text: &str);
    fn pages(id: u32, count: u32);
    #[wasm_bindgen(catch)] fn download_sample(bytes: &[u8]) -> Result<(), JsValue>;
}

struct Frame { pixels: Vec<u32>, w: u32, h: u32 }
enum Request { Menu(String, Reply), Type(String, Reply), File(bool, Reply), Download(Vec<u8>, mpsc::Sender<Result<(),String>>) }
#[derive(Default)]
struct State {
    events: crossbeam::queue::SegQueue<Input>,
    frame: Mutex<Option<Frame>>,
    requests: crossbeam::queue::SegQueue<Request>,
    reply: Mutex<Option<Reply>>,
    mods: AtomicU8,
    closed: AtomicBool,
    locked: AtomicBool,
    page: AtomicU8,
}
static WINDOWS: LazyLock<Mutex<HashMap<u32, Arc<State>>>> = LazyLock::new(Default::default);
thread_local! { static CALLBACK: std::cell::RefCell<Option<Closure<dyn FnMut(u32, String, f64, f64, f64, f64)>>> = const { std::cell::RefCell::new(None) }; }

#[derive(Clone)]
pub struct Waker(Arc<State>);
impl Waker { pub fn wake(&self) { self.0.events.push(Input::Tick); } }
pub fn notify(slot: &Slot) { if let Some(w) = slot.lock().unwrap().as_ref() { w.wake(); } }
pub struct EditorThread(Arc<State>);
impl Drop for EditorThread { fn drop(&mut self) { self.0.closed.store(true, Ordering::Release); } }

pub fn spawn(parent: Parent, (w,h): (u32,u32), slot: &Slot, run: impl FnOnce(Window) + Send + 'static) -> Option<EditorThread> {
    let state = Arc::new(State::default());
    WINDOWS.lock().unwrap().insert(parent, state.clone());
    *slot.lock().unwrap() = Some(Waker(state.clone()));
    CALLBACK.with_borrow_mut(|cb| {
        let cb = cb.get_or_insert_with(|| Closure::new(event));
        open(parent, w, h, js_sys::Reflect::get(&js_sys::global(), &"devicePixelRatio".into()).ok().and_then(|v| v.as_f64()).unwrap_or(1.0), cb.as_ref().unchecked_ref());
    });
    let worker_state = state.clone();
    wasm_thread::spawn(move || run(Window { state: worker_state, tick: Instant::now() }));
    Some(EditorThread(state))
}

fn event(id: u32, kind: String, x: f64, y: f64, value: f64, mods: f64) {
    let state = WINDOWS.lock().unwrap().get(&id).cloned();
    let Some(s) = state else { return };
    if kind == "frame" {
        if s.closed.load(Ordering::Acquire) {
            s.reply.lock().unwrap().take();
            while s.requests.pop().is_some() {}
            WINDOWS.lock().unwrap().remove(&id);
            close(id);
            return;
        }
        let frame = s.frame.try_lock().ok().and_then(|mut f| f.take());
        if let Some(f) = frame { draw(id, &f.pixels, f.w, f.h); }
        while let Some(request) = s.requests.pop() {
            match request {
                Request::Menu(items, tx) => { *s.reply.lock().unwrap() = Some(tx); menu(id, &items); }
                Request::Type(text, tx) => { *s.reply.lock().unwrap() = Some(tx); type_in(id, &text); }
                Request::Download(bytes, tx) => { let _ = tx.send(download_sample(&bytes).map_err(|e|format!("{e:?}"))); }
                Request::File(save, tx) => {
                    if save { let _ = tx.send(Some("/exports/sample.wav".into())); }
                    else { let state=s.clone(); wasm_bindgen_futures::spawn_local(async move {
                        let result = if let Some(f) = rfd::AsyncFileDialog::new().add_filter("Audio", &["wav","flac","ogg","mp3","aiff"]).pick_file().await {
                            let bytes=f.read().await;
                            if state.closed.load(Ordering::Acquire) { None }
                            else { os_paths::fs::import(&f.file_name(),bytes).ok().map(|p|p.to_string_lossy().into_owned()) }
                        } else { None };
                        let _ = tx.send(result);
                    }); }
                }
            }
        }
        return;
    }
    if kind.starts_with("reply:") {
        if let Some(tx) = s.reply.lock().unwrap().take() { let _ = tx.send(Some(kind[6..].to_string())); }
        return;
    }
    if kind == "cancel" { if let Some(tx) = s.reply.lock().unwrap().take() { let _ = tx.send(None); } return; }
    if kind == "page" { s.page.store(x as u8 + 1, Ordering::Release); return; }
    if kind == "close" { s.closed.store(true, Ordering::Release); return; }
    s.mods.store(mods as u8, Ordering::Relaxed);
    let btn = match value as u8 { 2 => Button::Right, 1 => Button::Middle, _ => Button::Left };
    let input = match kind.as_str() {
        "down" => Input::Down { x:x as f32,y:y as f32,btn,dbl:false },
        "double" => Input::Down { x:x as f32,y:y as f32,btn,dbl:true },
        "up" => Input::Up { btn },
        "move" => Input::Move { x:x as f32,y:y as f32 },
        "delta" if s.locked.load(Ordering::Relaxed) => Input::Delta { dx:x as f32,dy:y as f32 },
        "wheel" => Input::Wheel { x:x as f32,y:y as f32,notches:value as f32 },
        "blur" => { s.locked.store(false,Ordering::Relaxed); s.events.push(Input::Up {btn:Button::Left}); Input::Leave }
        _ => return,
    };
    s.events.push(input);
}

pub fn set_pages(id:u32,count:usize) { pages(id,count as u32); }
pub fn take_page(id:u32) -> Option<usize> { WINDOWS.lock().unwrap().get(&id).and_then(|s| s.page.swap(0,Ordering::AcqRel).checked_sub(1)).map(usize::from) }

pub fn is_closed(id: u32) -> bool { WINDOWS.lock().unwrap().get(&id).is_none_or(|s| s.closed.load(Ordering::Acquire)) }

pub struct Window { state: Arc<State>, tick: Instant }
impl Window {
    pub fn idle(&self) -> bool { self.state.events.is_empty() }
    pub fn mods(&self) -> (bool,bool,bool) { let m=self.state.mods.load(Ordering::Relaxed); (m&1!=0,m&2!=0,m&4!=0) }
    pub fn next(&mut self) -> Option<Input> {
        loop {
            if self.state.closed.load(Ordering::Acquire) { return None; }
            if let Some(e)=self.state.events.pop() { return Some(e); }
            if self.tick.elapsed() >= Duration::from_millis(30) { self.tick=Instant::now(); return Some(Input::Tick); }
            wasm_thread::sleep(Duration::from_millis(3));
        }
    }
    pub fn blit(&self, pixels: &[u32], w:u32,h:u32) { *self.state.frame.lock().unwrap()=Some(Frame{pixels:pixels.to_vec(),w,h}); }
    pub fn resize(&self,_w:u32,_h:u32) { self.state.events.push(Input::Expose); }
    pub fn to_root(&self,x:i32,y:i32)->(i32,i32) { (x,y) }
    pub fn lock(&mut self) { self.state.locked.store(true,Ordering::Release); }
    pub fn unlock(&mut self) { self.state.locked.store(false,Ordering::Release); }
    pub fn locked(&self)->bool { self.state.locked.load(Ordering::Acquire) }
    pub fn popup<C:Clone>(&mut self,_x:i32,_y:i32,def:&crate::MenuDef<C>,_pal:&os_theme::Palette,_scale:f32)->Option<C> {
        fn rows<C:Clone>(def:&crate::MenuDef<C>, commands:&mut Vec<C>) -> Vec<serde_json::Value> {
            def.items.iter().enumerate().map(|(i,item)| {
                let id=commands.len(); commands.push(def.cmds[i].clone());
                serde_json::json!({"label":match item {os_ui::kit::menu::MenuItem::Item{label,..}|os_ui::kit::menu::MenuItem::Head(label)=>label.as_str(),_=>"—"},"enabled":item.clickable(),"id":id,"children":def.sub(i).map(|d|rows(d,commands))})
            }).collect()
        }
        self.unlock();
        let mut commands=Vec::new(); let items=serde_json::to_string(&rows(def,&mut commands)).ok()?;
        let (tx,rx)=mpsc::channel(); self.state.requests.push(Request::Menu(items,tx));
        let index:usize=rx.recv().ok()??.parse().ok()?; commands.get(index).cloned()
    }
    pub fn type_in(&mut self,_x:i32,_y:i32,text:&str,_scale:f32)->Option<String> {
        self.unlock(); let(tx,rx)=mpsc::channel(); self.state.requests.push(Request::Type(text.into(),tx)); rx.recv().ok().flatten()
    }
    pub fn save_sample(&self,bytes:Vec<u8>)->Result<(),String> {
        let(tx,rx)=mpsc::channel();self.state.requests.push(Request::Download(bytes,tx));rx.recv().map_err(|e|e.to_string())?
    }
    pub fn file_dialog(&self,save:bool)->Option<String> {
        let(tx,rx)=mpsc::channel(); self.state.requests.push(Request::File(save,tx)); rx.recv().ok().flatten()
    }
}
