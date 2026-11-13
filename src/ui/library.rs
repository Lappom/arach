//! Bibliothèque: every game, as box art or as a table, by store, drive or favourites.

use rune::{Color, El, Tx, col, el, row, text};

use super::icons::{Icon, icon};
use super::overlay::select;
use super::theme::{Palette, caption, display, num, ui};
use super::widgets::{Tone, ago, button, check_box, context_menu, cover_card, game_icon, playtime, plural, size, status_dot, store_dot};
use crate::app::{Confirm, Filter, Overlay, Played, Shelf, State, fnv};
use crate::game::{Game, Source};
use crate::settings::{LibraryView, Sort};
use crate::store::Status;

const PAD: f32 = 40.0;

const SORTS: [(Sort, &str); 5] = [
    (Sort::Recent, "Récents"),
    (Sort::Name, "Nom"),
    (Sort::Size, "Taille"),
    (Sort::Playtime, "Temps de jeu"),
    (Sort::Rating, "Note"),
];


pub fn view(s: &State, p: &Palette, w: f32, layer: &mut Vec<El>) -> El {
    let searching = !s.query.text.trim().is_empty();
    let title = if searching {
        format!("« {} »", s.query.text.trim())
    } else {
        match s.shelf {
            Shelf::All => "Bibliothèque".into(),
            Shelf::Favorites => "Favoris".into(),
            Shelf::Store(src) => src.label().into(),
            Shelf::Drive(c) => format!("Disque {c}:"),
            Shelf::Collection(id) => s.collection(id).map_or_else(|| "Collection".into(), |c| c.name.clone()),
            Shelf::NotInstalled => "À installer".into(),
        }
    };
    let sub = if searching { format!("{} pour cette recherche", plural(s.visible.len(), "résultat", "résultats")) } else { plural(s.visible.len(), "jeu", "jeux") };
    let head = col().w_full().gap(6.0)
        .child(text(title).font(display(42.0)).color(p.text).truncate().min_w(0.0))
        .child(text(sub).font(num(13.5, 500.0)).color(p.faint));
    // A collection's own page: rename or delete it from here.
    let head = match s.shelf {
        Shelf::Collection(id) if !searching => row().w_full().gap(8.0).items_end()
            .child(head.grow(1.0).min_w(0.0))
            .child(button(p, "collection-rename", "Renommer", None, Tone::Secondary)
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.name_collection(Some(id), None, tx)))
            .child(button(p, "collection-delete", "Supprimer", Some(Icon::Trash), Tone::Secondary)
                .on_press(move |s: &mut State| s.overlay = Overlay::Confirm(Confirm::DeleteCollection(id)))),
        _ => head,
    };

    let sort = select("sort", "Trier la bibliothèque", &SORTS, s.settings.sort)
        .prefix("Trier par")
        .width(206.0)
        .build(s, p, layer, |s, v, tx| s.set(tx, |st| st.sort = v));
    let view_i = match s.settings.library_view { LibraryView::Grid => 0, LibraryView::List => 1 };
    let toolbar = row().w_full().gap(10.0).center_y()
        .child(shelves(s, p))
        .child(el().grow(1.0))
        .child(filter_button(s, p))
        .child(select_button(s, p))
        .child(sort)
        .child(view_toggle(p, view_i));
    super::overlay::filters(s, p, layer);
    if s.selecting {
        layer.push(super::overlay::selection_bar(s, p));
        super::overlay::bulk_menus(s, p, layer);
    }

    let body = if s.visible.is_empty() {
        empty(s, p)
    } else {
        match s.settings.library_view {
            LibraryView::Grid => grid(s, p, w - PAD * 2.0),
            LibraryView::List => list(s, p, w - PAD * 2.0),
        }
    };
    col().key("library").w_full().px(PAD).pt(32.0).pb(if s.selecting { 120.0 } else { 56.0 }).gap(24.0)
        .child(head)
        .child(toolbar)
        .children(active_filters(s, p))
        .child(body)
}

