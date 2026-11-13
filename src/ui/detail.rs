//! A game's page: its key art across the top, tinting the whole page; one bar to play it and
//! see where it stands; then what it is, where it lives, and how it starts.

use rune::{Color, El, SurfaceFit, TextStyle, Tx, col, el, row, text, text_input};

use super::extras as x;
use super::home::title_block;
use super::icons::{Icon, icon};
use super::theme::{Palette, caption, mono, num, ui};
use super::widgets::{
    ago, cover_art, fact, fact_num, fact_text, icon_button, play_button, playtime, section_head, segmented, size, status_color,
    stop_button, store_dot,
};
use crate::app::{Overlay, State};
use crate::meta::picker::Kind;
use crate::store::{Session, Status};
use crate::game::{Game, Launch, Source};

const PAD: f32 = 40.0;
const HERO_H: f32 = 440.0;

pub fn view(s: &State, p: &Palette, id: &str, w: f32, layer: &mut Vec<El>) -> El {
    let Some(g) = s.game(id) else {
        return col().w_full().h(400.0).center().child(text("Ce jeu n'est plus dans la bibliothèque").font(ui(15.0, 500.0)).color(p.muted));
    };
    let wide = w > 980.0;
    // Launch options mean nothing for a game that isn't installed.
    let options = (!g.not_installed).then(|| launch_options(s, p, g));
    let around = (!g.not_installed).then(|| super::settings::game_actions(s, p, g, layer));
    let body = if wide {
        row().w_full().px(PAD).gap(48.0).items_start()
            .child(col().grow(1.0).min_w(0.0).max_w(760.0).gap(44.0).child(about(s, p, g)).child(progress(s, p, g))
                .children(x::screenshots(s, p, g)).children(x::saves(s, p, g)).children(options).children(around))
            .child(el().grow(1.0).w(0.0))
            .child(col().w(340.0).gap(16.0).child(facts(s, p, g)).children(x::achievements(s, p, g)).children(x::hltb(s, p, g))
                .children(versions(s, p, g)).children(sessions(s, p, g)).children(location(s, p, g)).children(x::mods(s, p, g))
                .children(steam_tools(p, g)).children(x::uninstall(s, p, g)))
    } else {
        col().w_full().px(PAD).gap(32.0)
            .child(about(s, p, g))
            .child(progress(s, p, g))
            .children(x::screenshots(s, p, g))
            .children(x::saves(s, p, g))
            .children(options)
            .children(around)
            .child(facts(s, p, g))
            .children(x::achievements(s, p, g))
            .children(x::hltb(s, p, g))
            .children(versions(s, p, g))
            .children(sessions(s, p, g))
            .children(location(s, p, g))
            .children(x::mods(s, p, g))
            .children(steam_tools(p, g))
            .children(x::uninstall(s, p, g))
    };
    col().key(format!("game-{}", crate::app::fnv(id))).w_full().pb(64.0)
        .child(ambient(s, p, g, w))
        .child(hero(s, p, g, w))
        .child(play_bar(s, p, g))
        .child(body)
}

/// The signature: the game's art, blurred into a wash behind the whole top of the page.
/// It settles into the plain canvas before the reading column, so text never sits on picture.
fn ambient(s: &State, p: &Palette, g: &Game, w: f32) -> El {
    let layer = el().key("ambient").pin(Some(0.0), Some(0.0), Some(0.0), None).h(900.0);
    if !s.settings.ambient { return layer; }
    // Steam ships a pre-blurred copy; otherwise a tiny decode of the hero, stretched, is a blur.
    let wash = g.art.hero_blur.as_ref().map(|p| (p, 96)).or_else(|| g.art.hero.as_ref().map(|p| (p, 24)))
        .or_else(|| g.art.cover.as_ref().map(|p| (p, 16)));
    let Some((path, px)) = wash else { return layer };
    let Some(img) = s.images.borrow_mut().get(path, px) else { return layer };
    layer
        .child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).opacity(if p.dark { 0.42 } else { 0.28 })
            .pixels_fit(img.pixels(), SurfaceFit::Fill))
        .child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .linear_gradient(std::f32::consts::PI, [(0.0, p.bg.alpha(0.2)), (0.45, p.bg.alpha(0.82)), (0.7, p.bg), (1.0, p.bg)]))
        .w(w)
}

