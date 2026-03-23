use serde::{Deserialize, Serialize};
use std::fmt;

/// Urgency level per freedesktop.org spec
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", from = "u8")]
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

impl From<Urgency> for u8 {
    fn from(u: Urgency) -> u8 {
        match u {
            Urgency::Low      => 0,
            Urgency::Normal   => 1,
            Urgency::Critical => 2,
        }
    }
}

impl fmt::Display for Urgency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Urgency::Low      => write!(f, "low"),
            Urgency::Normal   => write!(f, "normal"),
            Urgency::Critical => write!(f, "critical"),
        }
    }
}

/// A single action (key + display label) provided by the sending app
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Raw image data from the `image-data` D-Bus hint (freedesktop spec §image-data).
#[derive(Debug, Clone)]
pub struct ImageData {
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub n_channels: i32,
    pub data: Vec<u8>,
}

/// A received desktop notification
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Timestamp when this notification was received by the daemon
    pub received_at: chrono::DateTime<chrono::Local>,
    /// Structured image data from the `image-data` hint
    #[serde(skip)]
    pub image_data: Option<ImageData>,
    /// File path or icon-theme name from the `image-path` hint
    #[serde(skip)]
    pub image_path: Option<String>,
    /// Non-zero means this notification replaces the one with this ID
    pub replaces_id: u32,
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
        image_data: Option<ImageData>,
        image_path: Option<String>,
        replaces_id: u32,
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
            received_at: chrono::Local::now(),
            image_data,
            image_path,
            replaces_id,
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
        Notification::new(1, "app".into(), "".into(), "Summary".into(), "Body".into(),
            vec![], 1, expire_timeout, None, None, 0)
    }

    #[test]
    fn test_notification_new_with_image_and_replaces() {
        let img = ImageData {
            width: 64, height: 64, rowstride: 64 * 4,
            has_alpha: true, bits_per_sample: 8, n_channels: 4,
            data: vec![0u8; 64 * 64 * 4],
        };
        let n = Notification::new(
            42, "app".into(), "icon".into(), "Hello".into(), "World".into(),
            vec![], 1, 0, Some(img), Some("/path/to/img.png".into()), 7,
        );
        assert_eq!(n.id, 42);
        assert_eq!(n.replaces_id, 7);
        assert!(n.image_data.is_some());
        assert_eq!(n.image_path.as_deref(), Some("/path/to/img.png"));
    }

    #[test]
    fn test_urgency_display() {
        assert_eq!(format!("{}", Urgency::Low),      "low");
        assert_eq!(format!("{}", Urgency::Normal),   "normal");
        assert_eq!(format!("{}", Urgency::Critical), "critical");
        assert_eq!(format!("{}", Urgency::from(0)), "low");
        assert_eq!(format!("{}", Urgency::from(1)), "normal");
        assert_eq!(format!("{}", Urgency::from(2)), "critical");
    }

    #[test]
    fn test_notification_serialises_received_at_not_image_data() {
        let n = make_notification(0);
        let json = serde_json::to_string(&n).unwrap();
        assert!(json.contains("\"received_at\""),
            "expected received_at in JSON, got: {json}");
        assert!(!json.contains("\"image_data\""),
            "image_data should be skipped, got: {json}");
    }
}
