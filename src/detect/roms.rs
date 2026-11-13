//! Console games: ROMs and disc images in the folders the emulators already know (their game
//! lists and recent files), those the user adds, and RetroArch's playlists. Each one starts
//! with the console's own emulator when it's installed, else with a RetroArch core. A game's
//! name and code come from the file itself (the DS banner, the 3DS SMDH, the disc header,
//! PS3's PARAM.SFO) when the format has them, so listing them stays offline.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::ScanOptions;
use super::discs;
use super::emulators::{self, RetroArch, RomDir};
use crate::game::{Art, Game, Launch, Rom, Source, created, norm_dir};

/// Subfolders followed under a folder the user added (or one an emulator deep-scans).
const MAX_DEPTH: usize = 4;
const MAX_ENTRIES: usize = 40_000;

/// Where the emulators and their games are, as found on this PC.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Setup {
    /// Each emulator found, by id (`emulators::EMULATORS`).
    pub emulators: HashMap<&'static str, PathBuf>,
    /// Folders to look through, with who listed them.
    pub dirs: Vec<RomDir>,
    /// Games the emulators opened lately or list one by one, wherever they are.
    pub files: Vec<PathBuf>,
}

impl Setup {
    pub fn retroarch(&self) -> Option<RetroArch> {
        self.emulators.get("retroarch").map(|exe| RetroArch::at(exe))
    }
}

/// Finds the emulators (the user's choice first) and every folder they list.
pub fn setup(opts: &ScanOptions) -> Setup {
    let mut s = Setup::default();
    for dir in &opts.roms {
        s.dirs.push(RomDir { path: dir.clone(), deep: true, from: "", consoles: &[] });
    }
    let near: Vec<PathBuf> = opts.roms.iter().chain(&opts.folders).cloned().collect();
    s.emulators = emulators::find_all(&opts.emulators, &near);
    for e in &emulators::EMULATORS {
        emulators::listed(e.id, s.emulators.get(e.id).map(PathBuf::as_path), &mut s.dirs, &mut s.files);
    }
    let mut seen = HashSet::new();
    s.dirs.retain(|d| d.path.is_dir() && seen.insert(norm_dir(&d.path)));
    let mut seen = HashSet::new();
    s.files.retain(|f| f.exists() && seen.insert(norm_dir(f)));
    s
}