/// A small frosted label over the art.
fn tag(p: &Palette, dot: Option<Source>, label: &str) -> El {
    row().h(26.0).px(10.0).gap(7.0).radius(13.0).center_y()
        .fill(Color::BLACK.alpha(if p.dark { 0.35 } else { 0.08 }))
        .backdrop_blur(12.0)
        .border(1.0, p.text.alpha(0.1))
        .children(dot.map(|d| store_dot(d, 7.0)))
        .child(text(label.to_string()).font(ui(12.0, 600.0)).color(p.text.alpha(0.92)))
}

/// `Riot Games, Inc.  ·  2021`: who made it and when, under the title.
fn byline(s: &State, g: &Game) -> Option<String> {
    let m = s.meta(&g.id)?;
    let year = m.release.split_whitespace().rev().find(|t| t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()));
    let parts: Vec<&str> = m.developers.first().map(String::as_str).into_iter().chain(year).collect();
    (!parts.is_empty()).then(|| parts.join("  ·  "))
}

fn hero(s: &State, p: &Palette, g: &Game, w: f32) -> El {
    let mut art = el().key("detail-art").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0));
    if let Some(img) = g.art.hero.as_ref().and_then(|h| s.images.borrow_mut().get(h, (w * 1.5).min(2400.0) as u32)) {
        art = art.pixels_fit(img.pixels(), SurfaceFit::Fill);
    }
    let fade = el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .linear_gradient(std::f32::consts::PI, [(0.0, p.bg.alpha(0.25)), (0.3, Color::TRANSPARENT), (0.6, p.bg.alpha(0.25)), (1.0, p.bg.alpha(0.7))]);
    let side = el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .linear_gradient(std::f32::consts::FRAC_PI_2, [(0.0, p.bg.alpha(0.8)), (0.5, p.bg.alpha(0.15)), (1.0, p.bg.alpha(0.0))]);

    let mut tags = row().gap(6.0).center_y().flex_wrap().child(tag(p, Some(g.source), g.source.label()))
        .children(g.update_pending.then(|| x::update_badge(p, false)));
    if let Some(m) = s.meta(&g.id) {
        for genre in m.genres.iter().take(3) {
            tags = tags.child(tag(p, None, genre));
        }
    }
    // A cover beside the title when there's no key art to carry the hero.
    let side_cover = (g.art.hero.is_none()).then(|| cover_art(s, g, p, 140.0, 210.0).shadow(12.0, 32.0, Color::BLACK.alpha(0.45)));
    let content = row().pin(Some(PAD), None, Some(PAD), Some(28.0)).gap(28.0).items_end()
        .children(side_cover)
        .child(col().gap(14.0).grow(1.0).min_w(0.0)
            .child(tags)
            .child(title_block(s, p, g, 460.0, 68.0))
            .children(byline(s, g).map(|b| text(b).key("byline").font(ui(14.0, 500.0)).color(p.text.alpha(0.72)).truncate())));
    // The art and its shading melt into the ambient wash below instead of ending on a flat
    // canvas: a solid bottom edge would meet the (tinted) wash on a hard line.
    let backdrop = el().key("detail-backdrop").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .clip_fade(0.0, 0.0, 0.0, 180.0)
        .child(art)
        .child(fade)
        .child(side);
    el().key("detail-hero").w_full().h(HERO_H)
        .child(backdrop)
        .child(content)
}

