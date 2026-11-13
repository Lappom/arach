//! What floats over the page: dropdowns, a game's menu and confirmations. Each is drawn here,
//! in the window's colours, rather than by the OS, and mounted at the root so it escapes the
//! page's scroll clip. Which one is open lives in `State::overlay`; Escape and a press outside
//! close it, and focus goes back to what opened it.

use rune::{Align, Color, El, Shadow, Side, Spring, SpringPair, TextStyle, Transition, Tx, Vec2, col, el, row, text, text_input};

use super::icons::{Icon, icon};
use super::theme::{Palette, caption, ui};
use super::widgets::{Tone, button, game_icon, segmented, size, store_dot};
use crate::app::{Confirm, EDIT_FIELDS, Filter, Overlay, Page, Played, State, toggle};
use crate::game::Source;
use crate::meta::picker::Kind;
use crate::store::Status;

/// A popover grows out of the edge it hangs from: quick, no bounce, and gone faster than it came.
fn pop(origin_x: f32) -> Transition {
    Transition::scale_anchor(0.96, Vec2::new(origin_x, 0.0)).springs(SpringPair::new(Spring::new(0.18, 0.0), Spring::new(0.12, 0.0)))
}

fn surface(p: &Palette, key: String) -> El {
    col()
        .key(key)
        .radius(8.0)
        .fill(p.popover)
        .border(1.0, p.popover_line)
        .box_shadow(Shadow { x: 0.0, y: 14.0, blur: 36.0, spread: -6.0, color: p.shadow, inset: false })
        .box_shadow(Shadow { x: 0.0, y: 2.0, blur: 6.0, spread: 0.0, color: p.shadow.alpha(p.shadow.a * 0.4), inset: false })
}

fn close(s: &mut State) {
    s.overlay = Overlay::None;
}

/// The row highlight that pointer and arrow keys share.
fn highlight(p: &Palette) -> Color {
    p.text.alpha(if p.dark { 0.075 } else { 0.06 })
}

// ---------------------------------------------------------------- select

/// A dropdown: the trigger shows the choice, the list opens under it.
pub struct Select<'a, T> {
    key: &'static str,
    name: &'a str,
    options: &'a [(T, &'static str)],
    current: T,
    width: f32,
    prefix: Option<&'a str>,
    end: bool,
}

pub fn select<'a, T>(key: &'static str, name: &'a str, options: &'a [(T, &'static str)], current: T) -> Select<'a, T> {
    Select { key, name, options, current, width: 200.0, prefix: None, end: true }
}

impl<'a, T: Copy + PartialEq + 'static> Select<'a, T> {
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    /// Faint words before the value, inside the trigger ("Trier par").
    pub fn prefix(mut self, p: &'a str) -> Self {
        self.prefix = Some(p);
        self
    }

    pub fn build(self, s: &State, p: &Palette, layer: &mut Vec<El>, pick: impl Fn(&mut State, T, &mut Tx) + Clone + 'static) -> El {
        let key = self.key;
        let open = s.overlay == Overlay::Select(key);
        let trigger_key = format!("select-{key}");
        let value = self.options.iter().find(|o| o.0 == self.current).map_or("", |o| o.1);
        let trigger = row()
            .key(trigger_key.clone())
            .role("combobox")
            .label(self.name.to_string())
            .a11y_value(value)
            .expanded(open)
            .focusable(true)
            .w(self.width)
            .h(34.0)
            .pl(12.0)
            .pr(9.0)
            .gap(6.0)
            .radius(5.0)
            .center_y()
            .fill(p.field)
            .border(1.0, if open { p.accent.alpha(0.7) } else { p.line })
            .shadow(1.0, 2.0, p.shadow.alpha(if p.dark { 0.0 } else { 0.05 }))
            .hover(p.hover)
            .children(self.prefix.map(|t| text(t.to_string()).font(ui(13.0, 500.0)).color(p.faint)))
            .child(text(value).font(ui(13.0, 600.0)).color(p.text).truncate().grow(1.0).min_w(0.0))
            .child(el().key("chevron").size(16.0, 16.0).rotate(if open { std::f32::consts::PI } else { 0.0 })
                .child(icon(Icon::ChevronDown, 16.0, p.faint)))
            .on_press(move |s: &mut State| s.toggle_overlay(Overlay::Select(key)));
        if !open {
            return trigger;
        }

        let mut list = col().key("options").role("listbox").label(self.name.to_string()).focus_group().padding(5.0).gap(1.0).items_stretch();
        for (i, (v, label)) in self.options.iter().enumerate() {
            let (v, on, pick) = (*v, *v == self.current, pick.clone());
            list = list.child(
                row()
                    .key(format!("option-{i}"))
                    .role("option")
                    .label(*label)
                    .focusable(true)
                    .focus_selected(on)
                    .position_in_set(i + 1, self.options.len())
                    .h(34.0)
                    .px(10.0)
                    .gap(10.0)
                    .radius(4.0)
                    .center_y()
                    .focus_highlight(highlight(p))
                    .child(text(*label).font(ui(13.0, if on { 600.0 } else { 500.0 })).color(p.text).grow(1.0))
                    .child(icon(Icon::Check, 15.0, p.accent).opacity(if on { 1.0 } else { 0.0 }))
                    .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                        s.overlay = Overlay::None;
                        pick(s, v, tx)
                    }),
            );
        }
        let pop_el = surface(p, format!("{key}-popover"))
            .label(self.name.to_string())
            .min_w(self.width)
            .max_w(360.0)
            .modal(true, close)
            .focus_return(trigger_key.clone())
            .child(list)
            .anchored_fit(trigger_key, Side::Bottom, if self.end { Align::End } else { Align::Start }, 6.0)
            .instant_changes()
            .transition(pop(if self.end { 1.0 } else { 0.0 }));
        layer.push(pop_el);
        trigger
    }
}

