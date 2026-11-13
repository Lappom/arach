//! Which emulator plays which console, where each one is, and which ROM folders it already
//! knows about. Every emulator is found the same way: the one the user picked, else a single
//! crawl of the usual places (Program Files, the user's folders, near the ROMs, the top of every
//! drive) for a folder named after it holding its executable. RetroArch is the fallback: a
//! console with no standalone emulator plays through one of RetroArch's cores.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::game::{Source, norm_dir};

/// What Arach knows about a console.
pub struct Console {
    pub source: Source,
    /// Standalone emulators that play it, best first (ids into [`EMULATORS`]).
    pub emulators: &'static [&'static str],
    /// RetroArch cores that play it, best first (file stems in RetroArch's `cores` folder).
    pub cores: &'static [&'static str],
    /// libretro's name for the system: its thumbnails and databases are filed under it.
    pub libretro: &'static str,
    /// The libretro-database DAT tying game codes to names (`no-intro/…`, `redump/…`).
    pub dat: Option<&'static str>,
    /// GameTDB's folder for the console, when it has box art for it.
    pub gametdb: Option<&'static str>,
    /// Whether libretro's title screens hold both screens, one above the other (DS, 3DS).
    pub dual_screen: bool,
}

pub const CONSOLES: [Console; 9] = [
    Console {
        source: Source::Ds,
        emulators: &["desmume"],
        cores: &["melondsds_libretro", "melonds_libretro", "desmume_libretro", "desmume2015_libretro"],
        libretro: "Nintendo - Nintendo DS",
        dat: Some("no-intro/Nintendo - Nintendo DS"),
        gametdb: Some("ds"),
        dual_screen: true,
    },
    Console {
        source: Source::N3ds,
        emulators: &["azahar"],
        cores: &["citra_libretro", "azahar_libretro"],
        libretro: "Nintendo - Nintendo 3DS",
        dat: Some("no-intro/Nintendo - Nintendo 3DS"),
        gametdb: Some("3ds"),
        dual_screen: true,
    },
    Console {
        source: Source::Gba,
        emulators: &["mgba"],
        cores: &["mgba_libretro", "vbam_libretro", "vba_next_libretro", "gpsp_libretro"],
        libretro: "Nintendo - Game Boy Advance",
        dat: Some("no-intro/Nintendo - Game Boy Advance"),
        gametdb: None,
        dual_screen: false,
    },
    Console {
        source: Source::Gc,
        emulators: &["dolphin"],
        cores: &["dolphin_libretro"],
        libretro: "Nintendo - GameCube",
        dat: Some("redump/Nintendo - GameCube"),
        gametdb: Some("wii"),
        dual_screen: false,
    },
    Console {
        source: Source::Wii,
        emulators: &["dolphin"],
        cores: &["dolphin_libretro"],
        libretro: "Nintendo - Wii",
        dat: Some("redump/Nintendo - Wii"),
        gametdb: Some("wii"),
        dual_screen: false,
    },
    Console {
        source: Source::Ps1,
        emulators: &["duckstation"],
        cores: &["swanstation_libretro", "mednafen_psx_hw_libretro", "mednafen_psx_libretro", "pcsx_rearmed_libretro"],
        libretro: "Sony - PlayStation",
        dat: Some("redump/Sony - PlayStation"),
        gametdb: None,
        dual_screen: false,
    },
    Console {
        source: Source::Ps2,
        emulators: &["pcsx2"],
        cores: &["pcsx2_libretro", "play_libretro"],
        libretro: "Sony - PlayStation 2",
        dat: Some("redump/Sony - PlayStation 2"),
        gametdb: None,
        dual_screen: false,
    },
    Console {
        source: Source::Ps3,
        emulators: &["rpcs3"],
        cores: &[],
        libretro: "Sony - PlayStation 3",
        dat: Some("redump/Sony - PlayStation 3"),
        gametdb: Some("ps3"),
        dual_screen: false,
    },
    Console {
        source: Source::Switch,
        emulators: &["ryujinx"],
        cores: &[],
        libretro: "Nintendo - Nintendo Switch",
        dat: None,
        gametdb: None,
        dual_screen: false,
    },
];

