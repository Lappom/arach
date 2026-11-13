//! The itch.io app: every game it installed keeps a receipt (`.itch\receipt.json.gz`) with the
//! game's id, title and cover. Games live under `%APPDATA%\itch\apps` unless the user added
//! install locations, which only the app's database (`butler.db`) records: its paths are picked
//! out of the file as text, which needs no SQLite.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::exe::pick_exe;
use crate::game::{Game, Launch, Source, created, norm_dir};

pub fn detect() -> Vec<Game> {
    let Some(itch) = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("itch")) else { return Vec::new() };
    let mut roots = vec![itch.join("apps")];
    if let Ok(db) = std::fs::read(itch.join("db/butler.db")) {
        roots.extend(paths_in(&db).into_iter().filter(|p| p.is_dir()));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        if !seen.insert(norm_dir(&root)) { continue; }
        // A location holds game folders; the database also names some game folders directly.
        let mut dirs = vec![root.clone()];
        if let Ok(read) = std::fs::read_dir(&root) {
            dirs.extend(read.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()));
        }
        for dir in dirs {
            if dir != root && !seen.insert(norm_dir(&dir)) { continue; }
            if let Some(g) = game(&dir) { out.push(g); }
        }
    }
    out
}

fn game(dir: &Path) -> Option<Game> {
    let gz = std::fs::File::open(dir.join(".itch/receipt.json.gz")).ok()?;
    let mut text = String::new();
    flate2::read::GzDecoder::new(gz).read_to_string(&mut text).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let g = v.get("game")?;
    let title = g.get("title").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
    let name = if title.is_empty() { dir.file_name()?.to_string_lossy().into_owned() } else { title };
    let id = g.get("id").and_then(|i| i.as_u64()).map(|i| i.to_string()).unwrap_or_else(|| norm_dir(dir));
    let exe = pick_exe(dir, &name)?;
    let cover = ["coverUrl", "stillCoverUrl"].iter().find_map(|k| g.get(*k).and_then(|c| c.as_str())).filter(|c| c.starts_with("http"));
    Some(Game {
        id: format!("itch:{id}"),
        name,
        source: Source::Itch,
        icon: Some(exe.clone()),
        launch: Launch::Exe { cwd: exe.parent().map(Path::to_path_buf), path: exe, args: String::new() },
        installed: created(dir),
        dir: Some(dir.to_path_buf()),
        remote_cover: cover.map(String::from),
        ..Default::default()
    })
}

/// Windows paths written in a binary file: `X:\…` runs of printable text.
fn paths_in(bytes: &[u8]) -> Vec<PathBuf> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i + 2 < chars.len() {
        if chars[i].is_ascii_alphabetic() && chars[i + 1] == ':' && chars[i + 2] == '\\' {
            let end = (i..chars.len()).find(|&j| chars[j].is_control() || chars[j] == '\u{fffd}' || chars[j] == '"').unwrap_or(chars.len());
            let p: String = chars[i..end].iter().collect();
            out.push(PathBuf::from(p.trim_end()));
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_paths_in_binary() {
        let mut db = b"\x00\x13garbageD:\\Games\\itch\x00\x01\x02C:\\x".to_vec();
        db.extend_from_slice(b"\x05");
        assert_eq!(paths_in(&db), vec![PathBuf::from(r"D:\Games\itch"), PathBuf::from(r"C:\x")]);
    }
}
