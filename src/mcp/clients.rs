//! The AI apps Arach knows how to connect: where each keeps its list of MCP servers, whether
//! Arach is in it, and adding or removing it there. Also what each assistant last did, as the
//! server notes it (`record_activity`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The name Arach goes by in every app's server list.
pub const SERVER: &str = "arach";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    /// `{"mcpServers": {"arach": {"command", "args"}}}`: Claude, Cursor, Windsurf.
    McpServers,
    /// `{"mcpServers": {"arach": {"type": "stdio", "command", "args"}}}`: Claude Code.
    McpServersTyped,
    /// `{"servers": {"arach": {"type": "stdio", "command", "args"}}}`: VS Code.
    Servers,
    /// `[mcp_servers.arach]` with `command` and `args`, in TOML: Codex.
    CodexToml,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Client {
    pub id: &'static str,
    pub name: &'static str,
    /// What the app needs after Arach is added: picked up at once, or after a restart.
    pub restart: bool,
    format: Format,
}

pub const CLIENTS: [Client; 6] = [
    Client { id: "claude-desktop", name: "Claude Desktop", restart: true, format: Format::McpServers },
    Client { id: "claude-code", name: "Claude Code", restart: true, format: Format::McpServersTyped },
    Client { id: "codex", name: "Codex (OpenAI)", restart: true, format: Format::CodexToml },
    Client { id: "cursor", name: "Cursor", restart: false, format: Format::McpServers },
    Client { id: "vscode", name: "Visual Studio Code", restart: false, format: Format::Servers },
    Client { id: "windsurf", name: "Windsurf", restart: false, format: Format::McpServers },
];

pub fn client(id: &str) -> Option<&'static Client> {
    CLIENTS.iter().find(|c| c.id == id)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// The app doesn't seem to be on this PC.
    Missing,
    /// Installed, Arach not in its list.
    Available,
    Connected,
    /// Arach is listed, but at another path (the exe moved, or another build).
    Elsewhere(String),
    /// Its settings file couldn't be read: left alone.
    Unreadable(String),
}

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default()
}

fn env_dir(var: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_default()
}

impl Client {
    /// The app's MCP settings file, and whether the app looks installed.
    pub fn config(&self) -> (PathBuf, bool) {
        match self.id {
            "claude-desktop" => {
                // The Microsoft Store build keeps %APPDATA% in its package.
                let packaged = std::fs::read_dir(env_dir("LOCALAPPDATA").join("Packages")).ok()
                    .and_then(|r| r.flatten().map(|e| e.path())
                        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("Claude_"))));
                if let Some(pkg) = packaged {
                    return (pkg.join(r"LocalCache\Roaming\Claude\claude_desktop_config.json"), true);
                }
                let dir = env_dir("APPDATA").join("Claude");
                let installed = dir.is_dir() || env_dir("LOCALAPPDATA").join("AnthropicClaude").is_dir();
                (dir.join("claude_desktop_config.json"), installed)
            }
            "claude-code" => {
                let file = home().join(".claude.json");
                let installed = file.is_file() || home().join(".claude").is_dir();
                (file, installed)
            }
            "cursor" => {
                let dir = home().join(".cursor");
                (dir.join("mcp.json"), dir.is_dir())
            }
            "codex" => {
                let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".codex"));
                (dir.join("config.toml"), dir.is_dir())
            }
            "vscode" => {
                let dir = env_dir("APPDATA").join(r"Code\User");
                (dir.join("mcp.json"), dir.is_dir())
            }
            _ => {
                let dir = home().join(r".codeium\windsurf");
                (dir.join("mcp_config.json"), dir.is_dir())
            }
        }
    }

    fn key(&self) -> &'static str {
        match self.format {
            Format::Servers => "servers",
            Format::CodexToml => "mcp_servers",
            _ => "mcpServers",
        }
    }

    /// Arach's entry in this app's list.
    pub fn entry(&self, exe: &Path) -> Value {
        let command = exe.display().to_string();
        match self.format {
            Format::McpServers => json!({ "command": command, "args": ["--mcp"] }),
            _ => json!({ "type": "stdio", "command": command, "args": ["--mcp"] }),
        }
    }

    pub fn status(&self, exe: &Path) -> Status {
        let (path, installed) = self.config();
        status_in(self, &path, installed, exe)
    }

    /// Adds Arach to the app's list (or points it at this exe). Returns the file changed.
    pub fn connect(&self, exe: &Path) -> Result<PathBuf, String> {
        let (path, _) = self.config();
        connect_in(self, &path, exe)?;
        Ok(path)
    }

    pub fn disconnect(&self) -> Result<(), String> {
        disconnect_in(self, &self.config().0)
    }
}

