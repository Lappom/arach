//! The game page's extra sections: screenshots and saves in the reading column; achievements,
//! how long to beat, mods and uninstalling in the side column. Each one shows only when it has
//! something to say.

use rune::{Color, El, SurfaceFit, Tx, col, el, row, text};

use super::icons::{Icon, icon};
use super::theme::{Palette, caption, mono, num, ui};
use super::widgets::{Tone, button, ghost_button, icon_button, playtime, section_head};
use crate::app::State;
use crate::extras::{self, Arm, Extras, Lookup, size_text, date_text};
use crate::game::Game;

const THUMB_W: f32 = 176.0;
const THUMB_H: f32 = 99.0;

fn card(p: &Palette, key: &str, title: &str) -> El {
    col().key(key.to_string()).w_full().px(18.0).pt(16.0).pb(12.0).gap(10.0).radius(10.0)
        .fill(p.surface.alpha(if p.dark { 0.55 } else { 1.0 })).border(1.0, p.line)
        .child(text(title.to_string()).font(caption()).color(p.faint))
}

fn quiet(p: &Palette, t: impl Into<String>) -> El {
    text(t.into()).font(ui(13.0, 450.0)).color(p.faint).wrap()
}

/// A bar filled to `done / total`, drawn with two growing halves so it needs no width.
fn meter(p: &Palette, key: &str, done: f32, total: f32, ink: Color) -> El {
    let f = if total > 0.0 { (done / total).clamp(0.0, 1.0) } else { 0.0 };
    let mut bar = row().key(key.to_string()).w_full().h(6.0).radius(3.0).clip_rounded(3.0).fill(p.text.alpha(0.08));
    if f > 0.0 { bar = bar.child(el().w(0.0).grow(f).h_full().fill(ink)); }
    if f < 1.0 { bar = bar.child(el().w(0.0).grow(1.0 - f).h_full()); }
    bar
}

fn side_link(p: &Palette, key: &str, label: &str, glyph: Icon, sub: Option<String>) -> El {
    row().key(key.to_string()).role("link").label(label.to_string()).w_full().min_h(40.0).px(10.0).py(6.0).gap(11.0).radius(6.0).center_y()
        .hover(p.hover)
        .press_scale(0.98)
        .child(icon(glyph, 16.0, p.muted))
        .child(col().grow(1.0).min_w(0.0).gap(2.0)
            .child(text(label.to_string()).font(ui(13.0, 550.0)).color(p.text))
            .children(sub.map(|s| text(s).font(ui(11.5, 450.0)).color(p.faint).truncate())))
        .child(icon(Icon::External, 14.0, p.faint))
}

// ---------------------------------------------------------------- screenshots

pub fn screenshots(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let e = s.extras(&g.id)?;
    if e.shots.is_empty() { return None; }
    let mut origins: Vec<&str> = Vec::new();
    for shot in &e.shots {
        if !origins.contains(&shot.origin.label()) { origins.push(shot.origin.label()); }
    }
    let n = e.shots.len();
    let count = text(format!("{n} · {}", origins.join(", "))).font(ui(12.5, 500.0)).color(p.faint);
    let mut grid = row().key("shots").w_full().gap(10.0).flex_wrap();
    for (i, shot) in e.shots.iter().take(e.shots_shown).enumerate() {
        let path = shot.path.clone();
        let mut tile = el().key(format!("shot-{i}")).role("button").label("Ouvrir la capture")
            .help(format!("{} · {}", shot.origin.label(), date_text(shot.taken)))
            .w(THUMB_W).h(THUMB_H).radius(6.0).clip_rounded(6.0).fill(p.text.alpha(0.06)).border(1.0, p.line)
            .press_scale(0.98);
        if let Some(img) = s.images.borrow_mut().get(&shot.path, (THUMB_W * 2.0) as u32) {
            tile = tile.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).pixels_fit(img.pixels(), SurfaceFit::Fill));
        }
        grid = grid.child(tile.on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_path(&path, tx)));
    }
    let id = g.id.clone();
    let folder = e.shots[0].path.clone();
    let mut more = row().gap(8.0).center_y()
        .child(ghost_button(p, "shots-folder", "Dossier", Some(Icon::Folder))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_in_folder(&folder, tx)));
    if n > e.shots_shown {
        more = more.child(ghost_button(p, "shots-more", &format!("Voir plus ({})", n - e.shots_shown), None)
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.more_shots(&id, tx)));
    }
    Some(col().key("screenshots").w_full().gap(12.0)
        .child(section_head(p, "Captures d'écran", Some(count)))
        .child(grid)
        .child(more))
}

