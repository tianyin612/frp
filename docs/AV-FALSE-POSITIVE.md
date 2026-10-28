# 被杀毒软件拦截 / 误报

## 现象

安装完成后，`C:\Program Files\frpc-desktop\` 里只有 `frpc-desktop.exe`，
**`frpc.exe` 不见了**；或者安装过程中弹窗提示「发现病毒/木马」。

这不是安装包坏了，也不是本项目被投毒。原因有两层：

1. **frp 是内网穿透/隧道工具**，杀软按"工具类别"而不是"具体文件"判定，
   直接归类为 `HackTool` / `RiskWare` / 远控后门。挖矿、勒索团伙大量滥用 frp，
   所以这一类程序天然高命中。
2. **产物未签名**（没有代码签名证书），系统与杀软没有任何信誉可依据。

> 签名能解决"未知发布者/SmartScreen"。**按类别的启发式查杀签名也压不住**，
> 那只能靠厂商白名单（下面第二节）。

## 一、立刻能用：把目录加进信任区，然后修复安装

被杀软删掉的只是 `frpc.exe`，主程序还在。加白之后重新跑一次安装包（选"修复"）即可补回。

### 腾讯电脑管家

1. 打开电脑管家 → 病毒查杀 → 隔离区，确认 `frpc.exe` 在隔离区里（能还原就还原）
2. 设置 → 信任区 / 白名单 → 添加目录：
   - `C:\Program Files\frpc-desktop`
3. 重新双击安装包，选"修复"

> 注意：本机实测电脑管家的实时防护服务（`QQPCRTP`）有自我保护，**管理员也停不掉**，
> 所以只能从界面加白，不要试图用命令行关它。

### Windows Defender

```powershell
# 管理员 PowerShell
Add-MpPreference -ExclusionPath "C:\Program Files\frpc-desktop"
```

或：Windows 安全中心 → 病毒和威胁防护 → 管理设置 → 排除项 → 添加文件夹。

### 360 / 火绒

各自「信任区 / 白名单」里添加安装目录；火绒是「防护中心 → 信任区」。

## 二、长期解法：提交误报 + 代码签名

### 提交误报（免费，几天生效，这是唯一能真正改掉查杀结果的途径）

**Microsoft**：<https://www.microsoft.com/en-us/wdsi/filesubmission>
选 *Software developer* → 上传文件 → 说明"开源 frp 的 GUI 封装，非恶意软件"。

**腾讯**：电脑管家 → 右上角反馈 / 或 <https://guanjia.qq.com/online_server/report.html> 提交误报。

材料建议一次给全：

| 项 | 值 |
| --- | --- |
| 文件 | `frpc.exe`（frp 0.52.3 windows_amd64） |
| SHA256 | `7251A83DBC0D7CF3975D21B03BCBE79723B5E0B1F7071C68EEF81862FA58CF8B` |
| 大小 | 14,151,680 字节 |
| 上游来源 | <https://github.com/fatedier/frp/releases/tag/v0.52.3> |
| 说明 | 开源项目 fatedier/frp 的官方发行二进制，本项目仅做 GUI 封装与分发 |

> 复核哈希：`certutil -hashfile frpc.exe SHA256`

### 代码签名

拿到 Windows 代码签名证书（OV/EV）后，配置这几个 Secrets 即可，工作流自动改走签名路径：

- `WIN_CERT_PFX_BASE64`：`.pfx` 的 base64
- `WIN_CERT_PASSWORD`：pfx 密码

EV 证书对 SmartScreen 立即生效；OV 需要下载量累积。**但对"按类别查杀"仍需配合上面的误报申诉。**

## 三、备选：不内置 frpc

如果不希望安装包带上这个高命中文件，可以只装 GUI，让用户自带 `frpc.exe`：

1. 安装后打开应用 → 设置 → **自定义 frpc 路径** → 指向任意位置的 `frpc.exe`
2. 或构建时去掉 sidecar（`tauri.conf.json` 里删掉 `bundle.externalBin`），
   这样安装过程不会再触发查杀，应用启动时会提示缺少内核并给出手动指定入口

## 四、应用内的表现

当 `frpc.exe` 缺失时，点"启动"会直接报：

```
找不到 frpc 内核 —— 内置的 frpc.exe 很可能已被杀毒软件隔离或删除
...
1) 把杀毒软件的「信任区/白名单」加上安装目录，然后重新运行安装包修复一次；
2) 或自行下载 frpc.exe 放到任意位置，在「设置 → 自定义 frpc 路径」里指定它；
```

不会出现"点启动什么都没发生"这种沉默失败。
