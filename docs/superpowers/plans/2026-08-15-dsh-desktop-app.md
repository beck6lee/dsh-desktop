# DSH 桌面应用（Tauri v2）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `dsh-workspace/dsh-desktop/` 下构建一个 macOS Tauri v2 桌面应用：双击启动时自动拉起 `dsh web` 服务并以原生窗口展示 DSH Web UI，退出时自动停止服务，窗口顶部带服务状态栏（启动中/已就绪/复用/异常 + 重启按钮），Dock 图标为鲸鱼娘。

**Architecture:** Tauri v2（Rust）壳 + `src/` 纯静态前端（状态栏 + iframe 指向 `http://127.0.0.1:<port>`）。Rust 侧 `ServerManager` 负责解析 dsh 命令、spawn/探测/杀进程组；前端经 `window.__TAURI__.core.invoke` 每 2s 轮询 `server_status`。图标用 sharp 合成 1024×1024 透明底源图后由 `tauri icon` 生成 .icns。

**Tech Stack:** Rust (tauri 2 / serde / libc)、Node（@tauri-apps/cli、sharp）、macOS 系统工具（sips 仅作备用）。无前端框架、无 bundler。

**前置说明：**
- 首次 `cargo` 编译需下载编译大量 crate，约 5–15 分钟，务必用后台任务执行。
- 当前 3080 端口正被本会话的 DSH 占用 → 应用会走「复用」路径；「自启自停」路径用 `DSH_DESKTOP_PORT=3999` 验收（见 Task 9），避免误杀正在运行的会话。
- 所有 `cd` 均在 `dsh-desktop/` 下执行；git 在 `dsh-workspace/` 根目录。

---

### Task 1: 项目脚手架与依赖安装

**Files:**
- Create: `dsh-desktop/package.json`
- Create: `dsh-desktop/scripts/make-icon-source.mjs`

- [ ] **Step 1: 创建目录与 package.json**

```bash
mkdir -p /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src \
         /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/scripts \
         /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri/src \
         /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri/icons
```

写入 `dsh-desktop/package.json`（完整内容）：

```json
{
  "name": "dsh-desktop",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "icons:source": "node scripts/make-icon-source.mjs",
    "icons": "npm run icons:source && tauri icon icons/icon-source.png",
    "build": "tauri build"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.5.0",
    "sharp": "^0.34.1"
  }
}
```