/// A labelled secondary action in the play bar: the word says what it does, the icon finds it.
fn bar_button(p: &Palette, key: &str, glyph: Icon, glyph_ink: Color, label: &str, help: &str) -> El {
    row().key(key.to_string()).role("button").label(help.to_string()).help(help.to_string())
        .h(40.0).pl(13.0).pr(15.0).gap(8.0).radius(20.0).center()
        .fill(p.text.alpha(0.07))
        .border(1.0, p.text.alpha(0.08))
        .hover(p.text.alpha(0.06))
        .press_scale(0.97)
        .child(icon(glyph, 16.0, glyph_ink))
        .child(text(label.to_string()).font(ui(13.5, 600.0)).color(p.text))
}

/// Everything you do with the game, and what you'd check before doing it, on one bar:
/// play and its companions on the left, the figures that matter on the right.
fn play_bar(s: &State, p: &Palette, g: &Game) -> El {
    let id = g.id.clone();
    let (play, fav, folder, more) = (id.clone(), id.clone(), id.clone(), id.clone());
    let is_fav = s.is_favorite(&g.id);
    let gold = rune::oklch(0.86, 0.15, 85.0);
    let menu_open = matches!(&s.overlay, Overlay::GameMenu { id, at: None } if *id == g.id);
    // While the game runs, Jouer becomes Arrêter, in the same place.
    let main = if g.not_installed {
        install_button(p, g).on_press_tx(move |s: &mut State, tx: &mut Tx| s.launch(&play, tx))
    } else if s.stopping.contains(&g.id) {
        stop_button(p, "detail-play", "Arrêt…", true).opacity(0.6)
    } else if s.is_launched(&g.id) {
        stop_button(p, "detail-play", "Arrêter", true).help("Fermer le jeu".to_string())
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.stop(&play, tx))
    } else {
        play_button(p, "detail-play", "Jouer", true).on_press_tx(move |s: &mut State, tx: &mut Tx| s.launch(&play, tx))
    };
    let mut actions = row().key("actions").gap(8.0).center_y()
        .child(main)
        .child(el().w(4.0))
        .child(bar_button(p, "detail-fav", if is_fav { Icon::StarFilled } else { Icon::Star }, if is_fav { gold } else { p.muted },
            if is_fav { "Favori" } else { "Ajouter aux favoris" }, if is_fav { "Retirer des favoris" } else { "Ajouter aux favoris" })
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.toggle_favorite(&fav, tx)));
    if g.dir.is_some() {
        actions = actions.child(bar_button(p, "detail-folder", Icon::Folder, p.muted, "Dossier", "Ouvrir le dossier du jeu")
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_folder(&folder, tx)));
    }
    actions = actions.child(icon_button(p, "detail-more", Icon::More, "Plus d'actions", 40.0)
        .expanded(menu_open)
        .on_press(move |s: &mut State| s.toggle_overlay(Overlay::GameMenu { id: more.clone(), at: None })));

    // Only figures that say something: an untracked playtime or a zero count is left out, not dashed.
    let played = s.last_played(g);
    let launches = s.user.launches.get(&g.id).copied().unwrap_or(0);
    let stat = |key: &str, label: &str, value: String, sub: Option<String>| {
        col().key(key.to_string()).px(20.0).gap(3.0).min_w(0.0)
            .child(text(label.to_string()).font(caption()).color(p.faint))
            .child(row().gap(8.0).center_y()
                .child(text(value).font(ui(17.0, 650.0)).color(p.text))
                .children(sub.map(|t| text(t).font(ui(12.0, 500.0)).color(p.faint))))
    };
    let mut cells: Vec<El> = Vec::new();
    if g.not_installed {
        cells.push(stat("st-owned", "Non installé", format!("Possédé sur {}", g.source.label()), None));
    }
    match s.playing.get(&g.id) {
        Some(&start) => {
            let min = ((crate::store::now() - start).max(0) / 60) as u64;
            cells.push(stat("st-last", "En ce moment", "En jeu".into(),
                Some(if min == 0 { "depuis moins d'une minute".into() } else { format!("depuis {}", playtime(min)) })));
        }
        None => cells.push(stat("st-last", "Dernière session",
            if played > 0 { ago(played) } else { "Jamais lancé".into() },
            match launches { 0 => None, 1 => Some("1 lancement via Arach".into()), n => Some(format!("{n} lancements via Arach")) })),
    }
    let minutes = s.playtime_min(g);
    if minutes > 0 {
        // Say where the figure comes from when Arach measured it, not the store.
        let measured = minutes > g.playtime_min;
        cells.push(stat("st-time", "Temps de jeu", playtime(minutes), measured.then(|| "mesuré par Arach".to_string())));
    }
    if g.size > 0 {
        cells.push(stat("st-size", "Sur le disque", size(g.size), g.drive().map(|d| format!("sur {d}"))));
    }
    let mut stats = row().key("stats").center_y().flex_wrap();
    let n = cells.len();
    for (i, c) in cells.into_iter().enumerate() {
        stats = stats.child(c);
        if i + 1 < n {
            stats = stats.child(el().w(1.0).h(30.0).fill(p.line));
        }
    }

    let bar = row().key("play-bar").w_full().min_h(72.0).px(14.0).py(12.0).gap(16.0).radius(10.0).center_y().flex_wrap()
        .fill(p.surface.alpha(if p.dark { 0.62 } else { 0.85 }))
        .backdrop_blur(24.0)
        .border(1.0, p.line)
        .child(actions)
        .child(el().grow(1.0))
        .child(stats);
    row().key("play-row").w_full().px(PAD).pb(48.0).child(bar)
}

