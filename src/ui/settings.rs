//! Paramètres: one page, six sections. Every change applies and saves at once; there is no
//! "Enregistrer" to forget.

use rune::{Color, El, FileDialogOptions, Tx, col, el, row, text};

use super::icons::{Icon, icon};
use super::overlay::select;
use super::theme::{ACCENTS, Palette, caption, display, mono, num, store_color, ui};
use super::widgets::{Tone, ago, button, game_icon, ghost_button, keycaps, link, plural, segmented, size, store_dot, switch, tool_button};
use crate::app::{Confirm, Overlay, Page, Scope, State, Tab, Update};
use crate::detect::emulators;
use crate::game::{Game, Source};
use crate::settings::{Actions, AfterLaunch, CoverSize, LibraryView, StartPage, ThemeMode};
use crate::system;

const PAD: f32 = 40.0;
/// Below this the section list goes above the content instead of beside it.
const SIDE_NAV: f32 = 900.0;

pub fn view(s: &State, p: &Palette, tab: Tab, w: f32, layer: &mut Vec<El>) -> El {
    let body = match tab {
        Tab::General => general(s, p, layer),
        Tab::Library => library(s, p),
        Tab::Appearance => appearance(s, p, layer),
        Tab::Online => online(s, p, layer),
        Tab::Hidden => hidden(s, p),
        Tab::About => about(s, p),
    };
    let side = w >= SIDE_NAV;
    let content = col().key(format!("section-{tab:?}")).grow(1.0).min_w(0.0).max_w(760.0).gap(32.0).child(body);
    let page = col().key("settings").w_full().px(PAD).pt(84.0).pb(64.0).gap(28.0)
        .child(text("Paramètres").font(display(42.0)).color(p.text));
    if side {
        page.child(row().w_full().gap(40.0).items_start().child(nav(s, p, tab, true)).child(content))
    } else {
        page.child(nav(s, p, tab, false)).child(content)
    }
}

/// The sections, as a list beside the content or as tabs above it.
fn nav(s: &State, p: &Palette, tab: Tab, vertical: bool) -> El {
    let hidden = s.user.hidden.len();
    let mut list = if vertical {
        col().key("tabs").role("tablist").focus_group().w(196.0).gap(2.0)
    } else {
        row().key("tabs").role("tablist").focus_group().w_full().gap(4.0).flex_wrap().pb(4.0)
    };
    for t in Tab::ALL {
        let on = t == tab;
        let count = (t == Tab::Hidden && hidden > 0).then(|| text(hidden.to_string()).font(num(12.0, 500.0)).color(p.faint));
        list = list.child(
            row().key(format!("tab-{t:?}")).role("tab").label(t.label()).checked(on).focus_selected(on)
                .h(36.0).px(12.0).gap(8.0).radius(5.0).center_y()
                .fill(if on { p.text.alpha(if p.dark { 0.075 } else { 0.06 }) } else { Color::TRANSPARENT })
                .hover(if on { Color::TRANSPARENT } else { p.hover })
                .press_scale(0.98)
                .child(text(t.label()).font(ui(13.5, if on { 600.0 } else { 500.0 })).color(if on { p.text } else { p.muted }).grow(1.0))
                .children(count)
                .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                    if t == Tab::Online { s.measure_cache(); }
                    s.go(Page::Settings(t), tx);
                }),
        );
    }
    list
}

// ---------------------------------------------------------------- building blocks

fn group(p: &Palette, title: &str, rows: Vec<El>) -> El {
    col().w_full().gap(10.0).child(text(title.to_string()).font(ui(13.0, 650.0)).color(p.muted).px(4.0)).child(card(p, rows))
}

/// Rows on a card, a hairline between each.
fn card(p: &Palette, rows: Vec<El>) -> El {
    let mut card = col().w_full().radius(8.0).fill(p.surface).border(1.0, p.line);
    if !p.dark {
        card = card.shadow(1.0, 3.0, p.shadow.alpha(0.05));
    }
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        card = card.child(r);
        if i + 1 < n {
            card = card.child(el().h(1.0).ml(20.0).fill(p.line));
        }
    }
    card
}

fn setting(p: &Palette, title: &str, detail: &str, control: El) -> El {
    row().w_full().min_h(64.0).px(20.0).py(14.0).gap(24.0).center_y()
        .child(col().grow(1.0).min_w(0.0).gap(3.0)
            .child(text(title.to_string()).font(ui(14.0, 600.0)).color(p.text))
            .children((!detail.is_empty()).then(|| text(detail.to_string()).font(ui(13.0, 450.0)).color(p.faint).wrap())))
        .child(control)
}

fn toggle(p: &Palette, key: &str, label: &str, on: bool, flip: impl Fn(&mut crate::settings::Settings) + 'static) -> El {
    switch(p, key, label, on).on_press_tx(move |s: &mut State, tx: &mut Tx| s.set(tx, &flip))
}

// ---------------------------------------------------------------- sections

