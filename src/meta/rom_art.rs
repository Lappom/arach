//! Pictures for console games, found by the game code the scan read from the ROM or disc
//! rather than by name: the box from GameTDB (`art.gametdb.com`) where it has one, and box art,
//! title screens and screenshots from libretro-thumbnails, which files them under No-Intro and
//! Redump names. libretro's databases tie each game code to those names, so the one for a
//! console is downloaded once a month and kept in the cache. RetroArch's games come with the
//! exact name already (its playlist label).

use std::collections::HashMap;
use std::path::PathBuf;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};

use super::{Job, Meta, art_dir, encode, image_bytes};
use crate::detect::emulators;
use crate::game::{Rom, Source};

/// Box art is drawn into Arach's 2:3 covers at this size.
const COVER: (u32, u32) = (600, 900);
/// Refresh the databases after this long.
const DAT_SECS: u64 = 30 * 24 * 3600;

pub fn lookup(agent: &ureq::Agent, job: &Job, console: Source, rom: &Rom, french: bool, meta: &mut Meta, dats: &mut Dats) {
    if !rom.publisher.is_empty() {
        meta.publishers = vec![rom.publisher.clone()];
    }
    let conf = emulators::console(console);
    let system = conf.map(|c| c.libretro.to_string()).unwrap_or_else(|| rom.system.clone());
    if system.is_empty() { return; }
    let code = rom.code.to_ascii_uppercase();
    let mut names = match conf.and_then(|c| c.dat) {
        Some(dat) if !code.is_empty() => dats.names(agent, dat, console, &code, french),
        _ => Vec::new(),
    };
    if !rom.stem.is_empty() && !names.contains(&rom.stem) { names.push(rom.stem.clone()); }
    let key = if code.is_empty() { format!("{:016x}", crate::app::fnv(&format!("{system}/{}", rom.stem))) } else { code.clone() };
    let tag = format!("{}_{}", crate::app::fnv(&system) % 100_000, key.replace('-', ""));
    let gametdb_first = matches!(console, Source::Ds | Source::N3ds | Source::Ps3);
    if job.local.cover.is_none() {
        let file = format!("{tag}_cover.jpg");
        meta.art.cover = cached(&file).or_else(|| {
            let tdb = || conf.and_then(|c| c.gametdb).and_then(|plat| gametdb(agent, plat, &code, french));
            let lr = || names.iter().find_map(|n| libretro(agent, &system, "Named_Boxarts", n));
            let img = if gametdb_first { tdb().or_else(lr) } else { lr().or_else(tdb) }?;
            save(&DynamicImage::ImageRgb8(boxed(&img)), &file, ImageFormat::Jpeg)
        });
    }
    if job.local.hero.is_none() {
        let file = format!("{tag}_hero.png");
        let dual = conf.is_some_and(|c| c.dual_screen);
        meta.art.hero = cached(&file).or_else(|| {
            let img = names
                .iter()
                .find_map(|n| libretro(agent, &system, "Named_Titles", n))
                .or_else(|| names.iter().find_map(|n| libretro(agent, &system, "Named_Snaps", n)))?;
            save(&screen(&img, dual), &file, ImageFormat::Png)
        });
    }
    if meta.art.cover.is_some() || meta.art.hero.is_some() {
        meta.rom = Some(key);
    }
}

fn cached(file: &str) -> Option<PathBuf> {
    let path = art_dir().join(file);
    path.is_file().then_some(path)
}

fn save(img: &DynamicImage, file: &str, format: ImageFormat) -> Option<PathBuf> {
    let path = art_dir().join(file);
    let tmp = path.with_extension("part");
    img.save_with_format(&tmp, format).ok()?;
    std::fs::rename(&tmp, &path).ok()?;
    Some(path)
}

fn fetch_image(agent: &ureq::Agent, url: &str) -> Option<DynamicImage> {
    image::load_from_memory(&image_bytes(agent, url)?).ok()
}