/// The store's install page for a game owned but not installed: same place and size as Jouer,
/// quieter, with the store's name on it.
fn install_button(p: &Palette, g: &Game) -> El {
    let label = format!("Installer avec {}", g.source.label());
    row().key("detail-play").role("button").label(label.clone())
        .h(46.0).px(24.0).gap(9.0).radius(23.0).center()
        .fill(p.text.alpha(0.1))
        .border(1.0, p.text.alpha(0.12))
        .hover(p.text.alpha(0.07))
        .press_scale(0.97)
        .child(icon(Icon::External, 15.0, p.text))
        .child(text(label).font(ui(15.0, 650.0)).color(p.text))
}

/// Where the player stands with the game: a status, stars, the collections it's in, and notes.
/// All of it lives in `user.json` and is saved at each change.
fn progress(s: &State, p: &Palette, g: &Game) -> El {
    let entry = s.user.entry(&g.id);
    let id = g.id.clone();

    let labels: Vec<&str> = Status::ALL.iter().map(|st| st.label()).collect();
    let selected = Status::ALL.iter().position(|st| *st == entry.status).unwrap_or(0);
    let status = {
        let id = id.clone();
        segmented(p, "status", &labels, selected, move |s, i, tx| s.set_status(&id, Status::ALL[i], tx))
    };
    let status = row().gap(12.0).center_y().flex_wrap()
        .child(status)
        .child(el().size(8.0, 8.0).radius(4.0).fill(status_color(p, entry.status)));

    let gold = rune::oklch(0.86, 0.15, 85.0);
    let mut stars = row().key("rating").role("radiogroup").label("Note").gap(2.0).center_y();
    for n in 1..=5u8 {
        let (id, on) = (id.clone(), n <= entry.rating);
        stars = stars.child(
            el().key(format!("star-{n}")).role("radio").label(format!("{n} sur 5")).checked(n == entry.rating)
                .help(if n == entry.rating { "Retirer la note".to_string() } else { format!("{n} sur 5") })
                .size(32.0, 32.0).radius(6.0).center()
                .hover(p.hover)
                .press_scale(0.9)
                .child(icon(if on { Icon::StarFilled } else { Icon::Star }, 20.0, if on { gold } else { p.faint }))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.set_rating(&id, n, tx)),
        );
    }
    let rating_word = match entry.rating { 0 => "Pas de note".to_string(), n => format!("{n} / 5") };
    let stars = row().gap(10.0).center_y().child(stars).child(text(rating_word).font(num(13.0, 500.0)).color(p.faint));

    // Every collection, as a chip that puts the game in or takes it out.
    let mut chips = row().key("collections").gap(6.0).flex_wrap();
    for c in &s.user.collections {
        let (id, cid, inside) = (id.clone(), c.id, c.games.contains(&g.id));
        chips = chips.child(
            row().key(format!("in-coll-{cid:016x}")).role("checkbox").label(c.name.clone()).checked(inside)
                .h(30.0).pl(10.0).pr(12.0).gap(6.0).radius(15.0).center_y()
                .fill(if inside { p.accent.alpha(0.14) } else { Color::TRANSPARENT })
                .border(1.0, if inside { p.accent.alpha(0.55) } else { p.line })
                .hover(p.hover)
                .press_scale(0.97)
                .child(icon(if inside { Icon::Check } else { Icon::Plus }, 13.0, if inside { p.accent } else { p.faint }))
                .child(text(c.name.clone()).font(ui(12.5, 600.0)).color(if inside { p.text } else { p.muted }))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.toggle_in_collection(cid, &id, tx)),
        );
    }
    {
        let id = id.clone();
        chips = chips.child(
            row().key("in-coll-new").role("button").label("Nouvelle collection").h(30.0).px(12.0).gap(6.0).radius(15.0).center_y()
                .border(1.0, p.line)
                .hover(p.hover)
                .press_scale(0.97)
                .child(icon(Icon::Plus, 13.0, p.faint))
                .child(text("Nouvelle collection").font(ui(12.5, 600.0)).color(p.muted))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.name_collection(None, Some(id.clone()), tx)),
        );
    }

    let notes = text_input(&s.notes, TextStyle::new(14.0, 450.0).lh(22.0))
        .key("notes")
        .grow(1.0)
        .color(p.text)
        .placeholder("Où j'en suis, ce qu'il reste à faire, les codes…", p.faint)
        .label("Notes")
        .on_edit(|s: &mut State| &mut s.notes)
        .on_input_change(|s: &mut State, _: &mut Tx| s.notes_changed());
    let notes = row().key("notes-box").w_full().min_h(110.0).px(14.0).py(11.0).radius(6.0).items_start()
        .fill(p.field)
        .border(1.0, p.line)
        .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
        .child(notes);

    let label = |t: &str| text(t.to_string()).font(caption()).color(p.faint);
    col().key("progress").w_full().gap(12.0)
        .child(section_head(p, "Ma progression", None))
        .child(label("Statut"))
        .child(status)
        .child(label("Note").pt(6.0))
        .child(stars)
        .child(label("Collections").pt(6.0))
        .child(chips)
        .child(label("Notes").pt(6.0))
        .child(notes)
}

