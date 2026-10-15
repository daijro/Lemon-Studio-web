// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

//! Browser files keep complete virtual paths, so identical basenames never overwrite one another.
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
static FILES: LazyLock<Mutex<HashMap<PathBuf, (u64, Arc<Vec<u8>>)>>> =
    LazyLock::new(Default::default);
static REVISION: AtomicU64 = AtomicU64::new(0);
pub const MAX_IMPORT_BYTES: usize = 256 * 1024 * 1024;
static ROOT: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| Mutex::new("/imports/current".into()));
fn missing(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} has not been imported", path.display()),
    )
}
pub fn insert(path: PathBuf, bytes: Vec<u8>) {
    let revision = REVISION.fetch_add(1, Ordering::Relaxed) + 1;
    FILES
        .lock()
        .unwrap()
        .insert(path, (revision, Arc::new(bytes)));
}
/// Keep the saved reference relative while isolating each open project's imported bytes.
pub fn import(name: &str, bytes: Vec<u8>) -> io::Result<PathBuf> {
    let relative = relative_path(name)?;
    let path = root().join(&relative);
    let mut files = FILES.lock().unwrap();
    if let Some((_, old)) = files.get(&path) {
        if **old == bytes { return Ok(relative); }
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "A different sample already uses this path; choose a distinct filename or folder"));
    }
    if files.values().map(|(_, b)| b.len()).sum::<usize>() + bytes.len() > MAX_IMPORT_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Imported samples exceed the 256 MiB browser limit"));
    }
    files.insert(path, (REVISION.fetch_add(1, Ordering::Relaxed) + 1, Arc::new(bytes)));
    Ok(relative)
}
pub fn root() -> PathBuf { ROOT.lock().unwrap().clone() }
pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let bytes = FILES
        .lock()
        .unwrap()
        .get(path.as_ref())
        .map(|(_, b)| b.clone())
        .ok_or_else(|| missing(path.as_ref()))?;
    Ok((*bytes).clone())
}
pub fn exists(path: impl AsRef<Path>) -> bool {
    FILES.lock().unwrap().contains_key(path.as_ref())
}
pub fn paths() -> Vec<PathBuf> {
    FILES.lock().unwrap().keys().cloned().collect()
}
pub fn revision(path: &Path) -> Option<u64> {
    FILES.lock().unwrap().get(path).map(|(r, _)| *r)
}
pub fn remove(path: &Path) {
    FILES.lock().unwrap().remove(path);
}
pub fn clear() {
    FILES.lock().unwrap().clear();
    let revision = REVISION.fetch_add(1, Ordering::Relaxed) + 1;
    set_root(format!("/imports/{revision}").into());
}
pub fn set_root(root: PathBuf) {
    if !root.as_os_str().is_empty() { *ROOT.lock().unwrap() = root; }
}
pub fn resolve(path: &str) -> PathBuf {
    let path = PathBuf::from(path.replace('\\', "/"));
    if exists(&path) {
        return path;
    }
    let relative = ROOT.lock().unwrap().join(&path);
    if exists(&relative) { relative } else { path }
}
/// Imported names are relative; reject traversal before prefixing their session directory.
pub fn relative_path(name: &str) -> io::Result<PathBuf> {
    let name = name.replace('\\', "/");
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(':')
        || name.chars().any(char::is_control)
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid relative file path",
        ));
    }
    Ok(name.into())
}
#[cfg(test)]
fn browser_files_keep_paths_and_invalidate_replaced_samples() {
    clear();
    set_root("/imports/project".into());
    insert("/imports/project/a/piano.wav".into(), vec![1]);
    insert("/imports/project/b/piano.wav".into(), vec![2]);
    assert_eq!(read(resolve("a/piano.wav")).unwrap(), [1]);
    assert_eq!(read(resolve("b/piano.wav")).unwrap(), [2]);
    assert!(!exists(resolve("piano.wav")), "Never guess by basename");
    let p = Path::new("/imports/project/a/piano.wav");
    let first = revision(p);
    insert(p.into(), vec![3]);
    assert_ne!(revision(p), first);
    for bad in ["../secret", "a/../b", "/absolute", "C:\\x", "a//b", ""] {
        assert!(relative_path(bad).is_err(), "{bad}");
    }
    assert_eq!(
        relative_path("a/piano.wav").unwrap(),
        Path::new("a/piano.wav")
    );
}

#[cfg(test)]
#[test]
fn imported_samples_keep_portable_names_and_reject_collisions() {
    browser_files_keep_paths_and_invalidate_replaced_samples();
    clear();
    set_root("/imports/first".into());
    let reference = import("a/piano.wav", vec![1, 2]).unwrap();
    assert_eq!(reference, Path::new("a/piano.wav"));
    assert_eq!(read(resolve(reference.to_str().unwrap())).unwrap(), [1, 2]);
    assert!(import("a/piano.wav", vec![9]).is_err());
    assert_eq!(import("b/piano.wav", vec![3]).unwrap(), Path::new("b/piano.wav"));
    clear();
    set_root("/imports/reopened".into());
    import("a/piano.wav", vec![1, 2]).unwrap();
    assert_eq!(read(resolve(reference.to_str().unwrap())).unwrap(), [1, 2]);
}
