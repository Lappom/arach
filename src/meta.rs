//! What the disk doesn't say about a game: description, studio, release date, genres and box
//! art, from the public Steam store. Games sold elsewhere are matched by name; those Steam
//! doesn't sell (League of Legends, Teamfight Tactics, Valorant…) are looked up on the Epic
//! Games Store instead. Everything is cached in `%LOCALAPPDATA%\Arach`, so each game is asked
//! about once.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::detect::exe::squash;
use crate::game::{Art, Rom, Source};

pub mod picker;
mod rom_art;

/// Retry a game that found nothing after this long.
const RETRY_SECS: i64 = 14 * 24 * 3600;
/// Bumped when the lookup learns a new source: games that found nothing are asked again.
const VERSION: u32 = 3;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub appid: Option<u32>,
    pub description: String,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub release: String,
    pub genres: Vec<String>,
    pub metacritic: Option<u32>,
    /// Downloaded pictures; local ones from the store win over these.
    pub art: Art,
    pub fetched: i64,
    pub french: bool,
    /// The Epic Games Store namespace, for games found there rather than on Steam.
    pub epic: Option<String>,
    /// The `VERSION` of the lookup that wrote this.
    pub v: u32,
    /// The console game code (or name) GameTDB or libretro had pictures for.
    pub rom: Option<String>,
}

impl Meta {
    /// Whether a store recognised the game.
    pub fn found(&self) -> bool {
        self.appid.is_some() || self.epic.is_some() || self.rom.is_some()
    }
}

/// Everything learned after a scan, kept between launches.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cache {
    pub meta: HashMap<String, Meta>,
    /// Measured folder sizes for games whose store doesn't report one.
    pub sizes: HashMap<String, u64>,
}

impl Cache {
    pub fn needs_fetch(&self, id: &str, french: bool, now: i64) -> bool {
        match self.meta.get(id) {
            None => true,
            Some(m) if !m.found() && m.v < VERSION => true,
            Some(m) if m.french != french && m.found() => true,
            Some(m) if !m.found() => now - m.fetched > RETRY_SECS,
            Some(_) => false,
        }
    }
}

pub struct Job {
    pub id: String,
    pub name: String,
    pub appid: Option<u32>,
    /// What the disk already has; only the missing pictures are downloaded.
    pub local: Art,
    /// A console ROM: looked up by its game code, never by name on Steam or Epic.
    pub rom: Option<(Source, Rom)>,
    /// Box art the store links to (itch.io): used when nothing better turns up.
    pub remote_cover: Option<String>,
}

pub fn art_dir() -> PathBuf {
    crate::store::cache_dir().join("art")
}

/// Looks every job up, one after another, calling `done` with each answer as it lands.
pub fn fetch(jobs: Vec<Job>, french: bool, done: impl Fn(String, Meta) + Send + 'static) {
    if jobs.is_empty() { return; }
    std::thread::spawn(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent("Arach/0.2 (game launcher)")
            .build();
        let _ = std::fs::create_dir_all(art_dir());
        let mut dats = rom_art::Dats::default();
        for job in jobs {
            let meta = lookup(&agent, &job, french, &mut dats);
            done(job.id, meta);
        }
    });
}

fn lookup(agent: &ureq::Agent, job: &Job, french: bool, dats: &mut rom_art::Dats) -> Meta {
    let mut meta = Meta { fetched: crate::store::now(), french, v: VERSION, ..Meta::default() };
    if let Some((console, rom)) = &job.rom {
        rom_art::lookup(agent, job, *console, rom, french, &mut meta, dats);
        return meta;
    }
    // Arach decodes PNG and JPEG only: an animated GIF cover is skipped.
    if let Some(url) = &job.remote_cover
        && job.local.cover.is_none()
        && !url.to_ascii_lowercase().contains(".gif")
    {
        let ext = if url.to_ascii_lowercase().contains(".png") { "png" } else { "jpg" };
        meta.art.cover = download(agent, url, &format!("remote_{:016x}.{ext}", crate::app::fnv(url)));
    }
    if let Some(appid) = job.appid.or_else(|| search(agent, &job.name)) {
        steam(agent, job, appid, french, &mut meta);
    }
    let lacks = |m: &Meta| {
        (job.local.cover.is_none() && m.art.cover.is_none()) || (job.local.hero.is_none() && m.art.hero.is_none())
    };
    // A Steam game always has its own pictures; anything else may be missing from Steam.
    if job.appid.is_none() && lacks(&meta) {
        epic(agent, job, french, &mut meta);
    }
    meta
}

