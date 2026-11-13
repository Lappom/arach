//! What survives a restart. `%APPDATA%\Arach` holds the user's own choices (settings, play
//! times, favourites, hidden games, games added by hand) and the last scan, so the library shows
//! on the first frame; `%LOCALAPPDATA%\Arach` holds what can be fetched again (art, store data).
//! Reads happen once before the window opens; every write goes through one writer thread.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};

use serde::{Deserialize, Serialize};

use crate::game::{Art, Game};
use crate::meta::Cache;
use crate::settings::{Actions, Settings};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserData {
    /// Game id → last launch from Arach, Unix seconds.
    pub played: HashMap<String, i64>,
    /// Game id → launches from Arach.
    pub launches: HashMap<String, u32>,
    pub hidden: HashSet<String>,
    pub favorites: HashSet<String>,
    /// Game id → extra command-line arguments.
    pub args: HashMap<String, String>,
    pub custom: Vec<Game>,
    /// Last scan: when, and how long it took (ms).
    pub last_scan: i64,
    pub last_scan_ms: u64,
    /// Game id → each session Arach saw it running, oldest first.
    pub sessions: HashMap<String, Vec<Session>>,
    /// Lists the user made ("À finir", "Coop"…), in the order they were made.
    pub collections: Vec<Collection>,
    /// Game id → where the user stands with it: status, rating, notes.
    pub entries: HashMap<String, Entry>,
    /// Game id → what the user corrected by hand (name, studio, date, genres…).
    pub edits: HashMap<String, Edit>,
    /// Game id → pictures the user chose, over the store's and the downloaded ones.
    pub art: HashMap<String, Art>,
    /// Game id → the game it was merged into: one entry in the library, two versions inside.
    pub merged: HashMap<String, String>,
    /// Games kept apart although they share a folder (the automatic merge got it wrong).
    pub split: HashSet<String>,
    /// Game id → its own actions before and after playing, over the general ones.
    pub actions: HashMap<String, Actions>,
}

/// Fields the user rewrote; `None` keeps what the store or the lookup says.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edit {
    pub name: Option<String>,
    pub description: Option<String>,
    pub developers: Option<Vec<String>>,
    pub publishers: Option<Vec<String>>,
    pub release: Option<String>,
    pub genres: Option<Vec<String>>,
}

impl Edit {
    pub fn is_empty(&self) -> bool {
        *self == Edit::default()
    }
}

/// One stretch of play, measured by watching the game's processes. Unix seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub start: i64,
    pub end: i64,
}

