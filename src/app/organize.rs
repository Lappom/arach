//! Shaping the library by hand: one game found by two stores shown once (or kept apart when
//! that guess is wrong), names and facts corrected, pictures chosen, filters combined, and
//! several games acted on at once.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rune::{FileDialogOptions, TextEdit, Tx};

use super::{Confirm, Overlay, Page, State};
use crate::game::{Art, Game, norm_dir};
use crate::meta::picker::{self, Candidate, Kind};
use crate::meta::{Cache, Meta};
use crate::store::{self, Edit, Status, UserData};

// ---------------------------------------------------------------- versions

/// What the library shows after merging: one game per entry, the others as its versions.
pub(super) struct Grouped {
    pub games: Vec<Game>,
    /// Primary id → the other copies, best first.
    pub versions: HashMap<String, Vec<Game>>,
    /// Version id → its primary.
    pub owner: HashMap<String, String>,
    /// Primary id → what to show about it: the cache's (or a version's), edits applied.
    pub metas: HashMap<String, Meta>,
}

/// Games sharing an install folder (an EA game bought on Steam, a GOG game also found in a
/// watched folder) become one, the first store in [`crate::game::Source`] order leading; the
/// user's merges and splits override the guess.
pub(super) fn group(all: Vec<Game>, user: &UserData, cache: &Cache) -> Grouped {
    let ids: HashSet<&str> = all.iter().map(|g| g.id.as_str()).collect();
    // The first game claiming each folder, in store order.
    let mut leader_of_dir: HashMap<String, (crate::game::Source, String)> = HashMap::new();
    for g in &all {
        if user.split.contains(&g.id) || g.not_installed || user.merged.contains_key(&g.id) { continue; }
        let Some(dir) = &g.dir else { continue };
        let key = norm_dir(dir);
        let e = leader_of_dir.entry(key).or_insert((g.source, g.id.clone()));
        if (g.source, &g.id) < (e.0, &e.1) { *e = (g.source, g.id.clone()); }
    }
    let manual = |id: &str| -> Option<String> {
        let p = user.primary(id);
        (p != id && ids.contains(p)).then(|| p.to_string())
    };
    let primary_of = |g: &Game| -> String {
        if let Some(p) = manual(&g.id) { return p; }
        if !user.split.contains(&g.id) && !g.not_installed
            && let Some(dir) = &g.dir
            && let Some((_, leader)) = leader_of_dir.get(&norm_dir(dir))
            && leader != &g.id
        {
            return manual(leader).unwrap_or_else(|| leader.clone());
        }
        g.id.clone()
    };
    let mut owner = HashMap::new();
    let mut versions: HashMap<String, Vec<Game>> = HashMap::new();
    let mut games = Vec::new();
    let primaries: Vec<String> = all.iter().map(primary_of).collect();
    for (g, p) in all.into_iter().zip(primaries) {
        if p == g.id {
            games.push(g);
        } else {
            owner.insert(g.id.clone(), p.clone());
            versions.entry(p).or_default().push(g);
        }
    }
    for g in &mut games {
        if let Some(art) = user.art.get(&g.id) {
            g.art = art.clone().or(&g.art);
        }
        if let Some(vs) = versions.get(&g.id) {
            for v in vs {
                g.playtime_min += v.playtime_min;
                g.store_played = g.store_played.max(v.store_played);
                g.art = g.art.clone().or(&v.art);
                g.steam_appid = g.steam_appid.or(v.steam_appid);
                g.size = g.size.max(v.size);
                g.not_installed &= v.not_installed;
            }
        }
    }
    let mut metas = HashMap::new();
    for g in &mut games {
        let mut m = cache.meta.get(&g.id).filter(|m| m.found()).cloned()
            .or_else(|| versions.get(&g.id)?.iter().find_map(|v| cache.meta.get(&v.id).filter(|m| m.found()).cloned()))
            .or_else(|| cache.meta.get(&g.id).cloned());
        if let Some(e) = user.edits.get(&g.id) {
            let m = m.get_or_insert_with(Meta::default);
            if let Some(v) = &e.description { m.description = v.clone(); }
            if let Some(v) = &e.developers { m.developers = v.clone(); }
            if let Some(v) = &e.publishers { m.publishers = v.clone(); }
            if let Some(v) = &e.release { m.release = v.clone(); }
            if let Some(v) = &e.genres { m.genres = v.clone(); }
            if let Some(v) = e.name.as_ref().filter(|n| !n.trim().is_empty()) { g.name = v.clone(); }
        }
        if let Some(m) = m { metas.insert(g.id.clone(), m); }
    }
    Grouped { games, versions, owner, metas }
}