// ---------------------------------------------------------------- saves

pub fn saves(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let e = s.extras(&g.id)?;
    if matches!(&e.saves, Lookup::Missing(why) if why.is_empty()) && e.backups.is_empty() { return None; }
    let id = g.id.clone();
    let action = matches!(e.saves, Lookup::Found(_)).then(|| {
        let id = id.clone();
        button(p, "saves-backup", if e.busy { "Copie…" } else { "Sauvegarder maintenant" }, Some(Icon::Plus), Tone::Secondary)
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.backup_saves(&id, tx))
    });
    let mut c = col().key("saves").w_full().gap(12.0).child(section_head(p, "Sauvegardes", action));
    match &e.saves {
        Lookup::Pending => c = c.child(quiet(p, "Recherche de l'emplacement des sauvegardes (base Ludusavi, PCGamingWiki)…")),
        Lookup::Missing(why) => c = c.child(quiet(p, why.clone())),
        Lookup::Found(l) => {
            let places = if l.places.len() > 1 { format!("{} emplacements", l.places.len()) } else { "1 emplacement".into() };
            c = c.child(text(format!("{places} · {} · d'après la {}", size_text(l.bytes), l.from.label())).font(ui(12.5, 500.0)).color(p.faint));
            let mut list = col().key("save-paths").w_full().radius(8.0).border(1.0, p.line).fill(p.field);
            for (i, path) in l.places.iter().enumerate() {
                let open = path.clone();
                if i > 0 { list = list.child(el().h(1.0).w_full().fill(p.line)); }
                list = list.child(row().w_full().min_h(40.0).pl(14.0).pr(6.0).gap(10.0).center_y()
                    .child(text(path.display().to_string()).font(mono(12.0).lh(18.0)).color(p.muted).wrap().selectable(true).grow(1.0).min_w(0.0))
                    .child(icon_button(p, format!("save-open-{i}"), Icon::Folder, "Ouvrir l'emplacement", 30.0)
                        .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_in_folder(&open, tx))));
            }
            c = c.child(list);
        }
    }
    if !e.backups.is_empty() {
        c = c.child(text("Copies").font(caption()).color(p.faint).pt(6.0)).child(backups(p, &id, e));
    }
    Some(c)
}

fn backups(p: &Palette, id: &str, e: &Extras) -> El {
    let mut list = col().key("backups").w_full().gap(2.0);
    for (i, b) in e.backups.iter().take(8).enumerate() {
        let (dir, id) = (b.dir.clone(), id.to_string());
        let armed_restore = e.armed == Some(Arm::Restore(dir.clone()));
        let armed_delete = e.armed == Some(Arm::DeleteBackup(dir.clone()));
        let label = col().grow(1.0).min_w(0.0).gap(2.0)
            .child(text(date_text(b.created)).font(ui(13.0, 550.0)).color(p.text))
            .child(text(match (b.automatic, armed_restore, armed_delete) {
                (_, true, _) => "Remplacer les sauvegardes actuelles ? Elles seront copiées avant.".to_string(),
                (_, _, true) => "Supprimer cette copie ?".to_string(),
                (true, ..) => format!("{} · faite avant une restauration", size_text(b.bytes)),
                (false, ..) => size_text(b.bytes),
            }).font(ui(12.0, 450.0)).color(if armed_restore || armed_delete { p.text.alpha(0.8) } else { p.faint }).wrap());
        let mut r = row().key(format!("backup-{i}")).w_full().min_h(48.0).px(10.0).py(6.0).gap(8.0).radius(6.0).center_y()
            .fill(if armed_restore || armed_delete { p.accent.alpha(0.08) } else { Color::TRANSPARENT })
            .child(icon(Icon::Folder, 15.0, p.faint))
            .child(label);
        if armed_restore || armed_delete {
            let (yes_id, yes_dir, no_id) = (id.clone(), dir.clone(), id.clone());
            let yes = if armed_restore {
                button(p, format!("backup-yes-{i}"), "Restaurer", None, Tone::Danger)
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.restore_backup(&yes_id, yes_dir.clone(), tx))
            } else {
                button(p, format!("backup-yes-{i}"), "Supprimer", None, Tone::Danger)
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.delete_backup(&yes_id, yes_dir.clone(), tx))
            };
            r = r.child(yes).child(ghost_button(p, format!("backup-no-{i}"), "Annuler", None)
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.disarm(&no_id, tx)));
        } else {
            let (restore_id, restore_dir) = (id.clone(), dir.clone());
            let (open_dir, del_id, del_dir) = (dir.clone(), id.clone(), dir.clone());
            r = r.hover(p.hover)
                .child(ghost_button(p, format!("backup-restore-{i}"), "Restaurer", Some(Icon::Refresh))
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.arm(&restore_id, Arm::Restore(restore_dir.clone()), tx)))
                .child(icon_button(p, format!("backup-open-{i}"), Icon::External, "Ouvrir la copie", 30.0)
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_path(&open_dir, tx)))
                .child(icon_button(p, format!("backup-del-{i}"), Icon::Trash, "Supprimer la copie", 30.0)
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.arm(&del_id, Arm::DeleteBackup(del_dir.clone()), tx)));
        }
        list = list.child(r);
    }
    list
}

