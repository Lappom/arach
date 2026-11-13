//! Accueil: the game you were playing, front and centre; then shelves, then where it all lives.

use std::collections::BTreeMap;

use rune::{Color, El, SurfaceFit, Tx, col, el, row, text};

use super::icons::{Icon, icon};
use super::theme::{Palette, display, num, store_color, ui};
use super::widgets::{
    ago, cover_art, cover_card, hero_button, icon_button, link, play_button, playtime, plural, section_head, size,
};
use crate::app::{Page, Shelf, State};
use crate::game::{Game, Source};

const PAD: f32 = 40.0;
const HERO_H: f32 = 440.0;

pub fn view(s: &State, p: &Palette, w: f32) -> El {
    let Some(featured) = s.featured() else { return empty(p) };
    let (fit, cover_w) = super::widgets::cover_columns(w - PAD * 2.0, s.settings.cover_size.width(), 20.0);

    let mut page = col().key("home").w_full().pb(48.0).child(hero(s, p, featured, w));
    let mut shelves = col().w_full().px(PAD).gap(40.0).pt(8.0);

    let recent: Vec<&Game> = s.recent().into_iter().filter(|g| g.id != featured.id).collect();
    if !recent.is_empty() {
        shelves = shelves.child(shelf(s, p, "Joués récemment", &recent, fit, cover_w, Some(Shelf::All)));
    }
    let favs = s.favorites();
    if !favs.is_empty() {
        shelves = shelves.child(shelf(s, p, "Favoris", &favs, fit, cover_w, Some(Shelf::Favorites)));
    }
    let newest = s.newest();
    shelves = shelves.child(shelf(s, p, "Installés récemment", &newest, fit, cover_w, None));
    shelves = shelves.child(storage(s, p, w - PAD * 2.0));
    page = page.child(shelves);
    page
}

fn hero(s: &State, p: &Palette, g: &Game, w: f32) -> El {
    let id = g.id.clone();
    let mut art = el().key("hero-art").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0));
    match g.art.hero.as_ref().and_then(|h| s.images.borrow_mut().get(h, (w * 1.5).min(2400.0) as u32)) {
        Some(img) => art = art.pixels_fit(img.pixels(), SurfaceFit::Fill),
        None => {
            // No key art: the cover, blown up and dimmed, keeps the page from going grey.
            art = art.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).opacity(0.5)
                .child(cover_art(s, g, p, w, HERO_H)));
        }
    }
    let fade_down = el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .linear_gradient(std::f32::consts::PI, [(0.0, p.bg.alpha(0.25)), (0.45, p.bg.alpha(0.15)), (0.82, p.bg.alpha(0.88)), (1.0, p.bg)]);
    let fade_side = el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .linear_gradient(std::f32::consts::FRAC_PI_2, [(0.0, p.bg.alpha(0.78)), (0.5, p.bg.alpha(0.2)), (1.0, p.bg.alpha(0.0))]);

    let played = s.last_played(g);
    // What this is, said once and quietly: a frosted tag over the art, not tracked capitals.
    let (tag_glyph, tag) = if played > 0 {
        (Icon::Play, format!("Reprendre · {}", ago(played).to_lowercase()))
    } else {
        (Icon::Star, "À découvrir".to_string())
    };
    let badge = row().h(28.0).pl(9.0).pr(12.0).gap(7.0).radius(14.0).center_y()
        .fill(Color::BLACK.alpha(if p.dark { 0.35 } else { 0.08 }))
        .backdrop_blur(12.0)
        .border(1.0, p.text.alpha(0.1))
        .child(icon(tag_glyph, 11.0, p.text.alpha(0.9)))
        .child(text(tag).font(ui(12.5, 600.0)).color(p.text.alpha(0.9)));
    let mut facts = row().gap(14.0).center_y()
        .child(row().gap(7.0).center_y().child(super::widgets::store_dot(g.source, 7.0))
            .child(text(g.source.label()).font(ui(13.0, 550.0)).color(p.text.alpha(0.86))));
    if s.playtime_min(g) > 0 {
        facts = facts.child(dot(p)).child(text(format!("{} de jeu", playtime(s.playtime_min(g)))).font(num(13.0, 500.0)).color(p.text.alpha(0.86)));
    }
    if g.size > 0 {
        facts = facts.child(dot(p)).child(text(size(g.size)).font(num(13.0, 500.0)).color(p.text.alpha(0.86)));
    }
    let fav = s.is_favorite(&g.id);
    let (play_id, open_id, fav_id) = (id.clone(), id.clone(), id.clone());
    let buttons = row().gap(10.0).center_y()
        .child(play_button(p, "hero-play", "Jouer", true).on_press_tx(move |s: &mut State, tx: &mut Tx| s.launch(&play_id, tx)))
        .child(hero_button(p, "hero-open", "Voir la fiche", None)
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.go(Page::Game(open_id.clone()), tx)))
        .child(icon_button(p, "hero-fav", if fav { Icon::StarFilled } else { Icon::Star },
            if fav { "Retirer des favoris" } else { "Ajouter aux favoris" }, 46.0)
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.toggle_favorite(&fav_id, tx)));

    let content = col()
        .key("hero-content")
        .pin(Some(PAD), None, Some(PAD), Some(36.0))
        .gap(16.0)
        .child(row().child(badge))
        .child(title_block(s, p, g, 420.0, 64.0))
        .child(facts)
        .child(el().h(4.0))
        .child(buttons);

    el().key("hero").w_full().h(HERO_H).clip_rounded(0.0)
        .child(art)
        .child(fade_down)
        .child(fade_side)
        .child(content)
}