// ---------------------------------------------------------------- game menu

struct Item {
    key: String,
    glyph: Icon,
    label: String,
    enabled: bool,
    danger: bool,
    run: Box<dyn Fn(&mut State, &mut Tx)>,
}

fn item(key: impl Into<String>, glyph: Icon, label: impl Into<String>, run: impl Fn(&mut State, &mut Tx) + 'static) -> Item {
    Item { key: key.into(), glyph, label: label.into(), enabled: true, danger: false, run: Box::new(run) }
}

/// A game's actions, after a right-click anywhere or from the "more" button on its page.
pub fn game_menu(s: &State, p: &Palette, layer: &mut Vec<El>) {
    let Overlay::GameMenu { id, at } = &s.overlay else { return };
    let Some(g) = s.game(id) else { return };
    let on_page = s.page == Page::Game(g.id.clone());
    let fav = s.is_favorite(&g.id);
    let id = g.id.clone();

    let mut groups: Vec<Vec<Item>> = Vec::new();
    let mut first = Vec::new();
    {
        let id = id.clone();
        let (glyph, label) = if g.not_installed { (Icon::External, format!("Installer avec {}", g.source.label())) } else { (Icon::Play, "Jouer".into()) };
        first.push(item("play", glyph, label, move |s, tx| s.launch(&id, tx)));
    }
    if !on_page {
        let id = id.clone();
        first.push(item("open", Icon::ChevronRight, "Voir la fiche", move |s, tx| s.go(Page::Game(id.clone()), tx)));
    }
    groups.push(first);

    let mut second = Vec::new();
    {
        let id = id.clone();
        second.push(item("fav", if fav { Icon::StarFilled } else { Icon::Star }, if fav { "Retirer des favoris" } else { "Ajouter aux favoris" },
            move |s, tx| s.toggle_favorite(&id, tx)));
    }
    {
        let id = id.clone();
        let mut folder = item("folder", Icon::Folder, "Ouvrir le dossier", move |s, tx| s.open_folder(&id, tx));
        folder.enabled = g.dir.is_some();
        second.push(folder);
    }
    if let Some(appid) = g.steam_appid.filter(|a| *a > 0) {
        second.push(item("store", Icon::External, "Voir sur le Steam Store", move |s, tx| {
            s.open_uri(&format!("https://store.steampowered.com/app/{appid}"), tx)
        }));
    }
    groups.push(second);

    // Collections: a check on those the game is in; a press puts it in or takes it out.
    let mut third = Vec::new();
    for c in &s.user.collections {
        let (id, cid) = (id.clone(), c.id);
        let inside = c.games.contains(&g.id);
        third.push(item(format!("coll-{cid:016x}"), if inside { Icon::Check } else { Icon::List }, c.name.clone(),
            move |s, tx| s.toggle_in_collection(cid, &id, tx)));
    }
    {
        let id = id.clone();
        third.push(item("coll-new", Icon::Plus, "Nouvelle collection…", move |s, tx| s.name_collection(None, Some(id.clone()), tx)));
    }
    groups.push(third);

    // Fixing what the stores got wrong.
    let mut fix = Vec::new();
    {
        let id = id.clone();
        fix.push(item("edit", Icon::Pencil, "Modifier les informations…", move |s, tx| s.open_edit(&id, tx)));
    }
    {
        let id = id.clone();
        fix.push(item("art", Icon::Image, "Changer la jaquette…", move |s, tx| s.open_picker(&id, Kind::Cover, tx)));
    }
    {
        let id = id.clone();
        fix.push(item("merge", Icon::Merge, "Fusionner avec…", move |s, tx| {
            s.go(Page::Library, tx);
            s.start_selection(tx);
            s.toggle_selected(&id, tx);
            s.notify("Choisissez les autres versions, puis « Fusionner »".into(), false, tx);
        }));
    }
    groups.push(fix);

    let custom = g.source == Source::Custom;
    let mut hide = {
        let id = id.clone();
        item("hide", if custom { Icon::Trash } else { Icon::Eye }, if custom { "Retirer de la bibliothèque" } else { "Masquer" },
            move |s, tx| s.toggle_hidden(&id, tx))
    };
    hide.danger = custom;
    groups.push(vec![hide]);

    // Who the menu is about: worth a glance when it opened over a cover in a grid of them.
    let head = row().key("menu-head").px(10.0).pt(8.0).pb(8.0).gap(10.0).center_y()
        .child(game_icon(s, g, p, 28.0))
        .child(col().gap(2.0).min_w(0.0).shrink()
            .child(text(g.name.clone()).font(ui(13.0, 650.0)).color(p.text).truncate().max_w(220.0))
            .child(row().gap(6.0).center_y().child(store_dot(g.source, 6.0))
                .child(text(g.source.label()).font(ui(11.5, 500.0)).color(p.faint))));

    let mut list = col().key("menu-items").role("menu").label(g.name.clone()).focus_group().padding(5.0).gap(1.0).items_stretch()
        .child(head)
        .child(rule(p));
    let n = groups.len();
    for (gi, group) in groups.into_iter().enumerate() {
        for it in group {
            list = list.child(menu_row(p, it));
        }
        if gi + 1 < n {
            list = list.child(rule(p));
        }
    }

    let (anchor, side_align, gap, origin) = match at {
        Some((x, y)) => {
            layer.push(el().key("menu-point").absolute(*x, *y).size(0.0, 0.0));
            ("menu-point".to_string(), Align::Start, 2.0, 0.0)
        }
        None => ("detail-more".to_string(), Align::End, 8.0, 1.0),
    };
    let menu = surface(p, "game-menu".into())
        .label(g.name.clone())
        .w(256.0)
        .modal(true, close)
        .focus_return(anchor.clone())
        .child(list)
        .anchored_fit(anchor, Side::Bottom, side_align, gap)
        .instant_changes()
        .transition(pop(origin));
    layer.push(menu);
}

