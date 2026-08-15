# DeepSeek Harness 桌面应用 — 设计文档

- 日期：2026-08-15
- 状态：已与用户确认（2026-08-15）
- 目标平台：macOS（Apple Silicon，个人自用）

## 1. 背景与目标

为 DeepSeek Harness（DSH）做一个 macOS 桌面应用：双击打开应用时自动启动 DSH 服务（`dsh web`），以原生窗口形式展示 DSH Web UI（默认 `http://127.0.0.1:3080`），退出应用时自动停止服务。应用图标使用「蓝色鲸鱼娘」形象。

**使用场景**：个人自用、本机运行。不做跨平台打包、不做签名分发、不做自动更新。

## 2. 已确认的需求决策

| 决策项 | 结论 |
|---|---|
| 分发范围 | 个人使用，本机即可（产出可双击的 .app） |
| 服务生命周期 | 跟随 App 启停；若端口上已有 DSH 在跑则复用、退出时不误杀 |
| 应用名称 | DeepSeek Harness（窗口标题 / Dock 名） |
| 桌面端功能 | 最简版 + 服务状态指示（启动中 / 已就绪 / 异常 + 重启按钮） |
| 技术栈 | Tauri v2（Rust），WKWebView 承载 |
| 图标素材 | `whale-girl-transparent.png`（社区仓库 deepseek-whale-girl-icon，角色「溟月」，CC BY-NC-SA 4.0，需署名） |

## 3. 架构

```
┌─────────────────────────────────────────────┐
│  Tauri 窗口（macOS .app，Dock 鲸鱼娘图标）      │
│  ┌─────────────────────────────────────────┐ │
│  │ 顶部状态栏（本地 HTML）                    │ │
│  │  🐋 已就绪 · 端口3080 · [重启服务]         │ │
│  ├─────────────────────────────────────────┤ │
│  │  iframe ──► http://127.0.0.1:3080        │ │
│  │           （DSH Web UI 全屏展示）          │ │
│  └─────────────────────────────────────────┘ │
└──────────────────────┬──────────────────────┘
                       │ spawn / kill
              ┌────────▼────────┐
              │  dsh web 子进程   │  (node + @deepseek-ai/dsh)
              └─────────────────┘
```

### 3.1 Rust 侧（src-tauri）

- **进程定位**：按顺序解析 dsh 可执行方式：
  1. 环境变量 `DSH_DESKTOP_DSH` / `DSH_DESKTOP_NODE`（可覆盖）
  2. PATH 中的 `dsh` 命令
  3. 兜底：`/opt/homebrew/bin/node` + `~/.npm/_npx/*/node_modules/@deepseek-ai/dsh/lib/bin.js`（取最新匹配）
- **启动**：`spawn("dsh web")`，捕获子进程句柄与进程组 id
- **端口探测**：TCP connect `127.0.0.1:3080`，轮询直到就绪（超时 30s，间隔 500ms）
- **端口策略**：默认 3080（已核实 `dsh web --port <port>` 支持显式端口，`--port 0` 可由系统分配）。若 3080 已开放但**不是 DSH**，顺延探测 3081、3082… 取首个空闲端口并以 `--port` 显式启动
- **DSH 指纹判定**：端口开放后 HTTP `GET /`，响应体含 DSH 特征（如 `#root` 容器 / dsh 相关脚本路径）即判定为 DSH；否则视为被其他程序占用
- **退出清理**：仅当服务由本应用启动时，kill 整个进程组（SIGTERM，超时后 SIGKILL），确保无残留
- **Tauri 命令**（invoke_handler）：
  - `server_status() -> { state: "starting"|"ready"|"error"|"reused", port, pid, owned }`
  - `restart_server() -> Result<(), String>`：kill 后重新 spawn 并等待就绪
- **窗口生命周期**：主窗口关闭 = 退出应用 = 停止服务（跟随启停语义）

### 3.2 前端侧（本地静态页，frontendDist）