/// The game's logo when it has one, else its name set in the display face.
pub fn title_block(s: &State, p: &Palette, g: &Game, max_w: f32, size: f32) -> El {
    if let Some(logo) = &g.art.logo
        && let Some(img) = s.images.borrow_mut().get(logo, (max_w * 2.0) as u32)
    {
        if img.is_ready() {
            let [iw, ih] = img.size().unwrap_or([4, 1]).map(|v| v as f32);
            let h = (max_w * ih / iw).min(size * 2.0);
            let w = h * iw / ih;
            return el().key("logo").w(w).h(h).pixels_fit(img.pixels(), SurfaceFit::Fit);
        }
        if img.error().is_none() {
            return el().h(size * 1.4); // the logo is a moment away: hold its place
        }
    }
    text(g.name.to_uppercase()).key("title").font(display(size)).color(p.text).wrap().max_lines(2).max_w(max_w * 1.6)
}

fn dot(p: &Palette) -> El {
    el().size(3.0, 3.0).radius(1.5).fill(p.text.alpha(0.5))
}

fn shelf(s: &State, p: &Palette, title: &str, games: &[&Game], fit: usize, w: f32, all: Option<Shelf>) -> El {
    let more = (games.len() > fit).then_some(()).and(all).map(|shelf| {
        link(p, format!("more-{title}"), &format!("Tout voir · {}", games.len()))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_shelf(shelf, tx))
    });
    let mut row_el = row().w_full().gap(20.0);
    for g in games.iter().take(fit) {
        row_el = row_el.child(cover_card(s, g, p, w));
    }
    col().key(format!("shelf-{title}")).w_full().gap(16.0).child(section_head(p, title, more)).child(row_el)
}

// ---------------------------------------------------------------- storage

/// One card per drive that holds games: how full it is, and how much of that is which store.
fn storage(s: &State, p: &Palette, w: f32) -> El {
    let mut by_drive: BTreeMap<String, Vec<&Game>> = BTreeMap::new();
    for g in s.shown() {
        if let Some(d) = g.drive() {
            by_drive.entry(d).or_default().push(g);
        }
    }
    let cols = if w > 1000.0 { 3 } else if w > 640.0 { 2 } else { 1 };
    let card_w = ((w - 16.0 * (cols as f32 - 1.0)) / cols as f32).floor();
    let mut grid = row().w_full().gap(16.0).flex_wrap();
    for (letter, games) in &by_drive {
        grid = grid.child(drive_card(s, p, letter, games, card_w));
    }
    let total: u64 = by_drive.values().flatten().map(|g| g.size).sum();
    let head = text(format!("{} · {}", plural(by_drive.len(), "disque", "disques"), size(total)))
        .font(num(13.0, 500.0)).color(p.faint);
    col().key("storage").w_full().gap(16.0).child(section_head(p, "Stockage", Some(head))).child(grid)
}

