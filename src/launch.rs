//! Starting a game through the Windows shell: store links (`steam://…`), executables (with
//! elevation when their manifest asks for it) and shortcuts all go through `ShellExecuteW`.

use std::path::Path;

use crate::game::{Game, Launch};

/// Starts `game`, adding the user's own arguments (`extra`) after the store's.
pub fn launch(game: &Game, extra: &str) -> Result<(), String> {
    let extra = extra.trim();
    match &game.launch {
        // Steam takes arguments through its run link: steam://run/<id>//<args>/
        Launch::Uri(_) if !extra.is_empty() && game.steam_appid.is_some() && game.id.starts_with("steam:") => {
            let id = game.steam_appid.unwrap_or_default();
            shell_open(&format!("steam://run/{id}//{}/", extra.replace(' ', "%20")), "", None)
        }
        Launch::Uri(uri) => shell_open(uri, "", None),
        Launch::Exe { path, args, cwd } => {
            if !path.exists() {
                return Err(format!("Exécutable introuvable : {}", path.display()));
            }
            let cwd = cwd.as_deref().or_else(|| path.parent());
            let all = [args.as_str(), extra].iter().filter(|a| !a.is_empty()).copied().collect::<Vec<_>>().join(" ");
            shell_open(&path.to_string_lossy(), &all, cwd)
        }
    }
}

/// A store link or web page.
pub fn open_uri(uri: &str) -> Result<(), String> {
    shell_open(uri, "", None)
}

/// A program with its arguments (an uninstaller); the shell asks for elevation if it needs it.
pub fn run(exe: &str, args: &str) -> Result<(), String> {
    shell_open(exe, args, None)
}

/// A folder, or the folder holding a file (a ROM) with that file selected.
pub fn open_folder(dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if dir.is_file() {
        return shell_open("explorer.exe", &format!("/select,\"{}\"", dir.display()), None);
    }
    shell_open(&dir.to_string_lossy(), "", None)
}

#[cfg(windows)]
fn shell_open(target: &str, args: &str, cwd: Option<&Path>) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }
    let verb = wide("open".as_ref());
    let file = wide(target.as_ref());
    let params = wide(args.as_ref());
    let dir = cwd.map(|d| wide(d.as_os_str()));
    // SAFETY: every pointer is a NUL-terminated UTF-16 buffer that outlives the call; the shell
    // wants COM on the calling thread for some protocol handlers, which this worker sets up.
    let code = unsafe {
        CoInitializeEx(std::ptr::null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32);
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            if args.is_empty() { std::ptr::null() } else { params.as_ptr() },
            dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            SW_SHOWNORMAL,
        )
    } as isize;
    // ShellExecute returns a value above 32 on success.
    match code {
        c if c > 32 => Ok(()),
        2 | 3 => Err("Fichier introuvable".into()),
        5 => Err("Accès refusé".into()),
        31 => Err("Aucune application n'ouvre ce lien. Le launcher du jeu est-il installé ?".into()),
        c => Err(format!("Le lancement a échoué (code {c})")),
    }
}

#[cfg(not(windows))]
fn shell_open(target: &str, args: &str, cwd: Option<&Path>) -> Result<(), String> {
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(target);
    if !args.is_empty() { cmd = std::process::Command::new(target); cmd.args(args.split_whitespace()); }
    if let Some(d) = cwd { cmd.current_dir(d); }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}
