use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DSH_MARKER: &str = "__DSH_BOOT__";
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_PORT: u16 = 3080;

enum ServerState {
    Starting,
    Ready { port: u16 },
    Reused { port: u16 },
    Error(String),
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub state: String,
    pub port: Option<u16>,
    pub error: Option<String>,
    pub pid: Option<u32>,
    pub owned: bool,
}

/// 端口上是否运行着 DSH web 服务（TCP 连接 + HTTP 指纹）。
pub fn is_dsh_running(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(800)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
    if stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 65536];
    let mut total = 0usize;
    loop {
        match stream.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total >= buf.len() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf[..total]).contains(DSH_MARKER)
}

/// 从 start 起第一个无人监听的端口。
pub fn find_free_port(start: u16) -> Option<u16> {
    (start..=u16::MAX).find(|&p| TcpStream::connect(("127.0.0.1", p)).is_err())
}

pub struct ServerManager {
    child: Mutex<Option<Child>>,
    owned: AtomicBool,
    port: Mutex<u16>,
    state: Mutex<ServerState>,
}

impl ServerManager {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            owned: AtomicBool::new(false),
            port: Mutex::new(0),
            state: Mutex::new(ServerState::Starting),
        }
    }
    pub fn start(&self) {}
    pub fn stop(&self) {}
    pub fn restart(&self) {}
    pub fn status(&self) -> StatusSnapshot {
        StatusSnapshot { state: "starting".to_string(), port: None, error: None, pid: None, owned: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// 起一个只回一次 HTTP 响应的假服务，返回端口。
    fn fake_http_server(body: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });
        port
    }

    #[test]
    fn find_free_port_skips_used_and_returns_free() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let used = listener.local_addr().unwrap().port();
        let free = find_free_port(used).unwrap();
        assert!(free > used);
        assert!(TcpStream::connect(("127.0.0.1", free)).is_err());
    }

    #[test]
    fn is_dsh_running_true_when_marker_present() {
        let port = fake_http_server("<!doctype html><html><script>window.__DSH_BOOT__=1</script></html>");
        assert!(is_dsh_running(port));
    }

    #[test]
    fn is_dsh_running_false_for_other_server() {
        let port = fake_http_server("<!doctype html><html><body>hello world</body></html>");
        assert!(!is_dsh_running(port));
    }

    #[test]
    fn is_dsh_running_false_when_nothing_listens() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(!is_dsh_running(port));
    }
}