const AFTER: [(AfterLaunch, &str); 3] =
    [(AfterLaunch::Minimize, "Réduire Arach"), (AfterLaunch::Stay, "Rester ouvert"), (AfterLaunch::Quit, "Quitter Arach")];
const START: [(StartPage, &str); 2] = [(StartPage::Home, "Accueil"), (StartPage::Library, "Bibliothèque")];
const COVERS: [(CoverSize, &str); 3] = [(CoverSize::Small, "Petites"), (CoverSize::Medium, "Moyennes"), (CoverSize::Large, "Grandes")];
const LANGS: [(bool, &str); 2] = [(true, "Français"), (false, "English")];

fn general(s: &State, p: &Palette, layer: &mut Vec<El>) -> El {
    let after = select("after", "Après avoir lancé un jeu", &AFTER, s.settings.after_launch).width(180.0)
        .build(s, p, layer, |s, v, tx| s.set(tx, |st| st.after_launch = v));
    let start = select("start", "Page d'ouverture", &START, s.settings.start_page).width(180.0)
        .build(s, p, layer, |s, v, tx| s.set(tx, |st| st.start_page = v));
    let launching = group(p, "Lancement", vec![
        setting(p, "Après avoir lancé un jeu",
            if s.settings.after_launch == AfterLaunch::Quit {
                "Arach fermé, le temps de jeu n'est pas mesuré. Un jeu qui a des actions après lui réduit Arach à la place, pour les faire."
            } else {
                "Ce que fait Arach une fois le jeu démarré. Il mesure le temps de jeu tant qu'il reste ouvert."
            },
            after),
        setting(p, "Page d'ouverture", "La page affichée quand Arach démarre.", start),
    ]);
    let keys = [
        ("Ctrl K", "Rechercher (ou tapez n'importe où)"),
        ("Entrée", "Lancer le premier résultat de la recherche"),
        ("Alt ←", "Page précédente"),
        ("Ctrl 1 / 2 / 3", "Accueil, Bibliothèque, Favoris"),
        ("Ctrl ,", "Paramètres"),
        ("F5", "Relancer la détection"),
        ("F11", "Plein écran"),
    ];
    let shortcut_rows = keys.iter().map(|(k, what)| {
        row().w_full().h(48.0).px(20.0).gap(16.0).center_y()
            .child(text(what.to_string()).font(ui(13.5, 500.0)).color(p.text).grow(1.0))
            .child(keycaps(p, k))
    }).chain(std::iter::once(
        row().w_full().h(48.0).px(20.0).gap(16.0).center_y()
            .child(text("Lancer un jeu depuis sa jaquette").font(ui(13.5, 500.0)).color(p.text).grow(1.0))
            .child(text("Double-clic").font(ui(12.5, 550.0)).color(p.muted)),
    )).collect();
    let around = group(p, "Avant et après chaque jeu", action_rows(s, p, layer, Scope::Global));
    col().w_full().gap(32.0)
        .child(launching)
        .child(col().w_full().gap(10.0)
            .child(around)
            .child(note(p, "Un jeu peut avoir les siens : sur sa fiche, « Avant et après le lancement ». Tout ce qui change est remis comme avant à la fermeture du jeu.")))
        .child(discord(s, p))
        .child(group(p, "Raccourcis clavier", shortcut_rows))
}

fn note(p: &Palette, t: &str) -> El {
    text(t.to_string()).font(ui(12.5, 450.0)).color(p.faint).wrap().px(4.0)
}

// ---------------------------------------------------------------- around a game

/// One script field: what runs before (0) or after (1) the game.
fn script_field(s: &State, p: &Palette, scope: &Scope, which: usize) -> El {
    let game = matches!(scope, Scope::Game(_));
    let field = if game { &s.game_scripts[which] } else { &s.scripts[which] };
    let key = format!("script-{}-{which}", if game { "game" } else { "all" });
    let (label, hint) = if which == 0 {
        ("Script avant le jeu", r"C:\Scripts\avant.bat")
    } else {
        ("Script après le jeu", r"C:\Scripts\apres.bat")
    };
    let scope = scope.clone();
    row().key(format!("{key}-box")).w(280.0).h(34.0).px(10.0).radius(5.0).center_y().fill(p.field).border(1.0, p.line)
        .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
        .child(rune::text_input(field, mono(12.5).lh(18.0))
            .key(key).single_line().grow(1.0).color(p.text)
            .placeholder(hint, p.faint).label(label)
            .on_edit(move |s: &mut State| if game { &mut s.game_scripts[which] } else { &mut s.scripts[which] })
            .on_input_change(move |s: &mut State, tx: &mut Tx| s.script_changed(&scope, which, tx)))
}

