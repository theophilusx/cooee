# cooee Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Wayland desktop notification daemon in Rust that implements the freedesktop.org notifications spec with GTK4 card popups, sound, TTS, DND modes, and notification action support.

**Architecture:** A single binary (`cooee`) runs in two modes — `daemon` (long-running GTK4 + tokio process) and `client` (connects to Unix socket, sends command, exits). The GTK4 main loop runs on the main thread; D-Bus and socket handling run on a tokio async runtime bridged to GTK via `glib::MainContext` channels. Shared runtime state is held in `Arc<Mutex<AppState>>`.

**Tech Stack:** Rust 2021, gtk4 + gtk4-layer-shell, zbus (D-Bus), tokio, rodio (sound), speech-dispatcher (TTS), clap (CLI), serde + toml (config), serde_json (socket protocol), notify (config file watching)

---

## File Map

| File | Responsibility |
|---|---|
| `Cargo.toml` | Manifest, all dependencies |
| `src/main.rs` | CLI entry point (clap), dispatches to daemon or client |
| `src/config.rs` | Config structs, TOML loading, default generation, file watching |
| `src/notification.rs` | `Notification` struct, `Urgency` enum, action list types |
| `src/daemon/mod.rs` | Daemon startup: wires tokio runtime, GTK loop, D-Bus, socket, state |
| `src/daemon/state.rs` | `AppState`: DND mode, last notification, `Arc<Mutex<>>` wrapper |
| `src/daemon/dbus.rs` | `zbus` server implementing `org.freedesktop.Notifications` |
| `src/daemon/socket.rs` | Unix socket server: accept connections, parse commands, dispatch |
| `src/daemon/sound.rs` | `rodio` playback: load file, play async |
| `src/daemon/tts.rs` | `speech-dispatcher` client: speak summary and full body |
| `src/daemon/ui.rs` | GTK4 window management + `gtk4-layer-shell` positioning, stacking |
| `src/daemon/hyprland.rs` | Query Hyprland IPC socket for active monitor name |
| `src/client.rs` | Connect to socket, send command JSON, print response, return exit code |

---

## Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs` (empty re-export root for test access)

- [ ] **Step 1: Initialise the Cargo project**

```bash
cd /home/tim/Projects/cooee
cargo init --name cooee
```

Expected: `src/main.rs` created, `Cargo.toml` created with `name = "cooee"`.

- [ ] **Step 2: Replace `Cargo.toml` with full dependency manifest**

```toml
[package]
name = "cooee"
version = "0.1.0"
edition = "2021"
license = "GPL-3.0"
repository = "https://github.com/theophilusx/cooee"
description = "Wayland desktop notification daemon with sound and TTS"

[dependencies]
# UI / Wayland
gtk4 = { version = "0.9", features = ["v4_12"] }
gtk4-layer-shell = "0.8"

# D-Bus
zbus = { version = "4", features = ["tokio"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# Sound
rodio = { version = "0.19", default-features = false, features = ["wav", "vorbis", "mp3"] }

# TTS
speech-dispatcher = "0.6"

# CLI
clap = { version = "4", features = ["derive"] }

# Config
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# Socket protocol
serde_json = "1"

# Error handling
anyhow = "1"
thiserror = "1"

# Config file watching
notify = "6"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Write `src/main.rs` with clap CLI skeleton**

```rust
use clap::{Parser, Subcommand};

mod config;
mod notification;
mod daemon;
mod client;

#[derive(Parser)]
#[command(name = "cooee", version, about = "Wayland notification daemon")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the notification daemon
    Daemon,
    /// Speak the full body of the last notification
    Speak,
    /// Get or set Do Not Disturb mode
    Dnd {
        /// Mode: off, silent, full, toggle
        mode: Option<String>,
    },
    /// Dismiss the most recently received popup
    Dismiss,
    /// Open picker to invoke an action on the last notification
    Action,
    /// Print daemon status
    Status,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon => daemon::run(),
        cmd => client::run(cmd),
    }
}
```

- [ ] **Step 4: Create stub modules so it compiles**

Create `src/config.rs`:
```rust
pub struct Config;
```

Create `src/notification.rs`:
```rust
pub struct Notification;
```

Create `src/daemon/mod.rs`:
```rust
pub fn run() -> anyhow::Result<()> { Ok(()) }
```

Create `src/client.rs`:
```rust
use crate::Command;
pub fn run(_cmd: Command) -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo build 2>&1
```

Expected: Compiles with no errors (warnings about unused stubs are fine).

- [ ] **Step 6: Commit**

```bash
git init
git add Cargo.toml src/
git commit -m "feat: scaffold cooee project with clap CLI"
```

---

## Task 2: Config Module

**Files:**
- Modify: `src/config.rs`
- Test: inline `#[cfg(test)]` in `src/config.rs`

- [ ] **Step 1: Write failing tests for config**

Replace `src/config.rs` with:

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
    Center,
    CenterTop,
    CenterBottom,
}

impl Default for Position {
    fn default() -> Self { Position::TopRight }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DndMode {
    Off,
    Silent,
    Full,
}

impl Default for DndMode {
    fn default() -> Self { DndMode::Off }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "GeneralConfig::default_position")]
    pub position: Position,
    #[serde(default = "GeneralConfig::default_margin_x")]
    pub margin_x: i32,
    #[serde(default = "GeneralConfig::default_margin_y")]
    pub margin_y: i32,
    #[serde(default = "GeneralConfig::default_max_visible")]
    pub max_visible: usize,
    #[serde(default = "GeneralConfig::default_timeout")]
    pub timeout: u32,
    #[serde(default = "GeneralConfig::default_icon_size")]
    pub icon_size: i32,
    #[serde(default = "GeneralConfig::default_width")]
    pub width: i32,
}