impl State {
    /// The game shown for `id`: itself, or the one it's a version of.
    pub fn primary_id(&self, id: &str) -> String {
        self.owner.get(id).cloned().unwrap_or_else(|| id.to_string())
    }

    /// A game or one of its versions, as found on disk.
    pub fn any_game(&self, id: &str) -> Option<&Game> {
        self.game(id).or_else(|| self.versions.values().flatten().find(|g| g.id == id))
    }

    /// Every id the game stands for: itself and its versions.
    pub fn ids_of(&self, id: &str) -> Vec<String> {
        let mut v = vec![id.to_string()];
        v.extend(self.versions.get(id).into_iter().flatten().map(|g| g.id.clone()));
        v
    }

    /// Makes the selected games one: the first keeps its name, page and lists; the others
    /// become its versions.
    pub fn merge(&mut self, ids: &[String], tx: &mut Tx) {
        let Some((first, rest)) = ids.split_first() else { return };
        let primary = self.primary_id(first);
        let name = self.game(&primary).map(|g| g.name.clone()).unwrap_or_default();
        let mut n = 0;
        for id in rest {
            let id = self.primary_id(id);
            if id == primary { continue; }
            n += 1;
            self.user.merged.insert(id.clone(), primary.clone());
            self.user.split.remove(&id);
            // What the user said about the copy carries over.
            if self.user.favorites.remove(&id) { self.user.favorites.insert(primary.clone()); }
            for c in &mut self.user.collections {
                if let Some(i) = c.games.iter().position(|g| *g == id) {
                    c.games.remove(i);
                    if !c.games.contains(&primary) { c.games.push(primary.clone()); }
                }
            }
            if let Some(e) = self.user.entries.remove(&id)
                && self.user.entry(&primary).is_empty()
            {
                self.user.entries.insert(primary.clone(), e);
            }
            if let Some(list) = self.user.sessions.remove(&id) {
                let mine = self.user.sessions.entry(primary.clone()).or_default();
                mine.extend(list);
                mine.sort_by_key(|s| s.start);
            }
        }
        if n == 0 { return; }
        self.save_user();
        self.end_selection();
        self.rebuild();
        self.notify(format!("{} version{} fusionnée{} dans {name}", n, if n > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" }), false, tx);
    }

    /// Takes a version out of its game: it shows on its own again, and stays apart at the next scan.
    pub fn split_version(&mut self, id: &str, tx: &mut Tx) {
        let name = self.any_game(id).map(|g| format!("{} ({})", g.name, g.source.label())).unwrap_or_default();
        self.user.merged.remove(id);
        self.user.merged.retain(|_, p| p != id);
        self.user.split.insert(id.to_string());
        self.save_user();
        self.rebuild();
        self.notify(format!("{name} séparé"), false, tx);
    }

    /// Plays one version rather than the main one.
    pub fn launch_version(&mut self, id: &str, tx: &mut Tx) {
        self.launch(id, tx);
    }

    fn save_user(&self) {
        store::save_user(self.user.clone());
    }
}

// ---------------------------------------------------------------- filters

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Played {
    #[default]
    Any,
    /// Never launched, from Arach or its store.
    Never,
    /// Played once, but not for six months.
    Stale,
}

impl Played {
    pub fn label(self) -> &'static str {
        match self {
            Played::Any => "Peu importe",
            Played::Never => "Jamais lancé",
            Played::Stale => "Pas lancé depuis 6 mois",
        }
    }
}

/// Every condition must hold; within one kind, any choice will do (but every genre chosen).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    pub genres: Vec<String>,
    pub years: Vec<i32>,
    pub statuses: Vec<Status>,
    pub collections: Vec<u64>,
    pub played: Played,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        *self == Filter::default()
    }

    pub fn count(&self) -> usize {
        self.genres.len() + self.years.len() + self.statuses.len() + self.collections.len() + usize::from(self.played != Played::Any)
    }
}

