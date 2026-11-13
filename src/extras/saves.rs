//! Save games: where they are, from Ludusavi's manifest (built from PCGamingWiki, downloaded
//! once a week) or, for a game it lacks, from the game's PCGamingWiki page; then copies of them
//! in `%APPDATA%\Arach\saves\<game>\<date>`, and those copies put back.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::screenshots::squash;
use crate::game::{Game, norm_dir};

const MANIFEST_URL: &str = "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
const MANIFEST_DAYS: u64 = 7;
/// A "save" bigger than this is more likely a whole install than a save.
const MAX_BYTES: u64 = 4 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Ludusavi,
    PcGamingWiki,
}

impl Base {
    pub fn label(self) -> &'static str {
        match self {
            Base::Ludusavi => "base Ludusavi",
            Base::PcGamingWiki => "PCGamingWiki",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Located {
    /// Files and folders that exist on this PC, outermost only.
    pub paths: Vec<PathBuf>,
    /// The same, files shown as their folder: what the page lists.
    pub places: Vec<PathBuf>,
    pub bytes: u64,
    pub from: Base,
}

/// One copy of a game's saves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    #[serde(skip)]
    pub dir: PathBuf,
    pub created: i64,
    pub bytes: u64,
    /// Made by Arach just before a restore, to undo it.
    #[serde(default)]
    pub automatic: bool,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub original: PathBuf,
    /// Relative to the backup's folder.
    pub stored: PathBuf,
}

// ---------------------------------------------------------------- finding

/// Where `g` keeps its saves on this PC. `Err` when no source could be read at all.
pub fn locate(g: &Game, steam: Option<&Path>, online: bool) -> Result<Option<Located>, String> {
    let ctx = Ctx::new(g, steam);
    let mut asked = false;
    if let Some(manifest) = manifest(online) {
        asked = true;
        if let Some(raw) = ludusavi_entry(&manifest, &g.name, g.steam_appid) {
            let paths = existing(raw.iter().filter_map(|p| ctx.expand_ludusavi(p)), &ctx);
            if !paths.is_empty() { return Ok(Some(located(paths, Base::Ludusavi))); }
        }
    }
    if online {
        match pcgw_paths(&g.name) {
            Ok(raw) => {
                asked = true;
                let paths = existing(raw.iter().filter_map(|p| ctx.expand_pcgw(p)), &ctx);
                if !paths.is_empty() { return Ok(Some(located(paths, Base::PcGamingWiki))); }
            }
            Err(e) if !asked => return Err(e),
            Err(_) => {}
        }
    }
    if asked { Ok(None) } else { Err("Les informations en ligne sont désactivées (Paramètres › En ligne).".into()) }
}

fn located(paths: Vec<PathBuf>, from: Base) -> Located {
    let bytes = paths.iter().map(|p| bytes_in(p)).sum();
    let mut places: Vec<PathBuf> = Vec::new();
    for p in &paths {
        let place = if p.is_file() { p.parent().map_or_else(|| p.clone(), Path::to_path_buf) } else { p.clone() };
        if !places.iter().any(|x| norm_dir(x) == norm_dir(&place)) { places.push(place); }
    }
    Located { paths, places, bytes, from }
}

/// The manifest as text: Ludusavi's own copy if it's installed, else ours, downloaded again
/// after a week (one download at a time).
fn manifest(online: bool) -> Option<String> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _held = LOCK.lock().ok()?;
    let ours = crate::store::cache_dir().join("ludusavi-manifest.yaml");
    let age = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|d| d.as_secs());
    let theirs = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("ludusavi/manifest.yaml"));
    let fresh = |p: &Path| age(p).is_some_and(|s| s < MANIFEST_DAYS * 86_400);
    if let Some(t) = theirs.as_ref().filter(|t| fresh(t)) {
        return std::fs::read_to_string(t).ok();
    }
    if online && !fresh(&ours) {
        let _ = download_manifest(&ours);
    }
    std::fs::read_to_string(&ours).ok().or_else(|| theirs.and_then(|t| std::fs::read_to_string(t).ok()))
}

