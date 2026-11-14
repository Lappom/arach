//! The window's side of the MCP server: connecting AI apps from Paramètres › Assistants IA, and
//! doing what assistants ask (`crate::mcp::bridge`): launching games, editing them, changing
//! their pictures and the actions around them.

use std::path::PathBuf;

use rune::{TextEdit, Tx};

use super::{Page, State};
use crate::mcp::bridge::{self, GamePatch, Reply, Request};
use crate::mcp::clients::{self, Activity, CLIENTS, Client, Status};
use crate::store::{self, Collection};

/// One AI app, as Paramètres shows it.
#[derive(Clone, Debug)]
pub struct McpApp {
    pub client: &'static Client,
    pub status: Status,
    pub config: PathBuf,
    /// Its last visit, when the server saw it.
    pub seen: Option<Activity>,
}

/// What Paramètres › Assistants IA shows, read off the UI thread.
#[derive(Clone, Debug)]
pub struct McpView {
    pub exe: PathBuf,
    pub apps: Vec<McpApp>,
    /// Assistants that connected but aren't among `CLIENTS`.
    pub others: Vec<Activity>,
}

impl McpView {
    pub fn read() -> McpView {
        let exe = clients::exe();
        let activity = clients::activity();
        let apps = CLIENTS.iter().map(|c| McpApp {
            client: c,
            status: c.status(&exe),
            config: c.config().0,
            seen: activity.iter().find(|a| a.client() == Some(c.id)).cloned(),
        }).collect();
        let others = activity.into_iter().filter(|a| a.client().is_none()).collect();
        McpView { exe, apps, others }
    }
}

impl State {
    /// Reads every AI app's settings again, in the background.
    pub fn refresh_mcp(&mut self) {
        let me = self.me.clone();
        std::thread::spawn(move || {
            let view = McpView::read();
            me.update(move |s: &mut State, _: &mut Tx| s.mcp = Some(view));
        });
    }

    /// Adds Arach to an AI app's MCP servers, or takes it out.
    pub fn connect_mcp(&mut self, id: &'static str, connect: bool, tx: &mut Tx) {
        let Some(c) = clients::client(id) else { return };
        let exe = clients::exe();
        let result = if connect { c.connect(&exe).map(|_| ()) } else { c.disconnect() };
        match result {
            Ok(()) if connect => {
                let then = if c.restart { format!(" : redémarrez {} pour qu'il le voie", c.name) } else { String::new() };
                self.notify(format!("Arach ajouté à {}{then}", c.name), false, tx);
            }
            Ok(()) => self.notify(format!("Arach retiré de {}", c.name), false, tx),
            Err(e) => self.notify(e, true, tx),
        }
        self.refresh_mcp();
    }

    /// Links handed to the window: `arach://mcp/<nonce>/<request>` from the MCP server, answered
    /// in a file it waits for.
    pub fn open_link(&mut self, url: &str, tx: &mut Tx) {
        let Some((nonce, req)) = bridge::parse(url) else { return };
        let reply = self.serve_request(req, tx);
        if !reply.ok {
            self.notify(format!("Assistant : {}", reply.message), true, tx);
        }
        bridge::answer(&nonce, &reply);
    }