impl GeneralConfig {
    fn default_position() -> Position { Position::TopRight }
    fn default_margin_x() -> i32 { 16 }
    fn default_margin_y() -> i32 { 48 }
    fn default_max_visible() -> usize { 5 }
    fn default_timeout() -> u32 { 5000 }
    fn default_icon_size() -> i32 { 36 }
    fn default_width() -> i32 { 360 }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            position: Self::default_position(),
            margin_x: Self::default_margin_x(),
            margin_y: Self::default_margin_y(),
            max_visible: Self::default_max_visible(),
            timeout: Self::default_timeout(),
            icon_size: Self::default_icon_size(),
            width: Self::default_width(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SoundConfig {
    #[serde(default = "SoundConfig::default_enabled")]
    pub enabled: bool,
    #[serde(default = "SoundConfig::default_file")]
    pub file: String,
    #[serde(default = "SoundConfig::default_volume")]
    pub volume: f32,
}

impl SoundConfig {
    fn default_enabled() -> bool { true }
    fn default_file() -> String {
        "~/.config/cooee/sounds/notify.ogg".to_string()
    }
    fn default_volume() -> f32 { 0.8 }
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            file: Self::default_file(),
            volume: Self::default_volume(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TtsConfig {
    #[serde(default = "TtsConfig::default_enabled")]
    pub enabled: bool,
    #[serde(default = "TtsConfig::default_speak_summary")]
    pub speak_summary: bool,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub rate: i32,
}

impl TtsConfig {
    fn default_enabled() -> bool { true }
    fn default_speak_summary() -> bool { true }
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            speak_summary: Self::default_speak_summary(),
            voice: String::new(),
            rate: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DndConfig {
    #[serde(default)]
    pub mode: DndMode,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionsConfig {
    #[serde(default = "ActionsConfig::default_picker")]
    pub picker: String,
}

impl ActionsConfig {
    fn default_picker() -> String {
        "rofi -dmenu -p 'Action:'".to_string()
    }
}

impl Default for ActionsConfig {
    fn default() -> Self {
        Self { picker: Self::default_picker() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub sound: SoundConfig,
    #[serde(default)]
    pub tts: TtsConfig,
    #[serde(default)]
    pub dnd: DndConfig,
    #[serde(default)]
    pub actions: ActionsConfig,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            let cfg = Config::default();
            cfg.write_default(&path)?;
            return Ok(cfg);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config {:?}", path))?;
        toml::from_str(&text).with_context(|| "parsing config TOML")
    }

    fn write_default(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Expand a `~` prefix to the home directory.
    pub fn expand_path(path: &str) -> PathBuf {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = dirs_next_home() {
                return home.join(rest);
            }
        }
        PathBuf::from(path)
    }
}

fn dirs_next_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

pub fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_next_home()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        });
    base.join("cooee").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn with_config_dir(content: &str) -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, content).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path().parent().unwrap());
        // We load directly from the file here to avoid XDG path complexities
        let cfg: Config = toml::from_str(content).unwrap();
        (dir, cfg)
    }

    #[test]
    fn test_default_config_has_expected_values() {
        let cfg = Config::default();
        assert_eq!(cfg.general.margin_x, 16);
        assert_eq!(cfg.general.margin_y, 48);
        assert_eq!(cfg.general.max_visible, 5);
        assert_eq!(cfg.general.timeout, 5000);
        assert_eq!(cfg.general.width, 360);
        assert_eq!(cfg.general.icon_size, 36);
        assert!(cfg.sound.enabled);
        assert!((cfg.sound.volume - 0.8).abs() < 0.001);
        assert!(cfg.tts.enabled);
        assert!(cfg.tts.speak_summary);
        assert_eq!(cfg.tts.rate, 0);
        assert_eq!(cfg.actions.picker, "rofi -dmenu -p 'Action:'");
    }

    #[test]
    fn test_partial_toml_uses_defaults() {
        let toml = r#"
[general]
margin_x = 32
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.general.margin_x, 32);
        assert_eq!(cfg.general.margin_y, 48); // default
        assert_eq!(cfg.general.width, 360);   // default
    }

    #[test]
    fn test_expand_path_replaces_tilde() {
        std::env::set_var("HOME", "/home/testuser");
        let p = Config::expand_path("~/.config/cooee/sounds/notify.ogg");
        assert_eq!(p.to_str().unwrap(), "/home/testuser/.config/cooee/sounds/notify.ogg");
    }

    #[test]
    fn test_expand_path_passthrough_absolute() {
        let p = Config::expand_path("/absolute/path");
        assert_eq!(p.to_str().unwrap(), "/absolute/path");
    }

    #[test]
    fn test_write_default_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub").join("config.toml");
        let cfg = Config::default();
        cfg.write_default(&path).unwrap();
        assert!(path.exists());
        let text = fs::read_to_string(&path).unwrap();
        let reparsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(reparsed.general.width, 360);
    }
}
```

- [ ] **Step 2: Run tests — expect failure (missing `dirs_next_home` is a stub, should compile)**

```bash
cargo test config:: 2>&1
```

Expected: All 5 tests pass. If `toml` or `serde` version issues arise, adjust `Cargo.toml` versions accordingly.

- [ ] **Step 3: Commit**

```bash
git add src/config.rs Cargo.toml Cargo.lock
git commit -m "feat: add config module with TOML loading and XDG path support"
```

---

## Task 3: Notification Struct

**Files:**
- Modify: `src/notification.rs`
- Test: inline `#[cfg(test)]` in `src/notification.rs`

- [ ] **Step 1: Write failing tests**

```bash
# Add to src/notification.rs — tests before implementation
```

- [ ] **Step 2: Write full `src/notification.rs`**

```rust
/// Urgency level per freedesktop.org spec
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low = 0,
    Normal = 1,
    Critical = 2,
}

impl From<u8> for Urgency {
    fn from(v: u8) -> Self {
        match v {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

/// A single action (key + display label) provided by the sending app
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// Parses the flat `[key, label, key, label, ...]` list from D-Bus into `Vec<Action>`
pub fn parse_actions(flat: &[String]) -> Vec<Action> {
    flat.chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                Some(Action {
                    key: chunk[0].clone(),
                    label: chunk[1].clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// A received desktop notification
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<Action>,
    pub urgency: Urgency,
    /// Expiry in milliseconds; 0 = server default, -1 = persistent
    pub expire_timeout: i32,
    /// Raw image data from hints (optional)
    pub image_data: Option<Vec<u8>>,
}

impl Notification {
    pub fn new(
        id: u32,
        app_name: String,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        urgency: u8,
        expire_timeout: i32,
        image_data: Option<Vec<u8>>,
    ) -> Self {
        Self {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions: parse_actions(&actions),
            urgency: Urgency::from(urgency),
            expire_timeout,
            image_data,
        }
    }

    /// Effective display duration in ms. Returns `default_ms` when expire_timeout == 0,
    /// and `None` (persistent) when expire_timeout == -1.
    pub fn display_duration_ms(&self, default_ms: u32) -> Option<u32> {
        match self.expire_timeout {
            -1 => None,
            0 => Some(default_ms),
            n => Some(n as u32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_actions_even_pairs() {
        let flat = vec![
            "default".to_string(), "Open".to_string(),
            "snooze".to_string(), "Snooze 5 min".to_string(),
        ];
        let actions = parse_actions(&flat);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].key, "default");
        assert_eq!(actions[0].label, "Open");
        assert_eq!(actions[1].key, "snooze");
        assert_eq!(actions[1].label, "Snooze 5 min");
    }

    #[test]
    fn test_parse_actions_odd_list_ignores_trailing() {
        let flat = vec!["orphan".to_string()];
        let actions = parse_actions(&flat);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_parse_actions_empty() {
        let actions = parse_actions(&[]);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_urgency_from_u8() {
        assert_eq!(Urgency::from(0), Urgency::Low);
        assert_eq!(Urgency::from(1), Urgency::Normal);
        assert_eq!(Urgency::from(2), Urgency::Critical);
        assert_eq!(Urgency::from(99), Urgency::Normal); // unknown → Normal
    }

    #[test]
    fn test_display_duration_default() {
        let n = make_notification(0);
        assert_eq!(n.display_duration_ms(5000), Some(5000));
    }

    #[test]
    fn test_display_duration_persistent() {
        let n = make_notification(-1);
        assert_eq!(n.display_duration_ms(5000), None);
    }

    #[test]
    fn test_display_duration_explicit() {
        let n = make_notification(3000);
        assert_eq!(n.display_duration_ms(5000), Some(3000));
    }

    fn make_notification(expire_timeout: i32) -> Notification {
        Notification::new(
            1, "app".into(), "".into(), "Summary".into(), "Body".into(),
            vec![], 1, expire_timeout, None,
        )
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test notification:: 2>&1
```

