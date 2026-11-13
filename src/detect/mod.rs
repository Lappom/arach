//! Finds every installed game, across stores and drives, in parallel.

pub mod discs;
pub mod emulators;
pub mod exe;
mod epic;
mod itch;
mod legendary;
mod owned;
mod folders;
#[cfg(windows)]
mod registry;
pub mod roms;
mod steam;
pub mod vdf;
mod xbox;

use std::collections::{BTreeMap, HashSet};

use std::path::PathBuf;

use crate::game::{Game, Source};

/// What to look through.
#[derive(Clone)]
pub struct ScanOptions {
    pub stores: Vec<Source>,
    pub folders: Vec<PathBuf>,
    /// Folders of ROMs added by hand.
    pub roms: Vec<PathBuf>,
    /// Emulators the user picked, by id; the others are looked for.
    pub emulators: BTreeMap<String, PathBuf>,
    /// ROM titles in French when the ROM has them.
    pub french: bool,
    /// Also list games owned but not installed.
    pub owned: bool,
}

/// What a scan found: the games, and where the emulators that play the ROMs are.
pub struct Scan {
    pub games: Vec<Game>,
    pub emulators: roms::Setup,
}

/// All stores at once; each runs on its own thread, so a scan takes as long as the slowest.
/// Two stores claiming the same folder (an EA game bought on Steam) both stay: the library
/// shows them as one game with two versions.
pub fn scan(opts: &ScanOptions) -> Scan {
    let on = |s: Source| opts.stores.contains(&s);
    let consoles = Source::CONSOLES.iter().any(|c| on(*c));
    let (mut found, emulators): (Vec<Game>, roms::Setup) = std::thread::scope(|scope| {
        let mut jobs = Vec::new();
        if on(Source::Steam) { jobs.push(scope.spawn(steam::detect)); }
        if on(Source::Epic) { jobs.push(scope.spawn(epic::detect)); }
        if on(Source::Itch) { jobs.push(scope.spawn(itch::detect)); }
        if on(Source::Heroic) { jobs.push(scope.spawn(legendary::heroic)); }
        if on(Source::Legendary) { jobs.push(scope.spawn(legendary::legendary)); }
        #[cfg(windows)]
        if on(Source::Xbox) { jobs.push(scope.spawn(xbox::detect)); }
        if !opts.folders.is_empty() { jobs.push(scope.spawn(|| folders::detect(&opts.folders))); }
        #[cfg(windows)]
        {
            if on(Source::Gog) { jobs.push(scope.spawn(registry::gog)); }
            jobs.push(scope.spawn(move || {
                let entries = registry::uninstall_entries();
                let mut out = Vec::new();
                if on(Source::Ubisoft) { out.extend(registry::ubisoft(&entries)); }
                if on(Source::Riot) { out.extend(registry::riot(&entries)); }
                if on(Source::Ea) { out.extend(registry::ea(&entries)); }
                if on(Source::BattleNet) { out.extend(registry::battlenet(&entries)); }
                if on(Source::Amazon) { out.extend(registry::amazon(&entries)); }
                if on(Source::Rockstar) { out.extend(registry::rockstar(&entries)); }
                out
            }));
        }
        // The emulators are looked for even with both consoles off, for Paramètres to show.
        let consoles_job = scope.spawn(move || {
            let setup = roms::setup(opts);
            let games = if consoles { roms::detect(opts, &setup) } else { Vec::new() };
            (games, setup)
        });
        let mut games: Vec<Game> = jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect();
        let (rom_games, setup) = consoles_job.join().unwrap_or_default();
        games.extend(rom_games);
        (games, setup)
    });
    found.sort_by(|a, b| a.source.cmp(&b.source));
    let mut ids = HashSet::new();
    found.retain(|g| ids.insert(g.id.clone()));
    if opts.owned {
        // Owned elsewhere and installed here (or on two stores): shown once, installed first.
        let mut names: HashSet<String> = found.iter().map(|g| exe::squash(&g.name)).collect();
        for g in owned::detect(opts) {
            if ids.contains(&g.id) || !names.insert(exe::squash(&g.name)) { continue; }
            ids.insert(g.id.clone());
            found.push(g);
        }
    }
    Scan { games: found, emulators }
}

#[cfg(test)]
mod tests {
    /// Runs the real detectors on this machine and prints what they found:
    /// `cargo test scan_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn scan_this_pc() {
        let t = std::time::Instant::now();
        let opts = super::ScanOptions {
            stores: crate::game::Source::STORES.to_vec(),
            folders: Vec::new(),
            roms: Vec::new(),
            emulators: Default::default(),
            french: true,
            owned: true,
        };
        let games = super::scan(&opts).games;
        let took = t.elapsed();
        for g in &games {
            println!("{:<11} {:<40} icon={:?}", g.source.label(), g.name, g.icon.as_ref().map(|p| p.display().to_string()));
            println!("            {:?} size={} played={}min art={:?}", g.launch, g.size, g.playtime_min, g.art.cover.is_some());
        }
        let owned = games.iter().filter(|g| g.not_installed).count();
        println!("{} games ({owned} not installed) in {took:?}", games.len());
    }
}