fn download_manifest(to: &Path) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(90)).user_agent("Arach/0.2 (game launcher)").build();
    let resp = agent.get(MANIFEST_URL).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader().take(200 << 20), &mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() < 1 << 20 { return Err("manifeste tronqué".into()); }
    if let Some(dir) = to.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
    let tmp = to.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, to).map_err(|e| e.to_string())
}

use std::io::Read as _;

/// The Windows save paths of one game in the manifest, found by Steam id, else by name.
///
/// The manifest is YAML, but a regular one: a game per unindented key, its `files:` paths
/// four spaces in, each with optional `when:` conditions (`- os: windows`) six spaces in.
pub fn ludusavi_entry(manifest: &str, name: &str, appid: Option<u32>) -> Option<Vec<String>> {
    let want = squash(name);
    let id_line = appid.map(|id| format!("    id: {id}"));
    let mut by_name: Option<Vec<String>> = None;
    let mut lines = manifest.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with(' ') || line.starts_with('#') || !line.ends_with(':') { continue; }
        let key = unquote(line.trim_end_matches(':'));
        // Read the whole block: needed to see its Steam id anyway.
        let mut block: Vec<&str> = Vec::new();
        while let Some(next) = lines.peek() {
            if !next.is_empty() && !next.starts_with(' ') { break; }
            block.push(lines.next().unwrap_or_default());
        }
        let same_id = id_line.as_ref().is_some_and(|id| {
            block.windows(2).any(|w| w[0].trim_end() == "  steam:" && w[1].trim_end() == id)
        });
        if same_id { return Some(files_of(&block)); }
        if by_name.is_none() && squash(&key) == want { by_name = Some(files_of(&block)); }
    }
    by_name
}

fn files_of(block: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_files = false;
    // The path being read and whether its conditions allow Windows so far.
    let mut current: Option<(String, Option<bool>)> = None;
    let flush = |c: &mut Option<(String, Option<bool>)>, out: &mut Vec<String>| {
        if let Some((p, windows)) = c.take() && windows.unwrap_or(true) { out.push(p); }
    };
    for line in block {
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        if indent == 2 {
            flush(&mut current, &mut out);
            in_files = t == "files:";
            continue;
        }
        if !in_files { continue; }
        if indent == 4 {
            flush(&mut current, &mut out);
            let path = t.trim_end_matches(" {}").trim_end_matches(':');
            current = Some((unquote(path), None));
        } else if let Some((_, windows)) = current.as_mut() && let Some(os) = t.trim_start_matches("- ").strip_prefix("os: ") {
            // Several conditions: any one naming Windows lets the path in.
            *windows = Some(windows.unwrap_or(false) || os.trim() == "windows");
        }
    }
    flush(&mut current, &mut out);
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && (s.starts_with('"') && s.ends_with('"') || s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].replace("\\\"", "\"").replace("''", "'")
    } else {
        s.to_string()
    }
}

/// `{{Game data/saves|Windows|{{p|appdata}}\Game\Saves|…}}` lines of the game's wiki page.
fn pcgw_paths(name: &str) -> Result<Vec<String>, String> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(12)).user_agent("Arach/0.2 (game launcher)").build();
    let title: String = name.replace(['™', '®', '©'], "").trim().replace(' ', "_");
    let url = format!("https://www.pcgamingwiki.com/w/api.php?action=parse&format=json&prop=wikitext&redirects=1&page={}", encode(&title));
    let json: serde_json::Value = agent.get(&url).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
    let Some(text) = json.pointer("/parse/wikitext/*").and_then(|v| v.as_str()) else { return Ok(Vec::new()) };
    Ok(pcgw_from_wikitext(text))
}

