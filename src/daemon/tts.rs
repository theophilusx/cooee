use crate::config::TtsConfig;

pub struct TtsClient {
    config: TtsConfig,
}

impl TtsClient {
    pub fn new(config: TtsConfig) -> Self {
        Self { config }
    }

    /// Speak the notification summary if TTS is enabled and speak_summary is true.
    pub fn speak_summary(&self, summary: &str) {
        if !self.config.enabled || !self.config.speak_summary {
            return;
        }
        self.speak(summary);
    }

    /// Speak the full notification body if TTS is enabled.
    pub fn speak_body(&self, body: &str) {
        if !self.config.enabled {
            return;
        }
        self.speak(body);
    }

    /// Spawn a background thread to speak the given text via speech-dispatcher.
    fn speak(&self, text: &str) {
        let text = text.to_owned();
        let voice = self.config.voice.clone();
        let rate = self.config.rate;

        std::thread::spawn(move || {
            use speech_dispatcher::{Connection, Mode, Priority};

            let conn = match Connection::open("cooee", "main", "cooee", Mode::Threaded) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("cooee: TTS connection failed: {}", e);
                    return;
                }
            };

            if !voice.is_empty() {
                let v = speech_dispatcher::Voice {
                    name: voice,
                    language: String::new(),
                    variant: None,
                };
                if let Err(e) = conn.set_synthesis_voice(&v) {
                    eprintln!("cooee: TTS set_synthesis_voice failed: {}", e);
                }
            }

            if rate != 0 {
                if let Err(e) = conn.set_voice_rate(rate) {
                    eprintln!("cooee: TTS set_voice_rate failed: {}", e);
                }
            }

            conn.say(Priority::Important, text);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_config() -> TtsConfig {
        TtsConfig {
            enabled: false,
            speak_summary: true,
            body_word_limit: 15,
            voice: String::new(),
            rate: 0,
        }
    }

    fn speak_summary_off_config() -> TtsConfig {
        TtsConfig {
            enabled: true,
            speak_summary: false,
            body_word_limit: 15,
            voice: String::new(),
            rate: 0,
        }
    }

    #[test]
    fn test_tts_client_disabled_does_not_panic() {
        let client = TtsClient::new(disabled_config());
        // Should return immediately without spawning a thread or panicking
        client.speak_summary("hello");
        client.speak_body("world");
    }

    #[test]
    fn test_tts_client_speak_summary_off_does_not_spawn() {
        let client = TtsClient::new(speak_summary_off_config());
        // speak_summary should be a no-op when speak_summary=false
        client.speak_summary("hello");
    }
}
