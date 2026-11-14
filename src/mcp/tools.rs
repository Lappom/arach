//! What an assistant can ask Arach: the tools, their input schemas, and the answers, built from
//! the same files the window reads.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::bridge::{self, ActionsPatch, ArtKind, GamePatch, Request};
use crate::game::{Game, Source};
use crate::meta::picker;
use crate::meta::{Cache, Meta};
use crate::settings::{Actions, Settings};
use crate::store::{self, Status, UserData};
use crate::system::{self, PowerPlan};
use crate::{extras, search};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

pub fn list() -> Value {
    let stores: Vec<&str> = Source::ALL.iter().map(|s| s.label()).collect();
    json!([
        {
            "name": "list_games",
            "title": "List games",
            "description": "Lists games in the user's Arach library. With `query`, games are matched by name (substring, \
initials such as `gta`, or letters in order) and the best matches come first; without it they are sorted by `sort`. \
Hidden games are never listed. Owned but not installed games are left out unless `include_not_installed` is true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Part of the game's name." },
                    "store": { "type": "string", "enum": stores, "description": "Only games from this store or console." },
                    "favorites": { "type": "boolean", "description": "Only the user's favorites." },
                    "collection": { "type": "string", "description": "Only games in the user's collection with this name." },
                    "status": { "type": "string", "enum": STATUS_SLUGS, "description": "Only games the user marked with this status." },
                    "never_played": { "type": "boolean", "description": "Only games never launched." },
                    "include_not_installed": { "type": "boolean", "description": "Also list games owned on Steam, Epic or GOG but not installed." },
                    "sort": { "type": "string", "enum": ["recent", "name", "playtime", "size", "rating", "installed"], "description": "Order when there is no query. Default: recent (last played first)." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "description": "At most this many games (default 50)." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "get_game",
            "title": "Game details",
            "description": "Everything Arach knows about one game: description, developers, release date, genres, Metacritic \
score, playtime and sessions, install folder and size, the user's status, rating, notes and collections, and the \
other stores it was found in. Takes the id from list_games; a name also works when it matches one game well.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The game's id (`steam:1091500`), or its name." }
                },
                "required": ["id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "library_overview",
            "title": "Library overview",
            "description": "A summary of the whole library: how many games per store, total playtime and disk size, the most \
played and most recently played games, favorites, collections and how many games have each status.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "launch_game",
            "title": "Launch a game",
            "description": "Starts an installed game on the user's PC through Arach, which measures the session and runs \
the user's before/after actions. Only call it when the user asked to play. Opens the Arach window if it isn't running.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The game's id from list_games." }
                },
                "required": ["id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "edit_game",
            "title": "Edit a game",
            "description": "Changes what Arach shows and keeps about a game. Only the fields given change. The facts (name, \
description, developers, publishers, release, genres) override what the stores say; `null` puts the store's value \
back. Also sets the user's status, rating (0 clears it), notes (replaces them), favorite, and collections (by name; \
a missing collection is created). Shows in the Arach window at once.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The game's id from list_games." },
                    "name": { "type": ["string", "null"] },
                    "description": { "type": ["string", "null"] },
                    "developers": { "type": ["array", "null"], "items": { "type": "string" } },
                    "publishers": { "type": ["array", "null"], "items": { "type": "string" } },
                    "release": { "type": ["string", "null"], "description": "Release date as the user wants it shown, e.g. `2020-12-10` or `10 déc. 2020`." },
                    "genres": { "type": ["array", "null"], "items": { "type": "string" } },
                    "status": { "type": "string", "enum": STATUS_SLUGS },
                    "rating": { "type": "integer", "minimum": 0, "maximum": 5 },
                    "notes": { "type": "string" },
                    "favorite": { "type": "boolean" },
                    "add_to_collections": { "type": "array", "items": { "type": "string" } },
                    "remove_from_collections": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "find_images",
            "title": "Find images",
            "description": "Looks for pictures of a game online (the Steam store, and SteamGridDB when the user gave Arach a \
key): covers (portrait, 2:3), backgrounds (wide hero art) or logos (transparent). Returns their URLs, to show the \
user or pass to set_image.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The game's id from list_games." },
                    "kind": { "type": "string", "enum": ARTS },
                    "query": { "type": "string", "description": "Name to search for, when the game's own name finds nothing." }
                },
                "required": ["id", "kind"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": true }
        },
        {
            "name": "set_image",
            "title": "Set an image",
            "description": "Sets a game's cover, background or logo: from an https URL (find_images gives some), from an image \
file on this PC (PNG or JPEG), or back to the store's with `reset`. Give exactly one of `url`, `file` or `reset`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The game's id from list_games." },
                    "kind": { "type": "string", "enum": ARTS },
                    "url": { "type": "string", "description": "An https link to a PNG or JPEG." },
                    "file": { "type": "string", "description": "Full path of a PNG or JPEG on this PC." },
                    "reset": { "type": "boolean", "description": "Go back to the picture the store provides." }
                },
                "required": ["id", "kind"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
        },
        {
            "name": "get_launch_actions",
            "title": "Launch actions",
            "description": "What Arach does before and after a game: close Discord (reopened after), switch the power plan, \
change the screen resolution, run a script before and one after. Without `id`: the actions every game uses. With \
`id`: that game's, and whether they are its own or every game's. Also lists the power plans and resolutions this PC \
offers, and what the user lets assistants change.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "A game's id; omit for every game's actions." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "set_launch_actions",
            "title": "Change launch actions",
            "description": "Changes what Arach does before and after a game. Without `id`, for every game (except games with \
their own). With `id`, for that game only: it then gets its own actions, starting from every game's, unless \
`use_global_actions` is true, which drops its own. Only the fields given change. `power_plan`: a name or GUID from \
get_launch_actions, or null to leave the plan alone. `resolution`: like `1920x1080`, or null to leave it alone. \
Scripts are command lines run with `cmd /C` (empty string removes one); the user must allow them separately.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "A game's id; omit to change every game's actions." },
                    "use_global_actions": { "type": "boolean", "description": "With `id`: true drops the game's own actions." },
                    "close_discord": { "type": "boolean" },
                    "power_plan": { "type": ["string", "null"] },
                    "resolution": { "type": ["string", "null"] },
                    "script_before": { "type": "string" },
                    "script_after": { "type": "string" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }
    ])
}