impl Session {
    pub fn secs(&self) -> i64 {
        (self.end - self.start).max(0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Collection {
    pub id: u64,
    pub name: String,
    /// Game ids, in the order they were added.
    pub games: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    #[default]
    NotStarted,
    Playing,
    Finished,
    Abandoned,
    Completed,
}

impl Status {
    pub const ALL: [Status; 5] = [Status::NotStarted, Status::Playing, Status::Finished, Status::Completed, Status::Abandoned];

    pub fn label(self) -> &'static str {
        match self {
            Status::NotStarted => "Non commencé",
            Status::Playing => "En cours",
            Status::Finished => "Terminé",
            Status::Abandoned => "Abandonné",
            Status::Completed => "100 %",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub status: Status,
    /// 1 to 5 stars; 0 when not rated.
    pub rating: u8,
    pub notes: String,
}

impl Entry {
    pub fn is_empty(&self) -> bool {
        *self == Entry::default()
    }
}

impl UserData {
    /// The game `id` stands under in the library: itself, or the one it was merged into
    /// (following a chain, never looping).
    pub fn primary<'a>(&'a self, id: &'a str) -> &'a str {
        let mut at = id;
        for _ in 0..16 {
            match self.merged.get(at) {
                Some(next) if next != id => at = next,
                _ => break,
            }
        }
        at
    }

    /// Where the user's own pictures go: `%APPDATA%\Arach\covers`.
    pub fn covers_dir() -> PathBuf {
        data_dir().join("covers")
    }

    /// Seconds Arach saw the game running, over every session.
    pub fn tracked_secs(&self, id: &str) -> i64 {
        self.sessions.get(id).map_or(0, |v| v.iter().map(Session::secs).sum())
    }

    pub fn entry(&self, id: &str) -> Entry {
        self.entries.get(id).cloned().unwrap_or_default()
    }

    /// Changes a game's entry, dropping it once it's back to nothing.
    pub fn edit_entry(&mut self, id: &str, f: impl FnOnce(&mut Entry)) {
        let mut e = self.entry(id);
        f(&mut e);
        if e.is_empty() { self.entries.remove(id); } else { self.entries.insert(id.to_string(), e); }
    }
}

/// `ARACH_DATA_DIR` moves it, so headless runs and tests don't touch the real library.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ARACH_DATA_DIR") {
        return PathBuf::from(dir);
    }
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Arach")
}

pub fn cache_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Arach")
}

pub fn load_library() -> Vec<Game> {
    read(&data_dir().join("library.json")).unwrap_or_default()
}

pub fn load_user() -> UserData {
    read(&data_dir().join("user.json")).unwrap_or_default()
}

pub fn load_settings() -> Settings {
    read::<Settings>(&data_dir().join("settings.json")).unwrap_or_default().migrate()
}

pub fn load_cache() -> Cache {
    read(&cache_dir().join("store.json")).unwrap_or_default()
}

pub fn save_library(games: Vec<Game>) {
    write_later(data_dir().join("library.json"), move || serde_json::to_vec(&games));
}

pub fn save_user(user: UserData) {
    write_later(data_dir().join("user.json"), move || serde_json::to_vec_pretty(&user));
}

pub fn save_settings(s: Settings) {
    write_later(data_dir().join("settings.json"), move || serde_json::to_vec_pretty(&s));
}

pub fn save_cache(c: Cache) {
    write_later(cache_dir().join("store.json"), move || serde_json::to_vec(&c));
}

/// Deletes downloaded art and store data.
pub fn clear_cache() {
    let dir = cache_dir();
    write_later_fn(Box::new(move || {
        let _ = std::fs::remove_dir_all(dir.join("art"));
        let _ = std::fs::remove_file(dir.join("store.json"));
    }));
}

/// Bytes in the art folder (for Paramètres → En ligne).
pub fn cache_bytes() -> u64 {
    std::fs::read_dir(cache_dir().join("art"))
        .map(|r| r.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

type Work = Box<dyn FnOnce() + Send>;

/// One writer thread, so two saves of the same file never race.
fn write_later_fn(work: Work) {
    static WRITER: OnceLock<Sender<Work>> = OnceLock::new();
    let tx = WRITER.get_or_init(|| {
        let (tx, rx) = channel::<Work>();
        std::thread::spawn(move || {
            for work in rx { work(); }
        });
        tx
    });
    let _ = tx.send(work);
}

/// Serializes on the writer, then writes a sibling and renames it over, so a crash mid-write
/// never leaves a torn file.
fn write_later(path: PathBuf, bytes: impl FnOnce() -> serde_json::Result<Vec<u8>> + Send + 'static) {
    write_later_fn(Box::new(move || {
        let Ok(bytes) = bytes() else { return };
        if let Some(dir) = path.parent() && std::fs::create_dir_all(dir).is_err() { return; }
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }));
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Seconds to add to a Unix time to read it on the local clock (3600 in Paris in winter).
pub fn utc_offset() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        // SAFETY: the struct is plain data the call fills in.
        unsafe {
            let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
            let mode = GetTimeZoneInformation(&mut tz);
            // UTC = local + bias, in minutes; daylight saving adds its own bias while it runs.
            let extra = if mode == 2 { tz.DaylightBias } else { tz.StandardBias };
            -((tz.Bias + extra) as i64) * 60
        }
    }
    #[cfg(not(windows))]
    {
        0
    }
}
