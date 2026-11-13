//! The window: a sidebar for where you are, a top bar for search, and the page.

mod detail;
mod extras;
mod home;
mod icons;
mod library;
mod overlay;
mod settings;
pub mod theme;
mod widgets;

use rune::{Color, Cx, El, Key, KeyEvent, PointerButton, ScrollSpec, Spring, SpringPair, TextStyle, Transition, Tx, col, el, row, text, text_input};

use crate::app::{DropTarget, Overlay, Page, Shelf, State, Tab};
use crate::game::Source;
use icons::{Icon, icon};
use theme::{Palette, caption, display, num, palette, ui};
use widgets::{keycaps, plural, store_dot, tool_button};
pub use widgets::playtime;

pub const TOPBAR: f32 = 48.0;
const SIDEBAR: f32 = 236.0;

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let p = palette(&s.settings, cx.system_dark());
    cx.caption_color(p.text);
    let insets = cx.chrome_insets();
    let main_w = cx.layout_size("main").map_or(900.0, |sz| sz[0]);

    // Overlays the page opens (dropdowns) land here and mount above everything else.
    let mut layer: Vec<El> = Vec::new();
    let page = match &s.page {
        Page::Home => home::view(s, &p, main_w),
        Page::Library => library::view(s, &p, main_w, &mut layer),
        Page::Game(id) => detail::view(s, &p, id, main_w, &mut layer),
        Page::Settings(tab) => settings::view(s, &p, *tab, main_w, &mut layer),
    };
    overlay::game_menu(s, &p, &mut layer);
    overlay::confirm(s, &p, &mut layer);
    overlay::name_collection(s, &p, &mut layer);
    overlay::edit_game(s, &p, &mut layer);
    overlay::art_picker(s, &p, &mut layer);
    record_drop_targets(s, cx);
    let main = col()
        .key("main")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .fill(p.bg)
        .child(
            col().key("page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(page),
        )
        .child(topbar(s, &p, insets.trailing));

    let mut root = row()
        .key("root")
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(sidebar(s, &p))
        .child(main)
        .on_drop_files(|s: &mut State, files, tx| {
            s.drop_over = false;
            s.add_paths(files, tx);
        })
        .on_drop_hover(|s: &mut State, over, _| s.drop_over = over)
        // The mouse's side button goes back, as in a browser. Buttons and cards take only the
        // primary, so the press climbs to here; a primary press here does nothing.
        .pointer_buttons([PointerButton::Back])
        .on_press_tx(|s: &mut State, tx: &mut Tx| {
            if tx.button() == PointerButton::Back && s.can_go_back() { s.back(tx) }
        });
    if s.drop_over {
        root = root.child(drop_veil(&p, matches!(s.page, Page::Game(_))));
    }
    if let Some(n) = &s.notice {
        root = root.child(toast(&n.text, n.error, &p));
    }
    if let Some(g) = drag_ghost(s, &p) {
        root = root.child(g);
    }
    root.children(layer)
}

// ---------------------------------------------------------------- top bar