/// The rows that set what happens around a game, for every game or for one.
fn action_rows(s: &State, p: &Palette, layer: &mut Vec<El>, scope: Scope) -> Vec<El> {
    let a = s.scope_actions(&scope).clone();
    let game = matches!(scope, Scope::Game(_));

    let plans = system::power_plans();
    let plan_options: Vec<(usize, &'static str)> =
        std::iter::once((0, "Ne pas changer")).chain(plans.iter().enumerate().map(|(i, pl)| (i + 1, pl.name))).collect();
    let plan_now = plans.iter().position(|pl| pl.guid == a.power_plan).map_or(0, |i| i + 1);
    let sc = scope.clone();
    let power = select(if game { "power-game" } else { "power-all" }, "Mode d'alimentation", &plan_options, plan_now).width(220.0)
        .build(s, p, layer, move |s, v, tx| {
            let guid = v.checked_sub(1).and_then(|i| system::power_plans().get(i)).map(|pl| pl.guid.clone()).unwrap_or_default();
            s.edit_actions(&sc, tx, |a| a.power_plan = guid);
        });

    let modes = system::resolutions();
    let res_options: Vec<(usize, &'static str)> =
        std::iter::once((0, "Ne pas changer")).chain(modes.iter().enumerate().map(|(i, m)| (i + 1, m.1))).collect();
    let res_now = a.resolution.and_then(|r| modes.iter().position(|m| m.0 == r)).map_or(0, |i| i + 1);
    let sc = scope.clone();
    let resolution = select(if game { "res-game" } else { "res-all" }, "Résolution de l'écran", &res_options, res_now).width(220.0)
        .build(s, p, layer, move |s, v, tx| {
            let r = v.checked_sub(1).and_then(|i| system::resolutions().get(i)).map(|m| m.0);
            s.edit_actions(&sc, tx, |a| a.resolution = r);
        });

    let sc = scope.clone();
    let discord = switch(p, if game { "discord-close-game" } else { "discord-close-all" }, "Fermer Discord", a.close_discord)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| s.edit_actions(&sc, tx, |a| a.close_discord = !a.close_discord));

    vec![
        setting(p, "Script avant le jeu", "Une commande ou un fichier .bat, .ps1… Arach attend qu'il finisse (une minute au plus).",
            script_field(s, p, &scope, 0)),
        setting(p, "Script après le jeu", "Lancé une fois le jeu fermé.", script_field(s, p, &scope, 1)),
        setting(p, "Fermer Discord", "Fermé au lancement, rouvert (réduit) après le jeu.", discord),
        setting(p, "Mode d'alimentation", "Le mode d'avant revient après le jeu.", power),
        setting(p, "Résolution de l'écran", "Écran principal. La résolution habituelle revient après le jeu.", resolution),
    ]
}

/// "Fermer Discord · Performances élevées · 1920 × 1080": what runs around every game.
fn summary(a: &Actions) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !a.before.trim().is_empty() { parts.push("script avant".into()); }
    if a.close_discord { parts.push("fermer Discord".into()); }
    if let Some(pl) = system::power_plans().iter().find(|pl| pl.guid == a.power_plan) { parts.push(pl.name.to_string()); }
    if let Some((w, h)) = a.resolution { parts.push(format!("{w} × {h}")); }
    if !a.after.trim().is_empty() { parts.push("script après".into()); }
    let mut out = parts.join(" · ");
    if let Some(c) = out.get(..1) {
        out = c.to_uppercase() + &out[1..];
    }
    out
}

/// The game page's section: everyone's actions, or the game's own.
pub fn game_actions(s: &State, p: &Palette, g: &Game, layer: &mut Vec<El>) -> El {
    let own = s.has_own_actions(&g.id);
    let id = g.id.clone();
    let choice = segmented(p, "actions-scope", &["Comme les autres jeux", "Propres à ce jeu"], own as usize,
        move |s: &mut State, i, tx: &mut Tx| s.set_own_actions(&id, i == 1, tx));
    let mut section = col().key("around").w_full().gap(12.0)
        .child(super::widgets::section_head(p, "Avant et après le lancement", None))
        .child(row().child(choice));
    if own {
        section = section.child(card(p, action_rows(s, p, layer, Scope::Game(g.id.clone()))));
    } else {
        let all = summary(&s.settings.actions);
        let line = if all.is_empty() {
            "Aucune action pour l'instant. Paramètres › Général les règle pour tous les jeux.".to_string()
        } else {
            format!("Pour tous les jeux : {all}.")
        };
        section = section.child(text(line).font(ui(12.5, 450.0)).color(p.faint).wrap().max_w(560.0));
    }
    if !s.trackable(g) {
        section = section.child(row().gap(8.0).center_y().max_w(560.0)
            .child(icon(Icon::Alert, 14.0, p.faint))
            .child(text("Arach ne voit pas quand ce jeu se ferme : il démarre sans ces actions.").font(ui(12.5, 450.0)).color(p.faint).wrap()));
    }
    section
}