/// `4 nov. 2021`, `Nov 4, 2021`, `2021`: the year.
pub fn year(release: &str) -> Option<i32> {
    release
        .split(|c: char| !c.is_ascii_digit())
        .find(|t| t.len() == 4)
        .and_then(|t| t.parse().ok())
        .filter(|y| (1970..=2100).contains(y))
}

const SIX_MONTHS: i64 = 182 * 86_400;

impl State {
    pub fn passes_filter(&self, g: &Game) -> bool {
        let f = &self.filter;
        if f.is_empty() { return true; }
        let meta = self.meta(&g.id);
        if !f.genres.is_empty() {
            let genres = meta.map(|m| m.genres.as_slice()).unwrap_or(&[]);
            if !f.genres.iter().all(|want| genres.iter().any(|have| have.eq_ignore_ascii_case(want))) { return false; }
        }
        if !f.years.is_empty() && !meta.and_then(|m| year(&m.release)).is_some_and(|y| f.years.contains(&y)) {
            return false;
        }
        if !f.statuses.is_empty() && !f.statuses.contains(&self.user.entry(&g.id).status) {
            return false;
        }
        if !f.collections.is_empty()
            && !f.collections.iter().any(|c| self.collection(*c).is_some_and(|c| c.games.contains(&g.id)))
        {
            return false;
        }
        let last = self.last_played(g);
        match f.played {
            Played::Any => true,
            Played::Never => last == 0 && self.playtime_min(g) == 0,
            Played::Stale => last > 0 && store::now() - last > SIX_MONTHS,
        }
    }

    /// The genres in the library, most common first, for the filter panel.
    pub fn genre_choices(&self) -> Vec<(String, usize)> {
        let mut count: HashMap<String, (String, usize)> = HashMap::new();
        for g in self.games.iter().filter(|g| self.in_library(g)) {
            for genre in self.meta(&g.id).map(|m| m.genres.as_slice()).unwrap_or(&[]) {
                count.entry(genre.to_lowercase()).or_insert_with(|| (genre.clone(), 0)).1 += 1;
            }
        }
        let mut v: Vec<(String, usize)> = count.into_values().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }

    /// Release years in the library, newest first.
    pub fn year_choices(&self) -> Vec<i32> {
        let mut v: Vec<i32> = self.games.iter().filter(|g| self.in_library(g)).filter_map(|g| year(&self.meta(&g.id)?.release)).collect();
        v.sort_unstable_by(|a, b| b.cmp(a));
        v.dedup();
        v
    }

    pub fn change_filter(&mut self, tx: &mut Tx, f: impl FnOnce(&mut Filter)) {
        tx.instant();
        f(&mut self.filter);
        self.refilter();
    }
}

/// Puts `v` in the list or takes it out.
pub fn toggle<T: PartialEq>(list: &mut Vec<T>, v: T) {
    match list.iter().position(|x| *x == v) {
        Some(i) => { list.remove(i); }
        None => list.push(v),
    }
}

// ---------------------------------------------------------------- selection

impl State {
    /// Ctrl+click, or any click while selecting: the game goes in or out of the selection.
    pub fn toggle_selected(&mut self, id: &str, tx: &mut Tx) {
        tx.instant();
        self.selecting = true;
        toggle(&mut self.selected, id.to_string());
    }

    pub fn start_selection(&mut self, tx: &mut Tx) {
        tx.instant();
        self.selecting = true;
        self.overlay = Overlay::None;
    }

