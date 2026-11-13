//! Folders the user points Arach at (`D:\Games`): each subfolder holding an executable is a game.

use std::path::Path;

use super::exe::pick_exe;
use crate::game::{Game, Launch, Source, created};

pub fn detect(roots: &[std::path::PathBuf]) -> Vec<Game> {
    roots.iter().flat_map(|root| folder(root)).collect()
}

fn folder(root: &Path) -> Vec<Game> {
    let Ok(read) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let dir = entry.path();
        if !entry.file_type().is_ok_and(|t| t.is_dir()) { continue; }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with('$') { continue; }
        let Some(exe) = pick_exe(&dir, &name) else { continue };
        // DeSmuME unpacked beside the games is the emulator, not a game.
        if super::emulators::is_emulator(&exe) { continue; }
        out.push(Game {
            id: format!("folder:{:016x}", crate::app::fnv(&crate::game::norm_dir(&dir))),
            name,
            source: Source::Folder,
            icon: Some(exe.clone()),
            launch: Launch::Exe { cwd: exe.parent().map(Path::to_path_buf), path: exe, args: String::new() },
            installed: created(&dir),
            dir: Some(dir),
            ..Default::default()
        });
    }
    out
}