- [ ] **Step 2: 安装依赖**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop && npm install`
Expected: 正常结束，`node_modules/@tauri-apps/cli` 与 `node_modules/sharp` 存在（`ls node_modules/.bin/tauri` 可见）。

- [ ] **Step 3: 写图标合成脚本**

写入 `dsh-desktop/scripts/make-icon-source.mjs`（完整内容）：

```js
import sharp from 'sharp';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const src = path.resolve(here, '../../assets/whale-girl/whale-girl-transparent.png');
const outDir = path.resolve(here, '../icons');
await mkdir(outDir, { recursive: true });
await sharp(src)
  .resize(1024, 1024, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .png()
  .toFile(path.join(outDir, 'icon-source.png'));
console.log('icon source written:', path.join(outDir, 'icon-source.png'));
```

- [ ] **Step 4: 提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git config user.name dsh-builder
git config user.email dsh-builder@local
git add dsh-desktop/package.json dsh-desktop/scripts
git commit -m "chore: dsh-desktop 脚手架（npm + sharp 图标合成脚本）"
```

---

### Task 2: 生成应用图标

**Files:**
- Create: `dsh-desktop/src-tauri/icons/*`（tauri icon 生成）

- [ ] **Step 1: 合成 1024×1024 透明底源图**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop && npm run icons:source`
Expected: 输出 `icon source written: .../dsh-desktop/icons/icon-source.png`

- [ ] **Step 2: 生成全套图标**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop && npx tauri icon icons/icon-source.png --output src-tauri/icons`
Expected: `src-tauri/icons/` 出现 `icon.icns`、`icon.png`、`32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.ico` 等文件（`ls src-tauri/icons/` 确认）。

> ⚠️ 注意：必须带 `--output src-tauri/icons`。不带时 tauri-cli（≥2.11）会因 `tauri.conf.json` 尚不存在（Task 3 才创建）而 panic（"Couldn't recognize the current folder as a Tauri project"）。Task 3 之后 `npm run icons`（不带 --output）即可正常工作，产物落点一致。
>
> ℹ️ 再生成提示：`tauri icon` 每次生成的 `icon.icns` 容器字节序可能不同（chunk 内容一致，HashMap 迭代顺序导致），macOS 忽略顺序不影响使用；若重跑 `npm run icons` 后 git 显示 `icon.icns` 变动属正常现象。

- [ ] **Step 3: 提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git add dsh-desktop/icons dsh-desktop/src-tauri/icons
git commit -m "feat: 鲸鱼娘应用图标（.icns 全尺寸）"
```

---

### Task 3: Tauri 骨架（可编译）

**Files:**
- Create: `dsh-desktop/src-tauri/Cargo.toml`
- Create: `dsh-desktop/src-tauri/build.rs`
- Create: `dsh-desktop/src-tauri/tauri.conf.json`
- Create: `dsh-desktop/src-tauri/capabilities/default.json`
- Create: `dsh-desktop/src-tauri/src/main.rs`
- Create: `dsh-desktop/src-tauri/src/lib.rs`
- Create: `dsh-desktop/src/index.html`（占位）

- [ ] **Step 1: 写 Cargo.toml**（完整内容）

```toml
[package]
name = "dsh-desktop"
version = "0.1.0"
description = "DeepSeek Harness 桌面应用（Tauri v2）"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
libc = "0.2"

[profile.release]
strip = true
```

- [ ] **Step 2: 写 build.rs**（完整内容）

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 3: 写 tauri.conf.json**（完整内容）

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "DeepSeek Harness",
  "version": "0.1.0",
  "identifier": "com.dsh.desktop",
  "build": {
    "frontendDist": "../src"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "label": "main",
        "title": "DeepSeek Harness",
        "width": 1280,
        "height": 800,
        "minWidth": 940,
        "minHeight": 600,
        "resizable": true,
        "center": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["app"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns"
    ]
  }
}
```

- [ ] **Step 4: 写 capabilities/default.json**（完整内容）

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

- [ ] **Step 5: 写 src-tauri/src/main.rs**（完整内容）

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dsh_desktop_lib::run()
}
```

- [ ] **Step 6: 写 src-tauri/src/lib.rs**（完整内容，骨架版）

```rust
mod server;

use std::sync::Arc;
use tauri::{Manager, RunEvent, WindowEvent};

pub struct AppState {
    pub server: Arc<server::ServerManager>,
}

#[tauri::command]
fn server_status(state: tauri::State<'_, AppState>) -> server::StatusSnapshot {
    state.server.status()
}

#[tauri::command]
fn restart_server(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mgr = state.server.clone();
    std::thread::spawn(move || mgr.restart());
    Ok("restarting".to_string())
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            server: Arc::new(server::ServerManager::new()),
        })
        .invoke_handler(tauri::generate_handler![server_status, restart_server])
        .setup(|app| {
            let mgr = app.state::<AppState>().server.clone();
            std::thread::spawn(move || mgr.start());
            let handle = app.handle().clone();
            if let Some(win) = app.get_webview_window("main") {
                win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { .. } = event.event() {
                        handle.exit(0);
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build tauri application")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                app_handle.state::<AppState>().server.stop();
            }
        });
}
```

- [ ] **Step 7: 写 server.rs 最小占位**（完整内容，后续 Task 逐步补全）

```rust
use std::sync::Mutex;
use std::process::Child;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub state: String,
    pub port: Option<u16>,
    pub error: Option<String>,
    pub pid: Option<u32>,
    pub owned: bool,
}

pub struct ServerManager {
    child: Mutex<Option<Child>>,
    owned: AtomicBool,
    port: Mutex<u16>,
    state: Mutex<ServerState>,
}

