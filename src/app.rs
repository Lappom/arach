//! The window's state: which page shows, the library as the view sees it, and what each
//! click, key or background answer does to it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rune::{AppHandle, Image, ImageCache, TextEdit, Tx};

use crate::drives::Drive;
use crate::game::{Game, Launch, Source, is_image};
use crate::meta::{self, Meta};
use crate::settings::{Actions, AfterLaunch, Settings, Sort, StartPage};
use crate::store::{Collection, Session, Status};
use crate::{actions, detect, discord, drives, launch, search, store, system, track};

mod extras;
mod mcp;
mod organize;
pub use organize::{EDIT_FIELDS, Filter, Picker, Played, toggle};
mod updates;

pub(crate) use organize::Grouped;
pub use mcp::McpApp;
pub use updates::Update;

const NOTICE_SECS: f64 = 3.4;
pub const ICON_PT: f32 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    General,
    Library,
    Appearance,
    Online,
    Assistants,
    Hidden,
    About,
}

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::General, Tab::Library, Tab::Appearance, Tab::Online, Tab::Assistants, Tab::Hidden, Tab::About];
    pub fn label(self) -> &'static str {
        match self {
            Tab::General => "Général",
            Tab::Library => "Bibliothèque",
            Tab::Appearance => "Apparence",
            Tab::Online => "En ligne",
            Tab::Assistants => "Assistants IA",
            Tab::Hidden => "Jeux masqués",
            Tab::About => "À propos",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Home,
    Library,
    Game(String),
    Settings(Tab),
}

/// Which games the library page lists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shelf {
    All,
    Favorites,
    Store(Source),
    /// Games on one drive, by letter.
    Drive(char),
    /// A collection the user made, by id.
    Collection(u64),
    /// Games owned on a store but not installed.
    NotInstalled,
}

/// What floats over the page. One at a time: opening one closes the other.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Overlay {
    #[default]
    None,
    /// A dropdown, by its key.
    Select(&'static str),
    /// A game's actions: at the pointer after a right-click, or under the page's "more" button.
    GameMenu { id: String, at: Option<(f32, f32)> },
    /// A destructive action waiting for a yes.
    Confirm(Confirm),
    /// Naming a collection: a new one (adding `add` to it once made), or renaming `id`.
    NameCollection { id: Option<u64>, add: Option<String> },
    /// The library's filter panel, under its button.
    Filters,
    /// Correcting a game's name and facts.
    EditGame(String),
    /// Choosing a game's cover, background or logo.
    ArtPicker { id: String, kind: meta::picker::Kind },
    /// The selection bar's collection and status menus.
    BulkCollections,
    BulkStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Confirm {
    ClearCache,
    RemoveFolder(PathBuf),
    DeleteCollection(u64),
    /// Merge these games into the first.
    Merge(Vec<String>),
}

/// Whose actions before and after playing are being edited: every game's, or one game's own.
#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    Global,
    Game(String),
}

/// Where a game dragged from the library can be dropped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DropTarget {
    Favorites,
    Collection(u64),
}

/// A game being dragged by its cover or row, toward a collection in the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct Dragging {
    pub id: String,
    /// The pointer, in window points.
    pub at: (f32, f32),
    /// Past the threshold: the ghost shows and drop targets light up.
    pub active: bool,
}

pub struct Notice {
    pub text: String,
    pub error: bool,
    pub until: f64,
}

pub struct State {
    /// Last scan, as found on disk.
    library: Vec<Game>,
    pub user: store::UserData,
    pub settings: Settings,
    pub cache: meta::Cache,
    /// `library` plus the user's own games, with fetched art and measured sizes folded in.
    pub games: Vec<Game>,
    names: Vec<String>,
    pub query: TextEdit,
    /// The launch-options field on a game's page.
    pub args: TextEdit,
    /// The notes field on a game's page.
    pub notes: TextEdit,
    /// The name field of the collection dialog.
    pub name_field: TextEdit,
    /// What the library page keeps, on top of the shelf and the search.
    pub filter: Filter,
    /// Selection mode: presses pick games instead of opening them.
    pub selecting: bool,
    /// Picked games, in the order they were picked.
    pub selected: Vec<String>,
    /// The edit dialog's fields (`EDIT_FIELDS`).
    pub edit_fields: Vec<TextEdit>,
    pub picker: Picker,
    /// The SteamGridDB key field in Paramètres › En ligne.
    pub sgdb_field: TextEdit,
    /// Primary id → the other copies of the game (another store, a watched folder).
    pub versions: HashMap<String, Vec<Game>>,
    /// Version id → the game it's shown under.
    owner: HashMap<String, String>,
    /// Game id → its facts, the user's corrections applied.
    metas: HashMap<String, Meta>,
    /// Game id → when Arach saw it start, while it runs.
    pub playing: HashMap<String, i64>,
    /// Games whose processes are being watched (launched, maybe not running yet), with what
    /// to look for.
    watching: HashMap<String, track::Target>,
    /// Games Arach was asked to stop, until their session ends.
    pub stopping: HashSet<String>,
    /// Game id → what to put back once it closes (power plan, resolution, Discord, script).
    restore: HashMap<String, actions::Restore>,
    /// The game shown on the Discord profile.
    presence: Option<String>,
    /// The scripts run before and after every game, as typed in Paramètres.
    pub scripts: [TextEdit; 2],
    /// The same, for the game whose page shows, when it has its own.
    pub game_scripts: [TextEdit; 2],
    /// The Discord application id field.
    pub dragging: Option<Dragging>,
    /// Where each drop target sits in the window, as of the last frame.
    pub drop_targets: RefCell<Vec<(DropTarget, [f32; 4])>>,
    pub page: Page,
    history: Vec<Page>,
    pub shelf: Shelf,
    /// Indices into `games` for the library page, filtered and sorted.
    pub visible: Vec<usize>,
    pub scanning: bool,
    /// Store lookups still running.
    pub fetching: usize,
    pub drives: Vec<Drive>,
    /// Where Azahar, DeSmuME and their ROM folders are, as of the last scan.
    pub emulators: Option<detect::roms::Setup>,
    pub drop_over: bool,
    pub overlay: Overlay,
    pub notice: Option<Notice>,
    pub icons: HashMap<String, Image>,
    icons_settled: usize,
    icon_cache: Option<ImageCache>,
    pub images: RefCell<Images>,
    pub cache_bytes: u64,
    /// The arachnid emblem for the sidebar.
    pub brand: Option<Image>,
    /// Arach's own update, checked on GitHub.
    pub update: Update,
    /// Started by an update: says so once the window is up.
    pub just_updated: bool,
    /// Game id → its saves, screenshots, achievements… as last gathered for its page.
    pub extras: HashMap<String, crate::extras::Extras>,
    hltb: crate::extras::hltb::Cache,
    /// Paramètres › Assistants IA: each AI app, whether Arach is in it, and what it last did.
    pub mcp: Option<mcp::McpView>,
    pub me: AppHandle<State>,
}

