# 打包、签名、公证 —— 让系统不再报"存在风险"

> 本文讲**原理和手动步骤**；日常发布走 `docs/RELEASE.md`（`pnpm release:mac` / `pnpm release:win` / CI），别手敲这些命令。

## 0. 根因回顾

系统报风险不是代码有问题，而是三件事叠加：**二进制未签名/未公证** + **frp 是典型的隧道双用途工具（杀软按工具类别拦）** + **下载包带 quarantine 标记**。

自研桌面端能解决第 1、3 条，第 2 条靠签名证书建立信誉 + 提交误报申诉。

## 1. macOS

### 1.1 证书

需要 Apple Developer Program（$99/年）：
- `Developer ID Application: Your Name (TEAMID)` —— 分发用
- 导入钥匙串，`security find-identity -v -p codesigning` 能看到

### 1.2 环境变量（Tauri 自动签名）

```bash
export APPLE_CERTIFICATE="$(base64 -i cert.p12)"
export APPLE_CERTIFICATE_PASSWORD="p12密码"
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="app-specific-password"   # 或 APPLE_API_KEY / APPLE_API_ISSUER
export APPLE_TEAM_ID="TEAMID"
```

`pnpm tauri build` 会自动 codesign + `notarytool submit` + `stapler staple`。

### 1.3 sidecar 必须一起签（最常见的翻车点）

Tauri 打包会深签整个 `.app`，但**内置的 frpc 是独立 Mach-O**，必须按"由内到外"顺序签，且要带 `--options runtime`：

```bash
codesign --force --options runtime --timestamp \
  --sign "Developer ID Application: Your Name (TEAMID)" \
  "dist/frpc-desktop.app/Contents/MacOS/frpc"

codesign --force --options runtime --timestamp \
  --sign "Developer ID Application: Your Name (TEAMID)" \
  "dist/frpc-desktop.app"

codesign --verify --deep --strict --verbose=2 "dist/frpc-desktop.app"
spctl -a -vvv -t exec "dist/frpc-desktop.app"   # 应输出 accepted / Notarized Developer ID
```

放在 `beforeBundleCommand` 里自动执行，或打包后跑一个 `scripts/sign-macos.sh`。

### 1.4 验证清单

```bash
xattr -l dist/frpc-desktop.app          # 不应有 com.apple.quarantine
codesign -dv --verbose=4 dist/frpc-desktop.app
codesign -dv --verbose=4 "dist/frpc-desktop.app/Contents/MacOS/frpc"
spctl -a -vvv -t exec dist/frpc-desktop.app
```

### 1.5 通用二进制（Intel + Apple Silicon）

```bash
lipo -create binaries/frpc-x86_64-apple-darwin binaries/frpc-aarch64-apple-darwin \
  -output binaries/frpc-universal-apple-darwin
pnpm tauri build --target universal-apple-darwin
```

## 2. Windows

### 2.1 代码签名

- OV 证书：便宜，但 SmartScreen 信誉要"养"（有量的下载才转白）
- EV 证书：**立即获得 SmartScreen 信誉**，推荐给分发场景

```powershell
signtool sign /fd sha256 /tr http://timestamp.digicert.com /td sha256 `
  /f cert.pfx /p 密码 `
  "target\release\frpc-desktop.exe"
```

Tauri 配置：

```json
"bundle": { "windows": { "signCommand": "signtool sign /fd sha256 /tr http://timestamp.digicert.com /td sha256 /f cert.pfx /p %PASSWORD% %1" } }
```

sidecar `frpc.exe` 也要签，否则 Defender 仍可能单独拦它。

### 2.2 误报申诉（必做）

即使签了名，`frpc.exe` 仍可能被 Defender 判 `HackTool:Win32/Frp`。提交误报：

- Microsoft：<https://www.microsoft.com/en-us/wdsi/filesubmission>（选 "Software developer" → 说明是开源 frp 打包分发）
- 火绒/360/腾讯管家：各自的白名单/误报反馈入口

申诉时把签名主体、官网/仓库地址、文件 SHA256 一起给，通过率更高。

## 3. 分发形态

| 平台 | 产物 | 说明 |
| --- | --- | --- |
| macOS | `.dmg` | 内含已签名公证的 `.app` |
| Windows | `.msi`（WiX）或 `.exe`（NSIS） | 带签名；NSIS 更适合自带更新 |
| 自动更新 | `tauri-plugin-updater` | 更新包同样要签名，公钥内置 |

## 4. 发布前 Checklist

- [ ] `frpc` 与主程序同一架构（arm64 包别塞 x86_64 内核）
- [ ] sidecar 已签名（macOS 带 `--options runtime`）
- [ ] 公证成功且 `stapler staple` 已执行
- [ ] 安装包无 `com.apple.quarantine` 残留
- [ ] Windows 安装包与 sidecar 均已 signtool 签名 + 时间戳
- [ ] 提交过误报申诉
- [ ] README 里写清楚"这是开源 frp 的 GUI，不是木马"，降低用户恐慌