    pub fn select_all(&mut self, tx: &mut Tx) {
        tx.instant();
        self.selecting = true;
        let all: Vec<String> = self.visible.iter().map(|&i| self.games[i].id.clone()).collect();
        if all.iter().all(|id| self.selected.contains(id)) {
            self.selected.clear();
        } else {
            self.selected = all;
        }
    }

    pub fn end_selection(&mut self) {
        self.selecting = false;
        self.selected.clear();
        if matches!(self.overlay, Overlay::BulkCollections | Overlay::BulkStatus | Overlay::Confirm(Confirm::Merge(_))) {
            self.overlay = Overlay::None;
        }
    }

    pub fn is_selected(&self, id: &str) -> bool {
        self.selected.iter().any(|s| s == id)
    }

    /// Favourites: all of them if any isn't yet, else none.
    pub fn bulk_favorite(&mut self, tx: &mut Tx) {
        let ids = self.selected.clone();
        let add = ids.iter().any(|id| !self.is_favorite(id));
        for id in &ids {
            if add { self.user.favorites.insert(id.clone()); } else { self.user.favorites.remove(id); }
        }
        self.save_user();
        self.refilter();
        let n = ids.len();
        let (x, s) = if n > 1 { ("x", "s") } else { ("", "") };
        let what = if add { format!("ajouté{s} aux favoris") } else { format!("retiré{s} des favoris") };
        self.notify(format!("{n} jeu{x} {what}"), false, tx);
    }

    pub fn bulk_hide(&mut self, tx: &mut Tx) {
        let ids = self.selected.clone();
        for id in &ids {
            match self.game(id) {
                Some(g) if g.source == crate::game::Source::Custom => self.user.custom.retain(|c| c.id != *id),
                Some(_) => { self.user.hidden.insert(id.clone()); }
                None => {}
            }
        }
        self.save_user();
        self.end_selection();
        self.rebuild();
        let n = ids.len();
        self.notify(format!("{n} jeu{} masqué{} · Paramètres › Jeux masqués pour les afficher", if n > 1 { "x" } else { "" }, if n > 1 { "s" } else { "" }), false, tx);
    }

    /// Into the collection all of them; out of it when all were in already.
    pub fn bulk_collection(&mut self, cid: u64, tx: &mut Tx) {
        let ids = self.selected.clone();
        let Some(c) = self.user.collections.iter_mut().find(|c| c.id == cid) else { return };
        let all_in = ids.iter().all(|id| c.games.contains(id));
        if all_in {
            c.games.retain(|g| !ids.contains(g));
        } else {
            for id in &ids {
                if !c.games.contains(id) { c.games.push(id.clone()); }
            }
        }
        let name = c.name.clone();
        self.save_user();
        self.refilter();
        let n = ids.len();
        let what = if all_in { format!("retiré{} de", if n > 1 { "s" } else { "" }) } else { format!("ajouté{} à", if n > 1 { "s" } else { "" }) };
        self.notify(format!("{n} jeu{} {what} « {name} »", if n > 1 { "x" } else { "" }), false, tx);
    }

    pub fn bulk_status(&mut self, status: Status, tx: &mut Tx) {
        let ids = self.selected.clone();
        for id in &ids {
            self.user.edit_entry(id, |e| e.status = status);
        }
        self.save_user();
        self.refilter();
        self.notify(format!("Statut « {} » pour {} jeu{}", status.label(), ids.len(), if ids.len() > 1 { "x" } else { "" }), false, tx);
    }

    /// The selection, merged into its first game, after a confirmation.
    pub fn ask_merge(&mut self) {
        if self.selected.len() >= 2 {
            self.overlay = Overlay::Confirm(Confirm::Merge(self.selected.clone()));
        }
    }
}

// ---------------------------------------------------------------- editing facts

/// The edit dialog's fields, in order.
pub const EDIT_FIELDS: [&str; 6] = ["Nom", "Description", "Développeurs", "Éditeurs", "Date de sortie", "Genres"];

fn list(s: &str) -> Vec<String> {
    s.split([',', ';']).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
}

