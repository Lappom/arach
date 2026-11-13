//! What the other consoles' games say about themselves: the GameCube and Wii disc header (plain,
//! RVZ/WIA, WBFS, CISO), the PlayStation and PlayStation 2 boot file inside the ISO 9660 image
//! (`SYSTEM.CNF`, which names the disc's serial), a PlayStation 3 game's `PARAM.SFO`, and the
//! Game Boy Advance cartridge header. Only a few small reads per file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::game::Source;

fn read_at(f: &mut File, at: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap_or_default())
}

fn u32be(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap_or_default())
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// Printable ASCII up to the first NUL, trimmed; empty when it isn't readable text.
fn ascii(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let s = &b[..end];
    if s.iter().all(|&c| (0x20..0x7F).contains(&c)) { String::from_utf8_lossy(s).trim().to_string() } else { String::new() }
}

// ---------------------------------------------------------------- GameCube and Wii

/// A disc's six-character id (`GALE01`) and internal name.
pub struct Disc {
    pub console: Source,
    pub id: String,
    pub title: String,
}

/// The disc header, wherever the container keeps it.
pub fn nintendo_disc(path: &Path) -> Option<Disc> {
    let mut f = File::open(path).ok()?;
    let head = read_at(&mut f, 0, 0x60)?;
    let at = match &head[..4] {
        b"RVZ\x01" | b"WIA\x01" => 0x58,
        b"WBFS" => 1u64 << head[8].min(20),
        b"CISO" => 0x8000,
        _ => 0,
    };
    let h = read_at(&mut f, at, 0x80)?;
    let console = if u32be(&h, 0x18) == 0x5D1C_9EA3 {
        Source::Wii
    } else if u32be(&h, 0x1C) == 0xC233_9F3D {
        Source::Gc
    } else {
        return None;
    };
    let id = ascii(&h[..6]);
    if id.len() != 6 { return None; }
    Some(Disc { console, id, title: ascii(&h[0x20..0x80]) })
}

// ---------------------------------------------------------------- PlayStation 1 and 2

/// A CD or DVD image, cooked (2048-byte sectors) or raw (2352, as `.bin` files are).
struct Image {
    f: File,
    sector: u64,
    skip: u64,
}

impl Image {
    fn open(path: &Path) -> Option<Image> {
        let mut f = File::open(path).ok()?;
        let sync = read_at(&mut f, 16 * 2352, 16)?;
        let raw = sync[..12] == [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0];
        let image = match (raw, sync.get(15)) {
            (true, Some(2)) => Image { f, sector: 2352, skip: 24 },
            (true, _) => Image { f, sector: 2352, skip: 16 },
            _ => Image { f, sector: 2048, skip: 0 },
        };
        Some(image)
    }

    fn read(&mut self, lba: u64, len: usize) -> Option<Vec<u8>> {
        if self.sector == 2048 { return read_at(&mut self.f, lba * 2048, len); }
        let mut out = Vec::with_capacity(len);
        let mut lba = lba;
        while out.len() < len {
            let chunk = read_at(&mut self.f, lba * self.sector + self.skip, 2048)?;
            out.extend_from_slice(&chunk[..(len - out.len()).min(2048)]);
            lba += 1;
        }
        Some(out)
    }
}

/// The serial a PlayStation disc boots (`SLUS-20062`) and which PlayStation it's for, from
/// `SYSTEM.CNF` at the root of its ISO 9660 file system.
pub fn playstation_disc(path: &Path) -> Option<(Source, String)> {
    let mut img = Image::open(path)?;
    let pvd = img.read(16, 2048)?;
    if &pvd[1..6] != b"CD001" { return None; }
    let root = &pvd[156..190];
    let (lba, len) = (u32le(root, 2) as u64, (u32le(root, 10) as usize).min(64 * 1024));
    let dir = img.read(lba, len)?;
    let mut i = 0;
    while i < dir.len() {
        let rec = dir[i] as usize;
        if rec == 0 {
            // Records don't cross sectors: skip to the next one.
            i = (i / 2048 + 1) * 2048;
            continue;
        }
        if i + 33 > dir.len() || i + rec > dir.len() { break; }
        let name_len = dir[i + 32] as usize;
        let name = String::from_utf8_lossy(&dir[i + 33..(i + 33 + name_len).min(dir.len())]).to_ascii_uppercase();
        if name.starts_with("SYSTEM.CNF") {
            let file_lba = u32le(&dir, i + 2) as u64;
            let size = (u32le(&dir, i + 10) as usize).min(2048);
            let cnf = img.read(file_lba, size)?;
            return system_cnf(&String::from_utf8_lossy(&cnf));
        }
        i += rec;
    }
    None
}