fn rule(p: &Palette) -> El {
    el().h(1.0).mx(-5.0).my(4.0).fill(p.popover_line).a11y_hidden()
}

fn menu_row(p: &Palette, it: Item) -> El {
    let ink = if it.danger { p.danger } else { p.text };
    let mut r = row()
        .key(it.key)
        .role("menuitem")
        .label(it.label.clone())
        .h(34.0)
        .px(10.0)
        .gap(11.0)
        .radius(4.0)
        .center_y()
        .disabled(!it.enabled)
        .focusable(it.enabled)
        .child(icon(it.glyph, 16.0, if it.danger { p.danger } else { p.muted }))
        .child(text(it.label).font(ui(13.0, 500.0)).color(ink).grow(1.0));
    if it.enabled {
        let run = it.run;
        r = r.focus_highlight(if it.danger { p.danger.alpha(0.12) } else { highlight(p) }).on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.overlay = Overlay::None;
            run(s, tx)
        });
    } else {
        r = r.layer_opacity(0.4);
    }
    r
}

// ---------------------------------------------------------------- confirm

/// A destructive action asks first, in a dialog centred on the window.
pub fn confirm(s: &State, p: &Palette, layer: &mut Vec<El>) {
    let Overlay::Confirm(what) = &s.overlay else { return };
    let (title, body, action) = match what {
        Confirm::ClearCache => (
            "Vider le cache ?",
            format!(
                "Les jaquettes et descriptions téléchargées{} seront supprimées, puis récupérées à nouveau en ligne.",
                if s.cache_bytes > 0 { format!(" ({})", size(s.cache_bytes)) } else { String::new() }
            ),
            "Vider le cache",
        ),
        Confirm::RemoveFolder(dir) => (
            "Retirer ce dossier ?",
            format!("Les jeux trouvés dans {} quitteront la bibliothèque. Rien n'est supprimé du disque.", dir.display()),
            "Retirer",
        ),
        Confirm::Merge(ids) => {
            let first = ids.first().and_then(|id| s.game(id)).map_or(String::new(), |g| g.name.clone());
            let others: Vec<String> = ids.iter().skip(1).filter_map(|id| s.game(id)).map(|g| format!("{} ({})", g.name, g.source.label())).collect();
            (
                "Fusionner ces jeux ?",
                format!(
                    "{} deviendra une version de « {first} » : une seule entrée, un temps de jeu commun, et le choix de la version à lancer sur sa fiche. « Séparer » les remet à part.",
                    others.join(", ")
                ),
                "Fusionner",
            )
        }
        Confirm::DeleteCollection(id) => (
            "Supprimer cette collection ?",
            format!(
                "« {} » disparaîtra de la barre latérale. Ses jeux restent dans la bibliothèque.",
                s.collection(*id).map_or("", |c| c.name.as_str())
            ),
            "Supprimer",
        ),
    };
    layer.push(
        el().key("dialog-veil").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .fill(Color::BLACK.alpha(if p.dark { 0.55 } else { 0.28 }))
            .transition(Transition::opacity().springs(SpringPair::new(Spring::new(0.2, 0.0), Spring::new(0.14, 0.0)))),
    );
    let badge = el().size(40.0, 40.0).radius(20.0).center().fill(p.danger.alpha(0.14))
        .child(icon(Icon::Alert, 20.0, p.danger));
    let card = col()
        .key("dialog")
        .role("dialog")
        .label(title)
        .w(440.0)
        .padding(24.0)
        .gap(10.0)
        .radius(10.0)
        .fill(p.popover)
        .border(1.0, p.popover_line)
        .box_shadow(Shadow { x: 0.0, y: 24.0, blur: 64.0, spread: -8.0, color: p.shadow, inset: false })
        .modal(true, close)
        .child(badge)
        .child(text(title).font(ui(17.0, 650.0)).color(p.text).pt(6.0))
        .child(text(body).font(rune::TextStyle::new(13.5, 450.0).lh(20.0)).color(p.muted).wrap())
        .child(
            row().w_full().pt(14.0).gap(8.0).justify_end()
                .child(button(p, "dialog-cancel", "Annuler", None, Tone::Secondary).autofocus().on_press(close))
                .child(button(p, "dialog-ok", action, None, Tone::Danger).default_button()
                    .on_press_tx(|s: &mut State, tx: &mut Tx| s.confirmed(tx))),
        )
        .transition(Transition::scale(0.96).springs(SpringPair::new(Spring::new(0.22, 0.0), Spring::new(0.14, 0.0))));
    layer.push(el().key("dialog-center").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().child(card));
}