fn steam(agent: &ureq::Agent, job: &Job, appid: u32, french: bool, meta: &mut Meta) {
    meta.appid = Some(appid);
    let lang = if french { "french" } else { "english" };
    let url = format!("https://store.steampowered.com/api/appdetails?appids={appid}&l={lang}");
    let mut header = None;
    if let Ok(json) = get_json(agent, &url)
        && let Some(d) = json.get(appid.to_string()).and_then(|a| a.get("data"))
    {
        let s = |k: &str| d.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let list = |k: &str| -> Vec<String> {
            d.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).collect()).unwrap_or_default()
        };
        meta.description = clean(&s("short_description"));
        meta.developers = list("developers");
        meta.publishers = list("publishers");
        meta.release = d.pointer("/release_date/date").and_then(|v| v.as_str()).unwrap_or("").to_string();
        meta.genres = d
            .get("genres")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|g| g.get("description")?.as_str()).map(String::from).collect())
            .unwrap_or_default();
        meta.metacritic = d.pointer("/metacritic/score").and_then(|v| v.as_u64()).map(|v| v as u32);
        header = d.get("header_image").and_then(|v| v.as_str()).map(String::from);
    }
    let cdn = |file: &str| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/{file}");
    if job.local.cover.is_none() {
        meta.art.cover = download(agent, &cdn("library_600x900_2x.jpg"), &format!("{appid}_cover.jpg"))
            .or_else(|| download(agent, &cdn("library_600x900.jpg"), &format!("{appid}_cover.jpg")));
    }
    if job.local.hero.is_none() {
        meta.art.hero = download(agent, &cdn("library_hero.jpg"), &format!("{appid}_hero.jpg"))
            .or_else(|| header.and_then(|h| download(agent, &h, &format!("{appid}_header.jpg"))));
    }
    if job.local.logo.is_none() {
        meta.art.logo = download(agent, &cdn("logo.png"), &format!("{appid}_logo.png"));
    }
}

/// Epic's own store search (the query its website sends), for games Steam doesn't sell. Only
/// fills what Steam left empty.
fn epic(agent: &ureq::Agent, job: &Job, french: bool, meta: &mut Meta) {
    let want = squash(&strip_marks(&job.name));
    if want.len() < 2 { return; }
    let (country, locale) = if french { ("FR", "fr") } else { ("US", "en-US") };
    let vars = serde_json::json!({
        "keywords": job.name, "country": country, "locale": locale, "count": 10, "category": "games/edition/base",
    });
    let ext = serde_json::json!({ "persistedQuery": { "version": 1, "sha256Hash": EPIC_SEARCH_HASH } });
    let url = format!(
        "https://store.epicgames.com/graphql?operationName=searchStoreQuery&variables={}&extensions={}",
        encode(&vars.to_string()),
        encode(&ext.to_string()),
    );
    let Ok(json) = get_json(agent, &url) else { return };
    let Some(items) = json.pointer("/data/Catalog/searchStore/elements").and_then(|v| v.as_array()) else { return };
    let Some(item) = items.iter().find(|i| i.get("title").and_then(|t| t.as_str()).is_some_and(|t| same_name(&want, t)))
    else {
        return;
    };
    let s = |k: &str| item.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let ns = s("namespace");
    if ns.is_empty() { return; }
    if meta.description.is_empty() { meta.description = clean(&s("description")); }
    let dev = s("developerDisplayName");
    if meta.developers.is_empty() && !dev.is_empty() { meta.developers = vec![dev]; }
    let publisher = s("publisherDisplayName");
    if meta.publishers.is_empty() && !publisher.is_empty() { meta.publishers = vec![publisher]; }
    if meta.release.is_empty() { meta.release = date(&s("releaseDate"), french); }
    let image = |kinds: &[&str]| -> Option<String> {
        let list = item.get("keyImages")?.as_array()?;
        kinds.iter().find_map(|kind| {
            list.iter()
                .find(|i| i.get("type").and_then(|t| t.as_str()) == Some(*kind))
                .and_then(|i| i.get("url")?.as_str())
                .filter(|u| u.starts_with("https://"))
                .map(String::from)
        })
    };
    // Epic's CDN scales on request; the originals run to 2560 px.
    if job.local.cover.is_none() && meta.art.cover.is_none() {
        meta.art.cover = image(&["OfferImageTall", "DieselStoreFrontTall", "Thumbnail"])
            .and_then(|u| download(agent, &format!("{u}?w=600&h=800&resize=1"), &format!("epic_{ns}_cover.jpg")));
    }
    if job.local.hero.is_none() && meta.art.hero.is_none() {
        meta.art.hero = image(&["OfferImageWide", "DieselStoreFrontWide"])
            .and_then(|u| download(agent, &format!("{u}?w=1920&h=1080&resize=1"), &format!("epic_{ns}_hero.jpg")));
    }
    meta.epic = Some(ns);
}

/// The persisted query behind Epic's store search.
const EPIC_SEARCH_HASH: &str = "7d58e12d9dd8cb14c84a3ff18d360bf9f0caa96bf218f2c5fda68ba88d68a437";

/// `2021-11-04T16:30:00.000Z` → `4 nov. 2021` / `Nov 4, 2021`, as Steam writes dates.
fn date(iso: &str, french: bool) -> String {
    let mut parts = iso.get(..10).unwrap_or("").split('-').map(|p| p.parse::<u32>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next()) else {
        return String::new();
    };
    if !(1..=12).contains(&m) || !(1970..=2090).contains(&y) { return String::new(); }
    const FR: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let i = m as usize - 1;
    if french { format!("{d} {} {y}", FR[i]) } else { format!("{} {d}, {y}", EN[i]) }
}

