//! Play time for every game, not only Steam's: after a launch, Arach watches for a process
//! running from the game's folder (or the emulator, for a ROM), and records a session from the
//! moment one appears until none is left. Store links (`steam://`, `uplay://`…) start the game
//! through its launcher, so the watch is on the folder, not on the process Arach started.

use std::path::Path;
use std::time::{Duration, Instant};

/// How long a store may take to start the game (updates, sign-in) before Arach gives up.
const WAIT: Duration = Duration::from_secs(5 * 60);
/// While waiting for the game to appear.
const WAIT_POLL: Duration = Duration::from_secs(2);
/// While it runs.
const RUN_POLL: Duration = Duration::from_secs(5);
/// Polls in a row with nothing running before the session ends: a launcher handing over to
/// the game, or a game restarting itself, leaves a gap of a few seconds.
const MISSES: u32 = 3;

/// What to look for: processes whose executable lives under one of `dirs`, or is one of `exes`.
#[derive(Clone, Debug, Default)]
pub struct Target {
    dirs: Vec<String>,
    exes: Vec<String>,
}

impl Target {
    /// A folder: any process inside it counts. A drive root or a folder right under it
    /// (`D:\Games`) is too wide to say which game runs, so it's ignored.
    pub fn dir(mut self, dir: &Path) -> Self {
        let n = norm(dir);
        let depth = n.trim_end_matches('\\').split('\\').count();
        if depth >= 3 && dir.is_dir() {
            self.dirs.push(n);
        }
        self
    }

    pub fn exe(mut self, exe: &Path) -> Self {
        self.exes.push(norm(exe).trim_end_matches('\\').to_string());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty() && self.exes.is_empty()
    }

    fn matches(&self, path: &str) -> bool {
        self.exes.iter().any(|e| e == path) || self.dirs.iter().any(|d| path.starts_with(d.as_str()))
    }
}

/// Lowercase, backslashes, a folder ends with one: `c:\games\celeste\`.
fn norm(p: &Path) -> String {
    let mut s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    if let Some(rest) = s.strip_prefix("\\\\?\\") { s = rest.to_string(); }
    if !s.ends_with('\\') && !s.ends_with(".exe") { s.push('\\'); }
    s
}

pub enum Event {
    /// The game showed up, at this Unix time.
    Started(i64),
    /// Still running: about once a minute, so the time shown moves on.
    Running,
    /// It's gone: the session ran from `start` to `end`.
    Ended { start: i64, end: i64 },
    /// Nothing ever ran from its folder.
    NotSeen,
}

/// Watches on its own thread and reports each step through `on`.
pub fn watch(target: Target, on: impl Fn(Event) + Send + 'static) {
    std::thread::spawn(move || {
        let mut procs = Procs::default();
        let begin = Instant::now();
        let start = loop {
            if procs.any(&target) { break crate::store::now(); }
            if begin.elapsed() > WAIT {
                on(Event::NotSeen);
                return;
            }
            std::thread::sleep(WAIT_POLL);
        };
        on(Event::Started(start));
        let (mut last, mut misses, mut told) = (start, 0, start);
        while misses < MISSES {
            std::thread::sleep(RUN_POLL);
            if procs.any(&target) {
                last = crate::store::now();
                misses = 0;
                if last - told >= 60 {
                    told = last;
                    on(Event::Running);
                }
            } else {
                misses += 1;
            }
        }
        on(Event::Ended { start, end: last });
    });
}

/// The running processes' executables, remembered by pid between polls so each is asked once.
#[derive(Default)]
struct Procs {
    #[cfg(windows)]
    paths: std::collections::HashMap<u32, Option<String>>,
}

impl Procs {
    #[cfg(windows)]
    fn any(&mut self, target: &Target) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
        };

        let mut pids = Vec::new();
        // SAFETY: the snapshot handle is checked and closed; the entry's size is set before
        // the first call, as the API requires.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE || snap.is_null() { return false; }
            let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok {
                pids.push(e.th32ProcessID);
                ok = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
        let me = std::process::id();
        let live: std::collections::HashSet<u32> = pids.iter().copied().collect();
        self.paths.retain(|pid, _| live.contains(pid));
        let mut found = false;
        for pid in pids {
            if pid == 0 || pid == 4 || pid == me { continue; }
            let path = self.paths.entry(pid).or_insert_with(|| image_path(pid));
            if path.as_deref().is_some_and(|p| target.matches(p)) { found = true; }
        }
        found
    }

    #[cfg(not(windows))]
    fn any(&mut self, _: &Target) -> bool {
        false
    }
}

/// Every process Arach can see (not itself), with its executable's path, lowercased.
#[cfg(windows)]
fn processes() -> Vec<(u32, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    let mut pids = Vec::new();
    // SAFETY: as in `Procs::any`.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() { return Vec::new(); }
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            pids.push(e.th32ProcessID);
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    let me = std::process::id();
    pids.into_iter()
        .filter(|&pid| pid != 0 && pid != 4 && pid != me)
        .filter_map(|pid| Some((pid, image_path(pid)?)))
        .collect()
}

#[cfg(not(windows))]
fn processes() -> Vec<(u32, String)> {
    Vec::new()
}

/// The executable of the first process whose (lowercase) path passes `pred`.
pub fn find_process(pred: impl Fn(&str) -> bool) -> Option<std::path::PathBuf> {
    processes().into_iter().find(|(_, p)| pred(p)).map(|(_, p)| p.into())
}

/// Ends every process whose (lowercase) path passes `pred`; says how many ended.
pub fn kill_where(pred: impl Fn(&str) -> bool) -> usize {
    processes().into_iter().filter(|(_, p)| pred(p)).filter(|(pid, _)| terminate(*pid)).count()
}

/// Ends the game: every process `target` would count as it running.
pub fn kill(target: &Target) -> usize {
    if target.is_empty() { return 0; }
    kill_where(|p| target.matches(p))
}

#[cfg(windows)]
fn terminate(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    // SAFETY: the handle is checked and closed.
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h.is_null() { return false; }
        let ok = TerminateProcess(h, 1) != 0;
        CloseHandle(h);
        ok
    }
}

#[cfg(not(windows))]
fn terminate(_: u32) -> bool {
    false
}

/// The full path of a process's executable, lowercased; `None` for protected processes.
#[cfg(windows)]
fn image_path(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};

    // SAFETY: the handle is checked and closed; the buffer outlives the call and its length
    // goes in and comes back through `len`.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() { return None; }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]).to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_match_what_runs_inside_them() {
        let t = Target { dirs: vec![norm(Path::new(r"D:\SteamLibrary\steamapps\common\Celeste"))], exes: vec![] };
        assert!(t.matches(r"d:\steamlibrary\steamapps\common\celeste\celeste.exe"));
        assert!(!t.matches(r"d:\steamlibrary\steamapps\common\celeste 2\celeste.exe"));
        let t = Target::default().exe(Path::new("C:/Emu/Azahar/azahar.exe"));
        assert!(t.matches(r"c:\emu\azahar\azahar.exe"));
    }

    #[test]
    fn wide_folders_are_ignored() {
        assert!(Target::default().dir(Path::new(r"C:\")).is_empty());
        assert!(Target::default().dir(Path::new(r"C:\Windows")).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn sees_this_process_tree() {
        // Windows' own folder always has something running.
        let t = Target { dirs: vec![norm(Path::new(r"C:\Windows\System32"))], exes: vec![] };
        assert!(Procs::default().any(&t));
    }
}
