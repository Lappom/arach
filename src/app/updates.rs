//! Arach updating itself: checking GitHub, downloading in the background, and restarting into
//! the new version when the user says so.

use std::path::PathBuf;

use rune::Tx;

use super::State;
use crate::update::{self, Release};

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Update {
    #[default]
    Idle,
    Checking,
    /// GitHub has nothing newer, as of this check.
    UpToDate,
    Downloading { version: String, done: u64, total: u64 },
    /// Downloaded and checked: installs on the user's word.
    Ready { release: Release, path: PathBuf },
    Failed(String),
}

impl Update {
    pub fn busy(&self) -> bool {
        matches!(self, Update::Checking | Update::Downloading { .. })
    }
}

impl State {
    /// Asks GitHub for a newer release and downloads it. `manual` (the button in À propos) also
    /// says when there's nothing new or the check failed; a check on its own stays quiet.
    pub fn check_update(&mut self, manual: bool) {
        if self.update.busy() || matches!(self.update, Update::Ready { .. }) { return; }
        self.update = Update::Checking;
        let me = self.me.clone();
        std::thread::spawn(move || {
            let found = update::latest();
            me.update(move |s: &mut State, tx: &mut Tx| match found {
                Ok(Some(release)) => s.download_update(release),
                Ok(None) => {
                    s.update = Update::UpToDate;
                    if manual { s.notify(format!("Arach {} est à jour", update::current()), false, tx); }
                }
                Err(e) => {
                    if manual { s.notify(e.clone(), true, tx); }
                    s.update = Update::Failed(e);
                }
            });
        });
    }

    fn download_update(&mut self, release: Release) {
        self.update = Update::Downloading { version: release.version.clone(), done: 0, total: release.size };
        let me = self.me.clone();
        std::thread::spawn(move || {
            let progress = me.clone();
            let got = update::download(&release, |done| {
                progress.update(move |s: &mut State, _: &mut Tx| {
                    if let Update::Downloading { done: d, .. } = &mut s.update { *d = done; }
                });
            });
            me.update(move |s: &mut State, tx: &mut Tx| match got {
                Ok(path) => {
                    s.notify(format!("Arach {} est prêt · redémarrez pour l'installer", release.version), false, tx);
                    s.update = Update::Ready { release, path };
                }
                Err(e) => {
                    s.notify(e.clone(), true, tx);
                    s.update = Update::Failed(e);
                }
            });
        });
    }

    /// Puts the downloaded version in place and restarts into it.
    pub fn install_update(&mut self, tx: &mut Tx) {
        let Update::Ready { path, .. } = &self.update else { return };
        match update::install(path) {
            Ok(()) => tx.quit(),
            Err(e) => {
                self.notify(e.clone(), true, tx);
                self.update = Update::Failed(e);
            }
        }
    }

    /// After a restart into a new version.
    pub fn updated(&mut self, tx: &mut Tx) {
        self.notify(format!("Arach a été mis à jour en version {}", update::current()), false, tx);
    }
}
