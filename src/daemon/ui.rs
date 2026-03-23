use gtk4::prelude::*;
use gtk4::{gdk, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use crate::config::{Config, GeneralConfig, Position, SharedConfig};
use crate::notification::Notification;
use tokio::sync::mpsc;

/// Generates the CSS string for notification windows based on config.
pub fn build_css(config: &Config) -> String {
    let fs = config.general.font_size;
    let fs_summary = fs + 1;
    let fs_body = fs - 1;
    format!(r#"
.notification-card {{
    background-color: rgba(28, 28, 36, 0.95);
    border: 1px solid rgba(120, 160, 230, 0.25);
    border-radius: 10px;
    padding: 12px 14px;
    color: #d0d4e8;
    font-size: {fs}px;
}}
.notification-summary {{
    font-weight: bold;
    font-size: {fs_summary}px;
    color: #e0e4f8;
}}
.notification-body {{
    color: #a8b0cc;
    font-size: {fs_body}px;
    margin-top: 2px;
}}
.notification-dismiss {{
    background: transparent;
    border: none;
    color: #606480;
    padding: 0 4px;
    min-width: 0;
    min-height: 0;
    margin-left: auto;
    font-size: {fs}px;
}}
.notification-dismiss:hover {{
    color: #e08090;
    background: transparent;
}}
.notification-actions {{
    margin-top: 6px;
}}
.notification-action-btn {{
    background-color: rgba(120, 160, 230, 0.12);
    border: 1px solid rgba(120, 160, 230, 0.28);
    border-radius: 6px;
    color: #88aaee;
    padding: 3px 10px;
    font-size: {fs_body}px;
}}
.notification-action-btn:hover {{
    background-color: rgba(120, 160, 230, 0.24);
}}
"#)
}

/// Events sent from the async runtime into the GTK main loop
#[derive(Debug)]
pub enum UiEvent {
    ShowNotification(Notification),
    CloseNotification(u32),
    DismissLatest,
    Shutdown,
}

/// Manages the stack of visible notification windows
pub struct NotificationManager {
    config: SharedConfig,
    windows: Vec<(u32, ApplicationWindow)>,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
}

impl NotificationManager {
    pub fn new(config: SharedConfig, action_tx: mpsc::UnboundedSender<(u32, String)>) -> Self {
        Self { config, windows: Vec::new(), action_tx }
    }

    /// Load CSS into the GTK display. Must be called after the GTK display is ready.
    pub fn init_css(&self) {
        if let Some(display) = gdk::Display::default() {
            let provider = gtk4::CssProvider::new();
            let css = {
                let cfg = self.config.read().unwrap();
                build_css(&cfg)
            };
            provider.load_from_string(&css);
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    /// Remove windows that have already been destroyed.
    fn prune_closed(&mut self) {
        self.windows.retain(|(_, win)| win.is_realized());
    }

    pub fn show(&mut self, app: &Application, notification: Notification) {
        self.prune_closed();
        if notification.replaces_id > 0 {
            self.close(notification.replaces_id);
        }
        // Snapshot only the general config fields before GTK work
        let general_snapshot = {
            let cfg = self.config.read().unwrap();
            cfg.general.clone()
        };
        // Evict oldest if at capacity
        if self.windows.len() >= general_snapshot.max_visible {
            if let Some((_, win)) = self.windows.first() {
                win.destroy();
            }
            self.windows.remove(0);
        }
        let win = build_notification_window(app, &notification, &general_snapshot, self.action_tx.clone());
        self.position_window(&win);
        win.present();
        self.windows.push((notification.id, win));
    }

    pub fn close(&mut self, id: u32) {
        if let Some(pos) = self.windows.iter().position(|(wid, _)| *wid == id) {
            let (_, win) = self.windows.remove(pos);
            win.destroy();
        }
    }

    pub fn dismiss_latest(&mut self) {
        self.prune_closed();
        if let Some((_, win)) = self.windows.last() {
            win.destroy();
        }
        self.windows.pop();
    }

    fn position_window(&self, win: &ApplicationWindow) {
        let (position, margin_x, margin_y) = {
            let cfg = self.config.read().unwrap();
            (cfg.general.position.clone(), cfg.general.margin_x, cfg.general.margin_y)
        };

        match position {
            Position::TopRight => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Right, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, margin_y);
                win.set_margin(Edge::Right, margin_x);
            }
            Position::TopLeft => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Left, true);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, margin_y);
                win.set_margin(Edge::Left, margin_x);
            }
            Position::BottomRight => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Right, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, margin_y);
                win.set_margin(Edge::Right, margin_x);
            }
            Position::BottomLeft => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Left, true);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, margin_y);
                win.set_margin(Edge::Left, margin_x);
            }
            Position::Center => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Bottom, false);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_margin(Edge::Top, margin_y);
            }
            Position::CenterTop => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, margin_y);
            }
            Position::CenterBottom => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, margin_y);
            }
        }
    }
}

