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

/// 在 npx 缓存根目录下查找最新的 dsh 脚本。
pub fn find_dsh_in_npx(root: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let candidate = entry
                .path()
                .join("node_modules/@deepseek-ai/dsh/lib/bin.js");
            if candidate.is_file() {
                let mtime = std::fs::metadata(&candidate).and_then(|m| m.modified()).ok();
                let better = match (&best, mtime) {
                    (None, _) => true,
                    (Some((t, _)), Some(m)) => m > *t,
                    (Some(_), None) => false,
                };
                if better {
                    best = Some((mtime.unwrap_or(std::time::UNIX_EPOCH), candidate));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// 在 PATH 中查找名为 name 的可执行文件。
pub fn find_on_path(path_var: &str, name: &str) -> Option<PathBuf> {
    path_var
        .split(':')
        .map(Path::new)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file() && is_executable(p))
}

/// 返回候选列表中第一个存在且可执行的 node。
pub fn find_node(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|p| p.is_file() && is_executable(p))
        .cloned()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedCommand {
    PathDsh { command: String },
    NodeScript { node: PathBuf, script: PathBuf },
}

pub struct ResolveParams<'a> {
    pub dsh_env: Option<&'a str>,
    pub node_env: Option<&'a str>,
    pub path_var: &'a str,
    pub node_candidates: &'a [PathBuf],
    pub npx_root: &'a Path,
}

/// 解析 dsh 启动命令：环境变量 > PATH 中的 dsh > node + npx 缓存脚本。
pub fn resolve_command(p: &ResolveParams) -> Option<ResolvedCommand> {
    if let Some(cmd) = p.dsh_env {
        if !cmd.is_empty() {
            return Some(ResolvedCommand::PathDsh {
                command: cmd.to_string(),
            });
        }
    }
    if let Some(cmd) = find_on_path(p.path_var, "dsh") {
        return Some(ResolvedCommand::PathDsh {
            command: cmd.to_string_lossy().into_owned(),
        });
    }
    let node = p
        .node_env
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| find_node(p.node_candidates))?;
    let script = find_dsh_in_npx(p.npx_root)?;
    Some(ResolvedCommand::NodeScript { node, script })
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

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dsh-desktop-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn find_dsh_in_npx_picks_newest() {
        let root = temp_dir("npx");
        let a = root.join("aaaa/node_modules/@deepseek-ai/dsh/lib/bin.js");
        let b = root.join("bbbb/node_modules/@deepseek-ai/dsh/lib/bin.js");
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        std::fs::write(&a, "a").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&b, "b").unwrap();
        let found = find_dsh_in_npx(&root).unwrap();
        assert_eq!(found, b);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_on_path_finds_executable() {
        let dir = temp_dir("path");
        make_executable(&dir.join("dsh"));
        let found = find_on_path(&dir.to_string_lossy(), "dsh");
        assert_eq!(found, Some(dir.join("dsh")));
        let missing = find_on_path(&dir.to_string_lossy(), "nope");
        assert_eq!(missing, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_node_returns_first_existing() {
        let dir = temp_dir("node");
        make_executable(&dir.join("node"));
        let found = find_node(&[dir.join("missing"), dir.join("node")]);
        assert_eq!(found, Some(dir.join("node")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_command_prefers_env_then_path_then_npx() {
        let dir = temp_dir("resolve");
        // 1) 环境变量优先
        let r = resolve_command(&ResolveParams {
            dsh_env: Some("/opt/custom/dsh"),
            node_env: None,
            path_var: "",
            node_candidates: &[],
            npx_root: &dir,
        });
        assert_eq!(r, Some(ResolvedCommand::PathDsh { command: "/opt/custom/dsh".to_string() }));
        // 2) PATH 中的 dsh
        make_executable(&dir.join("dsh"));
        let r = resolve_command(&ResolveParams {
            dsh_env: None,
            node_env: None,
            path_var: &dir.to_string_lossy(),
            node_candidates: &[],
            npx_root: &dir,
        });
        assert_eq!(r, Some(ResolvedCommand::PathDsh { command: dir.join("dsh").to_string_lossy().into_owned() }));
        // 3) node + npx 脚本
        make_executable(&dir.join("node"));
        let script_dir = dir.join("n1/node_modules/@deepseek-ai/dsh/lib");
        std::fs::create_dir_all(&script_dir).unwrap();
        std::fs::write(script_dir.join("bin.js"), "#!/usr/bin/env node\n").unwrap();
        let r = resolve_command(&ResolveParams {
            dsh_env: None,
            node_env: None,
            path_var: "",
            node_candidates: &[dir.join("node")],
            npx_root: &dir,
        });
        assert_eq!(
            r,
            Some(ResolvedCommand::NodeScript {
                node: dir.join("node"),
                script: script_dir.join("bin.js"),
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