    /// Does what an assistant asked, if the user lets it (the server checked already; the window
    /// checks again, since any program can send it a link).
    fn serve_request(&mut self, req: Request, tx: &mut Tx) -> Reply {
        let m = self.settings.mcp.clone();
        if !m.enabled {
            return Reply::err("AI assistant access is turned off in Arach.");
        }
        let id = match &req {
            Request::Launch { id } | Request::EditGame { id, .. } | Request::SetArt { id, .. } => Some(id.clone()),
            Request::SetActions { id, .. } => id.clone(),
        };
        let game = match &id {
            Some(id) => match self.any_game(id) {
                Some(_) => {
                    let primary = self.primary_id(id);
                    self.game(&primary).cloned()
                }
                None => return Reply::err(format!("No game `{id}` in Arach.")),
            },
            None => None,
        };
        match req {
            Request::Launch { id } => {
                if !m.allow_launch { return Reply::err("The user doesn't allow assistants to launch games."); }
                let Some(g) = game else { return Reply::err("No such game.") };
                if g.not_installed { return Reply::err(format!("{} isn't installed.", g.name)); }
                self.launch(&id, tx);
                Reply::ok(format!("Arach is starting {}.", g.name))
            }
            Request::EditGame { patch, .. } => {
                if !m.edit_games { return Reply::err("The user doesn't allow assistants to edit games."); }
                let Some(g) = game else { return Reply::err("No such game.") };
                let done = self.apply_game_patch(&g.id, &patch);
                self.notify(format!("Un assistant a modifié {}", g.name), false, tx);
                Reply::ok(format!("{}: {}.", g.name, done.join(", ")))
            }
            Request::SetArt { kind, path, .. } => {
                if !m.edit_games { return Reply::err("The user doesn't allow assistants to edit games."); }
                let Some(g) = game else { return Reply::err("No such game.") };
                // Only a picture the server put in the user's own folder.
                if path.as_ref().is_some_and(|p| !p.starts_with(crate::store::UserData::covers_dir()) || !p.is_file()) {
                    return Reply::err("That picture isn't in Arach's covers folder.");
                }
                let reset = path.is_none();
                self.set_art(&g.id, kind.picker(), path, tx);
                let what = kind.picker().label().to_lowercase();
                Reply::ok(if reset { format!("{}: {what} back to the store's.", g.name) } else { format!("{}: new {what} set.", g.name) })
            }
            Request::SetActions { patch, .. } => {
                if patch.touches_scripts() && !m.edit_scripts {
                    return Reply::err("The user doesn't allow assistants to change scripts.");
                }
                if (patch.touches_system() || patch.own.is_some()) && !m.edit_actions {
                    return Reply::err("The user doesn't allow assistants to change launch actions.");
                }
                match game {
                    None => {
                        self.set(tx, |st| patch.apply(&mut st.actions));
                        let a = &self.settings.actions;
                        self.scripts = [TextEdit::new(&a.before), TextEdit::new(&a.after)];
                        self.notify("Un assistant a modifié les actions avant et après les jeux".into(), false, tx);
                        Reply::ok("Actions for every game updated.")
                    }
                    Some(g) => {
                        if patch.own == Some(false) {
                            self.user.actions.remove(&g.id);
                        } else {
                            let mut a = self.actions_for(&g.id).clone();
                            patch.apply(&mut a);
                            self.user.actions.insert(g.id.clone(), a);
                        }
                        store::save_user(self.user.clone());
                        if self.page == Page::Game(g.id.clone()) {
                            let a = self.actions_for(&g.id);
                            self.game_scripts = [TextEdit::new(&a.before), TextEdit::new(&a.after)];
                        }
                        self.notify(format!("Un assistant a modifié les actions de {}", g.name), false, tx);
                        Reply::ok(if patch.own == Some(false) {
                            format!("{} now uses every game's actions.", g.name)
                        } else {
                            format!("{} has its own actions, updated.", g.name)
                        })
                    }
                }
            }
        }
    }

    /// Applies an assistant's changes to a game; returns what changed, in words.
    fn apply_game_patch(&mut self, id: &str, p: &GamePatch) -> Vec<String> {
        let mut done = Vec::new();
        if p.touches_facts() {
            let src = self.source_values(id);
            let mut e = self.user.edits.get(id).cloned().unwrap_or_default();
            // A value equal to the store's is no correction: drop it, so later store updates show.
            let text = |v: &Option<Option<String>>, src: &str, field: &mut Option<String>| {
                if let Some(v) = v { *field = v.clone().filter(|v| v.trim() != src.trim()); }
            };
            let list = |v: &Option<Option<Vec<String>>>, src: &str, field: &mut Option<Vec<String>>| {
                if let Some(v) = v { *field = v.clone().filter(|v| v.join(", ") != src.trim()); }
            };
            text(&p.name, &src[0], &mut e.name);
            text(&p.description, &src[1], &mut e.description);
            list(&p.developers, &src[2], &mut e.developers);
            list(&p.publishers, &src[3], &mut e.publishers);
            text(&p.release, &src[4], &mut e.release);
            list(&p.genres, &src[5], &mut e.genres);
            self.store_edit(id, e);
            done.push("facts updated".into());
        }
        if let Some(status) = p.status {
            self.user.edit_entry(id, |e| e.status = status);
            done.push(format!("status {}", status.label()));
        }
        if let Some(r) = p.rating {
            self.user.edit_entry(id, |e| e.rating = r.min(5));
            done.push(if r == 0 { "rating cleared".into() } else { format!("rated {r}/5") });
        }
        if let Some(notes) = &p.notes {
            self.user.edit_entry(id, |e| e.notes = notes.clone());
            if self.page == Page::Game(id.to_string()) { self.notes = TextEdit::new(notes); }
            done.push("notes updated".into());
        }
        if let Some(fav) = p.favorite {
            if fav { self.user.favorites.insert(id.to_string()); } else { self.user.favorites.remove(id); }
            done.push(if fav { "added to favorites".into() } else { "removed from favorites".into() });
        }
        for name in &p.add_to {
            let found = self.user.collections.iter().position(|c| c.name.eq_ignore_ascii_case(name.trim()));
            let i = match found {
                Some(i) => i,
                None => {
                    let cid = super::fnv(&format!("{name}{}", store::now())) | 1;
                    self.user.collections.push(Collection { id: cid, name: name.trim().to_string(), games: Vec::new() });
                    done.push(format!("collection « {name} » created"));
                    self.user.collections.len() - 1
                }
            };
            let c = &mut self.user.collections[i];
            if !c.games.iter().any(|g| g == id) { c.games.push(id.to_string()); }
            done.push(format!("in « {} »", c.name));
        }
        for name in &p.remove_from {
            match self.user.collections.iter_mut().find(|c| c.name.eq_ignore_ascii_case(name.trim())) {
                Some(c) => {
                    c.games.retain(|g| g != id);
                    done.push(format!("out of « {} »", c.name));
                }
                None => done.push(format!("no collection « {name} »")),
            }
        }
        store::save_user(self.user.clone());
        self.rebuild();
        done
    }
}
