//! Finding a game's main executable in its folder when the store doesn't say.

use std::path::{Path, PathBuf};

/// Folders that never hold the game itself.
const SKIP_DIRS: &[&str] = &[
    "__installer", "_commonredist", "commonredist", "redist", "redistributables", "directx", "vcredist",
    "dotnet", "support", "easyanticheat", "battleye", "prereq", "prerequisites", "installers", "crashreporter",
    "crashpad", "_redist", "tools", "docs", "manual", "mods", "logs", "saves", "cache", "shadercache",
];

/// Executables that are never the game.
const SKIP_EXE: &[&str] = &[
    "crash", "unins", "setup", "install", "redist", "vc_", "dxsetup", "helper", "report", "easyanticheat",
    "eac_", "battleye", "be_service", "cefprocess", "cefsharp", "webhelper", "ue4prereq", "ueprereq",
    "updater", "update", "touchup", "cleanup", "dotnet", "overlay", "server", "editor", "uploader",
    "activation", "register", "vrmonitor", "quicksfv", "dxwebsetup", "physx", "openvr", "notification",
];

const MAX_DEPTH: usize = 3;
const MAX_ENTRIES: usize = 400;

/// The likeliest game executable in `dir` for a game called `name`.
pub fn pick_exe(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = squash(name);
    let mut best: Option<(i64, PathBuf)> = None;
    let mut queue = vec![(dir.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((folder, depth)) = queue.pop() {
        let Ok(read) = std::fs::read_dir(&folder) else { continue };
        for entry in read.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES { break; }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            let file = entry.file_name().to_string_lossy().to_lowercase();
            if kind.is_dir() {
                if depth < MAX_DEPTH && !SKIP_DIRS.contains(&file.as_str()) && !file.starts_with('.') {
                    queue.push((path, depth + 1));
                }
                continue;
            }
            let Some(stem) = file.strip_suffix(".exe") else { continue };
            if SKIP_EXE.iter().any(|bad| stem.contains(bad)) { continue; }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let score = score(stem, &want, depth, size);
            if best.as_ref().is_none_or(|(b, _)| score > *b) {
                best = Some((score, path));
            }
        }
    }
    best.map(|(_, p)| p)
}

fn score(stem: &str, want: &str, depth: usize, size: u64) -> i64 {
    let have = squash(stem);
    let mut s = 0i64;
    if !want.is_empty() && !have.is_empty() {
        if have == want {
            s += 120;
        } else if want.contains(&have) || have.contains(want) {
            s += 60;
        } else {
            let common = have.chars().zip(want.chars()).take_while(|(a, b)| a == b).count();
            if common >= 3 { s += 10 + common as i64 * 3; }
        }
    }
    if stem.contains("launcher") { s -= 15; }
    if stem.contains("shipping") || stem.ends_with("win64") || stem.ends_with("_x64") { s += 8; }
    s -= depth as i64 * 12;
    // Bigger binaries are usually the game rather than a tool beside it.
    s += (64 - (size.max(1)).leading_zeros() as i64) * 2;
    s
}

/// Lowercase letters and digits only: "Hades II" → "hadesii".
pub fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_named_binary_over_tools_and_deep_shipping_builds() {
        let root = std::env::temp_dir().join(format!("arach-exe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bins = root.join("Game/Binaries/Win64");
        std::fs::create_dir_all(&bins).unwrap();
        std::fs::create_dir_all(root.join("_CommonRedist")).unwrap();
        std::fs::write(root.join("Zero Hour.exe"), vec![0u8; 4096]).unwrap();
        std::fs::write(root.join("UnityCrashHandler64.exe"), vec![0u8; 900_000]).unwrap();
        std::fs::write(root.join("_CommonRedist/vc_redist.x64.exe"), vec![0u8; 900_000]).unwrap();
        std::fs::write(bins.join("ZeroHour-Win64-Shipping.exe"), vec![0u8; 90_000]).unwrap();
        let picked = pick_exe(&root, "Zero Hour").unwrap();
        assert_eq!(picked.file_name().unwrap(), "Zero Hour.exe");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
