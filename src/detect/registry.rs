//! Stores that register their games with Windows: GOG and Ubisoft keep their own keys; Riot,
//! EA and Battle.net are read from the system's uninstall entries, read once and shared.

use std::path::{Path, PathBuf};

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

use super::exe::pick_exe;
use crate::game::{Game, Launch, Source, created};

/// One entry of "Apps & features".
pub struct Uninstall {
    pub key: String,
    pub name: String,
    pub publisher: String,
    pub location: String,
    pub icon: String,
    pub uninstall: String,
}

pub fn uninstall_entries() -> Vec<Uninstall> {
    let roots = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];
    let mut out = Vec::new();
    for (hive, path) in roots {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else { continue };
        for key in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey_with_flags(&key, KEY_READ) else { continue };
            let s = |v: &str| k.get_value::<String, _>(v).unwrap_or_default();
            let name = s("DisplayName");
            if name.is_empty() { continue; }
            out.push(Uninstall {
                key,
                name,
                publisher: s("Publisher"),
                location: s("InstallLocation"),
                icon: s("DisplayIcon"),
                uninstall: s("UninstallString"),
            });
        }
    }
    out
}

/// `"C:\x\game.exe",0` → `C:\x\game.exe`, if it exists.
fn icon_path(display_icon: &str) -> Option<PathBuf> {
    let raw = display_icon.trim();
    let raw = match raw.rfind(',') {
        Some(i) if raw[i + 1..].trim().parse::<i32>().is_ok() => &raw[..i],
        _ => raw,
    };
    let p = PathBuf::from(raw.trim().trim_matches('"'));
    let ext = p.extension()?.to_str()?.to_ascii_lowercase();
    (matches!(ext.as_str(), "exe" | "ico") && p.is_file()).then_some(p)
}

fn existing_dir(s: &str) -> Option<PathBuf> {
    let p = PathBuf::from(s.trim().trim_matches('"'));
    (!s.trim().is_empty() && p.is_dir()).then_some(p)
}

// ---------------------------------------------------------------- GOG

pub fn gog() -> Vec<Game> {
    let Ok(root) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for id in root.enum_keys().flatten() {
        let Ok(k) = root.open_subkey(&id) else { continue };
        let s = |v: &str| k.get_value::<String, _>(v).unwrap_or_default();
        if !s("dependsOn").trim().is_empty() { continue; } // a DLC of another entry
        let Some(dir) = existing_dir(&s("path")) else { continue };
        let name = s("gameName");
        let exe = PathBuf::from(s("exe"));
        let exe = if exe.is_file() { Some(exe) } else { pick_exe(&dir, &name) };
        let Some(exe) = exe else { continue };
        let ico = dir.join(format!("goggame-{id}.ico"));
        let cwd = existing_dir(&s("workingDir")).unwrap_or_else(|| dir.clone());
        out.push(Game {
            id: format!("gog:{id}"),
            name,
            source: Source::Gog,
            icon: Some(if ico.is_file() { ico } else { exe.clone() }),
            launch: Launch::Exe { path: exe, args: s("launchParam"), cwd: Some(cwd) },
            installed: created(&dir),
            dir: Some(dir),
            ..Default::default()
        });
    }
    out
}

// ---------------------------------------------------------------- Ubisoft Connect

