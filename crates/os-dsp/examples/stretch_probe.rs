// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

//! Deterministic native/browser parity fixture, including lifecycle and streaming checks.
use os_dsp::stretch::{self, Stream};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn stretch_probe() -> Vec<f32> {
    let mut result = Vec::new();
    for rate in [44100, 48000] {
        let n = rate as usize / 4;
        let left: Vec<f32> = (0..n)
            .map(|i| (i as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin() as f32 * 0.5)
            .collect();
        let right: Vec<f32> = left.iter().map(|v| -v * 0.5).collect();
        for (frames, transpose) in [(n * 2, 1.0), (n, 2.0), (31, 1.0), (0, 1.0)] {
            let (l, r) = stretch::whole(&left, &right, rate, frames, transpose);
            assert_eq!(l.len(), frames);
            assert_eq!(r.len(), frames);
            assert!(l.iter().chain(&r).all(|v| v.is_finite()));
            if frames > n {
                assert!(l.iter().any(|v| v.abs() > 0.01));
            }
            result.extend(l);
            result.extend(r);
        }
        let (l, r) = stretch::whole(&[], &[], rate, 16, 1.0);
        assert_eq!((l, r), (vec![0.0; 16], vec![0.0; 16]));
        let silence = vec![0.0; n];
        let (l, r) = stretch::whole(&silence, &silence, rate, n * 2, 1.0);
        assert!(l.iter().chain(&r).all(|v| v.abs() < 1e-6));
        let mut impulse = vec![0.0; n];
        impulse[0] = 1.0;
        let (l, r) = stretch::whole(&impulse, &impulse, rate, n, 1.0);
        result.extend(l);
        result.extend(r);
        let input: Vec<f32> = left
            .iter()
            .zip(&right)
            .flat_map(|(&l, &r)| [l, r])
            .collect();
        for _ in 0..3 {
            let mut a = Stream::new(rate);
            let mut b = Stream::new(rate);
            assert_eq!(a.lead(), b.lead());
            assert_eq!(a.delay(), b.delay());
            result.extend([a.lead() as f32, a.delay() as f32]);
            a.start(&input[..a.lead() * 2], 1.0);
            b.start(&input[..b.lead() * 2], 1.0);
            let mut l = vec![0.0; 512];
            let mut r = vec![0.0; 512];
            a.process(&input[..1024], &mut l, &mut r);
            let mut bl = vec![0.0; 512];
            let mut br = vec![0.0; 512];
            b.process(&input[..1024], &mut bl, &mut br);
            assert_eq!(l, bl);
            assert_eq!(r, br);
            a.start(&input[..a.lead() * 2], 1.0);
            a.process(&input[..1024], &mut bl, &mut br);
            assert_eq!(l, bl);
            assert_eq!(r, br);
            result.extend(l);
            result.extend(r);
        }
    }
    result
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let output = std::env::args()
            .nth(1)
            .expect("usage: stretch_probe OUTPUT.f32");
        let bytes: Vec<u8> = stretch_probe()
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        std::fs::write(output, bytes).unwrap();
    }
}
