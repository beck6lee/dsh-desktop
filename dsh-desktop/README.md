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
