# cooee

Wayland desktop notification daemon for Hyprland with sound and TTS.

Implements the [freedesktop.org Desktop Notifications Specification v1.2](https://specifications.freedesktop.org/notification-spec/notification-spec-latest.html). Notifications appear as GTK4 card popups on the active monitor, with optional sound and text-to-speech via speech-dispatcher.

## Prerequisites

**System packages** (install before building):

Fedora:
```bash
sudo dnf install gtk4-devel gtk4-layer-shell-devel speech-dispatcher-devel
```

Arch Linux:
```bash
sudo pacman -S gtk4 gtk4-layer-shell speech-dispatcher
```

Debian / Ubuntu:
```bash
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev libspeechd-dev
```

**Rust toolchain** — install via [rustup](https://rustup.rs) if not already present:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
Minimum supported Rust version: stable 1.75.

**speech-dispatcher** — required for TTS. Install the package (above) and no further setup is needed: speech-dispatcher starts automatically on first use via client-side autospawn.

**`~/.local/bin` on your PATH** — the default install prefix puts the binary there:
```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc   # or ~/.zshrc
```

## Install

```bash
make install
```

This does:
1. `cargo build --release` — compile the binary
2. Install `cooee` to `~/.local/bin/cooee`
3. Install `data/cooee.service` to `~/.config/systemd/user/cooee.service`
4. Install `data/sounds/notify.ogg` to `~/.config/cooee/sounds/notify.ogg`
5. `systemctl --user daemon-reload && systemctl --user enable --now cooee`

**Manual alternative** (if you prefer explicit control):
```bash
cargo build --release
install -Dm755 target/release/cooee ~/.local/bin/cooee
install -Dm644 data/cooee.service ~/.config/systemd/user/cooee.service
install -Dm644 data/sounds/notify.ogg ~/.config/cooee/sounds/notify.ogg
systemctl --user daemon-reload
systemctl --user enable --now cooee
```

**Uninstall:**
```bash
make uninstall
```

## Systemd + Hyprland setup

cooee uses `graphical-session.target` — the standard systemd target for session-scoped graphical services. It starts automatically at login as long as `graphical-session.target` is activated by your Hyprland session.

### Using uwsm (recommended)

If you launch Hyprland via [uwsm](https://github.com/Vladimir-csp/uwsm), `graphical-session.target` is activated automatically. cooee will start on login with no additional configuration.

Verify uwsm is managing your session:
```bash
systemctl --user status graphical-session.target
```
Expected: `active (exited)`.

### Using plain `exec Hyprland`

If you start Hyprland with a bare `exec Hyprland` in `.bash_profile`, `.zprofile`, or a display manager session file, `graphical-session.target` may not be activated. In that case, add this to your `~/.config/hypr/hyprland.conf`:

```
exec-once = systemctl --user start graphical-session.target
```

Or switch to uwsm for a fully integrated session.

### Conflict with other notification daemons

Only one process can own `org.freedesktop.Notifications` on the D-Bus session bus. If mako, dunst, swaync, or another daemon is already running, cooee will fail to start.

Disable any existing daemon before enabling cooee:
```bash
systemctl --user disable --now mako    # or dunst, swaync, etc.
```

### Verifying the service

```bash
systemctl --user status cooee          # check running state
journalctl --user -u cooee -f          # follow logs
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

`~/.config/cooee/config.toml` is written on first run with defaults. Edit it while the daemon is running — cooee reloads the file automatically when it changes. `[dnd].mode` is runtime state managed via the `cooee dnd` command and is not overwritten by a config reload.

```toml
[general]
position = "top-right"   # where popups appear
                         # values: top-right | top-left | bottom-right | bottom-left
                         #         center | center-top | center-bottom
margin_x = 16            # horizontal gap from screen edge, in pixels
margin_y = 48            # vertical gap from screen edge, in pixels
max_visible = 5          # maximum number of popups stacked at once
                         # oldest popup is dismissed when the limit is reached
timeout = 5000           # how long each popup stays visible, in milliseconds
                         # 0 = persistent (never auto-dismiss)
icon_size = 36           # app icon size in pixels
width = 360              # notification card width in pixels

[sound]
enabled = true
file = "~/.config/cooee/sounds/notify.ogg"
                         # path to sound file; supports .ogg, .wav, .mp3
                         # ~ is expanded to $HOME
volume = 0.8             # playback volume: 0.0 (silent) to 1.0 (full)

[tts]
enabled = true
speak_summary = true     # speak the notification summary automatically on arrival
voice = ""               # speech-dispatcher voice name
                         # empty string = use the system default voice
                         # run `spd-say --list-synthesis-voices` to see available voices
rate = 0                 # speech rate: -100 (slowest) to 100 (fastest), 0 = default

[dnd]
mode = "off"             # initial Do Not Disturb mode on daemon start
                         # values: off | silent | full
                         # change at runtime with: cooee dnd <mode>

[actions]
picker = "rofi -dmenu -p 'Action:'"
                         # command used to display notification action choices
                         # cooee writes one action label per line to stdin
                         # the selected label is read from stdout
                         # other compatible pickers: "wofi --dmenu", "fzf"
```

### Do Not Disturb modes

| Mode | Popups | Sound | TTS |
|---|---|---|---|
| `off` | yes | yes | yes |
| `silent` | yes | no | no |
| `full` | no (discarded) | no | no |

Toggle with `cooee dnd toggle` (cycles: off → silent → full → off).

## CLI reference

| Command | Description |
|---|---|
| `cooee daemon` | Start the notification daemon |
| `cooee speak` | Speak the full body of the last notification via TTS |
| `cooee dnd [off\|silent\|full\|toggle]` | Set DND mode; no argument prints the current mode |
| `cooee dismiss` | Dismiss the most recently received popup |
| `cooee action` | Open the action picker for the last notification |
| `cooee status` | Print daemon status and current DND mode |

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `cooee daemon is not running` | Daemon not started | `systemctl --user start cooee` |
| No sound | Sound file missing | Check `[sound].file`; ensure `~/.config/cooee/sounds/notify.ogg` exists |
| No sound (file exists) | Audio device unavailable | Run `cooee daemon` in a terminal to see rodio error details |
| No TTS | speech-dispatcher not installed | Install `speech-dispatcher` package and ensure `DisableAutoSpawn` is not set in `speechd.conf` |
| Notifications not appearing | Another daemon owns D-Bus name | `systemctl --user stop mako dunst swaync`; then `systemctl --user restart cooee` |
| Service not starting on login | `graphical-session.target` not activated | Use uwsm, or add `exec-once = systemctl --user start graphical-session.target` to `hyprland.conf` |

## License

GPLv3 — see LICENSE.
