//! Arach: every game on this PC, from every store and every drive, in one window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod detect;
mod discord;
mod drives;
mod extras;
mod game;
mod launch;
mod mcp;
mod meta;
mod search;
mod settings;
mod store;
mod system;
mod thumbs;
mod track;
mod ui;
mod update;

use rune::{App, AppSpec, Titlebar};

use app::{Confirm, Overlay, Page, State, Tab};

fn main() {
    // `arach --update`: the whole update without a window.
    if std::env::args().any(|a| a == "--update") {
        std::process::exit(update::cli());
    }
    // `arach --mcp`: the MCP server an AI assistant starts, on stdin/stdout, beside the window.
    if std::env::args().any(|a| a == "--mcp") {
        std::process::exit(mcp::serve());
    }
    // Headless snapshots run beside an open window.
    let headless = std::env::args().any(|a| a == "--snap");
    // After an update, the old process lets go of the single instance first.
    let updated = !headless && update::finish();
    if !headless && !rune::single_instance("com.arach.launcher") {
        return; // the running window comes forward instead
    }
    // The exe's icon resource (assets/arach.rc), for the title bar, taskbar and Alt+Tab.
    #[cfg(windows)]
    rune::set_app_icon_resource(1);
    rune::prewarm_gpu();
    // Small JSON files: the last scan shows on the first frame, before any disk walk.
    let (library, user, settings, cache) = (store::load_library(), store::load_user(), store::load_settings(), store::load_cache());

    let mut spec = AppSpec::new("Arach", (1280.0, 800.0), || unreachable!(), ui::view);
    spec.min_size = Some((880.0, 560.0));
    spec.setup = Some(|s: &mut State, waker| s.setup(waker));
    spec.poll = Some(|s: &mut State, now, _| s.poll(now));
    spec.on_key = Some(ui::window_keys);
    // Headless snapshots: `arach --snap out.png --scene library --frames 120`.
    spec.scenes = &[
        ("library", |s| s.scene_page(Page::Library)),
        ("list", |s| {
            s.settings.library_view = settings::LibraryView::List;
            s.scene_page(Page::Library)
        }),
        ("search", |s| s.scene_query("kcd")),
        ("game", |s| s.scene_page(Page::Game("gog:1423049311".into()))),
        ("game-steam", |s| s.scene_page(Page::Game("steam:1771300".into()))),
        ("game-3ds", |s| {
            let id = s.games.iter().find(|g| g.source == game::Source::N3ds).map(|g| g.id.clone()).unwrap_or_default();
            s.scene_page(Page::Game(id))
        }),
        ("library-ds", |s| {
            s.shelf = app::Shelf::Store(game::Source::Ds);
            s.refilter();
            s.scene_page(Page::Library)
        }),
        ("settings", |s| s.scene_page(Page::Settings(Tab::General))),
        ("playing", |s| s.scene_playing("steam:1771300")),
        ("settings-library", |s| s.scene_page(Page::Settings(Tab::Library))),
        ("settings-look", |s| s.scene_page(Page::Settings(Tab::Appearance))),
        ("settings-about", |s| s.scene_page(Page::Settings(Tab::About))),
        ("settings-ai", |s| s.scene_page(Page::Settings(Tab::Assistants))),
        ("update", |s| {
            let release = update::Release {
                version: "9.9.0".into(),
                page: update::releases_page(),
                exe_url: String::new(),
                size: 0,
                sha256_url: None,
            };
            s.update = app::Update::Ready { release, path: Default::default() };
            s.scene_page(Page::Settings(Tab::About))
        }),
        ("light", |s| s.settings.theme = settings::ThemeMode::Light),
        ("sort-open", |s| s.scene_overlay(Page::Library, Overlay::Select("sort"))),
        ("menu", |s| s.scene_overlay(Page::Library, Overlay::GameMenu { id: "steam:1771300".into(), at: Some((620.0, 300.0)) })),
        ("more", |s| s.scene_overlay(Page::Game("steam:1771300".into()), Overlay::GameMenu { id: "steam:1771300".into(), at: None })),
        ("settings-select", |s| s.scene_overlay(Page::Settings(Tab::General), Overlay::Select("after"))),
        ("confirm", |s| s.scene_overlay(Page::Settings(Tab::Online), Overlay::Confirm(Confirm::ClearCache))),
        ("toast", |s| s.scene_notice("Le lancement a échoué : aucune application n'ouvre ce lien", true)),
        ("filters", |s| s.scene_overlay(Page::Library, Overlay::Filters)),
        ("selecting", |s| {
            s.selecting = true;
            s.selected = s.visible.iter().take(3).map(|&i| s.games[i].id.clone()).collect();
            s.scene_page(Page::Library)
        }),
        ("edit", |s| s.scene_edit("steam:1771300")),
        ("picker", |s| s.scene_overlay(Page::Game("steam:1771300".into()),
            Overlay::ArtPicker { id: "steam:1771300".into(), kind: meta::picker::Kind::Cover })),
    ];

    // `arach://launch/<id>`: an assistant asked for a game (`mcp`), through this window.
    let app = App::new(spec).on_open_url(State::open_link);
    let me = app.handle();
    // Opens maximized, with its caption buttons; F11 toggles full screen.
    app.init(move || {
        let mut s = State::new(me, library, user, settings, cache);
        s.just_updated = updated;
        s
    })
        .titlebar(Titlebar::new(ui::TOPBAR))
        .maximized(true)
        .run();
}
