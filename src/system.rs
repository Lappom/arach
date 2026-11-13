//! What Arach changes on the PC around a game: the power plan, the screen resolution, Discord,
//! and the user's own scripts. Each change made before the game is undone after it
//! (`actions::Restore`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// A power plan Windows offers: its GUID (lowercase, with hyphens) and its name in the
/// system's language ("Performances élevées").
pub struct PowerPlan {
    pub guid: String,
    pub name: &'static str,
}

/// The power plans on this PC, read once. Names are leaked: a handful of short strings for the
/// whole run, so the settings dropdown can show them as `&'static str`.
pub fn power_plans() -> &'static [PowerPlan] {
    static PLANS: OnceLock<Vec<PowerPlan>> = OnceLock::new();
    PLANS.get_or_init(read_power_plans)
}

/// Screen sizes the main display offers, largest first, with their label ("2560 × 1440").
pub fn resolutions() -> &'static [((u32, u32), &'static str)] {
    static MODES: OnceLock<Vec<((u32, u32), &'static str)>> = OnceLock::new();
    MODES.get_or_init(|| {
        let mut sizes = display_modes();
        sizes.sort_by(|a, b| (b.0 * b.1).cmp(&(a.0 * a.1)).then(b.0.cmp(&a.0)));
        sizes.dedup();
        sizes.into_iter().map(|(w, h)| ((w, h), &*Box::leak(format!("{w} × {h}").into_boxed_str()))).collect()
    })
}

/// Reads both lists on a worker so the first visit to Paramètres doesn't wait on them.
pub fn warm() {
    std::thread::spawn(|| {
        power_plans();
        resolutions();
    });
}

// ---------------------------------------------------------------- power plans

#[cfg(windows)]
fn read_power_plans() -> Vec<PowerPlan> {
    use windows_sys::Win32::System::Power::{ACCESS_SCHEME, PowerEnumerate, PowerReadFriendlyName};
    use windows_sys::core::GUID;

    let mut plans = Vec::new();
    for index in 0..64 {
        let mut guid = GUID { data1: 0, data2: 0, data3: 0, data4: [0; 8] };
        let mut size = std::mem::size_of::<GUID>() as u32;
        // SAFETY: the buffer is a GUID and `size` says how big it is.
        let err = unsafe {
            PowerEnumerate(std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), ACCESS_SCHEME, index, (&mut guid as *mut GUID).cast(), &mut size)
        };
        if err != 0 { break; }
        let mut buf = [0u16; 256];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: the buffer holds `bytes` bytes; the call writes a NUL-terminated UTF-16 name.
        let ok = unsafe {
            PowerReadFriendlyName(std::ptr::null_mut(), &guid, std::ptr::null(), std::ptr::null(), buf.as_mut_ptr().cast(), &mut bytes)
        } == 0;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let name = if ok && len > 0 { String::from_utf16_lossy(&buf[..len]) } else { guid_string(&guid) };
        plans.push(PowerPlan { guid: guid_string(&guid), name: Box::leak(name.into_boxed_str()) });
    }
    plans
}

#[cfg(not(windows))]
fn read_power_plans() -> Vec<PowerPlan> {
    Vec::new()
}

/// The plan in use now.
#[cfg(windows)]
pub fn active_power_plan() -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::System::Power::PowerGetActiveScheme;
    use windows_sys::core::GUID;

    let mut ptr: *mut GUID = std::ptr::null_mut();
    // SAFETY: on success Windows allocates the GUID with LocalAlloc; it's read, then freed.
    unsafe {
        if PowerGetActiveScheme(std::ptr::null_mut(), &mut ptr) != 0 || ptr.is_null() { return None; }
        let s = guid_string(&*ptr);
        LocalFree(ptr.cast());
        Some(s)
    }
}

#[cfg(not(windows))]
pub fn active_power_plan() -> Option<String> {
    None
}

#[cfg(windows)]
pub fn set_power_plan(guid: &str) -> Result<(), String> {
    use windows_sys::Win32::System::Power::PowerSetActiveScheme;
    let g = parse_guid(guid).ok_or("Mode d'alimentation inconnu")?;
    // SAFETY: a valid GUID, read during the call.
    match unsafe { PowerSetActiveScheme(std::ptr::null_mut(), &g) } {
        0 => Ok(()),
        e => Err(format!("Le mode d'alimentation n'a pas changé (erreur {e})")),
    }
}

#[cfg(not(windows))]
pub fn set_power_plan(_: &str) -> Result<(), String> {
    Err("Indisponible sur ce système".into())
}

#[cfg(windows)]
fn guid_string(g: &windows_sys::core::GUID) -> String {
    let d = g.data4;
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1, g.data2, g.data3, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
    )
}

#[cfg(windows)]
fn parse_guid(s: &str) -> Option<windows_sys::core::GUID> {
    let hex: String = s.chars().filter(|c| *c != '-' && *c != '{' && *c != '}').collect();
    if hex.len() != 32 { return None; }
    let n = u128::from_str_radix(&hex, 16).ok()?;
    let b = n.to_be_bytes();
    Some(windows_sys::core::GUID {
        data1: u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        data2: u16::from_be_bytes([b[4], b[5]]),
        data3: u16::from_be_bytes([b[6], b[7]]),
        data4: [b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]],
    })
}

// ---------------------------------------------------------------- screen resolution