const ARTS: [&str; 3] = ["cover", "background", "logo"];

pub fn exists(name: &str) -> bool {
    matches!(name, "list_games" | "get_game" | "library_overview" | "launch_game" | "edit_game"
        | "find_images" | "set_image" | "get_launch_actions" | "set_launch_actions")
}

pub fn call(name: &str, args: &Value, settings: &Settings) -> Result<Value, String> {
    let lib = Library::load(settings.clone());
    match name {
        "list_games" => lib.list_games(args),
        "get_game" => lib.get_game(args),
        "library_overview" => Ok(lib.overview()),
        "launch_game" => launch_game(&lib, args, settings),
        "edit_game" => edit_game(&lib, args, settings),
        "find_images" => find_images(&lib, args, settings),
        "set_image" => set_image(&lib, args, settings),
        "get_launch_actions" => lib.launch_actions(args),
        "set_launch_actions" => set_launch_actions(&lib, args, settings),
        _ => Err(format!("Unknown tool: {name}")),
    }
}

const STATUS_SLUGS: [&str; 5] = ["not_started", "playing", "finished", "completed", "abandoned"];

fn status_slug(s: Status) -> &'static str {
    match s {
        Status::NotStarted => "not_started",
        Status::Playing => "playing",
        Status::Finished => "finished",
        Status::Completed => "completed",
        Status::Abandoned => "abandoned",
    }
}

/// The library as an assistant sees it: the window's, hidden games aside.
pub struct Library {
    games: Vec<Game>,
    versions: HashMap<String, Vec<Game>>,
    owner: HashMap<String, String>,
    metas: HashMap<String, Meta>,
    user: UserData,
    settings: Settings,
}

impl Library {
    fn load(settings: Settings) -> Library {
        Library::from_parts(&store::load_library(), store::load_user(), &store::load_cache(), settings)
    }

    pub fn from_parts(library: &[Game], user: UserData, cache: &Cache, settings: Settings) -> Library {
        let g = crate::app::assemble(library, &user, cache);
        Library { games: g.games, versions: g.versions, owner: g.owner, metas: g.metas, user, settings }
    }

    fn ids_of<'a>(&'a self, g: &'a Game) -> impl Iterator<Item = &'a str> {
        std::iter::once(g.id.as_str()).chain(self.versions.get(&g.id).into_iter().flatten().map(|v| v.id.as_str()))
    }

    fn last_played(&self, g: &Game) -> i64 {
        self.ids_of(g).fold(g.store_played, |last, id| {
            let session = self.user.sessions.get(id).and_then(|v| v.last()).map_or(0, |s| s.end);
            last.max(self.user.played.get(id).copied().unwrap_or(0)).max(session)
        })
    }

    fn playtime_min(&self, g: &Game) -> u64 {
        let secs: i64 = self.ids_of(g).map(|id| self.user.tracked_secs(id)).sum();
        g.playtime_min.max((secs / 60) as u64)
    }

    /// Games an assistant may see: not hidden; not installed ones only if asked and shown.
    fn visible(&self, with_owned: bool) -> impl Iterator<Item = &Game> {
        let owned = with_owned && self.settings.show_owned;
        self.games.iter().filter(move |g| !self.user.hidden.contains(&g.id) && (!g.not_installed || owned))
    }

    /// The game `key` names: its id, a version's id, or failing those its name.
    fn find(&self, key: &str) -> Option<&Game> {
        let key = key.trim();
        let primary = self.owner.get(key).map_or(key, String::as_str);
        if let Some(g) = self.games.iter().find(|g| g.id == primary) {
            return Some(g);
        }
        let q = search::normalize(key);
        if q.is_empty() { return None; }
        let mut hits: Vec<(i32, &Game)> = self.visible(true)
            .filter_map(|g| Some((search::score(&q, &search::normalize(&g.name))?, g)))
            .collect();
        hits.sort_by_key(|(s, g)| (std::cmp::Reverse(*s), g.not_installed));
        // An exact name, or one clearly best match; never a guess among equals.
        if let Some((_, g)) = hits.iter().find(|(_, g)| search::normalize(&g.name) == q) {
            return Some(g);
        }
        match hits.as_slice() {
            [(_, g)] => Some(g),
            [(a, g), (b, _), ..] if a - b >= 100 => Some(g),
            _ => None,
        }
    }