/// Every console game in the folders `setup` found and in RetroArch's playlists.
pub fn detect(opts: &ScanOptions, setup: &Setup) -> Vec<Game> {
    let on: Vec<Source> = opts.stores.iter().copied().filter(|s| s.is_console()).collect();
    if on.is_empty() { return Vec::new(); }
    // Each candidate with the consoles whoever listed it plays.
    let mut paths: Vec<(PathBuf, &'static [Source])> = Vec::new();
    let mut seen = HashSet::new();
    for f in &setup.files {
        if seen.insert(norm_dir(f)) { paths.push((f.clone(), &[])); }
    }
    for d in &setup.dirs {
        for f in walk(&d.path, if d.deep { MAX_DEPTH } else { 0 }) {
            if seen.insert(norm_dir(&f)) { paths.push((f, d.consoles)); }
        }
    }
    // A `.cue`'s tracks and an `.m3u`'s discs are one game with it.
    let mut parts = HashSet::new();
    for (p, _) in &paths {
        if matches!(ext(p).as_str(), "cue" | "m3u") {
            parts.extend(discs::referenced(p).iter().map(|r| norm_dir(r)));
        }
    }
    let ra = setup.retroarch();
    let mut games: Vec<Game> = paths
        .iter()
        .filter(|(p, _)| !parts.contains(&norm_dir(p)) && !later_disc(p))
        .filter_map(|(path, hint)| {
            let found = classify(path, hint, opts.french)?;
            if !on.contains(&found.console) { return None; }
            Some(game(path, found, setup, ra.as_ref()))
        })
        .collect();
    if let Some(ra) = &ra {
        let mut known: HashSet<String> = paths.iter().map(|(p, _)| norm_dir(p)).collect();
        for g in playlists(ra, &on) {
            if g.dir.as_deref().is_some_and(|d| known.insert(norm_dir(d))) { games.push(g); }
        }
    }
    disambiguate(&mut games);
    games
}

fn ext(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

/// The consoles a file with this extension may be for; empty when it isn't a game.
fn consoles_for(ext: &str) -> &'static [Source] {
    match ext {
        "nds" | "dsi" => &[Source::Ds],
        "3ds" | "cci" | "cxi" | "cia" | "3dsx" => &[Source::N3ds],
        "gba" => &[Source::Gba],
        "gcm" | "rvz" | "wia" | "wbfs" | "ciso" | "gcz" => &[Source::Gc, Source::Wii],
        "iso" => &[Source::Gc, Source::Wii, Source::Ps2, Source::Ps1],
        "chd" => &[Source::Ps2, Source::Ps1],
        "cue" | "m3u" | "pbp" | "bin" => &[Source::Ps1, Source::Ps2],
        "cso" | "zso" => &[Source::Ps2],
        "nsp" | "xci" | "nsz" | "xcz" => &[Source::Switch],
        _ => &[],
    }
}

/// `Game (Disc 2).cue` beside `Game (Disc 1).cue`: the first disc stands for the game.
fn later_disc(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { return false };
    let lower = name.to_ascii_lowercase();
    let Some(i) = lower.find("(disc ") else { return false };
    let at = i + 6;
    match lower.as_bytes().get(at) {
        Some(d) if d.is_ascii_digit() && *d != b'1' => {
            let mut first = name.into_bytes();
            first[at] = b'1';
            path.with_file_name(String::from_utf8_lossy(&first).as_ref()).is_file()
        }
        _ => false,
    }
}

fn walk(root: &Path, depth: usize) -> Vec<PathBuf> {
    if discs::is_ps3_dir(root) { return vec![root.to_path_buf()]; }
    let mut out = Vec::new();
    let mut queue = vec![(root.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((dir, d)) = queue.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES { return out; }
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || name.starts_with('$') { continue; }
                if discs::is_ps3_dir(&path) {
                    out.push(path);
                } else if d < depth {
                    queue.push((path, d + 1));
                }
            } else if kind.is_file() && !consoles_for(&ext(&path)).is_empty() {
                out.push(path);
            }
        }
    }
    out
}

/// What a file turned out to be.
#[derive(Default)]
struct Found {
    console: Source,
    title: Option<String>,
    code: String,
    publisher: String,
    /// Width, height and RGBA pixels, drawn from the ROM.
    pixels: Option<(u32, u32, Vec<u8>)>,
    /// A picture on disk (PS3's ICON0.PNG).
    icon: Option<PathBuf>,
    hero: Option<PathBuf>,
    /// What the emulator opens, when it isn't the path itself (PS3's EBOOT.BIN).
    boot: Option<PathBuf>,
}

fn classify(path: &Path, hint: &[Source], french: bool) -> Option<Found> {
    if path.is_dir() {
        let g = discs::ps3_game(path)?;
        let code = discs::serial(&g.id).unwrap_or(g.id);
        return Some(Found { console: Source::Ps3, title: Some(g.title), code, icon: g.icon, hero: g.hero, boot: Some(g.eboot), ..Default::default() });
    }
    let options = consoles_for(&ext(path));
    match options {
        [] => None,
        [Source::Ds] | [Source::N3ds] => {
            let info = read_rom(path, options[0], french)?;
            Some(Found { console: info.console, title: info.title, code: info.code, publisher: info.publisher, pixels: info.icon, ..Default::default() })
        }
        [Source::Gba] => Some(Found { console: Source::Gba, code: discs::gba_code(path).unwrap_or_default(), ..Default::default() }),
        [Source::Switch] => {
            let name = path.file_name()?.to_string_lossy().into_owned();
            if !discs::switch_is_base(&name) { return None; }
            Some(Found { console: Source::Switch, code: discs::switch_title_id(&name).unwrap_or_default(), ..Default::default() })
        }
        _ => {
            if options.contains(&Source::Gc) && let Some(d) = discs::nintendo_disc(path) {
                let title = Some(d.title).filter(|t| !t.is_empty());
                return Some(Found { console: d.console, title, code: d.id, ..Default::default() });
            }
            if options.contains(&Source::Ps1) || options.contains(&Source::Ps2) {
                // A playlist's first disc, a cue sheet's first track, or the image itself.
                let mut image = path.to_path_buf();
                for _ in 0..2 {
                    if matches!(ext(&image).as_str(), "cue" | "m3u") {
                        image = discs::referenced(&image).into_iter().next()?;
                    }
                }
                if let Some((console, code)) = discs::playstation_disc(&image) {
                    return Some(Found { console, code, ..Default::default() });
                }
            }
            // Compressed (`.chd`, `.gcz`, `.cso`) or unreadable: who listed it, or where it is, says.
            let console = guess(path, options, hint)?;
            Some(Found { console, ..Default::default() })
        }
    }
}

