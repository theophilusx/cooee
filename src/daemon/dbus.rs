use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use zbus::{interface, Connection, SignalContext};
use crate::notification::Notification;
use crate::config::{Config, DndMode};
use crate::daemon::state::SharedState;
use crate::daemon::ui::UiEvent;
use tokio::sync::mpsc;

pub struct NotificationServer {
    state: SharedState,
    config: Arc<Config>,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    async fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(1);

        // Check DND Full — discard the notification but still emit NotificationClosed
        let dnd_full_id = {
            let mut state = self.state.lock().unwrap();
            if matches!(state.dnd_mode, DndMode::Full) {
                Some(state.next_notification_id())
            } else {
                None
            }
        }; // MutexGuard dropped here before any await
        if let Some(id) = dnd_full_id {
            let _ = Self::notification_closed(&ctx, id, 3).await;
            return id;
        }

        let mut state = self.state.lock().unwrap();

        let id = if replaces_id > 0 { replaces_id } else { state.next_notification_id() };
        let notification = Notification::new(
            id, app_name, app_icon, summary, body, actions, urgency, expire_timeout,
            None, None, replaces_id,
        );
        state.last_notification = Some(notification.clone());
        let is_silent = matches!(state.dnd_mode, DndMode::Silent);
        drop(state);

        if !is_silent {
            let sound = crate::daemon::sound::SoundPlayer::new(self.config.sound.clone());
            sound.play();
            let tts = crate::daemon::tts::TtsClient::new(self.config.tts.clone());
            tts.speak_smart(&notification.summary, &notification.body);
        }

        let _ = self.ui_tx.send(UiEvent::ShowNotification(notification));

        id
    }

    async fn close_notification(
        &mut self,
        id: u32,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) {
        let _ = self.ui_tx.send(UiEvent::CloseNotification(id));
        let _ = Self::notification_closed(&ctx, id, 3).await;
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".into(),
            "body-markup".into(),
            "icon-static".into(),
            "actions".into(),
        ]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "cooee".to_string(),
            "cooee".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(ctx: &SignalContext<'_>, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(ctx: &SignalContext<'_>, id: u32, action_key: String) -> zbus::Result<()>;
}

/// Public helper to emit the `ActionInvoked` D-Bus signal.
pub async fn emit_action_invoked(conn: &Connection, id: u32, action_key: String) -> zbus::Result<()> {
    let iface_ref = conn
        .object_server()
        .interface::<_, NotificationServer>("/org/freedesktop/Notifications")
        .await?;
    let ctx = iface_ref.signal_context();
    NotificationServer::action_invoked(ctx, id, action_key).await
}

pub async fn start_dbus_server(
    state: SharedState,
    config: Arc<Config>,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
) -> Result<Connection> {
    let server = NotificationServer { state, config, ui_tx };
    let conn = zbus::ConnectionBuilder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at("/org/freedesktop/Notifications", server)?
        .build()
        .await?;
    Ok(conn)
}