Expected: 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/notification.rs
git commit -m "feat: add Notification struct with action parsing and urgency"
```

---

## Task 4: App State

**Files:**
- Create: `src/daemon/state.rs`
- Test: inline `#[cfg(test)]` in `src/daemon/state.rs`

- [ ] **Step 1: Write `src/daemon/state.rs`**

```rust
use std::sync::{Arc, Mutex};
use crate::config::DndMode;
use crate::notification::Notification;

#[derive(Debug)]
pub struct AppState {
    pub dnd_mode: DndMode,
    pub last_notification: Option<Notification>,
    pub next_id: u32,
}

impl AppState {
    pub fn new(initial_dnd: DndMode) -> Self {
        Self {
            dnd_mode: initial_dnd,
            last_notification: None,
            next_id: 1,
        }
    }

    pub fn next_notification_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn set_dnd(&mut self, mode: DndMode) {
        self.dnd_mode = mode;
    }

    /// Cycle: off → silent → full → off
    pub fn toggle_dnd(&mut self) {
        self.dnd_mode = match self.dnd_mode {
            DndMode::Off => DndMode::Silent,
            DndMode::Silent => DndMode::Full,
            DndMode::Full => DndMode::Off,
        };
    }

    pub fn dnd_mode_str(&self) -> &'static str {
        match self.dnd_mode {
            DndMode::Off => "off",
            DndMode::Silent => "silent",
            DndMode::Full => "full",
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn new_shared_state(initial_dnd: DndMode) -> SharedState {
    Arc::new(Mutex::new(AppState::new(initial_dnd)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_id_increments() {
        let mut state = AppState::new(DndMode::Off);
        assert_eq!(state.next_notification_id(), 1);
        assert_eq!(state.next_notification_id(), 2);
        assert_eq!(state.next_notification_id(), 3);
    }

    #[test]
    fn test_toggle_dnd_cycles() {
        let mut state = AppState::new(DndMode::Off);
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "silent");
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "full");
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "off");
    }

    #[test]
    fn test_set_dnd() {
        let mut state = AppState::new(DndMode::Off);
        state.set_dnd(DndMode::Full);
        assert_eq!(state.dnd_mode_str(), "full");
    }

    #[test]
    fn test_shared_state_mutex() {
        let shared = new_shared_state(DndMode::Off);
        {
            let mut s = shared.lock().unwrap();
            s.toggle_dnd();
        }
        let s = shared.lock().unwrap();
        assert_eq!(s.dnd_mode_str(), "silent");
    }
}
```

- [ ] **Step 2: Add module declaration to `src/daemon/mod.rs`**

```rust
pub mod state;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 3: Run tests**

```bash
cargo test daemon::state:: 2>&1
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/daemon/state.rs src/daemon/mod.rs
git commit -m "feat: add AppState with DND mode management"
```

---

## Task 5: Unix Socket Protocol

**Files:**
- Create: `src/daemon/socket.rs`
- Create: `src/client.rs` (replace stub)
- Test: inline in `src/daemon/socket.rs`

- [ ] **Step 1: Write `src/daemon/socket.rs`**

This module defines the message types and handles the server side of the socket.

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

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
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let cmd = serde_json::from_str(line.trim())?;
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        let p = socket_path();
        assert_eq!(p.to_str().unwrap(), "/run/user/1000/cooee.sock");
    }
}
```

- [ ] **Step 2: Replace stub `src/client.rs`**

```rust
use anyhow::{bail, Result};
use serde_json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use crate::daemon::socket::{Command, socket_path};

pub fn run(cmd: crate::Command) -> Result<()> {
    let socket_cmd = translate_command(cmd)?;
    let path = socket_path();
    let mut stream = UnixStream::connect(&path)
        .map_err(|_| anyhow::anyhow!("cooee daemon is not running (could not connect to {:?})", path))?;

    let mut line = serde_json::to_string(&socket_cmd)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;

    let mut reader = BufReader::new(&stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: serde_json::Value = serde_json::from_str(response_line.trim())?;
    if let Some(false) = response["ok"].as_bool() {
        let err = response["error"].as_str().unwrap_or("unknown error");
        eprintln!("cooee: {}", err);
        std::process::exit(1);
    }
    if let Some(dnd) = response["dnd"].as_str() {
        println!("DND mode: {}", dnd);
    }
    if let Some(status) = response["status"].as_str() {
        println!("{}", status);
    }
    Ok(())
}

fn translate_command(cmd: crate::Command) -> Result<Command> {
    Ok(match cmd {
        crate::Command::Speak => Command::Speak,
        crate::Command::Dnd { mode } => {
            let m = mode.unwrap_or_else(|| "status".to_string());
            Command::Dnd { mode: m }
        },
        crate::Command::Dismiss => Command::Dismiss,
        crate::Command::Action => Command::Action,
        crate::Command::Status => Command::Status,
        crate::Command::Daemon => bail!("translate_command called with Daemon variant"),
    })
}
```

- [ ] **Step 3: Update `src/daemon/mod.rs` to declare socket module**

```rust
pub mod state;
pub mod socket;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 4: Run tests**

```bash
cargo test daemon::socket:: 2>&1
```

Expected: 8 tests pass.

- [ ] **Step 5: Compile check**

```bash
cargo build 2>&1
```

Expected: Compiles without errors.

- [ ] **Step 6: Commit**

```bash
git add src/daemon/socket.rs src/client.rs src/daemon/mod.rs
git commit -m "feat: add Unix socket protocol with JSON command/response types"
```

---

## Task 6: Sound Module

**Files:**
- Create: `src/daemon/sound.rs`

- [ ] **Step 1: Write `src/daemon/sound.rs`**

```rust
use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use crate::config::SoundConfig;

