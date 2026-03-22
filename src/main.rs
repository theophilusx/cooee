use clap::Parser;
use cooee::Command;

#[derive(Parser)]
#[command(name = "cooee", version, about = "Wayland notification daemon")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon => cooee::daemon::run(),
        cmd => cooee::client::run(cmd),
    }
}
