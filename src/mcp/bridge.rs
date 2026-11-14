//! How the MCP server asks the window to change something. The window holds the library in
//! memory and saves it whenever it changes, so a write from the server's own process would be
//! lost at the window's next save: every change goes through the window instead.
//!
//! The server starts `arach.exe arach://mcp/<nonce>/<request>`. A running window receives the
//! link (`rune::single_instance` hands it over and the new process exits); with no window, that
//! process becomes it. The window applies the request and writes its answer to
//! `%LOCALAPPDATA%\Arach\mcp\replies\<nonce>.json`, which the server waits for.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer, Serialize};

use crate::store::Status;

/// How long the server waits for the window, which may have to start first.
const WAIT: Duration = Duration::from_secs(25);
const PREFIX: &str = "arach://mcp/";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Launch { id: String },
    EditGame { id: String, patch: GamePatch },
    /// `path`: a picture already in the user's covers folder; `None` goes back to the store's.
    SetArt { id: String, kind: ArtKind, path: Option<PathBuf> },
    /// `id`: one game's actions; `None`: every game's.
    SetActions { id: Option<String>, patch: ActionsPatch },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtKind {
    Cover,
    Background,
    Logo,
}

impl ArtKind {
    pub fn parse(s: &str) -> Option<ArtKind> {
        match s {
            "cover" => Some(ArtKind::Cover),
            "background" | "hero" => Some(ArtKind::Background),
            "logo" => Some(ArtKind::Logo),
            _ => None,
        }
    }

    pub fn picker(self) -> crate::meta::picker::Kind {
        use crate::meta::picker::Kind;
        match self {
            ArtKind::Cover => Kind::Cover,
            ArtKind::Background => Kind::Hero,
            ArtKind::Logo => Kind::Logo,
        }
    }
}

/// `Some(None)` from a JSON `null`: a field set back to what the store says.
fn some<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// What an assistant changes about a game. Absent fields stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GamePatch {
    #[serde(deserialize_with = "some", skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    #[serde(deserialize_with = "some", skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(deserialize_with = "some", skip_serializing_if = "Option::is_none")]
    pub developers: Option<Option<Vec<String>>>,
    #[serde(deserialize_with = "some", skip_serializing_if = "Option::is_none")]
    pub publishers: Option<Option<Vec<String>>>,
    #[serde(deserialize_with = "some", skip_serializing_if = "Option::is_none")]
    pub release: Option<Option<String>>,
    #[serde(deserialize_with = "some", skip_serializing_if = "Option::is_none")]
    pub genres: Option<Option<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    /// 0 clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
    /// Collections by name; a missing one is created.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub add_to: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remove_from: Vec<String>,
}

impl GamePatch {
    pub fn touches_facts(&self) -> bool {
        self.name.is_some() || self.description.is_some() || self.developers.is_some()
            || self.publishers.is_some() || self.release.is_some() || self.genres.is_some()
    }
}

/// What an assistant changes around a game. Absent fields stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionsPatch {
    /// For one game: `false` puts it back on every game's actions, `true` gives it its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_discord: Option<bool>,
    /// A plan's GUID; empty leaves the power plan alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_plan: Option<String>,
    #[serde(deserialize_with = "some", skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Option<(u32, u32)>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl ActionsPatch {
    pub fn touches_scripts(&self) -> bool {
        self.before.is_some() || self.after.is_some()
    }

    pub fn touches_system(&self) -> bool {
        self.close_discord.is_some() || self.power_plan.is_some() || self.resolution.is_some()
    }

    pub fn apply(&self, a: &mut crate::settings::Actions) {
        if let Some(v) = self.close_discord { a.close_discord = v; }
        if let Some(v) = &self.power_plan { a.power_plan = v.clone(); }
        if let Some(v) = self.resolution { a.resolution = v; }
        if let Some(v) = &self.before { a.before = v.clone(); }
        if let Some(v) = &self.after { a.after = v.clone(); }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
}

impl Reply {
    pub fn ok(message: impl Into<String>) -> Reply {
        Reply { ok: true, message: message.into() }
    }
    pub fn err(message: impl Into<String>) -> Reply {
        Reply { ok: false, message: message.into() }
    }
}