pub struct SoundPlayer {
    config: SoundConfig,
}

impl SoundPlayer {
    pub fn new(config: SoundConfig) -> Self {
        Self { config }
    }

    /// Play the configured notification sound on a background thread.
    /// Returns immediately; playback happens asynchronously.
    pub fn play(&self) {
        if !self.config.enabled {
            return;
        }
        let path = crate::config::Config::expand_path(&self.config.file);
        let volume = self.config.volume;
        std::thread::spawn(move || {
            if let Err(e) = play_file(&path, volume) {
                eprintln!("cooee: sound error: {}", e);
            }
        });
    }
}

fn play_file(path: &PathBuf, volume: f32) -> Result<()> {
    let (_stream, stream_handle) = OutputStream::try_default()
        .context("opening audio output stream")?;
    let sink = Sink::try_new(&stream_handle)
        .context("creating audio sink")?;
    let file = File::open(path)
        .with_context(|| format!("opening sound file {:?}", path))?;
    let source = Decoder::new(BufReader::new(file))
        .context("decoding audio file")?;
    sink.set_volume(volume);
    sink.append(source);
    sink.sleep_until_end();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_player_disabled_does_not_panic() {
        let cfg = SoundConfig {
            enabled: false,
            file: "/nonexistent/file.ogg".to_string(),
            volume: 0.8,
        };
        let player = SoundPlayer::new(cfg);
        // Should return without error or panic when disabled
        player.play();
    }

    #[test]
    fn test_sound_player_constructs() {
        let cfg = SoundConfig::default();
        let _player = SoundPlayer::new(cfg);
    }
}
```

- [ ] **Step 2: Add to `src/daemon/mod.rs`**

```rust
pub mod state;
pub mod socket;
pub mod sound;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 3: Run tests**

```bash
cargo test daemon::sound:: 2>&1
```

Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/daemon/sound.rs src/daemon/mod.rs
git commit -m "feat: add sound playback module using rodio"
```

---

## Task 7: TTS Module

**Files:**
- Create: `src/daemon/tts.rs`

- [ ] **Step 1: Write `src/daemon/tts.rs`**

```rust
use anyhow::{Context, Result};
use speech_dispatcher::{Connection, Mode, Priority};
use crate::config::TtsConfig;

pub struct TtsClient {
    config: TtsConfig,
}

impl TtsClient {
    pub fn new(config: TtsConfig) -> Self {
        Self { config }
    }

    /// Speak the notification summary (called on notification arrival)
    pub fn speak_summary(&self, summary: &str) {
        if !self.config.enabled || !self.config.speak_summary {
            return;
        }
        self.speak(summary);
    }

    /// Speak the full notification body (called on `cooee speak` command)
    pub fn speak_body(&self, body: &str) {
        if !self.config.enabled {
            return;
        }
        self.speak(body);
    }

    fn speak(&self, text: &str) {
        let text = text.to_string();
        let voice = self.config.voice.clone();
        let rate = self.config.rate;
        std::thread::spawn(move || {
            if let Err(e) = do_speak(&text, &voice, rate) {
                eprintln!("cooee: TTS error: {}", e);
            }
        });
    }
}

fn do_speak(text: &str, voice: &str, rate: i32) -> Result<()> {
    let conn = Connection::open("cooee", "main", "cooee", Mode::Threaded)
        .context("connecting to speech-dispatcher")?;
    if !voice.is_empty() {
        conn.set_synthesis_voice(voice);
    }
    if rate != 0 {
        conn.set_voice_rate(rate);
    }
    conn.say(Priority::Important, text);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_client_disabled_does_not_panic() {
        let cfg = TtsConfig {
            enabled: false,
            speak_summary: true,
            voice: String::new(),
            rate: 0,
        };
        let client = TtsClient::new(cfg);
        // Should return without spawning or panicking
        client.speak_summary("hello");
        client.speak_body("world");
    }

    #[test]
    fn test_tts_client_speak_summary_off_does_not_spawn() {
        let cfg = TtsConfig {
            enabled: true,
            speak_summary: false,
            voice: String::new(),
            rate: 0,
        };
        let client = TtsClient::new(cfg);
        // speak_summary should be a no-op when speak_summary = false
        client.speak_summary("should not be spoken");
    }
}
```

- [ ] **Step 2: Add to `src/daemon/mod.rs`**

```rust
pub mod state;
pub mod socket;
pub mod sound;
pub mod tts;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 3: Run tests**

```bash
cargo test daemon::tts:: 2>&1
```

Expected: 2 tests pass (no speech-dispatcher daemon required for these tests).

- [ ] **Step 4: Commit**

```bash
git add src/daemon/tts.rs src/daemon/mod.rs
git commit -m "feat: add TTS module using speech-dispatcher"
```

---

## Task 8: Hyprland Monitor Detection

**Files:**
- Create: `src/daemon/hyprland.rs`
- Test: inline in `src/daemon/hyprland.rs`

- [ ] **Step 1: Write `src/daemon/hyprland.rs`**

```rust
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
    // Hyprland socket path: $XDG_RUNTIME_DIR/hypr/$SIG/.socket.sock
    Some(PathBuf::from(runtime).join("hypr").join(&sig).join(".socket.sock"))
}

fn query_active_monitor(path: &PathBuf) -> Result<String> {
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("connecting to Hyprland socket {:?}", path))?;
    // Request JSON of active workspace; parse monitor from response
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
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "abc123");
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        let path = hyprland_socket_path().unwrap();
        assert_eq!(path.to_str().unwrap(), "/run/user/1000/hypr/abc123/.socket.sock");
    }

    #[test]
    fn test_active_monitor_name_no_hyprland_env() {
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        // Should return None gracefully when not running under Hyprland
        assert!(active_monitor_name().is_none());
    }
}
```

- [ ] **Step 2: Add to `src/daemon/mod.rs`**

```rust
pub mod state;
pub mod socket;
pub mod sound;
pub mod tts;
pub mod hyprland;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 3: Run tests**

```bash
cargo test daemon::hyprland:: 2>&1
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/daemon/hyprland.rs src/daemon/mod.rs
git commit -m "feat: add Hyprland IPC monitor detection"
```

---

## Task 9: GTK4 UI Module

**Files:**
- Create: `src/daemon/ui.rs`

This module manages notification card windows using GTK4 and gtk4-layer-shell. It cannot be unit tested headlessly; rely on compile-time type safety and manual testing with a running Wayland compositor.

- [ ] **Step 1: Write `src/daemon/ui.rs`**

```rust
use gtk4::prelude::*;
use gtk4::{gdk, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation, Picture};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use crate::config::{Config, Position};
use crate::notification::Notification;
use crate::daemon::state::SharedState;
use std::sync::{Arc, Mutex};

