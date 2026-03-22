use anyhow::{bail, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use crate::daemon::socket::{Command as SocketCommand, socket_path};

pub fn run(cmd: crate::Command) -> Result<()> {
    let socket_cmd = translate_command(cmd)?;
    let path = socket_path();
    let mut stream = UnixStream::connect(&path)
        .map_err(|_| anyhow::anyhow!("cooee daemon is not running (could not connect to {:?})", path))?;

    let mut line = serde_json::to_string(&socket_cmd)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;

    let mut reader = BufReader::new(&stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: serde_json::Value = serde_json::from_str(response_line.trim())?;
    if let Some(false) = response["ok"].as_bool() {
        let err = response["error"].as_str().unwrap_or("unknown error");
        eprintln!("cooee: {}", err);
        std::process::exit(1);
    }
    if let Some(dnd) = response["dnd"].as_str() {
        println!("DND mode: {}", dnd);
    }
    if let Some(status) = response["status"].as_str() {
        println!("{}", status);
    }
    Ok(())
}

fn translate_command(cmd: crate::Command) -> Result<SocketCommand> {
    Ok(match cmd {
        crate::Command::Speak => SocketCommand::Speak,
        crate::Command::Dnd { mode } => {
            let m = mode.unwrap_or_else(|| "status".to_string());
            SocketCommand::Dnd { mode: m }
        },
        crate::Command::Dismiss => SocketCommand::Dismiss,
        crate::Command::Action => SocketCommand::Action,
        crate::Command::Status => SocketCommand::Status,
        crate::Command::Daemon => bail!("translate_command called with Daemon variant"),
    })
}