fn pcgw_from_wikitext(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{Game data/saves|Windows|") {
        rest = &rest[i + "{{Game data/saves|Windows|".len()..];
        // Up to the template's closing braces, nested ones included.
        let mut depth = 1;
        let mut end = rest.len();
        let b = rest.as_bytes();
        let mut j = 0;
        while j + 1 < b.len() {
            if &b[j..j + 2] == b"{{" { depth += 1; j += 2; continue; }
            if &b[j..j + 2] == b"}}" {
                depth -= 1;
                if depth == 0 { end = j; break; }
                j += 2;
                continue;
            }
            j += 1;
        }
        let body = strip_refs(&rest[..end]);
        out.extend(split_top(&body).into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        rest = &rest[end.min(rest.len())..];
    }
    out
}

fn strip_refs(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("<ref") {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        rest = match (after.find("/>"), after.find("</ref>")) {
            (Some(a), Some(b)) if a < b && !after[..a].contains('>') => &after[a + 2..],
            (_, Some(b)) => &after[b + 6..],
            (Some(a), None) => &after[a + 2..],
            (None, None) => "",
        };
    }
    out.push_str(rest);
    out
}

/// Splits on `|` outside `{{ }}`.
fn split_top(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' && chars.get(i + 1) == Some(&'{') { depth += 1; cur.push_str("{{"); i += 2; continue; }
        if c == '}' && chars.get(i + 1) == Some(&'}') { depth -= 1; cur.push_str("}}"); i += 2; continue; }
        if c == '|' && depth == 0 { out.push(std::mem::take(&mut cur)); } else { cur.push(c); }
        i += 1;
    }
    out.push(cur);
    out
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

/// What placeholders stand for on this PC.
struct Ctx {
    game_dir: Option<PathBuf>,
    steam: Option<PathBuf>,
    appid: Option<u32>,
    steam_game: bool,
    home: PathBuf,
    documents: PathBuf,
}

impl Ctx {
    fn new(g: &Game, steam: Option<&Path>) -> Ctx {
        let home = env("USERPROFILE").unwrap_or_else(|| PathBuf::from(r"C:\Users\Default"));
        let documents = documents_dir().unwrap_or_else(|| home.join("Documents"));
        Ctx { game_dir: g.dir.clone().filter(|d| d.is_dir()), steam: steam.map(Path::to_path_buf), appid: g.steam_appid, steam_game: g.id.starts_with("steam:"), home, documents }
    }

    fn expand_ludusavi(&self, raw: &str) -> Option<String> {
        let s = |p: &Path| p.to_string_lossy().replace('\\', "/");
        let mut out = raw.to_string();
        if out.contains("<base>") || out.contains("<root>") || out.contains("<game>") {
            // <base> is the install folder; <root> the store's own folder (Steam's, holding
            // userdata), so it only means something for a Steam game.
            let dir = self.game_dir.as_ref()?;
            out = out.replace("<base>", &s(dir)).replace("<game>", &dir.file_name()?.to_string_lossy());
            if out.contains("<root>") {
                let root = self.steam.as_ref().filter(|_| self.steam_game)?;
                out = out.replace("<root>", &s(root));
            }
        }
        let local_low = self.home.join("AppData/LocalLow");
        let pairs: [(&str, Option<String>); 10] = [
            ("<home>", Some(s(&self.home))),
            ("<winAppData>", env("APPDATA").map(|p| s(&p))),
            ("<winLocalAppDataLow>", Some(s(&local_low))),
            ("<winLocalAppData>", env("LOCALAPPDATA").map(|p| s(&p))),
            ("<winDocuments>", Some(s(&self.documents))),
            ("<winPublic>", env("PUBLIC").map(|p| s(&p))),
            ("<winProgramData>", env("PROGRAMDATA").map(|p| s(&p))),
            ("<winDir>", env("WINDIR").map(|p| s(&p))),
            ("<osUserName>", std::env::var("USERNAME").ok()),
            ("<storeGameId>", Some(self.appid.map_or("*".into(), |a| a.to_string()))),
        ];
        for (k, v) in pairs {
            if out.contains(k) { out = out.replace(k, &v?); }
        }
        out = out.replace("<storeUserId>", "*");
        // Linux and macOS places, or anything left unknown.
        (!out.contains('<') && !out.starts_with('/') && !out.starts_with('~')).then_some(out)
    }

    fn expand_pcgw(&self, raw: &str) -> Option<String> {
        let mut out = raw.replace('\\', "/");
        while let Some(i) = out.find("{{p|") {
            let end = out[i..].find("}}")? + i;
            let what = out[i + 4..end].to_ascii_lowercase();
            let to = match what.as_str() {
                "game" => self.game_dir.clone()?,
                "steam" => self.steam.clone()?,
                "uid" => PathBuf::from("*"),
                "appdata" => env("APPDATA")?,
                "localappdata" => env("LOCALAPPDATA")?,
                "userprofile" => self.home.clone(),
                "userprofile/documents" => self.documents.clone(),
                "userprofile/appdata/locallow" => self.home.join("AppData/LocalLow"),
                "programdata" => env("PROGRAMDATA")?,
                "public" => env("PUBLIC")?,
                "username" => PathBuf::from(std::env::var("USERNAME").ok()?),
                "windir" => env("WINDIR")?,
                _ => return None,
            };
            out.replace_range(i..end + 2, &to.to_string_lossy().replace('\\', "/"));
        }
        // `{{code|…}}` and other templates aren't paths.
        (!out.contains("{{") && out.len() > 3 && out.as_bytes().get(1) == Some(&b':')).then_some(out)
    }
}

fn env(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

fn documents_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders").ok()?;
        k.get_value::<String, _>("Personal").ok().map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The paths that exist, globs expanded; the game's whole folder and anything inside another
/// match are dropped.
fn existing(raw: impl Iterator<Item = String>, ctx: &Ctx) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = raw.flat_map(|p| glob(&p)).collect();
    found.retain(|p| ctx.game_dir.as_ref().is_none_or(|d| norm_dir(d) != norm_dir(p)));
    found.sort_by_key(|p| norm_dir(p).len());
    let mut out: Vec<PathBuf> = Vec::new();
    for p in found {
        let n = norm_dir(&p);
        if out.iter().any(|o| { let on = norm_dir(o); n == on || n.starts_with(&format!("{on}/")) }) { continue; }
        out.push(p);
    }
    out
}

/// `C:/Users/*/Saved Games/**/*.sav`: `*` and `?` within a name, `**` across folders.
pub fn glob(pattern: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = pattern.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let Some((first, rest)) = parts.split_first() else { return Vec::new() };
    let root = if first.ends_with(':') { PathBuf::from(format!("{first}\\")) } else { PathBuf::from(first) };
    let mut out = Vec::new();
    walk(root, rest, &mut out, 0);
    out
}

fn walk(at: PathBuf, rest: &[&str], out: &mut Vec<PathBuf>, depth: usize) {
    if out.len() > 2000 || depth > 24 { return; }
    let Some((part, tail)) = rest.split_first() else {
        if at.exists() { out.push(at); }
        return;
    };
    if *part == "**" {
        walk(at.clone(), tail, out, depth + 1);
        if let Ok(read) = std::fs::read_dir(&at) {
            for e in read.flatten().filter(|e| e.path().is_dir()) {
                walk(e.path(), rest, out, depth + 1);
            }
        }
    } else if part.contains(['*', '?']) {
        let Ok(read) = std::fs::read_dir(&at) else { return };
        for e in read.flatten() {
            if wildcard(part, &e.file_name().to_string_lossy()) { walk(e.path(), tail, out, depth + 1); }
        }
    } else {
        walk(at.join(part), tail, out, depth + 1);
    }
}

/// Case-insensitive `*` / `?` match of one name.
fn wildcard(pat: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pat.to_lowercase().chars().collect(), name.to_lowercase().chars().collect());
    let (mut pi, mut ni, mut star, mut mark) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) { pi += 1; ni += 1; }
        else if pi < p.len() && p[pi] == '*' { star = Some(pi); mark = ni; pi += 1; }
        else if let Some(s) = star { pi = s + 1; mark += 1; ni = mark; }
        else { return false; }
    }
    while pi < p.len() && p[pi] == '*' { pi += 1; }
    pi == p.len()
}

