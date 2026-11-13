//! Uninstalling any game: Steam through its own link, the Microsoft Store through Windows'
//! settings, everything else through the `UninstallString` its installer left in the registry
//! (matched by install folder, else by name).

use std::path::Path;

use super::screenshots::squash;
use crate::game::{Game, Source, norm_dir};

#[derive(Clone, Debug, PartialEq)]
pub enum Uninstaller {
    /// A link handed to the shell (`steam://uninstall/<id>`, `ms-settings:appsfeatures`).
    Uri(String),
    /// The installer's command line, split into program and arguments.
    Command { exe: String, args: String, name: String },
}

impl Uninstaller {
    /// What the button says happens.
    pub fn how(&self, g: &Game) -> String {
        match self {
            Uninstaller::Uri(u) if u.starts_with("steam:") => "Steam demande confirmation, puis supprime le jeu.".into(),
            Uninstaller::Uri(_) => "Ouvre Applications installées de Windows, où le jeu se désinstalle.".into(),
            Uninstaller::Command { name, .. } => format!("Lance le programme de désinstallation de {} (« {name} » dans Applications installées).", g.source.label()),
        }
    }
}

pub fn find(g: &Game) -> Option<Uninstaller> {
    if g.not_installed || g.source.is_console() { return None; }
    if g.source == Source::Steam && let Some(id) = g.steam_appid {
        return Some(Uninstaller::Uri(format!("steam://uninstall/{id}")));
    }
    if let Some(found) = registry(g) { return Some(found); }
    (g.source == Source::Xbox).then(|| Uninstaller::Uri("ms-settings:appsfeatures".into()))
}

#[cfg(windows)]
fn registry(g: &Game) -> Option<Uninstaller> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    let roots = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];
    let dir = g.dir.as_deref().map(norm_dir);
    let want = squash(&g.name);
    // (by folder, display name, command): a folder match beats a name match.
    let mut best: Option<(bool, String, String)> = None;
    for (hive, path) in roots {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else { continue };
        for key in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey_with_flags(&key, KEY_READ) else { continue };
            let s = |v: &str| k.get_value::<String, _>(v).unwrap_or_default();
            let cmd = s("UninstallString");
            let name = s("DisplayName");
            if cmd.trim().is_empty() || name.is_empty() { continue; }
            let loc = s("InstallLocation");
            let by_dir = dir.as_ref().is_some_and(|d| !loc.trim().is_empty() && norm_dir(Path::new(loc.trim().trim_matches('"'))) == *d);
            let by_name = !want.is_empty() && squash(&name) == want;
            if !(by_dir || by_name) { continue; }
            if best.as_ref().is_none_or(|b| by_dir && !b.0) {
                best = Some((by_dir, name, cmd));
            }
        }
    }
    let (_, name, cmd) = best?;
    let (exe, args) = split_command(&cmd)?;
    Some(Uninstaller::Command { exe, args, name })
}

#[cfg(not(windows))]
fn registry(_: &Game) -> Option<Uninstaller> {
    None
}

/// `"C:\x\unins000.exe" /SILENT` → (`C:\x\unins000.exe`, `/SILENT`); `MsiExec.exe /X{…}` too;
/// an unquoted path with spaces ends at its `.exe`.
pub fn split_command(cmd: &str) -> Option<(String, String)> {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((rest[..end].to_string(), rest[end + 1..].trim().to_string()));
    }
    let lower = cmd.to_ascii_lowercase();
    if let Some(i) = lower.find(".exe") {
        let end = i + 4;
        return Some((cmd[..end].to_string(), cmd[end..].trim().to_string()));
    }
    let mut parts = cmd.splitn(2, ' ');
    Some((parts.next()?.to_string(), parts.next().unwrap_or("").trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::split_command;

    #[test]
    fn splits_uninstall_commands() {
        assert_eq!(split_command(r#""C:\Games\X\unins000.exe" /SILENT"#), Some((r"C:\Games\X\unins000.exe".into(), "/SILENT".into())));
        assert_eq!(split_command("MsiExec.exe /X{1234}"), Some(("MsiExec.exe".into(), "/X{1234}".into())));
        assert_eq!(split_command(r"C:\Program Files\EA\Uninstall.exe uninstall_game -id x"),
            Some((r"C:\Program Files\EA\Uninstall.exe".into(), "uninstall_game -id x".into())));
    }
}
