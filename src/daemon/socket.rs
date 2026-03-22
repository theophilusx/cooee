use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Commands the client can send to the daemon
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Speak,
    Dnd { mode: String },
    Dismiss,
    Action,
    Status,
}

/// Responses the daemon sends back
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl Response {
    pub fn ok() -> Self { Self { ok: true, dnd: None, error: None, status: None } }
    pub fn ok_dnd(mode: &str) -> Self { Self { ok: true, dnd: Some(mode.to_string()), error: None, status: None } }
    pub fn err(msg: &str) -> Self { Self { ok: false, dnd: None, error: Some(msg.to_string()), status: None } }
    pub fn ok_status(s: &str) -> Self { Self { ok: true, dnd: None, error: None, status: Some(s.to_string()) } }
}

pub fn socket_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime).join("cooee.sock")
}

/// Write a `Response` to a `UnixStream` as a newline-terminated JSON line
pub async fn write_response(stream: &mut UnixStream, response: &Response) -> Result<()> {
    let mut line = serde_json::to_string(response)?;
    line.push('\n');
    stream.write_all(line.as_bytes()).await?;
    Ok(())
}

/// Read a `Command` from a `UnixStream` (reads one newline-terminated JSON line)
pub async fn read_command(stream: &mut UnixStream) -> Result<Command> {
    let mut reader = BufReader::new(&mut *stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let cmd = serde_json::from_str(line.trim())?;
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_command_serialise_speak() {
        let cmd = Command::Speak;
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(json, r#"{"cmd":"speak"}"#);
    }

    #[test]
    fn test_command_deserialise_speak() {
        let cmd: Command = serde_json::from_str(r#"{"cmd":"speak"}"#).unwrap();
        assert_eq!(cmd, Command::Speak);
    }

    #[test]
    fn test_command_serialise_dnd() {
        let cmd = Command::Dnd { mode: "silent".to_string() };
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(json, r#"{"cmd":"dnd","mode":"silent"}"#);
    }

    #[test]
    fn test_command_deserialise_dnd() {
        let cmd: Command = serde_json::from_str(r#"{"cmd":"dnd","mode":"full"}"#).unwrap();
        assert_eq!(cmd, Command::Dnd { mode: "full".to_string() });
    }

    #[test]
    fn test_command_serialise_action() {
        let cmd = Command::Action;
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(json, r#"{"cmd":"action"}"#);
    }

    #[test]
    fn test_command_deserialise_action() {
        let cmd: Command = serde_json::from_str(r#"{"cmd":"action"}"#).unwrap();
        assert_eq!(cmd, Command::Action);
    }

    #[test]
    fn test_response_ok_serialise() {
        let r = Response::ok();
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#"{"ok":true}"#);
    }

    #[test]
    fn test_response_err_serialise() {
        let r = Response::err("no notification");
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("\"error\":\"no notification\""));
    }

    #[test]
    fn test_socket_path_uses_xdg_runtime_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        let p = socket_path();
        assert_eq!(p.to_str().unwrap(), "/run/user/1000/cooee.sock");
    }
}
