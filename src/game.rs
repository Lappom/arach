//! What a game is, independent of where it was found.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Source {
    #[default]
    Steam,
    Epic,
    Gog,
    Ubisoft,
    BattleNet,
    Riot,
    Ea,
    /// Microsoft Store and PC Game Pass, read from the installed app packages.
    Xbox,
    /// Amazon Games, from its uninstall entries.
    Amazon,
    /// Rockstar Games Launcher.
    Rockstar,
    /// The itch.io app's install folders.
    Itch,
    /// Heroic Games Launcher (Epic, GOG and Amazon games it installed).
    Heroic,
    /// Legendary, the command-line Epic client.
    Legendary,
    /// A Nintendo DS ROM, played with DeSmuME.
    Ds,
    /// A Nintendo 3DS ROM, played with Azahar.
    N3ds,
    /// Game Boy Advance, with mGBA.
    Gba,
    /// GameCube, with Dolphin.
    Gc,
    /// Wii, with Dolphin.
    Wii,
    /// PlayStation, with DuckStation.
    Ps1,
    /// PlayStation 2, with PCSX2.
    Ps2,
    /// PlayStation 3, with RPCS3.
    Ps3,
    /// Nintendo Switch, with Ryujinx.
    Switch,
    /// Any other system, from RetroArch's playlists.
    Retro,
    /// Found in a folder the user asked Arach to watch.
    Folder,
    /// Dropped on the window.
    Custom,
}

impl Source {
    pub const ALL: [Source; 25] = [
        Source::Steam, Source::Epic, Source::Gog, Source::Ubisoft, Source::BattleNet, Source::Riot, Source::Ea,
        Source::Xbox, Source::Amazon, Source::Rockstar, Source::Itch, Source::Heroic, Source::Legendary,
        Source::Ds, Source::N3ds, Source::Gba, Source::Gc, Source::Wii, Source::Ps1, Source::Ps2, Source::Ps3,
        Source::Switch, Source::Retro,
        Source::Folder, Source::Custom,
    ];
    /// The stores Arach reads by itself (settings can turn each off).
    pub const STORES: [Source; 23] = [
        Source::Steam, Source::Epic, Source::Gog, Source::Ubisoft, Source::BattleNet, Source::Riot, Source::Ea,
        Source::Xbox, Source::Amazon, Source::Rockstar, Source::Itch, Source::Heroic, Source::Legendary,
        Source::Ds, Source::N3ds, Source::Gba, Source::Gc, Source::Wii, Source::Ps1, Source::Ps2, Source::Ps3,
        Source::Switch, Source::Retro,
    ];
    /// The consoles whose ROMs Arach finds and hands to an emulator.
    pub const CONSOLES: [Source; 10] = [
        Source::Ds, Source::N3ds, Source::Gba, Source::Gc, Source::Wii, Source::Ps1, Source::Ps2, Source::Ps3,
        Source::Switch, Source::Retro,
    ];
    /// Sources that appeared in settings version 2: switched on for settings saved before.
    pub const SINCE_V2: [Source; 14] = [
        Source::Xbox, Source::Amazon, Source::Rockstar, Source::Itch, Source::Heroic, Source::Legendary,
        Source::Gba, Source::Gc, Source::Wii, Source::Ps1, Source::Ps2, Source::Ps3, Source::Switch, Source::Retro,
    ];