impl State {
    /// What the store says, before the user's corrections: the dialog compares with it.
    fn source_values(&self, id: &str) -> [String; 6] {
        let base = self.library.iter().chain(self.user.custom.iter()).find(|g| g.id == id);
        let meta = self.cache.meta.get(id).filter(|m| m.found())
            .or_else(|| self.versions.get(id)?.iter().find_map(|v| self.cache.meta.get(&v.id).filter(|m| m.found())))
            .or_else(|| self.cache.meta.get(id));
        let m = meta.cloned().unwrap_or_default();
        [
            base.map(|g| g.name.clone()).unwrap_or_default(),
            m.description,
            m.developers.join(", "),
            m.publishers.join(", "),
            m.release,
            m.genres.join(", "),
        ]
    }

    pub fn open_edit(&mut self, id: &str, tx: &mut Tx) {
        let id = self.primary_id(id);
        let src = self.source_values(&id);
        let e = self.user.edits.get(&id).cloned().unwrap_or_default();
        let values = [
            e.name.clone().unwrap_or_else(|| src[0].clone()),
            e.description.clone().unwrap_or_else(|| src[1].clone()),
            e.developers.as_ref().map(|v| v.join(", ")).unwrap_or_else(|| src[2].clone()),
            e.publishers.as_ref().map(|v| v.join(", ")).unwrap_or_else(|| src[3].clone()),
            e.release.clone().unwrap_or_else(|| src[4].clone()),
            e.genres.as_ref().map(|v| v.join(", ")).unwrap_or_else(|| src[5].clone()),
        ];
        self.edit_fields = values.iter().map(|v| TextEdit::new(v)).collect();
        self.overlay = Overlay::EditGame(id);
        tx.focus("edit-0", false);
    }

    /// The edit dialog open on `id`, for headless snapshots.
    pub fn scene_edit(&mut self, id: &str) {
        let src = self.source_values(id);
        self.edit_fields = src.iter().map(|v| TextEdit::new(v)).collect();
        self.scene_overlay(Page::Game(id.to_string()), Overlay::EditGame(id.to_string()));
    }

    /// Keeps only what differs from the store; a new name looks the game up again under it.
    pub fn save_edit(&mut self, tx: &mut Tx) {
        let Overlay::EditGame(id) = std::mem::take(&mut self.overlay) else { return };
        let src = self.source_values(&id);
        let typed: Vec<String> = self.edit_fields.iter().map(|f| f.text.trim().to_string()).collect();
        if typed.len() != 6 { return; }
        let differs = |i: usize| typed[i] != src[i].trim();
        let as_list = |i: usize| differs(i).then(|| list(&typed[i])).filter(|l| l.join(", ") != src[i]);
        let edit = Edit {
            name: (differs(0) && !typed[0].is_empty()).then(|| typed[0].clone()),
            description: differs(1).then(|| typed[1].clone()),
            developers: as_list(2),
            publishers: as_list(3),
            release: differs(4).then(|| typed[4].clone()),
            genres: as_list(5),
        };
        let renamed = self.user.edits.get(&id).and_then(|e| e.name.clone()) != edit.name;
        if edit.is_empty() { self.user.edits.remove(&id); } else { self.user.edits.insert(id.clone(), edit.clone()); }
        self.save_user();
        // A game the stores didn't recognise may be found under its right name.
        let unknown = !self.cache.meta.get(&id).is_some_and(Meta::found) || self.game(&id).is_some_and(|g| g.steam_appid.is_none());
        if renamed && unknown && self.game(&id).is_some_and(|g| g.rom.is_none()) {
            self.cache.meta.remove(&id);
            store::save_cache(self.cache.clone());
            self.rebuild();
            self.fetch_meta();
        } else {
            self.rebuild();
        }
        self.notify("Informations enregistrées".into(), false, tx);
    }

    /// Back to what the stores say.
    pub fn reset_edit(&mut self, tx: &mut Tx) {
        let Overlay::EditGame(id) = std::mem::take(&mut self.overlay) else { return };
        if self.user.edits.remove(&id).is_some() {
            self.save_user();
            self.rebuild();
            self.notify("Informations d'origine rétablies".into(), false, tx);
        }
    }

