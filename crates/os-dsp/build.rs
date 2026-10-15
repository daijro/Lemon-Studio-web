// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

use std::{env, path::PathBuf, process::Command};
fn run(cmd: &mut Command) {
    let status = cmd
        .status()
        .expect("could not start the Emscripten compiler");
    assert!(status.success(), "browser DSP compiler failed: {cmd:?}");
}
fn main() {
    println!("cargo:rerun-if-env-changed=EMSDK");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("unknown")
        || env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32")
    {
        return;
    }
    let sdk = PathBuf::from(
        env::var_os("EMSDK").expect("web builds require EMSDK pointing to Emscripten 4.0.23"),
    );
    let sysroot = sdk.join("upstream/emscripten/cache/sysroot");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=vendor/signalsmith");
    for name in ["wrapper", "runtime"] {
        run(Command::new(sdk.join("upstream/bin/clang++"))
            .args([
                "--target=wasm32-unknown-unknown",
                "-std=c++14",
                "-O3",
                "-msimd128",
                "-ffunction-sections",
                "-fdata-sections",
                "-fno-exceptions",
                "-fno-rtti",
                "-nostdinc++",
            ])
            .args(if env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default().split(',').any(|f| f == "atomics") { vec!["-matomics", "-mbulk-memory"] } else { vec![] })
            .arg(format!("--sysroot={}", sysroot.display()))
            .arg("-isystem")
            .arg(sysroot.join("include/c++/v1"))
            .args([
                "-Ivendor/signalsmith",
                "-Ivendor/signalsmith/signalsmith-stretch",
                "-c",
            ])
            .arg(format!("vendor/signalsmith/{name}.cpp"))
            .arg("-o")
            .arg(out.join(format!("{name}.o"))));
    }
    run(Command::new(sdk.join("upstream/bin/llvm-ar"))
        .arg("crs")
        .arg(out.join("liblemon_stretch.a"))
        .arg(out.join("wrapper.o"))
        .arg(out.join("runtime.o")));
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=lemon_stretch");
}
