//! Pictures to choose from when the one found isn't right: the Steam store's for every game
//! whose name matches, and SteamGridDB's (covers, heroes, logos made by its community) when
//! the user gave a key. Thumbnails are cached; the full picture is downloaded once chosen, into
//! the user's own folder so a cache cleanup never loses it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{encode, get_json, image_bytes, strip_marks};
use crate::detect::exe::squash;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Cover,
    Hero,
    Logo,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Cover => "Jaquette",
            Kind::Hero => "Image de fond",
            Kind::Logo => "Logo",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Kind::Cover => "cover",
            Kind::Hero => "hero",
            Kind::Logo => "logo",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// A small copy on disk, to show.
    pub thumb: PathBuf,
    /// The picture itself.
    pub url: String,
    /// "Steam" or "SteamGridDB", with the game it belongs to.
    pub label: String,
}

const MAX: usize = 18;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(12)).user_agent("Arach/0.3 (game launcher)").build()
}

/// Everything found for `query`, Steam first. Slow (one download per thumbnail): run it on a
/// worker.
pub fn search(query: &str, kind: Kind, appid: Option<u32>, sgdb_key: &str) -> Vec<Candidate> {
    let agent = agent();
    let mut found: Vec<(String, String, String)> = Vec::new(); // (thumb url, full url, label)
    let mut apps: Vec<(u32, String)> = Vec::new();
    if let Some(id) = appid { apps.push((id, String::new())); }
    apps.extend(steam_matches(&agent, query).into_iter().filter(|(id, _)| Some(*id) != appid));
    for (id, name) in apps.iter().take(6) {
        let cdn = |f: &str| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/{f}");
        let label = if name.is_empty() { "Steam".to_string() } else { format!("Steam · {name}") };
        match kind {
            Kind::Cover => found.push((cdn("library_600x900.jpg"), cdn("library_600x900_2x.jpg"), label)),
            Kind::Hero => found.push((cdn("library_hero.jpg"), cdn("library_hero.jpg"), label)),
            Kind::Logo => found.push((cdn("logo.png"), cdn("logo.png"), label)),
        }
    }
    if !sgdb_key.trim().is_empty() {
        found.extend(steamgriddb(&agent, query, kind, sgdb_key.trim()));
    }
    found.truncate(MAX);
    let dir = crate::store::cache_dir().join("picker");
    let _ = std::fs::create_dir_all(&dir);
    // One thread per thumbnail: a dozen small downloads at once.
    std::thread::scope(|scope| {
        let jobs: Vec<_> = found
            .into_iter()
            .map(|(thumb, url, label)| {
                let (agent, dir) = (agent.clone(), dir.clone());
                scope.spawn(move || {
                    let path = fetch(&agent, &thumb, &dir)?;
                    Some(Candidate { thumb: path, url, label })
                })
            })
            .collect();
        jobs.into_iter().filter_map(|j| j.join().ok().flatten()).collect()
    })
}

/// Downloads the chosen picture into the user's folder; the thumbnail stands in when the full
/// one is gone.
pub fn choose(c: &Candidate, id: &str, kind: Kind) -> Option<PathBuf> {
    let dir = crate::store::UserData::covers_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let agent = agent();
    let bytes = image_bytes(&agent, &c.url).or_else(|| std::fs::read(&c.thumb).ok())?;
    let path = dir.join(format!("{}.{}", stem(id, kind), ext_of(&bytes)));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Copies a picture the user dropped or picked into their folder.
pub fn import(file: &Path, id: &str, kind: Kind) -> Option<PathBuf> {
    let bytes = std::fs::read(file).ok()?;
    image::load_from_memory(&bytes).ok()?;
    let dir = crate::store::UserData::covers_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{}.{}", stem(id, kind), ext_of(&bytes)));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// A new name each time, so the image cache never shows the previous picture.
fn stem(id: &str, kind: Kind) -> String {
    format!("{:016x}_{}_{}", crate::app::fnv(id), kind.slug(), crate::store::now())
}

fn ext_of(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") { "png" } else { "jpg" }
}

fn fetch(agent: &ureq::Agent, url: &str, dir: &Path) -> Option<PathBuf> {
    let base = dir.join(format!("{:016x}", crate::app::fnv(url)));
    for ext in ["png", "jpg"] {
        let p = base.with_extension(ext);
        if p.is_file() { return Some(p); }
    }
    let bytes = image_bytes(agent, url)?;
    // Only what Arach can draw.
    image::load_from_memory(&bytes).ok()?;
    let path = base.with_extension(ext_of(&bytes));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Steam apps whose name is close to `query`, closest first.
fn steam_matches(agent: &ureq::Agent, query: &str) -> Vec<(u32, String)> {
    let want = squash(&strip_marks(query));
    if want.len() < 2 { return Vec::new(); }
    let url = format!("https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US", encode(query));
    let Ok(json) = get_json(agent, &url) else { return Vec::new() };
    let mut items: Vec<(u32, String)> = json
        .get("items")
        .and_then(|i| i.as_array())
        .into_iter()
        .flatten()
        .filter_map(|i| Some((i.get("id")?.as_u64()? as u32, i.get("name")?.as_str()?.to_string())))
        .collect();
    items.sort_by_key(|(_, n)| squash(&strip_marks(n)) != want);
    items
}

/// SteamGridDB: the games matching the name, then each one's pictures of that kind.
fn steamgriddb(agent: &ureq::Agent, query: &str, kind: Kind, key: &str) -> Vec<(String, String, String)> {
    let api = "https://www.steamgriddb.com/api/v2";
    let get = |url: &str| -> Option<serde_json::Value> {
        agent.get(url).set("Authorization", &format!("Bearer {key}")).call().ok()?.into_json().ok()
    };
    let Some(games) = get(&format!("{api}/search/autocomplete/{}", encode(query))) else { return Vec::new() };
    let mut out = Vec::new();
    for g in games.get("data").and_then(|d| d.as_array()).into_iter().flatten().take(2) {
        let Some(id) = g.get("id").and_then(|i| i.as_u64()) else { continue };
        let name = g.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let url = match kind {
            Kind::Cover => format!("{api}/grids/game/{id}?dimensions=600x900,342x482,660x930&mimes=image/png,image/jpeg&types=static"),
            Kind::Hero => format!("{api}/heroes/game/{id}?mimes=image/png,image/jpeg&types=static"),
            Kind::Logo => format!("{api}/logos/game/{id}?mimes=image/png&types=static"),
        };
        let Some(list) = get(&url) else { continue };
        for img in list.get("data").and_then(|d| d.as_array()).into_iter().flatten().take(10) {
            let s = |k: &str| img.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (full, thumb) = (s("url"), s("thumb"));
            if full.is_empty() { continue; }
            let thumb = if thumb.is_empty() { full.clone() } else { thumb };
            out.push((thumb, full, format!("SteamGridDB · {name}")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// `cargo test picker_finds -- --ignored --nocapture` (network).
    #[test]
    #[ignore]
    fn picker_finds_steam_covers() {
        for kind in [super::Kind::Cover, super::Kind::Hero, super::Kind::Logo] {
            let found = super::search("Hollow Knight", kind, None, "");
            println!("{kind:?}: {:#?}", found.iter().map(|c| (&c.label, &c.url)).collect::<Vec<_>>());
            assert!(!found.is_empty());
        }
    }
}
