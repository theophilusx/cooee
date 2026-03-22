use gtk4::prelude::*;
use gtk4::{gdk, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use crate::config::{Config, Position};
use crate::notification::Notification;
use std::sync::Arc;
use tokio::sync::mpsc;

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
    config: Arc<Config>,
    windows: Vec<(u32, ApplicationWindow)>,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
}

impl NotificationManager {
    pub fn new(config: Arc<Config>, action_tx: mpsc::UnboundedSender<(u32, String)>) -> Self {
        Self { config, windows: Vec::new(), action_tx }
    }

    pub fn show(&mut self, app: &Application, notification: Notification) {
        // Evict oldest if at capacity
        if self.windows.len() >= self.config.general.max_visible {
            if let Some((_, win)) = self.windows.first() {
                win.destroy();
            }
            self.windows.remove(0);
        }
        let win = build_notification_window(app, &notification, &self.config, self.action_tx.clone());
        self.position_window(&win, self.windows.len());
        win.present();
        self.windows.push((notification.id, win));
    }

    pub fn close(&mut self, id: u32) {
        if let Some(pos) = self.windows.iter().position(|(wid, _)| *wid == id) {
            let (_, win) = self.windows.remove(pos);
            win.destroy();
            self.reposition_all();
        }
    }

    pub fn dismiss_latest(&mut self) {
        if let Some((_, win)) = self.windows.last() {
            win.destroy();
        }
        self.windows.pop();
    }

    fn reposition_all(&mut self) {
        for (i, (_, win)) in self.windows.iter().enumerate() {
            self.position_window(win, i);
        }
    }

    fn position_window(&self, win: &ApplicationWindow, stack_index: usize) {
        let cfg = &self.config.general;
        let card_height = 100i32;
        let gap = 8i32;
        let stack_offset = (stack_index as i32) * (card_height + gap);

        match cfg.position {
            Position::TopRight => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Right, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Right, cfg.margin_x);
            }
            Position::TopLeft => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Left, true);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Left, cfg.margin_x);
            }
            Position::BottomRight => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Right, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Right, cfg.margin_x);
            }
            Position::BottomLeft => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Left, true);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, cfg.margin_y + stack_offset);
                win.set_margin(Edge::Left, cfg.margin_x);
            }
            Position::Center => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Bottom, false);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_margin(Edge::Top, cfg.margin_y + stack_offset);
            }
            Position::CenterTop => {
                win.set_anchor(Edge::Top, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Bottom, false);
                win.set_margin(Edge::Top, cfg.margin_y + stack_offset);
            }
            Position::CenterBottom => {
                win.set_anchor(Edge::Bottom, true);
                win.set_anchor(Edge::Left, false);
                win.set_anchor(Edge::Right, false);
                win.set_anchor(Edge::Top, false);
                win.set_margin(Edge::Bottom, cfg.margin_y + stack_offset);
            }
        }
    }
}

fn build_notification_window(
    app: &Application,
    notification: &Notification,
    config: &Config,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
) -> ApplicationWindow {
    let win = ApplicationWindow::new(app);
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

    let vbox = GtkBox::new(Orientation::Vertical, 8);
    vbox.add_css_class("notification-card");

    // Header row: icon + summary
    let hbox = GtkBox::new(Orientation::Horizontal, 8);
    if !notification.app_icon.is_empty() {
        let icon = gtk4::Image::from_icon_name(&notification.app_icon);
        icon.set_pixel_size(config.general.icon_size);
        hbox.append(&icon);
    }
    let summary = Label::new(Some(&notification.summary));
    summary.add_css_class("notification-summary");
    summary.set_xalign(0.0);
    hbox.append(&summary);

    // Dismiss button
    let dismiss_btn = Button::with_label("×");
    dismiss_btn.add_css_class("notification-dismiss");
    hbox.append(&dismiss_btn);
    let win_ref = win.clone();
    dismiss_btn.connect_clicked(move |_| win_ref.destroy());
    vbox.append(&hbox);

    // Body text (supports Pango markup)
    if !notification.body.is_empty() {
        let body = Label::new(None);
        body.set_markup(&notification.body);
        body.set_wrap(true);
        body.set_xalign(0.0);
        body.add_css_class("notification-body");
        vbox.append(&body);
    }

    // Action buttons
    if !notification.actions.is_empty() {
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
        vbox.append(&action_box);
    }

    win.set_child(Some(&vbox));
    win.set_default_width(config.general.width);

    // Auto-dismiss timer
    let win_clone = win.clone();
    let timeout_ms = notification.display_duration_ms(config.general.timeout);
    if let Some(ms) = timeout_ms {
        gtk4::glib::timeout_add_local_once(
            std::time::Duration::from_millis(ms as u64),
            move || win_clone.destroy(),
        );
    }

    win
}