fn discord(s: &State, p: &Palette) -> El {
    let on = s.settings.discord_presence;
    let missing = s.settings.discord_app_id.is_empty();
    let field = row().key("discord-id-box").w(220.0).h(34.0).px(10.0).radius(5.0).center_y().fill(p.field)
        .border(1.0, if on && missing { p.danger.alpha(0.6) } else { p.line })
        .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
        .child(rune::text_input(&s.discord_id, mono(12.5).lh(18.0))
            .key("discord-id").single_line().grow(1.0).color(p.text)
            .placeholder("123456789012345678", p.faint).label("ID d'application Discord")
            .on_edit(|s: &mut State| &mut s.discord_id)
            .on_input_change(|s: &mut State, tx: &mut Tx| s.discord_id_changed(tx)));
    let id_help = if missing {
        "Requis. Sur discord.com/developers/applications : New Application, nommez-la « Arach », puis copiez l'Application ID ici."
    } else {
        "L'application Discord sous laquelle l'activité s'affiche."
    };
    group(p, "Discord", vec![
        setting(p, "Afficher le jeu en cours",
            "Votre profil Discord indique le jeu lancé depuis Arach et depuis quand, quel que soit son store. Discord doit être ouvert.",
            toggle(p, "discord-presence", "Afficher le jeu en cours sur Discord", on, |st| st.discord_presence = !st.discord_presence)),
        setting(p, "ID d'application", id_help, field),
    ])
}

fn library(s: &State, p: &Palette) -> El {
    let mut stores = Vec::new();
    for src in Source::STORES.into_iter().filter(|s| !Source::CONSOLES.contains(s)) {
        let on = s.settings.stores.contains(&src);
        let n = s.games.iter().filter(|g| g.source == src).count();
        let detail = if !on { "Ignoré".to_string() } else if n == 0 { "Aucun jeu trouvé".into() } else { plural(n, "jeu détecté", "jeux détectés") };
        stores.push(
            row().w_full().h(60.0).px(20.0).gap(14.0).center_y()
                .child(el().size(32.0, 32.0).radius(6.0).center().fill(store_color(src).alpha(0.14)).child(store_dot(src, 9.0)))
                .child(col().grow(1.0).gap(2.0)
                    .child(text(src.label()).font(ui(14.0, 600.0)).color(if on { p.text } else { p.muted }))
                    .child(text(detail).font(ui(12.5, 450.0)).color(p.faint)))
                .child(toggle(p, &format!("store-{src:?}"), src.label(), on, move |st| {
                    if let Some(i) = st.stores.iter().position(|x| *x == src) { st.stores.remove(i); } else { st.stores.push(src); }
                })),
        );
    }

    let mut folders: Vec<El> = s.settings.folders.iter().map(|dir| {
        let n = s.games.iter().filter(|g| g.source == Source::Folder && g.dir.as_ref().is_some_and(|d| d.starts_with(dir))).count();
        let target = dir.clone();
        row().w_full().h(60.0).px(20.0).gap(14.0).center_y()
            .child(el().size(32.0, 32.0).radius(6.0).center().fill(store_color(Source::Folder).alpha(0.14))
                .child(icon(Icon::Folder, 17.0, store_color(Source::Folder))))
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(dir.display().to_string()).font(mono(12.5)).color(p.text).truncate_middle())
                .child(text(plural(n, "jeu", "jeux")).font(ui(12.5, 450.0)).color(p.faint)))
            .child(tool_button(p, format!("rm-{}", dir.display()), Icon::Trash, "Retirer ce dossier", 32.0)
                .on_press(move |s: &mut State| s.overlay = Overlay::Confirm(Confirm::RemoveFolder(target.clone()))))
    }).collect();
    folders.push(
        row().w_full().min_h(60.0).px(20.0).py(12.0).gap(16.0).center_y()
            .child(text("Chaque sous-dossier contenant un .exe devient un jeu.").font(ui(13.0, 450.0)).color(p.faint).wrap().grow(1.0).min_w(0.0))
            .child(ghost_button(p, "add-folder", "Ajouter un dossier…", Some(Icon::Plus)).on_press_tx(|_: &mut State, tx: &mut Tx| {
                let options = FileDialogOptions { title: "Dossier contenant des jeux".into(), ..Default::default() };
                tx.open_folder(options, |s: &mut State, picked, tx| match picked {
                    Ok(Some(dir)) => s.add_folder(dir, tx),
                    Ok(None) => {}
                    Err(e) => s.notify(format!("Impossible d'ouvrir le sélecteur : {e}"), true, tx),
                });
            })),
    );

    let scan_detail = if s.scanning {
        "Analyse en cours…".to_string()
    } else if s.user.last_scan > 0 {
        format!("Dernière analyse {} · {} ms", ago(s.user.last_scan).to_lowercase(), s.user.last_scan_ms)
    } else {
        "Jamais".into()
    };
    let scan = group(p, "Analyse", vec![
        setting(p, "Analyser au démarrage", "La bibliothèque s'affiche aussitôt, puis se met à jour en arrière-plan.",
            toggle(p, "scan-start", "Analyser au démarrage", s.settings.scan_on_start, |st| st.scan_on_start = !st.scan_on_start)),
        setting(p, "Jeux possédés non installés",
            "Les jeux achetés sur Steam, Epic et GOG mais pas installés apparaissent grisés, avec un bouton Installer.",
            toggle(p, "show-owned", "Jeux possédés non installés", s.settings.show_owned, |st| st.show_owned = !st.show_owned)),
        setting(p, "Analyser maintenant", &scan_detail,
            ghost_button(p, "scan-now", if s.scanning { "Analyse…" } else { "Analyser" }, Some(Icon::Refresh))
                .on_press_tx(|s: &mut State, _: &mut Tx| s.rescan())),
    ]);
    col().w_full().gap(32.0)
        .child(group(p, "Stores", stores))
        .child(group(p, "Consoles", consoles(s, p)))
        .child(group(p, "Émulateurs", emulator_rows(s, p)))
        .child(group(p, "Dossiers de ROM", rom_folders(s, p)))
        .child(group(p, "Dossiers de jeux", folders))
        .child(scan)
}

