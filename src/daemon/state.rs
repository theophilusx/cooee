use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use crate::config::DndMode;
use crate::notification::Notification;

#[derive(Debug)]
pub struct AppState {
    pub dnd_mode: DndMode,
    pub last_notification: Option<Notification>,
    pub next_id: u32,
    pub history: VecDeque<Notification>,
    pub history_max: usize,
}

impl AppState {
    pub fn new(initial_dnd: DndMode, history_max: usize) -> Self {
        Self {
            dnd_mode: initial_dnd,
            last_notification: None,
            next_id: 1,
            history: VecDeque::new(),
            history_max,
        }
    }

    pub fn next_notification_id(&mut self) -> u32 {
        let id = self.next_id;
        // wrapping_add then max(1) ensures we never hand out ID 0 (reserved by spec)
        self.next_id = self.next_id.wrapping_add(1).max(1);
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

    /// Append or replace in-place; trims oldest entry when at capacity. No-op if history_max == 0.
    pub fn push_history(&mut self, n: Notification) {
        if self.history_max == 0 {
            return;
        }
        if n.replaces_id > 0 {
            if let Some(pos) = self.history.iter().position(|e| e.id == n.replaces_id) {
                self.history[pos] = n;
                return;
            }
        }
        if self.history.len() >= self.history_max {
            self.history.pop_front();
        }
        self.history.push_back(n);
    }

    /// Returns up to `count` most recent entries, newest first.
    pub fn get_history(&self, count: Option<usize>) -> Vec<&Notification> {
        let iter = self.history.iter().rev();
        match count {
            Some(n) => iter.take(n).collect(),
            None    => iter.collect(),
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn new_shared_state(initial_dnd: DndMode, history_max: usize) -> SharedState {
    Arc::new(Mutex::new(AppState::new(initial_dnd, history_max)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_id_increments() {
        let mut state = AppState::new(DndMode::Off, 50);
        assert_eq!(state.next_notification_id(), 1);
        assert_eq!(state.next_notification_id(), 2);
        assert_eq!(state.next_notification_id(), 3);
    }

    #[test]
    fn test_toggle_dnd_cycles() {
        let mut state = AppState::new(DndMode::Off, 50);
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "silent");
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "full");
        state.toggle_dnd();
        assert_eq!(state.dnd_mode_str(), "off");
    }

    #[test]
    fn test_set_dnd() {
        let mut state = AppState::new(DndMode::Off, 50);
        state.set_dnd(DndMode::Full);
        assert_eq!(state.dnd_mode_str(), "full");
    }

    #[test]
    fn test_shared_state_mutex() {
        let shared = new_shared_state(DndMode::Off, 50);
        {
            let mut s = shared.lock().unwrap();
            s.toggle_dnd();
        }
        let s = shared.lock().unwrap();
        assert_eq!(s.dnd_mode_str(), "silent");
    }

    fn make_notif(id: u32, replaces_id: u32) -> Notification {
        Notification::new(
            id, format!("app{id}"), "".into(), format!("summary {id}"), "".into(),
            vec![], 1, 0, None, None, replaces_id,
        )
    }

    #[test]
    fn test_push_history_single_entry() {
        let mut state = AppState::new(DndMode::Off, 50);
        state.push_history(make_notif(1, 0));
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].id, 1);
    }

    #[test]
    fn test_push_history_capacity_eviction() {
        let mut state = AppState::new(DndMode::Off, 2);
        state.push_history(make_notif(1, 0));
        state.push_history(make_notif(2, 0));
        state.push_history(make_notif(3, 0));
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[0].id, 2);
        assert_eq!(state.history[1].id, 3);
    }

    #[test]
    fn test_push_history_replaces_id_dedup_existing() {
        let mut state = AppState::new(DndMode::Off, 50);
        state.push_history(make_notif(1, 0));
        state.push_history(make_notif(2, 0));
        let replacement = make_notif(1, 1); // replaces_id=1
        state.push_history(replacement);
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[0].id, 1);
        assert_eq!(state.history[0].summary, "summary 1");
    }

    #[test]
    fn test_push_history_replaces_id_no_existing_entry_appends() {
        let mut state = AppState::new(DndMode::Off, 50);
        state.push_history(make_notif(1, 0));
        state.push_history(make_notif(2, 99)); // replaces_id=99 not in history
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[1].id, 2);
    }

    #[test]
    fn test_push_history_history_max_zero_noop() {
        let mut state = AppState::new(DndMode::Off, 0);
        state.push_history(make_notif(1, 0));
        assert!(state.history.is_empty());
    }

    #[test]
    fn test_get_history_none_returns_all_newest_first() {
        let mut state = AppState::new(DndMode::Off, 50);
        state.push_history(make_notif(1, 0));
        state.push_history(make_notif(2, 0));
        state.push_history(make_notif(3, 0));
        let entries = state.get_history(None);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, 3);
        assert_eq!(entries[1].id, 2);
        assert_eq!(entries[2].id, 1);
    }

    #[test]
    fn test_get_history_some_limits_count() {
        let mut state = AppState::new(DndMode::Off, 50);
        state.push_history(make_notif(1, 0));
        state.push_history(make_notif(2, 0));
        state.push_history(make_notif(3, 0));
        let entries = state.get_history(Some(2));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, 3);
        assert_eq!(entries[1].id, 2);
    }

    #[test]
    fn test_get_history_empty() {
        let state = AppState::new(DndMode::Off, 50);
        let entries = state.get_history(None);
        assert!(entries.is_empty());
    }
}
