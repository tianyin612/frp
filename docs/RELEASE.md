# 发布手册

## 0. 最快路径：CI 出未签名包（不需要证书）

`.github/workflows/build.yml` 默认就走这条路：**没有 Secrets 也能出安装包**。

```bash
git remote add origin https://github.com/<你>/<仓库>.git
git push -u origin main
```

推上去后到仓库 **Actions → build → Run workflow** 手动触发，几分钟后在该次运行页面底部下载：

- `windows-<sha>`：`.msi` + `.exe`（NSIS）
- `macos-aarch64-apple-darwin-<sha>`：`.dmg` + `.app`
- `macos-x86_64-apple-darwin-<sha>`：同上（Intel）

**未签名包的行为**：

- Windows：安装/运行会弹 SmartScreen「已保护你的电脑」，点「更多信息 → 仍要运行」；frpc 可能被 Defender/管家再拦一次。
- macOS：Gatekeeper 会拦，需右键打开或在「隐私与安全性」里点「仍要打开」。**这就是为什么最终要给用户签名版**。

想升级成签名版：把下面第 3 节的 Secrets 配上，同一条工作流会自动改走 `pnpm release:mac` / `pnpm release:win`，产物即签名（macOS 还会自动公证 + staple），不需要改代码。

---

一句话流程：**预检 → 构建（不带自动签名）→ 由内到外签名 → 公证 → staple → 打包 → 自查**。

顺序不能颠倒。Tauri 默认的"build 时自动签名+公证"会在 sidecar 还没签的情况下就把整个 `.app` 提交公证，导致 `frpc` 被拒——所以脚本里刻意先 `env -u APPLE_*` 把 Tauri 的自动逻辑屏蔽掉。

## 1. 一次性准备

### macOS

| 项 | 说明 |
| --- | --- |
| Apple Developer Program | $99/年，个人/公司均可 |
| `Developer ID Application` 证书 | 从 developer.apple.com 导出 `.p12`，导入钥匙串 |
| App 专用密码 | appleid.apple.com → 登录与安全 → App 专用密码 |
| Team ID | 证书名括号里的 10 位，或开发者后台 Membership 页 |

```bash
export SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_TEAM_ID="TEAMID"
export APPLE_APP_PASSWORD="abcd-efgh-ijkl-mnop"
```

### Windows

| 项 | 说明 |
| --- | --- |
| 代码签名证书 | OV（便宜，信誉需积累）或 EV（立即过 SmartScreen，推荐） |
| `.pfx` + 密码 | `WIN_CERT_PFX` / `WIN_CERT_PASSWORD`；或在 CurrentUser\My 里用指纹 `WIN_CERT_THUMBPRINT` |
| Windows SDK | 提供 `signtool.exe`，脚本会自动找 |

## 2. 本地发布

```bash
# 0) 预检：版本一致性、sidecar 架构、图标
pnpm preflight

# 1) macOS（会自动 build + 签名 + 公证 + 出 DMG）
pnpm release:mac

# 2) Windows（会自动 build + 签 exe/sidecar + 打安装包 + 签安装包）
pnpm release:win
```

默认 `TARGET=aarch64-apple-darwin`。Intel 用 `TARGET=x86_64-apple-darwin`，通用包：

```bash
lipo -create src-tauri/binaries/frpc-x86_64-apple-darwin \
            src-tauri/binaries/frpc-aarch64-apple-darwin \
  -output src-tauri/binaries/frpc-universal-apple-darwin
TARGET=universal-apple-darwin pnpm release:mac
```

## 3. CI 发布（推荐）

`.github/workflows/release.yml`，打 tag 自动跑：

```bash
git tag v0.1.0 && git push origin v0.1.0
```

需要在仓库 Secrets 里配置：

| Secret | 内容 |
| --- | --- |
| `APPLE_CERTIFICATE` | `.p12` 的 base64（`base64 -i cert.p12 \| pbcopy`） |
| `APPLE_CERTIFICATE_PASSWORD` | p12 密码 |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: ... (TEAMID)` |
| `APPLE_ID` / `APPLE_TEAM_ID` / `APPLE_APP_PASSWORD` | 公证凭据 |
| `WIN_CERT_PFX_BASE64` | Windows `.pfx` 的 base64 |
| `WIN_CERT_PASSWORD` | pfx 密码 |

CI 会从 GitHub Releases 现拉 frpc（`FRP_VERSION` 环境变量控制），不依赖仓库里是否提交了二进制。

## 4. 本地自查

```bash
bash scripts/verify-macos.sh src-tauri/target/aarch64-apple-darwin/release/bundle/macos/frpc-desktop.app
```

六项全绿才算能发：

1. 主程序是 `Developer ID Application` 且开 hardened runtime
2. **sidecar `frpc` 也签了**（这是最容易漏的一项）
3. `codesign --verify --deep --strict` 通过
4. `spctl` accepted
5. `stapler validate` 通过（否则离线首启被拦）
6. 无 `com.apple.quarantine` 残留

## 5. 故障排查

| 现象 | 原因 / 处理 |
| --- | --- |
| `errSecInternalComponent` | keychain 被锁：`security unlock-keychain -p <pwd> ~/Library/Keychains/login.keychain-db` |
| 公证 `Invalid`，日志指向 `frpc` | sidecar 没签或没带 `--options runtime`，回到第 3 步重签 |
| 公证 `Invalid`：`The executable does not have the hardened runtime enabled` | 主程序 `--options runtime` 漏了 |
| `spctl` 报 `source=Unnotarized Developer ID` | 没有 staple，`xcrun stapler staple <app/dmg>` |
| 用户装完仍提示"无法验证开发者" | 分发的是未公证的旧包，或 zip 二次压缩破坏了签名 |
| Windows SmartScreen 警告 | OV 证书未积累信誉；EV 立即生效 |
| Defender 报 `HackTool:Win32/Frp` | 独立问题，见下节；签名不解决这个 |
| `notarytool submit` 401 | 用的是账号密码而不是 App 专用密码 |

## 6. 关于杀毒软件误报

签名 + 公证解决的是"**未签名开发者被系统拦**"，**不解决**"frp 是隧道工具被杀软按类别拦"。

后者只能靠：

1. 提交误报申诉：<https://www.microsoft.com/en-us/wdsi/filesubmission>（选 Software developer，说明是开源 frp 的 GUI 封装）
2. 火绒 / 360 / 腾讯管家 各自的白名单入口
3. 产品侧降低可疑特征：不做注册表隐藏自启、不常驻守护、日志可见、安装包带发行者信息

申诉材料建议一次给全：签名主体、仓库地址、安装包 SHA256、frpc 官方 release 的 SHA256 对照。