/// Pictures the view asks for while it builds (covers, heroes, logos), bounded so a long
/// library doesn't keep every decoded cover alive. Each loads from a copy shrunk to the width
/// it draws at (`thumbs`), never the full-size file.
#[derive(Default)]
pub struct Images {
    cache: Option<ImageCache>,
    thumbs: Option<crate::thumbs::Thumbs>,
    map: HashMap<(PathBuf, u32), (Image, u64)>,
    /// The file each picture loads from, once its small copy is known.
    resolved: HashMap<(PathBuf, u32), PathBuf>,
    /// Pictures whose small copy is being looked for or made.
    pending: HashSet<(PathBuf, u32)>,
    tick: u64,
    settled: usize,
    /// The window's pixels per point, as of the last view build.
    pub scale: f32,
}

const MAX_IMAGES: usize = 160;

impl Images {
    /// The picture at `path`, decoded to fit `px` pixels wide on a 2× screen (callers size for
    /// one; a 1× window gets half as many, a quarter of the memory); `None` before the window
    /// exists and while its small copy is being made.
    pub fn get(&mut self, path: &std::path::Path, px: u32) -> Option<Image> {
        let cache = self.cache.as_ref()?;
        self.tick += 1;
        let density = if self.scale > 0.0 { (self.scale / 2.0).clamp(0.5, 1.5) } else { 1.0 };
        let px = (px as f32 * density).ceil() as u32;
        let key = (path.to_path_buf(), crate::thumbs::bucket(px));
        if let Some((img, t)) = self.map.get_mut(&key) {
            *t = self.tick;
            return Some(img.clone());
        }
        let Some(file) = self.resolved.get(&key) else {
            if let Some(thumbs) = &self.thumbs && self.pending.insert(key.clone()) {
                thumbs.request(key);
            }
            return None;
        };
        let img = cache.file(file, key.1 as f32 / 2.0, 2.0);
        self.map.insert(key, (img.clone(), self.tick));
        if self.map.len() > MAX_IMAGES {
            let mut ages: Vec<_> = self.map.iter().map(|(k, (_, t))| (*t, k.clone())).collect();
            ages.sort();
            for (_, k) in ages.into_iter().take(self.map.len() - MAX_IMAGES) {
                self.map.remove(&k);
            }
        }
        Some(img)
    }

    /// True when a picture finished (or failed) since the last call.
    fn changed(&mut self) -> bool {
        let mut resolved = false;
        if let Some(thumbs) = &self.thumbs {
            for (key, file) in thumbs.finished() {
                self.pending.remove(&key);
                self.resolved.insert(key, file);
                resolved = true;
            }
        }
        let settled = self.map.values().filter(|(i, _)| i.is_ready() || i.error().is_some()).count();
        std::mem::replace(&mut self.settled, settled) != settled || resolved
    }

    /// Forgets every picture, for when the files under them were deleted.
    fn forget(&mut self) {
        self.map.clear();
        self.resolved.clear();
        self.pending.clear();
        self.settled = 0;
    }
}

impl State {
    pub fn new(
        me: AppHandle<State>,
        library: Vec<Game>,
        user: store::UserData,
        settings: Settings,
        cache: meta::Cache,
    ) -> State {
        let mut query = TextEdit::new("");
        query.focused = false;
        let page = match settings.start_page {
            StartPage::Home => Page::Home,
            StartPage::Library => Page::Library,
        };
        let scripts = [TextEdit::new(&settings.actions.before), TextEdit::new(&settings.actions.after)];
        let mut s = State {
            library,
            user,
            settings,
            cache,
            games: Vec::new(),
            names: Vec::new(),
            query,
            args: TextEdit::new(""),
            notes: TextEdit::new(""),
            name_field: TextEdit::new(""),
            filter: Filter::default(),
            selecting: false,
            selected: Vec::new(),
            edit_fields: Vec::new(),
            picker: Picker::default(),
            sgdb_field: TextEdit::new(""),
            versions: HashMap::new(),
            owner: HashMap::new(),
            metas: HashMap::new(),
            playing: HashMap::new(),
            watching: HashMap::new(),
            stopping: HashSet::new(),
            restore: HashMap::new(),
            presence: None,
            scripts,
            game_scripts: [TextEdit::new(""), TextEdit::new("")],
            dragging: None,
            drop_targets: RefCell::new(Vec::new()),
            page,
            history: Vec::new(),
            shelf: Shelf::All,
            visible: Vec::new(),
            scanning: false,
            fetching: 0,
            drives: Vec::new(),
            emulators: None,
            drop_over: false,
            overlay: Overlay::None,
            notice: None,
            icons: HashMap::new(),
            icons_settled: 0,
            icon_cache: None,
            images: RefCell::new(Images::default()),
            cache_bytes: 0,
            brand: None,
            update: Update::Idle,
            just_updated: false,
            extras: HashMap::new(),
            hltb: crate::extras::hltb::load_cache(),
            mcp: None,
            me,
        };
        s.sgdb_field = TextEdit::new(&s.settings.sgdb_key).secure();
        s.rebuild();
        s
    }

    /// Runs once the window exists: pictures start decoding and a fresh scan starts behind the
    /// cached library.
    pub fn setup(&mut self, waker: rune::Waker) {
        let images = self.images.get_mut();
        images.cache = Some(ImageCache::new(waker.clone()));
        images.thumbs = Some(crate::thumbs::Thumbs::new(waker.clone()));
        let icons = ImageCache::new(waker);
        // The app icon's emblem without its black tile.
        // The tile is the SVG's only <rect>; cut it whatever its colour, so recolouring it can't break this.
        let svg = include_str!("../assets/icon.svg");
        let emblem = match svg.find("<rect").and_then(|a| svg[a..].find("/>").map(|b| (a, a + b + 2))) {
            Some((a, b)) => format!("{}{}", &svg[..a], &svg[b..]),
            None => svg.to_string(),
        };
        self.brand = Some(icons.svg(emblem.into_bytes(), 30.0, 2.0));
        self.icon_cache = Some(icons);
        self.sync_icons();
        system::warm();
        if self.settings.scan_on_start || self.library.is_empty() {
            self.rescan();
        } else {
            self.after_scan();
        }
        if self.just_updated {
            self.me.update(|s: &mut State, tx: &mut Tx| s.updated(tx));
        }
        if self.settings.auto_update && crate::update::auto_allowed() {
            self.check_update(false);
        }
    }

