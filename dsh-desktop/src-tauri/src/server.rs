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

// ---------- 进程管理 ----------

pub fn spawn_server(cmd: &ResolvedCommand, port: u16) -> std::io::Result<Child> {
    let mut c = match cmd {
        ResolvedCommand::PathDsh { command } => Command::new(command),
        ResolvedCommand::NodeScript { node, script } => {
            let mut c = Command::new(node);
            c.arg(script);
            c
        }
    };
    c.args(["web", "--port", &port.to_string()]);
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    c.process_group(0);
    c.spawn()
}

fn process_alive(pid: i32) -> bool {
    // 僵尸进程对 kill(pid, 0) 仍返回 0，无法据此区分；
    // 先以 WNOHANG 收割：返回 pid 说明已退出（僵尸被收割），视为不存活。
    let mut status: libc::c_int = 0;
    let reaped = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if reaped == pid {
        return false;
    }
    unsafe { libc::kill(pid, 0) == 0 }
}

pub fn kill_group(child: &Child) {
    let pid = child.id() as i32;
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !process_alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // 无论直接子进程是否已退出，都对整个进程组补 SIGKILL，
    // 确保无视 SIGTERM 的孙进程也被清理（组已空时 kill 返回 ESRCH，无害）。
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}

// ---------- 管理器 ----------

pub struct ServerManager {
    /// 串行化 start/stop/restart，避免退出与重启竞态（Task 3 质量审查要求）。
    lifecycle: Mutex<()>,
    /// stop() 置位后，in-flight 的 start_inner 轮询立即返回，退出不被启动阻塞。
    stop_requested: AtomicBool,
    child: Mutex<Option<Child>>,
    owned: AtomicBool,
    state: Mutex<ServerState>,
}

impl ServerManager {
    pub fn new() -> Self {
        Self {
            lifecycle: Mutex::new(()),
            stop_requested: AtomicBool::new(false),
            child: Mutex::new(None),
            owned: AtomicBool::new(false),
            state: Mutex::new(ServerState::Starting),
        }
    }

    /// 阻塞式启动：复用检测 → 选端口 → spawn → 轮询就绪。由调用方放入线程。
    pub fn start(&self) {
        self.stop_requested.store(false, Ordering::SeqCst);
        let _guard = self.lifecycle.lock().unwrap();
        self.start_inner();
    }