/// Events sent from the async runtime into the GTK main loop
#[derive(Debug)]
pub enum UiEvent {
    ShowNotification(Notification),
    CloseNotification(u32),
    DismissLatest,
    Shutdown,
}

/// Manages the stack of visible notification windows
pub struct NotificationManager {
    config: Arc<Config>,
    windows: Vec<(u32, ApplicationWindow)>,
}

impl NotificationManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config, windows: Vec::new() }
    }

    pub fn show(&mut self, app: &Application, notification: Notification) {
        // Evict oldest if at capacity
        if self.windows.len() >= self.config.general.max_visible {
            if let Some((_, win)) = self.windows.first() {
                win.close();
            }
            self.windows.remove(0);
        }
        let win = build_notification_window(app, &notification, &self.config);
        self.position_window(&win, self.windows.len());
        win.present();
        self.windows.push((notification.id, win));
    }

    pub fn close(&mut self, id: u32) {
        if let Some(pos) = self.windows.iter().position(|(wid, _)| *wid == id) {
            let (_, win) = self.windows.remove(pos);
            win.close();
            self.reposition_all();
        }
    }

    pub fn dismiss_latest(&mut self) {
        if let Some((_, win)) = self.windows.last() {
            win.close();
        }
        self.windows.pop();
    }

    fn reposition_all(&mut self) {
        for (i, (_, win)) in self.windows.iter().enumerate() {
            self.position_window(win, i);
        }
    }

    fn position_window(&self, win: &ApplicationWindow, stack_index: usize) {
        let cfg = &self.config.general;
        let card_height = 100i32; // approximate; GTK will adjust after first render
        let gap = 8i32;
        let stack_offset = (stack_index as i32) * (card_height + gap);

        match cfg.position {
            Position::TopRight => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Right, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Right, cfg.margin_x);
            }
            Position::TopLeft => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Left, true);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Left, cfg.margin_x);
            }
            Position::BottomRight => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Right, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Right, cfg.margin_x);
            }
            Position::BottomLeft => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Left, true);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Left, cfg.margin_x);
            }
            Position::Center => {
                // Centre: no horizontal anchor, vertical centre approximated via equal top/bottom margins
                win.set_anchor(Edge::Top, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
            }
            Position::CenterTop => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, cfg.margin_y + stack_offset);
            }
            Position::CenterBottom => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, cfg.margin_y + stack_offset);
            }
        }
    }
}

fn build_notification_window(
    app: &Application,
    notification: &Notification,
    config: &Config,
) -> ApplicationWindow {
    let win = ApplicationWindow::new(app);
    win.init_layer_shell();
    win.set_layer(Layer::Overlay);
    win.set_namespace("cooee");

    // Set monitor if Hyprland provides one
    if let Some(monitor_name) = crate::daemon::hyprland::active_monitor_name() {
        let display = gdk::Display::default().unwrap();
        for i in 0..display.n_monitors() {
            if let Some(monitor) = display.monitor(i) {
                if monitor.connector().map(|c| c.to_string()) == Some(monitor_name.clone()) {
                    win.set_monitor(&monitor);
                    break;
                }
            }
        }
    }

    let vbox = GtkBox::new(Orientation::Vertical, 8);
    vbox.add_css_class("notification-card");

    // Header row: icon + summary
    let hbox = GtkBox::new(Orientation::Horizontal, 8);
    if !notification.app_icon.is_empty() {
        let icon = gtk4::Image::from_icon_name(&notification.app_icon);
        icon.set_pixel_size(config.general.icon_size);
        hbox.append(&icon);
    }
    let summary = Label::new(Some(&notification.summary));
    summary.add_css_class("notification-summary");
    summary.set_xalign(0.0);
    hbox.append(&summary);

    // Dismiss button
    let dismiss_btn = Button::with_label("×");
    dismiss_btn.add_css_class("notification-dismiss");
    hbox.append(&dismiss_btn);
    vbox.append(&hbox);

    // Body text (supports Pango markup)
    if !notification.body.is_empty() {
        let body = Label::new(None);
        body.set_markup(&notification.body);
        body.set_wrap(true);
        body.set_xalign(0.0);
        body.add_css_class("notification-body");
        vbox.append(&body);
    }

    // Action buttons
    if !notification.actions.is_empty() {
        let action_box = GtkBox::new(Orientation::Horizontal, 4);
        action_box.add_css_class("notification-actions");
        for action in &notification.actions {
            let btn = Button::with_label(&action.label);
            btn.add_css_class("notification-action-btn");
            let key = action.key.clone();
            let nid = notification.id;
            // ActionInvoked signal emitted via D-Bus — connected in daemon/mod.rs
            btn.connect_clicked(move |_| {
                // The actual ActionInvoked signal is fired through the shared state channel
                // See daemon/mod.rs for the full wiring
                eprintln!("cooee: action '{}' invoked on notification {}", key, nid);
            });
            action_box.append(&btn);
        }
        vbox.append(&action_box);
    }

    win.set_child(Some(&vbox));
    win.set_default_width(config.general.width);

    // Auto-dismiss timer
    let win_clone = win.clone();
    let timeout_ms = notification.display_duration_ms(config.general.timeout);
    if let Some(ms) = timeout_ms {
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(ms as u64),
            move || win_clone.close(),
        );
    }

    win
}
```

- [ ] **Step 2: Add to `src/daemon/mod.rs`**

```rust
pub mod state;
pub mod socket;
pub mod sound;
pub mod tts;
pub mod hyprland;
pub mod ui;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 3: Verify compile**

```bash
cargo build 2>&1
```

Expected: Compiles. Fix any gtk4 API mismatches (the gtk4 and gtk4-layer-shell crate APIs evolve; adjust method names to match the installed version if needed).

- [ ] **Step 4: Commit**

```bash
git add src/daemon/ui.rs src/daemon/mod.rs
git commit -m "feat: add GTK4 notification card UI with layer-shell positioning"
```

---

## Task 10: D-Bus Server

**Files:**
- Create: `src/daemon/dbus.rs`

- [ ] **Step 1: Write `src/daemon/dbus.rs`**