/// Opens the filter panel; says how many filters hold.
fn filter_button(s: &State, p: &Palette) -> El {
    let n = s.filter.count();
    let open = s.overlay == Overlay::Filters;
    row().key("filter-button").role("button").label("Filtres").expanded(open).h(34.0).px(12.0).gap(7.0).radius(5.0).center_y()
        .fill(if n > 0 { p.accent.alpha(0.12) } else { p.field })
        .border(1.0, if n > 0 || open { p.accent.alpha(0.6) } else { p.line })
        .hover(p.hover)
        .press_scale(0.97)
        .child(icon(Icon::Filter, 15.0, if n > 0 { p.accent } else { p.muted }))
        .child(text("Filtres").font(ui(13.0, 600.0)).color(p.text))
        .children((n > 0).then(|| row().h(18.0).min_w(18.0).px(5.0).radius(9.0).center().fill(p.accent)
            .child(text(n.to_string()).font(num(11.0, 700.0)).color(p.on_accent))))
        .on_press(|s: &mut State| s.toggle_overlay(Overlay::Filters))
}

/// Enters or leaves selection mode (Ctrl+clic on a game does too).
fn select_button(s: &State, p: &Palette) -> El {
    let on = s.selecting;
    row().key("select-button").role("button").label("Sélectionner").help("Sélectionner plusieurs jeux (Ctrl+clic)").checked(on)
        .h(34.0).px(12.0).gap(7.0).radius(5.0).center_y()
        .fill(if on { p.text } else { p.field })
        .border(1.0, if on { p.text } else { p.line })
        .hover(if on { Color::TRANSPARENT } else { p.hover })
        .press_scale(0.97)
        .child(icon(Icon::Check, 15.0, if on { p.bg } else { p.muted }))
        .child(text("Sélectionner").font(ui(13.0, 600.0)).color(if on { p.bg } else { p.text }))
        .on_press_tx(|s: &mut State, tx: &mut Tx| if s.selecting { tx.instant(); s.end_selection() } else { s.start_selection(tx) })
}

/// The filters in force, each a chip that takes it off.
fn active_filters(s: &State, p: &Palette) -> Option<El> {
    let f = &s.filter;
    if f.is_empty() { return None; }
    let chip = |key: String, label: String, off: Box<dyn Fn(&mut Filter)>| {
        row().key(key).role("button").label(format!("Retirer le filtre {label}")).h(28.0).pl(11.0).pr(7.0).gap(6.0).radius(14.0).center_y()
            .fill(p.accent.alpha(0.12))
            .border(1.0, p.accent.alpha(0.45))
            .hover(p.accent.alpha(0.2))
            .press_scale(0.97)
            .child(text(label).font(ui(12.5, 600.0)).color(p.text))
            .child(icon(Icon::Close, 12.0, p.muted))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.change_filter(tx, |f| off(f)))
    };
    let mut row_el = row().key("active-filters").w_full().gap(6.0).center_y().flex_wrap();
    for st in &f.statuses {
        let st = *st;
        row_el = row_el.child(chip(format!("af-st-{st:?}"), st.label().into(), Box::new(move |f| f.statuses.retain(|x| *x != st))));
    }
    if f.played != Played::Any {
        row_el = row_el.child(chip("af-played".into(), f.played.label().into(), Box::new(|f| f.played = Played::Any)));
    }
    for g in &f.genres {
        let g2 = g.clone();
        row_el = row_el.child(chip(format!("af-g-{g}"), g.clone(), Box::new(move |f| f.genres.retain(|x| *x != g2))));
    }
    for y in &f.years {
        let y = *y;
        row_el = row_el.child(chip(format!("af-y-{y}"), y.to_string(), Box::new(move |f| f.years.retain(|x| *x != y))));
    }
    for c in &f.collections {
        let c = *c;
        let name = s.collection(c).map_or_else(|| "Collection".to_string(), |c| c.name.clone());
        row_el = row_el.child(chip(format!("af-c-{c:016x}"), name, Box::new(move |f| f.collections.retain(|x| *x != c))));
    }
    row_el = row_el.child(
        row().key("af-clear").role("button").label("Effacer les filtres").h(28.0).px(10.0).radius(14.0).center_y().hover(p.hover)
            .child(text("Tout effacer").font(ui(12.5, 600.0)).color(p.muted))
            .on_press_tx(|s: &mut State, tx: &mut Tx| s.change_filter(tx, |f| *f = Filter::default())),
    );
    Some(row_el)
}

