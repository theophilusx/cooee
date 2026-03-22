// tests/integration_test.rs
// Starts a minimal socket server and verifies command round-trips.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn start_test_socket_server(socket_path: &std::path::Path) -> thread::JoinHandle<()> {
    let path = socket_path.to_path_buf();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use cooee::daemon::socket::{read_command, write_response, Response, Command};
            use tokio::net::UnixListener;
            let listener = UnixListener::bind(&path).unwrap();
            if let Ok((mut stream, _)) = listener.accept().await {
                let cmd = read_command(&mut stream).await.unwrap();
                let resp = match cmd {
                    Command::Status => Response::ok_status("running | DND: off"),
                    _ => Response::ok(),
                };
                write_response(&mut stream, &resp).await.unwrap();
            }
        });
    })
}

#[test]
fn test_status_command_round_trip() {
    let dir = TempDir::new().unwrap();
    let socket_path = dir.path().join("test_cooee.sock");

    let _server = start_test_socket_server(&socket_path);
    thread::sleep(Duration::from_millis(50)); // let server bind

    let mut stream = UnixStream::connect(&socket_path).unwrap();
    let cmd = cooee::daemon::socket::Command::Status;
    let mut line = serde_json::to_string(&cmd).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).unwrap();

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    let val: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(val["ok"], true);
    assert!(val["status"].as_str().unwrap().contains("running"));
}