/// `BOOT2 = cdrom0:\SLUS_200.62;1` → (PS2, `SLUS-20062`); `BOOT = cdrom:\SCUS_944.55;1` → PS1.
fn system_cnf(text: &str) -> Option<(Source, String)> {
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let console = match key.trim().to_ascii_uppercase().as_str() {
            "BOOT2" => Source::Ps2,
            "BOOT" => Source::Ps1,
            _ => continue,
        };
        let file = value.trim().rsplit(['\\', ':', '/']).next().unwrap_or("");
        let file = file.split(';').next().unwrap_or("").trim();
        return Some((console, serial(file)?));
    }
    None
}

/// `SLUS_200.62` → `SLUS-20062`.
pub fn serial(s: &str) -> Option<String> {
    let clean: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    let split = clean.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = clean.split_at(split);
    (letters.len() == 4 && digits.len() >= 3 && digits.chars().all(|c| c.is_ascii_digit())).then(|| format!("{letters}-{digits}"))
}

/// The files a `.cue` sheet or an `.m3u` list points at, beside it.
pub fn referenced(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let dir = path.parent().unwrap_or(Path::new(""));
    let cue = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue"));
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if cue {
                let rest = l.strip_prefix("FILE ")?;
                let name = if let Some(r) = rest.strip_prefix('"') { r.split('"').next()? } else { rest.split_whitespace().next()? };
                Some(dir.join(name))
            } else {
                (!l.is_empty() && !l.starts_with('#')).then(|| dir.join(l))
            }
        })
        .collect()
}

// ---------------------------------------------------------------- PlayStation 3

/// A PS3 game folder: a disc dump (`PS3_GAME\PARAM.SFO`) or an installed game
/// (`dev_hdd0\game\BLUS30443\PARAM.SFO`).
pub struct Ps3Game {
    pub title: String,
    pub id: String,
    /// What RPCS3 boots.
    pub eboot: PathBuf,
    pub icon: Option<PathBuf>,
    pub hero: Option<PathBuf>,
}

pub fn ps3_game(dir: &Path) -> Option<Ps3Game> {
    let base = if dir.join("PS3_GAME/PARAM.SFO").is_file() { dir.join("PS3_GAME") } else { dir.to_path_buf() };
    let sfo = sfo(&std::fs::read(base.join("PARAM.SFO")).ok()?);
    let get = |k: &str| sfo.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone()).unwrap_or_default();
    // Disc and installed games; patches and game data ("GD") can't start on their own.
    if !matches!(get("CATEGORY").as_str(), "DG" | "HG" | "") { return None; }
    let eboot = base.join("USRDIR/EBOOT.BIN");
    if !eboot.is_file() { return None; }
    let title = get("TITLE").split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() { return None; }
    let file = |n: &str| Some(base.join(n)).filter(|p| p.is_file());
    Some(Ps3Game { title, id: get("TITLE_ID"), eboot, icon: file("ICON0.PNG"), hero: file("PIC1.PNG") })
}

/// Whether `dir` is a PS3 game folder rather than a folder of them.
pub fn is_ps3_dir(dir: &Path) -> bool {
    dir.join("PS3_GAME/PARAM.SFO").is_file() || (dir.join("PARAM.SFO").is_file() && dir.join("USRDIR").is_dir())
}