// ---------------------------------------------------------------- collection name

/// Naming a new collection, or renaming one: a field and OK, Enter for OK.
pub fn name_collection(s: &State, p: &Palette, layer: &mut Vec<El>) {
    let Overlay::NameCollection { id, add } = &s.overlay else { return };
    let title = if id.is_some() { "Renommer la collection" } else { "Nouvelle collection" };
    let hint = match add.as_deref().and_then(|g| s.game(g)) {
        Some(g) if id.is_none() => format!("{} y sera ajouté.", g.name),
        _ if id.is_none() => "Glissez-y des jeux depuis la bibliothèque, ou ajoutez-les par clic droit.".into(),
        _ => String::new(),
    };
    let ready = !s.name_field.text.trim().is_empty();
    let field = row().h(38.0).px(12.0).radius(5.0).center_y().fill(p.field).border(1.0, p.line)
        .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
        .child(
            text_input(&s.name_field, TextStyle::new(14.0, 500.0).lh(20.0))
                .key("collection-name")
                .single_line()
                .grow(1.0)
                .color(p.text)
                .placeholder("À finir, Coop, Rétro…", p.faint)
                .label("Nom de la collection")
                .on_edit(|s: &mut State| &mut s.name_field)
                .on_submit(|s: &mut State, tx| s.named_collection(tx)),
        );
    layer.push(
        el().key("dialog-veil").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .fill(Color::BLACK.alpha(if p.dark { 0.55 } else { 0.28 }))
            .transition(Transition::opacity().springs(SpringPair::new(Spring::new(0.2, 0.0), Spring::new(0.14, 0.0)))),
    );
    let label = if id.is_some() { "Renommer" } else { "Créer" };
    let ok = row().key("dialog-ok").role("button").label(label).h(34.0).px(16.0).radius(5.0).center()
        .fill(p.accent)
        .border(1.0, Color::WHITE.alpha(0.12))
        .hover(Color::WHITE.alpha(0.1))
        .press_scale(0.97)
        .child(text(label).font(ui(13.0, 650.0)).color(p.on_accent));
    let ok = if ready {
        ok.default_button().on_press_tx(|s: &mut State, tx: &mut Tx| s.named_collection(tx))
    } else {
        ok.disabled(true).layer_opacity(0.5)
    };
    let card = col()
        .key("dialog")
        .role("dialog")
        .label(title)
        .w(420.0)
        .padding(24.0)
        .gap(12.0)
        .radius(10.0)
        .fill(p.popover)
        .border(1.0, p.popover_line)
        .box_shadow(Shadow { x: 0.0, y: 24.0, blur: 64.0, spread: -8.0, color: p.shadow, inset: false })
        .modal(true, close)
        .child(text(title).font(ui(17.0, 650.0)).color(p.text))
        .child(field)
        .children((!hint.is_empty()).then(|| text(hint).font(TextStyle::new(12.5, 450.0).lh(18.0)).color(p.muted).wrap()))
        .child(
            row().w_full().pt(10.0).gap(8.0).justify_end()
                .child(button(p, "dialog-cancel", "Annuler", None, Tone::Secondary).on_press(close))
                .child(ok),
        )
        .transition(Transition::scale(0.96).springs(SpringPair::new(Spring::new(0.22, 0.0), Spring::new(0.14, 0.0))));
    layer.push(el().key("dialog-center").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().child(card));
}

// ---------------------------------------------------------------- filters