- `index.html` + 内联 JS/CSS，纯静态，无构建框架
- 顶部状态栏：状态圆点 + 文案（启动中/已就绪/异常/复用中）+ 「重启服务」按钮
- 通过 `@tauri-apps/api` 的 `invoke` 每 2s 轮询 `server_status`
- 主体为 `<iframe src="http://127.0.0.1:3080">`，占满剩余空间

### 3.3 Tauri 配置要点

- `tauri.conf.json`：窗口 1280×800、标题 DeepSeek Harness、`csp: null`（避免干扰远程页面）、bundle 图标 icns
- macOS 产物：`src-tauri/target/release/bundle/macos/*.app`

## 4. 进程生命周期矩阵

| 事件 | 行为 |
|---|---|
| 打开 App | 探测 3080 → 已有 DSH 则复用（`owned=false`）→ 否则 spawn `dsh web`，等待就绪 |
| 运行中 | 每 2s 轮询：端口连通 + 子进程存活 |
| 服务异常（端口断/进程死） | 状态栏「异常」+ 重启按钮 |
| 关闭窗口 / Cmd+Q | `owned=true` 时 kill 进程组；`owned=false` 时不杀 |

## 5. 图标与品牌

- 素材：`assets/whale-girl/whale-girl-transparent.png`（910×941 透明底 PNG）
- 处理：用 sharp（Node）合成为 1024×1024 透明底方形 → `tauri icon` 生成 `.icns`（16~1024 全尺寸）与各平台图标，写入 `src-tauri/icons/`
- 署名：README 中注明素材来源与 CC BY-NC-SA 4.0 授权

## 6. 工程结构

```
dsh-workspace/
├── dsh-desktop/            # 主项目（npm + src-tauri）
│   ├── package.json        # @tauri-apps/cli、@tauri-apps/api（devDeps / deps）
│   ├── src/                # 状态栏前端（index.html + 内联 js/css）
│   └── src-tauri/
│       ├── src/main.rs     # 生命周期、端口探测、kill、Tauri 命令
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       ├── capabilities/default.json
│       └── icons/          # tauri icon 生成
├── assets/whale-girl/      # 原始素材 + 预览（已下载）
├── scripts/                # 辅助脚本（如 gen-icon-page.js 等）
└── docs/superpowers/specs/ # 本文档
```

## 7. 错误处理

- 启动超时（30s）：显示「启动超时」+ 重启按钮
- 端口被非 DSH 占用：按 DSH 指纹判定后自动顺延端口；状态栏展示实际端口
- 子进程意外退出：状态栏「异常」，可一键重启
- 找不到 node/dsh：窗口内显示错误页 + 指引（说明如何配置 `DSH_DESKTOP_DSH`）

## 8. 已知风险与备选方案

| 风险 | 应对 |
|---|---|
| DSH 页面拒绝 iframe 嵌入（X-Frame-Options/CSP） | 备选：整窗直接加载 DSH 页面；状态改为 Dock 角标 + 原生菜单（先按 iframe 实现，遇阻切换） |
| WKWebView 兼容性 | DSH 为标准 React 应用，风险低；若遇兼容问题可切回 Electron（架构不变） |
| npx 缓存路径变动 | 解析逻辑含 PATH 优先 + 环境变量覆盖，文档说明 |

## 9. 验收标准

1. 双击 `.app` → 状态栏变「已就绪」，DSH 界面完整可用（登录/会话/工具均正常）
2. 退出应用后 `ps aux | grep dsh` 无残留 dsh 进程
3. 当前正在运行的 DSH（浏览器中 127.0.0.1:3080）不会被误杀
4. Dock 显示鲸鱼娘图标；.app 图标正常
5. 手动 kill 服务进程 → 状态栏「异常」→ 点「重启服务」恢复
6. `dsh-desktop/src-tauri/target/release/bundle/macos/DeepSeek Harness.app` 双击可运行