/// Each console, on or off, with what plays its games.
fn consoles(s: &State, p: &Palette) -> Vec<El> {
    let found = s.emulators.as_ref();
    Source::CONSOLES.into_iter().map(|src| {
        let on = s.settings.stores.contains(&src);
        let n = s.games.iter().filter(|g| g.source == src).count();
        let games = if !on { "Ignoré".to_string() } else { plural(n, "jeu", "jeux") };
        let standalone = emulators::console(src).map_or(&[][..], |c| c.emulators);
        let by = standalone.iter().find(|id| found.is_some_and(|f| f.emulators.contains_key(*id))).and_then(|id| emulators::emulator(id));
        let core = found.and_then(|f| f.retroarch()).filter(|ra| ra.core_for(src).is_some());
        let player = match (by, core, found) {
            (Some(e), _, _) => format!("joué avec {}", e.name),
            (None, Some(_), _) => "joué avec RetroArch".to_string(),
            _ if src == Source::Retro => "listés dans les playlists de RetroArch".to_string(),
            (None, None, None) => "recherche de l'émulateur…".to_string(),
            (None, None, Some(_)) => {
                let names: Vec<&str> = standalone.iter().filter_map(|id| emulators::emulator(id)).map(|e| e.name).collect();
                if names.is_empty() { "aucun émulateur trouvé".to_string() } else { format!("{} introuvable", names.join(" ou ")) }
            }
        };
        row().w_full().min_h(60.0).px(20.0).py(10.0).gap(14.0).center_y()
            .child(el().size(32.0, 32.0).radius(6.0).center().fill(store_color(src).alpha(0.14)).child(store_dot(src, 9.0)))
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(src.label()).font(ui(14.0, 600.0)).color(if on { p.text } else { p.muted }))
                .child(text(format!("{games} · {player}")).font(ui(12.5, 450.0)).color(p.faint).truncate_middle()))
            .child(toggle(p, &format!("store-{src:?}"), src.label(), on, move |st| {
                if let Some(i) = st.stores.iter().position(|x| *x == src) { st.stores.remove(i); } else { st.stores.push(src); }
            }))
    }).collect()
}

/// Every emulator Arach knows, found or picked by hand.
fn emulator_rows(s: &State, p: &Palette) -> Vec<El> {
    let found = s.emulators.as_ref();
    emulators::EMULATORS.iter().map(|emu| {
        let (id, name, exe) = (emu.id, emu.name, emu.exe_hint);
        let path = found.and_then(|f| f.emulators.get(id));
        let picked = s.settings.emulators.contains_key(id);
        let detail = match (path, found) {
            (Some(path), _) => format!("{}{}", if picked { "Choisi : " } else { "" }, path.display()),
            (None, None) => "Recherche…".to_string(),
            (None, Some(_)) => format!("Introuvable : choisissez {exe}"),
        };
        let choose = tool_button(p, format!("emu-{id}"), Icon::Folder, &format!("Choisir {name}"), 32.0)
            .on_press_tx(move |_: &mut State, tx: &mut Tx| {
                let options = FileDialogOptions { title: format!("Exécutable de {name}"), ..Default::default() }
                    .filter(name, &["exe"]);
                tx.open_file(options, move |s: &mut State, picked, tx| match picked {
                    Ok(Some(exe)) => s.set(tx, |st| { st.emulators.insert(id.to_string(), exe); }),
                    Ok(None) => {}
                    Err(e) => s.notify(format!("Impossible d'ouvrir le sélecteur : {e}"), true, tx),
                });
            });
        // Back to looking for it alone.
        let auto = picked.then(|| {
            tool_button(p, format!("emu-auto-{id}"), Icon::Refresh, "Détecter automatiquement", 32.0)
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.set(tx, |st| { st.emulators.remove(id); }))
        });
        let color = if path.is_some() { p.accent } else { p.faint };
        row().w_full().min_h(56.0).px(20.0).py(8.0).gap(14.0).center_y()
            .child(el().size(32.0, 32.0).radius(6.0).center().fill(color.alpha(0.12)).child(icon(Icon::Play, 15.0, color)))
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(name).font(ui(14.0, 600.0)).color(if path.is_some() { p.text } else { p.muted }))
                .child(text(detail).font(ui(12.5, 450.0)).color(p.faint).truncate_middle()))
            .children(auto)
            .child(choose)
    }).collect()
}

