//! The server exits on `exit` even while the client keeps stdin open, as
//! Neovim does; it used to wait for stdin to close, and the client for it.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vale-ls"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("vale-ls should start");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut server = Server {
            child,
            stdin,
            stdout,
        };

        server.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"capabilities": {}, "processId": null, "rootUri": null}}));
        server.reply_to(1);
        server.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
        server
    }

    fn send(&mut self, msg: Value) {
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Reads messages until the response to request `id`.
    fn reply_to(&mut self, id: u64) -> Value {
        loop {
            let mut len = 0;
            loop {
                let mut line = String::new();
                self.stdout.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                    len = n.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            self.stdout.read_exact(&mut body).unwrap();
            let msg: Value = serde_json::from_slice(&body).unwrap();
            if msg["id"] == id && msg.get("method").is_none() {
                return msg;
            }
        }
    }

    /// Waits up to `timeout` for the process to exit, with stdin still open.
    fn exit_code(&mut self, timeout: Duration) -> Option<i32> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn exits_zero_on_exit_after_shutdown() {
    let mut server = Server::start();
    server.send(json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}));
    assert!(server.reply_to(2).get("result").is_some());
    server.send(json!({"jsonrpc": "2.0", "method": "exit"}));
    assert_eq!(server.exit_code(Duration::from_secs(5)), Some(0));
}

#[test]
fn exits_one_on_exit_without_shutdown() {
    let mut server = Server::start();
    server.send(json!({"jsonrpc": "2.0", "method": "exit"}));
    assert_eq!(server.exit_code(Duration::from_secs(5)), Some(1));
}

#[test]
fn stays_up_after_shutdown_until_exit() {
    let mut server = Server::start();
    server.send(json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}));
    server.reply_to(2);
    assert_eq!(server.exit_code(Duration::from_millis(500)), None);
}
