# TTS Smart Speak — Design Spec

**Date:** 2026-03-22
**Status:** Approved

## Problem

The current TTS call site always speaks the notification summary (heading), regardless of whether a meaningful body is present. Short-body notifications (e.g. "Door unlocked", "Build passed") convey their useful content in the body, not the heading. Speaking only the heading loses that information.

## Goal

Automatically speak the body when it is short enough to be useful as audio, and the summary when the body is too long or absent. When both are short, speak them together as one utterance.

## Decision

**Option A — `speak_smart` method on `TtsClient`.**
`TtsClient` becomes the single owner of all TTS selection logic. The call site in `dbus.rs` is a one-line change. Logic is isolated and testable without touching speech-dispatcher.

## Config

One new field added to `TtsConfig`:

```toml
[tts]
enabled = true
speak_summary = true      # existing — when false, all auto-TTS is suppressed
body_word_limit = 15      # new: words threshold; 0 = always speak summary only
voice = ""
rate = 0
```

`body_word_limit: u32` defaults to `15`. Because `u32::default()` returns `0` (which would silently enable summary-only mode), the default **must** be expressed as a named serde helper function, following the existing pattern in `TtsConfig`:

```rust
#[serde(default = "TtsConfig::default_body_word_limit")]
pub body_word_limit: u32,

fn default_body_word_limit() -> u32 { 15 }
```

This ensures existing configs without the field continue to use `15`, not `0`.

Word count is measured by splitting on ASCII whitespace — simple, no dependencies. The `speak_summary` guard is respected before any word-count logic runs.

**`body_word_limit = 0`** is treated as "always speak summary only". The combined-speak branch requires both `body_words <= limit` and `summary_words <= limit`. With `limit = 0`, any non-empty body has at least one word (`body_words >= 1`), so `body_words <= 0` is false. Independently, any non-empty summary also has at least one word, so `summary_words <= 0` is false. Either guard alone would block the combined branch; both must remain in the implementation. Note: `summary_words <= limit` is not redundant — it is what prevents a long summary from being spoken in combined mode even when the body is short (covered by `test_compose_long_summary`).

## Logic

String composition and TTS dispatch are separated into two units:

### `compose_utterance` (pure, free function)

```rust
fn compose_utterance(summary: &str, body: &str, limit: u32) -> String
```

Contains the word-count selection logic; returns the text to speak. No side effects — fully unit-testable without speech-dispatcher.

```
if summary is empty → return ""

body_words    = word_count(body)   // word_count returns u32
summary_words = word_count(summary)

if body is non-empty AND body_words <= limit AND summary_words <= limit:
    return "{summary}.  {body}"    // period + two spaces → natural pause
else:
    return summary                  // fallback: always speak summary verbatim,
                                    // even if summary itself exceeds the limit
```

The fallback always speaks the summary verbatim. There is no truncation — if the summary is long, it is spoken in full. The limit only controls whether the combined path is taken.

### `speak_smart` (method on `TtsClient`)

```rust
pub fn speak_smart(&self, summary: &str, body: &str)
```

Guards on `enabled` / `speak_summary`, then delegates to `compose_utterance` and `speak`:

```
if !enabled or !speak_summary → return (no-op)

let text = compose_utterance(summary, body, self.config.body_word_limit)
if text is empty → return (no-op)
self.speak(&text)
```

### `word_count` (pure, free function)

```rust
fn word_count(s: &str) -> u32 {
    s.split_whitespace().count() as u32
}
```

Returns `u32` to match `body_word_limit`, avoiding a type mismatch in the `<=` comparison.

The separator `".  "` (period, two spaces) produces a natural mid-sentence pause in speech-dispatcher without requiring a second `say()` call.

Existing methods `speak_summary` and `speak_body` are unchanged. `speak_body` continues to serve the `cooee speak` socket command.

## Call Site Change

`dbus.rs` — one line replaced:

```rust
// before
tts.speak_summary(&notification.summary);

// after
tts.speak_smart(&notification.summary, &notification.body);
```

## Tests

Tests are split into two groups:

**`compose_utterance` tests** — assert on the returned `String` directly; no disabled config needed.

| Test | Scenario | Expected return value |
|---|---|---|
| `test_compose_short_body_and_summary` | both ≤ limit | `"Summary.  Body"` |
| `test_compose_long_body` | body > limit | `"Summary"` |
| `test_compose_empty_body` | body is `""` | `"Summary"` |
| `test_compose_empty_summary` | summary is `""` | `""` |
| `test_compose_long_summary` | summary > limit, body short | `"Summary"` (spoken verbatim, no truncation) |
| `test_compose_zero_limit` | `limit = 0` | `"Summary"` |

**`speak_smart` integration tests** — use disabled config; assert no-op / no-panic only.

| Test | Scenario | Expected |
|---|---|---|
| `test_speak_smart_disabled` | `enabled = false` | no-op |
| `test_speak_smart_speak_summary_off` | `speak_summary = false` | no-op |

**`word_count` test**

| Test | Scenario | Expected |
|---|---|---|
| `test_word_count` | empty, single word, multi-word, extra whitespace | correct `u32` counts |

All existing `TtsConfig` struct-literal test helpers (`disabled_config`, `speak_summary_off_config`) must be updated to include the new `body_word_limit` field.

## Files Changed

| File | Change |
|---|---|
| `src/config.rs` | Add `body_word_limit: u32` to `TtsConfig` with named serde default `15`; add `default_body_word_limit()` helper; update `Default` impl; add `assert_eq!(cfg.tts.body_word_limit, 15)` to `test_default_config_has_expected_values` |
| `src/daemon/tts.rs` | Add `speak_smart`, `compose_utterance`, `word_count`; new tests; update existing `TtsConfig` struct-literal helpers to include `body_word_limit` |
| `src/daemon/dbus.rs` | Replace `speak_summary` call with `speak_smart` |