/// A chip that's on or off, for the filter panel.
fn choice(p: &Palette, key: String, label: String, on: bool, n: Option<usize>, press: impl Fn(&mut State, &mut Tx) + 'static) -> El {
    row().key(key).role("checkbox").label(label.clone()).checked(on).h(28.0).px(11.0).gap(6.0).radius(14.0).center_y()
        .fill(if on { p.accent.alpha(0.16) } else { Color::TRANSPARENT })
        .border(1.0, if on { p.accent.alpha(0.6) } else { p.line })
        .hover(p.hover)
        .press_scale(0.97)
        .children(on.then(|| icon(Icon::Check, 12.0, p.accent)))
        .child(text(label).font(ui(12.5, if on { 600.0 } else { 500.0 })).color(if on { p.text } else { p.muted }))
        .children(n.map(|n| text(n.to_string()).font(ui(11.5, 500.0)).color(p.faint)))
        .on_press_tx(press)
}

/// Every filter at once, under the Filtres button: each kind narrows the list further.
pub fn filters(s: &State, p: &Palette, layer: &mut Vec<El>) {
    if s.overlay != Overlay::Filters { return; }
    let f = &s.filter;
    let label = |t: &str| text(t.to_string()).font(caption()).color(p.faint).pt(4.0);
    let mut body = col().key("filters-body").padding(16.0).gap(10.0);

    let mut statuses = row().gap(6.0).flex_wrap();
    for st in Status::ALL {
        statuses = statuses.child(choice(p, format!("f-st-{st:?}"), st.label().into(), f.statuses.contains(&st), None,
            move |s, tx| s.change_filter(tx, |f| toggle(&mut f.statuses, st))));
    }
    body = body.child(label("Statut")).child(statuses);

    let mut played = row().gap(6.0).flex_wrap();
    for pl in [Played::Never, Played::Stale] {
        let on = f.played == pl;
        played = played.child(choice(p, format!("f-pl-{pl:?}"), pl.label().into(), on, None,
            move |s, tx| s.change_filter(tx, |f| f.played = if f.played == pl { Played::Any } else { pl })));
    }
    body = body.child(label("Dernière session")).child(played);

    if !s.user.collections.is_empty() {
        let mut cols = row().gap(6.0).flex_wrap();
        for c in &s.user.collections {
            let cid = c.id;
            cols = cols.child(choice(p, format!("f-c-{cid:016x}"), c.name.clone(), f.collections.contains(&cid), Some(c.games.len()),
                move |s, tx| s.change_filter(tx, |f| toggle(&mut f.collections, cid))));
        }
        body = body.child(label("Collection")).child(cols);
    }

    let genres = s.genre_choices();
    if !genres.is_empty() {
        let mut chips = row().gap(6.0).flex_wrap();
        // The chosen ones always show, even past the most common.
        for (g, n) in genres.iter().enumerate().filter(|(i, (g, _))| *i < 24 || f.genres.contains(g)).map(|(_, x)| x) {
            let on = f.genres.iter().any(|x| x.eq_ignore_ascii_case(g));
            let genre = g.clone();
            chips = chips.child(choice(p, format!("f-g-{g}"), g.clone(), on, Some(*n),
                move |s, tx| s.change_filter(tx, |f| toggle(&mut f.genres, genre.clone()))));
        }
        body = body.child(label("Genres (tous ceux choisis)")).child(chips);
    }

    let years = s.year_choices();
    if !years.is_empty() {
        let mut chips = row().gap(6.0).flex_wrap();
        for y in years {
            chips = chips.child(choice(p, format!("f-y-{y}"), y.to_string(), f.years.contains(&y), None,
                move |s, tx| s.change_filter(tx, |f| toggle(&mut f.years, y))));
        }
        body = body.child(label("Année de sortie")).child(chips);
    }

    let foot = row().w_full().px(16.0).py(12.0).gap(8.0).center_y()
        .child(text(format!("{} jeu{}", s.visible.len(), if s.visible.len() > 1 { "x" } else { "" })).font(ui(12.5, 500.0)).color(p.faint).grow(1.0))
        .child(button(p, "filters-clear", "Tout effacer", None, Tone::Secondary)
            .on_press_tx(|s: &mut State, tx: &mut Tx| s.change_filter(tx, |f| *f = Filter::default())))
        .child(button(p, "filters-done", "OK", None, Tone::Secondary).on_press(close));
    let panel = surface(p, "filters-popover".into())
        .label("Filtres")
        .w(460.0)
        .modal(true, close)
        .focus_return("filter-button")
        .child(col().key("filters-scroll").max_h(460.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body))
        .child(rule(p).mx(0.0).my(0.0))
        .child(foot)
        .anchored_fit("filter-button", Side::Bottom, Align::End, 6.0)
        .instant_changes()
        .transition(pop(1.0));
    layer.push(panel);
}

// ---------------------------------------------------------------- selection

