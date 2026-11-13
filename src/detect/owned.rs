//! Games bought but not installed. No store publishes the list offline in full, so each is
//! read from what its client keeps on disk:
//!
//! - Steam: the apps the library has art cached for (`appcache\librarycache\<id>`) and those
//!   with play time, named and typed by the client's `appcache\appinfo.vdf` (binary).
//! - Epic Games: the launcher's catalog cache (`Data\Catalog\catcache.bin`, base64 JSON).
//! - GOG: GOG Galaxy's database (`Galaxy\storage\galaxy-2.0.db`), when Galaxy is installed.
//!
//! Each game keeps the id it will have once installed (`steam:<id>`, `epic:<AppName>`,
//! `gog:<id>`), so favourites, collections and notes follow it across the install.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use serde::Deserialize;

use super::{ScanOptions, steam};
use crate::game::{Art, Game, Launch, Source};

pub fn detect(opts: &ScanOptions) -> Vec<Game> {
    let on = |s: Source| opts.stores.contains(&s);
    std::thread::scope(|scope| {
        let mut jobs = Vec::new();
        if on(Source::Steam) { jobs.push(scope.spawn(steam_owned)); }
        if on(Source::Epic) { jobs.push(scope.spawn(epic_owned)); }
        #[cfg(windows)]
        if on(Source::Gog) { jobs.push(scope.spawn(gog_owned)); }
        jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
    })
}

// ---------------------------------------------------------------- Steam

fn steam_owned() -> Vec<Game> {
    let Some(root) = steam::steam_root() else { return Vec::new() };
    let cache = root.join("appcache/librarycache");
    let mut ids: BTreeSet<u32> = BTreeSet::new();
    if let Ok(read) = std::fs::read_dir(&cache) {
        for e in read.flatten() {
            // `<id>\…` in current clients, `<id>_library_600x900.jpg` in older ones.
            let name = e.file_name().to_string_lossy().into_owned();
            let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(id) = digits.parse() { ids.insert(id); }
        }
    }
    let played = steam::play_stats(&root);
    for (id, (minutes, last)) in &played {
        if (*minutes > 0 || *last > 0) && let Ok(id) = id.parse() { ids.insert(id); }
    }
    let wanted: HashSet<u32> = ids.iter().copied().collect();
    let info = appinfo::read(&root.join("appcache/appinfo.vdf"), &wanted);
    let mut out = Vec::new();
    for id in ids {
        let Some(app) = info.get(&id) else { continue };
        if !app.kind.eq_ignore_ascii_case("game") || app.name.is_empty() { continue; }
        let key = id.to_string();
        if steam::NOT_GAMES.contains(&key.as_str()) { continue; }
        let (minutes, last) = played.get(&key).copied().unwrap_or_default();
        let art_dir = cache.join(&key);
        out.push(Game {
            id: format!("steam:{id}"),
            name: app.name.clone(),
            source: Source::Steam,
            launch: Launch::Uri(format!("steam://install/{id}")),
            icon: steam::icon(&cache, &key),
            store_played: last,
            steam_appid: Some(id),
            playtime_min: minutes,
            art: Art {
                cover: steam::find_art(&art_dir, "library_600x900.jpg"),
                hero: steam::find_art(&art_dir, "library_hero.jpg"),
                hero_blur: steam::find_art(&art_dir, "library_hero_blur.jpg"),
                logo: steam::find_art(&art_dir, "logo.png"),
            },
            not_installed: true,
            remote_cover: None,
            ..Default::default()
        });
    }
    out
}

/// Steam's binary app info cache: per app, a binary key-value tree. Only `common/name` and
/// `common/type` are read, and only for the apps asked about.
pub mod appinfo {
    use std::collections::{HashMap, HashSet};
    use std::path::Path;

    #[derive(Debug, Default)]
    pub struct App {
        pub name: String,
        pub kind: String,
    }