    fn summary(&self, g: &Game) -> Value {
        let entry = self.user.entry(&g.id);
        let last = self.last_played(g);
        json!({
            "id": g.id,
            "name": g.name,
            "store": g.source.label(),
            "installed": !g.not_installed,
            "favorite": self.user.favorites.contains(&g.id),
            "playtime_hours": hours(self.playtime_min(g)),
            "last_played": (last > 0).then(|| date(last)),
            "status": status_slug(entry.status),
            "rating": (entry.rating > 0).then_some(entry.rating),
        })
    }

    fn list_games(&self, args: &Value) -> Result<Value, String> {
        let text = |k: &str| args.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
        let store = match text("store") {
            Some(name) => Some(Source::ALL.into_iter()
                .find(|s| s.label().eq_ignore_ascii_case(name) || format!("{s:?}").eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("Unknown store `{name}`. Stores: {}.", Source::ALL.map(|s| s.label()).join(", ")))?),
            None => None,
        };
        let collection = match text("collection") {
            Some(name) => {
                let c = self.user.collections.iter().find(|c| c.name.eq_ignore_ascii_case(name))
                    .or_else(|| self.user.collections.iter().find(|c| search::normalize(&c.name) == search::normalize(name)));
                Some(c.ok_or_else(|| {
                    let names: Vec<&str> = self.user.collections.iter().map(|c| c.name.as_str()).collect();
                    if names.is_empty() { "The user has no collections.".to_string() }
                    else { format!("No collection named `{name}`. Collections: {}.", names.join(", ")) }
                })?)
            }
            None => None,
        };
        let status = match text("status") {
            Some(s) => Some(Status::ALL.into_iter().find(|st| status_slug(*st) == s)
                .ok_or_else(|| format!("Unknown status `{s}`. Statuses: {}.", STATUS_SLUGS.join(", ")))?),
            None => None,
        };
        let limit = args.get("limit").and_then(Value::as_u64).map_or(DEFAULT_LIMIT, |n| (n as usize).clamp(1, MAX_LIMIT));
        let q = text("query").map(search::normalize).unwrap_or_default();

        let mut hits: Vec<(i32, &Game)> = self.visible(flag("include_not_installed"))
            .filter(|g| store.is_none_or(|s| g.source == s))
            .filter(|g| !flag("favorites") || self.user.favorites.contains(&g.id))
            .filter(|g| collection.is_none_or(|c| c.games.contains(&g.id)))
            .filter(|g| status.is_none_or(|s| self.user.entry(&g.id).status == s))
            .filter(|g| !flag("never_played") || self.last_played(g) == 0)
            .filter_map(|g| Some((search::score(&q, &search::normalize(&g.name))?, g)))
            .collect();
        let sort = text("sort").unwrap_or("recent");
        hits.sort_by(|(sa, a), (sb, b)| {
            let by_sort = match sort {
                "name" => std::cmp::Ordering::Equal,
                "playtime" => self.playtime_min(b).cmp(&self.playtime_min(a)),
                "size" => b.size.cmp(&a.size),
                "rating" => self.user.entry(&b.id).rating.cmp(&self.user.entry(&a.id).rating),
                "installed" => b.installed.cmp(&a.installed),
                _ => self.last_played(b).cmp(&self.last_played(a)),
            };
            a.not_installed.cmp(&b.not_installed)
                .then(sb.cmp(sa))
                .then(by_sort)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let total = hits.len();
        let games: Vec<Value> = hits.into_iter().take(limit).map(|(_, g)| self.summary(g)).collect();
        Ok(json!({ "total": total, "shown": games.len(), "games": games }))
    }

    fn get_game(&self, args: &Value) -> Result<Value, String> {
        let g = self.game_arg(args)?;
        let mut v = self.summary(g);
        let o = v.as_object_mut().expect("an object");
        let entry = self.user.entry(&g.id);
        if let Some(m) = self.metas.get(&g.id) {
            let some = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
            o.insert("description".into(), json!(some(&m.description)));
            o.insert("developers".into(), json!(m.developers));
            o.insert("publishers".into(), json!(m.publishers));
            o.insert("release".into(), json!(some(&m.release)));
            o.insert("genres".into(), json!(m.genres));
            o.insert("metacritic".into(), json!(m.metacritic));
        }
        o.insert("notes".into(), json!((!entry.notes.trim().is_empty()).then(|| entry.notes.trim().to_string())));
        let collections: Vec<&str> = self.user.collections.iter().filter(|c| c.games.contains(&g.id)).map(|c| c.name.as_str()).collect();
        o.insert("collections".into(), json!(collections));
        o.insert("size_gb".into(), json!((g.size > 0).then(|| gb(g.size))));
        o.insert("install_dir".into(), json!(g.dir.as_ref().map(|d| d.display().to_string())));
        o.insert("installed_on".into(), json!((g.installed > 0).then(|| date(g.installed))));
        o.insert("steam_appid".into(), json!(g.steam_appid));
        o.insert("update_pending".into(), json!(g.update_pending));
        if let Some(rom) = &g.rom {
            o.insert("console".into(), json!(g.source.label()));
            o.insert("game_code".into(), json!((!rom.code.is_empty()).then(|| rom.code.clone())));
        }
        let launches: u32 = self.ids_of(g).filter_map(|id| self.user.launches.get(id)).sum();
        o.insert("launches_from_arach".into(), json!(launches));
        let mut sessions: Vec<&store::Session> = self.ids_of(g).flat_map(|id| self.user.sessions.get(id).into_iter().flatten()).collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.start));
        let week = store::now() - 7 * 86_400;
        o.insert("sessions".into(), json!({
            "count": sessions.len(),
            "hours_last_7_days": hours((sessions.iter().filter(|s| s.end >= week).map(|s| s.secs()).sum::<i64>() / 60) as u64),
            "recent": sessions.iter().take(5).map(|s| json!({
                "date": date(s.start),
                "minutes": s.secs() / 60,
            })).collect::<Vec<_>>(),
        }));
        let versions: Vec<Value> = self.versions.get(&g.id).into_iter().flatten()
            .map(|v| json!({ "id": v.id, "store": v.source.label() })).collect();
        o.insert("other_versions".into(), json!(versions));
        if self.user.hidden.contains(&g.id) {
            o.insert("hidden".into(), json!(true));
        }
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string());
        o.insert("images".into(), json!({ "cover": path(&g.art.cover), "background": path(&g.art.hero), "logo": path(&g.art.logo) }));
        let mut actions = actions_json(self.user.actions.get(&g.id).unwrap_or(&self.settings.actions));
        actions["own"] = json!(self.user.actions.contains_key(&g.id));
        o.insert("launch_actions".into(), actions);
        Ok(v)
    }

    /// The game an `id` argument names.
    fn game_arg(&self, args: &Value) -> Result<&Game, String> {
        let key = args.get("id").and_then(Value::as_str).ok_or("`id` is required.")?;
        self.find(key).ok_or_else(|| format!("No game matches `{key}`. Use list_games to find its id."))
    }

    fn launch_actions(&self, args: &Value) -> Result<Value, String> {
        let mut out = match args.get("id").and_then(Value::as_str) {
            Some(_) => {
                let g = self.game_arg(args)?;
                let own = self.user.actions.get(&g.id);
                json!({
                    "scope": "game",
                    "id": g.id,
                    "name": g.name,
                    "own": own.is_some(),
                    "actions": actions_json(own.unwrap_or(&self.settings.actions)),
                })
            }
            None => {
                let own: Vec<Value> = self.games.iter().filter(|g| self.user.actions.contains_key(&g.id))
                    .map(|g| json!({ "id": g.id, "name": g.name })).collect();
                json!({ "scope": "every game", "actions": actions_json(&self.settings.actions), "games_with_their_own": own })
            }
        };
        out["available"] = json!({
            "power_plans": system::power_plans().iter().map(|pl| pl.name).collect::<Vec<_>>(),
            "resolutions": system::resolutions().iter().map(|m| format!("{}x{}", m.0.0, m.0.1)).collect::<Vec<_>>(),
        });
        let m = &self.settings.mcp;
        out["assistant_may_change"] = json!({ "discord_power_resolution": m.edit_actions, "scripts": m.edit_scripts });
        Ok(out)
    }

    fn overview(&self) -> Value {
        let installed: Vec<&Game> = self.visible(false).collect();
        let mut by_store: BTreeMap<Source, usize> = BTreeMap::new();
        for g in &installed { *by_store.entry(g.source).or_default() += 1; }
        let mut stores: Vec<(Source, usize)> = by_store.into_iter().collect();
        stores.sort_by_key(|(s, n)| (std::cmp::Reverse(*n), *s));
        let not_installed = self.visible(true).filter(|g| g.not_installed).count();

        let mut played: Vec<&&Game> = installed.iter().filter(|g| self.playtime_min(g) > 0).collect();
        played.sort_by_key(|g| std::cmp::Reverse(self.playtime_min(g)));
        let mut recent: Vec<&&Game> = installed.iter().filter(|g| self.last_played(g) > 0).collect();
        recent.sort_by_key(|g| std::cmp::Reverse(self.last_played(g)));
        let brief = |g: &Game| json!({ "id": g.id, "name": g.name, "playtime_hours": hours(self.playtime_min(g)) });

        let mut statuses: BTreeMap<&str, usize> = BTreeMap::new();
        for g in self.visible(true) {
            let s = self.user.entry(&g.id).status;
            if s != Status::NotStarted { *statuses.entry(status_slug(s)).or_default() += 1; }
        }
        let favorites: Vec<Value> = self.visible(true).filter(|g| self.user.favorites.contains(&g.id))
            .map(|g| json!({ "id": g.id, "name": g.name })).collect();
        json!({
            "installed_games": installed.len(),
            "owned_not_installed": not_installed,
            "hidden_games": self.user.hidden.len(),
            "stores": stores.iter().map(|(s, n)| json!({ "store": s.label(), "games": n })).collect::<Vec<_>>(),
            "total_playtime_hours": hours(installed.iter().map(|g| self.playtime_min(g)).sum()),
            "total_size_gb": gb(installed.iter().map(|g| g.size).sum()),
            "most_played": played.iter().take(10).map(|g| brief(g)).collect::<Vec<_>>(),
            "recently_played": recent.iter().take(10).map(|g| {
                let mut b = brief(g);
                b["last_played"] = json!(date(self.last_played(g)));
                b
            }).collect::<Vec<_>>(),
            "favorites": favorites,
            "collections": self.user.collections.iter().map(|c| json!({ "name": c.name, "games": c.games.len() })).collect::<Vec<_>>(),
            "statuses": statuses,
        })
    }
}