/// The sessions Arach measured: the last few, how long in all and this week.
fn sessions(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let list = s.user.sessions.get(&g.id).filter(|v| !v.is_empty())?;
    let total: i64 = list.iter().map(Session::secs).sum();
    let week_ago = crate::store::now() - 7 * 86_400;
    let week: i64 = list.iter().filter(|x| x.end >= week_ago).map(Session::secs).sum();
    let mins = |secs: i64| match secs / 60 { 0 => "< 1 min".to_string(), m => playtime(m as u64) };
    let mut rows: Vec<El> = vec![
        fact(p, "Sessions", fact_num(p, list.len().to_string())),
        fact(p, "Au total", fact_num(p, mins(total))),
        fact(p, "Cette semaine", fact_num(p, if week > 0 { mins(week) } else { "—".into() })),
    ];
    let mut recent = col().key("session-list").w_full().py(8.0).gap(2.0);
    for x in list.iter().rev().take(6) {
        recent = recent.child(row().w_full().h(28.0).gap(12.0).center_y()
            .child(text(date(x.start + crate::store::utc_offset())).font(ui(12.5, 500.0)).color(p.muted).grow(1.0))
            .child(text(clock(x.start)).font(num(12.5, 500.0)).color(p.faint))
            .child(text(mins(x.secs())).font(num(12.5, 600.0)).color(p.text).w(64.0).align(rune::Align::End)));
    }
    rows.push(recent);
    Some(with_rules(p, card(p, "sessions").child(card_title(p, "Sessions mesurées")), rows))
}