pub fn bytes_in(p: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(p) else { return 0 };
    if meta.is_file() { return meta.len(); }
    if !meta.is_dir() { return 0; }
    std::fs::read_dir(p).map(|r| r.flatten().map(|e| bytes_in(&e.path())).sum()).unwrap_or(0)
}

// ---------------------------------------------------------------- copies

/// `%APPDATA%\Arach\saves\<game>`: a folder per game, named so any id fits.
pub fn backups_dir(game_id: &str) -> PathBuf {
    let safe: String = game_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    crate::store::data_dir().join("saves").join(safe)
}

const INDEX: &str = "arach-backup.json";

/// The game's copies, newest first.
pub fn list(game_id: &str) -> Vec<Backup> {
    let Ok(read) = std::fs::read_dir(backups_dir(game_id)) else { return Vec::new() };
    let mut out: Vec<Backup> = read.flatten().filter_map(|e| {
        let dir = e.path();
        let mut b: Backup = serde_json::from_slice(&std::fs::read(dir.join(INDEX)).ok()?).ok()?;
        b.dir = dir;
        Some(b)
    }).collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.created));
    out
}

/// Copies every path into a new dated folder.
pub fn backup(game_id: &str, paths: &[PathBuf], automatic: bool) -> Result<Backup, String> {
    let bytes: u64 = paths.iter().map(|p| bytes_in(p)).sum();
    if bytes > MAX_BYTES {
        return Err(format!("{} de sauvegardes : trop pour une copie, vérifiez les emplacements", format!("{:.1} Go", bytes as f64 / 1e9)));
    }
    let created = crate::store::now();
    let root = backups_dir(game_id);
    let mut dir = root.join(stamp(created));
    let mut n = 1;
    while dir.exists() { n += 1; dir = root.join(format!("{}-{n}", stamp(created))); }
    let mut items = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let name = p.file_name().map(|f| f.to_os_string()).unwrap_or_else(|| "save".into());
        let stored = PathBuf::from(i.to_string()).join(name);
        copy(p, &dir.join(&stored)).map_err(|e| format!("Copie de {} impossible : {e}", p.display()))?;
        items.push(Item { original: p.clone(), stored });
    }
    let b = Backup { dir: dir.clone(), created, bytes, automatic, items };
    std::fs::write(dir.join(INDEX), serde_json::to_vec_pretty(&b).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(b)
}