// ---------------------------------------------------------------- GameTDB

/// Where a code says the game was sold, as a Nintendo region letter: the last letter of a
/// four-letter code (`CPUF`), the fourth of a six-letter disc id (`GALE01`), the third of a
/// PlayStation serial (`BLUS30443`: U for the US).
fn region(code: &str) -> char {
    let b = code.as_bytes();
    match b.len() {
        4 | 6 => b[3] as char,
        n if n >= 7 && b[..4].iter().all(u8::is_ascii_alphabetic) => match b[2] {
            b'U' => 'E',
            b'E' => 'P',
            b'J' | b'P' | b'M' => 'J',
            b'K' | b'A' => 'K',
            _ => '?',
        },
        _ => '?',
    }
}

/// GameTDB files covers by language folder. The exact code first, then (for Nintendo codes)
/// the same game from another region.
fn gametdb(agent: &ureq::Agent, plat: &str, code: &str, french: bool) -> Option<DynamicImage> {
    let code: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let kinds: &[(&str, &str)] = match plat {
        "ds" => &[("coverHQ", "jpg"), ("coverM", "jpg"), ("coverS", "png")],
        "3ds" => &[("coverHQ", "jpg"), ("coverM", "jpg")],
        "wii" => &[("coverfullHQ", "png"), ("cover", "png")],
        _ => &[("coverM", "jpg"), ("cover", "jpg")],
    };
    let own = region(&code);
    let mut tries: Vec<(String, Vec<&str>)> = vec![(code.clone(), langs(own, french))];
    if matches!(code.len(), 4 | 6) {
        for other in ['P', 'E', 'J'] {
            if other == own { continue; }
            let mut alt = code.clone().into_bytes();
            alt[3] = other as u8;
            tries.push((String::from_utf8_lossy(&alt).into_owned(), langs(other, french).into_iter().take(2).collect()));
        }
    }
    for (code, langs) in tries {
        for lang in langs {
            for (kind, ext) in kinds {
                let url = format!("https://art.gametdb.com/{plat}/{kind}/{lang}/{code}.{ext}");
                if let Some(img) = fetch_image(agent, &url) { return Some(img); }
            }
        }
    }
    None
}

/// GameTDB's language folders for a region letter, likeliest first.
fn langs(region: char, french: bool) -> Vec<&'static str> {
    match region {
        'E' => vec!["US", "EN"],
        'P' | 'V' | 'X' | 'Y' | 'Z' | 'L' if french => vec!["FR", "EN", "DE", "ES", "IT", "NL"],
        'P' | 'V' | 'X' | 'Y' | 'Z' | 'L' => vec!["EN", "FR", "DE", "ES", "IT", "NL"],
        'F' => vec!["FR", "EN"],
        'D' => vec!["DE", "EN"],
        'S' => vec!["ES", "EN"],
        'I' => vec!["IT", "EN"],
        'H' => vec!["NL", "EN"],
        'U' => vec!["AU", "EN"],
        'J' => vec!["JA"],
        'K' => vec!["KO"],
        _ if french => vec!["FR", "EN", "US", "JA"],
        _ => vec!["EN", "US", "FR", "JA"],
    }
}

// ---------------------------------------------------------------- libretro

fn libretro(agent: &ureq::Agent, system: &str, kind: &str, name: &str) -> Option<DynamicImage> {
    // libretro swaps the characters file names can't hold for underscores.
    let file: String = name.chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect();
    let url = format!("https://thumbnails.libretro.com/{}/{kind}/{}.png", encode(system), encode(&file));
    fetch_image(agent, &url)
}

/// Names by game code, per console, loaded on first use.
#[derive(Default)]
pub struct Dats {
    lists: HashMap<Source, HashMap<String, Vec<String>>>,
}