    /// Called every frame; true only when something the view shows changed.
    pub fn poll(&mut self, now: f64) -> bool {
        let mut changed = self.images.get_mut().changed();
        let settled = self.icons.values().filter(|i| i.is_ready() || i.error().is_some()).count();
        if settled != self.icons_settled {
            self.icons_settled = settled;
            changed = true;
        }
        if self.notice.as_ref().is_some_and(|n| n.until <= now) {
            self.notice = None;
            changed = true;
        }
        changed
    }
}

// The icon cache is separate from the bounded picture store: icons are tiny and every list
// row shows one.
impl State {
    fn sync_icons(&mut self) {
        let Some(cache) = &self.icon_cache else { return };
        self.icons.retain(|id, _| self.games.iter().any(|g| &g.id == id));
        for g in &self.games {
            if self.icons.contains_key(&g.id) { continue; }
            let Some(path) = &g.icon else { continue };
            let image = if is_image(path) { cache.file(path, ICON_PT, 2.0) } else { cache.app_icon(path, ICON_PT) };
            self.icons.insert(g.id.clone(), image);
        }
    }
}

// ---------------------------------------------------------------- the library

/// The library as the window shows it: the last scan and the user's own games, with fetched
/// art and measured sizes folded in, each game's copies grouped under it. The MCP server
/// (`crate::mcp`) builds the same from the files on disk.
pub(crate) fn assemble(library: &[Game], user: &store::UserData, cache: &meta::Cache) -> Grouped {
    let all: Vec<Game> = library
        .iter()
        .chain(user.custom.iter())
        .map(|g| {
            let mut g = g.clone();
            if let Some(m) = cache.meta.get(&g.id) {
                g.art = g.art.or(&m.art);
                g.steam_appid = g.steam_appid.or(m.appid);
            }
            if g.size == 0 && let Some(&size) = cache.sizes.get(&g.id) {
                g.size = size;
            }
            g
        })
        .collect();
    organize::group(all, user, cache)
}

impl State {
    /// Folds fetched art and measured sizes into the scan, groups the copies of each game, and
    /// recomputes every list.
    fn rebuild(&mut self) {
        let grouped = assemble(&self.library, &self.user, &self.cache);
        (self.games, self.versions, self.owner, self.metas) = (grouped.games, grouped.versions, grouped.owner, grouped.metas);
        self.selected.retain(|id| self.games.iter().any(|g| &g.id == id));
        self.names = self.games.iter().map(|g| search::normalize(&g.name)).collect();
        self.sync_icons();
        self.refilter();
    }

    /// Recomputes the library page's list from the shelf, the query and the sort.
    pub fn refilter(&mut self) {
        let q = search::normalize(&self.query.text);
        let mut hits: Vec<(i32, usize)> = self
            .games
            .iter()
            .enumerate()
            .filter(|(_, g)| self.in_library(g) && self.on_shelf(g, self.shelf))
            .filter(|(_, g)| self.passes_filter(g))
            .filter_map(|(i, _)| Some((search::score(&q, &self.names[i])?, i)))
            .collect();
        let g = &self.games;
        hits.sort_by(|a, b| {
            // Installed games first: those to install follow, greyed.
            let by_install = g[a.1].not_installed.cmp(&g[b.1].not_installed);
            let by_match = b.0.cmp(&a.0);
            let by_sort = match self.settings.sort {
                Sort::Recent => self.last_played(&g[b.1]).cmp(&self.last_played(&g[a.1])),
                Sort::Name => std::cmp::Ordering::Equal,
                Sort::Size => g[b.1].size.cmp(&g[a.1].size),
                Sort::Playtime => self.playtime_min(&g[b.1]).cmp(&self.playtime_min(&g[a.1])),
                Sort::Rating => self.user.entry(&g[b.1].id).rating.cmp(&self.user.entry(&g[a.1].id).rating),
            };
            by_install.then(by_match).then(by_sort).then_with(|| self.names[a.1].cmp(&self.names[b.1]))
        });
        self.visible = hits.into_iter().map(|h| h.1).collect();
    }

    /// The last time the game, or any of its versions, was played.
    pub fn last_played(&self, g: &Game) -> i64 {
        self.ids_of(&g.id).iter().fold(g.store_played, |last, id| {
            let session = self.user.sessions.get(id).and_then(|v| v.last()).map_or(0, |s| s.end);
            last.max(self.user.played.get(id).copied().unwrap_or(0)).max(session)
        })
    }

    /// Minutes played: what the store counts (Steam), or what Arach measured, whichever is
    /// more; a session running now counts too.
    pub fn playtime_min(&self, g: &Game) -> u64 {
        let live = self.playing.get(&g.id).map_or(0, |start| (store::now() - start).max(0));
        let secs: i64 = self.ids_of(&g.id).iter().map(|id| self.user.tracked_secs(id)).sum();
        let tracked = ((secs + live) / 60) as u64;
        g.playtime_min.max(tracked)
    }

    /// Whether the library lists the game: not hidden, and installed unless the user wants
    /// owned games too.
    pub fn in_library(&self, g: &Game) -> bool {
        !self.user.hidden.contains(&g.id) && (!g.not_installed || self.settings.show_owned)
    }

    pub fn on_shelf(&self, g: &Game, shelf: Shelf) -> bool {
        match shelf {
            Shelf::All => true,
            Shelf::Favorites => self.user.favorites.contains(&g.id),
            Shelf::Store(s) => g.source == s,
            Shelf::Drive(c) => g.drive().is_some_and(|d| d.starts_with(c)),
            Shelf::Collection(id) => self.collection(id).is_some_and(|c| c.games.contains(&g.id)),
            Shelf::NotInstalled => g.not_installed,
        }
    }

    /// How many games the library would list on `shelf`, before any search.
    pub fn shelf_count(&self, shelf: Shelf) -> usize {
        self.games.iter().filter(|g| self.in_library(g) && self.on_shelf(g, shelf)).count()
    }

    pub fn collection(&self, id: u64) -> Option<&Collection> {
        self.user.collections.iter().find(|c| c.id == id)
    }

