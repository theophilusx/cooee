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

use crate::config::{Config, DndMode, SharedConfig};
use crate::daemon::state::new_shared_state;
use crate::daemon::ui::{build_css, NotificationManager, UiEvent};

const APP_ID: &str = "org.theophilusx.cooee";

/// A thread-safe queue used to bridge events from the tokio runtime into the GTK main loop.
type EventQueue = Arc<Mutex<VecDeque<UiEvent>>>;

pub fn run() -> Result<()> {
    let config = Config::load()?.shared();

    // Write ~/.config/cooee/style.css on first run so users can customise it.
    let default_css = build_css(&config.read().unwrap());
    if let Err(e) = Config::ensure_default_style(&default_css) {
        eprintln!("cooee: could not write default style.css: {e}");
    }

    let initial_dnd = config.read().unwrap().dnd.mode.clone();
    let history_max = config.read().unwrap().history.max_entries;
    let shared_state = new_shared_state(initial_dnd, history_max);

    // Channel: D-Bus server → tokio bridge task → glib main loop
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiEvent>();
    // Channel: socket Action handler → D-Bus ActionInvoked emitter
    let (action_tx, action_rx) = mpsc::unbounded_channel::<(u32, String)>();
    let action_tx_for_socket = action_tx.clone();
    let action_tx_for_gtk = action_tx.clone();

    // Shared queue: tokio threads push events; glib timer handler drains them in the GTK thread.
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

            // Watch config file for changes; reload SharedConfig and trigger CSS refresh.
            {
                let shared_config = config_for_tokio.clone();
                let event_queue = event_queue_for_tokio.clone();
                tokio::task::spawn_blocking(move || {
                    use notify::Watcher;
                    let (tx, rx) = std::sync::mpsc::channel();
                    let mut watcher = match notify::recommended_watcher(tx) {
                        Ok(w) => w,
                        Err(e) => {
                            eprintln!("cooee: config watcher unavailable: {e}");
                            return;
                        }
                    };
                    watcher.watch(&Config::config_path(), notify::RecursiveMode::NonRecursive).ok();
                    watcher.watch(&Config::style_path(), notify::RecursiveMode::NonRecursive).ok();
                    let style_path = Config::style_path();
                    loop {
                        match rx.recv() {
                            Ok(Ok(event)) if matches!(event.kind, notify::EventKind::Modify(_)) => {
                                if event.paths.iter().any(|p| p == &style_path) {
                                    // Style file changed — just refresh CSS, no config reload needed.
                                    event_queue.lock().unwrap().push_back(UiEvent::ReloadCss);
                                } else {
                                    match Config::load() {
                                        Ok(new_cfg) => {
                                            *shared_config.write().unwrap() = new_cfg;
                                            event_queue.lock().unwrap().push_back(UiEvent::ReloadCss);
                                        }
                                        Err(e) => eprintln!("cooee: config reload failed: {e}"),
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                });
            }

            // Start Unix socket server
            socket_server(
                state_for_tokio,
                config_for_tokio,
                event_queue_for_socket,
                action_tx_for_socket,
            ).await;
        });
    });

    // Start GTK application.
    let app = Application::builder().application_id(APP_ID).build();
    let config_for_gtk = config.clone();

    // Permanently hold the application — it must never auto-quit between notifications.
    // The guard must be stored; dropping it would release the hold immediately (RAII).
    let hold_guard = std::rc::Rc::new(std::cell::RefCell::new(None));
    let hold_guard_for_startup = hold_guard.clone();
    app.connect_startup(move |app| {
        *hold_guard_for_startup.borrow_mut() = Some(app.hold());
    });

    app.connect_activate(move |app| {
        let mgr_instance = NotificationManager::new(config_for_gtk.clone(), action_tx_for_gtk.clone());
        mgr_instance.init_css();
        let manager = Arc::new(Mutex::new(mgr_instance));
        let app_clone = app.clone();
        let queue = event_queue.clone();

        // Poll for queued events at a fixed interval. Using timeout_add_local (not idle_add_local)
        // avoids a busy-wait: idle_add_local with ControlFlow::Continue would spin at 100% CPU
        // whenever the GTK main loop has no other work, whereas this fires at most 100 times/sec.
        gtk4::glib::timeout_add_local(
            std::time::Duration::from_millis(10),
            move || {
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
                        UiEvent::ReloadCss => mgr.init_css(),
                    }
                }
                gtk4::glib::ControlFlow::Continue
            },
        );
    });

    app.run_with_args::<&str>(&[]);
    Ok(())
}

async fn socket_server(
    state: crate::daemon::state::SharedState,
    config: SharedConfig,
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
    config: &SharedConfig,
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
                    let tts_config = config.read().unwrap().tts.clone();
                    let tts = tts::TtsClient::new(tts_config);
                    let text = if n.body.is_empty() { &n.summary } else { &n.body };
                    tts.speak_body(text);
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
                    let picker = config.read().unwrap().actions.picker.clone();
                    match action_picker::pick_action(&picker, &n.actions) {
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
        Command::History { count } => {
            let entries = state.lock().unwrap()
                .get_history(count)
                .into_iter()
                .cloned()
                .collect();
            Response::ok_history(entries)
        }
    }
}
