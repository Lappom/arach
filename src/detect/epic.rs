//! Epic Games: one JSON `.item` manifest per installed app, wherever it was installed.

use std::path::PathBuf;

use serde::Deserialize;

use crate::game::{Game, Launch, Source, created};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Item {
    display_name: String,
    install_location: String,
    #[serde(default)]
    launch_executable: String,
    catalog_namespace: String,
    catalog_item_id: String,
    app_name: String,
    #[serde(default)]
    main_game_app_name: String,
    #[serde(default)]
    app_categories: Vec<String>,
    #[serde(default, rename = "bIsIncompleteInstall")]
    incomplete: bool,
    #[serde(default)]
    install_size: u64,
}

pub fn detect() -> Vec<Game> {
    let data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let Ok(read) = std::fs::read_dir(data.join(r"Epic\EpicGamesLauncher\Data\Manifests")) else { return Vec::new() };
    read.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "item"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|bytes| serde_json::from_slice::<Item>(&bytes).ok())
        .filter_map(game)
        .collect()
}

fn game(item: Item) -> Option<Game> {
    let is_game = item.app_categories.iter().any(|c| c == "games");
    let is_dlc = !item.main_game_app_name.is_empty() && item.main_game_app_name != item.app_name;
    if !is_game || is_dlc || item.incomplete { return None; }
    let dir = PathBuf::from(&item.install_location);
    if !dir.is_dir() { return None; }
    let exe = dir.join(&item.launch_executable);
    let icon = if !item.launch_executable.is_empty() && exe.is_file() {
        Some(exe)
    } else {
        super::exe::pick_exe(&dir, &item.display_name)
    };
    Some(Game {
        id: format!("epic:{}", item.app_name),
        launch: Launch::Uri(format!(
            "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
            item.catalog_namespace, item.catalog_item_id, item.app_name
        )),
        name: item.display_name,
        source: Source::Epic,
        installed: created(&dir),
        size: item.install_size,
        dir: Some(dir),
        icon,
        ..Default::default()
    })
}