/// `21:40`, local time.
fn clock(ts: i64) -> String {
    let t = ts + crate::store::utc_offset();
    format!("{:02}:{:02}", t.rem_euclid(86_400) / 3600, t.rem_euclid(3600) / 60)
}

fn about(s: &State, p: &Palette, g: &Game) -> El {
    let meta = s.meta(&g.id);
    let mut c = col().key("about").w_full().gap(14.0).child(section_head(p, "À propos", None));
    match meta.filter(|m| !m.description.is_empty()) {
        Some(m) => {
            c = c.child(text(m.description.clone()).font(TextStyle::new(15.5, 400.0).lh(26.0)).color(p.text.alpha(0.86)).wrap().max_w(680.0));
            // The first three genres already sit over the art; only the rest are news here.
            if m.genres.len() > 3 {
                let mut chips = row().gap(6.0).flex_wrap().pt(4.0);
                for genre in &m.genres[3..] {
                    chips = chips.child(row().h(26.0).px(11.0).radius(13.0).center().border(1.0, p.line)
                        .child(text(genre.clone()).font(ui(12.0, 550.0)).color(p.muted)));
                }
                c = c.child(chips);
            }
        }
        None => {
            let why = if !s.settings.online {
                "Les informations en ligne sont désactivées (Paramètres › En ligne)."
            } else if g.rom.is_some() {
                "Le nom, l'éditeur et l'icône viennent de la ROM ; GameTDB et libretro ne fournissent que les images."
            } else if s.fetching > 0 {
                "Recherche des informations sur le Steam Store…"
            } else {
                "Ce jeu n'a pas été trouvé sur le Steam Store."
            };
            c = c.child(text(why).font(ui(14.0, 450.0)).color(p.faint));
        }
    }
    c
}

fn card(p: &Palette, key: &str) -> El {
    col().key(key.to_string()).w_full().px(18.0).pt(16.0).pb(6.0).radius(10.0)
        .fill(p.surface.alpha(if p.dark { 0.55 } else { 1.0 })).border(1.0, p.line)
}

fn card_title(p: &Palette, title: &str) -> El {
    text(title.to_string()).font(caption()).color(p.faint).pb(4.0)
}

fn with_rules(p: &Palette, mut card: El, rows: Vec<El>) -> El {
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        card = card.child(r);
        if i + 1 < n {
            card = card.child(el().h(1.0).w_full().fill(p.line));
        }
    }
    card
}

