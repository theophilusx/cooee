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

    /// Speak the notification using word-count-based selection:
    /// - Both summary and body short → speak "{summary}.  {body}"
    /// - Body long, empty, or summary long → speak summary only
    /// - summary empty → no-op
    pub fn speak_smart(&self, summary: &str, body: &str) {
        if !self.config.enabled || !self.config.speak_summary {
            return;
        }
        let text = compose_utterance(summary, body, self.config.body_word_limit);
        if text.is_empty() {
            return;
        }
        self.speak(&text);
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

/// Returns the text to speak for a notification.
/// Pure function — no side effects, fully testable without speech-dispatcher.
fn compose_utterance(summary: &str, body: &str, limit: u32) -> String {
    if summary.is_empty() {
        return String::new();
    }
    let body_words = word_count(body);
    let summary_words = word_count(summary);
    if !body.is_empty() && body_words <= limit && summary_words <= limit {
        format!("{}.  {}", summary, body)
    } else {
        summary.to_owned()
    }
}

/// Counts whitespace-separated words. Returns u32 to match body_word_limit.
fn word_count(s: &str) -> u32 {
    s.split_whitespace().count() as u32
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

    // --- existing tests (unchanged) ---

    #[test]
    fn test_tts_client_disabled_does_not_panic() {
        let client = TtsClient::new(disabled_config());
        client.speak_summary("hello");
        client.speak_body("world");
    }

    #[test]
    fn test_tts_client_speak_summary_off_does_not_spawn() {
        let client = TtsClient::new(speak_summary_off_config());
        client.speak_summary("hello");
    }

    // --- word_count ---

    #[test]
    fn test_word_count() {
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("hello"), 1);
        assert_eq!(word_count("hello world"), 2);
        assert_eq!(word_count("  extra   spaces  "), 2);
        assert_eq!(word_count("one two three four five"), 5);
    }

    // --- compose_utterance ---

    #[test]
    fn test_compose_short_body_and_summary() {
        // both under limit → speak combined with separator
        let result = compose_utterance("Summary", "Body", 15);
        assert_eq!(result, "Summary.  Body");
    }

    #[test]
    fn test_compose_long_body() {
        // body over limit → summary only
        let body = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen";
        let result = compose_utterance("Summary", body, 15);
        assert_eq!(result, "Summary");
    }

    #[test]
    fn test_compose_empty_body() {
        // no body → summary only
        let result = compose_utterance("Summary", "", 15);
        assert_eq!(result, "Summary");
    }

    #[test]
    fn test_compose_empty_summary() {
        // no summary → empty string (caller treats as no-op)
        let result = compose_utterance("", "Body", 15);
        assert_eq!(result, "");
    }

    #[test]
    fn test_compose_long_summary() {
        // summary over limit, body short → summary still spoken verbatim (no truncation)
        let summary = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen";
        let result = compose_utterance(summary, "Body", 15);
        assert_eq!(result, summary);
    }

    #[test]
    fn test_compose_zero_limit() {
        // limit=0 → combined branch unreachable, summary only
        let result = compose_utterance("Summary", "Body", 0);
        assert_eq!(result, "Summary");
    }

    // --- speak_smart guard tests (disabled config; assert no-op only) ---

    #[test]
    fn test_speak_smart_disabled() {
        let client = TtsClient::new(disabled_config());
        // enabled=false → no-op, must not panic
        client.speak_smart("Summary", "Body");
    }

    #[test]
    fn test_speak_smart_speak_summary_off() {
        let client = TtsClient::new(speak_summary_off_config());
        // speak_summary=false → no-op, must not panic
        client.speak_smart("Summary", "Body");
    }
}