fn topbar(s: &State, p: &Palette, trailing: f32) -> El {
    let over_art = matches!(s.page, Page::Home | Page::Game(_));
    let field = text_input(&s.query, TextStyle::new(13.5, 450.0).lh(20.0))
        .key("search")
        .single_line()
        .grow(1.0)
        .color(p.text)
        .placeholder("Rechercher dans la bibliothèque", p.faint)
        .label("Rechercher un jeu")
        .on_edit(|s: &mut State| &mut s.query)
        .on_input_change(|s: &mut State, tx| s.searched(tx))
        .on_input_key(keys)
        .on_submit(|s: &mut State, tx| {
            if let Some(&i) = s.visible.first() {
                let id = s.games[i].id.clone();
                s.launch(&id, tx);
            }
        });
    // The hint while empty, a way to clear once typed in.
    let end = if s.query.text.is_empty() {
        keycaps(p, "Ctrl K")
    } else {
        el().key("search-clear").role("button").label("Effacer la recherche").help("Effacer (Échap)")
            .size(22.0, 22.0).radius(11.0).center().fill(p.text.alpha(0.1)).hover(p.text.alpha(0.08)).press_scale(0.9)
            .child(icon(Icon::Close, 12.0, p.muted))
            .on_press_tx(|s: &mut State, tx: &mut Tx| {
                s.query.clear();
                s.refilter();
                tx.instant();
                tx.focus("search", false);
            })
    };
    let search = row()
        .key("search-box")
        .h(36.0)
        .w(440.0)
        .max_w(520.0)
        .shrink()
        .min_w(180.0)
        .pl(12.0)
        .pr(7.0)
        .gap(10.0)
        .radius(6.0)
        .center_y()
        .fill(if over_art { Color::BLACK.alpha(if p.dark { 0.42 } else { 0.08 }) } else { p.field })
        .backdrop_blur(if over_art { 16.0 } else { 0.0 })
        .border(1.0, if over_art { Color::WHITE.alpha(0.1) } else { p.line })
        .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
        .child(icon(Icon::Search, 16.0, p.faint))
        .child(field)
        .child(end);
    let back = s.can_go_back().then(|| {
        el().key("back").role("button").label("Retour").help("Retour (Alt+←)").size(36.0, 36.0).radius(6.0).center()
            .fill(if over_art { Color::BLACK.alpha(if p.dark { 0.42 } else { 0.08 }) } else { p.field })
            .backdrop_blur(if over_art { 16.0 } else { 0.0 })
            .border(1.0, if over_art { Color::WHITE.alpha(0.1) } else { p.line })
            .hover(p.hover)
            .press_scale(0.94)
            .child(icon(Icon::Back, 16.0, p.text))
            .on_press_tx(|s: &mut State, tx: &mut Tx| s.back(tx))
    });
    row()
        .key("topbar")
        .pin(Some(0.0), Some(0.0), Some(0.0), None)
        .h(TOPBAR)
        .pl(24.0)
        .pr(trailing + 12.0)
        .gap(10.0)
        .center_y()
        .window_drag()
        .gradient(p.bg.alpha(if over_art { 0.0 } else { 0.96 }), p.bg.alpha(if over_art { 0.0 } else { 0.86 }))
        .children(back)
        .child(search)
}

// ---------------------------------------------------------------- sidebar

