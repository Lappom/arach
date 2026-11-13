//! Steam: every library folder from `libraryfolders.vdf` (one per drive), then each
//! `appmanifest_<id>.acf` in it. Play time comes from the signed-in users' `localconfig.vdf`,
//! artwork from the client's own library cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::vdf;
use crate::game::{Art, Game, Launch, Source, created, norm_dir};

/// Runtimes and tools Steam installs alongside games.
pub(super) const NOT_GAMES: &[&str] = &["228980", "1070560", "1391110", "1628350", "1493710", "2180100", "1826330", "2805730"];

pub fn detect() -> Vec<Game> {
    let Some(root) = steam_root() else { return Vec::new() };
    let mut libraries = vec![root.clone()];
    if let Ok(src) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        let doc = vdf::parse(&src);
        if let Some(folders) = doc.get("libraryfolders") {
            for (_, lib) in folders.entries() {
                if let Some(p) = lib.str("path") {
                    let p = PathBuf::from(p);
                    if !libraries.iter().any(|l| norm_dir(l) == norm_dir(&p)) {
                        libraries.push(p);
                    }
                }
            }
        }
    }
    let cache = root.join("appcache/librarycache");
    let played = play_stats(&root);
    libraries.iter().flat_map(|lib| library(lib, &cache, &played)).collect()
}

pub(super) fn steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam")
            && let Ok(p) = key.get_value::<String, _>("SteamPath")
        {
            let p = PathBuf::from(p);
            if p.is_dir() { return Some(p); }
        }
    }
    let fallback = PathBuf::from(r"C:\Program Files (x86)\Steam");
    fallback.is_dir().then_some(fallback)
}

/// App id → (minutes played, last played), summed and maxed over every account on the PC.
pub(super) fn play_stats(root: &Path) -> HashMap<String, (u64, i64)> {
    let mut out: HashMap<String, (u64, i64)> = HashMap::new();
    let Ok(users) = std::fs::read_dir(root.join("userdata")) else { return out };
    for user in users.flatten() {
        let Ok(src) = std::fs::read_to_string(user.path().join("config/localconfig.vdf")) else { continue };
        let doc = vdf::parse(&src);
        let apps = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]
            .iter()
            .try_fold(&doc, |node, key| node.get(key));
        let Some(apps) = apps else { continue };
        for (id, app) in apps.entries() {
            let minutes: u64 = app.str("Playtime").and_then(|v| v.parse().ok()).unwrap_or(0);
            let last: i64 = app.str("LastPlayed").and_then(|v| v.parse().ok()).unwrap_or(0);
            let e = out.entry(id.clone()).or_default();
            e.0 += minutes;
            e.1 = e.1.max(last);
        }
    }
    out
}

fn library(lib: &Path, cache: &Path, played: &HashMap<String, (u64, i64)>) -> Vec<Game> {
    let apps = lib.join("steamapps");
    let Ok(read) = std::fs::read_dir(&apps) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) { continue; }
        let Ok(src) = std::fs::read_to_string(entry.path()) else { continue };
        if let Some(g) = manifest(&vdf::parse(&src), &apps, cache, played) { out.push(g); }
    }
    out
}

fn manifest(doc: &vdf::Vdf, apps: &Path, cache: &Path, played: &HashMap<String, (u64, i64)>) -> Option<Game> {
    let app = doc.get("AppState")?;
    let id = app.str("appid")?;
    let name = app.str("name")?.trim();
    let flags: u32 = app.str("StateFlags").and_then(|f| f.parse().ok()).unwrap_or(0);
    if flags & 4 == 0 || NOT_GAMES.contains(&id) { return None; }
    if name.starts_with("Proton") || name.starts_with("Steam Linux Runtime") || name.starts_with("Steamworks") {
        return None;
    }
    let dir = apps.join("common").join(app.str("installdir")?);
    if !dir.is_dir() { return None; }
    let (minutes, last) = played.get(id).copied().unwrap_or_default();
    let manifest_last: i64 = app.str("LastPlayed").and_then(|t| t.parse().ok()).unwrap_or(0);
    let art_dir = cache.join(id);
    Some(Game {
        id: format!("steam:{id}"),
        name: name.to_string(),
        source: Source::Steam,
        icon: icon(cache, id).or_else(|| super::exe::pick_exe(&dir, name)),
        installed: created(&dir),
        launch: Launch::Uri(format!("steam://rungameid/{id}")),
        store_played: manifest_last.max(last),
        size: app.str("SizeOnDisk").and_then(|s| s.parse().ok()).unwrap_or(0),
        steam_appid: id.parse().ok(),
        playtime_min: minutes,
        art: Art {
            cover: find_art(&art_dir, "library_600x900.jpg"),
            hero: find_art(&art_dir, "library_hero.jpg"),
            hero_blur: find_art(&art_dir, "library_hero_blur.jpg"),
            logo: find_art(&art_dir, "logo.png"),
        },
        dir: Some(dir),
        rom: None,
        not_installed: false,
        remote_cover: None,
        update_pending: update_pending(app, flags),
    })
}

/// `StateFlags` bits 2 (update required), 256 (running), 512 (paused) and 1024 (started), or a
/// target build that isn't the installed one.
fn update_pending(app: &vdf::Vdf, flags: u32) -> bool {
    let build = |k: &str| app.str(k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let (have, want) = (build("buildid"), build("TargetBuildID"));
    flags & (2 | 256 | 512 | 1024) != 0 || (have != 0 && want != 0 && have != want)
}

/// Current clients keep each picture either beside the app's folder or one level down in a
/// hashed folder.
pub(super) fn find_art(dir: &Path, file: &str) -> Option<PathBuf> {
    let direct = dir.join(file);
    if direct.is_file() { return Some(direct); }
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path().join(file)).find(|p| p.is_file())
}

/// The small library icon: `<id>/<sha1>.jpg` in current clients, `<id>_icon.jpg` in older ones.
pub(super) fn icon(cache: &Path, id: &str) -> Option<PathBuf> {
    if let Ok(read) = std::fs::read_dir(cache.join(id)) {
        for entry in read.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(stem) = name.strip_suffix(".jpg")
                && stem.len() == 40
                && stem.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Some(entry.path());
            }
        }
    }
    let old = cache.join(format!("{id}_icon.jpg"));
    old.is_file().then_some(old)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_a_pending_update() {
        let app = |flags: &str, build: &str, target: &str| {
            vdf::parse(&format!(r#""AppState" {{ "StateFlags" "{flags}" "buildid" "{build}" "TargetBuildID" "{target}" }}"#))
        };
        let pending = |doc: &vdf::Vdf| {
            let a = doc.get("AppState").unwrap();
            update_pending(a, a.str("StateFlags").unwrap().parse().unwrap())
        };
        assert!(!pending(&app("4", "100", "100")), "fully installed, up to date");
        assert!(pending(&app("6", "100", "100")), "update required");
        assert!(pending(&app("1028", "100", "100")), "update started");
        assert!(pending(&app("4", "100", "120")), "a newer build is targeted");
        assert!(!pending(&app("4", "100", "0")));
    }
}
