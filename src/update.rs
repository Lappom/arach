//! Arach's own updates, from the GitHub Releases of `REPO`.
//!
//! The latest release's tag (`v0.2.0`) is compared with this build's version. A newer one's
//! `.exe` downloads to `%LOCALAPPDATA%\Arach\update\`, checked against the size GitHub gives
//! and, when the release carries one, its `.sha256`. Installing renames the running exe aside
//! (Windows lets a running image be renamed, not overwritten), copies the new one in its place
//! and starts it with `--updated-from=<pid>`: the new process waits for this one to exit
//! before it claims the single instance, then deletes the old exe.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const REPO: &str = "Lappom/arach";
/// The argument a freshly installed Arach is started with, followed by the old process's pid.
const UPDATED_FROM: &str = "--updated-from=";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// Without the tag's `v`.
    pub version: String,
    /// The release's page on GitHub.
    pub page: String,
    pub exe_url: String,
    pub size: u64,
    pub sha256_url: Option<String>,
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn releases_page() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

/// `v1.2.3`, `1.2`, `1.2.3-beta` → (1, 2, 3), (1, 2, 0), (1, 2, 3). Pre-release and build
/// suffixes are dropped: GitHub's "latest" never points at a pre-release.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    if parts.next().is_some() { return None; }
    Some((major, minor, patch))
}

pub fn is_newer(tag: &str, than: &str) -> bool {
    match (parse_version(tag), parse_version(than)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(&format!("Arach/{} (game launcher; updater)", current()))
        .build()
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

/// The newest release, when it's newer than this build and carries an `.exe`.
pub fn latest() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let res = agent().get(&url).set("Accept", "application/vnd.github+json").call();
    let rel: GhRelease = match res {
        Ok(r) => r.into_json().map_err(|e| format!("Réponse de GitHub illisible : {e}"))?,
        // No release published yet.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(403 | 429, _)) => return Err("GitHub limite les requêtes, réessayez plus tard".into()),
        Err(ureq::Error::Status(code, _)) => return Err(format!("GitHub a répondu {code}")),
        Err(e) => return Err(format!("GitHub injoignable : {e}")),
    };
    Ok(pick(rel, current()))
}

fn pick(rel: GhRelease, current: &str) -> Option<Release> {
    if rel.draft || rel.prerelease || !is_newer(&rel.tag_name, current) { return None; }
    let exes = rel.assets.iter().filter(|a| a.name.to_ascii_lowercase().ends_with(".exe"));
    // An archive or installer could sit beside it: the one named after Arach wins.
    let exe = exes.clone().find(|a| a.name.to_ascii_lowercase().contains("arach")).or_else(|| exes.clone().next())?;
    let sha_name = format!("{}.sha256", exe.name).to_ascii_lowercase();
    let sha = rel.assets.iter().find(|a| a.name.to_ascii_lowercase() == sha_name);
    Some(Release {
        version: rel.tag_name.trim().trim_start_matches(['v', 'V']).to_string(),
        page: rel.html_url,
        exe_url: exe.browser_download_url.clone(),
        size: exe.size,
        sha256_url: sha.map(|a| a.browser_download_url.clone()),
    })
}

fn download_dir() -> PathBuf {
    crate::store::cache_dir().join("update")
}

/// Downloads the release's exe (or finds it already downloaded), calling `progress` with the
/// bytes so far now and then.
pub fn download(rel: &Release, mut progress: impl FnMut(u64)) -> Result<PathBuf, String> {
    let dir = download_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Impossible de créer {} : {e}", dir.display()))?;
    let dest = dir.join(format!("arach-{}.exe", rel.version));
    // Earlier versions' downloads go.
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if entry.path() != dest { let _ = std::fs::remove_file(entry.path()); }
    }
    let agent = agent();
    let expected = match &rel.sha256_url {
        Some(url) => Some(fetch_sha256(&agent, url)?),
        None => None,
    };
    if dest.metadata().is_ok_and(|m| m.len() == rel.size) && verify(&dest, expected.as_deref()).is_ok() {
        return Ok(dest);
    }

    let res = agent.get(&rel.exe_url).call().map_err(|e| format!("Téléchargement impossible : {e}"))?;
    let part = dest.with_extension("part");
    let mut file = std::fs::File::create(&part).map_err(|e| format!("Impossible d'écrire {} : {e}", part.display()))?;
    let mut body = res.into_reader().take(rel.size + 1);
    let mut buf = vec![0u8; 64 * 1024];
    let (mut done, mut told) = (0u64, 0u64);
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("Téléchargement interrompu : {e}"))?;
        if n == 0 { break; }
        file.write_all(&buf[..n]).map_err(|e| format!("Écriture impossible : {e}"))?;
        done += n as u64;
        if done - told >= 256 * 1024 {
            told = done;
            progress(done);
        }
    }
    file.flush().map_err(|e| format!("Écriture impossible : {e}"))?;
    drop(file);
    progress(done);
    if done != rel.size {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Téléchargement incomplet ({done} octets sur {})", rel.size));
    }
    if let Err(e) = verify(&part, expected.as_deref()) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &dest).map_err(|e| format!("Impossible de ranger la mise à jour : {e}"))?;
    Ok(dest)
}

/// The hash in a `.sha256` file: `<hex>` alone or `<hex>  arach.exe`, as `sha256sum` writes.
fn fetch_sha256(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let text = agent.get(url).call().and_then(|r| Ok(r.into_string()?)).map_err(|e| format!("Empreinte SHA-256 introuvable : {e}"))?;
    parse_sha256(&text).ok_or_else(|| "Empreinte SHA-256 illisible".to_string())
}

