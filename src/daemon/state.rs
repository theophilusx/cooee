use std::sync::{Arc, Mutex};
use crate::config::DndMode;
use crate::notification::Notification;

#[derive(Debug)]
pub struct AppState {
    pub dnd_mode: DndMode,
    pub last_notification: Option<Notification>,
    pub next_id: u32,
}

impl AppState {
    pub fn new(initial_dnd: DndMode) -> Self {
        Self {
            dnd_mode: initial_dnd,
            last_notification: None,
            next_id: 1,
        }
    }

    pub fn next_notification_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn set_dnd(&mut self, mode: DndMode) {
        self.dnd_mode = mode;
    }

    /// Cycle: off → silent → full → off
    pub fn toggle_dnd(&mut self) {
        self.dnd_mode = match self.dnd_mode {
            DndMode::Off => DndMode::Silent,
            DndMode::Silent => DndMode::Full,
            DndMode::Full => DndMode::Off,
        };
    }

    pub fn dnd_mode_str(&self) -> &'static str {
        match self.dnd_mode {
            DndMode::Off => "off",
            DndMode::Silent => "silent",
            DndMode::Full => "full",
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn new_shared_state(initial_dnd: DndMode) -> SharedState {
    Arc::new(Mutex::new(AppState::new(initial_dnd)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_id_increments() {
        let mut state = AppState::new(DndMode::Off);
        assert_eq!(state.next_notification_id(), 1);
        assert_eq!(state.next_notification_id(), 2);
        assert_eq!(state.next_notification_id(), 3);
    }

    #[test]
    fn test_toggle_dnd_cycles() {
        let mut state = AppState::new(DndMode::Off);
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "silent");
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "full");
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "off");
    }

    #[test]
    fn test_set_dnd() {
        let mut state = AppState::new(DndMode::Off);
        state.set_dnd(DndMode::Full);
        assert_eq!(state.dnd_mode_str(), "full");
    }

    #[test]
    fn test_shared_state_mutex() {
        let shared = new_shared_state(DndMode::Off);
        {
            let mut s = shared.lock().unwrap();
            s.toggle_dnd();
        }
        let s = shared.lock().unwrap();
        assert_eq!(s.dnd_mode_str(), "silent");
    }
}
