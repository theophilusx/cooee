pub mod state;
pub mod socket;
pub mod sound;
pub mod tts;
pub mod hyprland;
pub mod ui;
pub mod dbus;
pub mod action_picker;

use anyhow::Result;
use gtk4::prelude::*;
use gtk4::Application;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::config::{Config, DndMode};
use crate::daemon::state::new_shared_state;
use crate::daemon::ui::{NotificationManager, UiEvent};

const APP_ID: &str = "org.theophilusx.cooee";

/// A thread-safe queue used to bridge events from the tokio runtime into the GTK main loop.
type EventQueue = Arc<Mutex<VecDeque<UiEvent>>>;

pub fn run() -> Result<()> {
    let config = Arc::new(Config::load()?);

    let initial_dnd = config.dnd.mode.clone();
    let shared_state = new_shared_state(initial_dnd);

    // Channel: D-Bus server → tokio bridge task → glib main loop
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiEvent>();
    // Channel: socket Action handler → D-Bus ActionInvoked emitter
    let (action_tx, action_rx) = mpsc::unbounded_channel::<(u32, String)>();
    let action_tx_for_socket = action_tx.clone();

    // Shared queue: tokio threads push events; glib idle handler drains them in the GTK thread.
    let event_queue: EventQueue = Arc::new(Mutex::new(VecDeque::new()));
    let event_queue_for_tokio = event_queue.clone();
    let event_queue_for_socket = event_queue.clone();

    let state_for_tokio = shared_state.clone();
    let config_for_tokio = config.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            // Bridge: forward UiEvents from tokio mpsc → shared queue
            let queue_bridge = event_queue_for_tokio.clone();
            tokio::spawn(async move {
                let mut rx = ui_rx;
                while let Some(event) = rx.recv().await {
                    queue_bridge.lock().unwrap().push_back(event);
                }
            });

            // Start D-Bus server
            let conn = match dbus::start_dbus_server(
                state_for_tokio.clone(),
                config_for_tokio.clone(),
                ui_tx,
            ).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("cooee: D-Bus server failed to start: {}", e);
                    return;
                }
            };

            // Forward ActionInvoked signals via D-Bus
            tokio::spawn(async move {
                let mut rx = action_rx;
                while let Some((id, key)) = rx.recv().await {
                    let _ = dbus::emit_action_invoked(&conn, id, key).await;
                }
            });

            // Start Unix socket server
            socket_server(
                state_for_tokio,
                config_for_tokio,
                event_queue_for_socket,
                action_tx_for_socket,
            ).await;
        });
    });

    // Start GTK application
    let app = Application::builder().application_id(APP_ID).build();
    let config_for_gtk = config.clone();

    app.connect_activate(move |app| {
        let manager = Arc::new(Mutex::new(NotificationManager::new(config_for_gtk.clone())));
        let app_clone = app.clone();
        let queue = event_queue.clone();

        // Install an idle handler that drains the event queue each time it runs.
        gtk4::glib::idle_add_local(move || {
            let events: Vec<UiEvent> = {
                let mut q = queue.lock().unwrap();
                q.drain(..).collect()
            };
            let mut mgr = manager.lock().unwrap();
            for event in events {
                match event {
                    UiEvent::ShowNotification(n) => mgr.show(&app_clone, n),
                    UiEvent::CloseNotification(id) => mgr.close(id),
                    UiEvent::DismissLatest => mgr.dismiss_latest(),
                    UiEvent::Shutdown => app_clone.quit(),
                }
            }
            gtk4::glib::ControlFlow::Continue
        });
    });

    app.run();
    Ok(())
}

async fn socket_server(
    state: crate::daemon::state::SharedState,
    config: Arc<Config>,
    event_queue: EventQueue,
    action_tx: mpsc::UnboundedSender<(u32, String)>,
) {
    use tokio::net::UnixListener;
    use crate::daemon::socket::{socket_path, read_command, write_response};

    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cooee: failed to bind socket {:?}: {}", path, e);
            return;
        }
    };

    loop {
        if let Ok((mut stream, _)) = listener.accept().await {
            let state = state.clone();
            let config = config.clone();
            let event_queue = event_queue.clone();
            let action_tx = action_tx.clone();
            tokio::spawn(async move {
                let cmd = match read_command(&mut stream).await {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let response = handle_command(cmd, &state, &config, &event_queue, &action_tx).await;
                let _ = write_response(&mut stream, &response).await;
            });
        }
    }
}

async fn handle_command(
    cmd: crate::daemon::socket::Command,
    state: &crate::daemon::state::SharedState,
    config: &Config,
    event_queue: &EventQueue,
    action_tx: &mpsc::UnboundedSender<(u32, String)>,
) -> crate::daemon::socket::Response {
    use crate::daemon::socket::{Command, Response};

    match cmd {
        Command::Speak => {
            let notification = state.lock().unwrap().last_notification.clone();
            match notification {
                None => Response::err("no notification to speak"),
                Some(n) => {
                    let tts = tts::TtsClient::new(config.tts.clone());
                    tts.speak_body(&n.body);
                    Response::ok()
                }
            }
        }
        Command::Dnd { mode } => {
            if mode.as_str() == "status" {
                let s = state.lock().unwrap();
                return Response::ok_dnd(s.dnd_mode_str());
            }
            let mut s = state.lock().unwrap();
            match mode.as_str() {
                "off" => s.set_dnd(DndMode::Off),
                "silent" => s.set_dnd(DndMode::Silent),
                "full" => s.set_dnd(DndMode::Full),
                "toggle" => s.toggle_dnd(),
                _ => return Response::err("unknown DND mode: use off, silent, full, or toggle"),
            }
            let mode_str = s.dnd_mode_str().to_string();
            Response::ok_dnd(&mode_str)
        }
        Command::Dismiss => {
            event_queue.lock().unwrap().push_back(UiEvent::DismissLatest);
            Response::ok()
        }
        Command::Action => {
            let notification = state.lock().unwrap().last_notification.clone();
            match notification {
                None => Response::err("no notification to act on"),
                Some(n) => {
                    if n.actions.is_empty() {
                        return Response::err("last notification has no actions");
                    }
                    match action_picker::pick_action(&config.actions.picker, &n.actions) {
                        Ok(action) => {
                            let _ = action_tx.send((n.id, action.key));
                            Response::ok()
                        }
                        Err(e) => Response::err(&e.to_string()),
                    }
                }
            }
        }
        Command::Status => {
            let s = state.lock().unwrap();
            let status = format!("running | DND: {}", s.dnd_mode_str());
            Response::ok_status(&status)
        }
    }
}