/// Settles a file its header couldn't: the console of the emulator that lists it, else a
/// console named in its path (`D:\ROMs\PS2\…`). A `.bin` stays unknown: it may be anything.
fn guess(path: &Path, options: &[Source], hint: &[Source]) -> Option<Source> {
    if ext(path) == "bin" { return None; }
    let hinted: Vec<Source> = hint.iter().copied().filter(|h| options.contains(h)).collect();
    if hinted.len() == 1 { return Some(hinted[0]); }
    let lower = path.to_string_lossy().to_lowercase().replace(['_', '-'], " ");
    let said = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let named = if said(&["ps2", "playstation 2", "playstation2"]) {
        Source::Ps2
    } else if said(&["ps1", "psx", "psone", "playstation"]) {
        Source::Ps1
    } else if said(&["wii"]) {
        Source::Wii
    } else if said(&["gamecube", "ngc", "gcn"]) {
        Source::Gc
    } else if options.len() == 1 || ext(path) == "gcz" {
        options[0]
    } else {
        return None;
    };
    options.contains(&named).then_some(named)
}

fn prefix(console: Source) -> &'static str {
    match console {
        Source::Ds => "nds",
        Source::N3ds => "3ds",
        Source::Gba => "gba",
        Source::Gc => "gc",
        Source::Wii => "wii",
        Source::Ps1 => "ps1",
        Source::Ps2 => "ps2",
        Source::Ps3 => "ps3",
        Source::Switch => "nx",
        _ => "retro",
    }
}

fn game(path: &Path, found: Found, setup: &Setup, ra: Option<&RetroArch>) -> Game {
    let console = found.console;
    let key = format!("{:016x}", crate::app::fnv(&norm_dir(path)));
    let boot = found.boot.as_deref().unwrap_or(path);
    let icon = match &found.pixels {
        Some((w, h, rgba)) => save_icon(&format!("{}_{key}", prefix(console)), *w, *h, rgba),
        None => found.icon.clone(),
    };
    let size = if path.is_dir() { 0 } else { std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) };
    Game {
        id: format!("{}:{key}", prefix(console)),
        name: found.title.unwrap_or_else(|| file_title(path)),
        source: console,
        dir: Some(path.to_path_buf()),
        launch: launch(console, boot, setup, ra),
        icon,
        size,
        installed: created(path),
        art: Art { hero: found.hero, ..Default::default() },
        rom: Some(Rom { code: found.code, publisher: found.publisher, stem: stem(path), system: String::new() }),
        ..Default::default()
    }
}

/// The console's own emulator when there is one, else a RetroArch core, else whatever Windows
/// opens the file with.
pub fn launch(console: Source, rom: &Path, setup: &Setup, ra: Option<&RetroArch>) -> Launch {
    let quoted = format!("\"{}\"", rom.display());
    let ids = emulators::console(console).map_or(&[][..], |c| c.emulators);
    for id in ids {
        if let (Some(exe), Some(emu)) = (setup.emulators.get(id), emulators::emulator(id)) {
            return Launch::Exe {
                path: exe.clone(),
                args: format!("{} {quoted}", emu.args).trim().to_string(),
                cwd: exe.parent().map(Path::to_path_buf),
            };
        }
    }
    if let Some(ra) = ra && let Some(core) = ra.core_for(console) {
        return retroarch(ra, &core, &quoted);
    }
    Launch::Exe { path: rom.to_path_buf(), args: String::new(), cwd: rom.parent().map(Path::to_path_buf) }
}

