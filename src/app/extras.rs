//! The game page's extras: gathered in the background when the page opens, and the actions on
//! them (copy and restore saves, uninstall). Destructive ones take two presses: the first arms
//! the button, the second runs it.

use std::path::PathBuf;

use rune::Tx;

use super::State;
use crate::extras::{self, Arm, Extras, Lookup, achievements, hltb, saves};
use crate::{launch, store};

/// Screenshots shown before "Voir plus".
pub const SHOTS_STEP: usize = 8;

impl State {
    /// Starts gathering what `id`'s page shows. What was found last time stays up meanwhile.
    pub(super) fn open_extras(&mut self, id: &str) {
        let Some(g) = self.game(id).cloned() else { return };
        let (online, french) = (self.settings.online, self.settings.french);
        let e = self.extras.entry(g.id.clone()).or_default();
        e.armed = None;
        e.shots_shown = SHOTS_STEP;
        if !matches!(e.saves, Lookup::Found(_)) {
            e.saves = if g.not_installed || g.source.is_console() { Lookup::Missing(String::new()) } else { Lookup::Pending };
        }

        // HowLongToBeat: from the cache when fresh, else asked.
        let now = store::now();
        match self.hltb.fresh(&g.id, now) {
            Some(Some(h)) => e.hltb = Lookup::Found(h),
            Some(None) => e.hltb = Lookup::Missing("Pas trouvé sur HowLongToBeat".into()),
            None if !online => e.hltb = Lookup::Missing(String::new()),
            None => {
                e.hltb = Lookup::Pending;
                let (me, id, name) = (self.me.clone(), g.id.clone(), g.name.clone());
                std::thread::spawn(move || {
                    let r = hltb::lookup(&name);
                    me.update(move |s: &mut State, _: &mut Tx| {
                        let shown = match &r {
                            Ok(Some(h)) => Lookup::Found(h.clone()),
                            Ok(None) => Lookup::Missing("Pas trouvé sur HowLongToBeat".into()),
                            Err(_) => Lookup::Missing("HowLongToBeat ne répond pas".into()),
                        };
                        // A failed request isn't kept: the next visit asks again.
                        if let Ok(found) = r {
                            s.hltb.0.insert(id.clone(), (store::now(), found));
                            hltb::save_cache(&s.hltb);
                        }
                        s.extras.entry(id).or_default().hltb = shown;
                    });
                });
            }
        }

        // What's on disk, then the online fallbacks.
        let me = self.me.clone();
        std::thread::spawn(move || {
            let local = extras::local(&g, french);
            let id = g.id.clone();
            let need_public = local.achievements.is_none() && online && g.id.starts_with("steam:") && g.steam_appid.is_some();
            me.update({
                let id = id.clone();
                move |s: &mut State, _: &mut Tx| {
                    let e = s.extras.entry(id).or_default();
                    e.backups = local.backups;
                    e.shots = local.shots;
                    e.mods = local.mods;
                    e.uninstall = local.uninstall;
                    e.achievements = match local.achievements {
                        Some(a) => Lookup::Found(a),
                        None if need_public => Lookup::Pending,
                        None => Lookup::Missing(String::new()),
                    };
                }
            });
            if need_public {
                let steam = extras::steam_root();
                let appid = g.steam_appid.unwrap_or_default();
                let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(10)).user_agent("Arach/0.2 (game launcher)").build();
                let found = steam.as_deref().map(achievements::accounts).unwrap_or_default().into_iter().take(2)
                    .find_map(|acc| achievements::public(&agent, acc, appid, french));
                let id = id.clone();
                me.update(move |s: &mut State, _: &mut Tx| {
                    s.extras.entry(id).or_default().achievements = match found {
                        Some(a) => Lookup::Found(a),
                        None => Lookup::Missing(String::new()),
                    };
                });
            }
            if g.not_installed || g.source.is_console() { return; }
            let located = saves::locate(&g, extras::steam_root().as_deref(), online);
            me.update(move |s: &mut State, _: &mut Tx| {
                s.extras.entry(id).or_default().saves = match located {
                    Ok(Some(l)) => Lookup::Found(l),
                    Ok(None) => Lookup::Missing("Emplacement des sauvegardes inconnu de Ludusavi et de PCGamingWiki".into()),
                    Err(e) => Lookup::Missing(e),
                };
            });
        });
    }

    pub fn extras(&self, id: &str) -> Option<&Extras> {
        self.extras.get(id)
    }

    /// First press of a destructive button: it asks for a second one.
    pub fn arm(&mut self, id: &str, what: Arm, tx: &mut Tx) {
        tx.instant();
        let e = self.extras.entry(id.to_string()).or_default();
        e.armed = if e.armed.as_ref() == Some(&what) { None } else { Some(what) };
    }

    pub fn disarm(&mut self, id: &str, tx: &mut Tx) {
        tx.instant();
        if let Some(e) = self.extras.get_mut(id) { e.armed = None; }
    }

    pub fn more_shots(&mut self, id: &str, tx: &mut Tx) {
        tx.instant();
        if let Some(e) = self.extras.get_mut(id) { e.shots_shown += SHOTS_STEP * 2; }
    }

    /// Opens a file or folder with whatever Windows opens it with.
    pub fn open_path(&mut self, path: &std::path::Path, tx: &mut Tx) {
        if let Err(e) = launch::open_uri(&path.to_string_lossy()) { self.notify(e, true, tx); }
    }

    pub fn open_in_folder(&mut self, path: &std::path::Path, tx: &mut Tx) {
        if let Err(e) = launch::open_folder(path) { self.notify(e, true, tx); }
    }

    /// Copies the game's saves now.
    pub fn backup_saves(&mut self, id: &str, tx: &mut Tx) {
        let Some(e) = self.extras.get_mut(id) else { return };
        let Lookup::Found(located) = &e.saves else { return };
        if e.busy { return; }
        e.busy = true;
        let (paths, id, me) = (located.paths.clone(), id.to_string(), self.me.clone());
        self.notify("Copie des sauvegardes…".into(), false, tx);
        std::thread::spawn(move || {
            let r = saves::backup(&id, &paths, false);
            let list = saves::list(&id);
            me.update(move |s: &mut State, tx: &mut Tx| {
                let e = s.extras.entry(id).or_default();
                e.busy = false;
                e.backups = list;
                match r {
                    Ok(b) => s.notify(format!("Sauvegardes copiées · {}", extras::size_text(b.bytes)), false, tx),
                    Err(err) => s.notify(err, true, tx),
                }
            });
        });
    }

    /// Second press: puts a copy back (the current saves are copied first).
    pub fn restore_backup(&mut self, id: &str, dir: PathBuf, tx: &mut Tx) {
        if self.playing.contains_key(id) {
            self.notify("Quittez le jeu avant de restaurer ses sauvegardes".into(), true, tx);
            return;
        }
        let Some(e) = self.extras.get_mut(id) else { return };
        let Some(b) = e.backups.iter().find(|b| b.dir == dir).cloned() else { return };
        if e.busy { return; }
        e.busy = true;
        e.armed = None;
        let (id, me) = (id.to_string(), self.me.clone());
        std::thread::spawn(move || {
            let r = saves::restore(&id, &b);
            let list = saves::list(&id);
            me.update(move |s: &mut State, tx: &mut Tx| {
                let e = s.extras.entry(id).or_default();
                e.busy = false;
                e.backups = list;
                match r {
                    Ok(()) => s.notify(format!("Sauvegardes du {} restaurées · les précédentes sont gardées", extras::date_text(b.created)), false, tx),
                    Err(err) => s.notify(format!("Restauration interrompue : {err}"), true, tx),
                }
            });
        });
    }

    pub fn delete_backup(&mut self, id: &str, dir: PathBuf, tx: &mut Tx) {
        let Some(e) = self.extras.get_mut(id) else { return };
        e.armed = None;
        let Some(b) = e.backups.iter().find(|b| b.dir == dir).cloned() else { return };
        match saves::delete(&b) {
            Ok(()) => {
                e.backups.retain(|x| x.dir != dir);
                self.notify("Copie supprimée".into(), false, tx);
            }
            Err(err) => self.notify(err, true, tx),
        }
    }

    /// Second press: runs the store's or the installer's uninstaller.
    pub fn uninstall_game(&mut self, id: &str, tx: &mut Tx) {
        let name = self.game(id).map(|g| g.name.clone()).unwrap_or_default();
        let Some(e) = self.extras.get_mut(id) else { return };
        e.armed = None;
        let Some(u) = e.uninstall.clone() else { return };
        let r = match &u {
            extras::uninstall::Uninstaller::Uri(uri) => launch::open_uri(uri),
            extras::uninstall::Uninstaller::Command { exe, args, .. } => launch::run(exe, args),
        };
        match r {
            // The scan finds the game gone once the uninstaller is done.
            Ok(()) => self.notify(format!("Désinstallation de {name} · relancez l'analyse (F5) une fois terminée"), false, tx),
            Err(err) => self.notify(err, true, tx),
        }
    }
}
