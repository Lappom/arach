//! Discord Rich Presence: "Joue à Hollow Knight" on the user's profile for every game Arach
//! launches, whatever its store. Spoken over Discord's local pipe (`\\.\pipe\discord-ipc-N`):
//! each frame is an opcode and a length (little-endian u32s), then JSON.
//!
//! Discord shows the activity under an application's name; `status_display_type: 2` asks it
//! to show the details line (the game's name) instead, so the profile reads the game, not
//! "Arach". The application is Arach's own (`settings::DISCORD_APP_ID`).

use std::io::{Read, Write};
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};

use serde_json::{Value, json};

use crate::settings::DISCORD_APP_ID;

pub struct Activity {
    pub game: String,
    /// "Steam", "Nintendo 3DS"…; empty for a game added by hand.
    pub store: String,
    /// Unix seconds: Discord counts the time played from here.
    pub start: i64,
    /// A picture's https address, shown beside the name.
    pub image: Option<String>,
}

enum Msg {
    Set(Activity),
    Clear,
}

/// Shows `a` on the user's Discord profile, replacing whatever Arach showed before.
pub fn set(a: Activity) {
    send(Msg::Set(a));
}

/// Takes the activity off the profile.
pub fn clear() {
    send(Msg::Clear);
}

/// One thread owns the pipe: Discord keeps the activity while it stays open.
fn send(m: Msg) {
    static TX: OnceLock<Sender<Msg>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = channel::<Msg>();
        std::thread::spawn(move || {
            let mut conn: Option<Pipe> = None;
            for m in rx {
                match m {
                    Msg::Set(a) => {
                        let frame = activity_frame(&a);
                        // A pipe left over from a Discord that restarted fails once; try anew.
                        for _ in 0..2 {
                            if conn.is_none() { conn = Pipe::connect(DISCORD_APP_ID); }
                            let Some(pipe) = &mut conn else { break };
                            if pipe.call(&frame).is_ok() { break; }
                            conn = None;
                        }
                    }
                    // Closing the pipe clears the activity.
                    Msg::Clear => conn = None,
                }
            }
        });
        tx
    });
    let _ = tx.send(m);
}

fn activity_frame(a: &Activity) -> Value {
    let mut activity = json!({
        "type": 0,
        "details": clip(&a.game),
        "status_display_type": 2,
        "timestamps": { "start": a.start },
    });
    if !a.store.is_empty() {
        activity["state"] = json!(clip(&format!("Sur {}", a.store)));
    }
    if let Some(url) = &a.image {
        activity["assets"] = json!({ "large_image": url, "large_text": clip(&a.game) });
    }
    json!({
        "cmd": "SET_ACTIVITY",
        "nonce": format!("{}", crate::store::now()),
        "args": { "pid": std::process::id(), "activity": activity },
    })
}

/// Discord refuses fields over 128 characters.
fn clip(s: &str) -> String {
    s.chars().take(128).collect()
}

struct Pipe(std::fs::File);

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;

impl Pipe {
    /// Discord listens on the first free pipe of ten.
    fn connect(app_id: &str) -> Option<Pipe> {
        for i in 0..10 {
            let path = if cfg!(windows) {
                format!(r"\\.\pipe\discord-ipc-{i}")
            } else {
                let dir = std::env::var("XDG_RUNTIME_DIR").or_else(|_| std::env::var("TMPDIR")).unwrap_or_else(|_| "/tmp".into());
                format!("{dir}/discord-ipc-{i}")
            };
            let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(&path) else { continue };
            let mut pipe = Pipe(file);
            if pipe.write(HANDSHAKE, &json!({ "v": 1, "client_id": app_id })).is_ok()
                && pipe.read().is_ok_and(|(op, _)| op == FRAME)
            {
                return Some(pipe);
            }
        }
        None
    }

    /// Sends a command and waits for Discord's answer, so frames never pile up unread.
    fn call(&mut self, v: &Value) -> std::io::Result<()> {
        self.write(FRAME, v)?;
        match self.read()? {
            (CLOSE, _) => Err(std::io::ErrorKind::ConnectionAborted.into()),
            _ => Ok(()),
        }
    }

    fn write(&mut self, op: u32, v: &Value) -> std::io::Result<()> {
        let body = serde_json::to_vec(v)?;
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        self.0.write_all(&frame)
    }

    fn read(&mut self) -> std::io::Result<(u32, Value)> {
        let mut head = [0u8; 8];
        self.0.read_exact(&mut head)?;
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if len > 1 << 20 { return Err(std::io::ErrorKind::InvalidData.into()); }
        let mut body = vec![0u8; len];
        self.0.read_exact(&mut body)?;
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_names_the_game() {
        let f = activity_frame(&Activity {
            game: "Hollow Knight".into(),
            store: "GOG".into(),
            start: 1_700_000_000,
            image: None,
        });
        let a = &f["args"]["activity"];
        assert_eq!(a["details"], "Hollow Knight");
        assert_eq!(a["state"], "Sur GOG");
        assert_eq!(a["status_display_type"], 2);
        assert!(a.get("assets").is_none());
    }
}