fn launch_game(lib: &Library, args: &Value, settings: &Settings) -> Result<Value, String> {
    if !settings.mcp.allow_launch {
        return Err(off("launch games"));
    }
    let g = lib.game_arg(args)?;
    if g.not_installed {
        return Err(format!("{} is owned on {} but not installed. The user can install it from Arach.", g.name, g.source.label()));
    }
    let message = bridge::send(&Request::Launch { id: g.id.clone() })?;
    Ok(json!({ "ok": true, "id": g.id, "name": g.name, "message": message }))
}

/// Why a change was refused: the user turned it off.
fn off(what: &str) -> String {
    format!("The user doesn't allow assistants to {what}. They can allow it in Arach › Paramètres › Assistants IA.")
}

fn edit_game(lib: &Library, args: &Value, settings: &Settings) -> Result<Value, String> {
    if !settings.mcp.edit_games {
        return Err(off("edit games"));
    }
    let g = lib.game_arg(args)?;
    let patch = game_patch(args)?;
    let message = bridge::send(&Request::EditGame { id: g.id.clone(), patch })?;
    Ok(json!({ "ok": true, "id": g.id, "message": message }))
}

/// The changes `edit_game` was given, checked.
fn game_patch(args: &Value) -> Result<GamePatch, String> {
    let obj = args.as_object().ok_or("Arguments must be an object.")?;
    let mut p = GamePatch::default();
    let text = |k: &str, v: &Value| -> Result<Option<String>, String> {
        match v {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s.trim().to_string())),
            _ => Err(format!("`{k}` must be a string or null.")),
        }
    };
    let list = |k: &str, v: &Value| -> Result<Option<Vec<String>>, String> {
        match v {
            Value::Null => Ok(None),
            Value::Array(a) => a.iter()
                .map(|x| x.as_str().map(|s| s.trim().to_string()).ok_or(format!("`{k}` must list strings.")))
                .filter(|r| r.as_ref().map_or(true, |s| !s.is_empty()))
                .collect::<Result<Vec<_>, _>>().map(Some),
            _ => Err(format!("`{k}` must be a list of strings or null.")),
        }
    };
    let names = |k: &str, v: &Value| -> Result<Vec<String>, String> {
        list(k, v)?.ok_or(format!("`{k}` must be a list of names."))
    };
    for (k, v) in obj {
        match k.as_str() {
            "id" => {}
            // An empty name would leave the game without one: back to the store's.
            "name" => p.name = Some(text(k, v)?.filter(|s| !s.is_empty())),
            "description" => p.description = Some(text(k, v)?),
            "release" => p.release = Some(text(k, v)?),
            "developers" => p.developers = Some(list(k, v)?),
            "publishers" => p.publishers = Some(list(k, v)?),
            "genres" => p.genres = Some(list(k, v)?),
            "status" => {
                let s = v.as_str().unwrap_or_default();
                p.status = Some(Status::ALL.into_iter().find(|st| status_slug(*st) == s)
                    .ok_or_else(|| format!("Unknown status `{s}`. Statuses: {}.", STATUS_SLUGS.join(", ")))?);
            }
            "rating" => p.rating = Some(v.as_u64().filter(|r| *r <= 5).ok_or("`rating` must be 0 to 5 (0 clears it).")? as u8),
            "notes" => p.notes = Some(v.as_str().ok_or("`notes` must be a string.")?.to_string()),
            "favorite" => p.favorite = Some(v.as_bool().ok_or("`favorite` must be true or false.")?),
            "add_to_collections" => p.add_to = names(k, v)?,
            "remove_from_collections" => p.remove_from = names(k, v)?,
            other => return Err(format!("Unknown field `{other}`.")),
        }
    }
    if p == GamePatch::default() {
        return Err("Nothing to change: give at least one field besides `id`.".into());
    }
    Ok(p)
}