fn retroarch(ra: &RetroArch, core: &Path, quoted: &str) -> Launch {
    Launch::Exe {
        path: ra.exe.clone(),
        args: format!("-L \"{}\" {quoted}", core.display()),
        cwd: ra.exe.parent().map(Path::to_path_buf),
    }
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---------------------------------------------------------------- RetroArch playlists

/// Every game in RetroArch's playlists (`Nintendo - Super Nintendo Entertainment System.lpl`),
/// started with the core the playlist names. Systems Arach knows go under their console;
/// the others under RetroArch.
fn playlists(ra: &RetroArch, on: &[Source]) -> Vec<Game> {
    let Ok(read) = std::fs::read_dir(&ra.playlists) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if ext(&path) != "lpl" { continue; }
        let system = stem(&path);
        // History and favourites mix systems, and repeat what the others list.
        if system.starts_with("content_") || system.starts_with("builtin") { continue; }
        let console = emulators::console_by_libretro(&system).unwrap_or(Source::Retro);
        if !on.contains(&console) { continue; }
        let Some(v) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else { continue };
        let default_core = v.get("default_core_path").and_then(|c| c.as_str()).unwrap_or("").to_string();
        for item in v.get("items").and_then(|i| i.as_array()).into_iter().flatten() {
            let s = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let rom = s("path");
            // `game.zip#game.sfc`: the archive is what's on disk.
            let file = PathBuf::from(rom.split('#').next().unwrap_or(""));
            if rom.is_empty() || !file.is_file() { continue; }
            let core = [s("core_path"), default_core.clone()]
                .into_iter()
                .find(|c| !c.is_empty() && c != "DETECT" && Path::new(c).is_file())
                .map(PathBuf::from)
                .or_else(|| ra.core_for(console));
            let quoted = format!("\"{rom}\"");
            let launch = match core {
                Some(core) => retroarch(ra, &core, &quoted),
                None => Launch::Exe { path: ra.exe.clone(), args: quoted, cwd: ra.exe.parent().map(Path::to_path_buf) },
            };
            let label = Some(s("label")).filter(|l| !l.is_empty()).unwrap_or_else(|| stem(&file));
            let key = format!("{:016x}", crate::app::fnv(&norm_dir(&file)));
            out.push(Game {
                id: format!("{}:{key}", prefix(console)),
                name: clean_title(&label),
                source: console,
                size: std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0),
                installed: created(&file),
                dir: Some(file),
                launch,
                rom: Some(Rom { code: String::new(), publisher: String::new(), stem: label, system: system.clone() }),
                ..Default::default()
            });
        }
    }
    out
}

/// Copies of one game (a ROM hack beside the original) would show the same title twice: each
/// keeps its file's name after the title.
fn disambiguate(games: &mut [Game]) {
    let mut count: HashMap<(Source, String), usize> = HashMap::new();
    for g in games.iter() {
        *count.entry((g.source, g.name.to_lowercase())).or_default() += 1;
    }
    for g in games.iter_mut() {
        if count[&(g.source, g.name.to_lowercase())] > 1 && let Some(path) = &g.dir {
            g.name = format!("{} · {}", g.name, file_title(path));
        }
    }
}

/// `3784 - Pokemon - Version Platine (FR)(EXiMiUS).nds` → `Pokemon - Version Platine`.
fn file_title(path: &Path) -> String {
    clean_title(&stem(path))
}

fn clean_title(stem: &str) -> String {
    let mut s = stem;
    // A release number in front.
    if let Some((num, rest)) = s.split_once(" - ") && !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
        s = rest;
    }
    // Tags at the end: (Europe) [b] (Rev 10)…
    let s = match s.find(['(', '[']) { Some(i) if i > 0 => &s[..i], _ => s };
    let s = s.replace('_', " ");
    let s = s.trim();
    if s.is_empty() { stem.to_string() } else { s.to_string() }
}

// ---------------------------------------------------------------- reading ROMs

#[derive(Debug, Default)]
pub struct RomInfo {
    pub console: Source,
    pub title: Option<String>,
    pub publisher: String,
    pub code: String,
    /// Width, height and RGBA pixels.
    pub icon: Option<(u32, u32, Vec<u8>)>,
}

