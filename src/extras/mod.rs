//! What a game's page shows beyond the library: its saves (and copies of them), its
//! screenshots, its mods folder, its Steam achievements, how long it takes to beat, and how to
//! uninstall it. All of it is gathered off the UI thread when the page opens.

pub mod achievements;
pub mod hltb;
mod kv;
pub mod saves;
pub mod screenshots;
pub mod uninstall;

use std::path::{Path, PathBuf};

use crate::game::Game;

/// Something looked up in the background: not back yet, found, or why not.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Lookup<T> {
    #[default]
    Pending,
    Found(T),
    /// Nothing to show; the text says why (empty: nothing worth saying).
    Missing(String),
}

/// An action on the page waiting for a second press.
#[derive(Clone, Debug, PartialEq)]
pub enum Arm {
    Restore(PathBuf),
    DeleteBackup(PathBuf),
    Uninstall,
}

#[derive(Clone, Debug, Default)]
pub struct Extras {
    pub saves: Lookup<saves::Located>,
    pub backups: Vec<saves::Backup>,
    /// A copy or restore running.
    pub busy: bool,
    pub shots: Vec<screenshots::Shot>,
    /// How many screenshots the page shows; "Voir plus" raises it.
    pub shots_shown: usize,
    pub mods: Option<PathBuf>,
    pub achievements: Lookup<achievements::Achievements>,
    pub hltb: Lookup<hltb::Hltb>,
    pub uninstall: Option<uninstall::Uninstaller>,
    pub armed: Option<Arm>,
}

/// What's on disk: quick, read again each time the page opens.
pub struct Local {
    pub backups: Vec<saves::Backup>,
    pub shots: Vec<screenshots::Shot>,
    pub mods: Option<PathBuf>,
    pub achievements: Option<achievements::Achievements>,
    pub uninstall: Option<uninstall::Uninstaller>,
}

pub fn local(g: &Game, french: bool) -> Local {
    let steam = steam_root();
    let steam_game = g.steam_appid.filter(|_| g.id.starts_with("steam:"));
    Local {
        backups: saves::list(&g.id),
        shots: screenshots::find(g, steam.as_deref()),
        mods: g.dir.as_deref().and_then(mods_dir),
        achievements: steam.as_deref().zip(steam_game).and_then(|(s, id)| achievements::local(s, id, french)),
        uninstall: uninstall::find(g),
    }
}

/// The folder mods go in, when the game has one: `Mods`, BepInEx's plugins, Unreal's `~mods`…
pub fn mods_dir(dir: &Path) -> Option<PathBuf> {
    const CANDIDATES: [&str; 6] = ["Mods", "mods", "BepInEx/plugins", "Content/Paks/~mods", "Data/Mods", "Game/Mods"];
    if !dir.is_dir() { return None; }
    CANDIDATES.iter().map(|c| dir.join(c)).find(|p| p.is_dir())
}

/// Nexus Mods' search for the game: its own address isn't guessable from the name.
pub fn nexus_url(name: &str) -> String {
    let q: String = name.replace(['™', '®', '©'], "").trim().bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
        b' ' => "+".into(),
        _ => format!("%{b:02X}"),
    }).collect();
    format!("https://www.nexusmods.com/search?keyword={q}")
}

pub fn steam_root() -> Option<PathBuf> {
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

/// `340 Ko`, `12 Mo`, `1,4 Go`: saves are often tiny, so kilobytes count here.
pub fn size_text(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        0 => "vide".into(),
        n if n < 1 << 20 => format!("{:.0} Ko", (b / 1024.0).max(1.0)),
        n if n < 1 << 30 => format!("{:.0} Mo", b / 1048576.0),
        _ => format!("{:.1} Go", b / 1073741824.0).replace('.', ","),
    }
}

/// `13 nov. 2026 à 21:40`, local time.
pub fn date_text(ts: i64) -> String {
    const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    let t = ts + crate::store::utc_offset();
    let (y, m, d) = saves::civil(t.div_euclid(86_400));
    format!("{d} {} {y} à {:02}:{:02}", MONTHS[(m - 1) as usize], t.rem_euclid(86_400) / 3600, t.rem_euclid(3600) / 60)
}