pub fn ubisoft(entries: &[Uninstall]) -> Vec<Game> {
    let Ok(root) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs")
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for id in root.enum_keys().flatten() {
        let Ok(k) = root.open_subkey(&id) else { continue };
        let Some(dir) = existing_dir(&k.get_value::<String, _>("InstallDir").unwrap_or_default()) else { continue };
        let entry = entries.iter().find(|e| e.key == format!("Uplay Install {id}"));
        let name = entry.map(|e| e.name.clone()).unwrap_or_else(|| folder_name(&dir));
        let icon = entry.and_then(|e| icon_path(&e.icon)).or_else(|| pick_exe(&dir, &name));
        out.push(Game {
            id: format!("ubisoft:{id}"),
            name,
            source: Source::Ubisoft,
            launch: Launch::Uri(format!("uplay://launch/{id}/0")),
            dir: Some(dir),
            icon,
            ..Default::default()
        });
    }
    out
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---------------------------------------------------------------- Riot Games

pub fn riot(entries: &[Uninstall]) -> Vec<Game> {
    let Some(client) = riot_client() else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries {
        // "Riot Game league_of_legends.live"; "Riot Game Riot_Client." is the client itself.
        let Some(product) = e.key.strip_prefix("Riot Game ") else { continue };
        let Some((product, patchline)) = product.rsplit_once('.') else { continue };
        if product.eq_ignore_ascii_case("riot_client") || patchline.is_empty() { continue; }
        let Some(dir) = existing_dir(&e.location) else { continue };
        out.push(Game {
            id: format!("riot:{product}.{patchline}"),
            name: e.name.trim().to_string(),
            source: Source::Riot,
            icon: icon_path(&e.icon),
            launch: Launch::Exe {
                path: client.clone(),
                args: format!("--launch-product={product} --launch-patchline={patchline}"),
                cwd: None,
            },
            installed: created(&dir),
            dir: Some(dir),
            ..Default::default()
        });
    }
    out
}

fn riot_client() -> Option<PathBuf> {
    let data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let src = std::fs::read(data.join(r"Riot Games\RiotClientInstalls.json")).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&src).ok()?;
    ["rc_default", "rc_live", "rc_beta"]
        .iter()
        .filter_map(|k| json.get(k)?.as_str())
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

// ---------------------------------------------------------------- EA app

pub fn ea(entries: &[Uninstall]) -> Vec<Game> {
    let mut out = Vec::new();
    for e in entries {
        if !e.publisher.contains("Electronic Arts") || e.name.starts_with("EA app") { continue; }
        let Some(dir) = existing_dir(&e.location) else { continue };
        let installer = dir.join("__Installer");
        if !installer.is_dir() { continue; } // not installed by the EA app
        let exe = ea_exe(&installer, &dir).or_else(|| pick_exe(&dir, &e.name));
        let Some(exe) = exe else { continue };
        out.push(Game {
            id: format!("ea:{}", e.key.trim_matches(|c| c == '{' || c == '}')),
            name: e.name.clone(),
            source: Source::Ea,
            icon: icon_path(&e.icon).or_else(|| Some(exe.clone())),
            launch: Launch::Exe { path: exe, args: String::new(), cwd: Some(dir.clone()) },
            installed: created(&dir),
            dir: Some(dir),
            ..Default::default()
        });
    }
    out
}

/// `installerdata.xml` names the game binary as `<filePath>[HKEY…\Install Dir]game.exe</filePath>`.
fn ea_exe(installer: &Path, dir: &Path) -> Option<PathBuf> {
    let xml = std::fs::read_to_string(installer.join("installerdata.xml")).ok()?;
    let mut rest = xml.as_str();
    while let Some(start) = rest.find("<filePath>") {
        rest = &rest[start + "<filePath>".len()..];
        let end = rest.find("</filePath>")?;
        let value = &rest[..end];
        let rel = value.rsplit(']').next().unwrap_or(value).trim().trim_start_matches(['\\', '/']);
        let p = dir.join(rel);
        if rel.to_ascii_lowercase().ends_with(".exe") && p.is_file() { return Some(p); }
    }
    None
}

// ---------------------------------------------------------------- Battle.net

/// Uninstall `--uid=` prefixes → the product code `Battle.net.exe --exec="launch <code>"` takes.
const BNET_CODES: &[(&str, &str)] = &[
    ("wow_classic_era", "WoW_wow_classic_era"), ("wow_classic", "WoW_wow_classic"), ("wow", "WoW"),
    ("prometheus", "Pro"), ("pro", "Pro"), ("osi", "OSI"), ("fenris", "Fen"), ("diablo3", "D3"), ("d3", "D3"),
    ("s2", "S2"), ("s1", "S1"), ("hs_beta", "WTCG"), ("hero", "Hero"), ("w3", "W3"), ("odin", "ODIN"),
    ("viper", "VIPR"), ("lazarus", "LAZR"), ("zeus", "ZEUS"), ("fore", "FORE"), ("auks", "AUKS"),
    ("anbs", "ANBS"), ("rtro", "RTRO"), ("wlby", "WLBY"), ("spot", "SPOT"),
];

pub fn battlenet(entries: &[Uninstall]) -> Vec<Game> {
    let client = entries
        .iter()
        .find(|e| e.name == "Battle.net")
        .and_then(|e| existing_dir(&e.location))
        .map(|d| d.join("Battle.net.exe"))
        .filter(|p| p.is_file())
        .or_else(|| Some(PathBuf::from(r"C:\Program Files (x86)\Battle.net\Battle.net.exe")).filter(|p| p.is_file()));
    let mut out = Vec::new();
    for e in entries {
        if !e.publisher.contains("Blizzard") || e.name == "Battle.net" { continue; }
        let Some(dir) = existing_dir(&e.location) else { continue };
        let uid = e.uninstall.split("--uid=").nth(1).map(|u| u.split([' ', '"']).next().unwrap_or("").to_lowercase());
        let code = uid.as_deref().and_then(|u| BNET_CODES.iter().find(|(p, _)| u.starts_with(p)).map(|(_, c)| *c));
        let exe = pick_exe(&dir, &e.name);
        let launch = match (code, &client, &exe) {
            (Some(code), Some(client), _) => {
                Launch::Exe { path: client.clone(), args: format!("--exec=\"launch {code}\""), cwd: None }
            }
            (_, _, Some(exe)) => Launch::Exe { path: exe.clone(), args: String::new(), cwd: Some(dir.clone()) },
            _ => continue,
        };
        out.push(Game {
            id: format!("bnet:{}", uid.unwrap_or_else(|| crate::game::norm_dir(&dir))),
            name: e.name.clone(),
            source: Source::BattleNet,
            icon: icon_path(&e.icon).or(exe),
            launch,
            installed: created(&dir),
            dir: Some(dir),
            ..Default::default()
        });
    }
    out
}

// ---------------------------------------------------------------- Amazon Games

/// The app uninstalls its games with `Amazon Game Remover.exe -m Game -p <product id>`; that
/// id is what its play link takes.
pub fn amazon(entries: &[Uninstall]) -> Vec<Game> {
    let mut out = Vec::new();
    for e in entries {
        if !e.uninstall.contains("Amazon Game Remover") { continue; }
        let Some(id) = arg_after(&e.uninstall, "-p") else { continue };
        let Some(dir) = existing_dir(&e.location) else { continue };
        let exe = pick_exe(&dir, &e.name);
        out.push(Game {
            id: format!("amazon:{id}"),
            name: e.name.clone(),
            source: Source::Amazon,
            icon: icon_path(&e.icon).or(exe),
            launch: Launch::Uri(format!("amazon-games://play/{id}")),
            installed: created(&dir),
            dir: Some(dir),
            ..Default::default()
        });
    }
    out
}

/// The word after `flag` on a command line: `… -p amzn1.adg.product.x` → `amzn1.adg.product.x`.
fn arg_after(cmd: &str, flag: &str) -> Option<String> {
    let mut words = cmd.split_whitespace();
    words.by_ref().find(|w| *w == flag)?;
    let v = words.next()?.trim_matches('"');
    (!v.is_empty()).then(|| v.to_string())
}

// ---------------------------------------------------------------- Rockstar Games Launcher

/// Each game's uninstaller is the launcher's own, given `-uninstall=<title>`; the launcher
/// starts a game from its folder.
pub fn rockstar(entries: &[Uninstall]) -> Vec<Game> {
    let mut out = Vec::new();
    for e in entries {
        let Some(title) = e.uninstall.split("-uninstall=").nth(1) else { continue };
        let title = title.split([' ', '"']).next().unwrap_or("").to_ascii_lowercase();
        if title.is_empty() || matches!(title.as_str(), "launcher" | "socialclub" | "rockstargameslauncher") { continue; }
        let uninstaller = PathBuf::from(e.uninstall.split("-uninstall=").next().unwrap_or("").trim().trim_matches('"'));
        let Some(launcher) = uninstaller.parent().map(|d| d.join("Launcher.exe")).filter(|p| p.is_file()) else { continue };
        let Some(dir) = existing_dir(&e.location) else { continue };
        let exe = pick_exe(&dir, &e.name);
        out.push(Game {
            id: format!("rockstar:{title}"),
            name: e.name.clone(),
            source: Source::Rockstar,
            icon: icon_path(&e.icon).or(exe),
            launch: Launch::Exe {
                path: launcher.clone(),
                args: format!("-launchTitleInFolder \"{}\"", dir.display()),
                cwd: launcher.parent().map(Path::to_path_buf),
            },
            installed: created(&dir),
            dir: Some(dir),
            ..Default::default()
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_amazon_product_ids() {
        let cmd = r#""C:\Users\a\AppData\Local\Amazon Games\App\Amazon Game Remover.exe" -m Game -p amzn1.adg.product.1234"#;
        assert_eq!(arg_after(cmd, "-p").as_deref(), Some("amzn1.adg.product.1234"));
    }

    #[test]
    fn display_icon_drops_its_index_and_quotes() {
        let exe = std::env::current_exe().unwrap();
        let raw = format!("\"{}\",0", exe.display());
        assert_eq!(icon_path(&raw), Some(exe.clone()));
        assert_eq!(icon_path(&exe.display().to_string()), Some(exe));
        assert_eq!(icon_path(r"C:\nope\missing.exe,0"), None);
    }
}