/// Puts a copy back over the current saves, after copying those (so the restore can be undone).
pub fn restore(game_id: &str, b: &Backup) -> Result<(), String> {
    let current: Vec<PathBuf> = b.items.iter().map(|i| i.original.clone()).filter(|p| p.exists()).collect();
    if !current.is_empty() { backup(game_id, &current, true)?; }
    for item in &b.items {
        let from = b.dir.join(&item.stored);
        if !from.exists() { return Err(format!("Copie incomplète : {} manque", item.stored.display())); }
        // A folder is replaced whole, so files the newer save added don't linger.
        if item.original.is_dir() { std::fs::remove_dir_all(&item.original).map_err(|e| e.to_string())?; }
        copy(&from, &item.original).map_err(|e| format!("{} : {e}", item.original.display()))?;
    }
    Ok(())
}

pub fn delete(b: &Backup) -> Result<(), String> {
    // Only ever a folder Arach made under its own saves folder.
    if !b.dir.starts_with(crate::store::data_dir().join("saves")) || !b.dir.join(INDEX).is_file() {
        return Err("Ce dossier n'est pas une copie d'Arach".into());
    }
    std::fs::remove_dir_all(&b.dir).map_err(|e| e.to_string())
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)?.flatten() {
            copy(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(dir) = to.parent() { std::fs::create_dir_all(dir)?; }
        std::fs::copy(from, to).map(|_| ())
    }
}

