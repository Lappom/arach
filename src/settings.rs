//! Everything the Paramètres page changes, saved in `%APPDATA%\Arach\settings.json`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::game::Source;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AfterLaunch {
    #[default]
    Minimize,
    Stay,
    Quit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartPage {
    #[default]
    Home,
    Library,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl CoverSize {
    pub fn width(self) -> f32 {
        match self {
            CoverSize::Small => 124.0,
            CoverSize::Medium => 152.0,
            CoverSize::Large => 188.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryView {
    #[default]
    Grid,
    List,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sort {
    #[default]
    Recent,
    Name,
    Size,
    Playtime,
    Rating,
}

/// What Arach does around a game: before it starts, and once it has closed. Everything changed
/// before is put back after.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Actions {
    /// A command line run before the game (Arach waits for it, a minute at most).
    pub before: String,
    /// A command line run once the game has closed.
    pub after: String,
    /// Close Discord while playing, and start it again after.
    pub close_discord: bool,
    /// The power plan to play with, by GUID; empty leaves it alone.
    pub power_plan: String,
    /// The screen resolution to play in; `None` leaves it alone.
    pub resolution: Option<(u32, u32)>,
}

impl Actions {
    pub fn is_empty(&self) -> bool {
        self.before.trim().is_empty() && !self.has_after()
    }

    /// Has something to do once the game closes: put the PC back, or run the script.
    pub fn has_after(&self) -> bool {
        self.close_discord || !self.power_plan.is_empty() || self.resolution.is_some() || !self.after.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub after_launch: AfterLaunch,
    pub start_page: StartPage,
    pub scan_on_start: bool,
    /// Stores Arach reads; the others are skipped.
    pub stores: Vec<Source>,
    /// Folders whose subfolders are games.
    pub folders: Vec<PathBuf>,
    /// Box art, descriptions and release dates from the Steam store.
    pub online: bool,
    /// Descriptions in French (else English).
    pub french: bool,
    pub theme: ThemeMode,
    /// Index into `ui::theme::ACCENTS`.
    pub accent: usize,
    pub cover_size: CoverSize,
    pub library_view: LibraryView,
    pub sort: Sort,
    /// Tint pages with the game's artwork.
    pub ambient: bool,
    /// Folders holding DS and 3DS ROMs, besides those the emulators already list.
    pub roms: Vec<PathBuf>,
    /// The emulators the user picked rather than letting Arach look, by id
    /// (`detect::emulators::EMULATORS`).
    pub emulators: BTreeMap<String, PathBuf>,
    /// Read from settings saved before `emulators` existed.
    #[serde(skip_serializing)]
    pub azahar: Option<PathBuf>,
    #[serde(skip_serializing)]
    pub desmume: Option<PathBuf>,
    /// A SteamGridDB API key: more covers to pick from.
    pub sgdb_key: String,
    /// Whether the consoles were added to `stores` (settings saved before they existed lack them).
    #[serde(default)]
    pub consoles: bool,
    /// Games bought but not installed (Steam, Epic, GOG), greyed in the library.
    pub show_owned: bool,
    /// Before and after every game, unless the game has its own (`UserData::actions`).
    pub actions: Actions,
    /// The game being played on the user's Discord profile.
    pub discord_presence: bool,
    /// The Discord application the activity is shown under (discord.com/developers).
    pub discord_app_id: String,
    /// Which settings format this was saved with (0: before versions were counted). Missing
    /// means 0, not `Settings::default()`'s current version.
    #[serde(default)]
    pub version: u32,
    /// Look for a new Arach on GitHub at startup and download it in the background.
    pub auto_update: bool,
}

pub const VERSION: u32 = 2;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            after_launch: AfterLaunch::Minimize,
            start_page: StartPage::Home,
            scan_on_start: true,
            stores: Source::STORES.to_vec(),
            folders: Vec::new(),
            online: true,
            french: true,
            theme: ThemeMode::System,
            accent: 0,
            cover_size: CoverSize::Medium,
            library_view: LibraryView::Grid,
            sort: Sort::Recent,
            ambient: true,
            roms: Vec::new(),
            emulators: BTreeMap::new(),
            azahar: None,
            desmume: None,
            sgdb_key: String::new(),
            consoles: true,
            show_owned: true,
            actions: Actions::default(),
            discord_presence: true,
            discord_app_id: String::new(),
            version: VERSION,
            auto_update: true,
        }
    }
}

impl Settings {
    pub fn scan_options(&self) -> crate::detect::ScanOptions {
        crate::detect::ScanOptions {
            stores: self.stores.clone(),
            folders: self.folders.clone(),
            roms: self.roms.clone(),
            emulators: self.emulators.clone(),
            french: self.french,
            owned: self.show_owned,
        }
    }

    /// Brings settings saved by an older Arach up to date.
    pub fn migrate(mut self) -> Self {
        if !self.consoles {
            self.consoles = true;
            for c in Source::CONSOLES {
                if !self.stores.contains(&c) { self.stores.push(c); }
            }
        }
        if self.version < 2 {
            for c in Source::SINCE_V2 {
                if !self.stores.contains(&c) { self.stores.push(c); }
            }
        }
        self.version = VERSION;
        if let Some(p) = self.azahar.take() { self.emulators.entry("azahar".into()).or_insert(p); }
        if let Some(p) = self.desmume.take() { self.emulators.entry("desmume".into()).or_insert(p); }
        self
    }
}