    pub fn game(&self, id: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.id == id)
    }

    pub fn meta(&self, id: &str) -> Option<&Meta> {
        self.metas.get(id)
    }

    pub fn is_hidden(&self, id: &str) -> bool {
        self.user.hidden.contains(id)
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.user.favorites.contains(id)
    }

    /// Shown games, most recently played first (those never played are left out).
    pub fn recent(&self) -> Vec<&Game> {
        let mut v: Vec<&Game> =
            self.shown().filter(|g| self.last_played(g) > 0).collect();
        v.sort_by_key(|g| std::cmp::Reverse(self.last_played(g)));
        v
    }

    /// Shown games, newest install first.
    pub fn newest(&self) -> Vec<&Game> {
        let mut v: Vec<&Game> = self.shown().collect();
        v.sort_by_key(|g| std::cmp::Reverse(g.installed));
        v
    }

    pub fn favorites(&self) -> Vec<&Game> {
        self.shown().filter(|g| self.user.favorites.contains(&g.id)).collect()
    }

    /// Installed games not hidden.
    pub fn shown(&self) -> impl Iterator<Item = &Game> {
        self.games.iter().filter(|g| !g.not_installed && !self.user.hidden.contains(&g.id))
    }

    /// The game the home page puts first: the last one played, else the biggest install.
    pub fn featured(&self) -> Option<&Game> {
        self.recent().into_iter().next().or_else(|| self.shown().max_by_key(|g| g.size))
    }

    pub fn count(&self, source: Source) -> usize {
        self.shown().filter(|g| g.source == source).count()
    }
}

// ---------------------------------------------------------------- navigation

impl State {
    pub fn go(&mut self, page: Page, tx: &mut Tx) {
        self.overlay = Overlay::None;
        if self.page == page { return; }
        self.save_args();
        if let Page::Game(id) = &page {
            self.load_fields(&id.clone());
        }
        if page == Page::Settings(Tab::Assistants) {
            self.refresh_mcp();
        }
        let from = std::mem::replace(&mut self.page, page);
        self.history.push(from);
        if self.history.len() > 32 { self.history.remove(0); }
        tx.instant();
        tx.scroll_to_instant("page", 0.0, 0.0);
    }

    pub fn back(&mut self, tx: &mut Tx) {
        self.overlay = Overlay::None;
        let Some(prev) = self.history.pop() else { return };
        self.save_args();
        if let Page::Game(id) = &prev {
            self.load_fields(&id.clone());
        }
        self.page = prev;
        tx.instant();
    }

    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn open_shelf(&mut self, shelf: Shelf, tx: &mut Tx) {
        self.overlay = Overlay::None;
        self.shelf = shelf;
        self.refilter();
        if self.page == Page::Library {
            tx.instant();
            tx.scroll_to_instant("page", 0.0, 0.0);
        } else {
            self.go(Page::Library, tx);
        }
    }

    /// Typing in the title bar's search shows the library with the matches.
    pub fn searched(&mut self, tx: &mut Tx) {
        tx.instant();
        self.refilter();
        if self.page != Page::Library && !self.query.text.is_empty() {
            self.shelf = Shelf::All;
            self.refilter();
            self.go(Page::Library, tx);
        }
    }

    /// Fills a game page's fields from what was saved.
    fn load_fields(&mut self, id: &str) {
        self.args = TextEdit::new(self.user.args.get(id).map(String::as_str).unwrap_or(""));
        self.notes = TextEdit::new(&self.user.entry(id).notes);
        self.open_extras(id);
        let a = self.actions_for(id);
        self.game_scripts = [TextEdit::new(&a.before), TextEdit::new(&a.after)];
    }

    /// Keeps what was typed in a game's launch options when leaving its page.
    fn save_args(&mut self) {
        let Page::Game(id) = &self.page else { return };
        let typed = self.args.text.trim().to_string();
        let before = self.user.args.get(id).cloned().unwrap_or_default();
        if typed != before {
            if typed.is_empty() { self.user.args.remove(id); } else { self.user.args.insert(id.clone(), typed); }
            store::save_user(self.user.clone());
        }
    }
}

// ---------------------------------------------------------------- actions

impl State {
    pub fn rescan(&mut self) {
        if self.scanning { return; }
        self.scanning = true;
        let me = self.me.clone();
        let opts = self.settings.scan_options();
        std::thread::spawn(move || {
            let t = std::time::Instant::now();
            let detect::Scan { games: found, emulators } = detect::scan(&opts);
            let ms = t.elapsed().as_millis() as u64;
            let disks = drives::list();
            me.update(move |s: &mut State, tx: &mut Tx| s.apply_scan(found, emulators, disks, ms, tx));
        });
    }

    fn apply_scan(&mut self, found: Vec<Game>, emulators: detect::roms::Setup, disks: Vec<Drive>, ms: u64, tx: &mut Tx) {
        self.scanning = false;
        self.emulators = Some(emulators);
        self.drives = disks;
        self.user.last_scan = store::now();
        self.user.last_scan_ms = ms;
        store::save_user(self.user.clone());
        // A refreshed library shouldn't slide covers around under the reader.
        tx.instant();
        if found != self.library {
            self.library = found;
            store::save_library(self.library.clone());
        }
        self.rebuild();
        self.after_scan();
    }

    /// Measures folders and asks the store about games it doesn't know yet.
    fn after_scan(&mut self) {
        if self.drives.is_empty() {
            let me = self.me.clone();
            std::thread::spawn(move || {
                let disks = drives::list();
                me.update(move |s: &mut State, _: &mut Tx| s.drives = disks);
            });
        }
        let sizes: Vec<(String, PathBuf)> = self
            .games
            .iter()
            .filter(|g| g.size == 0 && !self.cache.sizes.contains_key(&g.id))
            .filter_map(|g| Some((g.id.clone(), g.dir.clone()?)))
            .collect();
        let me = self.me.clone();
        meta::measure(sizes, move |id, bytes| {
            me.update(move |s: &mut State, tx: &mut Tx| {
                tx.instant();
                s.cache.sizes.insert(id, bytes);
                store::save_cache(s.cache.clone());
                s.rebuild();
            })
        });
        self.fetch_meta();
    }