/// What the game is: who made it, when, and how it's rated.
fn facts(s: &State, p: &Palette, g: &Game) -> El {
    let meta = s.meta(&g.id);
    let join = |v: &Vec<String>| if v.is_empty() { "—".to_string() } else { v.join(", ") };
    let mut rows: Vec<El> = Vec::new();
    if let Some(m) = meta {
        rows.push(fact(p, "Développeur", fact_text(p, join(&m.developers))));
        // A publisher that is also the developer says nothing new.
        if m.publishers != m.developers {
            rows.push(fact(p, "Éditeur", fact_text(p, join(&m.publishers))));
        }
        rows.push(fact(p, "Sortie", fact_text(p, if m.release.is_empty() { "—".into() } else { m.release.clone() })));
        if let Some(score) = m.metacritic {
            let hue = if score >= 75 { 145.0 } else if score >= 50 { 85.0 } else { 25.0 };
            rows.push(fact(p, "Metacritic", row().child(
                row().h(24.0).min_w(32.0).px(7.0).radius(4.0).center().fill(rune::oklch(0.62, 0.15, hue).alpha(0.18))
                    .border(1.0, rune::oklch(0.68, 0.15, hue).alpha(0.4))
                    .child(text(score.to_string()).font(num(12.5, 700.0)).color(rune::oklch(if p.dark { 0.8 } else { 0.5 }, 0.15, hue))))));
        }
    }
    rows.push(fact(p, "Store", row().gap(7.0).center_y().child(store_dot(g.source, 7.0)).child(fact_text(p, g.source.label()))));
    if let Some(code) = g.rom.as_ref().map(|r| r.code.clone()).filter(|c| !c.is_empty()) {
        rows.push(fact(p, "Code du jeu", fact_num(p, code).selectable(true)));
    }
    if let Some(appid) = g.steam_appid {
        rows.push(fact(p, "Steam ID", fact_num(p, appid.to_string()).selectable(true)));
    }
    let (edit, art) = (g.id.clone(), g.id.clone());
    let small = |key: &str, glyph: Icon, label: &str| {
        row().key(key.to_string()).role("button").label(label.to_string()).h(30.0).px(10.0).gap(7.0).radius(6.0).center_y()
            .border(1.0, p.line).hover(p.hover).press_scale(0.97)
            .child(icon(glyph, 14.0, p.muted))
            .child(text(label.to_string()).font(ui(12.5, 600.0)).color(p.text))
    };
    rows.push(row().key("fix").w_full().py(12.0).gap(8.0).flex_wrap()
        .child(small("edit-info", Icon::Pencil, "Modifier").on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_edit(&edit, tx)))
        .child(small("edit-art", Icon::Image, "Images").on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_picker(&art, Kind::Cover, tx))));
    let title = if s.user.edits.contains_key(&g.id) { "Informations · modifiées" } else { "Informations" };
    with_rules(p, card(p, "facts").child(card_title(p, title)), rows)
}

/// The same game from other stores: each can be played on its own, or taken apart again.
fn versions(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let others = s.versions.get(&g.id).filter(|v| !v.is_empty())?;
    let line = |v: &Game, main: bool| {
        let (play, split) = (v.id.clone(), v.id.clone());
        let where_ = v.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
        row().key(format!("version-{}", crate::app::fnv(&v.id))).w_full().min_h(52.0).py(8.0).gap(10.0).center_y()
            .child(store_dot(v.source, 8.0))
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(if main { format!("{} · principale", v.source.label()) } else { v.source.label().to_string() })
                    .font(ui(13.0, 600.0)).color(p.text))
                .children((!where_.is_empty()).then(|| text(where_).font(mono(11.0)).color(p.faint).truncate_middle())))
            .children((!main).then(|| row().key("split").role("button").label(format!("Séparer la version {}", v.source.label()))
                .help("En faire un jeu à part").h(28.0).px(9.0).radius(6.0).center().hover(p.hover)
                .child(text("Séparer").font(ui(12.0, 600.0)).color(p.muted))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.split_version(&split, tx))))
            .child(el().key("play").role("button").label(format!("Jouer avec {}", v.source.label())).help(format!("Jouer avec {}", v.source.label()))
                .size(30.0, 30.0).radius(15.0).center().fill(p.accent.alpha(0.14)).hover(p.accent.alpha(0.24)).press_scale(0.94)
                .child(icon(Icon::Play, 12.0, p.accent))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.launch_version(&play, tx)))
    };
    let mut rows = vec![line(g, true)];
    rows.extend(others.iter().map(|v| line(v, false)));
    Some(with_rules(p, card(p, "versions").child(card_title(p, "Versions")), rows))
}