fn art_arg(args: &Value) -> Result<ArtKind, String> {
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or_default();
    ArtKind::parse(kind).ok_or_else(|| format!("`kind` must be one of {}.", ARTS.join(", ")))
}

fn find_images(lib: &Library, args: &Value, settings: &Settings) -> Result<Value, String> {
    if !settings.online {
        return Err("Online lookups are turned off in Arach (Paramètres › En ligne).".into());
    }
    let g = lib.game_arg(args)?;
    let kind = art_arg(args)?;
    let query = args.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()).unwrap_or(&g.name);
    let found = picker::search(query, kind.picker(), g.steam_appid, &settings.sgdb_key);
    let images: Vec<Value> = found.iter().map(|c| json!({ "url": c.url, "source": c.label })).collect();
    let note = if images.is_empty() { "Nothing found: try another `query`, or ask the user for a file." } else { "Pass a url to set_image." };
    Ok(json!({ "id": g.id, "kind": ARTS[kind as usize], "images": images, "note": note }))
}

fn set_image(lib: &Library, args: &Value, settings: &Settings) -> Result<Value, String> {
    if !settings.mcp.edit_games {
        return Err(off("edit games"));
    }
    let g = lib.game_arg(args)?;
    let kind = art_arg(args)?;
    let url = args.get("url").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let file = args.get("file").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let reset = args.get("reset").and_then(Value::as_bool).unwrap_or(false);
    let path = match (url, file, reset) {
        (Some(url), None, false) => {
            if !url.starts_with("https://") {
                return Err("`url` must be an https link.".into());
            }
            let c = picker::Candidate { thumb: PathBuf::new(), url: url.to_string(), label: String::new() };
            let path = picker::choose(&c, &g.id, kind.picker()).ok_or("Couldn't download a PNG or JPEG from that link.")?;
            if image::image_dimensions(&path).is_err() {
                let _ = std::fs::remove_file(&path);
                return Err("That link isn't a PNG or JPEG Arach can show.".into());
            }
            Some(path)
        }
        (None, Some(file), false) => {
            Some(picker::import(Path::new(file), &g.id, kind.picker()).ok_or("That file can't be read as a PNG or JPEG.")?)
        }
        (None, None, true) => None,
        _ => return Err("Give exactly one of `url`, `file` or `reset`.".into()),
    };
    match bridge::send(&Request::SetArt { id: g.id.clone(), kind, path: path.clone() }) {
        Ok(message) => Ok(json!({ "ok": true, "id": g.id, "message": message })),
        Err(e) => {
            if let Some(p) = path { let _ = std::fs::remove_file(p); }
            Err(e)
        }
    }
}