```rust
use anyhow::Result;
use std::collections::HashMap;
use zbus::{interface, Connection, SignalContext};
use crate::notification::Notification;
use crate::config::DndMode;
use crate::daemon::state::SharedState;
use crate::daemon::ui::UiEvent;
use tokio::sync::mpsc;

pub struct NotificationServer {
    state: SharedState,
    config: Arc<crate::config::Config>,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    async fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|v| v.downcast_ref::<u8>().ok().copied())
            .unwrap_or(1);

        let image_data: Option<Vec<u8>> = None; // TODO: parse image-data hint in future iteration

        let mut state = self.state.lock().unwrap();

        // Check DND
        if matches!(state.dnd_mode, DndMode::Full) {
            return state.next_notification_id();
        }

        let id = if replaces_id > 0 { replaces_id } else { state.next_notification_id() };
        let notification = Notification::new(
            id, app_name, app_icon, summary, body, actions, urgency, expire_timeout, image_data,
        );
        state.last_notification = Some(notification.clone());
        let is_silent = matches!(state.dnd_mode, DndMode::Silent);
        drop(state);

        // Play sound and speak summary when DND is off
        if !is_silent {
            let sound = crate::daemon::sound::SoundPlayer::new(self.config.sound.clone());
            sound.play();
            let tts = crate::daemon::tts::TtsClient::new(self.config.tts.clone());
            tts.speak_summary(&notification.summary);
        }

        let _ = self.ui_tx.send(UiEvent::ShowNotification(notification));

        id
    }

    async fn close_notification(
        &mut self,
        id: u32,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) {
        let _ = self.ui_tx.send(UiEvent::CloseNotification(id));
        let _ = Self::notification_closed(&ctx, id, 3).await; // reason 3 = closed by CloseNotification call
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".into(),
            "body-markup".into(),
            "body-images".into(),
            "icon-static".into(),
            "actions".into(),
        ]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "cooee".to_string(),
            "cooee".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(ctx: &SignalContext<'_>, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(ctx: &SignalContext<'_>, id: u32, action_key: String) -> zbus::Result<()>;
}

pub async fn start_dbus_server(
    state: SharedState,
    config: Arc<crate::config::Config>,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
) -> Result<Connection> {
    let server = NotificationServer { state, config, ui_tx, action_tx };
    let conn = zbus::ConnectionBuilder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at("/org/freedesktop/Notifications", server)?
        .build()
        .await?;
    Ok(conn)
}
```

- [ ] **Step 2: Add to `src/daemon/mod.rs`**

