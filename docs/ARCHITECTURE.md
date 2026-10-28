# 架构设计

## 1. 分层

```
┌───────────────────────────────────────────────┐
│ 前端 React（WebView）                          │
│  配置表单 / 日志面板 / 服务器管理 / 设置        │
└───────────────┬───────────────────────────────┘
                │ invoke（Tauri IPC，参数 camelCase）
┌───────────────▼───────────────────────────────┐
│ Tauri 命令层 commands.rs                       │
│  参数校验、错误转 String、组合各服务           │
├──────────────┬──────────────┬─────────────────┤
│ store.rs     │ supervisor   │ admin.rs        │
│ 持久化       │ 子进程托管    │ HTTP 管理 API    │
└──────┬───────┴──────┬───────┴────────┬────────┘
       │              │                │
  store.json    frpc 进程        frpc:7400 / frps:7500
                （sidecar 或用户指定路径）
```

## 2. 数据模型

```
AppSettings  { logLevel, logMaxLines, frpcPath, defaultAdminPort, ... }

Server       { id, name, addr, port, protocol, authMethod, token,
               tlsEnable, tlsServerName, tlsInsecure,
               frpsAdminPort, frpsAdminUser, frpsAdminPassword,
               remark, subscriptionUrl }
     │ 1
     │
     │ n
Profile      { id, name, serverId, autoStart, proxies[], adminPort,
               logLevel, rawOverride }
     │ 1
     │ n
ProxyDef     { id, name, kind(tcp|udp|http|https|stcp|sudp|xtcp|tcpmux),
               localIp, localPort, remotePort, customDomains, subdomain,
               locations, secretKey, plugin }
```

为什么把 Server 和 Profile 拆开：同一个 frps（同一个 token/TLS 配置）上通常挂多组端口映射（ssh 一组、web 一组、rdp 一组），拆开才能复用，也和 frp 自己的 `serverAddr + [[proxies]]` 结构一致。

`Profile.rawOverride` 是留给高级用户的逃生舱：一段 TOML 片段，最后覆盖到生成结果上（给插件参数、`healthCheck`、`transport.poolCount` 这类没做 UI 的字段用）。

## 3. 启动链路（关键路径）

```
start_profile(id)
  ├─ 1. 从 store 取 profile + server，缺 server 直接报错
  ├─ 2. config::build_frpc_toml() → String
  ├─ 3. 写 $APP_DATA/profiles/{id}.toml（UTF-8，无 BOM）
  ├─ 4. resolve_frpc()：settings.frpcPath → resource_dir → exe_dir → dev binaries/
  ├─ 5. frpc verify -c cfg   ← 失败则把 stderr 原文返回给 UI
  ├─ 6. supervisor.spawn()：stdout/stderr 两个线程 → 环形缓冲（默认 2000 行）
  └─ 7. 写 running.json { profileId: pid }
```

应用退出（`RunEvent::ExitRequested` / 窗口关闭）时 `supervisor.stop_all()`，并清空 `running.json`。
下次启动时 `cleanup_orphans()` 读 `running.json`：对每个 pid 先确认进程还活着、且命令行/映像名包含 `frpc`，才 kill——避免 pid 复用误杀。

## 4. 日志策略

- 进程内环形缓冲（`VecDeque`，按 `logMaxLines` 截断），不落盘，避免长跑撑爆磁盘。
- 前端轮询 `get_logs`（1s）+ 手动"清空"，比事件通道简单且不会丢事件；需要更实时再改 `emit`。
- 每行带 `seq`（单调递增），前端可按 seq 增量渲染。

## 5. 管理 API

frpc 侧（`webServer`，默认 7400，Basic Auth）：
- `GET /api/status` → 各代理运行状态
- `POST /api/reload` → 热加载配置（body `{"configContent": "<toml>"}`）
- `POST /api/stop`

frps 侧（`webServer`，默认 7500，Basic Auth）：
- `GET /api/serverinfo` → 版本、连接数、流量
- `GET /api/proxy/{tcp|udp|http|https|stcp|sudp|xtcp|tcpmux}` → 在线代理列表

注意：`/api/reload` 的 body 结构在 frp 各版本间有过变化，本项目**兜底策略是"改完配置直接重启进程"**（`restart_profile`），热加载只作为可选优化。

## 6. 打包

- frpc 通过 `bundle.externalBin: ["binaries/frpc"]` 打进安装包，Tauri 会自动补 target triple 后缀找文件。
- 运行时用 `resolve_frpc()` 自己找，不依赖 shell 插件，少一层权限配置。
- macOS 打包 `.app` + `.dmg`，Windows 出 `.msi`/`.nsis`，签名与公证见 `docs/BUILD.md`。

## 7. 路线图

**P1（已完成）**：配置 CRUD、启停、日志、校验、导入导出、订阅、frps 看板、签名/公证发布链路。

**P2（已完成）**：系统托盘（菜单 = 显示主窗口 / 启动全部 / 停止全部 / 退出）、关窗最小化到托盘、开机自启、窗口标题显示运行数。

- 托盘实现在 `src-tauri/src/tray.rs`，只做入口，动作全部复用 `commands` 里的函数，避免两套逻辑。
- 关窗拦截在 `main.rs` 的 `on_window_event`：`CloseRequested` + `minimize_to_tray` 时 `prevent_close()` 并 `hide()`；该设置关掉就是正常退出。
- 自启用 `tauri-plugin-autostart`：macOS 写 `~/Library/LaunchAgents` 明文 plist，Windows 写 `HKCU\...\CurrentVersion\Run`（会出现在任务管理器"启动"页）。**刻意不做隐藏** —— 隐藏自启正是杀软判定后门的典型特征。
- 自启拉起时带 `--hidden`，配合 `minimize_to_tray` 静默进托盘，不打断用户。

**P3**：流量曲线（定时采样 `/api/status` 做时间序列）、多语言、主题、快捷键、崩溃日志上报（可选、需明示同意）、`tauri-plugin-updater` 增量更新（更新包必须签名，否则又会被拦）。

**P4**：frps 侧管理增强（在线代理踢下线、按 token 统计），以及 STCP/XTCP 的 visitor 配置 UI。

## 8. 已知坑

| 坑 | 处理 |
| --- | --- |
| GUI 退出但 frpc 还在跑 | `stop_all()` + `running.json` 孤儿清理 |
| pid 复用导致误杀 | kill 前校验进程名 |
| 配置编码错（UTF-16） | 配置全由程序生成，固定 UTF-8 |
| frpc 版本差异（0.52 → 0.6x 配置项改名） | `rawOverride` + 版本探测（`frpc -v`）提示 |
| macOS 沙箱 | 不启用 App Sandbox（需 spawn 子进程、写用户目录） |
| 托盘图标缺失崩溃 | 图标必须先 `pnpm tauri icon` 生成 |
| **Tauri 2 没有 `RunEvent::Reopen`** | 那是 v1 API，写了必然编译失败。macOS 关窗隐藏后靠点托盘图标唤回；Dock 点击不恢复（要彻底解决得 `set_activation_policy` 走纯菜单栏 App） |
| `args([...])` 混用 `&String` 和 `&str` | `Command::args` 的数组元素必须同型，`&pid.to_string()` 要预先 `let s = pid.to_string()` 再 `s.as_str()` |
| `AppHandle::tray_by_id` 参数类型跨版本不一致 | 没用它。运行数量改用 `window.set_title()` 体现，API 稳定 |
| `tauri-plugin-autostart` 2.7 要求 rustc ≥ 1.90 | `Cargo.toml` 的 `rust-version` 已同步为 1.90 |
