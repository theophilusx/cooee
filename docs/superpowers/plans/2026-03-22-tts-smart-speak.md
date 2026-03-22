# TTS Smart Speak Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically speak the notification body (or both body and summary) when the text is short, falling back to summary-only when the body is long or absent, with a configurable word-count threshold.

**Architecture:** Add `body_word_limit: u32` to `TtsConfig`; extract a pure `compose_utterance` helper in `tts.rs` that owns all selection logic and is tested directly; add `speak_smart` as a thin wrapper that guards and dispatches; replace the single `speak_summary` call in `dbus.rs`.

**Tech Stack:** Rust, serde/toml (config), speech-dispatcher (TTS), cargo test (unit tests)

**Spec:** `docs/superpowers/specs/2026-03-22-tts-smart-speak-design.md`

---

## File Map

| File | What changes |
|---|---|
| `src/config.rs` | Add `body_word_limit: u32` field to `TtsConfig` |
| `src/daemon/tts.rs` | Add `word_count`, `compose_utterance`, `speak_smart`; update test helpers |
| `src/daemon/dbus.rs` | Replace `speak_summary` with `speak_smart` at call site |

---

### Task 1: Add `body_word_limit` to `TtsConfig`

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write the failing test**

  Open `src/config.rs`. In the `tests` module, find `test_default_config_has_expected_values` (around line 232). Add one assertion at the end of the test body:

  ```rust
  assert_eq!(cfg.tts.body_word_limit, 15);
  ```

- [ ] **Step 2: Run to confirm it fails**

  ```bash
  cargo test test_default_config_has_expected_values 2>&1
  ```

  Expected: compile error — `body_word_limit` does not exist on `TtsConfig`.

- [ ] **Step 3: Add the field to `TtsConfig`**

  In `src/config.rs`, update `TtsConfig` and its `impl` block. The struct currently ends at the `rate` field (line ~113). Add the new field and default helper, following the exact same pattern used for `speak_summary`:

  ```rust
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
      fn default_enabled() -> bool { true }
      fn default_speak_summary() -> bool { true }
      fn default_body_word_limit() -> u32 { 15 }
  }
  ```

  **Why not `#[serde(default)]` alone?** `u32::default()` returns `0`, which silently enables summary-only mode for anyone upgrading without the field in their config. The named helper guarantees `15`.

  Also update the `Default` impl for `TtsConfig` (around line 121) to include the new field:

  ```rust
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
  ```

- [ ] **Step 4: Run the test to confirm it passes**

  ```bash
  cargo test test_default_config_has_expected_values 2>&1
  ```

  Expected: `test config::tests::test_default_config_has_expected_values ... ok`

- [ ] **Step 5: Run the full test suite to catch any breakage**

  The two struct-literal `TtsConfig` constructors in `src/daemon/tts.rs` will now fail to compile because they don't include the new `body_word_limit` field. That is expected — they will be fixed in Task 2.

  ```bash
  cargo test 2>&1 | head -20
  ```

  Expected: compile errors referencing `disabled_config` and `speak_summary_off_config` in `tts.rs` missing `body_word_limit`. No other errors.

- [ ] **Step 6: Commit**

  ```bash
  git add src/config.rs
  git commit -m "feat: add body_word_limit to TtsConfig, default 15"
  ```

---

### Task 2: Add `word_count`, `compose_utterance`, and `speak_smart` to `TtsClient`

**Files:**
- Modify: `src/daemon/tts.rs`

- [ ] **Step 1: Write the failing tests**

  Open `src/daemon/tts.rs`. Replace the entire `#[cfg(test)]` block with the following (this updates the two broken helpers and adds all new tests):

  ```rust
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
  ```

- [ ] **Step 2: Run to confirm tests fail**

  ```bash
  cargo test --lib 2>&1 | grep -E "error|FAILED"
  ```

  Expected: compile errors — `word_count`, `compose_utterance`, `speak_smart` not found.

- [ ] **Step 3: Implement `word_count`, `compose_utterance`, and `speak_smart`**

  In `src/daemon/tts.rs`, add the following after the closing `}` of `speak_body` (before the private `fn speak`):

  ```rust
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
  ```

  Then add the two free functions after the `TtsClient` impl block (before `#[cfg(test)]`):

  ```rust
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
  ```

- [ ] **Step 4: Run the tests to confirm they pass**

  ```bash
  cargo test --lib 2>&1 | grep -E "daemon::tts|FAILED"
  ```

  Expected: all `daemon::tts::*` lines show `ok`. No `FAILED`.

- [ ] **Step 5: Run the full suite to confirm nothing else broke**

  ```bash
  cargo test 2>&1 | tail -5
  ```

  Expected: `test result: ok. N passed; 0 failed`

- [ ] **Step 6: Commit**

  ```bash
  git add src/daemon/tts.rs
  git commit -m "feat: add speak_smart, compose_utterance, word_count to TtsClient"
  ```

---

### Task 3: Update the call site in `dbus.rs`

**Files:**
- Modify: `src/daemon/dbus.rs`

- [ ] **Step 1: Replace the call site**

  Open `src/daemon/dbus.rs`. Find line 67:

  ```rust
  tts.speak_summary(&notification.summary);
  ```

  Replace it with:

  ```rust
  tts.speak_smart(&notification.summary, &notification.body);
  ```

- [ ] **Step 2: Build to confirm it compiles**

  ```bash
  cargo build 2>&1 | grep -E "error|warning: unused"
  ```

  Expected: clean build, no errors, no new unused-import warnings.

- [ ] **Step 3: Run the full test suite**

  ```bash
  cargo test 2>&1 | tail -5
  ```

  Expected: `test result: ok. N passed; 0 failed`

- [ ] **Step 4: Install and smoke-test**

  ```bash
  make install
  ```

  Then send a notification with a short body and confirm it is spoken:

  ```bash
  notify-send "Door" "Front door unlocked"
  ```

  Expected: TTS speaks "Door.  Front door unlocked".

  Then send one with a long body and confirm only the summary is spoken:

  ```bash
  notify-send "Update" "A system update is available that includes security patches for several components and performance improvements across the board"
  ```

  Expected: TTS speaks "Update" only.

- [ ] **Step 5: Commit**

  ```bash
  git add src/daemon/dbus.rs
  git commit -m "feat: use speak_smart in dbus notify handler for body-aware TTS"
  ```

---

## Done

All three tasks complete. The feature is live. `body_word_limit` in `~/.config/cooee/config.toml` controls the threshold (default 15 words).