    struct Reader<'a> {
        d: &'a [u8],
        i: usize,
    }

    impl<'a> Reader<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let s = self.d.get(self.i..self.i.checked_add(n)?)?;
            self.i += n;
            Some(s)
        }
        fn u8(&mut self) -> Option<u8> {
            Some(self.take(1)?[0])
        }
        fn u32(&mut self) -> Option<u32> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
        }
        fn u64(&mut self) -> Option<u64> {
            Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
        }
        fn cstr(&mut self) -> Option<&'a str> {
            let rest = self.d.get(self.i..)?;
            let n = rest.iter().position(|&b| b == 0)?;
            self.i += n + 1;
            std::str::from_utf8(&rest[..n]).ok()
        }
    }

    pub fn read(path: &Path, wanted: &HashSet<u32>) -> HashMap<u32, App> {
        let Ok(data) = std::fs::read(path) else { return HashMap::new() };
        parse(&data, wanted).unwrap_or_default()
    }

    fn parse(data: &[u8], wanted: &HashSet<u32>) -> Option<HashMap<u32, App>> {
        let mut r = Reader { d: data, i: 0 };
        let magic = r.u32()?;
        if magic >> 8 != 0x07_5644 { return None; }
        let version = magic & 0xff;
        let _universe = r.u32()?;
        // From version 0x29 on, keys are indices into a table at the end of the file.
        let strings: Option<Vec<&str>> = if version >= 0x29 {
            let at = r.u64()? as usize;
            let mut t = Reader { d: data, i: at };
            let n = t.u32()?;
            let mut v = Vec::with_capacity(n as usize);
            for _ in 0..n { v.push(t.cstr()?); }
            Some(v)
        } else {
            None
        };
        let mut out = HashMap::new();
        loop {
            let appid = r.u32()?;
            if appid == 0 { break; }
            let size = r.u32()? as usize;
            let end = r.i.checked_add(size)?;
            if end > data.len() { break; }
            if wanted.contains(&appid) {
                // info state, last update, PICS token, SHA-1, change number, then (0x28+) the
                // SHA-1 of the binary data.
                let header = 4 + 4 + 8 + 20 + 4 + if version >= 0x28 { 20 } else { 0 };
                let mut kv = Reader { d: &data[..end], i: r.i + header };
                let mut app = App::default();
                let mut path: Vec<&str> = Vec::new();
                walk(&mut kv, strings.as_deref(), &mut path, &mut app);
                out.insert(appid, app);
            }
            r.i = end;
        }
        Some(out)
    }

    /// Reads one object's fields up to its end marker, noting `appinfo/common/{name,type}`.
    fn walk<'a>(r: &mut Reader<'a>, strings: Option<&[&'a str]>, path: &mut Vec<&'a str>, app: &mut App) -> Option<()> {
        if path.len() > 16 { return None; }
        loop {
            let kind = r.u8()?;
            if kind == 0x08 { return Some(()); }
            let key = match strings {
                Some(t) => *t.get(r.u32()? as usize)?,
                None => r.cstr()?,
            };
            match kind {
                0x00 => {
                    path.push(key);
                    walk(r, strings, path, app)?;
                    path.pop();
                }
                0x01 => {
                    let value = r.cstr()?;
                    if path.len() == 2 && path[1].eq_ignore_ascii_case("common") {
                        if key.eq_ignore_ascii_case("name") { app.name = value.trim().to_string(); }
                        if key.eq_ignore_ascii_case("type") { app.kind = value.to_string(); }
                    }
                }
                0x02 | 0x03 | 0x04 | 0x06 => { r.take(4)?; }
                0x07 | 0x0a => { r.take(8)?; }
                // A UTF-16 string: up to its two-byte terminator.
                0x05 => loop {
                    let c = r.take(2)?;
                    if c == [0, 0] { break; }
                },
                _ => return None,
            }
        }
    }
}

// ---------------------------------------------------------------- Epic Games

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogItem {
    id: String,
    namespace: String,
    title: String,
    #[serde(default)]
    categories: Vec<Category>,
    #[serde(default)]
    release_info: Vec<Release>,
    #[serde(default)]
    main_game_item: Option<MainItem>,
}

#[derive(Deserialize)]
struct Category {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    #[serde(default)]
    app_id: String,
    #[serde(default)]
    platform: Vec<String>,
}

#[derive(Deserialize)]
struct MainItem {
    #[serde(default)]
    id: String,
}