fn parse_sha256(text: &str) -> Option<String> {
    let hex = text.split_whitespace().next()?.to_ascii_lowercase();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

fn verify(path: &Path, expected: Option<&str>) -> Result<(), String> {
    let Some(expected) = expected else { return Ok(()) };
    let mut file = std::fs::File::open(path).map_err(|e| format!("Lecture impossible : {e}"))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("Lecture impossible : {e}"))?;
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got == expected { Ok(()) } else { Err("Le fichier téléchargé ne correspond pas à son empreinte SHA-256".into()) }
}

/// Where the running exe goes while the new one takes its name.
fn old_exe(exe: &Path) -> PathBuf {
    exe.with_extension("old.exe")
}

/// Puts `new` in place of the running exe and starts it. The caller quits right after.
pub fn install(new: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Exécutable d'Arach introuvable : {e}"))?;
    let old = old_exe(&exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| denied(&exe, e))?;
    // A copy, not a rename: the download sits on another drive when Arach doesn't live on C:.
    if let Err(e) = std::fs::copy(new, &exe) {
        let _ = std::fs::remove_file(&exe);
        let _ = std::fs::rename(&old, &exe);
        return Err(denied(&exe, e));
    }
    start(&exe).map_err(|e| format!("Mise à jour installée, mais Arach n'a pas pu redémarrer : {e}"))
}

fn denied(exe: &Path, e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        format!("Arach n'a pas le droit de remplacer {} : téléchargez la mise à jour depuis GitHub", exe.display())
    } else {
        format!("Impossible de remplacer {} : {e}", exe.display())
    }
}

fn start(exe: &Path) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(exe);
    cmd.arg(format!("{UPDATED_FROM}{}", std::process::id())).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = exe.parent() { cmd.current_dir(dir); }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn().map(drop)
}

/// At startup, before the single-instance claim: when this process was started by an update,
/// waits for the old one to exit, then clears what the update left. True after an update.
pub fn finish() -> bool {
    let pid = std::env::args().find_map(|a| a.strip_prefix(UPDATED_FROM).and_then(|p| p.parse::<u32>().ok()));
    if let Some(pid) = pid { wait_for_exit(pid, Duration::from_secs(15)); }
    if let Ok(exe) = std::env::current_exe() {
        let old = old_exe(&exe);
        if old.exists() {
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::remove_dir_all(download_dir());
        }
    }
    pid.is_some()
}

#[cfg(windows)]
fn wait_for_exit(pid: u32, within: Duration) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
    // SAFETY: a plain handle request; a process that already exited answers null. The handle is
    // closed once.
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if h.is_null() { return; }
        WaitForSingleObject(h, within.as_millis() as u32);
        CloseHandle(h);
    }
}

#[cfg(not(windows))]
fn wait_for_exit(_pid: u32, within: Duration) {
    std::thread::sleep(within.min(Duration::from_secs(2)));
}

/// Checks run on their own only in a real install: not in a debug build (it would replace
/// `target\debug\arach.exe`), a headless snapshot or a run with its own data folder.
pub fn auto_allowed() -> bool {
    !cfg!(debug_assertions) && std::env::var_os("ARACH_DATA_DIR").is_none() && !std::env::args().any(|a| a == "--snap")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.4"), Some((0, 4, 0)));
        assert_eq!(parse_version("V2.0.1-beta.1"), Some((2, 0, 1)));
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    fn asset(name: &str) -> GhAsset {
        GhAsset { name: name.into(), size: 10, browser_download_url: format!("https://x/{name}") }
    }

    #[test]
    fn picks_the_exe() {
        let rel = GhRelease {
            tag_name: "v0.3.0".into(),
            html_url: "https://github.com/Lappom/arach/releases/tag/v0.3.0".into(),
            draft: false,
            prerelease: false,
            assets: vec![asset("Source.zip"), asset("helper.exe"), asset("arach.exe"), asset("arach.exe.sha256")],
        };
        let r = pick(rel, "0.1.0").unwrap();
        assert_eq!(r.version, "0.3.0");
        assert_eq!(r.exe_url, "https://x/arach.exe");
        assert_eq!(r.sha256_url.as_deref(), Some("https://x/arach.exe.sha256"));
    }

    #[test]
    fn ignores_old_and_prerelease() {
        let mk = |tag: &str, pre: bool| GhRelease { tag_name: tag.into(), html_url: String::new(), draft: false, prerelease: pre, assets: vec![asset("arach.exe")] };
        assert!(pick(mk("v0.1.0", false), "0.1.0").is_none());
        assert!(pick(mk("v0.2.0", true), "0.1.0").is_none());
        let no_exe = GhRelease { assets: vec![asset("arach.zip")], ..mk("v0.2.0", false) };
        assert!(pick(no_exe, "0.1.0").is_none());
    }

    #[test]
    fn sha256_files() {
        let h = "a".repeat(64);
        assert_eq!(parse_sha256(&format!("{h}  arach.exe\n")), Some(h.clone()));
        assert_eq!(parse_sha256(&h.to_uppercase()), Some(h));
        assert_eq!(parse_sha256("nope"), None);
    }

    #[test]
    #[ignore = "network"]
    fn latest_release() {
        println!("{:?}", latest());
    }
}