    /// 仅当服务由本应用启动时才停止（复用中的服务不杀）。
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        let _guard = self.lifecycle.lock().unwrap();
        if self.owned.load(Ordering::SeqCst) {
            self.stop_owned();
        }
    }

    pub fn restart(&self) {
        self.stop_requested.store(false, Ordering::SeqCst);
        let _guard = self.lifecycle.lock().unwrap();
        self.stop_owned();
        self.start_inner();
    }

    fn start_inner(&self) {
        *self.state.lock().unwrap() = ServerState::Starting;
        let default_port: u16 = std::env::var("DSH_DESKTOP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&p| p > 0)
            .unwrap_or(DEFAULT_PORT);
        if is_dsh_running(default_port) {
            *self.state.lock().unwrap() = ServerState::Reused { port: default_port };
            return;
        }
        let Some(port) = find_free_port(default_port) else {
            *self.state.lock().unwrap() = ServerState::Error("没有可用端口".to_string());
            return;
        };
        // 先绑定到局部变量，避免 ResolveParams 借用临时值（E0716）
        let dsh_env = std::env::var("DSH_DESKTOP_DSH").ok();
        let node_env = std::env::var("DSH_DESKTOP_NODE").ok();
        let path_var = std::env::var("PATH").unwrap_or_default();
        let node_candidates = vec![PathBuf::from("/opt/homebrew/bin/node")];
        let npx_root = Path::new(&std::env::var("HOME").unwrap_or_default())
            .join(".npm/_npx");
        let params = ResolveParams {
            dsh_env: dsh_env.as_deref(),
            node_env: node_env.as_deref(),
            path_var: &path_var,
            node_candidates: &node_candidates,
            npx_root: &npx_root,
        };
        let Some(cmd) = resolve_command(&params) else {
            *self.state.lock().unwrap() =
                ServerState::Error("找不到 node/dsh，请设置 DSH_DESKTOP_DSH".to_string());
            return;
        };
        let child = match spawn_server(&cmd, port) {
            Ok(c) => c,
            Err(e) => {
                *self.state.lock().unwrap() = ServerState::Error(format!("启动失败: {e}"));
                return;
            }
        };
        self.owned.store(true, Ordering::SeqCst);
        *self.child.lock().unwrap() = Some(child);
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            // 中止标志：stop() 请求后不再等待就绪，立即退出（in-flight 启动不阻塞退出）。
            // 返回前 stop_owned：stop() 可能发生在 start() 的 store(false) 与拿锁之间，
            // 直接 return 会留下孤儿 dsh 进程。
            if self.stop_requested.load(Ordering::SeqCst) {
                self.stop_owned();
                return;
            }
            if is_dsh_running(port) {
                *self.state.lock().unwrap() = ServerState::Ready { port };
                return;
            }
            // 子进程提前退出 → 快速失败，报出真实原因（避免白等 30s）。
            // 先取结果再释放 guard：不能在持有 self.child 锁时调用 stop_owned（非重入死锁）。
            let exited = self
                .child
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten());
            if let Some(status) = exited {
                self.stop_owned();
                *self.state.lock().unwrap() =
                    ServerState::Error(format!("dsh 进程提前退出: {status}"));
                return;
            }
            if Instant::now() >= deadline {
                self.stop_owned();
                *self.state.lock().unwrap() =
                    ServerState::Error(format!("端口 {port} 等待超时"));
                return;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn stop_owned(&self) {
        if let Some(child) = self.child.lock().unwrap().take() {
            kill_group(&child);
        }
        self.owned.store(false, Ordering::SeqCst);
    }

    pub fn status(&self) -> StatusSnapshot {
        // 存活检查（spec 验收 5：手动 kill 服务后状态栏应转异常）：
        // 同一锁内检测子进程退出并清空句柄，避免后续对已收割 pid 误发信号。
        // 理论上：若并发 start() 在此之后替换了 child，child_exited 会指向旧进程；
        // 实际不可达（gap 为微秒级，而 start_inner 至少耗时数百毫秒），且下一轮轮询即自愈。
        let child_exited = {
            let mut guard = self.child.lock().unwrap();
            let exited = guard
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten())
                .is_some();
            if exited {
                *guard = None;
            }
            exited
        };
        let (state, port, error) = {
            let mut st = self.state.lock().unwrap();
            match &*st {
                ServerState::Ready { port } if child_exited || !is_dsh_running(*port) => {
                    *st = ServerState::Error("服务已停止（进程退出或端口无响应）".to_string());
                    // 仅当确认子进程已退出才移交 owned：探测误报（进程仍存活）时
                    // 保留 owned，重启/退出仍会清理，避免孤儿。
                    if child_exited {
                        self.owned.store(false, Ordering::SeqCst);
                    }
                    (
                        "error".to_string(),
                        None,
                        Some("服务已停止（进程退出或端口无响应）".to_string()),
                    )
                }
                ServerState::Reused { port } if !is_dsh_running(*port) => {
                    *st = ServerState::Error("外部服务已停止".to_string());
                    ("error".to_string(), None, Some("外部服务已停止".to_string()))
                }
                ServerState::Starting => ("starting".to_string(), None, None),
                ServerState::Ready { port } => ("ready".to_string(), Some(*port), None),
                ServerState::Reused { port } => ("reused".to_string(), Some(*port), None),
                ServerState::Error(e) => ("error".to_string(), None, Some(e.clone())),
            }
        };
        let pid = self.child.lock().unwrap().as_ref().map(|c| c.id());
        StatusSnapshot {
            state,
            port,
            error,
            pid,
            owned: self.owned.load(Ordering::SeqCst),
        }
    }
}

// ---------- 更新检查 ----------