fn status_in(c: &Client, path: &Path, installed: bool, exe: &Path) -> Status {
    if c.format == Format::CodexToml {
        return toml::status(path, installed, exe);
    }
    let doc = match read(path) {
        Ok(Some(doc)) => doc,
        Ok(None) => return if installed { Status::Available } else { Status::Missing },
        Err(e) => return Status::Unreadable(e),
    };
    let Some(entry) = doc.get(c.key()).and_then(|s| s.get(SERVER)) else { return Status::Available };
    let command = entry.get("command").and_then(Value::as_str).unwrap_or_default();
    let mcp = entry.get("args").and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x == "--mcp"));
    if same_path(Path::new(command), exe) && mcp { Status::Connected } else { Status::Elsewhere(command.to_string()) }
}

fn connect_in(c: &Client, path: &Path, exe: &Path) -> Result<(), String> {
    if c.format == Format::CodexToml {
        return toml::connect(path, exe);
    }
    let mut doc = read(path)?.unwrap_or_else(|| Value::Object(Map::new()));
    let root = doc.as_object_mut().ok_or_else(|| format!("{} ne contient pas un objet JSON", path.display()))?;
    let servers = root.entry(c.key()).or_insert_with(|| Value::Object(Map::new()));
    let servers = servers.as_object_mut().ok_or_else(|| format!("« {} » n'est pas un objet dans {}", c.key(), path.display()))?;
    servers.insert(SERVER.to_string(), c.entry(exe));
    write(path, &doc)
}

fn disconnect_in(c: &Client, path: &Path) -> Result<(), String> {
    if c.format == Format::CodexToml {
        return toml::disconnect(path);
    }
    let Some(mut doc) = read(path)? else { return Ok(()) };
    let removed = doc.get_mut(c.key()).and_then(Value::as_object_mut).and_then(|s| s.remove(SERVER)).is_some();
    if removed { write(path, &doc) } else { Ok(()) }
}

/// The file's JSON; `None` when there's no file yet (or an empty one).
fn read(path: &Path) -> Result<Option<Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} illisible : {e}", path.display())),
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() { return Ok(None); }
    serde_json::from_str(text).map(Some)
        .map_err(|e| format!("{} n'est pas du JSON valide ({e}) : Arach n'y touche pas", path.display()))
}

/// Writes beside and renames over, so the app never reads half a file. The first time Arach
/// changes a file, the original is kept next to it (`.arach-backup`).
fn write(path: &Path, doc: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    write_bytes(path, &bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Impossible de créer {} : {e}", dir.display()))?;
    }
    let backup = path.with_extension(format!("{}.arach-backup", path.extension().and_then(|e| e.to_str()).unwrap_or("json")));
    if path.is_file() && !backup.exists() {
        let _ = std::fs::copy(path, &backup);
    }
    let tmp = path.with_extension("arach-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("Impossible d'écrire {} : {e}", path.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Impossible d'écrire {} : {e}", path.display())
    })
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_start_matches(r"\\?\").to_lowercase();
    norm(a) == norm(b)
}

/// The exe assistants should start: this one.
pub fn exe() -> PathBuf {
    std::env::current_exe().ok()
        .map(|p| PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\")))
        .unwrap_or_else(|| PathBuf::from("arach.exe"))
}

/// What to paste into any other MCP app's settings.
pub fn snippet(exe: &Path) -> String {
    let doc = json!({ "mcpServers": { SERVER: CLIENTS[0].entry(exe) } });
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

/// The command line, as a terminal would take it.
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" --mcp", exe.display())
}

/// Codex's `config.toml`, edited in place: its comments and layout stay.
mod toml {
    use std::path::Path;