pub fn console(source: Source) -> Option<&'static Console> {
    CONSOLES.iter().find(|c| c.source == source)
}

/// The console libretro files a system under: `Nintendo - Game Boy Advance` → `Gba`.
pub fn console_by_libretro(system: &str) -> Option<Source> {
    CONSOLES.iter().find(|c| c.libretro.eq_ignore_ascii_case(system)).map(|c| c.source)
}

/// A standalone emulator.
pub struct Emulator {
    /// Key in `Settings::emulators`.
    pub id: &'static str,
    pub name: &'static str,
    /// What to look for in the file dialog.
    pub exe_hint: &'static str,
    /// Folder names worth looking into.
    hints: &'static [&'static str],
    is_exe: fn(&str) -> bool,
    /// The command line before the ROM, which is appended quoted.
    pub args: &'static str,
}

pub const EMULATORS: [Emulator; 9] = [
    Emulator {
        id: "azahar",
        name: "Azahar",
        exe_hint: "azahar.exe",
        hints: &["azahar", "lime3ds", "citra"],
        is_exe: |f| matches!(f, "azahar.exe" | "lime3ds.exe" | "lime3ds-gui.exe" | "lime-qt.exe" | "citra-qt.exe"),
        args: "",
    },
    Emulator {
        id: "desmume",
        name: "DeSmuME",
        exe_hint: "DeSmuME*.exe",
        hints: &["desmume"],
        is_exe: |f| f.starts_with("desmume") && f.ends_with(".exe") && !f.contains("dev"),
        args: "",
    },
    Emulator {
        id: "mgba",
        name: "mGBA",
        exe_hint: "mGBA.exe",
        hints: &["mgba"],
        is_exe: |f| f == "mgba.exe",
        args: "",
    },
    Emulator {
        id: "dolphin",
        name: "Dolphin",
        exe_hint: "Dolphin.exe",
        hints: &["dolphin"],
        is_exe: |f| matches!(f, "dolphin.exe" | "dolphinwx.exe"),
        args: "-b -e",
    },
    Emulator {
        id: "duckstation",
        name: "DuckStation",
        exe_hint: "duckstation-qt*.exe",
        hints: &["duckstation"],
        is_exe: |f| f.starts_with("duckstation") && f.ends_with(".exe") && !f.contains("updater") && !f.contains("regtest"),
        args: "-batch --",
    },
    Emulator {
        id: "pcsx2",
        name: "PCSX2",
        exe_hint: "pcsx2-qt*.exe",
        hints: &["pcsx2"],
        is_exe: |f| f.starts_with("pcsx2") && f.ends_with(".exe") && !f.contains("updater"),
        args: "-batch --",
    },
    Emulator {
        id: "rpcs3",
        name: "RPCS3",
        exe_hint: "rpcs3.exe",
        hints: &["rpcs3"],
        is_exe: |f| f == "rpcs3.exe",
        args: "--no-gui",
    },
    Emulator {
        id: "ryujinx",
        name: "Ryujinx",
        exe_hint: "Ryujinx.exe",
        hints: &["ryujinx", "ryubing"],
        is_exe: |f| f == "ryujinx.exe",
        args: "",
    },
    Emulator {
        id: "retroarch",
        name: "RetroArch",
        exe_hint: "retroarch.exe",
        hints: &["retroarch"],
        is_exe: |f| f == "retroarch.exe",
        args: "",
    },
];

pub fn emulator(id: &str) -> Option<&'static Emulator> {
    EMULATORS.iter().find(|e| e.id == id)
}

/// True for an emulator's own executable: a games folder holding DeSmuME isn't a game.
pub fn is_emulator(exe: &Path) -> bool {
    let f = exe.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    EMULATORS.iter().any(|e| (e.is_exe)(&f)) || f == "azahar-room.exe"
}

