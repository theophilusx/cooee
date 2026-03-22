use clap::{Parser, Subcommand};

mod config;
mod notification;
mod daemon;
mod client;

#[derive(Parser)]
#[command(name = "cooee", version, about = "Wayland notification daemon")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon => daemon::run(),
        cmd => client::run(cmd),
    }
}