pub fn link(nonce: &str, req: &Request) -> String {
    let json = serde_json::to_string(req).unwrap_or_default();
    let mut out = format!("{PREFIX}{nonce}/");
    for b in json.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The nonce and request a link carries; `None` for any other link.
pub fn parse(url: &str) -> Option<(String, Request)> {
    let (nonce, body) = url.strip_prefix(PREFIX)?.split_once('/')?;
    if !valid_nonce(nonce) { return None; }
    let bytes = body.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some((nonce.to_string(), serde_json::from_slice(&out).ok()?))
}

/// Hex only, so it can name a file and nothing else.
fn valid_nonce(n: &str) -> bool {
    !n.is_empty() && n.len() <= 32 && n.bytes().all(|b| b.is_ascii_hexdigit())
}

fn replies() -> PathBuf {
    crate::store::cache_dir().join("mcp").join("replies")
}

/// The window's answer to a request, written once the saves it made are on disk, so the
/// server's next read sees them.
pub fn answer(nonce: &str, reply: &Reply) {
    if !valid_nonce(nonce) { return; }
    let (nonce, reply) = (nonce.to_string(), reply.clone());
    crate::store::after_saves(move || write_answer(&nonce, &reply));
}

fn write_answer(nonce: &str, reply: &Reply) {
    let dir = replies();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{nonce}.json"));
    let tmp = path.with_extension("tmp");
    if let Ok(bytes) = serde_json::to_vec(reply) && std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Hands `req` to the window and waits for its answer.
pub fn send(req: &Request) -> Result<String, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let nonce = format!("{:x}{:x}", std::process::id(), nanos & 0xffff_ffff_ffff);
    let exe = std::env::current_exe().map_err(|e| format!("Arach can't find its own executable: {e}"))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(link(&nonce, req))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn().map_err(|e| format!("Arach couldn't start: {e}"))?;
    let path = replies().join(format!("{nonce}.json"));
    let start = Instant::now();
    while start.elapsed() < WAIT {
        if let Ok(bytes) = std::fs::read(&path) {
            let _ = std::fs::remove_file(&path);
            let reply: Reply = serde_json::from_slice(&bytes).map_err(|e| format!("Unreadable answer from Arach: {e}"))?;
            return if reply.ok { Ok(reply.message) } else { Err(reply.message) };
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Err("Arach didn't answer in time. Is its window stuck, or blocked by a dialog?".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_survive_the_link() {
        let reqs = [
            Request::Launch { id: r"folder:D:\Games\Hollow Knight".into() },
            Request::EditGame { id: "steam:1".into(), patch: GamePatch {
                name: Some(Some("Été 85 & co".into())),
                description: Some(None),
                genres: Some(Some(vec!["RPG".into()])),
                status: Some(Status::Finished),
                rating: Some(4),
                add_to: vec!["À finir".into()],
                ..Default::default()
            } },
            Request::SetArt { id: "epic:x".into(), kind: ArtKind::Logo, path: None },
            Request::SetActions { id: None, patch: ActionsPatch { resolution: Some(None), close_discord: Some(true), ..Default::default() } },
        ];
        for req in reqs {
            let l = link("abc123", &req);
            let tail = &l[PREFIX.len()..];
            assert!(tail.matches('/').count() == 1 && !tail.contains([' ', '\\', '"']), "{l}");
            assert_eq!(parse(&l), Some(("abc123".to_string(), req)));
        }
    }

    #[test]
    fn null_means_back_to_the_store() {
        let p: GamePatch = serde_json::from_str(r#"{"name": null, "notes": "x"}"#).unwrap();
        assert_eq!(p.name, Some(None));
        assert_eq!(p.description, None);
        assert!(p.touches_facts());
    }

    #[test]
    fn refuses_odd_nonces() {
        let l = link("abc", &Request::Launch { id: "a".into() });
        assert!(parse(&l.replace("abc", "..")).is_none());
        assert!(parse(&l.replace("abc", "")).is_none());
        assert!(parse("arach://launch/steam:1").is_none());
    }
}