fn sidebar(s: &State, p: &Palette) -> El {
    let on_page = |page: &Page| &s.page == page;
    let lib_on = |shelf: Shelf| s.page == Page::Library && s.shelf == shelf;
    let total = s.shown().count();
    let favs = s.favorites().len();
    // While a game is dragged: where it would land lights up, the other targets hint.
    let dragging = s.dragging.as_ref().is_some_and(|d| d.active);
    let over = s.dragging.as_ref().filter(|d| d.active).and_then(|d| s.drop_target_at(d.at.0, d.at.1));
    let target = |el: El, t: DropTarget| -> El {
        if !dragging { return el; }
        if over == Some(t) { el.fill(p.accent.alpha(0.16)).border(1.0, p.accent.alpha(0.7)) } else { el.border(1.0, p.line) }
    };

    let mut nav = col().gap(2.0)
        .child(nav_item(p, "nav-home", Icon::Home, "Accueil", None, on_page(&Page::Home))
            .on_press_tx(|s: &mut State, tx: &mut Tx| s.go(Page::Home, tx)))
        .child(nav_item(p, "nav-library", Icon::Grid, "Bibliothèque", Some(total), lib_on(Shelf::All))
            .on_press_tx(|s: &mut State, tx: &mut Tx| { s.query.clear(); s.open_shelf(Shelf::All, tx) }))
        .child(target(nav_item(p, "nav-favorites", Icon::Star, "Favoris", Some(favs), lib_on(Shelf::Favorites)), DropTarget::Favorites)
            .on_press_tx(|s: &mut State, tx: &mut Tx| { s.query.clear(); s.open_shelf(Shelf::Favorites, tx) }));

    let mut collections = col().gap(1.0);
    for c in &s.user.collections {
        let (id, on) = (c.id, lib_on(Shelf::Collection(c.id)));
        let n = s.shelf_count(Shelf::Collection(id));
        let item = row().key(collection_key(id)).role("button").label(c.name.clone()).h(32.0).px(10.0).gap(11.0).radius(5.0).center_y()
            .fill(if on { p.text.alpha(if p.dark { 0.075 } else { 0.06 }) } else { Color::TRANSPARENT })
            .hover(p.hover)
            .child(el().size(18.0, 18.0).center().child(icon(Icon::List, 15.0, if on { p.accent } else { p.faint })))
            .child(text(c.name.clone()).font(ui(13.0, if on { 600.0 } else { 500.0 })).color(if on { p.text } else { p.muted }).grow(1.0).truncate().min_w(0.0))
            .child(text(n.to_string()).font(num(12.0, 500.0)).color(p.faint))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| { s.query.clear(); s.open_shelf(Shelf::Collection(id), tx) });
        collections = collections.child(target(item, DropTarget::Collection(id)));
    }
    if s.user.collections.is_empty() {
        collections = collections.child(
            row().key("nav-collection-first").role("button").label("Nouvelle collection").h(32.0).px(10.0).gap(11.0).radius(5.0).center_y()
                .hover(p.hover)
                .child(el().size(18.0, 18.0).center().child(icon(Icon::Plus, 15.0, p.faint)))
                .child(text("Nouvelle collection").font(ui(13.0, 500.0)).color(p.faint).grow(1.0))
                .on_press_tx(|s: &mut State, tx: &mut Tx| s.name_collection(None, None, tx)),
        );
    }
    let add = tool_button(p, "nav-collection-add", Icon::Plus, "Nouvelle collection", 24.0)
        .on_press_tx(|s: &mut State, tx: &mut Tx| s.name_collection(None, None, tx));
    nav = nav.child(el().h(20.0))
        .child(row().pl(10.0).pr(4.0).h(26.0).center_y().child(text("Collections").font(caption()).color(p.faint).grow(1.0)).child(add))
        .child(collections);

    let mut stores = col().gap(1.0);
    for src in Source::ALL {
        let n = s.count(src);
        if n == 0 { continue; }
        stores = stores.child(
            row().key(format!("nav-store-{src:?}")).role("button").label(src.label()).h(32.0).px(10.0).gap(11.0).radius(5.0).center_y()
                .fill(if lib_on(Shelf::Store(src)) { p.text.alpha(if p.dark { 0.075 } else { 0.06 }) } else { Color::TRANSPARENT })
                .hover(p.hover)
                .child(el().size(18.0, 18.0).center().child(store_dot(src, 7.0)))
                .child(text(src.label()).font(ui(13.0, if lib_on(Shelf::Store(src)) { 600.0 } else { 500.0 }))
                    .color(if lib_on(Shelf::Store(src)) { p.text } else { p.muted }).grow(1.0))
                .child(text(n.to_string()).font(num(12.0, 500.0)).color(p.faint))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| { s.query.clear(); s.open_shelf(Shelf::Store(src), tx) }),
        );
    }
    nav = nav.child(el().h(20.0)).child(row().px(10.0).h(26.0).center_y().child(text("Stores").font(caption()).color(p.faint))).child(stores);

    let status = if s.scanning {
        "Analyse des disques…".to_string()
    } else if s.fetching > 0 {
        format!("Jaquettes : {} restantes", s.fetching)
    } else if s.user.last_scan > 0 {
        format!("{} · {}", plural(total, "jeu", "jeux"), widgets::ago(s.user.last_scan).to_lowercase())
    } else {
        plural(total, "jeu", "jeux")
    };
    let footer = col().gap(2.0)
        .child(row().h(34.0).px(10.0).gap(8.0).center_y()
            .child(el().size(6.0, 6.0).radius(3.0).fill(if s.scanning || s.fetching > 0 { p.accent } else { p.faint.alpha(0.6) })
                .cycle_if(s.scanning || s.fetching > 0))
            .child(text(status).font(ui(11.5, 500.0)).color(p.faint).truncate().grow(1.0).min_w(0.0))
            .child(tool_button(p, "rescan", Icon::Refresh, "Relancer la détection (F5)", 28.0)
                .on_press_tx(|s: &mut State, _: &mut Tx| s.rescan())))
        .children(update_ready(s, p))
        .child(nav_item(p, "nav-settings", Icon::Settings, "Paramètres", None, matches!(s.page, Page::Settings(_)))
            .on_press_tx(|s: &mut State, tx: &mut Tx| s.go(Page::Settings(Tab::General), tx)));

    col()
        .key("sidebar")
        .w(SIDEBAR)
        .h_full()
        .fill(p.side)
        .px(12.0)
        .pb(12.0)
        .child(
            row().key("brand").h(TOPBAR + 8.0).px(10.0).gap(10.0).center_y().window_drag()
                .child(brand_mark(s, p))
                .child(text("ARACH").font(display(24.0)).color(p.text)),
        )
        .child(col().key("nav").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(nav))
        .children(running(s, p))
        .child(el().h(1.0).w_full().fill(p.line).my(8.0))
        .child(footer)
        .child(el().pin(None, Some(0.0), Some(0.0), Some(0.0)).w(1.0).fill(p.line))
}

