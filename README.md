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