/// Folders the emulators already list (kept as they are) and the ones added here.
fn rom_folders(s: &State, p: &Palette) -> Vec<El> {
    let count = |dir: &std::path::Path| {
        s.games.iter().filter(|g| g.rom.is_some() && g.dir.as_ref().is_some_and(|d| d.starts_with(dir))).count()
    };
    let line = |key: String, dir: &std::path::Path, detail: String, action: Option<El>| {
        row().key(key).w_full().h(60.0).px(20.0).gap(14.0).center_y()
            .child(el().size(32.0, 32.0).radius(6.0).center().fill(store_color(Source::N3ds).alpha(0.14))
                .child(icon(Icon::Folder, 17.0, store_color(Source::N3ds))))
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(dir.display().to_string()).font(mono(12.5)).color(p.text).truncate_middle())
                .child(text(detail).font(ui(12.5, 450.0)).color(p.faint)))
            .children(action)
    };
    let mut rows: Vec<El> = Vec::new();
    for d in s.emulators.iter().flat_map(|e| e.dirs.iter()).filter(|d| !d.from.is_empty()) {
        let deep = if d.deep { " · sous-dossiers compris" } else { "" };
        rows.push(line(format!("romdir-{}", d.path.display()), &d.path,
            format!("{} · listé par {}{deep}", plural(count(&d.path), "jeu", "jeux"), d.from), None));
    }
    for dir in &s.settings.roms {
        let target = dir.clone();
        let remove = tool_button(p, format!("rm-rom-{}", dir.display()), Icon::Trash, "Retirer ce dossier", 32.0)
            .on_press(move |s: &mut State| s.overlay = Overlay::Confirm(Confirm::RemoveFolder(target.clone())));
        rows.push(line(format!("romdir-own-{}", dir.display()), dir,
            format!("{} · sous-dossiers compris", plural(count(dir), "jeu", "jeux")), Some(remove)));
    }
    rows.push(
        row().w_full().min_h(60.0).px(20.0).py(12.0).gap(16.0).center_y()
            .child(text("ROM et images disque de toutes les consoles ci-dessus : .nds, .3ds, .gba, .iso, .rvz, .chd, .cue, .nsp, dossiers PS3…")
                .font(ui(13.0, 450.0)).color(p.faint).wrap().grow(1.0).min_w(0.0))
            .child(ghost_button(p, "add-roms", "Ajouter un dossier…", Some(Icon::Plus)).on_press_tx(|_: &mut State, tx: &mut Tx| {
                let options = FileDialogOptions { title: "Dossier contenant des ROM".into(), ..Default::default() };
                tx.open_folder(options, |s: &mut State, picked, tx| match picked {
                    Ok(Some(dir)) => {
                        if s.settings.roms.contains(&dir) { return; }
                        s.notify(format!("Analyse de {}…", dir.display()), false, tx);
                        s.set(tx, |st| st.roms.push(dir));
                    }
                    Ok(None) => {}
                    Err(e) => s.notify(format!("Impossible d'ouvrir le sélecteur : {e}"), true, tx),
                });
            })),
    );
    rows
}

fn appearance(s: &State, p: &Palette, layer: &mut Vec<El>) -> El {
    let theme = match s.settings.theme { ThemeMode::System => 0, ThemeMode::Dark => 1, ThemeMode::Light => 2 };
    let view = match s.settings.library_view { LibraryView::Grid => 0, LibraryView::List => 1 };
    let mut swatches = row().role("radiogroup").label("Couleur du bouton Jouer").focus_group().gap(8.0);
    for (i, (name, d, l, _)) in ACCENTS.iter().enumerate() {
        let c = if p.dark { *d } else { *l };
        let on = s.settings.accent == i;
        // The chosen colour wears a ring set off from it by the card's own colour.
        swatches = swatches.child(
            el().key(format!("accent-{i}")).role("radio").label(*name).checked(on).focus_selected(on).help(*name)
                .size(30.0, 30.0).radius(15.0).center()
                .border(2.0, if on { p.text.alpha(0.9) } else { Color::TRANSPARENT })
                .press_scale(0.92)
                .child(el().size(22.0, 22.0).radius(11.0).fill(rune::oklch(c.0, c.1, c.2))
                    .border(1.0, Color::WHITE.alpha(0.12)))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.set(tx, |st| st.accent = i)),
        );
    }
    let covers = select("covers", "Taille des jaquettes", &COVERS, s.settings.cover_size).width(150.0)
        .build(s, p, layer, |s, v, tx| s.set(tx, |st| st.cover_size = v));
    col().w_full().gap(32.0)
        .child(group(p, "Couleurs", vec![
            setting(p, "Thème", "Système suit le réglage de Windows.",
                segmented(p, "theme", &["Système", "Sombre", "Clair"], theme, |s, i, tx| {
                    s.set(tx, |st| st.theme = [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light][i]);
                })),
            setting(p, "Couleur du bouton Jouer", ACCENTS[s.settings.accent.min(ACCENTS.len() - 1)].0, swatches),
            setting(p, "Teinte d'ambiance", "La fiche d'un jeu prend les couleurs de son illustration.",
                toggle(p, "ambient", "Teinte d'ambiance", s.settings.ambient, |st| st.ambient = !st.ambient)),
        ]))
        .child(group(p, "Bibliothèque", vec![
            setting(p, "Taille des jaquettes", "Dans la bibliothèque et sur l'accueil.", covers),
            setting(p, "Affichage", "Jaquettes ou liste détaillée (taille, temps de jeu).",
                segmented(p, "libview", &["Jaquettes", "Liste"], view, |s, i, tx| {
                    s.set(tx, |st| st.library_view = if i == 0 { LibraryView::Grid } else { LibraryView::List });
                })),
        ]))
}

