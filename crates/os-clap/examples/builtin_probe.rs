// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

use std::path::Path;
use os_host::{Instrument, HostedEffect, Plugin as _};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn builtin_probe() -> String {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();
    #[cfg(target_arch = "wasm32")]
    assert!(os_clap::Library::load(Path::new("external.clap")).err().unwrap().contains("native plugin binaries"));
    let entries = [
        &lemon_sampler::LINKED_CLAP_ENTRY as *const _ as *const std::ffi::c_void,
        &lemon_3x_osc::LINKED_CLAP_ENTRY as *const _ as *const std::ffi::c_void,
        &lemon_parametric_eq_2::LINKED_CLAP_ENTRY as *const _ as *const std::ffi::c_void,
    ];
    let mut results = Vec::new();
    for (i, entry) in entries.into_iter().enumerate() {
        let lib = unsafe { os_clap::Library::from_entry(entry, Path::new("builtin")) }.unwrap();
        let mut p = os_clap::Plugin::new(&lib, &lib.plugins[0].id).unwrap();
        if i == 0 {
            let samples: Vec<f32> = (0..4800).flat_map(|n| { let x = (n as f32 * 0.05).sin() * 0.5; [x, x] }).collect();
            let mut bytes = Vec::new();
            os_engine::wav::write_f32_stereo(&mut bytes, 48000, &samples).unwrap();
            let path = "/tmp/lemon-builtin-probe.wav";
            #[cfg(not(target_arch = "wasm32"))]
            std::fs::write(path, bytes).unwrap();
            #[cfg(target_arch = "wasm32")]
            os_paths::fs::insert(path.into(), bytes);
            let json = serde_json::json!({"version":"0.0.1", "params":{}, "fields":{"sample-path":serde_json::to_string(path).unwrap()}}).to_string();
            let mut state = (json.len() as u64).to_le_bytes().to_vec();
            state.extend_from_slice(json.as_bytes());
            assert!(p.load_state(&state));
        }
        assert!(p.activate(48000));
        let param = p.param_info(0).unwrap();
        assert!(p.set_param(param.id, 0.25));
        p.idle();
        let value = p.param_value(param.id).unwrap();
        let state = p.save_state().unwrap();
        let mut q = os_clap::Plugin::new(&lib, &lib.plugins[0].id).unwrap();
        assert!(q.load_state(&state));
        assert!(q.activate(48000));
        assert_eq!(q.param_value(param.id), Some(value));
        let (mut l, mut r) = (vec![0.1; 512], vec![0.1; 512]);
        if i == 2 {
            let main = (0..q.param_count()).filter_map(|n| q.param_info(n)).find(|p| p.name == "Main level").unwrap();
            assert!(q.set_param(main.id, 0.0));
            q.idle();
            for _ in 0..100 { l.fill(0.1); r.fill(0.1); q.process_fx(48000, &mut l, &mut r); }
            assert!(l.iter().all(|v| v.abs() < 0.05), "EQ level must affect audio");
        } else {
            q.note_on(0, 60, 0.8);
            q.process(48000, &mut l, &mut r);
            assert!(l.iter().any(|v| v.abs() > 0.001), "instrument must sound");
            q.note_off(0, 60);
            for _ in 0..1000 { q.process(48000, &mut l, &mut r); }
            assert!(l.iter().all(|v| v.abs() < 0.0001), "note off must silence instrument");
        }
        assert!(l.iter().chain(&r).all(|x| x.is_finite()));
        results.push(serde_json::json!({"id": p.desc().uid(), "params": p.param_count(), "state": state}));
    }
    serde_json::to_string(&results).unwrap()
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let result = builtin_probe();
    std::fs::write(std::env::args().nth(1).unwrap(), result).unwrap();
}

#[cfg(target_arch = "wasm32")]
static WORK_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start_worker_probe() {
    let mut stretch = os_dsp::stretch::Stream::new(48000);
    wasm_thread::spawn(move || {
        let input: Vec<f32> = (0..24000).map(|n| (n as f32 * 0.02).sin()).collect();
        stretch.start(&input[..stretch.lead() * 2], 1.0);
        let (mut l, mut r) = (vec![0.0; 12000], vec![0.0; 12000]);
        stretch.process(&input, &mut l, &mut r);
        assert!(l.iter().all(|v| v.is_finite()));
        assert!(l.iter().any(|v| v.abs() > 0.01));
        drop(stretch);
        WORK_DONE.store(true, std::sync::atomic::Ordering::Release);
    });
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn worker_probe_done() -> bool { WORK_DONE.load(std::sync::atomic::Ordering::Acquire) }