/// PARAM.SFO: a key table and a data table; text values are UTF-8.
fn sfo(b: &[u8]) -> Vec<(String, String)> {
    if b.len() < 0x14 || &b[..4] != b"\0PSF" { return Vec::new(); }
    let (keys, data, n) = (u32le(b, 8) as usize, u32le(b, 12) as usize, u32le(b, 16) as usize);
    let mut out = Vec::new();
    for i in 0..n.min(128) {
        let e = 0x14 + i * 16;
        if e + 16 > b.len() { break; }
        let (key_off, fmt, len, data_off) = (u16le(b, e) as usize, u16le(b, e + 2), u32le(b, e + 4) as usize, u32le(b, e + 12) as usize);
        let k = keys + key_off;
        let key = b.get(k..).map(|s| String::from_utf8_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]).into_owned());
        let v = data + data_off;
        // 0x0204 is UTF-8 text; integers aren't wanted here.
        if fmt != 0x0204 && fmt != 0x0004 { continue; }
        let Some(bytes) = b.get(v..v + len) else { continue };
        let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
        if let Some(key) = key { out.push((key, String::from_utf8_lossy(&bytes[..end]).trim().to_string())); }
    }
    out
}

// ---------------------------------------------------------------- Game Boy Advance

/// The cartridge's four-letter code (`AXVE`), checked against the fixed header byte.
pub fn gba_code(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let h = read_at(&mut f, 0, 0xC0)?;
    if h[0xB2] != 0x96 { return None; }
    let code = ascii(&h[0xAC..0xB0]);
    (code.len() == 4 && code.chars().all(|c| c.is_ascii_alphanumeric())).then_some(code)
}

// ---------------------------------------------------------------- Switch

/// The title id in a dump's name: `Game [0100ABCD12345000][v0].nsp` → `0100ABCD12345000`.
pub fn switch_title_id(name: &str) -> Option<String> {
    name.split(['[', ']', '(', ')', ' ', '_'])
        .find(|t| t.len() == 16 && t.starts_with("01") && t.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|t| t.to_ascii_uppercase())
}

/// Updates end in `800`, add-ons in something else; base games in `000`.
pub fn switch_is_base(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.contains("[upd") || lower.contains("[dlc") || lower.contains("(update") || lower.contains("(dlc") { return false; }
    switch_title_id(name).is_none_or(|id| id.ends_with("000"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ps2_boot_lines() {
        assert_eq!(system_cnf("BOOT2 = cdrom0:\\SLUS_200.62;1\r\nVER = 1.00"), Some((Source::Ps2, "SLUS-20062".into())));
        assert_eq!(system_cnf("BOOT = cdrom:\\SCUS_944.55;1"), Some((Source::Ps1, "SCUS-94455".into())));
        assert_eq!(system_cnf("BOOT=cdrom:SLES_123.45;1"), Some((Source::Ps1, "SLES-12345".into())));
        assert_eq!(serial("BLUS30443"), Some("BLUS-30443".into()));
    }

    #[test]
    fn tells_switch_base_games_from_updates() {
        assert!(switch_is_base("Zelda [01007EF00011E000][v0].nsp"));
        assert!(!switch_is_base("Zelda [01007EF00011E800][v131072].nsp"));
        assert!(!switch_is_base("Zelda [01007EF00011F001].nsp"));
        assert!(switch_is_base("Celeste.xci"));
    }

    #[test]
    fn reads_gamecube_headers() {
        let dir = std::env::temp_dir().join(format!("arach-disc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("zelda.iso");
        let mut h = vec![0u8; 0x400];
        h[..6].copy_from_slice(b"GZLE01");
        h[0x1C..0x20].copy_from_slice(&0xC233_9F3Du32.to_be_bytes());
        h[0x20..0x20 + 14].copy_from_slice(b"The Wind Waker");
        std::fs::write(&path, &h).unwrap();
        let d = nintendo_disc(&path).unwrap();
        assert_eq!((d.console, d.id.as_str(), d.title.as_str()), (Source::Gc, "GZLE01", "The Wind Waker"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
