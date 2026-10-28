# frpc-desktop

frp 客户端桌面端（Tauri 2 + Rust + React），支持多服务器、多配置、启动/停止/日志实时查看、导入导出与订阅、以及 frps 服务端看板。

## 为什么自己写桌面端

原生 frpc 是命令行裸二进制、无签名无公证，macOS Gatekeeper 和 Windows Defender 一定会拦。自己打包后可以做三件事，从根上消除"存在风险"提示：

1. **签名**：macOS 用 Developer ID Application 证书 + hardened runtime；Windows 用代码签名证书（EV 更好）。
2. **公证**：macOS `notarytool` 提交公证，Gatekeeper 才会放行。
3. **合规化行为**：frpc 作为子进程由主程序托管，不写注册表自启、不做隐藏常驻，特征不再是"后门"。

## 状态

| 模块 | 状态 |
| --- | --- |
| 数据模型（Server/Profile/Proxy/Settings） | ✅ 已完成 |
| 本地存储（原子写入、版本迁移位） | ✅ 已完成 |
| frpc.toml 生成（0.52+ 格式） | ✅ 已完成 |
| frpc 进程托管（启动/停止/重启/日志环形缓冲/退出清理） | ✅ 已完成 |
| frpc 配置校验（`frpc verify`） | ✅ 已完成 |
| frpc 管理 API（`/api/status`、`/api/reload`） | ✅ 已完成 |
| frps 看板 API（`/api/serverinfo` 等） | ✅ 已完成 |
| 导入/导出（JSON 包）/ 订阅拉取（JSON、TOML、base64） | ✅ 已完成 |
| 前端：配置编辑、启停、日志、服务器管理、设置 | ✅ 基础版 |
| 系统托盘、关窗最小化、开机自启 | ✅ 托盘菜单、LaunchAgent 与 Windows Run 键 |
| 代理流量图表、多语言、自动更新 | ⬜ 见 `docs/ARCHITECTURE.md` 路线图 |
| macOS 签名/公证脚本、Windows 签名、CI 发布 | ✅ 见 `docs/RELEASE.md` |

> 本骨架未在本机编译验证（当前环境 shell 不可用）。首次运行按"快速开始"跑一遍，若有 Tauri 小版本 API 漂移，按报错改一行即可。

## 快速开始

前置：**Node 20+ / pnpm 9+ / Rust 1.90+**（`tauri-plugin-autostart` 2.7 的 MSRV 是 1.90）。

```bash
# 1. 安装依赖
pnpm install

# 2. 放入 frpc 内核（必须带 target triple 后缀，Tauri 按此命名打包 sidecar）
#    macOS arm64:
cp /path/to/frp_0.52.3_darwin_arm64/frpc  src-tauri/binaries/frpc-aarch64-apple-darwin
#    macOS intel:
cp /path/to/frp_0.52.3_darwin_amd64/frpc  src-tauri/binaries/frpc-x86_64-apple-darwin
#    Windows:
copy frp_0.52.3_windows_amd64\frpc.exe  src-tauri\binaries\frpc-x86_64-pc-windows-msvc.exe
chmod +x src-tauri/binaries/frpc-*    # macOS

# 3. 生成图标（必须有，否则打包失败）
pnpm tauri icon ./logo.png

# 4. 开发/构建
pnpm tauri dev
pnpm tauri build
```

内核也可以不内置：设置里填"自定义 frpc 路径"即可覆盖，方便你自己升级 frpc 版本。

## 目录结构

```
frpc-desktop/
├─ src/                      前端（React + TS）
│  ├─ App.tsx                主界面：配置 / 服务器 / 日志 / 设置
│  ├─ api.ts                 所有 invoke 封装
│  ├─ types.ts               与 Rust serde(camelCase) 对齐的类型
│  └─ styles.css
├─ src-tauri/
│  ├─ binaries/              frpc sidecar（按 target triple 命名）
│  ├─ src/
│  │  ├─ model.rs            Server / Profile / Proxy / Settings
│  │  ├─ store.rs            JSON 存储，原子写入
│  │  ├─ config.rs           模型 → frpc.toml
│  │  ├─ supervisor.rs       子进程生命周期 + 日志缓冲 + 孤儿进程清理
│  │  ├─ admin.rs            frpc / frps 管理 API 客户端
│  │  ├─ commands.rs         Tauri 命令（对前端暴露）
│  │  ├─ tray.rs             托盘菜单 / 工具提示 / 唤起窗口
│  │  ├─ autostart.rs        开机自启同步（LaunchAgent / Run 键）
│  │  └─ main.rs             组装、插件注册、关窗拦截、退出清理
│  ├─ tauri.conf.json
│  └─ entitlements.plist     hardened runtime 权限
├─ scripts/
│  ├─ preflight.mjs          发布前预检（版本一致性/内核架构/图标）
│  ├─ sign-macos.sh          构建 + 由内到外签名 + 公证 + DMG
│  ├─ verify-macos.sh        六项发布自查
│  └─ windows/sign.ps1       signtool 签 exe / sidecar / 安装包
├─ .github/workflows/release.yml   打 tag 自动产出双平台已签名包
└─ docs/
   ├─ ARCHITECTURE.md        架构、数据流、路线图
   ├─ BUILD.md               签名原理、公证、防误报
   └─ RELEASE.md             发布手册（环境变量 / CI / 排错）
```

## 关键设计取舍

- **不用 tauri-plugin-shell**：直接 `std::process::Command` 起 frpc，才能精确拿到 pid、做环形日志缓冲、在退出时杀掉所有子进程（避免 frpc-desktop 那个"Command+Q 后 frpc 还在跑"的经典 bug）。
- **配置先落盘再启动**：每次 start 都重新生成 `profiles/<id>.toml`，保证 UI 是唯一真相；改完配置点"重启"即生效。
- **校验前置**：start 之前先跑 `frpc verify -c xxx.toml`，把配置错误在 GUI 里暴露，而不是让进程一闪而过。
- **订阅是自定义 JSON**：frp 没有官方订阅格式，这里定义版本化 JSON 包（`version/servers/profiles`），同时兼容裸 `frpc.toml`、面板导出的 base64 JSON。

## 发布（这一步才是"不再报风险"的关键）

```bash
pnpm preflight      # 版本一致性 / 内核架构 / 图标 / externalBin
pnpm release:mac    # 构建 → 签名（含 sidecar）→ 公证 → staple → DMG
pnpm release:win    # 构建 → 签 exe + sidecar → 打安装包 → 签安装包
```

只打 tag 让 CI 干也行：`git tag v0.1.0 && git push origin v0.1.0`（Secrets 配置见 `docs/RELEASE.md`）。

**顺序不能颠倒**：Tauri 默认会在 build 时自动签名+公证，那会儿 sidecar 还没签，公证必挂。脚本里刻意用 `env -u APPLE_*` 屏蔽掉自动逻辑，改成构建完再"由内到外"手签。

发布前必跑 `bash scripts/verify-macos.sh <app 路径>`，六项全绿才算能发——其中最容易漏的是第 2 项（sidecar 也得带 `--options runtime` 签）。

## 提示

`frpc.toml` 必须是 UTF-8 无 BOM。用 Windows 记事本另存会产生 UTF-16，frpc 会直接报 `parse config error`——本项目的配置全部由程序生成，天然规避这个问题。
