// Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
// and not any other part of Lemon Studio.

// Windows: a GUI program, so started from Explorer, the Start menu or a shortcut it opens no console window beside its
// own. Started from a terminal it still writes there (`attach_console`).
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

fn main() { os_app::run(); }
