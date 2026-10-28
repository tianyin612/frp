# binaries/

放 frpc 内核，**文件名必须带 target triple 后缀**，Tauri 的 `externalBin` 按这个名字找文件：

| 平台 | 文件名 |
| --- | --- |
| macOS Apple Silicon | `frpc-aarch64-apple-darwin` |
| macOS Intel | `frpc-x86_64-apple-darwin` |
| macOS 通用 | `frpc-universal-apple-darwin` |
| Windows x64 | `frpc-x86_64-pc-windows-msvc.exe` |
| Linux x64 | `frpc-x86_64-unknown-linux-gnu` |

来源：<https://github.com/fatedier/frp/releases>（选对应版本的 `frp_<ver>_darwin_arm64.tar.gz` 等）。

```bash
# macOS
cp frp_0.52.3_darwin_arm64/frpc  src-tauri/binaries/frpc-aarch64-apple-darwin
chmod +x src-tauri/binaries/frpc-aarch64-apple-darwin

# 通用二进制
lipo -create binaries/frpc-x86_64-apple-darwin binaries/frpc-aarch64-apple-darwin \
  -output binaries/frpc-universal-apple-darwin
```

注意：
- 架构必须和打包目标一致（arm64 安装包里塞 x86_64 内核会 `Bad CPU type`）。
- 内置内核是 sidecar，**打包时必须在主程序签名之后再签它**（见 `docs/BUILD.md`），否则公证会失败。
- 不想内置就让用户在设置里填自定义 frpc 路径，走 `resolve_frpc()` 的覆盖分支。