fn online(s: &State, p: &Palette, layer: &mut Vec<El>) -> El {
    let found = s.cache.meta.values().filter(|m| m.found()).count();
    let status = if s.fetching > 0 {
        format!("{} restantes…", s.fetching)
    } else {
        format!("{found} jeux trouvés sur {}", s.games.len())
    };
    let lang = select("lang", "Langue des descriptions", &LANGS, s.settings.french).width(150.0)
        .build(s, p, layer, |s, v, tx| s.set(tx, |st| st.french = v));
    col().w_full().gap(32.0)
        .child(group(p, "Steam et Epic Games Store", vec![
            setting(p, "Jaquettes et descriptions",
                "Arach demande au Steam Store public (ou à l’Epic Games Store pour les jeux absents de Steam) les illustrations, la description, le studio et la date de sortie. Seuls les noms des jeux sont envoyés. Pour les jeux de console, seul le code du jeu (ou le nom du fichier) est envoyé à GameTDB et libretro.",
                toggle(p, "online", "Jaquettes et descriptions", s.settings.online, |st| st.online = !st.online)),
            setting(p, "Langue des descriptions", "", lang),
            setting(p, "Mettre à jour", &status,
                ghost_button(p, "fetch-now", "Mettre à jour", Some(Icon::Refresh)).on_press_tx(|s: &mut State, _: &mut Tx| s.fetch_meta())),
        ]))
        .child(group(p, "SteamGridDB", vec![
            setting(p, "Clé d'API",
                "Facultative : avec une clé (gratuite, sur steamgriddb.com › Préférences › API), « Changer les images » propose aussi les jaquettes, fonds et logos de la communauté SteamGridDB. Elle n'est envoyée qu'à SteamGridDB.",
                row().key("sgdb-box").w(260.0).h(34.0).px(10.0).radius(5.0).center_y().fill(p.field).border(1.0, p.line)
                    .focus_within_ring(2.0, 1.0, p.accent.alpha(0.5))
                    .child(rune::text_input(&s.sgdb_field, mono(12.5).lh(18.0))
                        .key("sgdb-key").single_line().secure().grow(1.0).color(p.text)
                        .placeholder("Coller la clé ici", p.faint).label("Clé d'API SteamGridDB")
                        .on_edit(|s: &mut State| &mut s.sgdb_field)
                        .on_input_change(|s: &mut State, tx: &mut Tx| {
                            let key = s.sgdb_field.text.trim().to_string();
                            s.set(tx, |st| st.sgdb_key = key);
                        }))),
        ]))
        .child(group(p, "Cache", vec![
            setting(p, "Illustrations téléchargées", &format!("{} dans {}", size(s.cache_bytes), crate::meta::art_dir().display()),
                ghost_button(p, "clear-cache", "Vider le cache…", Some(Icon::Trash))
                    .on_press(|s: &mut State| s.overlay = Overlay::Confirm(Confirm::ClearCache))),
        ]))
}

fn hidden(s: &State, p: &Palette) -> El {
    let games: Vec<_> = s.games.iter().filter(|g| s.is_hidden(&g.id)).collect();
    if games.is_empty() {
        return group(p, "Jeux masqués", vec![
            col().w_full().py(32.0).px(24.0).gap(8.0).center()
                .child(el().size(44.0, 44.0).radius(8.0).center().fill(p.text.alpha(0.06)).child(icon(Icon::Eye, 20.0, p.muted)))
                .child(text("Aucun jeu masqué").font(ui(14.0, 600.0)).color(p.text).pt(4.0))
                .child(text("Clic droit sur un jeu › Masquer pour le retirer de la bibliothèque sans le désinstaller.")
                    .font(ui(13.0, 450.0)).color(p.faint).wrap().align(rune::Align::Center)),
        ]);
    }
    let rows = games.iter().map(|g| {
        let id = g.id.clone();
        row().w_full().h(60.0).px(20.0).gap(14.0).center_y()
            .child(game_icon(s, g, p, 34.0))
            .child(col().grow(1.0).min_w(0.0).gap(2.0)
                .child(text(g.name.clone()).font(ui(14.0, 600.0)).color(p.text).truncate())
                .child(row().gap(6.0).center_y().child(store_dot(g.source, 6.0)).child(text(g.source.label()).font(ui(12.5, 450.0)).color(p.faint))))
            .child(ghost_button(p, format!("unhide-{}", crate::app::fnv(&g.id)), "Afficher", Some(Icon::Eye))
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.toggle_hidden(&id, tx)))
    }).collect();
    group(p, &format!("Jeux masqués · {}", games.len()), rows)
}