    use toml_edit::{Array, DocumentMut, Item, Table, value};

    use super::{SERVER, Status, same_path, write_bytes};

    fn read(path: &Path) -> Result<Option<DocumentMut>, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{} illisible : {e}", path.display())),
        };
        text.trim_start_matches('\u{feff}').parse::<DocumentMut>().map(Some)
            .map_err(|e| format!("{} n'est pas du TOML valide ({}) : Arach n'y touche pas", path.display(), e.message()))
    }

    pub fn status(path: &Path, installed: bool, exe: &Path) -> Status {
        let doc = match read(path) {
            Ok(Some(doc)) => doc,
            Ok(None) => return if installed { Status::Available } else { Status::Missing },
            Err(e) => return Status::Unreadable(e),
        };
        let Some(entry) = doc.get("mcp_servers").and_then(|s| s.get(SERVER)) else { return Status::Available };
        let command = entry.get("command").and_then(Item::as_str).unwrap_or_default();
        let mcp = entry.get("args").and_then(Item::as_array).is_some_and(|a| a.iter().any(|x| x.as_str() == Some("--mcp")));
        if same_path(Path::new(command), exe) && mcp { Status::Connected } else { Status::Elsewhere(command.to_string()) }
    }

    pub fn connect(path: &Path, exe: &Path) -> Result<(), String> {
        let mut doc = read(path)?.unwrap_or_default();
        let servers = doc.entry("mcp_servers").or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        });
        let servers = servers.as_table_mut().ok_or_else(|| format!("« mcp_servers » n'est pas une table dans {}", path.display()))?;
        let mut entry = Table::new();
        entry["command"] = value(exe.display().to_string());
        let mut args = Array::new();
        args.push("--mcp");
        entry["args"] = value(args);
        servers.insert(SERVER, Item::Table(entry));
        write_bytes(path, doc.to_string().as_bytes())
    }

    pub fn disconnect(path: &Path) -> Result<(), String> {
        let Some(mut doc) = read(path)? else { return Ok(()) };
        let removed = doc.get_mut("mcp_servers").and_then(Item::as_table_mut).and_then(|t| t.remove(SERVER)).is_some();
        if removed { write_bytes(path, doc.to_string().as_bytes()) } else { Ok(()) }
    }
}

// ---------------------------------------------------------------- activity

/// An assistant's last visit, kept in `%LOCALAPPDATA%\Arach\mcp\<name>.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Activity {
    /// `clientInfo.name`, as the assistant gives it (`claude-ai`, `cursor-vscode`…).
    pub name: String,
    pub version: String,
    pub seen: i64,
    /// The last tool it called.
    pub tool: Option<String>,
}

impl Activity {
    /// Which of `CLIENTS` this is, guessed from the name it gives.
    pub fn client(&self) -> Option<&'static str> {
        let n = self.name.to_lowercase();
        if n.contains("claude-code") || n.contains("claude code") { return Some("claude-code"); }
        if n.contains("codex") { return Some("codex"); }
        if n.contains("claude") { return Some("claude-desktop"); }
        if n.contains("cursor") { return Some("cursor"); }
        if n.contains("windsurf") || n.contains("codeium") { return Some("windsurf"); }
        if n.contains("visual studio code") || n.contains("vscode") || n == "code" { return Some("vscode"); }
        None
    }
}

fn activity_dir() -> PathBuf {
    crate::store::cache_dir().join("mcp")
}