#[cfg(windows)]
fn display_modes() -> Vec<(u32, u32)> {
    use windows_sys::Win32::Graphics::Gdi::{DEVMODEW, EnumDisplaySettingsW};
    let mut out = Vec::new();
    for i in 0..1024 {
        let mut m = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
        // SAFETY: `m` is a DEVMODEW with its size set, as the call requires.
        if unsafe { EnumDisplaySettingsW(std::ptr::null(), i, &mut m) } == 0 { break; }
        // Below 800 × 600 nothing modern runs well; 8- and 16-bit modes are legacy.
        if m.dmPelsWidth >= 800 && m.dmPelsHeight >= 600 && m.dmBitsPerPel >= 32 {
            out.push((m.dmPelsWidth, m.dmPelsHeight));
        }
    }
    out
}

#[cfg(not(windows))]
fn display_modes() -> Vec<(u32, u32)> {
    Vec::new()
}

/// Switches the main display to `w` × `h` until `restore_resolution`. The change isn't saved
/// in the registry, so a crash or a restart leaves the usual resolution.
#[cfg(windows)]
pub fn set_resolution(w: u32, h: u32) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::{ChangeDisplaySettingsW, DEVMODEW, DM_PELSHEIGHT, DM_PELSWIDTH, DISP_CHANGE_SUCCESSFUL, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW};
    let mut m = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY: as in `display_modes`; then the same struct, changed, is read by the call.
    unsafe {
        EnumDisplaySettingsW(std::ptr::null(), ENUM_CURRENT_SETTINGS, &mut m);
        if (m.dmPelsWidth, m.dmPelsHeight) == (w, h) { return Ok(()); }
        m.dmPelsWidth = w;
        m.dmPelsHeight = h;
        m.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT;
        match ChangeDisplaySettingsW(&m, 0) {
            DISP_CHANGE_SUCCESSFUL => Ok(()),
            e => Err(format!("L'écran n'est pas passé en {w} × {h} (erreur {e})")),
        }
    }
}

#[cfg(not(windows))]
pub fn set_resolution(_: u32, _: u32) -> Result<(), String> {
    Err("Indisponible sur ce système".into())
}

/// Back to the resolution saved in Windows' settings.
pub fn restore_resolution() {
    #[cfg(windows)]
    // SAFETY: a null mode asks Windows to reload the registry's.
    unsafe {
        windows_sys::Win32::Graphics::Gdi::ChangeDisplaySettingsW(std::ptr::null(), 0);
    }
}

// ---------------------------------------------------------------- Discord

const DISCORD_EXES: [&str; 3] = ["discord.exe", "discordptb.exe", "discordcanary.exe"];

/// Closes Discord (stable, PTB or Canary) and says where it ran from, to start it again later.
/// `None` when it wasn't running.
pub fn close_discord() -> Option<PathBuf> {
    let path = crate::track::find_process(|exe| DISCORD_EXES.iter().any(|d| exe.ends_with(&format!("\\{d}"))))?;
    crate::track::kill_where(|exe| DISCORD_EXES.iter().any(|d| exe.ends_with(&format!("\\{d}"))));
    Some(path)
}

/// Starts Discord again, minimized to the tray, through its updater when it has one (as its
/// own shortcut does).
pub fn start_discord(exe: &Path) {
    let name = exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let update = exe.parent().and_then(Path::parent).map(|d| d.join("Update.exe"));
    let mut cmd = match update {
        Some(u) if u.is_file() => {
            let mut c = std::process::Command::new(u);
            c.args(["--processStart", &name, "--process-start-args", "--start-minimized"]);
            c
        }
        _ => {
            let mut c = std::process::Command::new(exe);
            c.arg("--start-minimized");
            c
        }
    };
    let _ = cmd.spawn();
}

// ---------------------------------------------------------------- scripts

/// How long a script run before the game may take before the game starts anyway.
const SCRIPT_WAIT: Duration = Duration::from_secs(60);

/// Runs a command line through `cmd`, without a console window. With `wait`, returns once it
/// has finished (or after a minute); otherwise at once.
pub fn run_script(line: &str, wait: bool) -> Result<(), String> {
    let line = line.trim();
    if line.is_empty() { return Ok(()); }
    let mut cmd = std::process::Command::new("cmd");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.raw_arg("/C").raw_arg(line).creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(line);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Le script n'a pas démarré : {e}"))?;
    if !wait { return Ok(()); }
    let begin = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("Le script s'est terminé avec le code {}", status.code().unwrap_or(-1))),
            Ok(None) if begin.elapsed() > SCRIPT_WAIT => return Err("Le script dure plus d'une minute : le jeu démarre quand même".into()),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn guids_round_trip() {
        let s = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
        assert_eq!(guid_string(&parse_guid(s).unwrap()), s);
    }

    #[test]
    fn reads_this_pc() {
        let plans = power_plans();
        assert!(!plans.is_empty());
        let active = active_power_plan().unwrap();
        assert!(plans.iter().any(|p| p.guid == active));
        assert!(!resolutions().is_empty());
        for p in plans { println!("{} {}", p.guid, p.name); }
        println!("{:?}", resolutions().iter().map(|r| r.1).collect::<Vec<_>>());
    }

    #[test]
    fn scripts_run_and_report() {
        assert!(run_script("exit 0", true).is_ok());
        assert!(run_script("exit 3", true).is_err());
    }
}