/// Where the user's picks aren't enough: looks for every emulator not yet found in one crawl.
/// `near` are folders worth looking around (games and ROM folders).
pub fn find_all(picked: &BTreeMap<String, PathBuf>, near: &[PathBuf]) -> HashMap<&'static str, PathBuf> {
    let mut found: HashMap<&'static str, PathBuf> = HashMap::new();
    for e in &EMULATORS {
        if let Some(p) = picked.get(e.id).filter(|p| p.is_file()) {
            found.insert(e.id, p.clone());
        }
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut roots: Vec<(PathBuf, usize)> = Vec::new();
    for k in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(p) = env(k) { roots.push((p, 1)); }
    }
    if let Some(p) = env("LOCALAPPDATA") { roots.push((p.join("Programs"), 1)); }
    // RetroArch's installer puts it in %APPDATA%.
    if let Some(p) = env("APPDATA") { roots.push((p, 1)); }
    if let Some(home) = env("USERPROFILE") {
        for d in ["Desktop", "Downloads", "Documents", "Games", "Emulators", "Emulateurs"] { roots.push((home.join(d), 2)); }
    }
    for f in near {
        roots.push((f.clone(), 1));
        for up in f.ancestors().skip(1).take(3) { roots.push((up.to_path_buf(), 1)); }
    }
    for d in crate::drives::list() {
        roots.push((PathBuf::from(format!("{}\\", d.letter)), 2));
    }
    let mut seen = HashSet::new();
    for (root, depth) in roots {
        if found.len() == EMULATORS.len() { break; }
        if !seen.insert(norm_dir(&root)) { continue; }
        crawl(&root, depth, &mut found);
    }
    found
}

/// Walks `root` a few levels down; a folder named after an emulator not yet found is looked into.
fn crawl(root: &Path, depth: usize, found: &mut HashMap<&'static str, PathBuf>) {
    let check = |dir: &Path, found: &mut HashMap<&'static str, PathBuf>| {
        let Some(name) = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { return };
        for e in &EMULATORS {
            if found.contains_key(e.id) || !e.hints.iter().any(|h| name.contains(h)) { continue; }
            if let Some(exe) = exe_in(dir, e, 1) { found.insert(e.id, exe); }
        }
    };
    check(root, found);
    let mut queue = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, d)) = queue.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten().take(500) {
            if !entry.file_type().is_ok_and(|t| t.is_dir()) { continue; }
            let path = entry.path();
            check(&path, found);
            if d + 1 < depth { queue.push((path, d + 1)); }
        }
    }
}

fn exe_in(dir: &Path, emu: &Emulator, depth: usize) -> Option<PathBuf> {
    let read = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in read.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if kind.is_file() && (emu.is_exe)(&name) { return Some(entry.path()); }
        if kind.is_dir() { subdirs.push(entry.path()); }
    }
    if depth == 0 { return None; }
    subdirs.iter().find_map(|d| exe_in(d, emu, depth - 1))
}

// ---------------------------------------------------------------- what each emulator lists

/// A folder of games an emulator (or the user) listed.
#[derive(Clone, Debug, PartialEq)]
pub struct RomDir {
    pub path: PathBuf,
    pub deep: bool,
    /// "Azahar", "Dolphin"… or "" for a folder the user added.
    pub from: &'static str,
    /// The consoles the emulator listing it plays: settles a `.chd` or an `.iso` header can't.
    pub consoles: &'static [Source],
}