    /// The name lookups search for: the user's correction first.
    pub fn lookup_name(&self, id: &str, found: &str) -> String {
        self.user.edits.get(id).and_then(|e| e.name.clone()).unwrap_or_else(|| found.to_string())
    }
}

// ---------------------------------------------------------------- pictures

#[derive(Default)]
pub struct Picker {
    pub query: TextEdit,
    pub results: Vec<Candidate>,
    pub loading: bool,
    /// A search came back (empty results then mean nothing was found).
    pub done: bool,
    /// Bumped by each search, so a slow answer to an older one is dropped.
    seq: u64,
    /// The candidate being downloaded.
    pub choosing: Option<usize>,
}

impl State {
    pub fn open_picker(&mut self, id: &str, kind: Kind, tx: &mut Tx) {
        let id = self.primary_id(id);
        let name = self.game(&id).map(|g| g.name.clone()).unwrap_or_default();
        self.picker = Picker { query: TextEdit::new(&name), ..Picker::default() };
        self.overlay = Overlay::ArtPicker { id, kind };
        self.picker_search(tx);
    }

    pub fn picker_kind(&mut self, kind: Kind, tx: &mut Tx) {
        let Overlay::ArtPicker { id, .. } = &self.overlay else { return };
        self.overlay = Overlay::ArtPicker { id: id.clone(), kind };
        self.picker_search(tx);
    }

    pub fn picker_search(&mut self, tx: &mut Tx) {
        let Overlay::ArtPicker { id, kind } = &self.overlay else { return };
        if !self.settings.online {
            self.notify("Les recherches en ligne sont désactivées (Paramètres › En ligne)".into(), true, tx);
            return;
        }
        let (kind, query) = (*kind, self.picker.query.text.trim().to_string());
        let appid = self.game(id).and_then(|g| g.steam_appid);
        let key = self.settings.sgdb_key.clone();
        self.picker.seq += 1;
        self.picker.loading = true;
        self.picker.results.clear();
        let (seq, me) = (self.picker.seq, self.me.clone());
        std::thread::spawn(move || {
            let found = picker::search(&query, kind, appid, &key);
            me.update(move |s: &mut State, tx: &mut Tx| {
                if s.picker.seq != seq { return; }
                tx.instant();
                s.picker.loading = false;
                s.picker.done = true;
                s.picker.results = found;
            });
        });
    }

    pub fn picker_choose(&mut self, i: usize, tx: &mut Tx) {
        let Overlay::ArtPicker { id, kind } = self.overlay.clone() else { return };
        let Some(c) = self.picker.results.get(i).cloned() else { return };
        self.picker.choosing = Some(i);
        let me = self.me.clone();
        std::thread::spawn(move || {
            let path = picker::choose(&c, &id, kind);
            me.update(move |s: &mut State, tx: &mut Tx| {
                s.picker.choosing = None;
                match path {
                    Some(path) => {
                        s.overlay = Overlay::None;
                        s.set_art(&id, kind, Some(path), tx);
                    }
                    None => s.notify("Téléchargement impossible : essayez une autre image".into(), true, tx),
                }
            });
        });
        let _ = tx;
    }

    pub fn picker_file(&mut self, tx: &mut Tx) {
        let Overlay::ArtPicker { id, kind } = self.overlay.clone() else { return };
        let options = FileDialogOptions { title: format!("{} : choisir une image", kind.label()), ..Default::default() }
            .filter("Images", &["png", "jpg", "jpeg"]);
        tx.open_file(options, move |s: &mut State, picked, tx| match picked {
            Ok(Some(file)) => s.import_art(&id, kind, &file, tx),
            Ok(None) => {}
            Err(e) => s.notify(format!("Impossible d'ouvrir le sélecteur : {e}"), true, tx),
        });
    }

    /// A file dropped on the open dialog: the kind it shows.
    pub fn picker_drop(&mut self, file: &std::path::Path, tx: &mut Tx) {
        let Overlay::ArtPicker { id, kind } = self.overlay.clone() else { return };
        self.import_art(&id, kind, file, tx);
    }