enum ServerState {
    Starting,
    Ready { port: u16 },
    Reused { port: u16 },
    Error(String),
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
        StatusSnapshot {
            state: "starting".to_string(),
            port: None,
            error: None,
            pid: None,
            owned: false,
        }
    }
}
```

- [ ] **Step 8: 写占位前端**（完整内容）

```html
<!doctype html>
<html lang="zh-CN">
<head><meta charset="utf-8" /><title>DeepSeek Harness</title></head>
<body style="margin:0;background:#0b1220;color:#e2e8f0;font-family:-apple-system,sans-serif">
  <p style="padding:20px">DeepSeek Harness 桌面端（占位）</p>
</body>
</html>
```

- [ ] **Step 9: 编译验证**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo check 2>&1 | tail -5`
Expected: 以 `Finished` 结束、无 error（首次会编译大量依赖，放后台任务，timeout 给足）。

- [ ] **Step 10: 提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git add dsh-desktop/src-tauri dsh-desktop/src/index.html
git commit -m "feat: Tauri 骨架可编译（窗口 + 命令 + 生命周期钩子）"
```

---

### Task 4: 端口与 DSH 指纹逻辑（TDD）

**Files:**
- Modify: `dsh-desktop/src-tauri/src/server.rs`（整体替换）

- [ ] **Step 1: 写失败测试**

将 `server.rs` 整体替换为「只含测试 + 空实现」版本（完整内容，注意 `use super::*;` 下引用尚未实现的函数，编译即失败）：

```rust
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

// TODO: is_dsh_running / find_free_port 将在下一步实现
pub fn is_dsh_running(_port: u16) -> bool { unimplemented!() }
pub fn find_free_port(_start: u16) -> Option<u16> { unimplemented!() }

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
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo test 2>&1 | tail -8`
Expected: 出现 `panicked at 'not implemented'` 或 `unimplemented!()` 相关失败（测试编译通过但运行失败）。

- [ ] **Step 3: 实现**

将 `server.rs` 中 `// TODO ... unimplemented!()` 两行替换为（保持文件其余部分不变）：

```rust
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
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo test 2>&1 | tail -8`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 5: 提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git add dsh-desktop/src-tauri/src/server.rs
git commit -m "feat: 端口探测与 DSH 指纹判定（含单元测试）"
```

---

### Task 5: dsh 命令解析（TDD）

**Files:**
- Modify: `dsh-desktop/src-tauri/src/server.rs`

- [ ] **Step 1: 追加失败测试**

在 `server.rs` 的 `#[cfg(test)] mod tests { ... }` 中追加以下测试（完整内容，`tests` 模块内追加）：

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo test 2>&1 | grep -E "error\[|cannot find" | head -5`
Expected: `cannot find function find_dsh_in_npx` / `find_on_path` / `find_node` / `resolve_command` / 类型 `ResolvedCommand` / `ResolveParams` 等编译错误。

- [ ] **Step 3: 实现**

在 `server.rs` 的 `find_free_port` 函数之后（`// ---------- 进程管理 ----------` 之前）插入以下代码（完整内容）：

```rust
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
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo test 2>&1 | tail -8`
Expected: `test result: ok. 8 passed; 0 failed`

