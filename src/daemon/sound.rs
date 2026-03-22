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
