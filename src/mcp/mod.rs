//! `arach --mcp`: a Model Context Protocol server on stdin/stdout, which AI assistants (Claude,
//! Cursor, VS Code, Codex…) start to read the library, launch games and change them.
//!
//! Each message is one line of JSON-RPC 2.0. The library is read from Arach's own files at every
//! call, so it is as fresh as the window's last save. Games are launched, and anything changed,
//! through the window (`bridge`): the session is measured and the actions before and after it
//! run as for a game started from Arach itself, and no change is lost to the window's next save.
//!
//! Nothing may be written to stdout but replies: it is the protocol's channel.

pub mod bridge;
pub mod clients;
mod tools;

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::store;

/// Protocol revisions this server speaks, newest first.
const PROTOCOLS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// What the assistant is told about Arach when it connects.
const INSTRUCTIONS: &str = "Arach is the user's game launcher on Windows: it lists every game installed from \
Steam, Epic, GOG, Xbox, Battle.net, EA, Ubisoft, Riot and more, plus console games played with emulators. \
Use list_games to find games (fuzzy search by name, filters by store, collection, status), get_game for the \
details of one, library_overview for a summary of the whole library, and launch_game to start one. edit_game \
changes a game's facts (name, description, genres…), status, rating, notes, favorite and collections; find_images \
and set_image change its cover, background or logo; get_launch_actions and set_launch_actions read and change what \
Arach does before and after a game (close Discord, power plan, screen resolution, scripts), for every game or one. \
Game ids look like `steam:1091500` or `epic:Fortnite`; always take them from list_games rather than guessing. \
Only change what the user asked for. The user may have turned some changes off: the error then says so.";

/// Runs until the assistant closes stdin. Returns the process exit code.
pub fn serve() -> i32 {
    keep_pipes_to_ourselves();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut session = Session::default();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.into_iter().filter_map(|m| session.handle(m)).collect();
                (!replies.is_empty()).then(|| Value::Array(replies))
            }
            Ok(msg) => session.handle(msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("Parse error: {e}"))),
        };
        if let Some(reply) = reply {
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() { break; }
        }
    }
    0
}

/// Stops the assistant's pipes from being handed down to the processes this one starts (the
/// window, through `bridge`). Windows gives a child every inheritable handle, whatever its own
/// stdin and stdout are: a window started here would hold the assistant's pipe open, and the
/// assistant would never see this server end.
fn keep_pipes_to_ourselves() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation};
        use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
        for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            // SAFETY: plain calls on this process's own standard handles; a missing one is skipped.
            unsafe {
                let h = GetStdHandle(which);
                if !h.is_null() && h != INVALID_HANDLE_VALUE {
                    SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
                }
            }
        }
    }
}

/// One assistant's connection.
#[derive(Default)]
struct Session {
    /// `clientInfo.name` from `initialize`.
    client: Option<String>,
    version: String,
}

impl Session {
    /// The reply to one message; `None` for notifications and replies the client sent us.
    fn handle(&mut self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            // A reply to a request we never make, or garbage with an id.
            return id.filter(|_| msg.get("result").is_none() && msg.get("error").is_none())
                .map(|id| error(id, -32600, "Invalid request"));
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let id = id?; // notifications (`notifications/initialized`, `…/cancelled`) need no answer
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::list() })),
            "tools/call" => self.call(&params),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err((-32601, format!("Method not found: {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let info = params.get("clientInfo");
        self.client = info.and_then(|i| i.get("name")).and_then(Value::as_str).map(str::to_string);
        self.version = info.and_then(|i| i.get("version")).and_then(Value::as_str).unwrap_or_default().to_string();
        self.seen(None);
        let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
        let protocol = PROTOCOLS.iter().find(|p| **p == asked).copied().unwrap_or(PROTOCOLS[0]);
        json!({
            "protocolVersion": protocol,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "arach", "title": "Arach", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })
    }

    fn call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).ok_or((-32602, "Missing tool name".to_string()))?;
        if !tools::exists(name) {
            return Err((-32602, format!("Unknown tool: {name}")));
        }
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        self.seen(Some(name));
        let settings = store::load_settings();
        let outcome = if settings.mcp.enabled {
            tools::call(name, &args, &settings)
        } else {
            Err("The user turned off AI assistant access in Arach (Paramètres › Assistants IA).".to_string())
        };
        Ok(match outcome {
            Ok(value) => json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
                "structuredContent": value,
                "isError": false,
            }),
            Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
        })
    }

    /// Notes that this assistant was here, for Paramètres › Assistants IA.
    fn seen(&self, tool: Option<&str>) {
        let Some(name) = &self.client else { return };
        clients::record_activity(name, &self.version, tool);
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(s: &mut Session, msg: Value) -> Value {
        s.handle(msg).expect("a reply")
    }

    #[test]
    fn handshake_and_tool_list() {
        let mut s = Session::default();
        let init = ask(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{}}}));
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "arach");
        // An unknown revision gets ours.
        let mut other = Session::default();
        let init = ask(&mut other, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}));
        assert_eq!(init["result"]["protocolVersion"], PROTOCOLS[0]);

        assert!(s.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let list = ask(&mut s, json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["list_games", "get_game", "library_overview", "launch_game", "edit_game",
            "find_images", "set_image", "get_launch_actions", "set_launch_actions"]);
        for t in list["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
        }
        let missing = ask(&mut s, json!({"jsonrpc":"2.0","id":3,"method":"nope"}));
        assert_eq!(missing["error"]["code"], -32601);
        let unknown = ask(&mut s, json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope"}}));
        assert_eq!(unknown["error"]["code"], -32602);
    }
}