/// `20261113-214002`, local time.
fn stamp(ts: i64) -> String {
    let t = ts + crate::store::utc_offset();
    let (y, m, d) = civil(t.div_euclid(86_400));
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", t.rem_euclid(86_400) / 3600, t.rem_euclid(3600) / 60, t.rem_euclid(60))
}

/// Year, month, day of a day count since 1970 (Howard Hinnant's civil_from_days).
pub fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
"Celeste":
  files:
    <winAppData>/Celeste/Saves:
      tags:
        - save
      when:
        - os: windows
    <home>/.local/share/Celeste/Saves:
      when:
        - os: linux
    "<base>/Saves/*.celeste": {}
  steam:
    id: 504230
Other Game:
  files:
    <winDocuments>/Other:
      when:
        - os: windows
          store: steam
"#;

    #[test]
    fn reads_the_manifest_by_id_then_name() {
        assert_eq!(ludusavi_entry(MANIFEST, "whatever", Some(504230)).unwrap(),
            vec!["<winAppData>/Celeste/Saves".to_string(), "<base>/Saves/*.celeste".to_string()]);
        assert_eq!(ludusavi_entry(MANIFEST, "Other Game™", None).unwrap(), vec!["<winDocuments>/Other".to_string()]);
        assert!(ludusavi_entry(MANIFEST, "Nope", Some(1)).is_none());
    }

    #[test]
    fn reads_wiki_save_templates() {
        let text = "{{Game data/saves|Windows|{{p|appdata}}\\Celeste\\Saves<ref>{{Refcheck|user=X}}</ref>|{{p|game}}\\Saves}}\n{{Game data/saves|Linux|~/x}}";
        assert_eq!(pcgw_from_wikitext(text), vec!["{{p|appdata}}\\Celeste\\Saves".to_string(), "{{p|game}}\\Saves".to_string()]);
    }

    #[test]
    fn wildcards_match_names() {
        assert!(wildcard("*.SAV", "slot1.sav"));
        assert!(wildcard("slot?", "slot3"));
        assert!(!wildcard("*.sav", "slot1.bak"));
    }

    #[test]
    fn backs_up_and_restores() {
        let tmp = std::env::temp_dir().join(format!("arach-saves-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        unsafe { std::env::set_var("ARACH_DATA_DIR", tmp.join("data")); }
        let save = tmp.join("game/Saves");
        std::fs::create_dir_all(&save).unwrap();
        std::fs::write(save.join("slot1.sav"), b"old").unwrap();
        let b = backup("steam:1", std::slice::from_ref(&save), false).unwrap();
        std::fs::write(save.join("slot1.sav"), b"new").unwrap();
        std::fs::write(save.join("slot2.sav"), b"extra").unwrap();
        restore("steam:1", &list("steam:1").into_iter().find(|x| x.dir == b.dir).unwrap()).unwrap();
        assert_eq!(std::fs::read(save.join("slot1.sav")).unwrap(), b"old");
        assert!(!save.join("slot2.sav").exists());
        assert!(list("steam:1").iter().any(|x| x.automatic), "the restore keeps what it replaced");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    #[ignore]
    fn saves_on_this_pc() {
        let steam = crate::extras::steam_root();
        for g in crate::store::load_library().iter().filter(|g| !g.not_installed).take(40) {
            println!("{}: {:?}", g.name, locate(g, steam.as_deref(), true));
        }
    }
}