/// While picking games: what to do with all of them, along the bottom of the window.
pub fn selection_bar(s: &State, p: &Palette) -> El {
    let n = s.selected.len();
    let any = n > 0;
    let all_fav = any && s.selected.iter().all(|id| s.is_favorite(id));
    let act = |key: &str, glyph: Icon, label: &str, enabled: bool| {
        let b = row().key(key.to_string()).role("button").label(label.to_string()).h(34.0).px(12.0).gap(7.0).radius(17.0).center_y()
            .hover(p.hover)
            .press_scale(0.97)
            .disabled(!enabled)
            .child(icon(glyph, 15.0, p.muted))
            .child(text(label.to_string()).font(ui(13.0, 600.0)).color(p.text));
        if enabled { b } else { b.layer_opacity(0.4) }
    };
    let count = if any { format!("{n} sélectionné{}", if n > 1 { "s" } else { "" }) } else { "Cliquez sur des jeux".into() };
    let mut bar = row().key("selection-bar").role("toolbar").label("Sélection").min_h(52.0).pl(18.0).pr(8.0).py(8.0).gap(4.0).radius(26.0).center_y()
        .fill(p.popover)
        .border(1.0, p.popover_line)
        .shadow(14.0, 40.0, p.shadow)
        .flex_wrap()
        .child(text(count).font(ui(13.0, 650.0)).color(p.text).pr(10.0))
        .child(act("sel-all", Icon::Check, "Tout sélectionner", true).on_press_tx(|s: &mut State, tx: &mut Tx| s.select_all(tx)))
        .child(el().w(1.0).h(22.0).fill(p.line).mx(4.0));
    let fav = act("sel-fav", if all_fav { Icon::StarFilled } else { Icon::Star }, if all_fav { "Retirer des favoris" } else { "Favori" }, any);
    let coll = act("bulk-collections", Icon::List, "Collection…", any).expanded(s.overlay == Overlay::BulkCollections);
    let status = act("bulk-status", Icon::Check, "Statut…", any).expanded(s.overlay == Overlay::BulkStatus);
    let hide = act("sel-hide", Icon::Eye, "Masquer", any);
    let merge = act("sel-merge", Icon::Merge, "Fusionner", n >= 2).help("Un seul jeu, plusieurs versions (les autres stores)");
    if any {
        bar = bar
            .child(fav.on_press_tx(|s: &mut State, tx: &mut Tx| s.bulk_favorite(tx)))
            .child(coll.on_press(|s: &mut State| s.toggle_overlay(Overlay::BulkCollections)))
            .child(status.on_press(|s: &mut State| s.toggle_overlay(Overlay::BulkStatus)))
            .child(hide.on_press_tx(|s: &mut State, tx: &mut Tx| s.bulk_hide(tx)));
    } else {
        bar = bar.child(fav).child(coll).child(status).child(hide);
    }
    bar = bar.child(if n >= 2 { merge.on_press(|s: &mut State| s.ask_merge()) } else { merge });
    bar = bar.child(el().w(1.0).h(22.0).fill(p.line).mx(4.0))
        .child(row().key("sel-cancel").role("button").label("Annuler la sélection").help("Échap").h(34.0).px(14.0).radius(17.0).center()
            .fill(p.text.alpha(0.08))
            .hover(p.hover)
            .child(text("Annuler").font(ui(13.0, 600.0)).color(p.text))
            .on_press_tx(|s: &mut State, tx: &mut Tx| { tx.instant(); s.end_selection() }));
    row().key("selection-lane").pin(Some(260.0), None, Some(24.0), Some(22.0)).center()
        .child(bar.transition(Transition::offset(0.0, 16.0).combined(Transition::opacity())
            .springs(SpringPair::new(Spring::new(0.26, 0.0), Spring::new(0.16, 0.0)))))
}

/// The selection bar's menus: a collection or a status for every picked game.
pub fn bulk_menus(s: &State, p: &Palette, layer: &mut Vec<El>) {
    let (anchor, items): (&str, Vec<Item>) = match s.overlay {
        Overlay::BulkCollections => {
            let mut items: Vec<Item> = s.user.collections.iter().map(|c| {
                let cid = c.id;
                let all_in = !s.selected.is_empty() && s.selected.iter().all(|id| c.games.contains(id));
                item(format!("bulk-coll-{cid:016x}"), if all_in { Icon::Check } else { Icon::List }, c.name.clone(),
                    move |s, tx| s.bulk_collection(cid, tx))
            }).collect();
            items.push(item("bulk-coll-new", Icon::Plus, "Nouvelle collection…", |s, tx| s.name_collection(None, None, tx)));
            ("bulk-collections", items)
        }
        Overlay::BulkStatus => (
            "bulk-status",
            Status::ALL.iter().map(|st| {
                let st = *st;
                item(format!("bulk-st-{st:?}"), Icon::Check, st.label(), move |s, tx| s.bulk_status(st, tx))
            }).collect(),
        ),
        _ => return,
    };
    let mut list = col().key("bulk-items").role("menu").focus_group().padding(5.0).gap(1.0).items_stretch();
    for it in items {
        list = list.child(menu_row(p, it));
    }
    layer.push(
        surface(p, "bulk-menu".into())
            .w(240.0)
            .modal(true, close)
            .focus_return(anchor)
            .child(list)
            .anchored_fit(anchor, Side::Top, Align::Start, 8.0)
            .instant_changes()
            .transition(pop(0.0)),
    );
}

