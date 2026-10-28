# SignPath 免费代码签名（开源项目）

SignPath Foundation 给开源项目**免费**签发代码签名证书，私钥存在他们的 HSM 里，
不需要个人实名认证——它验证的是"**这个二进制确实由这个仓库构建**"，并以 SignPath 的名义担保。

- 申请页：<https://signpath.org/apply>
- 免费范围说明：<https://signpath.org/>
- 接入文档：<https://docs.signpath.io/trusted-build-systems/github>

## 一、适用范围（先看这个）

| 平台 | 支持情况 |
| --- | --- |
| Windows（exe / dll / msi / ps1） | ✅ 支持 Authenticode 签名 |
| macOS（app / dmg） | ❌ **不支持**，仍需 Apple Developer ID + 公证（$99/年） |
| Linux | ❌ 不适用（无 Authenticode） |

也就是说：SignPath 解决 **Windows 的 SmartScreen 和"未知发布者"**，macOS 那条路要另找。

## 二、申请前必须满足

1. 仓库是**公开**的，且有明确的开源许可证（本项目是 Apache-2.0，见 `LICENSE`）
2. 至少有一个可用的构建产物（Release 或 Actions 产物）
3. **构建必须全部跑在 GitHub 托管的 runner 上**（不能用自建 runner）——本项目符合
4. 接受 SignPath 的服务条款，并在 README 里保留项目来源说明

## 三、申请要填的内容（可直接复制）

- **Project name**：`frpc-desktop`
- **Repository**：<https://github.com/tianyin612/frp>
- **License**：Apache-2.0
- **Homepage**：（可选）仓库地址
- **Description**（英文，建议）：

  > frpc-desktop is a cross-platform desktop GUI for frp (fatedier/frp), the
  > open-source reverse proxy for NAT traversal. The app manages frpc
  > configuration files, controls the frpc process lifecycle, and streams logs.
  > It ships the unmodified upstream frpc binary as a sidecar.
  >
  > Signed artifacts would be the Windows installer (MSI/NSIS) and the two
  > executables inside it: frpc-desktop.exe (our code) and frpc.exe (upstream).

- **Artifacts to sign**：`frpc-desktop.exe`、`frpc.exe`、`*.msi`、`*.exe`(NSIS)
- **Build system**：GitHub Actions（`windows-latest`，全部 GitHub 托管）

## 四、批准后要做的事

### 1. 安装 GitHub App 并接通

1. 安装 <https://github.com/apps/signpath>，授权本仓库
2. 在 SignPath 后台把 **Trusted Build System "GitHub.com"** 加进组织，并**关联到项目**
3. 建一个项目，slug 建议 `frpc-desktop`

### 2. 建 Artifact Configuration

仓库里放了两份参考配置，需要在 SignPath 后台**校验后**再保存：

- `signpath/artifact-binaries.xml` —— 待签的两个 exe（先签它们，再打安装包）
- `signpath/artifact-installers.xml` —— 最终安装包

> 注意：`upload-artifact` 默认打成 ZIP，所以配置根节点用 `<zip-file>`。
> 若改用 `archive: false` 直传，则需在提交签名时加 `skip-decompress: true` 并调整配置。

### 3. 配置 Secrets / Variables

| 名称 | 类型 | 值 |
| --- | --- | --- |
| `SIGNPATH_API_TOKEN` | Secret | SignPath 后台生成的 API Token（submitter 权限） |
| `SIGNPATH_ORGANIZATION_ID` | Variable | 组织 ID |
| `SIGNPATH_PROJECT_SLUG` | Variable | 项目 slug，如 `frpc-desktop` |
| `SIGNPATH_SIGNING_POLICY_SLUG` | Variable | 签名策略 slug，如 `release-signing` |
| `SIGNPATH_ENABLED` | Variable | `true` —— 用来开关 `.github/workflows/windows-signed.yml` |

### 4. 跑一次验证

工作流 → **windows-signed** → Run workflow。成功后产物就是签过名的。

## 五、签名顺序（关键，别搞反）

安装包里嵌着两个 exe，如果先打 MSI 再签，MSI 里的 exe 仍然是未签名的，
用户装完看到的还是"未知发布者"。正确顺序：

```
pnpm tauri build --no-bundle     # 只出 exe
      ↓
签名请求 #1：frpc-desktop.exe + frpc.exe
      ↓
pnpm tauri bundle                # 用已签名的 exe 打 MSI/NSIS
      ↓
签名请求 #2：MSI / NSIS 安装包
      ↓
发布
```

`.github/workflows/windows-signed.yml` 就是按这个顺序写的。

## 六、免费额度的边界

- 仅限开源项目，且构建必须可被 SignPath 追溯到本仓库
- 证书有效期 1 年，到期需续（续期免费，但仍需重新走一次审核）
- 签名请求有频率限制（合理使用范围内无感）
- **不解决 `frpc.exe` 被按工具类别查杀的问题** —— 那是误报申诉的事，见 `docs/AV-FALSE-POSITIVE.md`