/// What the ROM says about itself; `None` for files Azahar or DeSmuME can't start as a game
/// (an update or DLC, an encrypted dump, something that isn't a ROM).
pub fn read_rom(path: &Path, console: Source, french: bool) -> Option<RomInfo> {
    let mut f = File::open(path).ok()?;
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match (console, ext.as_str()) {
        (Source::Ds, _) => nds(&mut f, french),
        (_, "3ds" | "cci") => {
            let header = read_at(&mut f, 0, 0x200)?;
            if &header[0x100..0x104] != b"NCSD" { return None; }
            ncch(&mut f, u32le(&header, 0x120) as u64 * 0x200, french)
        }
        (_, "cxi") => ncch(&mut f, 0, french),
        (_, "cia") => cia(&mut f, french),
        (_, "3dsx") => homebrew(&mut f, french),
        _ => None,
    }
}

fn read_at(f: &mut File, at: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap_or_default())
}

fn u32be(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap_or_default())
}

/// A NUL-terminated UTF-16LE string.
fn utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
    String::from_utf16_lossy(&units).trim().to_string()
}

/// A four-letter game code, or empty for homebrew (`####`, NULs).
fn game_code(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b).to_string();
    if s.len() == 4 && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) { s } else { String::new() }
}

/// Languages to try for a title, best first. Both formats start Japanese, English, French.
fn langs(french: bool) -> [usize; 3] {
    if french { [2, 1, 0] } else { [1, 2, 0] }
}

/// DS header and banner: the code at 0x0C, the banner's offset at 0x68; in the banner a 32×32
/// 4-bit icon and one title per language (`Title\nSubtitle\nPublisher`).
fn nds(f: &mut File, french: bool) -> Option<RomInfo> {
    let h = read_at(f, 0, 0x200)?;
    // The Nintendo logo's checksum is fixed for every retail and homebrew DS ROM.
    if u16le(&h, 0x15C) != 0xCF56 { return None; }
    let mut info = RomInfo { console: Source::Ds, code: game_code(&h[0x0C..0x10]), ..Default::default() };
    let short = String::from_utf8_lossy(&h[..12]).trim_matches(char::from(0)).trim().to_string();
    let banner_at = u32le(&h, 0x68) as u64;
    if banner_at != 0 && let Some(b) = read_at(f, banner_at, 0x840) {
        let titles: Vec<String> = (0..6).map(|i| utf16(&b[0x240 + i * 0x100..0x340 + i * 0x100])).collect();
        let pick = langs(french).into_iter().map(|i| &titles[i]).find(|t| !t.is_empty()).or_else(|| titles.iter().find(|t| !t.is_empty()));
        if let Some(t) = pick {
            let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if lines.len() >= 2 {
                info.title = Some(lines[..lines.len() - 1].join(" "));
                info.publisher = lines[lines.len() - 1].to_string();
            } else {
                info.title = lines.first().map(|l| l.to_string());
            }
        }
        info.icon = Some((32, 32, ds_icon(&b[0x20..0x220], &b[0x220..0x240])));
    }
    if info.title.is_none() && !short.is_empty() { info.title = Some(short); }
    Some(info)
}

/// 4×4 tiles of 8×8 pixels, two pixels a byte, a 15-bit palette whose first entry is clear.
fn ds_icon(bitmap: &[u8], palette: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 32 * 32 * 4];
    for i in 0..1024 {
        let (tile, inner) = (i / 64, i % 64);
        let x = (tile % 4) * 8 + inner % 8;
        let y = (tile / 4) * 8 + inner / 8;
        let byte = bitmap[i / 2];
        let index = if i % 2 == 0 { byte & 0x0F } else { byte >> 4 } as usize;
        if index == 0 { continue; }
        let c = u16le(palette, index * 2);
        let o = (y * 32 + x) * 4;
        out[o] = expand5(c & 31);
        out[o + 1] = expand5((c >> 5) & 31);
        out[o + 2] = expand5((c >> 10) & 31);
        out[o + 3] = 255;
    }
    out
}

fn expand5(v: u16) -> u8 {
    ((v << 3) | (v >> 2)) as u8
}