    pub fn picker_reset(&mut self, tx: &mut Tx) {
        let Overlay::ArtPicker { id, kind } = self.overlay.clone() else { return };
        self.overlay = Overlay::None;
        self.set_art(&id, kind, None, tx);
    }

    fn import_art(&mut self, id: &str, kind: Kind, file: &std::path::Path, tx: &mut Tx) {
        match picker::import(file, id, kind) {
            Some(path) => {
                if matches!(self.overlay, Overlay::ArtPicker { .. }) { self.overlay = Overlay::None; }
                self.set_art(id, kind, Some(path), tx);
            }
            None => self.notify("Cette image ne peut pas être lue (PNG ou JPEG)".into(), true, tx),
        }
    }

    /// Sets (or with `None` drops) the user's picture of a kind.
    pub fn set_art(&mut self, id: &str, kind: Kind, path: Option<PathBuf>, tx: &mut Tx) {
        let art = self.user.art.entry(id.to_string()).or_default();
        let old = match kind {
            Kind::Cover => std::mem::replace(&mut art.cover, path.clone()),
            Kind::Hero => {
                // The blurred wash follows the hero the user picked.
                art.hero_blur = None;
                std::mem::replace(&mut art.hero, path.clone())
            }
            Kind::Logo => std::mem::replace(&mut art.logo, path.clone()),
        };
        if *art == Art::default() { self.user.art.remove(id); }
        if let Some(old) = old && old.starts_with(UserData::covers_dir()) { let _ = std::fs::remove_file(old); }
        self.save_user();
        self.rebuild();
        let what = kind.label();
        self.notify(if path.is_some() { format!("{what} mise à jour") } else { format!("{what} d'origine rétablie") }, false, tx);
    }

    /// Images dropped on a game's page: a tall one is its cover, a wide one its background.
    pub fn drop_art(&mut self, files: &[PathBuf], tx: &mut Tx) -> bool {
        let Page::Game(id) = &self.page else { return false };
        let id = id.clone();
        let Some(file) = files.iter().find(|f| crate::game::is_image(f)) else { return false };
        let kind = match image::image_dimensions(file) {
            Ok((w, h)) if h > w => Kind::Cover,
            Ok(_) => Kind::Hero,
            Err(_) => {
                self.notify("Cette image ne peut pas être lue (PNG ou JPEG)".into(), true, tx);
                return true;
            }
        };
        self.import_art(&id, kind, file, tx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Launch, Source};

    fn game(id: &str, source: Source, dir: &str) -> Game {
        Game { id: id.into(), name: id.into(), source, dir: Some(PathBuf::from(dir)), launch: Launch::default(), playtime_min: 10, ..Default::default() }
    }

    #[test]
    fn games_sharing_a_folder_become_one() {
        let all = vec![
            game("ea:1", Source::Ea, r"D:\Games\Jedi"),
            game("steam:2", Source::Steam, r"d:/games/jedi/"),
            game("gog:3", Source::Gog, r"D:\Games\Other"),
        ];
        let user = UserData::default();
        let g = group(all.clone(), &user, &Cache::default());
        assert_eq!(g.games.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["steam:2", "gog:3"]);
        assert_eq!(g.owner["ea:1"], "steam:2");
        assert_eq!(g.games[0].playtime_min, 20);

        let mut user = UserData::default();
        user.split.insert("ea:1".into());
        user.merged.insert("gog:3".into(), "steam:2".into());
        let g = group(all, &user, &Cache::default());
        assert_eq!(g.games.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["ea:1", "steam:2"]);
        assert_eq!(g.versions["steam:2"].len(), 1);
    }

    #[test]
    fn reads_years_and_counts_filters() {
        assert_eq!(year("4 nov. 2021"), Some(2021));
        assert_eq!(year("Nov 4, 2021"), Some(2021));
        assert_eq!(year("Bientôt"), None);
        let f = Filter { genres: vec!["RPG".into()], played: Played::Never, ..Default::default() };
        assert_eq!(f.count(), 2);
    }
}
