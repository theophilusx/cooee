use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub type SharedConfig = Arc<RwLock<Config>>;

macro_rules! default_val {
    ($name:ident, $ty:ty, $val:expr) => {
        fn $name() -> $ty { $val }
    };
}

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    #[serde(default = "GeneralConfig::default_font_size")]
    pub font_size: i32,
}

impl GeneralConfig {
    default_val!(default_position, Position, Position::TopRight);
    default_val!(default_margin_x, i32, 16);
    default_val!(default_margin_y, i32, 48);
    default_val!(default_max_visible, usize, 5);
    default_val!(default_timeout, u32, 5000);
    default_val!(default_icon_size, i32, 36);
    default_val!(default_width, i32, 360);
    default_val!(default_font_size, i32, 14);
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
            font_size: Self::default_font_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SoundConfig {
    #[serde(default = "SoundConfig::default_enabled")]
    pub enabled: bool,
    /// Raw path string; callers must pass through `Config::expand_path` before use.
    #[serde(default = "SoundConfig::default_file")]
    pub file: String,
    #[serde(default = "SoundConfig::default_volume")]
    pub volume: f64,
}

impl SoundConfig {
    default_val!(default_enabled, bool, true);
    default_val!(default_file, String, "~/.config/cooee/sounds/notify.ogg".to_string());
    default_val!(default_volume, f64, 0.8);
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
    #[serde(default = "TtsConfig::default_body_word_limit")]
    pub body_word_limit: u32,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub rate: i32,
}

impl TtsConfig {
    default_val!(default_enabled, bool, true);
    default_val!(default_speak_summary, bool, true);
    default_val!(default_body_word_limit, u32, 15);
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            speak_summary: Self::default_speak_summary(),
            body_word_limit: Self::default_body_word_limit(),
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
    default_val!(default_picker, String, "rofi -dmenu -p 'Action:'".to_string());
}

impl Default for ActionsConfig {
    fn default() -> Self {
        Self { picker: Self::default_picker() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryConfig {
    #[serde(default = "HistoryConfig::default_max_entries")]
    pub max_entries: usize,
}

impl HistoryConfig {
    fn default_max_entries() -> usize { 50 }
}

impl Default for HistoryConfig {
    fn default() -> Self { Self { max_entries: 50 } }
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
    #[serde(default)]
    pub history: HistoryConfig,
}

impl Config {
    pub fn shared(self) -> SharedConfig {
        Arc::new(RwLock::new(self))
    }

    pub fn config_path() -> std::path::PathBuf {
        config_path()
    }

    pub fn style_path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs_next_home()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            });
        base.join("cooee").join("style.css")
    }

    /// Write `css` to `style_path()` only if the file does not yet exist.
    pub fn ensure_default_style(css: &str) -> Result<()> {
        let path = Self::style_path();
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, css)?;
        Ok(())
    }

    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            let cfg = Config::default();
            cfg.write_default(&path)?;
            return Ok(cfg);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config {:?}", path))?;
        toml::from_str(&text).with_context(|| format!(
            "invalid config at {}\nFix the value shown above, or delete the file to regenerate defaults",
            path.display()
        ))
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
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Serialise tests that mutate environment variables to prevent data races.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_config_has_expected_values() {
        let cfg = Config::default();
        assert_eq!(cfg.general.margin_x, 16);
        assert_eq!(cfg.general.margin_y, 48);
        assert_eq!(cfg.general.max_visible, 5);
        assert_eq!(cfg.general.timeout, 5000);
        assert_eq!(cfg.general.width, 360);
        assert_eq!(cfg.general.icon_size, 36);
        assert_eq!(cfg.general.font_size, 14);
        assert!(cfg.sound.enabled);
        assert!((cfg.sound.volume - 0.8).abs() < 0.001);
        assert!(cfg.tts.enabled);
        assert!(cfg.tts.speak_summary);
        assert_eq!(cfg.tts.body_word_limit, 15);
        assert_eq!(cfg.tts.rate, 0);
        assert_eq!(cfg.actions.picker, "rofi -dmenu -p 'Action:'");
    }

    #[test]
    fn test_font_size_can_be_set_via_toml() {
        let toml = r#"
[general]
font_size = 18
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.general.font_size, 18);
        assert_eq!(cfg.general.margin_x, 16); // other fields still default
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
        let _guard = ENV_LOCK.lock().unwrap();
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
    fn test_history_config_default_max_entries() {
        let cfg = Config::default();
        assert_eq!(cfg.history.max_entries, 50);
    }

    #[test]
    fn test_history_config_toml_override() {
        let toml = r#"
[history]
max_entries = 100
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.history.max_entries, 100);
        // other fields still at defaults
        assert_eq!(cfg.general.margin_x, 16);
    }

    #[test]
    fn test_history_config_zero_disables() {
        let toml = r#"
[history]
max_entries = 0
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.history.max_entries, 0);
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