fn about(s: &State, p: &Palette) -> El {
    let stores = Source::ALL.iter().filter(|src| s.count(**src) > 0).count();
    let drives = s.shown().filter_map(|g| g.drive()).collect::<std::collections::BTreeSet<_>>().len();
    let total: u64 = s.shown().map(|g| g.size).sum();
    let hours: u64 = s.shown().map(|g| g.playtime_min).sum::<u64>() / 60;
    let stat = |label: &str, value: String| {
        col().grow(1.0).min_w(110.0).px(18.0).py(16.0).gap(4.0)
            .child(text(label.to_string()).font(caption()).color(p.faint))
            .child(text(value).font(num(22.0, 650.0)).color(p.text))
    };
    let divider = || el().w(1.0).my(16.0).fill(p.line);
    let path_row = |label: &str, dir: std::path::PathBuf| {
        let open = dir.clone();
        setting(p, label, &dir.display().to_string(),
            ghost_button(p, format!("open-{label}"), "Ouvrir", Some(Icon::Folder)).on_press_tx(move |s: &mut State, tx: &mut Tx| {
                let _ = std::fs::create_dir_all(&open);
                if let Err(e) = crate::launch::open_folder(&open) { s.notify(e, true, tx); }
            }))
    };
    col().w_full().gap(32.0)
        .child(row().w_full().radius(8.0).fill(p.surface).border(1.0, p.line).flex_wrap()
            .child(stat("Jeux", s.shown().count().to_string()))
            .child(divider())
            .child(stat("Stores", stores.to_string()))
            .child(divider())
            .child(stat("Disques", drives.to_string()))
            .child(divider())
            .child(stat("Installé", size(total)))
            .child(divider())
            .child(stat("Heures Steam", hours.to_string())))
        .child(group(p, "Arach", vec![
            setting(p, &format!("Version {}", env!("CARGO_PKG_VERSION")), "Rendu GPU natif avec Rune · détection sans parcours de disque", el()),
            path_row("Données", crate::store::data_dir()),
            path_row("Cache", crate::store::cache_dir()),
        ]))
        .child(group(p, "Mises à jour", updates(s, p)))
}

/// Where Arach's own update stands, and the one button that moves it on.
fn updates(s: &State, p: &Palette) -> Vec<El> {
    let check = |label: &str| {
        ghost_button(p, "update-check", label, Some(Icon::Refresh)).on_press(|s: &mut State| s.check_update(true))
    };
    let (title, detail, control) = match &s.update {
        Update::Idle => ("Nouvelle version".to_string(), "Les versions d'Arach sont publiées sur GitHub.".to_string(), check("Rechercher")),
        Update::UpToDate => ("Arach est à jour".to_string(), format!("La version {} est la plus récente.", env!("CARGO_PKG_VERSION")), check("Rechercher")),
        Update::Checking => ("Recherche…".to_string(), "Arach demande à GitHub sa dernière version.".to_string(), el()),
        Update::Downloading { version, done, total } => {
            let pct = if *total > 0 { done * 100 / total } else { 0 };
            (format!("Téléchargement d'Arach {version}"), format!("{} sur {} · {pct} %", size(*done), size(*total)), el())
        }
        Update::Ready { release, .. } => {
            let notes = release.page.clone();
            (
                format!("Arach {} est prêt", release.version),
                "Arach redémarre dans la nouvelle version ; la bibliothèque et les réglages sont gardés.".to_string(),
                row().gap(14.0).center_y()
                    .child(link(p, "update-notes", "Nouveautés").on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_uri(&notes, tx)))
                    .child(button(p, "update-install", "Redémarrer et installer", Some(Icon::Download), Tone::Primary)
                        .on_press_tx(|s: &mut State, tx: &mut Tx| s.install_update(tx))),
            )
        }
        Update::Failed(e) => (
            "La mise à jour a échoué".to_string(),
            e.clone(),
            row().gap(14.0).center_y()
                .child(link(p, "update-github", "GitHub").on_press_tx(|s: &mut State, tx: &mut Tx| s.open_uri(&crate::update::releases_page(), tx)))
                .child(check("Réessayer")),
        ),
    };
    vec![
        setting(p, &title, &detail, control),
        setting(p, "Rechercher au démarrage", "Une nouvelle version se télécharge en arrière-plan ; elle s'installe quand vous le décidez.",
            toggle(p, "auto-update", "Rechercher les mises à jour au démarrage", s.settings.auto_update, |st| st.auto_update = !st.auto_update)),
    ]
}