fn epic_owned() -> Vec<Game> {
    let data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let Ok(raw) = std::fs::read(data.join(r"Epic\EpicGamesLauncher\Data\Catalog\catcache.bin")) else { return Vec::new() };
    let Some(json) = base64(&raw) else { return Vec::new() };
    let Ok(items) = serde_json::from_slice::<Vec<CatalogItem>>(&json) else { return Vec::new() };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let has = |c: &str| item.categories.iter().any(|x| x.path == c);
        let dlc = item.main_game_item.as_ref().is_some_and(|m| !m.id.is_empty());
        if !has("games") || has("addons") || dlc { continue; }
        let Some(rel) = item.release_info.iter().find(|r| !r.app_id.is_empty()) else { continue };
        if !rel.platform.is_empty() && !rel.platform.iter().any(|p| p == "Windows") { continue; }
        if !seen.insert(rel.app_id.clone()) { continue; }
        out.push(Game {
            id: format!("epic:{}", rel.app_id),
            name: item.title.trim().to_string(),
            source: Source::Epic,
            launch: Launch::Uri(format!(
                "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=install",
                item.namespace, item.id, rel.app_id
            )),
            not_installed: true,
            remote_cover: None,
            ..Default::default()
        });
    }
    out
}

/// Standard base64, ignoring whitespace.
fn base64(src: &[u8]) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(src.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in src {
        if c == b'=' { break; }
        if c.is_ascii_whitespace() { continue; }
        acc = (acc << 6) | val(c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- GOG

#[cfg(windows)]
fn gog_owned() -> Vec<Game> {
    gog_query().unwrap_or_default()
}

#[cfg(windows)]
fn gog_query() -> Option<Vec<Game>> {
    use rusqlite::{Connection, OpenFlags};

    let data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let path = data.join(r"GOG.com\Galaxy\storage\galaxy-2.0.db");
    if !path.is_file() { return None; }
    let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;

    let mut owned: Vec<String> = Vec::new();
    {
        let mut q = db.prepare("SELECT DISTINCT releaseKey FROM LibraryReleases WHERE releaseKey LIKE 'gog_%'").ok()?;
        let rows = q.query_map([], |r| r.get::<_, String>(0)).ok()?;
        owned.extend(rows.flatten());
    }

    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Title { title: String }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Dlcs { dlcs: Vec<String> }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Included { products: Vec<Product> }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Product { id: String }

    let mut titles: HashMap<String, (String, String)> = HashMap::new(); // key → (title, originalTitle)
    // Releases that are a DLC of another, or a bundle of others: not games of their own.
    let mut not_games: HashSet<String> = HashSet::new();
    {
        let mut q = db.prepare(
            "SELECT p.releaseKey, t.type, p.value FROM GamePieces p JOIN GamePieceTypes t ON p.gamePieceTypeId = t.id \
             WHERE t.type IN ('title', 'originalTitle', 'dlcs', 'includedInProducts') AND p.releaseKey LIKE 'gog_%'",
        ).ok()?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).ok()?;
        for (key, kind, value) in rows.flatten() {
            match kind.as_str() {
                "title" | "originalTitle" => {
                    let t = serde_json::from_str::<Title>(&value).unwrap_or_default().title;
                    let e = titles.entry(key).or_default();
                    if kind == "title" { e.0 = t } else { e.1 = t }
                }
                "dlcs" => not_games.extend(serde_json::from_str::<Dlcs>(&value).unwrap_or_default().dlcs),
                _ => not_games.extend(
                    serde_json::from_str::<Included>(&value).unwrap_or_default().products.into_iter().map(|p| format!("gog_{}", p.id)),
                ),
            }
        }
    }

    let mut out = Vec::new();
    for key in owned {
        if not_games.contains(&key) { continue; }
        let Some(id) = key.strip_prefix("gog_") else { continue };
        let name = titles.get(&key).map(|(t, o)| if t.is_empty() { o.clone() } else { t.clone() }).unwrap_or_default();
        if name.trim().is_empty() { continue; }
        out.push(Game {
            id: format!("gog:{id}"),
            name: name.trim().to_string(),
            source: Source::Gog,
            launch: Launch::Uri(format!("goggalaxy://openGameView/{id}")),
            not_installed: true,
            remote_cover: None,
            ..Default::default()
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_decodes() {
        assert_eq!(super::base64(b"aGVsbG8gd29ybGQ=").unwrap(), b"hello world");
        assert_eq!(super::base64(b"YQ==").unwrap(), b"a");
    }

    /// `cargo test owned_on_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn owned_on_this_pc() {
        for (label, games) in [("steam", super::steam_owned()), ("epic", super::epic_owned()), ("gog", super::gog_owned())] {
            println!("{label}: {}", games.len());
            for g in games.iter().take(12) {
                println!("   {:<14} {:<40} {:?}", g.id, g.name, g.launch);
            }
        }
    }
}