impl Dats {
    /// The names for a game code, best region first, at most three.
    fn names(&mut self, agent: &ureq::Agent, dat: &str, console: Source, code: &str, french: bool) -> Vec<String> {
        let list = self.lists.entry(console).or_insert_with(|| load_dat(agent, dat));
        let norm: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let mut names = list.get(&norm).cloned().unwrap_or_default();
        // A disc id (`GALE01`) is filed under its first four letters.
        if names.is_empty() && norm.len() == 6 {
            names = list.get(&norm[..4]).cloned().unwrap_or_default();
        }
        let region = region(&norm);
        names.sort_by_key(|n| -score(n, region, french));
        names.truncate(3);
        names
    }
}

fn score(name: &str, region: char, french: bool) -> i32 {
    let has = |s: &str| name.contains(s);
    let mut s = 0;
    let wanted = match region {
        'E' => "(USA",
        'P' | 'V' | 'X' | 'Y' | 'Z' | 'L' => "(Europe",
        'F' => "(France",
        'D' => "(Germany",
        'S' => "(Spain",
        'I' => "(Italy",
        'J' => "(Japan",
        'K' => "(Korea",
        _ if french => "(Europe",
        _ => "(USA",
    };
    if has(wanted) { s += 4; }
    if french && (has("(France") || has("(Europe")) { s += 2; }
    if has("(USA") || has("(Europe") { s += 1; }
    for bad in ["(Demo", "(Kiosk", "(Beta", "(Proto", "(Sample", "(Pirate"] {
        if has(bad) { s -= 8; }
    }
    s
}

/// `dat` is the file's place in libretro-database's `metadat` (`redump/Sony - PlayStation 2`).
fn load_dat(agent: &ureq::Agent, dat: &str) -> HashMap<String, Vec<String>> {
    let file = dat.replace(['/', ' '], "_").to_lowercase();
    let path = crate::store::cache_dir().join(format!("dat-{file}.dat"));
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age.as_secs() < DAT_SECS);
    if !fresh {
        let url = format!(
            "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/{}.dat",
            dat.split('/').map(encode).collect::<Vec<_>>().join("/")
        );
        if let Ok(resp) = agent.get(&url).call()
            && let Ok(text) = resp.into_string()
            && text.contains("game (")
        {
            let tmp = path.with_extension("part");
            if std::fs::write(&tmp, &text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
    std::fs::read_to_string(&path).map(|t| parse_dat(&t)).unwrap_or_default()
}

/// `game ( name "…" … serial "CTR-P-ECRA" … )`: the game's name under its whole serial
/// (`CTRPECRA`, `SLUS20062`) and under each four-letter part (`ECRA`).
fn parse_dat(text: &str) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut name = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("game (") {
            name.clear();
        } else if let Some(v) = quoted(line, "name") {
            name = v.to_string();
        } else if let Some(v) = quoted(line, "serial") {
            if name.is_empty() { continue; }
            for serial in v.split([',', ' ']).filter(|s| !s.is_empty()) {
                let whole: String = serial.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
                let mut keys = vec![whole];
                keys.extend(serial.split('-').filter(|p| p.len() == 4).map(|p| p.to_ascii_uppercase()));
                for key in keys {
                    let names = out.entry(key).or_default();
                    if !names.contains(&name) { names.push(name.clone()); }
                }
            }
        }
    }
    out
}

fn quoted<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(key)?.strip_prefix(" \"")?;
    rest.split('"').next()
}

// ---------------------------------------------------------------- shaping