/// An NCCH (the game inside a .3ds or .cia): its product code, and its SMDH from the ExeFS.
/// Azahar only plays decrypted games, so an encrypted one is skipped.
fn ncch(f: &mut File, at: u64, french: bool) -> Option<RomInfo> {
    let h = read_at(f, at, 0x200)?;
    if &h[0x100..0x104] != b"NCCH" { return None; }
    let flags = &h[0x188..0x190];
    // Content type: an executable, not a data archive (a manual, an update's data).
    if flags[5] & 0x2 == 0 { return None; }
    if flags[7] & 0x4 == 0 { return None; }
    let product = String::from_utf8_lossy(&h[0x150..0x160]).trim_matches(char::from(0)).to_string();
    let code = product.rsplit('-').next().map(|c| game_code(c.as_bytes())).unwrap_or_default();
    // The generic code homebrew ships with.
    let code = if code == "CTAP" { String::new() } else { code };
    let exefs = at + u32le(&h, 0x1A0) as u64 * 0x200;
    let mut info = RomInfo { console: Source::N3ds, code, ..Default::default() };
    if u32le(&h, 0x1A4) > 0 && let Some(fs) = read_at(f, exefs, 0x200) {
        for i in 0..10 {
            let e = &fs[i * 16..i * 16 + 16];
            if &e[..5] == b"icon\0" {
                let (off, size) = (u32le(e, 8) as u64, u32le(e, 12) as usize);
                if let Some(smdh) = read_at(f, exefs + 0x200 + off, size.min(0x36C0)) {
                    apply_smdh(&smdh, french, &mut info);
                }
            }
        }
    }
    Some(info)
}

/// A CIA: certificates, ticket, title metadata, then the contents. Only base games (title id
/// `00040000…`) count; updates and DLC can't be started on their own.
fn cia(f: &mut File, french: bool) -> Option<RomInfo> {
    let h = read_at(f, 0, 0x20)?;
    let align = |v: u64| v.div_ceil(64) * 64;
    let header = u32le(&h, 0) as u64;
    if header != 0x2020 { return None; }
    let (cert, ticket, tmd) = (u32le(&h, 8) as u64, u32le(&h, 0xC) as u64, u32le(&h, 0x10) as u64);
    let tmd_at = align(header) + align(cert) + align(ticket);
    let content_at = tmd_at + align(tmd);
    let sig = read_at(f, tmd_at, 4)?;
    let sig_len = match u32be(&sig, 0) {
        0x10000 | 0x10003 => 0x200 + 0x3C,
        0x10001 | 0x10004 => 0x100 + 0x3C,
        0x10002 | 0x10005 => 0x3C + 0x40,
        _ => return None,
    };
    let tmd_header = read_at(f, tmd_at + 4 + sig_len, 0xC4)?;
    if u32be(&tmd_header, 0x4C) != 0x0004_0000 { return None; }
    // The first content's chunk record says whether it's encrypted with the title key.
    let chunk = read_at(f, tmd_at + 4 + sig_len + 0xC4 + 0x900, 0x30)?;
    if u16::from_be_bytes([chunk[6], chunk[7]]) & 0x1 != 0 { return None; }
    ncch(f, content_at, french)
}

/// Homebrew: an optional SMDH after the header.
fn homebrew(f: &mut File, french: bool) -> Option<RomInfo> {
    let h = read_at(f, 0, 0x2C)?;
    if &h[..4] != b"3DSX" { return None; }
    let mut info = RomInfo { console: Source::N3ds, ..Default::default() };
    if u16le(&h, 4) >= 0x2C && let Some(smdh) = read_at(f, u32le(&h, 0x20) as u64, 0x36C0) {
        apply_smdh(&smdh, french, &mut info);
    }
    Some(info)
}

