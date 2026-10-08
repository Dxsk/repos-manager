//! Minimal HTTP/1.1 server for tests, so API clients run against canned
//! responses instead of the network. Also included by `tests/cli.rs`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(body: &str) -> Self {
        Self {
            status: 200,
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn bytes(body: Vec<u8>) -> Self {
        Self { status: 200, body }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            body: b"not found".to_vec(),
        }
    }
}

pub struct Server {
    pub url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Server {
    /// Serve forever on a random local port; `handler` maps a request target
    /// (path and query) to a response. The thread ends with the test process.
    pub fn start(handler: impl Fn(&str) -> Response + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut headers = vec![line.trim_end().to_string()];
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                        break;
                    }
                    headers.push(h.trim_end().to_string());
                }
                let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(headers.join("\n"));
                let res = handler(&target);
                let head = format!(
                    "HTTP/1.1 {} X\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                    res.status,
                    res.body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&res.body);
            }
        });
        Self { url, requests }
    }

    /// Request line and headers of every request received so far.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}