/// 从 npm registry JSON 响应中提取版本号（纯字符串解析，避免 JSON 依赖）。
pub fn extract_version_from_registry(body: &str) -> Option<String> {
    const KEY: &str = "\"version\":\"";
    let start = body.find(KEY)? + KEY.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 解析 dsh 启动命令（复用现有逻辑）并查询其版本（`<dsh> --version`，最多等 5s）。
pub fn query_dsh_version() -> Option<String> {
    let npx_root = Path::new(&std::env::var("HOME").unwrap_or_default()).join(".npm/_npx");
    // 先绑定到局部变量，避免 ResolveParams 借用临时值（E0716，同 start_inner）。
    let dsh_env = std::env::var("DSH_DESKTOP_DSH").ok();
    let node_env = std::env::var("DSH_DESKTOP_NODE").ok();
    let path_var = std::env::var("PATH").unwrap_or_default();
    let node_candidates = vec![PathBuf::from("/opt/homebrew/bin/node")];
    let params = ResolveParams {
        dsh_env: dsh_env.as_deref(),
        node_env: node_env.as_deref(),
        path_var: &path_var,
        node_candidates: &node_candidates,
        npx_root: &npx_root,
    };
    let cmd = resolve_command(&params)?;
    let mut child = match &cmd {
        ResolvedCommand::PathDsh { command } => Command::new(command)
            .arg("--version")
            .stdout(Stdio::piped())
            .spawn()
            .ok()?,
        ResolvedCommand::NodeScript { node, script } => Command::new(node)
            .arg(script)
            .arg("--version")
            .stdout(Stdio::piped())
            .spawn()
            .ok()?,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(_) => return None,
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut out = String::new();
    let _ = child.stdout.take().map(|mut s| s.read_to_string(&mut out));
    out.lines().next().map(|s| s.trim().to_string())
}

/// 用系统 curl（macOS 自带）查询官方最新版本；npmjs 失败则回退 npmmirror。
fn fetch_latest_version(url: &str) -> Option<String> {
    let output = Command::new("/usr/bin/curl")
        .args(["--max-time", "5", "-s", url])
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let body = String::from_utf8_lossy(&output.stdout);
    extract_version_from_registry(&body)
}

pub fn query_latest_dsh_version() -> Option<String> {
    const URLS: [&str; 2] = [
        "https://registry.npmjs.org/@deepseek-ai/dsh/latest",
        "https://registry.npmmirror.com/@deepseek-ai/dsh/latest",
    ];
    URLS.iter().find_map(|u| fetch_latest_version(u))
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub local_version: Option<String>,
    pub latest_version: Option<String>,
    pub error: Option<String>,
}

pub fn check_update() -> UpdateInfo {
    let local = query_dsh_version();
    let latest = query_latest_dsh_version();
    UpdateInfo {
        error: match (&local, &latest) {
            (Some(_), Some(_)) => None,
            (None, _) => Some("无法解析本机 dsh 版本".to_string()),
            (_, None) => Some("无法查询官方最新版本（网络？）".to_string()),
        },
        local_version: local,
        latest_version: latest,
    }
}

/// 托盘菜单版本行文案。
pub fn format_update_text(info: &UpdateInfo) -> String {
    match (&info.local_version, &info.latest_version) {
        (Some(local), Some(latest)) if local == latest => {
            format!("dsh 已是最新（v{local}）")
        }
        (Some(local), Some(latest)) => {
            format!("dsh 有更新：v{local} → v{latest}")
        }
        (Some(local), None) => format!("dsh v{local}（最新版本查询失败）"),
        (None, Some(latest)) => format!("本机 dsh 版本未知；官方最新 v{latest}"),
        (None, None) => "dsh 版本检查失败".to_string(),
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

    #[test]
    fn kill_group_terminates_process_group() {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 60 & echo $!; wait")
            .process_group(0)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        // 读取后台孙进程 pid：读到换行即停（2s 上限）。不能用 read_to_string：
        // sleep 60 持有管道写端，take(16) 只限总量、仍会等 EOF 而阻塞 60s。
        let mut out = String::new();
        let mut buf = [0u8; 16];
        let mut stdout = child.stdout.take().unwrap();
        let read_deadline = Instant::now() + Duration::from_secs(2);
        while !out.contains('\n') && Instant::now() < read_deadline {
            let n = stdout.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        let grandchild: i32 = out.trim().parse().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        kill_group(&child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline
            && (process_alive(child.id() as i32) || process_alive(grandchild))
        {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!process_alive(child.id() as i32), "直接子进程应被杀死");
        assert!(!process_alive(grandchild), "进程组内的孙进程应被杀死");
        let _ = child.kill();
    }

    #[test]
    fn extract_version_from_registry_parses() {
        let body = r#"{"bin":{"dsh":"lib/bin.js"},"version":"0.1.0-rc.7","license":"MIT"}"#;
        assert_eq!(
            extract_version_from_registry(body),
            Some("0.1.0-rc.7".to_string())
        );
    }

    #[test]
    fn extract_version_from_registry_returns_none_on_garbage() {
        assert_eq!(extract_version_from_registry("not json at all"), None);
        assert_eq!(extract_version_from_registry(""), None);
    }

    #[test]
    fn format_update_text_covers_all_cases() {
        let mk = |l: Option<&str>, r: Option<&str>| UpdateInfo {
            local_version: l.map(String::from),
            latest_version: r.map(String::from),
            error: None,
        };
        assert_eq!(
            format_update_text(&mk(Some("0.1.0-rc.6"), Some("0.1.0-rc.6"))),
            "dsh 已是最新（v0.1.0-rc.6）"
        );
        assert_eq!(
            format_update_text(&mk(Some("0.1.0-rc.6"), Some("0.1.0-rc.7"))),
            "dsh 有更新：v0.1.0-rc.6 → v0.1.0-rc.7"
        );
        assert_eq!(
            format_update_text(&mk(Some("0.1.0-rc.6"), None)),
            "dsh v0.1.0-rc.6（最新版本查询失败）"
        );
        assert_eq!(
            format_update_text(&mk(None, Some("0.1.0-rc.7"))),
            "本机 dsh 版本未知；官方最新 v0.1.0-rc.7"
        );
        assert_eq!(format_update_text(&mk(None, None)), "dsh 版本检查失败");
    }
}