/// Chips for the shelves: all, favourites, then each store present, each with its count.
fn shelves(s: &State, p: &Palette) -> El {
    let mut chips = row().gap(6.0).flex_wrap().shrink().min_w(0.0);
    let add = |chips: El, key: String, label: String, dot: Option<Source>, n: Option<usize>, shelf: Shelf| {
        let on = s.shelf == shelf;
        chips.child(
            row().key(key).role("radio").checked(on).label(label.clone()).h(32.0).pl(if dot.is_some() { 11.0 } else { 13.0 }).pr(13.0)
                .gap(7.0).radius(16.0).center_y()
                .fill(if on { p.text } else { Color::TRANSPARENT })
                .border(1.0, if on { p.text } else { p.line })
                .hover(if on { Color::TRANSPARENT } else { p.hover })
                .press_scale(0.97)
                .children(dot.map(|d| store_dot(d, 7.0)))
                .child(text(label).font(ui(13.0, 600.0)).color(if on { p.bg } else { p.muted }))
                .children(n.map(|n| text(n.to_string()).font(num(12.0, 500.0)).color(if on { p.bg.alpha(0.6) } else { p.faint })))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_shelf(shelf, tx)),
        )
    };
    let total = s.shown().count();
    chips = add(chips, "chip-all".into(), "Tous".into(), None, Some(total), Shelf::All);
    chips = add(chips, "chip-fav".into(), "Favoris".into(), None, Some(s.favorites().len()), Shelf::Favorites);
    for src in Source::ALL {
        let n = s.count(src);
        if n > 0 {
            chips = add(chips, format!("chip-{src:?}"), src.label().into(), Some(src), Some(n), Shelf::Store(src));
        }
    }
    for c in &s.user.collections {
        let n = s.shelf_count(Shelf::Collection(c.id));
        chips = add(chips, format!("chip-coll-{:016x}", c.id), c.name.clone(), None, Some(n), Shelf::Collection(c.id));
    }
    let to_install = s.shelf_count(Shelf::NotInstalled);
    if to_install > 0 {
        chips = add(chips, "chip-owned".into(), "À installer".into(), None, Some(to_install), Shelf::NotInstalled);
    }
    if let Shelf::Drive(c) = s.shelf {
        chips = add(chips, "chip-drive".into(), format!("Disque {c}:"), None, None, Shelf::Drive(c));
    }
    chips.role("radiogroup").label("Étagère")
}

fn view_toggle(p: &Palette, selected: usize) -> El {
    let item = |i: usize, glyph: Icon, label: &'static str| {
        let on = i == selected;
        el().key(format!("view-{i}")).role("radio").label(label).checked(on).help(label).size(32.0, 28.0).radius(4.0).center()
            .fill(if on { if p.dark { p.raised } else { p.surface } } else { Color::TRANSPARENT })
            .shadow(1.0, 3.0, p.shadow.alpha(if on { if p.dark { 0.35 } else { 0.12 } } else { 0.0 }))
            .hover(if on { Color::TRANSPARENT } else { p.hover })
            .child(icon(glyph, 16.0, if on { p.text } else { p.faint }))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                tx.instant();
                s.set(tx, |st| st.library_view = if i == 0 { LibraryView::Grid } else { LibraryView::List });
            })
    };
    row().key("view").role("radiogroup").label("Affichage").focus_group().h(34.0).padding(3.0).gap(2.0).radius(6.0)
        .fill(p.text.alpha(if p.dark { 0.06 } else { 0.05 }))
        .border(1.0, p.line)
        .child(item(0, Icon::Grid, "Jaquettes"))
        .child(item(1, Icon::List, "Liste"))
}

