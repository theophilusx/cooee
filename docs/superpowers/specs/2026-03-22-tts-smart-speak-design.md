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

`body_word_limit: u32` defaults to `15`. Word count is measured by splitting on ASCII whitespace — simple, no dependencies. The `speak_summary` guard is respected before any word-count logic runs.

**`body_word_limit = 0`** is treated as "always speak summary only" (a body of any length exceeds a limit of zero words).

## Logic

New method `speak_smart(summary: &str, body: &str)` on `TtsClient`:

```
if !enabled or !speak_summary → return (no-op)

body_words  = word_count(body)
summary_words = word_count(summary)
limit = body_word_limit

if body is non-empty AND body_words <= limit AND summary_words <= limit:
    speak("{summary}.  {body}")   // period + two spaces → natural pause
else:
    speak(summary)
```

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

## Helper

```rust
fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}
```

Pure function, lives in `tts.rs`, tested independently.

## Tests

All tests use a disabled or no-speech-dispatcher config to avoid spawning audio.

| Test | Scenario | Expected utterance |
|---|---|---|
| `test_word_count` | various strings | correct counts |
| `test_speak_smart_short_body_and_summary` | both ≤ limit | `"Summary.  Body"` |
| `test_speak_smart_long_body` | body > limit | `"Summary"` |
| `test_speak_smart_empty_body` | body is `""` | `"Summary"` |
| `test_speak_smart_long_summary` | summary > limit | `"Summary"` |
| `test_speak_smart_zero_limit` | `body_word_limit = 0` | `"Summary"` |
| `test_speak_smart_disabled` | `enabled = false` | no-op |
| `test_speak_smart_speak_summary_off` | `speak_summary = false` | no-op |

## Files Changed

| File | Change |
|---|---|
| `src/config.rs` | Add `body_word_limit: u32` to `TtsConfig` with default `15` |
| `src/daemon/tts.rs` | Add `speak_smart`, `word_count`; new tests |
| `src/daemon/dbus.rs` | Replace `speak_summary` call with `speak_smart` |