/// A downloaded update, one press from the page that installs it.
fn update_ready(s: &State, p: &Palette) -> Option<El> {
    let crate::app::Update::Ready { release, .. } = &s.update else { return None };
    let label = format!("Arach {} est prêt", release.version);
    Some(
        row().key("nav-update").role("button").label(label.clone()).help("Paramètres › À propos").h(34.0).px(10.0).gap(11.0).radius(5.0).center_y()
            .fill(p.accent.alpha(0.14))
            .hover(p.accent.alpha(0.22))
            .press_scale(0.98)
            .child(el().size(18.0, 18.0).center().child(icon(Icon::Download, 15.0, p.accent)))
            .child(text(label).font(ui(13.0, 600.0)).color(p.text).grow(1.0).truncate().min_w(0.0))
            .on_press_tx(|s: &mut State, tx: &mut Tx| s.go(Page::Settings(Tab::About), tx)),
    )
}

/// Games launched from Arach and still open, above the footer: a live dot, how long it has
/// run, and a button to end it. The name opens its page.
fn running(s: &State, p: &Palette) -> Option<El> {
    let games = s.running();
    if games.is_empty() { return None; }
    let mut list = col().key("running").w_full().gap(4.0).pt(8.0)
        .child(row().px(10.0).h(22.0).center_y().child(text("En cours").font(caption()).color(p.faint)));
    for (g, start) in games {
        let stopping = s.stopping.contains(&g.id);
        let (status, live) = match start {
            _ if stopping => ("Arrêt…".to_string(), false),
            Some(t) => {
                let min = ((crate::store::now() - t).max(0) / 60) as u64;
                (if min == 0 { "En jeu".to_string() } else { format!("En jeu · {}", playtime(min)) }, true)
            }
            None => ("Démarrage…".to_string(), false),
        };
        let (open, stop) = (g.id.clone(), g.id.clone());
        let key = crate::app::fnv(&g.id);
        let name = row().key(format!("running-open-{key:016x}")).role("button").label(format!("{} · {status}", g.name))
            .grow(1.0).min_w(0.0).h(44.0).pl(6.0).gap(10.0).radius(6.0).center_y()
            .hover(p.hover)
            .child(widgets::game_icon(s, g, p, 28.0))
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(g.name.clone()).font(ui(13.0, 600.0)).color(p.text).truncate())
                .child(row().gap(6.0).center_y()
                    .child(el().size(6.0, 6.0).radius(3.0).fill(if live { p.accent } else { p.faint }).cycle_if(!live && !stopping))
                    .child(text(status).font(ui(11.5, 500.0)).color(p.muted).truncate().min_w(0.0))))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.go(Page::Game(open.clone()), tx));
        let mut end = el().key(format!("running-stop-{key:016x}")).role("button")
            .label(format!("Arrêter {}", g.name)).help("Arrêter le jeu".to_string())
            .size(30.0, 30.0).radius(6.0).center()
            .hover(p.danger.alpha(0.14))
            .press_scale(0.94)
            .child(icons::icon(Icon::Stop, 13.0, if stopping { p.faint } else { p.danger }));
        if !stopping {
            end = end.on_press_tx(move |s: &mut State, tx: &mut Tx| s.stop(&stop, tx));
        }
        list = list.child(row().w_full().pr(4.0).gap(4.0).radius(8.0).center_y()
            .fill(p.accent.alpha(if p.dark { 0.08 } else { 0.06 }))
            .border(1.0, p.accent.alpha(0.18))
            .child(name)
            .child(end));
    }
    Some(list)
}