/// Same game, give or take an edition suffix: a wrong cover is worse than none.
fn same_name(want: &str, title: &str) -> bool {
    let have = squash(&strip_marks(title));
    have == want
        || ((have.starts_with(want) || want.starts_with(&have))
            && have.len().min(want.len()) * 4 >= have.len().max(want.len()) * 3)
}

/// The store's app for a game sold elsewhere, only when the names really match: a wrong
/// cover is worse than none.
fn search(agent: &ureq::Agent, name: &str) -> Option<u32> {
    let want = squash(&strip_marks(name));
    if want.len() < 2 { return None; }
    let url = format!("https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US", encode(name));
    let json = get_json(agent, &url).ok()?;
    json.get("items")?.as_array()?.iter().find_map(|item| {
        same_name(&want, item.get("name")?.as_str()?).then(|| item.get("id")?.as_u64().map(|v| v as u32)).flatten()
    })
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, ()> {
    agent.get(url).call().map_err(|_| ())?.into_json().map_err(|_| ())
}

/// Saves an image under the art folder; a file already there is reused.
fn download(agent: &ureq::Agent, url: &str, file: &str) -> Option<PathBuf> {
    let path = art_dir().join(file);
    if path.is_file() { return Some(path); }
    let bytes = image_bytes(agent, url)?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes).ok()?;
    std::fs::rename(&tmp, &path).ok()?;
    Some(path)
}

/// An image's bytes, or `None` for a 404 page or a stub.
fn image_bytes(agent: &ureq::Agent, url: &str) -> Option<Vec<u8>> {
    let resp = agent.get(url).call().ok()?;
    if !resp.content_type().starts_with("image/") { return None; }
    let mut bytes = Vec::new();
    resp.into_reader().take(12 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    (bytes.len() >= 512).then_some(bytes)
}

fn strip_marks(s: &str) -> String {
    s.replace(['™', '®', '©'], "")
}

/// Steam descriptions carry HTML entities.
fn clean(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&amp;", "&").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">")
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Bytes under `dir`, for games whose store doesn't say. Runs on a worker.
pub fn measure(jobs: Vec<(String, PathBuf)>, done: impl Fn(String, u64) + Send + 'static) {
    if jobs.is_empty() { return; }
    std::thread::spawn(move || {
        for (id, dir) in jobs {
            done(id, dir_size(&dir));
        }
    });
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&d) else { continue };
        for e in read.flatten() {
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                stack.push(e.path());
            } else if t.is_file() {
                total += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_queries_and_cleans_entities() {
        assert_eq!(encode("Hades II: Zagreus"), "Hades%20II%3A%20Zagreus");
        assert_eq!(clean("Rock &amp; &quot;Roll&quot;"), "Rock & \"Roll\"");
    }

    #[test]
    fn formats_epic_dates() {
        assert_eq!(date("2021-11-04T16:30:00.000Z", true), "4 nov. 2021");
        assert_eq!(date("2021-11-04T16:30:00.000Z", false), "Nov 4, 2021");
        assert_eq!(date("2099-01-01T00:00:00.000Z", false), "");
        assert_eq!(date("", true), "");
    }

    #[test]
    fn retries_games_that_found_nothing_under_an_older_lookup() {
        let mut cache = Cache::default();
        cache.meta.insert("riot:lol".into(), Meta { fetched: 100, french: true, ..Meta::default() });
        assert!(cache.needs_fetch("riot:lol", true, 200));
        cache.meta.insert("riot:lol".into(), Meta { fetched: 100, french: true, v: VERSION, ..Meta::default() });
        assert!(!cache.needs_fetch("riot:lol", true, 200));
        let epic = Meta { fetched: 100, french: true, v: VERSION, epic: Some("ns".into()), ..Meta::default() };
        cache.meta.insert("riot:lol".into(), epic);
        assert!(cache.needs_fetch("riot:lol", false, 200));
    }

    /// `cargo test lookup_league -- --ignored --nocapture` (network).
    #[test]
    #[ignore]
    fn lookup_league() {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
        std::fs::create_dir_all(art_dir()).unwrap();
        for name in ["League of Legends", "Teamfight Tactics"] {
            let job = Job { id: "riot:x".into(), name: name.into(), appid: None, local: Art::default(), rom: None, remote_cover: None };
            let m = lookup(&agent, &job, true, &mut rom_art::Dats::default());
            println!("{m:#?}");
            assert!(m.epic.is_some() && m.art.cover.is_some() && m.art.hero.is_some(), "{name}");
        }
    }

    /// `cargo test lookup_cyberpunk -- --ignored --nocapture` (network).
    #[test]
    #[ignore]
    fn lookup_cyberpunk() {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
        let job = Job { id: "gog:1".into(), name: "Cyberpunk 2077".into(), appid: None, local: Art::default(), rom: None, remote_cover: None };
        let m = lookup(&agent, &job, true, &mut rom_art::Dats::default());
        println!("{m:#?}");
        assert_eq!(m.appid, Some(1091500));
    }
}
