# 代码签名证书：能不能自己"做"？

**不能。** 代码签名证书是 CA（证书颁发机构）对**主体身份**的背书，必须实名核验后签发。
自己生成的叫自签名证书，Windows 不信任它：SmartScreen 照样拦、杀软照样报，
只能用于企业内部（把根证书通过组策略推到每台机器）——对分发给公众没有意义。

所以问题不是"能不能生成"，而是"**怎么以最低成本拿到一张受信任的证书**"。

## 一、可选路径对比

| 路径 | 价格 | 门槛 | 是否适合 CI |
| --- | --- | --- | --- |
| **Certum 开源代码签名（云）** | €329/年；**优秀开源项目可免费申请** | GitHub 托管、Star > 1k、至少一个 Release、申请人中国国籍 | ✅ 云签名，无 USB token，5000 次/月 |
| **SignPath（开源免费）** | 免费（需审核） | 开源项目，提交申请并接受其构建流程接入 | ✅ 官方 GitHub Action |
| **Azure Trusted Signing** | 约 $9.99/月 | Azure 订阅 + 组织/个人历史核验，部分场景还需要 Entra 配置 | ✅ 官方 Action |
| **传统 OV** | 约 $200–400/年 | 个人或企业实名 | ⚠️ 多为文件型 pfx，可上 CI |
| **传统 EV** | 约 $300–800/年 | 企业实名，**强制硬件 token 或云 HSM** | ⚠️ 硬件 token 无法上 CI，需买云 HSM |

> 价格来自公开渠道，随时变动，以各 CA 官网为准。
> Certum 的免费额度需要走人工审核（"Certum 保留最终解释权"），不是自动发放。

对本项目而言优先级建议：

1. **先冲 Star 数**（1k）→ 申请 Certum 免费开源证书。零成本、云签名、能直接接 CI。
2. 同时申请 **SignPath**（开源免费，不需要 Star 门槛，但要接受它托管签名流程）。
3. 如果急着上架、且是公司主体 → **Azure Trusted Signing**，月付最低、无需 token。

## 二、申请要准备什么

个人：

- 身份证 / 护照
- 手机号、邮箱
- 有时需要与证书主体一致的地址证明

企业：

- 营业执照
- 法人身份证明
- 企业电话回拨核验（CA 会打工商登记的电话）
- 有时要求邓白氏编码（D-U-N-S）

开源项目额外材料：

- 仓库地址（GitHub）
- Release 页面链接
- 项目许可证
- 说明"签名对象是本项目构建产物"

## 三、拿到证书后怎么接进来

### 方案 A：pfx 文件（OV / 部分云签名导出的 pfx）

1. 把 `.pfx` 转 base64，配成 Secrets：
   - `WIN_CERT_PFX_BASE64`
   - `WIN_CERT_PASSWORD`
2. 推送任意提交即可。工作流会检测到这两个 Secret，自动从"未签名构建"切到 `pnpm release:win`：
   先签 `frpc-desktop.exe` 和 `frpc.exe`，再打包，再签 `.msi` / `.exe` 安装包。
3. 本地签名：`scripts\windows\sign.ps1`（同样的逻辑，签名 + 时间戳 + 校验）。

### 方案 B：云 HSM（Certum SimplySign / Azure Trusted Signing）

不能导出 pfx，需要把签名交给云端 API：

- **Certum SimplySign**：在 CI 里安装其签名客户端，用账号凭据登录后调用本地签名接口；
  凭据放 Secrets，注意开启两步验证并用专用账号。
- **Azure Trusted Signing**：用 `azure/trusted-signing-action`，配置
  `AZURE_TENANT_ID` / `AZURE_CLIENT_ID` / `AZURE_CLIENT_SECRET` + endpoint/account/profile 名称。

两种都需要替换 `sign.ps1` 里的 `signtool sign /f` 调用，改成对应客户端命令。
现有脚本已经把"签名哪个文件"和"怎么校验"分开了，改动量很小。

## 四、本机工具链已就绪（已实测）

- `signtool.exe`：`C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe`
- 自签名冒烟测试通过：`signtool sign /sha1 <thumb> /fd sha256` 对 `frpc-desktop.exe` 签名成功，
  `Get-AuthenticodeSignature` 能读到签名者与有效期（状态为 `UnknownError`，因为自签名根不受信任——这是预期）。
- 当前安装的 `frpc-desktop.exe` 状态：`NotSigned`。

也就是说：**证书一到，`pnpm release:win` 就能直接出签名版，不需要改代码。**

## 五、必须说清楚的一件事

签名解决的是「未知发布者 / SmartScreen」这一类拦截。

**它不解决 frpc.exe 被按工具类别查杀的问题**（`HackTool` / `RiskWare`）。
那类判定只看"这是什么工具"，签名只证明"谁发布的"。要改掉它仍然必须走厂商误报申诉——
见 `docs/AV-FALSE-POSITIVE.md`。

实际最有效的组合是：**签名（降低整体可疑度）+ 误报申诉（改掉具体特征命中）+ 用户侧信任区（兜底）**。
