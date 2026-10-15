// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

//! Files explicitly imported into this browser session, keyed by their complete virtual path.
use std::{collections::HashMap, io, path::{Path, PathBuf}, sync::{Arc, Mutex, LazyLock, atomic::{AtomicU64, Ordering}}};
static FILES: LazyLock<Mutex<HashMap<PathBuf, (u64, Arc<Vec<u8>>)>>> = LazyLock::new(Default::default);

static REVISION: AtomicU64 = AtomicU64::new(0);
pub fn insert(path: PathBuf, bytes: Vec<u8>) {
    let revision=REVISION.fetch_add(1, Ordering::Relaxed)+1;
    FILES.lock().unwrap().insert(path, (revision, Arc::new(bytes)));
}
pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let bytes = FILES.lock().unwrap().get(path.as_ref()).map(|(_,bytes)|bytes.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{} has not been imported", path.as_ref().display())))?;
    Ok((*bytes).clone())
}
pub fn exists(path: impl AsRef<Path>) -> bool { FILES.lock().unwrap().contains_key(path.as_ref()) }
pub fn paths() -> Vec<PathBuf> { FILES.lock().unwrap().keys().cloned().collect() }
pub fn clear() { FILES.lock().unwrap().clear(); }

pub fn revision(path: &Path) -> Option<u64> { FILES.lock().unwrap().get(path).map(|(revision,_)|*revision) }
