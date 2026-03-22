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
