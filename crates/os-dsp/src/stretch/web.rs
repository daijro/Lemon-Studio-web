// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! The same C++ processor, statically linked into the Rust WASM memory.
use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::ffi::c_void;
unsafe extern "C" {
    fn signalsmith_stretch_create_preset_default(channels: i32, rate: f32) -> *mut c_void;
    fn signalsmith_stretch_destroy(p: *mut c_void);
    fn signalsmith_stretch_reset(p: *mut c_void);
    fn signalsmith_stretch_input_latency(p: *mut c_void) -> usize;
    fn signalsmith_stretch_output_latency(p: *mut c_void) -> usize;
    fn signalsmith_stretch_set_transpose_factor(p: *mut c_void, factor: f32, limit: f32);
    fn signalsmith_stretch_seek(p: *mut c_void, input: *const f32, frames: usize, speed: f64);
    fn signalsmith_stretch_process(
        p: *mut c_void,
        input: *const f32,
        frames: usize,
        output: *mut f32,
        out_frames: usize,
    );
    fn signalsmith_stretch_exact(
        p: *mut c_void,
        input: *const f32,
        frames: usize,
        output: *mut f32,
        out_frames: usize,
    ) -> bool;
}
// C++ delete has no layout; keep the allocation size in an aligned prefix.
#[unsafe(no_mangle)]
unsafe extern "C" fn lemon_cpp_alloc(size: usize) -> *mut c_void {
    let bytes = size.checked_add(16).expect("C++ allocation overflow");
    let layout = Layout::from_size_align(bytes, 16).expect("C++ allocation too large");
    let p = unsafe { alloc(layout) };
    if p.is_null() {
        handle_alloc_error(layout);
    }
    unsafe {
        p.cast::<usize>().write(bytes);
        p.add(16).cast()
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn lemon_cpp_free(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    unsafe {
        let base = p.cast::<u8>().sub(16);
        let bytes = base.cast::<usize>().read();
        dealloc(base, Layout::from_size_align_unchecked(bytes, 16));
    }
}
#[unsafe(no_mangle)]
extern "C" fn lemon_stretch_seed() -> u32 {
    (js_sys::Math::random() * (u32::MAX as f64 + 1.0)) as u32
}

pub(super) struct Stretch {
    inner: *mut c_void,
    channels: usize,
}
// The processor has no JS handles; callers synchronize access as on desktop.
unsafe impl Send for Stretch {}
unsafe impl Sync for Stretch {}
impl Stretch {
    pub fn preset_default(channels: u32, rate: u32) -> Self {
        Self {
            inner: unsafe {
                signalsmith_stretch_create_preset_default(channels as i32, rate as f32)
            },
            channels: channels as usize,
        }
    }
    pub fn reset(&mut self) {
        unsafe { signalsmith_stretch_reset(self.inner) }
    }
    pub fn input_latency(&self) -> usize {
        unsafe { signalsmith_stretch_input_latency(self.inner) }
    }
    pub fn output_latency(&self) -> usize {
        unsafe { signalsmith_stretch_output_latency(self.inner) }
    }
    pub fn set_transpose_factor(&mut self, factor: f32, limit: Option<f32>) {
        unsafe {
            signalsmith_stretch_set_transpose_factor(self.inner, factor, limit.unwrap_or(0.0))
        }
    }
    pub fn seek(&mut self, input: &[f32], speed: f64) {
        assert_eq!(input.len() % self.channels, 0);
        unsafe {
            signalsmith_stretch_seek(
                self.inner,
                input.as_ptr(),
                input.len() / self.channels,
                speed,
            )
        }
    }
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len() % self.channels, 0);
        assert_eq!(output.len() % self.channels, 0);
        unsafe {
            signalsmith_stretch_process(
                self.inner,
                input.as_ptr(),
                input.len() / self.channels,
                output.as_mut_ptr(),
                output.len() / self.channels,
            )
        }
    }
    pub fn exact(&mut self, input: &[f32], output: &mut [f32]) -> bool {
        assert_eq!(input.len() % self.channels, 0);
        assert_eq!(output.len() % self.channels, 0);
        unsafe {
            signalsmith_stretch_exact(
                self.inner,
                input.as_ptr(),
                input.len() / self.channels,
                output.as_mut_ptr(),
                output.len() / self.channels,
            )
        }
    }
}
impl Drop for Stretch {
    fn drop(&mut self) {
        unsafe { signalsmith_stretch_destroy(self.inner) }
    }
}
