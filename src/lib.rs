pub mod config;
pub mod notification;
pub mod daemon;
pub mod client;

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the notification daemon
    Daemon,
    /// Speak the full body of the last notification
    Speak,
    /// Get or set Do Not Disturb mode
    Dnd {
        /// Mode: off, silent, full, toggle
        mode: Option<String>,
    },
    /// Dismiss the most recently received popup
    Dismiss,
    /// Open picker to invoke an action on the last notification
    Action,
    /// Print daemon status
    Status,
}