    pub fn is_console(self) -> bool {
        Source::CONSOLES.contains(&self)
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Steam => "Steam",
            Source::Epic => "Epic Games",
            Source::Gog => "GOG",
            Source::Ubisoft => "Ubisoft",
            Source::BattleNet => "Battle.net",
            Source::Riot => "Riot Games",
            Source::Ea => "EA",
            Source::Xbox => "Xbox",
            Source::Amazon => "Amazon Games",
            Source::Rockstar => "Rockstar",
            Source::Itch => "itch.io",
            Source::Heroic => "Heroic",
            Source::Legendary => "Legendary",
            Source::Ds => "Nintendo DS",
            Source::N3ds => "Nintendo 3DS",
            Source::Gba => "Game Boy Advance",
            Source::Gc => "GameCube",
            Source::Wii => "Wii",
            Source::Ps1 => "PlayStation",
            Source::Ps2 => "PlayStation 2",
            Source::Ps3 => "PlayStation 3",
            Source::Switch => "Switch",
            Source::Retro => "RetroArch",
            Source::Folder => "Dossiers",
            Source::Custom => "Ajoutés",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Launch {
    /// A store link (`steam://…`), handed to the shell.
    Uri(String),
    /// An executable (or shortcut) with its arguments and working directory.
    Exe { path: PathBuf, args: String, cwd: Option<PathBuf> },
}

impl Default for Launch {
    fn default() -> Self {
        Launch::Uri(String::new())
    }
}

/// Artwork found on disk (Steam's library cache) or downloaded.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Art {
    /// 2:3 box art.
    pub cover: Option<PathBuf>,
    /// Wide key art behind the title.
    pub hero: Option<PathBuf>,
    /// The hero, pre-blurred by Steam: tints the page.
    pub hero_blur: Option<PathBuf>,
    /// The title as a transparent logo.
    pub logo: Option<PathBuf>,
}

impl Art {
    pub fn or(self, other: &Art) -> Art {
        Art {
            cover: self.cover.or_else(|| other.cover.clone()),
            hero: self.hero.or_else(|| other.hero.clone()),
            hero_blur: self.hero_blur.or_else(|| other.hero_blur.clone()),
            logo: self.logo.or_else(|| other.logo.clone()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Game {
    /// Stable across scans: `steam:4000`, `epic:<AppName>`, `custom:<hash>`…
    pub id: String,
    pub name: String,
    pub source: Source,
    pub dir: Option<PathBuf>,
    pub launch: Launch,
    /// An `.exe`/`.ico` (icon extracted) or a `.jpg`/`.png` (drawn as is).
    pub icon: Option<PathBuf>,
    /// Last time the store says it was played, Unix seconds (0: unknown).
    pub store_played: i64,
    /// Bytes on disk as the store reports them (0: unknown, measured later).
    #[serde(default)]
    pub size: u64,
    /// When its folder was created, Unix seconds.
    #[serde(default)]
    pub installed: i64,
    /// The Steam app id: its own for Steam games, looked up for others.
    #[serde(default)]
    pub steam_appid: Option<u32>,
    /// Minutes played, as the store counts them (Steam).
    #[serde(default)]
    pub playtime_min: u64,
    #[serde(default)]
    pub art: Art,
    /// For a DS or 3DS ROM: what its header says.
    #[serde(default)]
    pub rom: Option<Rom>,
    /// Owned on its store but not installed: `launch` is then the store's install link.
    #[serde(default)]
    pub not_installed: bool,
    /// Box art the store links to (itch.io), downloaded with the other pictures.
    #[serde(default)]
    pub remote_cover: Option<String>,
    /// Steam has an update waiting for it (the manifest's `StateFlags`).
    #[serde(default)]
    pub update_pending: bool,
}

/// What a console ROM says about itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rom {
    /// The game code: four letters for DS, 3DS and GBA (`CPUF`, `ECRA`, region last), six for
    /// GameCube and Wii (`GALE01`), the disc serial for PlayStation (`SLUS-20062`, `BLUS30443`).
    pub code: String,
    pub publisher: String,
    /// The file name without its extension, or RetroArch's label: often the exact No-Intro or
    /// Redump name libretro's thumbnails are filed under.
    pub stem: String,
    /// For a RetroArch game: the playlist's system (`Nintendo - Super Nintendo Entertainment System`).
    pub system: String,
}

impl Game {
    /// The drive the game is installed on: `"D:"`.
    pub fn drive(&self) -> Option<String> {
        let dir = self.dir.as_ref()?.to_string_lossy().replace('/', "\\");
        let b = dir.as_bytes();
        (b.len() >= 2 && b[1] == b':').then(|| format!("{}:", (b[0] as char).to_ascii_uppercase()))
    }
}

pub fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"))
}

/// Lowercase, forward slashes, no trailing slash: for comparing install folders across stores.
pub fn norm_dir(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/").to_lowercase();
    s.trim_end_matches('/').to_string()
}

/// When a folder was created, Unix seconds (0 when unknown).
pub fn created(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.created())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}