fn set_launch_actions(lib: &Library, args: &Value, settings: &Settings) -> Result<Value, String> {
    let game = match args.get("id").and_then(Value::as_str) {
        Some(_) => Some(lib.game_arg(args)?),
        None => None,
    };
    let patch = actions_patch(args, game.is_some(), system::power_plans(), system::resolutions())?;
    if patch.touches_scripts() && !settings.mcp.edit_scripts {
        return Err(off("change the scripts run before and after games, since they can run any command"));
    }
    if (patch.touches_system() || patch.own.is_some()) && !settings.mcp.edit_actions {
        return Err(off("change launch actions"));
    }
    let id = game.map(|g| g.id.clone());
    let message = bridge::send(&Request::SetActions { id: id.clone(), patch })?;
    Ok(json!({ "ok": true, "id": id, "message": message }))
}

/// The changes `set_launch_actions` was given, checked against this PC's plans and screen modes.
fn actions_patch(args: &Value, for_game: bool, plans: &[PowerPlan], modes: &[((u32, u32), &str)]) -> Result<ActionsPatch, String> {
    let obj = args.as_object().ok_or("Arguments must be an object.")?;
    let mut p = ActionsPatch::default();
    for (k, v) in obj {
        match k.as_str() {
            "id" => {}
            "use_global_actions" => {
                if !for_game { return Err("`use_global_actions` only applies with an `id`.".into()); }
                if v.as_bool().ok_or("`use_global_actions` must be true or false.")? { p.own = Some(false); }
            }
            "close_discord" => p.close_discord = Some(v.as_bool().ok_or("`close_discord` must be true or false.")?),
            "power_plan" => p.power_plan = Some(match v {
                Value::Null => String::new(),
                Value::String(s) if s.trim().is_empty() || s.eq_ignore_ascii_case("none") => String::new(),
                Value::String(s) => plans.iter()
                    .find(|pl| pl.name.eq_ignore_ascii_case(s.trim()) || pl.name.to_lowercase() == s.trim().to_lowercase() || pl.guid.eq_ignore_ascii_case(s.trim()))
                    .map(|pl| pl.guid.clone())
                    .ok_or_else(|| format!("No power plan `{s}` on this PC. Plans: {}.", plans.iter().map(|pl| pl.name).collect::<Vec<_>>().join(", ")))?,
                _ => return Err("`power_plan` must be a string or null.".into()),
            }),
            "resolution" => p.resolution = Some(match v {
                Value::Null => None,
                Value::String(s) if s.trim().is_empty() || s.eq_ignore_ascii_case("none") => None,
                Value::String(s) => {
                    let size = parse_resolution(s).ok_or_else(|| format!("`{s}` isn't a resolution like 1920x1080."))?;
                    if !modes.iter().any(|m| m.0 == size) {
                        let all: Vec<String> = modes.iter().map(|m| format!("{}x{}", m.0.0, m.0.1)).collect();
                        return Err(format!("The main screen doesn't offer {}x{}. It offers: {}.", size.0, size.1, all.join(", ")));
                    }
                    Some(size)
                }
                _ => return Err("`resolution` must be a string or null.".into()),
            }),
            "script_before" => p.before = Some(v.as_str().ok_or("`script_before` must be a string.")?.trim().to_string()),
            "script_after" => p.after = Some(v.as_str().ok_or("`script_after` must be a string.")?.trim().to_string()),
            other => return Err(format!("Unknown field `{other}`.")),
        }
    }
    if !p.touches_scripts() && !p.touches_system() && p.own != Some(false) {
        return Err("Nothing to change.".into());
    }
    if for_game && p.own.is_none() {
        p.own = Some(true);
    }
    Ok(p)
}