- [ ] **Step 5: 提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git add dsh-desktop/src-tauri/src/server.rs
git commit -m "feat: dsh 命令解析（env > PATH > npx 缓存，含单元测试）"
```

---

### Task 6: ServerManager 启停与进程组清理（TDD）

**Files:**
- Modify: `dsh-desktop/src-tauri/src/server.rs`

- [ ] **Step 1: 追加失败测试**

在 `tests` 模块末尾追加（完整内容）：

```rust
    #[test]
    fn kill_group_terminates_spawned_process() {
        let mut child = Command::new("/bin/sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        kill_group(&child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && process_alive(child.id() as i32) {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!process_alive(child.id() as i32));
        let _ = child.kill();
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo test 2>&1 | grep -E "error\[|cannot find" | head -5`
Expected: `cannot find function kill_group` / `process_alive` 编译错误。

- [ ] **Step 3: 实现**

在 `server.rs` 的 `resolve_command` 之后追加进程管理与管理器（完整内容，追加到 `pub fn resolve_command` 结束处、`#[cfg(test)]` 之前）：

```rust
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
    unsafe { libc::kill(pid, 0) == 0 }
}

/// 终止整个进程组：先 SIGTERM，2 秒后仍未退出则 SIGKILL。
pub fn kill_group(child: &Child) {
    let pid = child.id() as i32;
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !process_alive(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}

// ---------- 管理器 ----------

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

    /// 阻塞式启动：复用检测 → 选端口 → spawn → 轮询就绪。由调用方放入线程。
    pub fn start(&self) {
        *self.state.lock().unwrap() = ServerState::Starting;
        let default_port: u16 = std::env::var("DSH_DESKTOP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        if is_dsh_running(default_port) {
            *self.state.lock().unwrap() = ServerState::Reused { port: default_port };
            return;
        }
        let Some(port) = find_free_port(default_port) else {
            *self.state.lock().unwrap() = ServerState::Error("没有可用端口".to_string());
            return;
        };
        let npx_root = Path::new(&std::env::var("HOME").unwrap_or_default())
            .join(".npm/_npx");
        let params = ResolveParams {
            dsh_env: std::env::var("DSH_DESKTOP_DSH").ok().as_deref(),
            node_env: std::env::var("DSH_DESKTOP_NODE").ok().as_deref(),
            path_var: std::env::var("PATH").unwrap_or_default().as_str(),
            node_candidates: &[PathBuf::from("/opt/homebrew/bin/node")],
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
        *self.port.lock().unwrap() = port;
        self.owned.store(true, Ordering::SeqCst);
        *self.child.lock().unwrap() = Some(child);
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            if is_dsh_running(port) {
                *self.state.lock().unwrap() = ServerState::Ready { port };
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

    /// 仅当服务由本应用启动时才停止（复用中的服务不杀）。
    pub fn stop(&self) {
        if self.owned.load(Ordering::SeqCst) {
            self.stop_owned();
        }
    }

    fn stop_owned(&self) {
        if let Some(child) = self.child.lock().unwrap().take() {
            kill_group(&child);
        }
        self.owned.store(false, Ordering::SeqCst);
    }

    pub fn restart(&self) {
        self.stop();
        self.start();
    }

    pub fn status(&self) -> StatusSnapshot {
        let (state, port, error) = match &*self.state.lock().unwrap() {
            ServerState::Starting => ("starting".to_string(), None, None),
            ServerState::Ready { port } => ("ready".to_string(), Some(*port), None),
            ServerState::Reused { port } => ("reused".to_string(), Some(*port), None),
            ServerState::Error(e) => ("error".to_string(), None, Some(e.clone())),
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
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo test 2>&1 | tail -8`
Expected: `test result: ok. 9 passed; 0 failed`

- [ ] **Step 5: 提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git add dsh-desktop/src-tauri/src/server.rs
git commit -m "feat: ServerManager 启停/重启/状态（进程组清理，含测试）"
```

---

### Task 7: Tauri 集成编译验证

**Files:**
- Modify: `dsh-desktop/src-tauri/src/lib.rs`（无改动，仅验证）
- Modify: `dsh-desktop/src-tauri/src/server.rs`（无改动，仅验证）

- [ ] **Step 1: 编译验证**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri && cargo check 2>&1 | tail -5`
Expected: `Finished` 无 error（lib.rs 骨架在 Task 3 已引用 `server::ServerManager::start/stop/restart/status`，Task 6 已补全实现）。

- [ ] **Step 2: 提交（若 Task 6 后 lib.rs 有未提交改动）**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git status --short
# 若无改动可跳过本步；有改动则：
git add dsh-desktop/src-tauri && git commit -m "chore: 集成编译验证"
```

---

### Task 8: 前端状态栏

**Files:**
- Modify: `dsh-desktop/src/index.html`（整体替换）

- [ ] **Step 1: 写完整前端**

将 `dsh-desktop/src/index.html` 整体替换为（完整内容）：

```html
<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<title>DeepSeek Harness</title>
<style>
  :root { --ok:#22c55e; --warn:#f59e0b; --err:#ef4444; }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  html, body { height: 100%; }
  body { display: flex; flex-direction: column; font-family: -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif; background: #0b1220; color: #e2e8f0; }
  #statusbar { display: flex; align-items: center; gap: 10px; padding: 6px 14px; background: #111a2e; border-bottom: 1px solid #1e293b; font-size: 13px; user-select: none; }
  #dot { width: 10px; height: 10px; border-radius: 50%; background: var(--warn); }
  #dot.starting { background: var(--warn); animation: pulse 1.2s infinite; }
  #dot.ready, #dot.reused { background: var(--ok); }
  #dot.error { background: var(--err); }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: .35; } }
  #status-text { flex: 1; }
  #restart { display: none; padding: 3px 12px; border-radius: 6px; border: 1px solid #334155; background: #1e293b; color: #e2e8f0; cursor: pointer; font-size: 12px; }
  #restart:hover { background: #334155; }
  #restart.show { display: inline-block; }
  #frame { flex: 1; width: 100%; border: 0; background: #fff; }
</style>
</head>
<body>
  <div id="statusbar">
    <span id="dot" class="starting"></span>
    <span id="status-text">正在启动 DSH 服务…</span>
    <button id="restart">重启服务</button>
  </div>
  <iframe id="frame" src="about:blank" allow="clipboard-read; clipboard-write; fullscreen"></iframe>
  <script>
    const tauri = window.__TAURI__;
    const $ = (id) => document.getElementById(id);
    const frame = $('frame');
    let currentPort = null;

    function render(snapshot) {
      const dot = $('dot');
      const text = $('status-text');
      const restart = $('restart');
      dot.className = '';
      const st = snapshot.state;
      if (st === 'starting') {
        dot.classList.add('starting');
        text.textContent = '正在启动 DSH 服务…';
        restart.classList.remove('show');
      } else if (st === 'ready') {
        dot.classList.add('ready');
        text.textContent = '已就绪 · 端口 ' + snapshot.port;
        restart.classList.remove('show');
      } else if (st === 'reused') {
        dot.classList.add('reused');
        text.textContent = '复用已有服务 · 端口 ' + snapshot.port;
        restart.classList.remove('show');
      } else {
        dot.classList.add('error');
        text.textContent = '服务异常：' + (snapshot.error || '未知错误');
        restart.classList.add('show');
      }
      const port = (st === 'ready' || st === 'reused') ? snapshot.port : null;
      if (port && port !== currentPort) {
        currentPort = port;
        frame.src = 'http://127.0.0.1:' + port;
      }
    }

    async function poll() {
      try {
        render(await tauri.core.invoke('server_status'));
      } catch (e) {
        $('status-text').textContent = '状态查询失败：' + e;
      }
      setTimeout(poll, 2000);
    }

    $('restart').addEventListener('click', async () => {
      $('restart').disabled = true;
      try { await tauri.core.invoke('restart_server'); } catch (e) { console.error(e); }
      $('restart').disabled = false;
    });

    if (!tauri) {
      frame.src = 'http://127.0.0.1:3080';
      $('status-text').textContent = '浏览器预览模式（非桌面运行时）';
    } else {
      poll();
    }
  </script>
</body>
</html>
```

- [ ] **Step 2: 静态检查**

Run: `cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop && node -e "const h=require('fs').readFileSync('src/index.html','utf8'); const m=h.match(/<script>([\s\S]*?)<\/script>/); new Function(m[1]); console.log('JS syntax OK')"`
Expected: 输出 `JS syntax OK`（无语法错误）。

- [ ] **Step 3: 提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git add dsh-desktop/src/index.html
git commit -m "feat: 状态栏前端（轮询 server_status + 重启按钮 + iframe）"
```

---

### Task 9: 发布构建与验收

**Files:**
- Create: `dsh-desktop/README.md`

- [ ] **Step 1: 发布构建（后台长任务）**

Run（后台）：`cd /Users/beck.lee/Desktop/dsh-workspace/dsh-desktop && npm run build 2>&1 | tail -20`
Expected（等待完成后）：`Finished` 且生成 `src-tauri/target/release/bundle/macos/DeepSeek Harness.app`（`ls -d "src-tauri/target/release/bundle/macos/DeepSeek Harness.app"`）。

- [ ] **Step 2: 复用模式验收（当前 3080 有本会话 DSH）**

Run: `open "/Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri/target/release/bundle/macos/DeepSeek Harness.app"`
Expected:
- 窗口出现，状态栏显示「复用已有服务 · 端口 3080」，iframe 加载出 DSH 界面
- 退出应用（Cmd+Q）后，运行 `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:3080/` 仍返回 200 —— **证明没有误杀正在运行的会话**

- [ ] **Step 3: 自启自停模式验收（用独立端口）**

Run: `DSH_DESKTOP_PORT=3999 "/Users/beck.lee/Desktop/dsh-workspace/dsh-desktop/src-tauri/target/release/bundle/macos/DeepSeek Harness.app/Contents/MacOS/DeepSeek Harness" 2>&1 | head -5`（放后台运行）
Expected: 状态栏「已就绪 · 端口 3999」；`curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:3999/` 返回 200。退出应用后 `lsof -nP -iTCP:3999 -sTCP:LISTEN` 无输出（进程已清理）。

- [ ] **Step 4: 图标验收**

Expected: Dock 与应用包图标显示鲸鱼娘（`sips -g pixelWidth "DeepSeek Harness.app/Contents/Resources/icon.icns"` 或直接目视确认）。

- [ ] **Step 5: 写 README**

写入 `dsh-desktop/README.md`（完整内容）：

```markdown
# DeepSeek Harness 桌面应用（Tauri v2）

macOS 个人自用桌面壳：打开自动启动 `dsh web`，关闭自动停止；顶部状态栏显示服务状态。

## 构建

```bash
npm install
npm run icons   # 生成图标（需要 assets/whale-girl/whale-girl-transparent.png）
npm run build   # 产物：src-tauri/target/release/bundle/macos/DeepSeek Harness.app
```

## 运行

双击 `DeepSeek Harness.app` 即可。已打开的 DSH（127.0.0.1:3080）会被自动复用，不会被误杀。

## 环境变量（可选）

- `DSH_DESKTOP_DSH`：dsh 可执行文件路径（默认：PATH 中的 `dsh`，或 node + `~/.npm/_npx/*/node_modules/@deepseek-ai/dsh/lib/bin.js`）
- `DSH_DESKTOP_NODE`：node 可执行文件路径（默认：`/opt/homebrew/bin/node`）
- `DSH_DESKTOP_PORT`：首选端口（默认 3080；被占用时自动顺延）

## 图标素材署名（CC BY-NC-SA 4.0）

- 角色形象「溟月」：上善无形（原创 OC）
- DeepSeek 元素二创：ZipZipPipe
- 改进版修复：QYQCAMIAO
- 来源：https://github.com/fornarwhal/deepseek-whale-girl-icon
- 原图保存于 `../assets/whale-girl/`
```

- [ ] **Step 6: 最终提交**

```bash
cd /Users/beck.lee/Desktop/dsh-workspace
git add dsh-desktop/README.md
git commit -m "docs: dsh-desktop README（构建/运行/署名）"
git log --oneline
```

---

## 自审记录

- **Spec 覆盖**：Tauri 壳（Task 3/7）、生命周期跟随启停（Task 6/9）、服务状态指示（Task 8）、端口策略与 DSH 指纹（Task 4/6）、图标（Task 2）、署名（Task 9）、错误处理（Task 6 状态机 + 前端异常显示）、验收（Task 9）——全部覆盖。
- **占位扫描**：无 TBD/TODO；所有步骤含完整代码与命令。
- **类型一致性**：`StatusSnapshot` 字段（state/port/error/pid/owned）在 server.rs 定义、lib.rs 命令返回、前端 JS 消费三处一致；`ResolvedCommand`/`ResolveParams` 在 Task 5 定义、Task 6 使用一致；`kill_group`/`process_alive`/`spawn_server` 命名一致。
- **已知取舍**：`cargo test` 会连同 tauri 一起编译（首次慢）；图标合成用 sharp 而非 sips（sips 不支持透明 padding）。