// ---------------------------------------------------------------- edit a game

fn veil(p: &Palette) -> El {
    el().key("dialog-veil").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(Color::BLACK.alpha(if p.dark { 0.55 } else { 0.28 }))
        .transition(Transition::opacity().springs(SpringPair::new(Spring::new(0.2, 0.0), Spring::new(0.14, 0.0))))
}

fn dialog(p: &Palette, title: &str, w: f32) -> El {
    col()
        .key("dialog")
        .role("dialog")
        .label(title.to_string())
        .w(w)
        .max_w(920.0)
        .padding(24.0)
        .gap(12.0)
        .radius(10.0)
        .fill(p.popover)
        .border(1.0, p.popover_line)
        .box_shadow(Shadow { x: 0.0, y: 24.0, blur: 64.0, spread: -8.0, color: p.shadow, inset: false })
        .modal(true, close)
        .child(text(title.to_string()).font(ui(17.0, 650.0)).color(p.text))
        .transition(Transition::scale(0.96).springs(SpringPair::new(Spring::new(0.22, 0.0), Spring::new(0.14, 0.0))))
}

/// Correcting a game's name and facts. Lists are written with commas; an emptied field stays
/// empty, and « Rétablir » goes back to what the stores say.
pub fn edit_game(s: &State, p: &Palette, layer: &mut Vec<El>) {
    let Overlay::EditGame(id) = &s.overlay else { return };
    if s.edit_fields.len() != EDIT_FIELDS.len() { return; }
    let edited = s.user.edits.contains_key(id);
    let mut form = col().gap(12.0);
    for (i, label) in EDIT_FIELDS.iter().enumerate() {
        let multi = i == 1;
        let mut input = text_input(&s.edit_fields[i], TextStyle::new(13.5, 450.0).lh(20.0))
            .key(format!("edit-{i}"))
            .grow(1.0)
            .color(p.text)
            .label(label.to_string())
            .on_edit(move |s: &mut State| &mut s.edit_fields[i]);
        input = match i {
            1 => input.placeholder("Quelques phrases sur le jeu", p.faint),
            2 | 3 => input.single_line().placeholder("Séparés par des virgules", p.faint),
            4 => input.single_line().placeholder("12 mars 2021, ou 2021", p.faint),
            5 => input.single_line().placeholder("Action, RPG… séparés par des virgules", p.faint),
            _ => input.single_line(),
        };
        if !multi {
            input = input.on_submit(|s: &mut State, tx| s.save_edit(tx));
        }
        let field = row().w_full().min_h(if multi { 96.0 } else { 38.0 }).px(12.0).py(if multi { 9.0 } else { 0.0 }).radius(5.0)
            .fill(p.field).border(1.0, p.line)
            .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
            .child(input);
        let field = if multi { field.items_start() } else { field.center_y() };
        form = form.child(col().gap(5.0).child(text(label.to_string()).font(caption()).color(p.faint)).child(field));
    }
    let reset = edited.then(|| {
        button(p, "edit-reset", "Rétablir l'original", None, Tone::Secondary).on_press_tx(|s: &mut State, tx: &mut Tx| s.reset_edit(tx))
    });
    let card = dialog(p, "Modifier les informations", 560.0)
        .child(text("Vos corrections priment sur celles des stores et restent après chaque analyse.").font(ui(12.5, 450.0)).color(p.muted).wrap())
        .child(form)
        .child(row().w_full().pt(10.0).gap(8.0).center_y()
            .children(reset)
            .child(el().grow(1.0))
            .child(button(p, "dialog-cancel", "Annuler", None, Tone::Secondary).on_press(close))
            .child(accent_button(p, "dialog-ok", "Enregistrer").default_button().on_press_tx(|s: &mut State, tx: &mut Tx| s.save_edit(tx))));
    layer.push(veil(p));
    layer.push(el().key("dialog-center").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().child(card));
}

fn accent_button(p: &Palette, key: &str, label: &str) -> El {
    row().key(key.to_string()).role("button").label(label.to_string()).h(34.0).px(16.0).radius(5.0).center()
        .fill(p.accent)
        .border(1.0, Color::WHITE.alpha(0.12))
        .hover(Color::WHITE.alpha(0.1))
        .press_scale(0.97)
        .child(text(label.to_string()).font(ui(13.0, 650.0)).color(p.on_accent))
}

// ---------------------------------------------------------------- choose a picture