/// SMDH: sixteen (short title, long title, publisher) entries, then a 48×48 RGB565 icon laid
/// out in 8×8 tiles, each in Z-order.
fn apply_smdh(s: &[u8], french: bool, info: &mut RomInfo) {
    if s.len() < 0x36C0 || &s[..4] != b"SMDH" { return; }
    let entry = |lang: usize| {
        let at = 0x8 + lang * 0x200;
        (utf16(&s[at..at + 0x80]), utf16(&s[at + 0x180..at + 0x200]))
    };
    let pick = langs(french).into_iter().chain(3..16).map(entry).find(|(t, _)| !t.is_empty());
    if let Some((title, publisher)) = pick {
        info.title = Some(title.split_whitespace().collect::<Vec<_>>().join(" "));
        info.publisher = publisher;
    }
    let mut out = vec![0u8; 48 * 48 * 4];
    for y in 0..48usize {
        for x in 0..48usize {
            let tile = (y / 8) * 6 + x / 8;
            let (tx, ty) = (x % 8, y % 8);
            let z = (tx & 1) | ((ty & 1) << 1) | ((tx & 2) << 1) | ((ty & 2) << 2) | ((tx & 4) << 2) | ((ty & 4) << 3);
            let c = u16le(s, 0x24C0 + (tile * 64 + z) * 2);
            let o = (y * 48 + x) * 4;
            let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
            out[o] = expand5(r);
            out[o + 1] = ((g << 2) | (g >> 4)) as u8;
            out[o + 2] = expand5(b);
            out[o + 3] = 255;
        }
    }
    info.icon = Some((48, 48, out));
}

/// The icon, blown up four times without smoothing so its pixels stay crisp, as a PNG in the
/// cache. Written once.
fn save_icon(name: &str, w: u32, h: u32, rgba: &[u8]) -> Option<PathBuf> {
    let dir = crate::store::cache_dir().join("rom-icons");
    let path = dir.join(format!("{name}.png"));
    if path.is_file() { return Some(path); }
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec())?;
    let big = image::imageops::resize(&img, w * 4, h * 4, image::imageops::FilterType::Nearest);
    std::fs::create_dir_all(&dir).ok()?;
    big.save(&path).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_from_file_names() {
        assert_eq!(file_title(Path::new("3784 - Pokemon - Version Platine (FR)(EXiMiUS).nds")), "Pokemon - Version Platine");
        assert_eq!(file_title(Path::new("Pokemon Omega Ruby Random2.3ds")), "Pokemon Omega Ruby Random2");
        assert_eq!(file_title(Path::new("Mario_Kart_DS [b].nds")), "Mario Kart DS");
    }

    #[test]
    fn game_codes_skip_homebrew() {
        assert_eq!(game_code(b"CPUF"), "CPUF");
        assert_eq!(game_code(b"####"), "");
        assert_eq!(game_code(b"\0\0\0\0"), "");
    }

    #[test]
    fn guesses_compressed_images_from_their_folder() {
        let ps = [Source::Ps2, Source::Ps1];
        assert_eq!(guess(Path::new(r"D:\ROMs\PS2\Okami.chd"), &ps, &[]), Some(Source::Ps2));
        assert_eq!(guess(Path::new(r"D:\ROMs\PSX\Vagrant Story.chd"), &ps, &[]), Some(Source::Ps1));
        assert_eq!(guess(Path::new(r"D:\Stuff\Okami.chd"), &ps, &[Source::Ps2]), Some(Source::Ps2));
        assert_eq!(guess(Path::new(r"D:\Stuff\Okami.chd"), &ps, &[]), None);
        assert_eq!(guess(Path::new(r"D:\Stuff\track.bin"), &[Source::Ps1], &[Source::Ps1]), None);
    }

    #[test]
    fn later_discs_need_the_first() {
        assert!(!later_disc(Path::new(r"Z:\nowhere\Game (Disc 2).cue")));
        assert!(!later_disc(Path::new(r"Z:\nowhere\Game (Disc 1).cue")));
    }

    /// `cargo test scan_roms -- --ignored --nocapture`: the emulators and ROMs on this PC.
    #[test]
    #[ignore]
    fn scan_roms() {
        let opts = ScanOptions {
            stores: Source::CONSOLES.to_vec(),
            folders: Vec::new(),
            roms: Vec::new(),
            emulators: Default::default(),
            french: true,
            owned: false,
        };
        let t = std::time::Instant::now();
        let setup = setup(&opts);
        println!("{setup:#?} in {:?}", t.elapsed());
        for g in detect(&opts, &setup) {
            println!("{:<13} {:<50} {:?} {:?}", g.source.label(), g.name, g.rom, g.launch);
        }
    }
}