/// Where the game lives: the drive, the whole folder path (selectable, never cut), and a way in.
fn location(s: &State, p: &Palette, g: &Game) -> Option<El> {
    let dir = g.dir.as_ref()?;
    let mut rows: Vec<El> = Vec::new();
    if let Some(d) = g.drive() {
        let label = s.drives.iter().find(|x| x.letter == d).map(|x| x.label.clone()).filter(|l| !l.is_empty());
        rows.push(fact(p, "Disque", fact_text(p, match label { Some(l) => format!("{d}  {l}"), None => d })));
    }
    if g.size > 0 {
        rows.push(fact(p, "Taille", fact_num(p, size(g.size))));
    }
    if g.installed > 0 {
        rows.push(fact(p, "Installé le", fact_text(p, date(g.installed))));
    }
    let folder = g.id.clone();
    rows.push(col().key("path").w_full().py(12.0).gap(10.0)
        .child(text(dir.display().to_string()).font(mono(12.0).lh(18.0)).color(p.muted).wrap().selectable(true))
        .child(row().key("open-folder").role("button").label("Ouvrir le dossier").h(32.0).px(10.0).gap(8.0).radius(6.0).center_y().self_start()
            .border(1.0, p.line)
            .hover(p.hover)
            .press_scale(0.97)
            .child(icon(Icon::Folder, 14.0, p.muted))
            .child(text("Ouvrir le dossier").font(ui(12.5, 600.0)).color(p.text))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_folder(&folder, tx))));
    Some(with_rules(p, card(p, "location").child(card_title(p, "Emplacement")), rows))
}

fn launch_options(s: &State, p: &Palette, g: &Game) -> El {
    let how = match &g.launch {
        Launch::Uri(_) if g.source == Source::Steam => "Transmises à Steam au lancement (comme dans Propriétés › Options de lancement).".to_string(),
        Launch::Uri(_) => format!("{} lance ce jeu lui-même : les arguments ne lui sont pas transmis.", g.source.label()),
        Launch::Exe { path, .. } => format!("Ajoutées après {} à chaque lancement.", path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()),
    };
    let field = text_input(&s.args, mono(13.0).lh(20.0))
        .key("args")
        .single_line()
        .grow(1.0)
        .color(p.text)
        .placeholder("-windowed -novid", p.faint)
        .label("Options de lancement")
        .on_edit(|s: &mut State| &mut s.args);
    let boxed = row().key("args-box").w_full().max_w(560.0).h(42.0).px(14.0).radius(6.0).center_y()
        .fill(p.field)
        .border(1.0, p.line)
        .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
        .child(field);
    col().key("launch").w_full().gap(12.0)
        .child(section_head(p, "Options de lancement", None))
        .child(boxed)
        .child(text(how).font(ui(12.5, 450.0)).color(p.faint).wrap().max_w(560.0))
}

/// Steam can verify and show a game's store page through its own links (uninstalling is in
/// its own card, for every store).
fn steam_tools(p: &Palette, g: &Game) -> Option<El> {
    if g.source != Source::Steam { return None; }
    let id = g.steam_appid?;
    let link = move |key: &str, label: &str, glyph: Icon, uri: String, danger: bool| {
        let ink = if danger { p.danger } else { p.text };
        row().key(key.to_string()).role("link").label(label.to_string()).w_full().h(40.0).px(10.0).gap(11.0).radius(6.0).center_y()
            .hover(if danger { p.danger.alpha(0.1) } else { p.hover })
            .press_scale(0.98)
            .child(icon(glyph, 16.0, if danger { p.danger } else { p.muted }))
            .child(text(label.to_string()).font(ui(13.0, 550.0)).color(ink).grow(1.0))
            .child(icon(Icon::External, 14.0, p.faint))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_uri(&uri, tx))
    };
    Some(col().key("steam-tools").w_full().px(8.0).pt(16.0).pb(8.0).radius(10.0)
        .fill(p.surface.alpha(if p.dark { 0.55 } else { 1.0 })).border(1.0, p.line)
        .child(row().px(10.0).child(card_title(p, "Steam")))
        .child(link("steam-validate", "Vérifier les fichiers", Icon::Check, format!("steam://validate/{id}"), false))
        .child(link("steam-store", "Page du store", Icon::Grid, format!("steam://store/{id}"), false)))
}

/// `13 nov. 2026`
pub fn date(ts: i64) -> String {
    const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    let days = ts.div_euclid(86_400);
    // Civil from days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates_read_in_french() {
        assert_eq!(super::date(0), "1 janv. 1970");
        assert_eq!(super::date(1_794_528_000), "13 nov. 2026");
    }
}