```rust
pub mod state;
pub mod socket;
pub mod sound;
pub mod tts;
pub mod hyprland;
pub mod ui;
pub mod dbus;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 3: Compile check**

```bash
cargo build 2>&1
```

Expected: Compiles. Resolve any zbus API version differences if needed (check `zbus` 4.x docs for `ConnectionBuilder`).

- [ ] **Step 4: Commit**

```bash
git add src/daemon/dbus.rs src/daemon/mod.rs
git commit -m "feat: add D-Bus server implementing org.freedesktop.Notifications"
```

---

## Task 11: Action Picker

**Files:**
- Create: `src/daemon/action_picker.rs`
- Test: inline in `src/daemon/action_picker.rs`

- [ ] **Step 1: Write `src/daemon/action_picker.rs`**

```rust
use anyhow::{bail, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use crate::notification::Action;

/// Presents `actions` to the user via the configured external picker command.
/// Returns the `Action` the user selected, or an error if picker exits non-zero or no match.
pub fn pick_action(picker_cmd: &str, actions: &[Action]) -> Result<Action> {
    if actions.is_empty() {
        bail!("last notification has no actions");
    }

    // Parse the picker command into program + args
    let mut parts = shell_words(picker_cmd);
    if parts.is_empty() {
        bail!("picker command is empty");
    }
    let program = parts.remove(0);

    let mut child = Command::new(&program)
        .args(&parts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to launch picker '{}': {}", program, e))?;

    // Write labels to stdin
    {
        let stdin = child.stdin.as_mut().unwrap();
        for action in actions {
            writeln!(stdin, "{}", action.label)?;
        }
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("picker cancelled (exit code {})", output.status);
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    actions
        .iter()
        .find(|a| a.label == selected)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("picker returned unknown label: '{}'", selected))
}

/// Minimal shell word splitter: splits on spaces, respects single quotes.
fn shell_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    for ch in s.chars() {
        match ch {
            '\'' => in_single_quote = !in_single_quote,
            ' ' if !in_single_quote => {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() { words.push(current); }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::Action;

    fn make_actions(pairs: &[(&str, &str)]) -> Vec<Action> {
        pairs.iter().map(|(k, l)| Action { key: k.to_string(), label: l.to_string() }).collect()
    }

    #[test]
    fn test_pick_action_empty_actions_errors() {
        let result = pick_action("cat", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no actions"));
    }

    #[test]
    fn test_pick_action_selects_correct_action() {
        // Use `echo` as the picker — it ignores stdin and outputs its argument
        let actions = make_actions(&[("default", "Open"), ("snooze", "Snooze")]);
        let result = pick_action("echo Open", &actions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().key, "default");
    }

    #[test]
    fn test_pick_action_non_zero_exit_is_cancel() {
        // `false` always exits with code 1
        let actions = make_actions(&[("default", "Open")]);
        let result = pick_action("false", &actions);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn test_shell_words_simple() {
        assert_eq!(shell_words("rofi -dmenu"), vec!["rofi", "-dmenu"]);
    }

    #[test]
    fn test_shell_words_single_quoted() {
        assert_eq!(
            shell_words("rofi -dmenu -p 'Action:'"),
            vec!["rofi", "-dmenu", "-p", "Action:"]
        );
    }

    #[test]
    fn test_shell_words_empty() {
        assert!(shell_words("").is_empty());
    }
}
```

- [ ] **Step 2: Add to `src/daemon/mod.rs`**

```rust
pub mod state;
pub mod socket;
pub mod sound;
pub mod tts;
pub mod hyprland;
pub mod ui;
pub mod dbus;
pub mod action_picker;

pub fn run() -> anyhow::Result<()> { Ok(()) }
```

- [ ] **Step 3: Run tests**

```bash
cargo test daemon::action_picker:: 2>&1
```

Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/daemon/action_picker.rs src/daemon/mod.rs
git commit -m "feat: add action picker module with shell word splitting"
```

---

## Task 12: Daemon Wiring

**Files:**
- Modify: `src/daemon/mod.rs` (replace stub `run()` with full wiring)

This ties together all daemon components: starts tokio runtime, GTK4 loop, D-Bus server, socket server, config watcher, and the channel bridge between them.

- [ ] **Step 1: Write full `src/daemon/mod.rs`**

```rust
pub mod state;
pub mod socket;
pub mod sound;
pub mod tts;
pub mod hyprland;
pub mod ui;
pub mod dbus;
pub mod action_picker;

use anyhow::Result;
use gtk4::prelude::*;
use gtk4::Application;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::config::{Config, DndMode};
use crate::daemon::state::new_shared_state;
use crate::daemon::ui::{NotificationManager, UiEvent};

const APP_ID: &str = "org.theophilusx.cooee";

pub fn run() -> Result<()> {
    let config = Arc::new(Config::load()?);

    // Set up shared state
    let initial_dnd = config.dnd.mode.clone();
    let shared_state = new_shared_state(initial_dnd);

    // Channel: async runtime → GTK main loop
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiEvent>();
    // Channel: socket Action handler → D-Bus ActionInvoked emitter
    let (action_tx, _action_rx) = mpsc::unbounded_channel::<(u32, String)>();
    let action_tx_for_socket = action_tx.clone();

    // Bridge: convert tokio mpsc into glib channel for GTK thread safety
    let (glib_tx, glib_rx) = glib::MainContext::channel::<UiEvent>(glib::Priority::DEFAULT);

    // Spawn tokio runtime on a background thread
    let state_for_tokio = shared_state.clone();
    let config_for_tokio = config.clone();
    let glib_tx_for_dbus = glib_tx.clone();
    let glib_tx_for_socket = glib_tx.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            // Forward UiEvents from the tokio mpsc channel into the glib channel.
            // This is the bridge: the D-Bus server sends UiEvents to `ui_tx`;
            // this task reads from `ui_rx` and pushes them to glib's main loop.
            let glib_tx_bridge = glib_tx_for_dbus.clone();
            tokio::spawn(async move {
                let mut rx = ui_rx;
                while let Some(event) = rx.recv().await {
                    // glib::Sender::send is infallible if the receiver is alive
                    let _ = glib_tx_bridge.send(event);
                }
            });

            // Start D-Bus server (uses ui_tx to push ShowNotification / CloseNotification events)
            let _conn = dbus::start_dbus_server(
                state_for_tokio.clone(),
                config_for_tokio.clone(),
                ui_tx,
                action_tx,
            ).await.expect("D-Bus server failed to start");

            // Emit ActionInvoked D-Bus signals from the action_tx channel.
            // The socket `Action` handler sends (id, key) here; we re-emit on D-Bus.
            let conn_for_action = _conn.clone();
            tokio::spawn(async move {
                while let Some((id, key)) = _action_rx.recv().await {
                    let iface_ref = conn_for_action
                        .object_server()
                        .interface::<_, dbus::NotificationServer>("/org/freedesktop/Notifications")
                        .await
                        .expect("interface ref");
                    let ctx = iface_ref.signal_context();
                    let _ = dbus::NotificationServer::action_invoked(ctx, id, key).await;
                }
            });

            // Start Unix socket server (uses glib_tx directly for Dismiss events)
            socket_server(state_for_tokio, config_for_tokio, glib_tx_for_socket, action_tx_for_socket).await;
        });
    });

    // Start GTK application
    let app = Application::builder().application_id(APP_ID).build();
    let config_for_gtk = config.clone();
    let shared_state_for_gtk = shared_state.clone();

    app.connect_activate(move |app| {
        let mut manager = NotificationManager::new(config_for_gtk.clone());
        let app_clone = app.clone();
        glib_rx.attach(None, move |event| {
            match event {
                UiEvent::ShowNotification(n) => manager.show(&app_clone, n),
                UiEvent::CloseNotification(id) => manager.close(id),
                UiEvent::DismissLatest => manager.dismiss_latest(),
                UiEvent::Shutdown => app_clone.quit(),
            }
            glib::ControlFlow::Continue
        });
    });

    app.run();
    Ok(())
}

async fn socket_server(
    state: crate::daemon::state::SharedState,
    config: Arc<Config>,
    glib_tx: glib::Sender<UiEvent>,
    action_tx: tokio::sync::mpsc::UnboundedSender<(u32, String)>,
) {
    use tokio::net::UnixListener;
    use crate::daemon::socket::{Command, Response, socket_path, read_command, write_response};

    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("binding cooee socket");

    loop {
        if let Ok((mut stream, _)) = listener.accept().await {
            let state = state.clone();
            let config = config.clone();
            let glib_tx = glib_tx.clone();
            tokio::spawn(async move {
                let cmd = match read_command(&mut stream).await {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let response = handle_command(cmd, &state, &config, &glib_tx, &action_tx).await;
                let _ = write_response(&mut stream, &response).await;
            });
        }
    }
}

async fn handle_command(
    cmd: crate::daemon::socket::Command,
    state: &crate::daemon::state::SharedState,
    config: &Config,
    glib_tx: &glib::Sender<UiEvent>,
    action_tx: &tokio::sync::mpsc::UnboundedSender<(u32, String)>,
) -> crate::daemon::socket::Response {
    use crate::daemon::socket::{Command, Response};

    match cmd {
        Command::Speak => {
            let notification = state.lock().unwrap().last_notification.clone();
            match notification {
                None => Response::err("no notification to speak"),
                Some(n) => {
                    let tts = tts::TtsClient::new(config.tts.clone());
                    tts.speak_body(&n.body);
                    Response::ok()
                }
            }
        }
        Command::Dnd { mode } => {
            let mut s = state.lock().unwrap();
            match mode.as_str() {
                "off" => s.set_dnd(DndMode::Off),
                "silent" => s.set_dnd(DndMode::Silent),
                "full" => s.set_dnd(DndMode::Full),
                "toggle" => s.toggle_dnd(),
                _ => return Response::err("unknown DND mode: use off, silent, full, or toggle"),
            }
            let mode_str = s.dnd_mode_str().to_string();
            Response::ok_dnd(&mode_str)
        }
        Command::Dismiss => {
            let _ = glib_tx.send(UiEvent::DismissLatest);
            Response::ok()
        }
        Command::Action => {
            let notification = state.lock().unwrap().last_notification.clone();
            match notification {
                None => Response::err("no notification to act on"),
                Some(n) => {
                    if n.actions.is_empty() {
                        return Response::err("last notification has no actions");
                    }
                    match action_picker::pick_action(&config.actions.picker, &n.actions) {
                        Ok(action) => {
                            // Send (id, key) to the action_tx channel; the tokio task in run()
                            // holds the D-Bus Connection and emits ActionInvoked from there.
                            let _ = action_tx.send((n.id, action.key));
                            Response::ok()
                        }
                        Err(e) => Response::err(&e.to_string()),
                    }
                }
            }
        }
        Command::Status => {
            let s = state.lock().unwrap();
            let status = format!("running | DND: {}", s.dnd_mode_str());
            Response::ok_status(&status)
        }
    }
}
```

**Note on glib/tokio bridging:** The above sketch illustrates the wiring. In practice, the `UiEvent` receiver from the tokio `mpsc` channel needs to be forwarded to the `glib::Sender`. Implement a dedicated tokio task that reads from `ui_rx` and calls `glib_tx.send()` in a loop. This is the standard pattern for bridging async Rust with GTK4.

- [ ] **Step 2: Compile check and fix**

```bash
cargo build 2>&1
```

Fix any compilation errors (borrow checker issues with the glib/tokio bridge are expected here — resolve by restructuring the mpsc receiver into the forwarding task).

- [ ] **Step 3: Smoke test (requires running Hyprland/Wayland session)**

```bash
cargo run -- daemon &
sleep 1
notify-send "Test" "Hello from cooee"
```

Expected: Notification card appears on screen, sound plays, TTS speaks the summary.

- [ ] **Step 4: Test DND and speak commands**

```bash
cargo run -- dnd silent
notify-send "Silent Test" "No sound or speech"
cargo run -- speak     # should speak "Silent Test" body
cargo run -- dnd off
cargo run -- dismiss
```

- [ ] **Step 5: Commit**

```bash
git add src/daemon/mod.rs
git commit -m "feat: wire daemon components — GTK4 loop, tokio runtime, D-Bus, socket server"
```

---

## Task 13: Systemd Service & Data Files

**Files:**
- Create: `data/cooee.service`
- Create: `data/sounds/` (placeholder; bundled .ogg placed here during packaging)
- Create: `Makefile` (install targets)

**Note on bundled sound:** The default `data/sounds/notify.ogg` is not created by this plan. A suitable free-license sound can be sourced from the GNOME sound theme package (`gnome-audio`) or freedesktop.org sound theme. On Fedora: `rpm -ql gnome-audio | grep message` will show candidates. Copy one to `data/sounds/notify.ogg` before running `make install`.

**Note on config hot-reload:** The `notify` crate is in `Cargo.toml` but file-watching is not wired in this plan. Config hot-reload can be added in a follow-up task: spawn a `notify::RecommendedWatcher` on the config path in the tokio thread and re-read + apply config on `EventKind::Modify`.

- [ ] **Step 1: Write `data/cooee.service`**

```ini
[Unit]
Description=cooee notification daemon
Documentation=https://github.com/theophilusx/cooee
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=%h/.local/bin/cooee daemon
Restart=on-failure
RestartSec=2
Type=dbus
BusName=org.freedesktop.Notifications

[Install]
WantedBy=graphical-session.target
```

- [ ] **Step 2: Write `Makefile`**

```makefile
PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin
SERVICEDIR := $(HOME)/.config/systemd/user
SOUNDDIR := $(HOME)/.config/cooee/sounds

.PHONY: build install uninstall

build:
	cargo build --release

install: build
	install -Dm755 target/release/cooee $(BINDIR)/cooee
	install -Dm644 data/cooee.service $(SERVICEDIR)/cooee.service
	install -Dm644 data/sounds/notify.ogg $(SOUNDDIR)/notify.ogg
	systemctl --user daemon-reload
	systemctl --user enable --now cooee
	@echo "cooee installed and started"

uninstall:
	systemctl --user disable --now cooee || true
	rm -f $(BINDIR)/cooee
	rm -f $(SERVICEDIR)/cooee.service
	systemctl --user daemon-reload
	@echo "cooee uninstalled"
```

- [ ] **Step 3: Add a README with quick-start instructions**

Create `README.md`:

```markdown
# cooee

Wayland desktop notification daemon for Hyprland with sound and TTS.

## Install

```bash
make install
```

## Hyprland keybinds

Add to `~/.config/hypr/hyprland.conf`:

```
bind = $mod, N, exec, cooee speak
bind = $mod SHIFT, N, exec, cooee dnd toggle
bind = $mod, X, exec, cooee dismiss
bind = $mod, A, exec, cooee action
```

## Configuration

`~/.config/cooee/config.toml` is written on first run with defaults.

## License

GPLv3 — see LICENSE.
```

- [ ] **Step 4: Commit**

```bash
git add data/ Makefile README.md
git commit -m "feat: add systemd service, Makefile install targets, and README"
```

---

## Task 14: Integration Test

**Files:**
- Create: `tests/integration_test.rs`

- [ ] **Step 1: Write integration tests for socket protocol round-trip**

```rust
// tests/integration_test.rs
// These tests start a minimal socket server and verify command round-trips.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn start_test_socket_server(socket_path: &std::path::Path) -> thread::JoinHandle<()> {
    let path = socket_path.to_path_buf();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use cooee::daemon::socket::{read_command, write_response, Response, Command};
            use tokio::net::UnixListener;
            let listener = UnixListener::bind(&path).unwrap();
            if let Ok((mut stream, _)) = listener.accept().await {
                let cmd = read_command(&mut stream).await.unwrap();
                let resp = match cmd {
                    Command::Status => Response::ok_status("running | DND: off"),
                    _ => Response::ok(),
                };
                write_response(&mut stream, &resp).await.unwrap();
            }
        });
    })
}

#[test]
fn test_status_command_round_trip() {
    let dir = TempDir::new().unwrap();
    let socket_path = dir.path().join("test_cooee.sock");

    let _server = start_test_socket_server(&socket_path);
    thread::sleep(Duration::from_millis(50)); // let server bind

    let mut stream = UnixStream::connect(&socket_path).unwrap();
    let cmd = cooee::daemon::socket::Command::Status;
    let mut line = serde_json::to_string(&cmd).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).unwrap();

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    let val: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(val["ok"], true);
    assert!(val["status"].as_str().unwrap().contains("running"));
}
```

Add to `Cargo.toml` under `[lib]`:
```toml
[lib]
name = "cooee"
path = "src/lib.rs"
```

Create `src/lib.rs`:
```rust
pub mod config;
pub mod notification;
pub mod daemon;
pub mod client;
```

- [ ] **Step 2: Run integration test**

```bash
cargo test --test integration_test 2>&1
```

Expected: 1 test passes.

- [ ] **Step 3: Run all tests**

```bash
cargo test 2>&1
```

Expected: All unit and integration tests pass.

- [ ] **Step 4: Final commit**

```bash
git add tests/ src/lib.rs Cargo.toml
git commit -m "test: add socket protocol integration test"
```

---

## Summary

| Task | Delivers |
|---|---|
| 1 — Scaffold | Binary compiles with clap CLI |
| 2 — Config | TOML loading, XDG paths, hot-reload groundwork |
| 3 — Notification | Struct, urgency, action parsing |
| 4 — State | DND mode, last notification, Arc<Mutex<>> |
| 5 — Socket | JSON protocol, client, server message types |
| 6 — Sound | rodio playback on background thread |
| 7 — TTS | speech-dispatcher client |
| 8 — Hyprland | Active monitor query via IPC socket |
| 9 — UI | GTK4 card windows, layer-shell, stacking |
| 10 — D-Bus | org.freedesktop.Notifications server |
| 11 — Action Picker | External picker (rofi), shell word split |
| 12 — Daemon Wiring | All components connected, tokio↔GTK bridge |
| 13 — Data Files | systemd service, Makefile, README |
| 14 — Integration Test | Socket round-trip verified |