fn grid(s: &State, p: &Palette, avail: f32) -> El {
    let (_, w) = super::widgets::cover_columns(avail, s.settings.cover_size.width(), 24.0);
    let mut g = row().key("grid").w_full().gaps(30.0, 24.0).flex_wrap();
    for &i in &s.visible {
        g = g.child(cover_card(s, &s.games[i], p, w));
    }
    g
}

fn list(s: &State, p: &Palette, w: f32) -> El {
    let wide = w > 760.0;
    let header = row().w_full().h(32.0).px(14.0).gap(16.0).center_y()
        .child(el().w(36.0))
        .child(col_label(p, "Jeu").grow(1.0))
        .children(wide.then(|| col_label(p, "Temps de jeu").w(110.0)))
        .child(col_label(p, "Dernière session").w(140.0))
        .child(col_label(p, "Taille").w(84.0))
        .child(el().w(84.0));
    let mut rows = col().key("rows").role("list").w_full().gap(2.0);
    for &i in &s.visible {
        rows = rows.child(list_row(s, p, &s.games[i], wide));
    }
    col().w_full().radius(8.0).fill(p.surface.alpha(if p.dark { 0.6 } else { 1.0 })).border(1.0, p.line).padding(6.0)
        .child(header)
        .child(el().h(1.0).mx(-6.0).mb(4.0).fill(p.line))
        .child(rows)
}

fn col_label(p: &Palette, label: &str) -> El {
    text(label.to_string()).font(caption()).color(p.faint)
}

