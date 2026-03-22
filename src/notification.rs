/// Urgency level per freedesktop.org spec
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low = 0,
    Normal = 1,
    Critical = 2,
}

impl From<u8> for Urgency {
    fn from(v: u8) -> Self {
        match v {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

/// A single action (key + display label) provided by the sending app
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// Parses the flat `[key, label, key, label, ...]` list from D-Bus into `Vec<Action>`
pub fn parse_actions(flat: &[String]) -> Vec<Action> {
    flat.chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                Some(Action {
                    key: chunk[0].clone(),
                    label: chunk[1].clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// A received desktop notification
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<Action>,
    pub urgency: Urgency,
    /// Expiry in milliseconds; 0 = server default, -1 = persistent
    pub expire_timeout: i32,
    /// Raw image data from hints (optional)
    pub image_data: Option<Vec<u8>>,
}

impl Notification {
    pub fn new(
        id: u32,
        app_name: String,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        urgency: u8,
        expire_timeout: i32,
        image_data: Option<Vec<u8>>,
    ) -> Self {
        Self {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions: parse_actions(&actions),
            urgency: Urgency::from(urgency),
            expire_timeout,
            image_data,
        }
    }

    /// Effective display duration in ms.
    ///
    /// Both `0` and `-1` map to `default_ms`. Although the FDO spec assigns different
    /// meanings to these two values, libnotify (used by virtually every app) sends `-1`
    /// to mean "use the server's default timeout", so treating `-1` as persistent would
    /// cause most notifications to never auto-close. This matches dunst/mako behaviour.
    pub fn display_duration_ms(&self, default_ms: u32) -> Option<u32> {
        match self.expire_timeout {
            n if n <= 0 => Some(default_ms),
            n => Some(n as u32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_actions_even_pairs() {
        let flat = vec![
            "default".to_string(), "Open".to_string(),
            "snooze".to_string(), "Snooze 5 min".to_string(),
        ];
        let actions = parse_actions(&flat);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].key, "default");
        assert_eq!(actions[0].label, "Open");
        assert_eq!(actions[1].key, "snooze");
        assert_eq!(actions[1].label, "Snooze 5 min");
    }

    #[test]
    fn test_parse_actions_odd_list_ignores_trailing() {
        let flat = vec!["orphan".to_string()];
        let actions = parse_actions(&flat);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_parse_actions_empty() {
        let actions = parse_actions(&[]);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_urgency_from_u8() {
        assert_eq!(Urgency::from(0), Urgency::Low);
        assert_eq!(Urgency::from(1), Urgency::Normal);
        assert_eq!(Urgency::from(2), Urgency::Critical);
        assert_eq!(Urgency::from(99), Urgency::Normal);
    }

    #[test]
    fn test_display_duration_default() {
        let n = make_notification(0);
        assert_eq!(n.display_duration_ms(5000), Some(5000));
    }

    #[test]
    fn test_display_duration_negative_one_uses_default() {
        // libnotify sends -1 to mean "use server default" — must not be treated as persistent
        let n = make_notification(-1);
        assert_eq!(n.display_duration_ms(5000), Some(5000));
    }

    #[test]
    fn test_display_duration_explicit() {
        let n = make_notification(3000);
        assert_eq!(n.display_duration_ms(5000), Some(3000));
    }

    fn make_notification(expire_timeout: i32) -> Notification {
        Notification::new(
            1, "app".into(), "".into(), "Summary".into(), "Body".into(),
            vec![], 1, expire_timeout, None,
        )
    }
}
