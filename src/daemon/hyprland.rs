use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

/// Returns the connector name of the currently active Hyprland monitor
/// (e.g. "DP-1", "HDMI-A-1"), or `None` if Hyprland IPC is unavailable.
pub fn active_monitor_name() -> Option<String> {
    let path = hyprland_socket_path()?;
    match query_active_monitor(&path) {
        Ok(name) => Some(name),
        Err(e) => {
            eprintln!("cooee: hyprland monitor query failed: {}", e);
            None
        }
    }
}

fn hyprland_socket_path() -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    Some(PathBuf::from(runtime).join("hypr").join(&sig).join(".socket.sock"))
}

fn query_active_monitor(path: &PathBuf) -> Result<String> {
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("connecting to Hyprland socket {:?}", path))?;
    stream.write_all(b"j/activeworkspace").context("sending activeworkspace request")?;
    let mut response = String::new();
    stream.read_to_string(&mut response).context("reading Hyprland response")?;
    parse_monitor_from_workspace_json(&response)
}

fn parse_monitor_from_workspace_json(json: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .context("parsing Hyprland workspace JSON")?;
    value["monitor"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("'monitor' field missing from Hyprland response"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_parse_monitor_from_workspace_json() {
        let json = r#"{"id":1,"name":"1","monitor":"DP-1","windows":2}"#;
        let name = parse_monitor_from_workspace_json(json).unwrap();
        assert_eq!(name, "DP-1");
    }

    #[test]
    fn test_parse_monitor_missing_field_returns_error() {
        let json = r#"{"id":1,"name":"1"}"#;
        let result = parse_monitor_from_workspace_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_hyprland_socket_path_construction() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "abc123");
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        let path = hyprland_socket_path().unwrap();
        assert_eq!(path.to_str().unwrap(), "/run/user/1000/hypr/abc123/.socket.sock");
    }

    #[test]
    fn test_active_monitor_name_no_hyprland_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        assert!(active_monitor_name().is_none());
    }
}