/// The arachnid emblem from `assets/icon.svg`.
fn brand_mark(s: &State, p: &Palette) -> El {
    match &s.brand {
        Some(img) => el().size(30.0, 30.0).pixels_fit(img.pixels(), rune::SurfaceFit::Fit),
        None => el().size(30.0, 30.0).radius(5.0).fill(p.accent),
    }
}

pub fn collection_key(id: u64) -> String {
    format!("nav-collection-{id:016x}")
}

/// Where Favoris and each collection sit in the window, for a game dropped on the sidebar.
/// Read from the last frame's layout: the sidebar doesn't move while a drag runs.
fn record_drop_targets(s: &State, cx: &mut Cx<State>) {
    let mut t = s.drop_targets.borrow_mut();
    t.clear();
    if s.dragging.is_none() { return; }
    if let Some(r) = cx.window_layout("nav-favorites") { t.push((DropTarget::Favorites, r)); }
    for c in &s.user.collections {
        if let Some(r) = cx.window_layout(&collection_key(c.id)) { t.push((DropTarget::Collection(c.id), r)); }
    }
}

/// The game's name under the pointer while it's dragged toward the sidebar.
fn drag_ghost(s: &State, p: &Palette) -> Option<El> {
    let d = s.dragging.as_ref().filter(|d| d.active)?;
    let g = s.game(&d.id)?;
    let landing = s.drop_target_at(d.at.0, d.at.1).is_some();
    Some(
        row().key("drag-ghost").absolute(d.at.0 + 14.0, d.at.1 + 10.0).h(34.0).pl(5.0).pr(14.0).gap(9.0).radius(17.0).center_y()
            .fill(p.popover)
            .border(1.0, if landing { p.accent.alpha(0.8) } else { p.popover_line })
            .shadow(10.0, 28.0, p.shadow)
            .child(widgets::game_icon(s, g, p, 24.0))
            .child(text(g.name.clone()).font(ui(13.0, 600.0)).color(p.text).truncate().max_w(240.0))
            .child(icon(Icon::Plus, 13.0, if landing { p.accent } else { p.faint })),
    )
}

fn nav_item(p: &Palette, key: &str, glyph: Icon, label: &str, count: Option<usize>, on: bool) -> El {
    let ink = if on { p.text } else { p.muted };
    row()
        .key(key.to_string())
        .role("button")
        .label(label.to_string())
        .h(36.0)
        .px(10.0)
        .gap(11.0)
        .radius(5.0)
        .center_y()
        .fill(if on { p.text.alpha(if p.dark { 0.075 } else { 0.06 }) } else { Color::TRANSPARENT })
        .hover(p.hover)
        .press_scale(0.98)
        .child(icon(glyph, 18.0, if on { p.accent } else { p.faint }))
        .child(text(label.to_string()).font(ui(13.5, if on { 600.0 } else { 500.0 })).color(ink).grow(1.0))
        .children(count.map(|n| text(n.to_string()).font(num(12.0, 500.0)).color(p.faint)))
}

// ---------------------------------------------------------------- overlays

fn drop_veil(p: &Palette, game_page: bool) -> El {
    let (title, hint) = if game_page {
        ("DÉPOSER UNE IMAGE", "Verticale : la jaquette · horizontale : l'image de fond · ou un .exe à ajouter")
    } else {
        ("DÉPOSER POUR AJOUTER", "Un .exe, un raccourci .lnk ou un lien .url")
    };
    el().key("drop")
        .pin(Some(10.0), Some(10.0), Some(10.0), Some(10.0))
        .radius(10.0)
        .fill(p.bg.alpha(0.9))
        .border(1.5, p.accent.alpha(0.8))
        .center()
        .transition(Transition::opacity())
        .child(
            col().gap(8.0).center()
                .child(icon(Icon::Plus, 34.0, p.accent))
                .child(text(title).font(display(30.0)).color(p.text))
                .child(text(hint).font(ui(13.0, 450.0)).color(p.muted)),
        )
}