    pub fn fetch_meta(&mut self) {
        if !self.settings.online || self.fetching > 0 { return; }
        let now = store::now();
        let french = self.settings.french;
        let jobs = self
            .library
            .iter()
            .chain(self.user.custom.iter())
            .filter(|g| self.cache.needs_fetch(&g.id, french, now))
            .filter(|g| !g.not_installed || self.settings.show_owned)
            .map(|g| meta::Job {
                id: g.id.clone(),
                name: self.lookup_name(&g.id, &g.name),
                appid: g.steam_appid,
                local: g.art.clone(),
                rom: g.rom.as_ref().map(|r| (g.source, r.clone())),
                remote_cover: g.remote_cover.clone(),
            })
            .collect::<Vec<_>>();
        // What's installed gets its art first; games to install follow.
        let mut jobs = jobs;
        jobs.sort_by_key(|j| self.game(&j.id).is_some_and(|g| g.not_installed));
        self.fetching = jobs.len();
        let me = self.me.clone();
        meta::fetch(jobs, french, move |id, m| {
            me.update(move |s: &mut State, tx: &mut Tx| {
                tx.instant();
                s.fetching = s.fetching.saturating_sub(1);
                s.cache.meta.insert(id, m);
                if s.fetching == 0 || s.fetching % 4 == 0 {
                    store::save_cache(s.cache.clone());
                }
                s.rebuild();
            })
        });
    }

    pub fn launch(&mut self, id: &str, tx: &mut Tx) {
        let Some(game) = self.any_game(id).cloned() else { return };
        if game.not_installed {
            self.install(&game, tx);
            return;
        }
        let primary = self.primary_id(id);
        self.save_args();
        let extra = self.user.args.get(&primary).cloned().unwrap_or_default();
        self.user.played.insert(primary.clone(), store::now());
        *self.user.launches.entry(primary.clone()).or_default() += 1;
        store::save_user(self.user.clone());
        self.notify(format!("Lancement de {}…", game.name), false, tx);
        let me = self.me.clone();
        let target = watch_target(&game);
        // Actions wait for the game to close to put things back, so they need to see it close.
        // A game already running had them run when it started.
        let acts = self.actions_for(&primary).clone();
        let skipped = !acts.is_empty() && target.is_empty();
        let acts = (!acts.is_empty() && !target.is_empty() && !self.watching.contains_key(&primary)).then_some(acts);
        let mut after = self.settings.after_launch;
        // Quitting would leave the PC changed and the script after never run.
        if after == AfterLaunch::Quit && acts.as_ref().is_some_and(Actions::has_after) {
            after = AfterLaunch::Minimize;
        }
        std::thread::spawn(move || {
            let (mut restore, mut errors) = acts.as_ref().map(actions::before).unwrap_or_default();
            let result = launch::launch(&game, &extra);
            if result.is_err() {
                errors.extend(std::mem::take(&mut restore).run());
            }
            me.update(move |s: &mut State, tx: &mut Tx| match result {
                Ok(()) => {
                    tx.instant();
                    // Sessions count for the game the library shows.
                    let mut game = game;
                    game.id = primary;
                    if !restore.is_empty() { s.restore.insert(game.id.clone(), restore); }
                    s.watch(&game, target);
                    s.refilter();
                    if let Some(e) = errors.into_iter().next() {
                        s.notify(e, true, tx);
                    } else if skipped {
                        s.notify(format!("{} démarre sans les actions : Arach ne peut pas voir quand il se ferme", game.name), false, tx);
                    }
                    match after {
                        AfterLaunch::Minimize => tx.minimize(),
                        AfterLaunch::Quit => tx.quit(),
                        AfterLaunch::Stay => {}
                    }
                }
                Err(e) => s.notify(e, true, tx),
            });
        });
    }

    /// Ends a running game: every process Arach counts as it. The session then closes as if
    /// the player had quit, and the actions after it run.
    pub fn stop(&mut self, id: &str, tx: &mut Tx) {
        let Some(target) = self.watching.get(id).cloned() else { return };
        let name = self.game(id).map(|g| g.name.clone()).unwrap_or_default();
        self.stopping.insert(id.to_string());
        tx.instant();
        let (me, id) = (self.me.clone(), id.to_string());
        std::thread::spawn(move || {
            let ended = track::kill(&target);
            me.update(move |s: &mut State, tx: &mut Tx| {
                if ended > 0 {
                    s.notify(format!("{name} arrêté"), false, tx);
                    return;
                }
                s.stopping.remove(&id);
                if s.playing.contains_key(&id) {
                    s.notify(format!("Arach n'a pas pu arrêter {name} : lancé en administrateur ?"), true, tx);
                } else {
                    // Launched, but its launcher hasn't started it yet: stop waiting for it.
                    s.closed(&id);
                    s.notify(format!("{name} ne tournait pas encore"), false, tx);
                }
            });
        });
    }

    /// Games launched from Arach and not closed yet, newest first, with when each started
    /// (`None` while its launcher is still getting it going).
    pub fn running(&self) -> Vec<(&Game, Option<i64>)> {
        let mut v: Vec<(&Game, Option<i64>)> = self.watching.keys()
            .filter_map(|id| Some((self.game(id)?, self.playing.get(id).copied())))
            .collect();
        v.sort_by_key(|(g, start)| (std::cmp::Reverse(start.unwrap_or(i64::MAX)), g.name.clone()));
        v
    }

    /// Whether Arach can see the game close, which actions and the stop button need.
    pub fn trackable(&self, g: &Game) -> bool {
        !watch_target(g).is_empty()
    }

    pub fn is_launched(&self, id: &str) -> bool {
        self.watching.contains_key(id)
    }