// ---------------------------------------------------------------- side cards

/// Steam achievements: how many, a bar, and the last ones won.
pub fn achievements(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let e = s.extras(&g.id)?;
    let a = match &e.achievements {
        Lookup::Found(a) => a,
        Lookup::Pending if g.id.starts_with("steam:") => return Some(card(p, "achievements", "Succès Steam").child(quiet(p, "Lecture du profil Steam…"))),
        _ => return None,
    };
    let pct = (a.unlocked as f32 / a.total.max(1) as f32 * 100.0).round() as u32;
    let gold = rune::oklch(0.82, 0.15, 85.0);
    let mut c = card(p, "achievements", "Succès Steam")
        .child(row().gap(8.0).items_end()
            .child(text(format!("{} / {}", a.unlocked, a.total)).font(num(20.0, 650.0)).color(p.text))
            .child(text(format!("{pct} %")).font(num(13.0, 550.0)).color(p.faint).pb(2.0)))
        .child(meter(p, "achievements-bar", a.unlocked as f32, a.total as f32, gold));
    for (i, (name, at)) in a.recent.iter().enumerate() {
        c = c.child(row().key(format!("ach-{i}")).w_full().gap(8.0).center_y()
            .child(icon(Icon::StarFilled, 13.0, gold))
            .child(text(name.clone()).font(ui(12.5, 550.0)).color(p.text).truncate().grow(1.0).min_w(0.0))
            .children((*at > 0).then(|| text(super::widgets::ago(*at)).font(ui(11.5, 450.0)).color(p.faint))));
    }
    let source = if a.public { "Profil Steam public" } else { "Lu dans le cache de Steam" };
    Some(c.child(text(source).font(ui(11.5, 450.0)).color(p.faint)))
}

/// HowLongToBeat's three figures, each against the time already played.
pub fn hltb(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let e = s.extras(&g.id)?;
    let h = match &e.hltb {
        Lookup::Found(h) if h.main + h.plus + h.full > 0 => h,
        Lookup::Pending => return Some(card(p, "hltb", "Durée de vie").child(quiet(p, "Recherche sur HowLongToBeat…"))),
        Lookup::Missing(why) if !why.is_empty() => return Some(card(p, "hltb", "Durée de vie").child(quiet(p, why.clone()))),
        _ => return None,
    };
    let played = s.playtime_min(g);
    let mut c = card(p, "hltb", "Durée de vie");
    for (key, label, len) in [("hltb-main", "Histoire", h.main), ("hltb-plus", "Histoire + extras", h.plus), ("hltb-full", "100 %", h.full)] {
        if len == 0 { continue; }
        let done = played >= len;
        c = c.child(col().key(key.to_string()).w_full().gap(6.0)
            .child(row().w_full().gap(8.0).center_y()
                .child(text(label.to_string()).font(ui(13.0, 500.0)).color(p.muted).grow(1.0))
                .child(text(playtime(len)).font(num(13.0, 650.0)).color(p.text)))
            .child(meter(p, &format!("{key}-bar"), played as f32, len as f32, if done { rune::oklch(0.7, 0.14, 145.0) } else { p.accent })));
    }
    let you = match (played, h.main) {
        (0, _) => "Pas encore joué".to_string(),
        (m, main) if main > 0 && m < main => format!("Joué : {} · {} % de l'histoire", playtime(m), m * 100 / main),
        (m, _) => format!("Joué : {}", playtime(m)),
    };
    let url = h.url();
    Some(c.child(text(you).font(ui(12.5, 550.0)).color(p.text).pt(2.0))
        .child(row().key("hltb-link").role("link").label("Voir sur HowLongToBeat").gap(6.0).center_y().self_start().radius(4.0)
            .hover(p.hover)
            .child(text("HowLongToBeat").font(ui(11.5, 500.0)).color(p.faint))
            .child(icon(Icon::External, 12.0, p.faint))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_uri(&url, tx))))
}

