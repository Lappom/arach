//! Legendary (the command-line Epic client) and Heroic, which runs Legendary for Epic, its own
//! client for GOG and Nile for Amazon. Each keeps an `installed.json`.

use std::path::{Path, PathBuf};

use super::exe::pick_exe;
use crate::game::{Game, Launch, Source, created};

/// Games Legendary installed by itself, started through `legendary launch` when it's on the
/// PATH, else directly.
pub fn legendary() -> Vec<Game> {
    let config = std::env::var_os("LEGENDARY_CONFIG_PATH")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config/legendary")));
    let Some(config) = config else { return Vec::new() };
    let cli = on_path("legendary.exe");
    legendary_installed(&config.join("installed.json"))
        .into_iter()
        .filter_map(|e| {
            let launch = match &cli {
                Some(cli) => Launch::Exe { path: cli.clone(), args: format!("launch {}", e.app), cwd: None },
                None => Launch::Exe { path: e.exe.clone()?, args: e.args.clone(), cwd: Some(e.dir.clone()) },
            };
            Some(e.game(Source::Legendary, format!("legendary:{}", e.app), launch))
        })
        .collect()
}

/// Everything Heroic installed, started through its `heroic://` link so its settings (Wine,
/// overlays, cloud saves) apply.
pub fn heroic() -> Vec<Game> {
    let Some(root) = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("heroic")) else { return Vec::new() };
    let link = |runner: &str, app: &str| Launch::Uri(format!("heroic://launch?appName={app}&runner={runner}"));
    let mut out = Vec::new();
    for e in legendary_installed(&root.join("legendaryConfig/legendary/installed.json")) {
        out.push(e.game(Source::Heroic, format!("heroic:legendary:{}", e.app), link("legendary", &e.app)));
    }
    if let Some(v) = json(&root.join("gog_store/installed.json")) {
        for item in v.get("installed").and_then(|i| i.as_array()).into_iter().flatten() {
            let s = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (app, dir) = (s("appName"), PathBuf::from(s("install_path")));
            if app.is_empty() || !dir.is_dir() || item.get("is_dlc").and_then(|d| d.as_bool()) == Some(true) { continue; }
            // `goggame-<id>.info` holds the title.
            let name = json(&dir.join(format!("goggame-{app}.info")))
                .and_then(|i| i.get("name").and_then(|n| n.as_str()).map(String::from))
                .unwrap_or_else(|| folder_name(&dir));
            let ico = dir.join(format!("goggame-{app}.ico"));
            let e = Entry { app: app.clone(), title: name, exe: None, args: String::new(), icon: ico.is_file().then_some(ico), dir };
            out.push(e.game(Source::Heroic, format!("heroic:gog:{app}"), link("gog", &app)));
        }
    }
    if let Some(serde_json::Value::Array(items)) = json(&root.join("nile_config/nile/installed.json")) {
        for item in items {
            let s = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (app, dir) = (s("id"), PathBuf::from(s("path")));
            if app.is_empty() || !dir.is_dir() { continue; }
            let e = Entry { app: app.clone(), title: folder_name(&dir), exe: None, args: String::new(), icon: None, dir };
            out.push(e.game(Source::Heroic, format!("heroic:nile:{app}"), link("nile", &app)));
        }
    }
    out
}

struct Entry {
    app: String,
    title: String,
    dir: PathBuf,
    exe: Option<PathBuf>,
    args: String,
    icon: Option<PathBuf>,
}

impl Entry {
    fn game(&self, source: Source, id: String, launch: Launch) -> Game {
        let exe = self.exe.clone().or_else(|| pick_exe(&self.dir, &self.title));
        Game {
            id,
            name: self.title.clone(),
            source,
            icon: self.icon.clone().or(exe),
            launch,
            installed: created(&self.dir),
            dir: Some(self.dir.clone()),
            ..Default::default()
        }
    }
}

/// Legendary's `installed.json`: `{ "<AppName>": { "title", "install_path", "executable", … } }`.
fn legendary_installed(path: &Path) -> Vec<Entry> {
    let Some(serde_json::Value::Object(map)) = json(path) else { return Vec::new() };
    let mut out = Vec::new();
    for (app, v) in map {
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        if v.get("is_dlc").and_then(|d| d.as_bool()) == Some(true) { continue; }
        let dir = PathBuf::from(s("install_path"));
        if !dir.is_dir() { continue; }
        let exe = Some(dir.join(s("executable"))).filter(|p| !s("executable").is_empty() && p.is_file());
        let title = Some(s("title")).filter(|t| !t.is_empty()).unwrap_or_else(|| app.clone());
        out.push(Entry { app, title, dir, exe, args: s("launch_parameters"), icon: None });
    }
    out
}

fn json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn on_path(exe: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(exe)).find(|p| p.is_file())
}