    /// Shows the game started last on the Discord profile, or nothing.
    fn update_presence(&mut self) {
        let on = self.settings.discord_presence;
        let latest = self.playing.iter().max_by_key(|(_, start)| **start).map(|(id, start)| (id.clone(), *start));
        let want = latest.filter(|_| on);
        let key = want.as_ref().map(|(id, start)| format!("{id}@{start}"));
        if key == self.presence { return; }
        self.presence = key;
        let Some((id, start)) = want else {
            discord::clear();
            return;
        };
        let Some(g) = self.game(&id) else { return };
        let store = match g.source {
            Source::Custom | Source::Folder => String::new(),
            s => s.label().to_string(),
        };
        let image = g.steam_appid
            .map(|a| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{a}/header.jpg"))
            .or_else(|| g.remote_cover.clone().filter(|u| u.starts_with("https://")));
        discord::set(discord::Activity { game: g.name.clone(), store, start, image });
    }

    // ------------------------------------------------ actions before and after

    /// The actions that run around `id`: its own, else everyone's.
    pub fn actions_for(&self, id: &str) -> &Actions {
        self.user.actions.get(id).unwrap_or(&self.settings.actions)
    }

    pub fn scope_actions(&self, scope: &Scope) -> &Actions {
        match scope {
            Scope::Global => &self.settings.actions,
            Scope::Game(id) => self.actions_for(id),
        }
    }

    pub fn has_own_actions(&self, id: &str) -> bool {
        self.user.actions.contains_key(id)
    }

    pub fn edit_actions(&mut self, scope: &Scope, tx: &mut Tx, f: impl FnOnce(&mut Actions)) {
        tx.instant();
        match scope {
            Scope::Global => self.set(tx, |st| f(&mut st.actions)),
            Scope::Game(id) => {
                let mut a = self.actions_for(id).clone();
                f(&mut a);
                self.user.actions.insert(id.clone(), a);
                store::save_user(self.user.clone());
            }
        }
    }

    /// Gives a game its own actions (starting from everyone's), or puts it back on everyone's.
    pub fn set_own_actions(&mut self, id: &str, own: bool, tx: &mut Tx) {
        tx.instant();
        if own {
            let a = self.settings.actions.clone();
            self.user.actions.entry(id.to_string()).or_insert(a);
        } else {
            self.user.actions.remove(id);
        }
        store::save_user(self.user.clone());
        let a = self.actions_for(id);
        self.game_scripts = [TextEdit::new(&a.before), TextEdit::new(&a.after)];
    }

    /// A script field changed (0: before, 1: after): kept at once.
    pub fn script_changed(&mut self, scope: &Scope, which: usize, tx: &mut Tx) {
        let text = match scope {
            Scope::Global => self.scripts[which].text.clone(),
            Scope::Game(_) => self.game_scripts[which].text.clone(),
        };
        self.edit_actions(scope, tx, |a| if which == 0 { a.before = text } else { a.after = text });
    }

    /// A game owned but not installed: its store's install page.
    fn install(&mut self, g: &Game, tx: &mut Tx) {
        let Launch::Uri(uri) = &g.launch else { return };
        match launch::open_uri(uri) {
            Ok(()) => self.notify(format!("Installation de {} dans {}…", g.name, g.source.label()), false, tx),
            Err(e) => self.notify(e, true, tx),
        }
    }

    /// Watches for the game's processes and records the session once they're gone.
    fn watch(&mut self, g: &Game, target: track::Target) {
        if self.watching.contains_key(&g.id) || target.is_empty() { return; }
        self.watching.insert(g.id.clone(), target.clone());
        let me = self.me.clone();
        let id = g.id.clone();
        track::watch(target, move |ev| {
            let id = id.clone();
            me.update(move |s: &mut State, tx: &mut Tx| s.tracked(id, ev, tx));
        });
    }

    fn tracked(&mut self, id: String, ev: track::Event, tx: &mut Tx) {
        match ev {
            track::Event::Started(at) => {
                self.playing.insert(id, at);
                self.update_presence();
            }
            // Nothing to keep: the update itself redraws the live play time.
            track::Event::Running => {}
            track::Event::Ended { start, end } => {
                self.closed(&id);
                let session = Session { start, end };
                // A crash at start or a launcher that closed at once isn't play.
                if session.secs() >= 10 {
                    let name = self.game(&id).map(|g| g.name.clone()).unwrap_or_default();
                    let min = (session.secs() / 60) as u64;
                    self.user.sessions.entry(id).or_default().push(session);
                    store::save_user(self.user.clone());
                    let took = if min == 0 { "moins d'une minute".into() } else { crate::ui::playtime(min) };
                    self.notify(format!("{name} · session de {took} enregistrée"), false, tx);
                }
                self.refilter();
            }
            track::Event::NotSeen => self.closed(&id),
        }
    }

    /// The game is gone (or never showed up): the PC goes back as it was.
    fn closed(&mut self, id: &str) {
        self.playing.remove(id);
        self.watching.remove(id);
        self.stopping.remove(id);
        self.update_presence();
        let Some(restore) = self.restore.remove(id) else { return };
        let me = self.me.clone();
        std::thread::spawn(move || {
            let errors = restore.run();
            if let Some(e) = errors.into_iter().next() {
                me.update(move |s: &mut State, tx: &mut Tx| s.notify(e, true, tx));
            }
        });
    }

    pub fn open_folder(&mut self, id: &str, tx: &mut Tx) {
        let Some(dir) = self.game(id).and_then(|g| g.dir.clone()) else { return };
        if let Err(e) = launch::open_folder(&dir) { self.notify(e, true, tx); }
    }

    /// `steam://validate/<id>` and friends.
    pub fn open_uri(&mut self, uri: &str, tx: &mut Tx) {
        if let Err(e) = launch::open_uri(uri) { self.notify(e, true, tx); }
    }

    pub fn toggle_favorite(&mut self, id: &str, tx: &mut Tx) {
        let name = self.game(id).map(|g| g.name.clone()).unwrap_or_default();
        if self.user.favorites.remove(id) {
            self.notify(format!("{name} retiré des favoris"), false, tx);
        } else {
            self.user.favorites.insert(id.to_string());
            self.notify(format!("{name} ajouté aux favoris"), false, tx);
        }
        store::save_user(self.user.clone());
        self.refilter();
    }

    // ------------------------------------------------ collections

    /// Opens the naming dialog: for a new collection (with `add` going in it), or to rename.
    pub fn name_collection(&mut self, id: Option<u64>, add: Option<String>, tx: &mut Tx) {
        let current = id.and_then(|i| self.collection(i)).map(|c| c.name.clone()).unwrap_or_default();
        self.name_field = TextEdit::new(&current);
        self.name_field.anchor = 0;
        self.name_field.focus = current.len();
        self.overlay = Overlay::NameCollection { id, add };
        tx.focus("collection-name", false);
    }

    /// The dialog's OK: makes or renames the collection.
    pub fn named_collection(&mut self, tx: &mut Tx) {
        let name = self.name_field.text.trim().to_string();
        if name.is_empty() { return; }
        let Overlay::NameCollection { id, add } = std::mem::take(&mut self.overlay) else { return };
        match id {
            Some(id) => {
                if let Some(c) = self.user.collections.iter_mut().find(|c| c.id == id) { c.name = name; }
            }
            None => {
                let id = fnv(&format!("{name}{}", store::now())) | 1;
                let games = add.iter().cloned().collect();
                self.user.collections.push(Collection { id, name: name.clone(), games });
                match add.and_then(|g| self.game(&g).map(|g| g.name.clone())) {
                    Some(game) => self.notify(format!("{game} ajouté à « {name} »"), false, tx),
                    None => self.notify(format!("Collection « {name} » créée"), false, tx),
                }
            }
        }
        store::save_user(self.user.clone());
        self.refilter();
    }

    pub fn delete_collection(&mut self, id: u64, tx: &mut Tx) {
        let Some(i) = self.user.collections.iter().position(|c| c.id == id) else { return };
        let c = self.user.collections.remove(i);
        store::save_user(self.user.clone());
        if self.shelf == Shelf::Collection(id) { self.shelf = Shelf::All; }
        self.refilter();
        self.notify(format!("Collection « {} » supprimée · les jeux restent dans la bibliothèque", c.name), false, tx);
    }

    /// Puts a game in a collection, or takes it out.
    pub fn toggle_in_collection(&mut self, cid: u64, id: &str, tx: &mut Tx) {
        let name = self.game(id).map(|g| g.name.clone()).unwrap_or_default();
        let Some(c) = self.user.collections.iter_mut().find(|c| c.id == cid) else { return };
        let msg = if let Some(i) = c.games.iter().position(|g| g == id) {
            c.games.remove(i);
            format!("{name} retiré de « {} »", c.name)
        } else {
            c.games.push(id.to_string());
            format!("{name} ajouté à « {} »", c.name)
        };
        store::save_user(self.user.clone());
        self.refilter();
        self.notify(msg, false, tx);
    }

    // ------------------------------------------------ drag and drop

    /// A cover or row dragged toward the sidebar: dropped on Favoris or a collection, the game
    /// goes in it.
    pub fn drag_game(&mut self, id: &str, d: &rune::Drag, tx: &mut Tx) {
        let [x, y] = d.window_point();
        match d.phase {
            rune::Phase::Down => self.dragging = Some(Dragging { id: id.to_string(), at: (x, y), active: false }),
            rune::Phase::Move => {
                if let Some(drag) = &mut self.dragging {
                    drag.at = (x, y);
                    drag.active |= d.dx.hypot(d.dy) > 8.0;
                }
                tx.instant();
            }
            rune::Phase::Up => {
                let Some(drag) = self.dragging.take() else { return };
                if !drag.active || d.cancelled() { return; }
                match self.drop_target_at(x, y) {
                    Some(DropTarget::Favorites) if !self.is_favorite(&drag.id) => self.toggle_favorite(&drag.id, tx),
                    Some(DropTarget::Collection(cid)) => {
                        let name = self.game(&drag.id).map(|g| g.name.clone()).unwrap_or_default();
                        match self.collection(cid) {
                            Some(c) if c.games.contains(&drag.id) => {
                                let msg = format!("{name} est déjà dans « {} »", c.name);
                                self.notify(msg, false, tx);
                            }
                            Some(_) => self.toggle_in_collection(cid, &drag.id, tx),
                            None => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// The drop target under a window point, if any.
    pub fn drop_target_at(&self, x: f32, y: f32) -> Option<DropTarget> {
        self.drop_targets.borrow().iter()
            .find(|(_, r)| x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3])
            .map(|(t, _)| *t)
    }

    // ------------------------------------------------ status, rating, notes

    pub fn set_status(&mut self, id: &str, status: Status, tx: &mut Tx) {
        tx.instant();
        self.user.edit_entry(id, |e| e.status = status);
        store::save_user(self.user.clone());
        if !self.filter.statuses.is_empty() { self.refilter(); }
    }

    /// Stars from 1 to 5; pressing the current rating clears it.
    pub fn set_rating(&mut self, id: &str, stars: u8, tx: &mut Tx) {
        tx.instant();
        self.user.edit_entry(id, |e| e.rating = if e.rating == stars { 0 } else { stars });
        store::save_user(self.user.clone());
        if self.settings.sort == Sort::Rating { self.refilter(); }
    }

    /// The notes field changed: kept at once, so nothing typed is lost to a crash.
    pub fn notes_changed(&mut self) {
        let Page::Game(id) = &self.page else { return };
        let id = id.clone();
        let text = self.notes.text.clone();
        self.user.edit_entry(&id, |e| e.notes = text);
        store::save_user(self.user.clone());
    }

    /// Hides a detected game, shows it again, or forgets a game added by hand.
    pub fn toggle_hidden(&mut self, id: &str, tx: &mut Tx) {
        let Some(g) = self.game(id).cloned() else { return };
        if g.source == Source::Custom {
            self.user.custom.retain(|c| c.id != g.id);
            self.notify(format!("{} retiré de la bibliothèque", g.name), false, tx);
            if self.page == Page::Game(g.id.clone()) { self.back(tx); }
        } else if self.user.hidden.remove(&g.id) {
            self.notify(format!("{} est de retour dans la bibliothèque", g.name), false, tx);
        } else {
            self.user.hidden.insert(g.id.clone());
            self.notify(format!("{} masqué · Paramètres › Jeux masqués pour l'afficher", g.name), false, tx);
        }
        store::save_user(self.user.clone());
        self.rebuild();
    }

    /// Files dropped on the window become games: executables, shortcuts and links. An image
    /// dropped on a game's page becomes its cover or background.
    pub fn add_paths(&mut self, paths: &[PathBuf], tx: &mut Tx) {
        if self.drop_art(paths, tx) { return; }
        let mut added = Vec::new();
        for path in paths {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "exe" | "lnk" | "url" | "bat" | "cmd") { continue; }
            let id = format!("custom:{:016x}", fnv(&path.to_string_lossy().to_lowercase()));
            if self.games.iter().any(|g| g.id == id) { continue; }
            let name = pretty_name(path);
            self.user.custom.push(Game {
                id: id.clone(),
                name: name.clone(),
                source: Source::Custom,
                dir: path.parent().map(PathBuf::from),
                launch: Launch::Exe { path: path.clone(), args: String::new(), cwd: path.parent().map(PathBuf::from) },
                icon: (ext == "exe").then(|| path.clone()),
                installed: store::now(),
                ..Default::default()
            });
            added.push((id, name));
        }
        if added.is_empty() {
            self.notify("Déposez un .exe, un raccourci .lnk ou un lien .url".into(), true, tx);
            return;
        }
        store::save_user(self.user.clone());
        self.rebuild();
        self.fetch_meta();
        if let [(id, name)] = added.as_slice() {
            self.notify(format!("{name} ajouté à la bibliothèque"), false, tx);
            let id = id.clone();
            self.go(Page::Game(id), tx);
        } else {
            self.notify(format!("{} jeux ajoutés à la bibliothèque", added.len()), false, tx);
        }
    }

    /// Applies a settings change, saves it, and redoes whatever depends on it.
    pub fn set(&mut self, tx: &mut Tx, change: impl FnOnce(&mut Settings)) {
        let before = self.settings.clone();
        change(&mut self.settings);
        if self.settings == before { return; }
        store::save_settings(self.settings.clone());
        // ROM titles come in the chosen language, so it changes what a scan finds.
        let roms_french = before.french != self.settings.french && self.games.iter().any(|g| g.rom.is_some());
        if before.stores != self.settings.stores
            || before.folders != self.settings.folders
            || before.roms != self.settings.roms
            || before.emulators != self.settings.emulators
            || roms_french
        {
            self.rescan();
        }
        if before.show_owned != self.settings.show_owned {
            self.rescan();
            self.refilter();
        }
        if before.sort != self.settings.sort {
            tx.instant();
            self.refilter();
        }
        if (!before.online && self.settings.online) || before.french != self.settings.french {
            self.fetch_meta();
        }
        if before.discord_presence != self.settings.discord_presence {
            self.update_presence();
        }
        if !before.auto_update && self.settings.auto_update {
            self.check_update(false);
        }
    }

    pub fn add_folder(&mut self, dir: PathBuf, tx: &mut Tx) {
        if self.settings.folders.contains(&dir) { return; }
        self.notify(format!("Analyse de {}…", dir.display()), false, tx);
        self.set(tx, |s| s.folders.push(dir));
    }

    pub fn clear_cache(&mut self, tx: &mut Tx) {
        store::clear_cache();
        self.images.get_mut().forget();
        self.cache.meta.clear();
        self.cache_bytes = 0;
        self.rebuild();
        self.notify("Cache vidé · les jaquettes seront téléchargées à nouveau".into(), false, tx);
        self.fetch_meta();
    }

    pub fn measure_cache(&mut self) {
        let me = self.me.clone();
        std::thread::spawn(move || {
            let bytes = store::cache_bytes();
            me.update(move |s: &mut State, _: &mut Tx| s.cache_bytes = bytes);
        });
    }

    /// Opens `o`, or closes it when it's the one showing.
    pub fn toggle_overlay(&mut self, o: Overlay) {
        self.overlay = if self.overlay == o { Overlay::None } else { o };
    }

    /// Runs the action a confirmation stood for.
    pub fn confirmed(&mut self, tx: &mut Tx) {
        match std::mem::take(&mut self.overlay) {
            Overlay::Confirm(Confirm::ClearCache) => self.clear_cache(tx),
            Overlay::Confirm(Confirm::RemoveFolder(dir)) => self.set(tx, move |st| {
                st.folders.retain(|d| *d != dir);
                st.roms.retain(|d| *d != dir);
            }),
            Overlay::Confirm(Confirm::DeleteCollection(id)) => self.delete_collection(id, tx),
            Overlay::Confirm(Confirm::Merge(ids)) => self.merge(&ids, tx),
            _ => {}
        }
    }

    pub fn notify(&mut self, text: String, error: bool, tx: &mut Tx) {
        let until = tx.clock() + NOTICE_SECS;
        self.notice = Some(Notice { text, error, until });
        tx.wake_at(until);
    }
}

/// Arach closing with a game still running: the PC goes back as it was now, since nobody will
/// be there when the game closes.
impl Drop for State {
    fn drop(&mut self) {
        for (_, r) in self.restore.drain() {
            r.run();
        }
    }
}

/// States for headless snapshots (`--scene`).
impl State {
    pub fn scene_page(&mut self, page: Page) {
        if let Page::Game(id) = &page {
            self.load_fields(&id.clone());
        }
        if page == Page::Settings(Tab::Assistants) {
            self.mcp = Some(mcp::McpView::read());
        }
        self.page = page;
    }

    pub fn scene_query(&mut self, q: &str) {
        self.query = TextEdit::new(q);
        self.query.focused = true;
        self.page = Page::Library;
        self.refilter();
    }

    pub fn scene_overlay(&mut self, page: Page, overlay: Overlay) {
        self.scene_page(page);
        self.overlay = overlay;
    }

    /// The game page while it runs, launched 25 minutes ago, with its own actions.
    pub fn scene_playing(&mut self, id: &str) {
        let Some(g) = self.game(id).cloned() else { return };
        self.watching.insert(g.id.clone(), track::Target::default());
        self.playing.insert(g.id.clone(), store::now() - 25 * 60);
        self.user.actions.insert(g.id.clone(), Actions { close_discord: true, before: r"C:\Scripts\avant.bat".into(), ..Actions::default() });
        self.scene_page(Page::Game(g.id));
    }

    pub fn scene_notice(&mut self, text: &str, error: bool) {
        self.notice = Some(Notice { text: text.into(), error, until: f64::MAX });
    }
}

/// What to watch for once `g` is launched: its folder, or the emulator for a ROM. A
/// shortcut or link dropped by hand points somewhere Arach can't see, so it isn't tracked.
fn watch_target(g: &Game) -> track::Target {
    let t = track::Target::default();
    let exe = |p: &std::path::Path| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    match (&g.launch, g.source) {
        (Launch::Exe { path, .. }, Source::Ds | Source::N3ds) => t.exe(path),
        (Launch::Exe { path, .. }, Source::Custom) if exe(path) => t.exe(path),
        (Launch::Exe { .. }, Source::Custom) => t,
        (_, _) => match &g.dir {
            Some(d) => t.dir(d),
            None => t,
        },
    }
}

pub fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// `D:\Games\Hollow_Knight\hollow_knight.exe` → "Hollow knight"; a generic name like
/// `game.exe` or `launcher.exe` takes its folder's name instead.
fn pretty_name(path: &std::path::Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let generic = matches!(stem.to_lowercase().as_str(), "game" | "launcher" | "start" | "play" | "main" | "app");
    let raw = if generic {
        path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or(stem)
    } else {
        stem
    };
    let spaced = raw.replace(['_', '-', '.'], " ");
    let mut chars = spaced.trim().chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_files_get_readable_names() {
        assert_eq!(pretty_name(std::path::Path::new(r"D:\Games\HK\hollow_knight.exe")), "Hollow knight");
        assert_eq!(pretty_name(std::path::Path::new(r"D:\Games\Celeste\Launcher.exe")), "Celeste");
    }
}