fn setup_layer_shell(win: &ApplicationWindow) {
    win.init_layer_shell();
    win.set_layer(Layer::Overlay);
    win.set_namespace(Some("cooee"));

    // Set monitor if Hyprland provides one
    if let Some(monitor_name) = crate::daemon::hyprland::active_monitor_name() {
        if let Some(display) = gdk::Display::default() {
            let monitors = display.monitors();
            let n = monitors.n_items();
            for i in 0..n {
                if let Some(obj) = monitors.item(i) {
                    if let Ok(monitor) = obj.downcast::<gdk::Monitor>() {
                        if monitor.connector().as_deref() == Some(monitor_name.as_str()) {
                            win.set_monitor(Some(&monitor));
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn build_header(notification: &Notification, config: &GeneralConfig, win: &ApplicationWindow) -> GtkBox {
    let win = win.clone();
    let hbox = GtkBox::new(Orientation::Horizontal, 8);

    // App icon
    if !notification.app_icon.is_empty() {
        let icon = gtk4::Image::from_icon_name(&notification.app_icon);
        icon.set_pixel_size(config.icon_size);
        hbox.append(&icon);
    }

    // Summary label
    let summary = Label::new(Some(&notification.summary));
    summary.add_css_class("notification-summary");
    summary.set_xalign(0.0);
    hbox.append(&summary);

    // Dismiss button
    let dismiss_btn = Button::with_label("×");
    dismiss_btn.add_css_class("notification-dismiss");
    hbox.append(&dismiss_btn);
    dismiss_btn.connect_clicked(move |_| win.destroy());

    hbox
}

fn build_body(notification: &Notification) -> Option<Label> {
    if notification.body.is_empty() {
        return None;
    }
    let body = Label::new(None);
    body.set_markup(&notification.body);
    body.set_wrap(true);
    body.set_xalign(0.0);
    body.add_css_class("notification-body");
    Some(body)
}

fn build_image(notification: &Notification) -> Option<gtk4::Image> {
    if let Some(d) = &notification.image_data {
        if d.rowstride <= 0 { return None; }
        let stride = d.rowstride as usize;
        let format = if d.has_alpha {
            gtk4::gdk::MemoryFormat::R8g8b8a8
        } else {
            gtk4::gdk::MemoryFormat::R8g8b8
        };
        let texture = gtk4::gdk::MemoryTexture::new(
            d.width,
            d.height,
            format,
            &gtk4::glib::Bytes::from(&d.data),
            stride,
        );
        let image = gtk4::Image::from_paintable(Some(&texture));
        image.add_css_class("notification-image");
        return Some(image);
    }

    if let Some(p) = &notification.image_path {
        let image = if p.starts_with('/') {
            gtk4::Image::from_file(p)
        } else if p.starts_with("file://") {
            gtk4::Image::from_file(&p["file://".len()..])
        } else {
            gtk4::Image::from_icon_name(p)
        };
        image.add_css_class("notification-image");
        return Some(image);
    }

    None
}

fn build_actions(
    notification: &Notification,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
) -> Option<GtkBox> {
    if notification.actions.is_empty() {
        return None;
    }
    let action_box = GtkBox::new(Orientation::Horizontal, 4);
    action_box.add_css_class("notification-actions");
    for action in &notification.actions {
        let btn = Button::with_label(&action.label);
        btn.add_css_class("notification-action-btn");
        let key = action.key.clone();
        let nid = notification.id;
        let tx = action_tx.clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send((nid, key.clone()));
        });
        action_box.append(&btn);
    }
    Some(action_box)
}

fn setup_auto_dismiss(win: &ApplicationWindow, notification: &Notification, config: &GeneralConfig) {
    if let Some(ms) = notification.display_duration_ms(config.timeout) {
        let win = win.clone();
        gtk4::glib::timeout_add_local_once(
            std::time::Duration::from_millis(u64::from(ms)),
            move || win.destroy(),
        );
    }
}

fn build_notification_window(
    app: &Application,
    notification: &Notification,
    config: &GeneralConfig,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
) -> ApplicationWindow {
    let win = ApplicationWindow::new(app);

    setup_layer_shell(&win);

    let vbox = GtkBox::new(Orientation::Vertical, 8);
    vbox.add_css_class("notification-card");

    let header = build_header(notification, config, &win);
    vbox.append(&header);

    if let Some(image) = build_image(notification) {
        vbox.append(&image);
    }

    if let Some(body) = build_body(notification) {
        vbox.append(&body);
    }

    if let Some(actions) = build_actions(notification, action_tx) {
        vbox.append(&actions);
    }

    win.set_child(Some(&vbox));
    win.set_default_width(config.width);

    setup_auto_dismiss(&win, notification, config);

    win
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config_with_font_size(font_size: i32) -> Config {
        let mut cfg = Config::default();
        cfg.general.font_size = font_size;
        cfg
    }

    #[test]
    fn build_css_contains_configured_font_size() {
        let cfg = test_config_with_font_size(18);
        let css = build_css(&cfg);
        assert!(css.contains("font-size: 18px"), "expected 18px in CSS, got:\n{css}");
    }

    #[test]
    fn build_css_summary_is_one_larger_than_base() {
        let cfg = test_config_with_font_size(14);
        let css = build_css(&cfg);
        assert!(css.contains("font-size: 15px"), "summary should be 15px");
    }

    #[test]
    fn build_css_body_is_one_smaller_than_base() {
        let cfg = test_config_with_font_size(14);
        let css = build_css(&cfg);
        assert!(css.contains("font-size: 13px"), "body should be 13px");
    }
}
