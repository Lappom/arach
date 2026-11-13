//! A game's screenshots, wherever they were taken: Steam (`userdata\<user>\760\remote\<appid>\
//! screenshots`), the Xbox Game Bar (`Videos\Captures`, files named after the window) and NVIDIA
//! (`Videos\<Game>` or `Videos\NVIDIA\<Game>`).

use std::path::{Path, PathBuf};

use crate::game::Game;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Steam,
    GameBar,
    Nvidia,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Steam => "Steam",
            Origin::GameBar => "Xbox Game Bar",
            Origin::Nvidia => "NVIDIA",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub path: PathBuf,
    pub origin: Origin,
    /// Unix seconds.
    pub taken: i64,
}

const MAX: usize = 120;

/// Newest first.
pub fn find(g: &Game, steam: Option<&Path>) -> Vec<Shot> {
    let mut out = Vec::new();
    if let (Some(steam), Some(appid)) = (steam, g.steam_appid) {
        let Ok(users) = std::fs::read_dir(steam.join("userdata")) else { return finish(out, g) };
        for user in users.flatten() {
            let dir = user.path().join(format!("760/remote/{appid}/screenshots"));
            images_in(&dir, Origin::Steam, &mut out, |_| true);
        }
    }
    finish(out, g)
}

fn finish(mut out: Vec<Shot>, g: &Game) -> Vec<Shot> {
    let want = squash(&g.name);
    if want.len() >= 2 {
        for videos in videos_dirs() {
            // Game Bar: "Hades II 13_11_2026 21_40_02.png".
            images_in(&videos.join("Captures"), Origin::GameBar, &mut out, |stem| game_bar_match(&g.name, stem));
            // NVIDIA: a folder per game, named after its window.
            for parent in [videos.clone(), videos.join("NVIDIA")] {
                let Ok(read) = std::fs::read_dir(&parent) else { continue };
                for e in read.flatten() {
                    if e.path().is_dir() && squash(&e.file_name().to_string_lossy()) == want {
                        images_in(&e.path(), Origin::Nvidia, &mut out, |_| true);
                    }
                }
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.taken));
    out.dedup_by(|a, b| a.path == b.path);
    out.truncate(MAX);
    out
}

fn images_in(dir: &Path, origin: Origin, out: &mut Vec<Shot>, keep: impl Fn(&str) -> bool) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for e in read.flatten() {
        let path = e.path();
        if !crate::game::is_image(&path) { continue; }
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if !keep(&stem) { continue; }
        let taken = e.metadata().and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64);
        out.push(Shot { path, origin, taken });
    }
}

/// The Videos folder, where the user moved it, and the default one.
fn videos_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
            && let Ok(v) = k.get_value::<String, _>("My Video")
        {
            out.push(PathBuf::from(v));
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let d = PathBuf::from(home).join("Videos");
        if !out.iter().any(|o| crate::game::norm_dir(o) == crate::game::norm_dir(&d)) { out.push(d); }
    }
    out
}

/// The capture's name is the window title, then the date: "Hades" must not take "Hades II"'s.
fn game_bar_match(name: &str, stem: &str) -> bool {
    let (want, have) = (crate::search::normalize(name), crate::search::normalize(stem));
    !want.is_empty() && have.strip_prefix(&want).is_some_and(|rest| rest.strip_prefix(' ').is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit())))
}

/// Letters and digits only, lowercase: window titles and folder names drop `:`, `™` and the like.
pub fn squash(s: &str) -> String {
    crate::search::normalize(s).replace(' ', "")
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_compare_without_symbols() {
        assert_eq!(super::squash("Hades II™"), super::squash("hades ii"));
        assert!(super::game_bar_match("Hades II", "Hades II 13_11_2026 21_40_02"));
        assert!(!super::game_bar_match("Hades", "Hades II 13_11_2026 21_40_02"));
    }
}
