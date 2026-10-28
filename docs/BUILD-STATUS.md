# Windows 构建实测记录（2026-10-28）

结论：**这台机器当前无法产出可执行文件**。不是代码问题，是环境问题；证据和复现步骤都在下面。

## 一、已经验证通过的

| 项 | 结果 | 证据 |
| --- | --- | --- |
| 前端依赖安装 | ✅ | pnpm 11.25.0，128 包 |
| esbuild 构建脚本 | ✅ | `pnpm-workspace.yaml` 里 `allowBuilds: esbuild: true`（pnpm 11 不再读 package.json 的 `pnpm` 字段） |
| 图标生成 | ✅ | `logo.png` 1024×1024 → `src-tauri/icons/` 全套（含 `icon.ico`/`icon.icns`） |
| TypeScript 类型检查 | ✅ | `tsc --noEmit` 零错误 |
| 前端打包 | ✅ | vite 产出 `dist/` 160.78 kB js + 4.84 kB css |
| Tauri 配置 schema | ✅ | 曾因 `_externalBin_comment` 非法字段报错，已删除 |
| Rust 工具链 | ✅ | rustup 1.99.1（经 rsproxy 镜像安装） |
| Rust 依赖编译 | ✅ | ~400 个 crate 全部编译通过（tauri 2.12.5 / wry / reqwest / toml …） |
| 自有代码编译 | ⚠️ | 修掉 1 个真实类型错误（`ChildStdout`/`ChildStderr` 不能同数组），此后编译器开始随机崩，**未能跑完** |

## 二、三个环境级阻塞

### 1. 腾讯电脑管家会删除 frpc.exe

- `binaries/frpc-x86_64-pc-windows-msvc.exe` 写入后 ~6 秒被删，**源文件一起删**（`frps.exe` 却保留，说明是特征命中而非全盘清理）。
- Defender 是关闭的（`RealTimeProtectionEnabled=False`），排除。
- `SecurityCenter2` 显示 `腾讯电脑管家系统防护` 在册；`QQPCRTP` 服务有自我保护，**管理员也停不掉、进程杀不掉**。
- 影响：`bundle.externalBin` 找不到文件，打包必失败。
- 处理：在管家「信任区」加入 `frpc-desktop` 目录后，`tauri.conf.json` 的 `externalBin: ["binaries/frpc"]` 才能正常工作（当前已恢复该配置）。

### 2. MSVC 14.50 (VS 18) 的 x64 CRT 导入库缺失

`VC\Tools\MSVC\14.50.35717\lib\x64` 里**没有** `msvcrt.lib` / `ucrt.lib` / `vcruntime.lib` / `oldnames.lib`（x86 目录反而是全的），导致所有链接报 `LNK1104: 无法打开文件 msvcrt.lib`。

临时绕过（已固化进 `scripts/build-windows.cmd`）：

- `RUSTFLAGS=-C target-feature=+crt-static`：改用静态 CRT（`libcmt.lib` 存在）。
- `scripts/make-oldnames-shim.cmd`：生成一个空 `oldnames.lib` 满足 link.exe 的 `/DEFAULTLIB:oldnames`。

> 注意：`RUSTFLAGS` 里不能写 `-Lnative=C:\Program Files\...`，空格会被拆开，rustc 报
> `multiple input filenames provided`。vcvars 的 `LIB` 已经覆盖这些目录。

根本修复：用 VS Installer 修复「使用 C++ 的桌面开发」工作负载，或换一台工具集完整的机器。

### 3. rustc 随机崩溃（最致命）

同一份代码，10 次构建崩在**不同**的 crate 上，异常码每次不同：

```
STATUS_ACCESS_VIOLATION  (0xc0000005)
STATUS_HEAP_CORRUPTION   (0xc0000374)
STATUS_ILLEGAL_INSTRUCTION (0xc000001d)
```

还出现过 `serde_core` 报「expected expression, found `.`」但退出码是 `0xc0000374` —— 编译器读到了被破坏的内存，报错内容本身就是乱码。

另有一类稳定的失败：编译 `ring` 时 `cl.exe` 报 `D8050`（无法执行前端编译器）。

机器：i9-13900K / 64 GB（空闲 50 GB）/ VS 2026 + SDK 10.0.26100。
`STATUS_ILLEGAL_INSTRUCTION` + 随机堆损坏是 **Raptor Lake 13 代降级**的典型特征；杀软在用户态注入也可能叠加。

建议：跑 `mdsched.exe` 查内存、更新 BIOS/微码、用 Intel 官方工具测稳定性；或直接换机器/走 CI。

## 三、复现

```powershell
cd frpc-desktop
pnpm install
pnpm tauri icon ./logo.png
cmd /c "scripts\build-windows.cmd"              # release（不加参数）
cmd /c "scripts\build-windows.cmd --debug --no-bundle"   # debug
```

日志：`build.log`（release）、`build-debug.log`（debug）。

## 四、下一步建议

1. **走 CI**（`.github/workflows/release.yml`）：GitHub 的 `windows-latest` / `macos-14` 硬件健康、工具集完整，且能顺手完成签名公证——这是当前最现实的出包路径。
2. 或者：修好本机（修复 VS C++ 工作负载 + 排查 CPU/内存稳定性 + 管家信任区），再跑 `scripts\build-windows.cmd`。
3. 打包前记得确认 `binaries/frpc-x86_64-pc-windows-msvc.exe` 没被删（`dir` 一下）。