/// The folders and recent files `exe`'s configuration lists.
pub fn listed(id: &str, exe: Option<&Path>, dirs: &mut Vec<RomDir>, files: &mut Vec<PathBuf>) {
    let beside = exe.and_then(Path::parent).map(Path::to_path_buf);
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let docs = env("USERPROFILE").map(|h| h.join("Documents"));
    let mut push = |path: PathBuf, deep: bool, from: &'static str, consoles: &'static [Source]| {
        if !path.as_os_str().is_empty() { dirs.push(RomDir { path, deep, from, consoles }); }
    };
    match id {
        "azahar" => azahar(exe, dirs, files),
        "desmume" => desmume(exe, dirs, files),
        "mgba" => {
            let mut inis: Vec<PathBuf> = beside.iter().map(|d| d.join("config.ini")).collect();
            inis.extend(env("APPDATA").map(|a| a.join("mGBA/config.ini")));
            for (key, value) in inis.iter().flat_map(|i| ini(i)) {
                let key = key.to_ascii_lowercase();
                if key == "lastdirectory" {
                    push(PathBuf::from(&value), false, "mGBA", &[Source::Gba]);
                } else if key.starts_with("recent") {
                    files.extend(value.split([',', ';']).map(str::trim).filter(|v| !v.is_empty()).map(PathBuf::from));
                }
            }
        }
        "dolphin" => {
            let mut inis: Vec<PathBuf> = beside.iter().map(|d| d.join("User/Config/Dolphin.ini")).collect();
            inis.extend(env("APPDATA").map(|a| a.join("Dolphin Emulator/Config/Dolphin.ini")));
            inis.extend(docs.iter().map(|d| d.join("Dolphin Emulator/Config/Dolphin.ini")));
            for i in &inis {
                let pairs = ini(i);
                let deep = pairs.iter().any(|(k, v)| k == "RecursiveISOPaths" && v.eq_ignore_ascii_case("true"));
                for (k, v) in &pairs {
                    if k.starts_with("ISOPath") && k != "ISOPaths" {
                        push(PathBuf::from(v), deep, "Dolphin", &[Source::Gc, Source::Wii]);
                    }
                }
            }
        }
        "duckstation" | "pcsx2" => {
            let (name, file, consoles): (&'static str, &str, &'static [Source]) = if id == "pcsx2" {
                ("PCSX2", "inis/PCSX2.ini", &[Source::Ps2])
            } else {
                ("DuckStation", "settings.ini", &[Source::Ps1])
            };
            let mut inis: Vec<PathBuf> = beside.iter().map(|d| d.join(file)).collect();
            inis.extend(docs.iter().map(|d| d.join(name).join(file)));
            inis.extend(env("LOCALAPPDATA").map(|a| a.join(name).join(file)));
            for (k, v) in inis.iter().flat_map(|i| ini(i)) {
                match k.as_str() {
                    "RecursivePaths" => push(PathBuf::from(&v), true, name, consoles),
                    "Paths" => push(PathBuf::from(&v), false, name, consoles),
                    _ => {}
                }
            }
        }
        "ryujinx" => {
            let mut jsons: Vec<PathBuf> = beside.iter().map(|d| d.join("portable/Config.json")).collect();
            jsons.extend(env("APPDATA").map(|a| a.join("Ryujinx/Config.json")));
            for j in jsons {
                let Some(v) = std::fs::read(&j).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else { continue };
                for d in v.get("game_dirs").and_then(|d| d.as_array()).into_iter().flatten().filter_map(|d| d.as_str()) {
                    push(PathBuf::from(d), true, "Ryujinx", &[Source::Switch]);
                }
            }
        }
        "rpcs3" => {
            // games.yml: `BLUS30443: D:/PS3/Demon's Souls/`, one game folder per line.
            let Some(dir) = &beside else { return };
            for yml in [dir.join("config/games.yml"), dir.join("games.yml")] {
                let Ok(text) = std::fs::read_to_string(&yml) else { continue };
                for line in text.lines() {
                    let Some((_, path)) = line.split_once(": ") else { continue };
                    let path = path.trim().trim_matches(['"', '\'']);
                    if !path.is_empty() { files.push(PathBuf::from(path)); }
                }
            }
            push(dir.join("dev_hdd0/game"), false, "RPCS3", &[Source::Ps3]);
        }
        _ => {}
    }
}

/// `key = value` pairs of an ini file, sections ignored, values unquoted and unescaped.
fn ini(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), unescape(v)))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

/// Azahar's game list (`Paths\gamedirs\N\path`) and recent files, from `qt-config.ini` in the
/// portable `user` folder or `%APPDATA%`.
fn azahar(exe: Option<&Path>, dirs: &mut Vec<RomDir>, files: &mut Vec<PathBuf>) {
    let mut inis = Vec::new();
    if let Some(dir) = exe.and_then(Path::parent) { inis.push(dir.join("user/config/qt-config.ini")); }
    if let Some(app) = std::env::var_os("APPDATA").map(PathBuf::from) {
        for name in ["Azahar", "Lime3DS", "Citra"] { inis.push(app.join(name).join("config/qt-config.ini")); }
    }
    for ini in inis {
        let Ok(text) = std::fs::read_to_string(&ini) else { continue };
        let mut found: HashMap<String, (String, bool)> = HashMap::new();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            if let Some(rest) = key.strip_prefix("Paths\\gamedirs\\") {
                match rest.split_once('\\') {
                    Some((n, "path")) => found.entry(n.to_string()).or_default().0 = unescape(value),
                    Some((n, "deep_scan")) => found.entry(n.to_string()).or_default().1 = value == "true",
                    _ => {}
                }
            } else if key == "Paths\\recentFiles" {
                files.extend(value.split(", ").map(unescape).filter(|f| !f.is_empty()).map(PathBuf::from));
            }
        }
        let mut found: Vec<_> = found.into_values().filter(|(p, _)| !p.is_empty() && p != "INSTALLED" && p != "SYSTEM").collect();
        found.sort();
        dirs.extend(found.into_iter().map(|(p, deep)| RomDir { path: PathBuf::from(p), deep, from: "Azahar", consoles: &[Source::N3ds] }));
    }
}