/// The mods folder (or the game's own), and Nexus Mods.
pub fn mods(s: &State, p: &Palette, g: &Game) -> Option<El> {
    if g.not_installed || g.source.is_console() { return None; }
    let e = s.extras(&g.id);
    let mut c = card(p, "mods", "Mods").px(8.0).gap(2.0);
    match (e.and_then(|e| e.mods.clone()), g.dir.clone()) {
        (Some(dir), _) => {
            let sub = dir.file_name().map(|f| f.to_string_lossy().into_owned());
            c = c.child(side_link(p, "mods-folder", "Ouvrir le dossier des mods", Icon::Folder, sub)
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_in_folder(&dir, tx)));
        }
        (None, Some(dir)) => {
            c = c.child(side_link(p, "mods-folder", "Ouvrir le dossier du jeu", Icon::Folder, Some("Pas de dossier de mods repéré".into()))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_in_folder(&dir, tx)));
        }
        (None, None) => {}
    }
    let url = extras::nexus_url(&g.name);
    Some(c.child(side_link(p, "mods-nexus", "Nexus Mods", Icon::Grid, Some("Chercher des mods pour ce jeu".into()))
        .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_uri(&url, tx))))
}

/// Uninstalling, whatever the store; a second press confirms.
pub fn uninstall(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let e = s.extras(&g.id)?;
    let u = e.uninstall.as_ref()?;
    let id = g.id.clone();
    let mut c = card(p, "uninstall", "Désinstaller");
    if e.armed == Some(Arm::Uninstall) {
        let (yes, no) = (id.clone(), id.clone());
        c = c.child(text(format!("Désinstaller {} ?", g.name)).font(ui(13.5, 600.0)).color(p.text).wrap())
            .child(quiet(p, u.how(g)))
            .child(row().gap(8.0)
                .child(button(p, "uninstall-yes", "Désinstaller", Some(Icon::Trash), Tone::Danger)
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.uninstall_game(&yes, tx)))
                .child(ghost_button(p, "uninstall-no", "Annuler", None)
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.disarm(&no, tx))));
    } else {
        c = c.px(8.0).gap(2.0).child(
            row().key("uninstall-arm").role("button").label("Désinstaller…").w_full().h(40.0).px(10.0).gap(11.0).radius(6.0).center_y()
                .hover(p.danger.alpha(0.1))
                .press_scale(0.98)
                .child(icon(Icon::Trash, 16.0, p.danger))
                .child(text("Désinstaller…").font(ui(13.0, 550.0)).color(p.danger).grow(1.0))
                .child(text(g.source.label()).font(ui(11.5, 500.0)).color(p.faint))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.arm(&id, Arm::Uninstall, tx)),
        );
    }
    Some(c)
}

/// "Mise à jour en attente": over the art on the game's page, and on its cover.
pub fn update_badge(p: &Palette, small: bool) -> El {
    let ink = rune::oklch(0.78, 0.14, 230.0);
    let (h, font, px) = if small { (20.0, 10.5, 7.0) } else { (26.0, 12.0, 10.0) };
    row().key("update-badge").role("status").label("Mise à jour en attente").help("Steam a une mise à jour à installer pour ce jeu".to_string())
        .h(h).px(px).gap(5.0).radius(h / 2.0).center_y()
        .fill(Color::BLACK.alpha(if p.dark { 0.45 } else { 0.55 }))
        .backdrop_blur(12.0)
        .border(1.0, ink.alpha(0.5))
        .child(icon(Icon::Refresh, font + 1.0, ink))
        .child(text(if small { "Mise à jour" } else { "Mise à jour en attente" }).font(ui(font, 650.0)).color(Color::WHITE.alpha(0.95)))
}