fn drive_card(s: &State, p: &Palette, letter: &str, games: &[&Game], w: f32) -> El {
    let info = s.drives.iter().find(|d| d.letter == letter);
    let games_bytes: u64 = games.iter().map(|g| g.size).sum();
    let (total, free) = info.map_or((games_bytes.max(1), 0), |d| (d.total.max(1), d.free));
    let used = total.saturating_sub(free);
    let bar_w = w - 40.0;

    let mut per_store: BTreeMap<Source, (u64, usize)> = BTreeMap::new();
    for g in games {
        let e = per_store.entry(g.source).or_default();
        e.0 += g.size;
        e.1 += 1;
    }
    let mut bar = row().key("bar").w(bar_w).h(8.0).radius(4.0).clip_rounded(4.0).gap(2.0).fill(p.text.alpha(0.07));
    for (src, (bytes, _)) in &per_store {
        let seg = bar_w * (*bytes as f32 / total as f32);
        if seg >= 1.0 {
            bar = bar.child(el().w(seg).h_full().fill(store_color(*src)).border(0.0, Color::TRANSPARENT));
        }
    }
    let other = used.saturating_sub(games_bytes);
    bar = bar.child(el().w(bar_w * other as f32 / total as f32).h_full().fill(p.text.alpha(0.16)));

    let mut legend = row().w_full().gap(14.0).flex_wrap();
    for (src, (bytes, n)) in &per_store {
        legend = legend.child(
            row().gap(6.0).center_y()
                .child(super::widgets::store_dot(*src, 7.0))
                .child(text(src.label()).font(ui(12.0, 550.0)).color(p.muted))
                .child(text(if *bytes > 0 { size(*bytes) } else { format!("{n}") }).font(num(12.0, 500.0)).color(p.faint)),
        );
    }
    legend = legend.child(
        row().gap(6.0).center_y()
            .child(el().size(7.0, 7.0).radius(3.5).fill(p.text.alpha(0.16)))
            .child(text("Autres").font(ui(12.0, 550.0)).color(p.muted)),
    );

    let label = info.map(|d| d.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| "Disque local".into());
    let free_line = if info.is_some() { format!("{} libres sur {}", size(free), size(total)) } else { String::new() };
    let target = letter.chars().next().unwrap_or('C');
    col()
        .key(format!("drive-{letter}"))
        .role("button")
        .label(format!("Jeux du disque {letter}"))
        .w(w)
        .padding(20.0)
        .gap(14.0)
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.line)
        .press_scale(0.99)
        .child(
            row().w_full().gap(12.0).center_y()
                .child(text(letter.to_string()).font(display(34.0)).color(p.text))
                .child(col().gap(2.0).grow(1.0).min_w(0.0)
                    .child(text(label).font(ui(13.0, 600.0)).color(p.text).truncate())
                    .child(text(plural(games.len(), "jeu", "jeux")).font(ui(12.0, 500.0)).color(p.faint)))
                .child(icon(Icon::ChevronRight, 18.0, p.faint)),
        )
        .child(bar)
        .child(row().w_full().child(text(free_line).font(num(12.5, 500.0)).color(p.muted)).child(el().grow(1.0))
            .child(text(format!("{} de jeux", size(games_bytes))).font(num(12.5, 500.0)).color(p.faint)))
        .child(legend)
        .hover(p.hover)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.query.clear();
            s.open_shelf(Shelf::Drive(target), tx);
        })
}

fn empty(p: &Palette) -> El {
    col().key("home-empty").w_full().h(560.0).center().gap(10.0)
        .child(el().size(64.0, 64.0).radius(10.0).center().fill(p.text.alpha(0.06)).border(1.0, p.line).mb(8.0)
            .child(icon(Icon::Grid, 28.0, p.muted)))
        .child(text("Aucun jeu pour l'instant").font(ui(20.0, 650.0)).color(p.text))
        .child(text("Arach lit Steam, Epic, GOG, EA, Ubisoft, Riot et Battle.net sur tous vos disques.")
            .font(ui(14.0, 450.0)).color(p.muted))
        .child(text("Glissez un .exe sur la fenêtre, ou ajoutez un dossier de jeux dans Paramètres › Bibliothèque.")
            .font(ui(13.0, 450.0)).color(p.faint))
}