/// DeSmuME's ROM folder and recent ROMs, from `desmume.ini` beside the executable or in
/// `%LOCALAPPDATA%`.
fn desmume(exe: Option<&Path>, dirs: &mut Vec<RomDir>, files: &mut Vec<PathBuf>) {
    let mut inis = Vec::new();
    if let Some(dir) = exe.and_then(Path::parent) { inis.push(dir.join("desmume.ini")); }
    for k in ["LOCALAPPDATA", "APPDATA"] {
        if let Some(p) = std::env::var_os(k).map(PathBuf::from) { inis.push(p.join("DeSmuME/desmume.ini")); }
    }
    for ini in inis {
        let Ok(text) = std::fs::read_to_string(&ini) else { continue };
        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name;
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            if value.is_empty() { continue; }
            if key.starts_with("Recent Rom") {
                files.push(PathBuf::from(value));
            } else if section == "PathSettings" && key == "Roms" {
                dirs.push(RomDir { path: PathBuf::from(value), deep: false, from: "DeSmuME", consoles: &[Source::Ds] });
            }
        }
    }
}

/// Qt ini values: `C:\\Users\\…` with doubled backslashes, sometimes in quotes.
pub fn unescape(v: &str) -> String {
    v.trim().trim_matches('"').replace("\\\\", "\\")
}

// ---------------------------------------------------------------- RetroArch

/// RetroArch's own folders, from `retroarch.cfg` (a leading `:` means beside the executable).
pub struct RetroArch {
    pub exe: PathBuf,
    pub cores: PathBuf,
    pub playlists: PathBuf,
}

impl RetroArch {
    pub fn at(exe: &Path) -> RetroArch {
        let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut ra = RetroArch { exe: exe.to_path_buf(), cores: dir.join("cores"), playlists: dir.join("playlists") };
        let resolve = |v: &str| {
            let v = v.trim().trim_matches('"');
            match v.strip_prefix(':') {
                Some(rest) => dir.join(rest.trim_start_matches(['\\', '/'])),
                None => PathBuf::from(v),
            }
        };
        for (k, v) in ini(&dir.join("retroarch.cfg")) {
            match k.as_str() {
                "libretro_directory" if v != "default" => ra.cores = resolve(&v),
                "playlist_directory" if v != "default" => ra.playlists = resolve(&v),
                _ => {}
            }
        }
        ra
    }

    /// The installed core that best plays `console`.
    pub fn core_for(&self, console: Source) -> Option<PathBuf> {
        console_cores(console).iter().map(|c| self.cores.join(format!("{c}.dll"))).find(|p| p.is_file())
    }
}

fn console_cores(console: Source) -> &'static [&'static str] {
    self::console(console).map_or(&[], |c| c.cores)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ini_values() {
        assert_eq!(unescape("D:\\\\Games\\\\ROMs\\\\3DS\\\\a.3ds"), "D:\\Games\\ROMs\\3DS\\a.3ds");
        assert_eq!(unescape("\"C:/a, b\""), "C:/a, b");
    }

    #[test]
    fn knows_emulator_binaries() {
        assert!(is_emulator(Path::new(r"D:\Emu\Dolphin-x64\Dolphin.exe")));
        assert!(is_emulator(Path::new(r"D:\Emu\pcsx2\pcsx2-qtx64-avx2.exe")));
        assert!(!is_emulator(Path::new(r"D:\Games\Hades\Hades.exe")));
    }
}