pub fn record_activity(name: &str, version: &str, tool: Option<&str>) {
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' }).take(60).collect();
    if slug.is_empty() { return; }
    let dir = activity_dir();
    let path = dir.join(format!("{slug}.json"));
    let previous: Activity = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let a = Activity {
        name: name.to_string(),
        version: version.to_string(),
        seen: crate::store::now(),
        tool: tool.map(str::to_string).or(previous.tool),
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(bytes) = serde_json::to_vec_pretty(&a) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Every assistant that ever connected, most recent first.
pub fn activity() -> Vec<Activity> {
    let mut v: Vec<Activity> = std::fs::read_dir(activity_dir()).into_iter().flatten().flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .collect();
    v.sort_by_key(|a: &Activity| std::cmp::Reverse(a.seen));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("arach-mcp-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn connects_keeping_the_rest_of_the_file() {
        let dir = temp("keep");
        let path = dir.join("claude_desktop_config.json");
        std::fs::write(&path, r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}},"zeta":1}"#).unwrap();
        let exe = Path::new(r"C:\Games\Arach\arach.exe");
        let c = client("claude-desktop").unwrap();
        assert_eq!(status_in(c, &path, true, exe), Status::Available);
        connect_in(c, &path, exe).unwrap();
        assert_eq!(status_in(c, &path, true, exe), Status::Connected);
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["theme"], "dark");
        assert_eq!(doc["mcpServers"]["other"]["command"], "x");
        assert_eq!(doc["mcpServers"]["arach"]["args"], json!(["--mcp"]));
        // Keys stay where they were.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.find("\"theme\"").unwrap() < text.find("\"zeta\"").unwrap());
        assert!(dir.join("claude_desktop_config.json.arach-backup").is_file());

        let moved = Path::new(r"D:\Arach\arach.exe");
        assert_eq!(status_in(c, &path, true, moved), Status::Elsewhere(exe.display().to_string()));
        disconnect_in(c, &path).unwrap();
        assert_eq!(status_in(c, &path, true, exe), Status::Available);
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["mcpServers"]["other"]["command"], "x");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_a_missing_file_and_leaves_broken_ones() {
        let dir = temp("new");
        let exe = Path::new(r"C:\Arach\arach.exe");
        let vscode = client("vscode").unwrap();
        let path = dir.join("User").join("mcp.json");
        assert_eq!(status_in(vscode, &path, false, exe), Status::Missing);
        connect_in(vscode, &path, exe).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["servers"]["arach"]["type"], "stdio");
        assert_eq!(status_in(vscode, &path, false, exe), Status::Connected);

        let broken = dir.join("broken.json");
        std::fs::write(&broken, "{ // a comment\n }").unwrap();
        assert!(matches!(status_in(vscode, &broken, true, exe), Status::Unreadable(_)));
        assert!(connect_in(vscode, &broken, exe).is_err());
        assert_eq!(std::fs::read_to_string(&broken).unwrap(), "{ // a comment\n }");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edits_codex_toml_keeping_comments() {
        let dir = temp("codex");
        let path = dir.join("config.toml");
        let before = "# my settings\nmodel = \"gpt-5\"\n\n[mcp_servers.other]\ncommand = \"x\"\n";
        std::fs::write(&path, before).unwrap();
        let exe = Path::new(r"C:\Games\Arach\arach.exe");
        let codex = client("codex").unwrap();
        assert_eq!(status_in(codex, &path, true, exe), Status::Available);
        connect_in(codex, &path, exe).unwrap();
        assert_eq!(status_in(codex, &path, true, exe), Status::Connected);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# my settings\nmodel = \"gpt-5\""), "{text}");
        assert!(text.contains("[mcp_servers.arach]") && text.contains("[mcp_servers.other]"), "{text}");
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(doc["mcp_servers"]["arach"]["command"].as_str(), Some(r"C:\Games\Arach\arach.exe"));
        disconnect_in(codex, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("arach") && text.contains("[mcp_servers.other]"), "{text}");
        // A file Codex hasn't written yet.
        let fresh = dir.join("new").join("config.toml");
        connect_in(codex, &fresh, exe).unwrap();
        assert_eq!(status_in(codex, &fresh, true, exe), Status::Connected);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn knows_assistants_by_the_name_they_give() {
        let by = |n: &str| Activity { name: n.into(), ..Default::default() }.client();
        assert_eq!(by("claude-ai"), Some("claude-desktop"));
        assert_eq!(by("claude-code"), Some("claude-code"));
        assert_eq!(by("cursor-vscode"), Some("cursor"));
        assert_eq!(by("Visual Studio Code"), Some("vscode"));
        assert_eq!(by("windsurf-client"), Some("windsurf"));
        assert_eq!(by("codex-mcp-client"), Some("codex"));
        assert_eq!(by("mcp-inspector"), None);
    }
}