fn toast(msg: &str, error: bool, p: &Palette) -> El {
    let (glyph, tint) = if error { (Icon::Alert, p.danger) } else { (Icon::Check, p.accent) };
    let pill = row()
        .key("toast")
        .role("status")
        .min_h(44.0)
        .max_w(560.0)
        .pl(8.0)
        .pr(16.0)
        .py(8.0)
        .radius(6.0)
        .center_y()
        .gap(10.0)
        .fill(p.popover)
        .border(1.0, p.popover_line)
        .shadow(12.0, 32.0, p.shadow)
        // In from just below, out the same way: it says where it lives.
        .transition(Transition::offset(0.0, 12.0).combined(Transition::opacity())
            .springs(SpringPair::new(Spring::new(0.28, 0.0), Spring::new(0.18, 0.0))))
        .child(el().size(28.0, 28.0).radius(5.0).center().fill(tint.alpha(0.15)).child(icon(glyph, 15.0, tint)))
        .child(text(msg.to_string()).font(ui(13.0, 500.0)).color(p.text).wrap().max_lines(2).shrink().min_w(0.0));
    row().key("toast-lane").pin(Some(SIDEBAR + 24.0), None, Some(24.0), Some(24.0)).center().child(pill)
}

// ---------------------------------------------------------------- keys

/// Keys the search field passes on.
pub fn keys(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    match &ev.key {
        Key::Escape if !s.query.text.is_empty() => {
            s.query.clear();
            s.refilter();
            tx.instant();
        }
        Key::Escape => tx.focus("page", false),
        _ => return shortcuts(s, ev, tx),
    }
    true
}

/// Keys with nothing focused, and those a focused control passed on.
pub fn window_keys(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if s.overlay != Overlay::None { return false; }
    if shortcuts(s, ev, tx) { return true; }
    match &ev.key {
        Key::Escape if s.selecting => {
            tx.instant();
            s.end_selection();
        }
        _ if ev.is_command('a') && s.page == Page::Library => s.select_all(tx),
        Key::Escape | Key::Backspace if s.can_go_back() => s.back(tx),
        // Typing anywhere searches.
        Key::Text(t) if !ev.mods.cmd && !ev.mods.alt && t.chars().all(|c| !c.is_control()) && t != " " => {
            s.query.text.push_str(t);
            let end = s.query.text.len();
            (s.query.anchor, s.query.focus) = (end, end);
            tx.focus("search", false);
            s.searched(tx);
        }
        _ => return false,
    }
    true
}

fn shortcuts(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    match &ev.key {
        Key::Function(5) => s.rescan(),
        Key::Function(11) => tx.toggle_full_screen(),
        Key::Left if ev.mods.alt => s.back(tx),
        _ if ev.is_command('k') || ev.is_command('f') => tx.focus("search", false),
        _ if ev.is_command('r') => s.rescan(),
        _ if ev.is_command(',') => s.go(Page::Settings(Tab::General), tx),
        _ if ev.is_command('1') => s.go(Page::Home, tx),
        _ if ev.is_command('2') => s.open_shelf(Shelf::All, tx),
        _ if ev.is_command('3') => s.open_shelf(Shelf::Favorites, tx),
        _ => return false,
    }
    true
}

/// Pulses a status dot while work runs; still otherwise.
trait CycleIf {
    fn cycle_if(self, on: bool) -> Self;
}

impl CycleIf for El {
    fn cycle_if(self, on: bool) -> El {
        if on { self.looping(rune_motion_pulse()) } else { self }
    }
}

fn rune_motion_pulse() -> rune::LoopSpec {
    rune::LoopSpec::opacity(1.2, 0.0, 0.35, 1.0)
}
