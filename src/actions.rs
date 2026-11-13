//! Running a game's actions: `before` changes the PC and says how to put it back; `Restore::run`
//! puts it back once the game has closed.

use std::path::PathBuf;

use crate::settings::Actions;
use crate::system;

/// What to undo, and what to run, once the game closes.
#[derive(Debug, Default)]
pub struct Restore {
    /// The plan in use before the game's.
    power_plan: Option<String>,
    resolution: bool,
    /// Discord, closed for the game, to start again.
    discord: Option<PathBuf>,
    after: String,
}

impl Restore {
    pub fn is_empty(&self) -> bool {
        self.power_plan.is_none() && !self.resolution && self.discord.is_none() && self.after.trim().is_empty()
    }

    /// Puts the PC back as it was, then runs the user's script. Blocks a moment: call it off
    /// the UI thread.
    pub fn run(self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.resolution { system::restore_resolution(); }
        if let Some(guid) = &self.power_plan && let Err(e) = system::set_power_plan(guid) { errors.push(e); }
        if let Some(exe) = &self.discord { system::start_discord(exe); }
        if let Err(e) = system::run_script(&self.after, false) { errors.push(e); }
        errors
    }
}

/// Runs everything `a` asks before a game starts, and returns how to undo it with what went
/// wrong. Blocks while the script runs: call it off the UI thread.
pub fn before(a: &Actions) -> (Restore, Vec<String>) {
    let mut errors = Vec::new();
    let mut restore = Restore { after: a.after.clone(), ..Restore::default() };
    if let Err(e) = system::run_script(&a.before, true) { errors.push(e); }
    if a.close_discord { restore.discord = system::close_discord(); }
    if !a.power_plan.is_empty() {
        let was = system::active_power_plan();
        if was.as_deref() != Some(a.power_plan.as_str()) {
            match system::set_power_plan(&a.power_plan) {
                Ok(()) => restore.power_plan = was,
                Err(e) => errors.push(e),
            }
        }
    }
    if let Some((w, h)) = a.resolution {
        match system::set_resolution(w, h) {
            Ok(()) => restore.resolution = true,
            Err(e) => errors.push(e),
        }
    }
    (restore, errors)
}