fn list_row(s: &State, p: &Palette, g: &Game, wide: bool) -> El {
    let (open, play, drag) = (g.id.clone(), g.id.clone(), g.id.clone());
    // Owned but not installed: always showing, as the row's one action.
    let play_btn = if g.not_installed {
        row().key("play").role("button").label(format!("Installer avec {}", g.source.label())).h(30.0).px(13.0).gap(6.0).radius(15.0).center()
            .border(1.0, p.line).hover(p.hover).press_scale(0.95)
            .child(text("Installer").font(ui(12.5, 650.0)).color(p.text))
    } else {
        row().key("play").role("button").label("Jouer").h(30.0).px(13.0).gap(6.0).radius(15.0).center()
            .fill(p.accent).group_opacity(0.0, 1.0).press_scale(0.95)
            .child(icon(Icon::Play, 11.0, p.on_accent))
            .child(text("Jouer").font(ui(12.5, 650.0)).color(p.on_accent))
    }
    .on_press_tx(move |s: &mut State, tx: &mut Tx| s.launch(&play, tx));
    let played = s.last_played(g);
    let minutes = s.playtime_min(g);
    let entry = s.user.entry(&g.id);
    let dim = if g.not_installed { 0.5 } else { 1.0 };
    row()
        .key(format!("row-{:016x}", fnv(&g.id)))
        .role("button")
        .label(g.name.clone())
        .w_full()
        .h(56.0)
        .px(14.0)
        .gap(16.0)
        .radius(6.0)
        .center_y()
        .hover(p.hover)
        .hover_group()
        .fill(if s.is_selected(&g.id) { p.accent.alpha(0.1) } else { Color::TRANSPARENT })
        .children(s.selecting.then(|| check_box(p, s.is_selected(&g.id))))
        .child(game_icon(s, g, p, 36.0).opacity(dim))
        .child(col().grow(1.0).min_w(0.0).gap(3.0)
            .child(row().gap(8.0).center_y()
                .child(text(g.name.clone()).font(ui(14.0, 600.0)).color(if g.not_installed { p.muted } else { p.text }).truncate().min_w(0.0))
                .children(s.is_favorite(&g.id).then(|| icon(Icon::StarFilled, 12.0, rune::oklch(0.86, 0.15, 85.0))))
                .children(s.playing.contains_key(&g.id).then(|| text("EN JEU").font(ui(10.5, 700.0)).color(p.accent)))
                .children((g.update_pending && !g.not_installed).then(|| text("MISE À JOUR").font(ui(10.5, 700.0)).color(rune::oklch(0.72, 0.14, 230.0))
                    .help("Steam a une mise à jour à installer pour ce jeu".to_string()))))
            .child(row().gap(6.0).center_y().child(store_dot(g.source, 6.0))
                .child(text(g.source.label()).font(ui(12.0, 500.0)).color(p.faint))
                .children(g.not_installed.then(|| text("·  non installé").font(ui(12.0, 500.0)).color(p.faint)))
                .children(g.drive().map(|d| text(format!("·  {d}")).font(num(12.0, 500.0)).color(p.faint)))
                .children((entry.status != Status::NotStarted).then(|| row().gap(5.0).center_y().pl(4.0)
                    .child(status_dot(p, entry.status, 6.0))
                    .child(text(entry.status.label()).font(ui(12.0, 500.0)).color(p.muted))))
                .children((entry.rating > 0).then(|| text(format!("·  {} ★", entry.rating)).font(num(12.0, 500.0)).color(p.faint)))))
        .children(wide.then(|| text(playtime(minutes)).font(num(13.0, 500.0)).color(if minutes > 0 { p.text } else { p.faint }).w(110.0)))
        .child(text(ago(played)).font(ui(13.0, 500.0)).color(if played > 0 { p.muted } else { p.faint }).w(140.0).truncate())
        .child(text(size(g.size)).font(num(13.0, 500.0)).color(p.muted).w(84.0))
        .child(row().w(84.0).justify_end().child(play_btn))
        .on_press_tx(move |s: &mut State, tx: &mut Tx| super::widgets::open_or_select(s, &open, tx))
        .on_drag(move |s: &mut State, d, tx| s.drag_game(&drag, d, tx))
        .on_secondary_press(context_menu(g.id.clone()))
}

fn empty(s: &State, p: &Palette) -> El {
    let (glyph, title, detail) = if !s.query.text.trim().is_empty() {
        (Icon::Search, "Aucun jeu ne correspond", "Essayez les initiales (« kcd ») ou une partie du nom. Échap efface la recherche.")
    } else if !s.filter.is_empty() {
        (Icon::Filter, "Aucun jeu ne passe ces filtres", "Retirez-en un, ou « Tout effacer » au-dessus.")
    } else if s.shelf == Shelf::Favorites {
        (Icon::Star, "Pas encore de favoris", "Ajoutez-en avec l'étoile sur la fiche d'un jeu, ou par clic droit sur sa jaquette.")
    } else if let Shelf::Collection(_) = s.shelf {
        (Icon::List, "Collection vide", "Glissez une jaquette sur son nom dans la barre latérale, ou ajoutez un jeu par clic droit.")
    } else {
        (Icon::Grid, "Rien ici", "Relancez la détection avec F5, ou glissez un .exe sur la fenêtre.")
    };
    col().key("library-empty").w_full().h(360.0).center().gap(10.0)
        .child(el().size(56.0, 56.0).radius(10.0).center().fill(p.text.alpha(0.06)).border(1.0, p.line).mb(6.0)
            .child(icon(glyph, 24.0, p.muted)))
        .child(text(title).font(ui(17.0, 650.0)).color(p.text))
        .child(text(detail).font(ui(13.5, 450.0)).color(p.muted).wrap().max_w(420.0).align(rune::Align::Center))
}