/// `1920x1080`, `1920 × 1080`, `1920*1080`.
fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X', '×', '*'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn actions_json(a: &Actions) -> Value {
    let text = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
    let plan = (!a.power_plan.is_empty()).then(|| {
        system::power_plans().iter().find(|pl| pl.guid == a.power_plan).map_or(a.power_plan.clone(), |pl| pl.name.to_string())
    });
    json!({
        "close_discord": a.close_discord,
        "power_plan": plan,
        "resolution": a.resolution.map(|(w, h)| format!("{w}x{h}")),
        "script_before": text(&a.before),
        "script_after": text(&a.after),
    })
}

fn gb(bytes: u64) -> f64 {
    (bytes as f64 / 1e9 * 10.0).round() / 10.0
}

fn hours(min: u64) -> f64 {
    (min as f64 / 60.0 * 10.0).round() / 10.0
}

/// `2026-11-13`, local time.
fn date(ts: i64) -> String {
    let t = ts + store::utc_offset();
    let (y, m, d) = extras::saves::civil(t.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Collection, Entry, Session};

    fn game(id: &str, name: &str, source: Source) -> Game {
        Game { id: id.into(), name: name.into(), source, ..Default::default() }
    }

    fn library() -> Library {
        let games = vec![
            game("steam:1", "Cyberpunk 2077", Source::Steam),
            game("steam:2", "Hollow Knight", Source::Steam),
            game("epic:3", "Hades", Source::Epic),
            game("gog:4", "The Witcher 3: Wild Hunt", Source::Gog),
            Game { not_installed: true, ..game("steam:5", "Celeste", Source::Steam) },
            game("steam:6", "Secret Game", Source::Steam),
        ];
        let mut user = UserData::default();
        user.hidden.insert("steam:6".into());
        user.favorites.insert("epic:3".into());
        user.played.insert("steam:2".into(), 2_000_000_000);
        user.played.insert("epic:3".into(), 1_900_000_000);
        user.sessions.insert("epic:3".into(), vec![Session { start: 1_000, end: 1_000 + 3 * 3600 }]);
        user.entries.insert("gog:4".into(), Entry { status: Status::Finished, rating: 5, notes: "Best".into() });
        user.collections.push(Collection { id: 1, name: "Rogue".into(), games: vec!["epic:3".into()] });
        Library::from_parts(&games, user, &Cache::default(), Settings::default())
    }

    fn names(v: &Value) -> Vec<String> {
        v["games"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn lists_by_recency_without_hidden_or_uninstalled() {
        let lib = library();
        let v = lib.list_games(&json!({})).unwrap();
        assert_eq!(names(&v), ["Hollow Knight", "Hades", "Cyberpunk 2077", "The Witcher 3: Wild Hunt"]);
        let v = lib.list_games(&json!({ "include_not_installed": true })).unwrap();
        assert_eq!(v["total"], 5);
        assert_eq!(names(&v).last().unwrap(), "Celeste");
    }

    #[test]
    fn filters_and_searches() {
        let lib = library();
        assert_eq!(names(&lib.list_games(&json!({ "query": "tw3" })).unwrap()), ["The Witcher 3: Wild Hunt"]);
        assert_eq!(names(&lib.list_games(&json!({ "query": "zzz" })).unwrap()), Vec::<String>::new());
        assert_eq!(names(&lib.list_games(&json!({ "query": "witcher" })).unwrap()), ["The Witcher 3: Wild Hunt"]);
        assert_eq!(names(&lib.list_games(&json!({ "store": "epic games" })).unwrap()), ["Hades"]);
        assert_eq!(names(&lib.list_games(&json!({ "favorites": true })).unwrap()), ["Hades"]);
        assert_eq!(names(&lib.list_games(&json!({ "collection": "rogue" })).unwrap()), ["Hades"]);
        assert_eq!(names(&lib.list_games(&json!({ "status": "finished" })).unwrap()), ["The Witcher 3: Wild Hunt"]);
        assert_eq!(names(&lib.list_games(&json!({ "sort": "playtime", "limit": 1 })).unwrap()), ["Hades"]);
        assert!(lib.list_games(&json!({ "store": "Nope" })).is_err());
        assert!(lib.list_games(&json!({ "collection": "Nope" })).is_err());
    }

    #[test]
    fn finds_a_game_by_id_or_name() {
        let lib = library();
        let v = lib.get_game(&json!({ "id": "gog:4" })).unwrap();
        assert_eq!(v["rating"], 5);
        assert_eq!(v["notes"], "Best");
        assert_eq!(v["status"], "finished");
        let v = lib.get_game(&json!({ "id": "hades" })).unwrap();
        assert_eq!(v["id"], "epic:3");
        assert_eq!(v["playtime_hours"], 3.0);
        assert_eq!(v["collections"], json!(["Rogue"]));
        assert!(lib.get_game(&json!({ "id": "zzz" })).is_err());
    }

    #[test]
    fn overview_counts_the_library() {
        let v = library().overview();
        assert_eq!(v["installed_games"], 4);
        assert_eq!(v["owned_not_installed"], 1);
        assert_eq!(v["stores"][0]["store"], "Steam");
        assert_eq!(v["stores"][0]["games"], 2);
        assert_eq!(v["most_played"][0]["name"], "Hades");
        assert_eq!(v["statuses"]["finished"], 1);
    }

    #[test]
    fn checks_game_edits() {
        let p = game_patch(&json!({ "id": "x", "name": "", "genres": ["RPG", " "], "rating": 4, "status": "playing",
            "description": null, "add_to_collections": ["À finir"] })).unwrap();
        assert_eq!(p.name, Some(None));
        assert_eq!(p.description, Some(None));
        assert_eq!(p.genres, Some(Some(vec!["RPG".to_string()])));
        assert_eq!(p.rating, Some(4));
        assert_eq!(p.status, Some(Status::Playing));
        assert_eq!(p.add_to, ["À finir"]);
        assert!(game_patch(&json!({ "id": "x" })).is_err());
        assert!(game_patch(&json!({ "id": "x", "rating": 6 })).is_err());
        assert!(game_patch(&json!({ "id": "x", "status": "done" })).is_err());
        assert!(game_patch(&json!({ "id": "x", "colour": "red" })).is_err());
    }

    #[test]
    fn checks_launch_actions() {
        let plans = [PowerPlan { guid: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(), name: "Performances élevées" }];
        let modes = [((1920, 1080), "1920 × 1080"), ((1280, 720), "1280 × 720")];
        let p = actions_patch(&json!({ "power_plan": "performances élevées", "resolution": "1280 x 720", "close_discord": true }),
            false, &plans, &modes).unwrap();
        assert_eq!(p.power_plan.as_deref(), Some("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"));
        assert_eq!(p.resolution, Some(Some((1280, 720))));
        assert_eq!(p.own, None);
        assert!(!p.touches_scripts());
        // One game: its own actions, unless sent back to everyone's.
        let p = actions_patch(&json!({ "id": "a", "resolution": null }), true, &plans, &modes).unwrap();
        assert_eq!((p.own, p.resolution), (Some(true), Some(None)));
        let p = actions_patch(&json!({ "id": "a", "use_global_actions": true }), true, &plans, &modes).unwrap();
        assert_eq!(p.own, Some(false));
        assert!(actions_patch(&json!({ "script_before": "x.bat" }), false, &plans, &modes).unwrap().touches_scripts());
        assert!(actions_patch(&json!({ "resolution": "800x600" }), false, &plans, &modes).is_err());
        assert!(actions_patch(&json!({ "power_plan": "Turbo" }), false, &plans, &modes).is_err());
        assert!(actions_patch(&json!({ "use_global_actions": true }), false, &plans, &modes).is_err());
        assert!(actions_patch(&json!({}), false, &plans, &modes).is_err());
    }

    #[test]
    fn refuses_what_the_user_turned_off() {
        let lib = library();
        let mut s = Settings::default();
        s.mcp.edit_games = false;
        assert!(edit_game(&lib, &json!({ "id": "steam:1", "rating": 3 }), &s).unwrap_err().contains("doesn't allow"));
        assert!(set_image(&lib, &json!({ "id": "steam:1", "kind": "cover", "reset": true }), &s).unwrap_err().contains("doesn't allow"));
        // Scripts are off unless the user turns them on.
        let s = Settings::default();
        assert!(set_launch_actions(&lib, &json!({ "script_after": "x.bat" }), &s).unwrap_err().contains("doesn't allow"));
        let v = lib.get_game(&json!({ "id": "steam:1" })).unwrap();
        assert_eq!(v["launch_actions"]["own"], false);
    }

    #[test]
    fn launching_respects_the_setting_and_installs() {
        let lib = library();
        let mut off = Settings::default();
        off.mcp.allow_launch = false;
        assert!(launch_game(&lib, &json!({ "id": "steam:1" }), &off).unwrap_err().contains("doesn't allow"));
        assert!(launch_game(&lib, &json!({ "id": "steam:5" }), &Settings::default()).unwrap_err().contains("not installed"));
    }
}