/// Pictures to pick from (Steam, SteamGridDB), a file of one's own, or back to the original.
pub fn art_picker(s: &State, p: &Palette, layer: &mut Vec<El>) {
    let Overlay::ArtPicker { id, kind } = &s.overlay else { return };
    let kind = *kind;
    let custom = s.user.art.get(id).is_some_and(|a| match kind {
        Kind::Cover => a.cover.is_some(),
        Kind::Hero => a.hero.is_some(),
        Kind::Logo => a.logo.is_some(),
    });
    let kinds = [Kind::Cover, Kind::Hero, Kind::Logo];
    let labels: Vec<&str> = kinds.iter().map(|k| k.label()).collect();
    let tabs = segmented(p, "art-kind", &labels, kinds.iter().position(|k| *k == kind).unwrap_or(0),
        move |s, i, tx| s.picker_kind(kinds[i], tx));
    let field = row().h(36.0).px(12.0).gap(10.0).radius(5.0).center_y().grow(1.0).fill(p.field).border(1.0, p.line)
        .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
        .child(icon(Icon::Search, 15.0, p.faint))
        .child(text_input(&s.picker.query, TextStyle::new(13.5, 450.0).lh(20.0))
            .key("picker-query").single_line().grow(1.0).color(p.text)
            .placeholder("Nom du jeu", p.faint).label("Rechercher des images")
            .on_edit(|s: &mut State| &mut s.picker.query)
            .on_submit(|s: &mut State, tx| s.picker_search(tx)));
    let (tw, th) = match kind {
        Kind::Cover => (120.0, 180.0),
        Kind::Hero => (250.0, 82.0),
        Kind::Logo => (180.0, 90.0),
    };
    let mut grid = row().key("picker-grid").w_full().gap(10.0).flex_wrap();
    for (i, c) in s.picker.results.iter().enumerate() {
        let busy = s.picker.choosing == Some(i);
        let mut frame = el().key("thumb").w(tw).h(th).radius(6.0).clip_rounded(6.0)
            .fill(if kind == Kind::Logo { rune::oklch(0.3, 0.01, 260.0) } else { p.text.alpha(0.06) });
        if let Some(img) = s.images.borrow_mut().get(&c.thumb, (tw * 2.0) as u32) {
            frame = frame.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .pixels_fit(img.pixels(), if kind == Kind::Logo { rune::SurfaceFit::Fit } else { rune::SurfaceFit::Fill }));
        }
        grid = grid.child(
            col().key(format!("cand-{i}")).role("button").label(format!("Choisir l'image {} ({})", i + 1, c.label)).help(c.label.clone())
                .w(tw).gap(5.0).hover_group().press_scale(0.97)
                .child(frame
                    .child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(6.0).border(2.0, p.accent).group_opacity(0.0, 1.0))
                    .children(busy.then(|| el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(Color::BLACK.alpha(0.5)).center()
                        .child(text("…").font(ui(18.0, 700.0)).color(Color::WHITE)))))
                .child(text(c.label.clone()).font(ui(11.0, 500.0)).color(p.faint).truncate().w(tw))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.picker_choose(i, tx)),
        );
    }
    let status = if s.picker.loading {
        Some("Recherche…".to_string())
    } else if s.picker.done && s.picker.results.is_empty() {
        Some(if s.settings.sgdb_key.is_empty() {
            "Rien trouvé sur Steam. Une clé SteamGridDB (Paramètres › En ligne) donne bien plus de choix.".to_string()
        } else {
            "Rien trouvé : essayez un autre nom.".to_string()
        })
    } else {
        None
    };
    let card = dialog(p, "Changer les images", 860.0)
        .child(row().w_full().gap(10.0).center_y().child(tabs).child(field)
            .child(button(p, "picker-go", "Rechercher", None, Tone::Secondary).on_press_tx(|s: &mut State, tx: &mut Tx| s.picker_search(tx))))
        .child(col().key("picker-scroll").w_full().h(440.0).scroll(rune::ScrollSpec { stick_bottom: false })
            .children(status.map(|t| text(t).font(ui(13.0, 450.0)).color(p.muted).pt(8.0).wrap()))
            .child(grid))
        .child(text("Astuce : glissez une image sur la fiche du jeu. Verticale, elle devient la jaquette ; horizontale, l'image de fond.")
            .font(ui(12.0, 450.0)).color(p.faint).wrap())
        .child(row().w_full().pt(6.0).gap(8.0).center_y()
            .child(button(p, "picker-file", "Choisir un fichier…", Some(Icon::Folder), Tone::Secondary)
                .on_press_tx(|s: &mut State, tx: &mut Tx| s.picker_file(tx)))
            .children(custom.then(|| button(p, "picker-reset", "Rétablir l'original", None, Tone::Secondary)
                .on_press_tx(|s: &mut State, tx: &mut Tx| s.picker_reset(tx))))
            .child(el().grow(1.0))
            .child(button(p, "dialog-cancel", "Fermer", None, Tone::Secondary).on_press(close)));
    layer.push(veil(p));
    // Images dropped on the open dialog go to the kind it shows.
    layer.push(el().key("dialog-center").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().child(
        card.on_drop_files(move |s: &mut State, files, tx| {
            if let Some(f) = files.first() { s.picker_drop(f, tx); }
        }).accepts_extensions(&["png", "jpg", "jpeg"]),
    ));
}