/// A handheld's box is about as wide as tall; Arach's covers are 2:3. The box sits in the
/// middle, over a blurred and darkened copy of itself that fills the rest.
fn boxed(img: &DynamicImage) -> image::RgbImage {
    let (w, h) = img.dimensions();
    if h as f32 / w.max(1) as f32 > 1.3 {
        return img.resize_to_fill(COVER.0, COVER.1, FilterType::Lanczos3).to_rgb8();
    }
    // A tiny fill blown back up is a blur.
    let mut bg = img
        .resize_to_fill(16, 24, FilterType::Triangle)
        .resize_exact(COVER.0, COVER.1, FilterType::Triangle)
        .to_rgb8();
    for p in bg.pixels_mut() {
        for c in p.0.iter_mut() { *c = (*c as f32 * 0.45) as u8; }
    }
    let fg = img.resize(COVER.0 - 36, COVER.1, FilterType::Lanczos3).to_rgb8();
    let x = (COVER.0 - fg.width()) / 2;
    let y = (COVER.1 - fg.height()) / 2;
    image::imageops::overlay(&mut bg, &fg, x as i64, y as i64);
    bg
}

/// A screenshot enlarged in whole steps so the pixels stay sharp. libretro's DS and 3DS shots
/// hold both screens, one above the other: with `dual` the hero keeps the top one.
fn screen(img: &DynamicImage, dual: bool) -> DynamicImage {
    let (w, h) = img.dimensions();
    let top = if dual && h as f32 >= w as f32 * 0.9 { img.crop_imm(0, 0, w, h / 2) } else { img.clone() };
    let k = 1280u32.div_ceil(top.width().max(1)).clamp(1, 8);
    top.resize_exact(top.width() * k, top.height() * k, FilterType::Nearest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_serials() {
        let dat = "game (\n\tname \"Pokemon - Version Platine (France)\"\n\tregion \"France\"\n\tserial \"CPUF\"\n\trom ( name \"x.nds\" serial \"CPUF\" )\n)\n\
                   game (\n\tname \"Pokemon Omega Ruby (Europe) (Rev 2)\"\n\tserial \"CTR-P-ECRP\"\n)\n\
                   game (\n\tname \"Okami (USA)\"\n\tserial \"SLUS-21115\"\n)\n";
        let map = parse_dat(dat);
        assert_eq!(map["CPUF"], vec!["Pokemon - Version Platine (France)".to_string()]);
        assert_eq!(map["ECRP"], vec!["Pokemon Omega Ruby (Europe) (Rev 2)".to_string()]);
        assert_eq!(map["SLUS21115"], vec!["Okami (USA)".to_string()]);
    }

    #[test]
    fn prefers_the_games_own_region() {
        assert!(score("Pokemon Ultra Sun (Europe)", 'A', true) > score("Pocket Monsters Ultra Sun (Japan)", 'A', true));
        assert!(score("Game (USA)", 'E', false) > score("Game (USA) (Demo)", 'E', false));
        assert_eq!(region("GALE01"), 'E');
        assert_eq!(region("SLES52056"), 'P');
        assert_eq!(region("CPUF"), 'F');
    }

    #[test]
    fn keeps_the_top_screen() {
        let shot = DynamicImage::new_rgb8(256, 384);
        assert_eq!(screen(&shot, true).dimensions(), (1280, 960));
        assert_eq!(screen(&DynamicImage::new_rgb8(320, 240), false).dimensions(), (1280, 960));
        assert_eq!(boxed(&DynamicImage::new_rgb8(400, 352)).dimensions(), COVER);
    }

    /// `cargo test lookup_roms -- --ignored --nocapture` (network).
    #[test]
    #[ignore]
    fn lookup_roms() {
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20)).build();
        std::fs::create_dir_all(art_dir()).unwrap();
        let mut dats = Dats::default();
        for (console, code) in [(Source::Ds, "CPUF"), (Source::N3ds, "ECRA"), (Source::Gc, "GZLE01"), (Source::Ps2, "SLUS-21115")] {
            let job = Job { id: "x".into(), name: String::new(), appid: None, local: Default::default(), rom: None, remote_cover: None };
            let rom = Rom { code: code.into(), ..Default::default() };
            let mut meta = Meta::default();
            lookup(&agent, &job, console, &rom, true, &mut meta, &mut dats);
            println!("{code}: {:?}", meta.art);
            assert!(meta.art.cover.is_some(), "{code}");
        }
    }
}
