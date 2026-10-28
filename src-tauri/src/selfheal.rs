//! 内置内核缺失时的自愈能力。
//!
//! 背景：frp 是内网穿透工具，容易被杀毒软件按类别误报并删除 frpc.exe。
//! 无论内置还是用户自备，那个文件都可能被删，所以产品必须能自己恢复：
//!   1. 自检报告状态（frpc_health）
//!   2. 从官方 release 重新下载并校验（download_frpc）
//!   3. 一键把安装目录加进 Windows Defender 排除项（add_defender_exclusion）
//!
//! 下载逻辑刻意用系统自带工具解压（Windows 的 Expand-Archive / unix 的 tar），
//! 不引第三方解压库；SHA256 用 sha2 计算并与官方 checksums 文件比对。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};

use crate::AppState;

const FRP_VERSION: &str = "0.52.3";
const MIN_ARCHIVE_BYTES: usize = 1_000_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrpcHealth {
    pub ok: bool,
    pub path: Option<String>,
    /// builtin | custom | missing
    pub source: String,
    pub hint: String,
    /// 主程序所在目录，用于「一键加白」
    pub app_dir: Option<String>,
}

fn app_dir() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()))
}

fn platform_asset() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("windows_amd64", "zip")
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            ("darwin_arm64", "tar.gz")
        } else {
            ("darwin_amd64", "tar.gz")
        }
    } else {
        ("linux_amd64", "tar.gz")
    }
}

fn frpc_file_name() -> &'static str {
    if cfg!(windows) {
        "frpc.exe"
    } else {
        "frpc"
    }
}

fn mirrors() -> [String; 3] {
    [
        format!("https://github.com/fatedier/frp/releases/download/v{FRP_VERSION}/"),
        format!("https://gh-proxy.com/https://github.com/fatedier/frp/releases/download/v{FRP_VERSION}/"),
        format!("https://ghfast.top/https://github.com/fatedier/frp/releases/download/v{FRP_VERSION}/"),
    ]
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn walk_find(root: &Path, name: &str, out: &mut Option<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_find(&path, name, out);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case(name))
            .unwrap_or(false)
            && out.is_none()
        {
            *out = Some(path);
        }
    }
}

#[cfg(windows)]
fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    let script = format!(
        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        archive.display(),
        dest.display()
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script.as_str()])
        .output()
        .map_err(|e| format!("调用 PowerShell 失败：{e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(unix)]
fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    let out = Command::new("tar")
        .args([
            "-xzf",
            archive.to_string_lossy().as_ref(),
            "-C",
            dest.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| format!("调用 tar 失败：{e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/* ------------------------------------------------------------------ 自检 */

#[tauri::command]
pub fn frpc_health(app: AppHandle, state: State<'_, AppState>) -> Result<FrpcHealth, String> {
    let settings = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.data.settings.clone()
    };

    match crate::commands::resolve_frpc(&app, &settings) {
        Ok((path, builtin)) => Ok(FrpcHealth {
            ok: true,
            path: Some(path.to_string_lossy().to_string()),
            source: if builtin { "builtin".into() } else { "custom".into() },
            hint: String::new(),
            app_dir: app_dir(),
        }),
        Err(e) => Ok(FrpcHealth {
            ok: false,
            path: None,
            source: "missing".into(),
            hint: e,
            app_dir: app_dir(),
        }),
    }
}

/* ------------------------------------------------------------ 下载并恢复 */

#[tauri::command]
pub async fn download_frpc(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let client = state.http.clone();
    let (plat, ext) = platform_asset();
    let asset = format!("frp_{FRP_VERSION}_{plat}.{ext}");
    let checksums_name = "frp_sha256_checksums.txt";
    let bases = mirrors();

    // 1) 官方 checksums（拿不到不阻断，但会在下面提示）
    let mut expected: Option<String> = None;
    for base in &bases {
        let url = format!("{base}{checksums_name}");
        if let Ok(resp) = client.get(&url).timeout(Duration::from_secs(20)).send().await {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Some(line) = text.lines().find(|l| l.contains(&asset)) {
                        if let Some(h) = line.split_whitespace().next() {
                            expected = Some(h.to_ascii_lowercase());
                            break;
                        }
                    }
                }
            }
        }
    }

    // 2) 依次尝试镜像下载
    let mut payload: Option<Vec<u8>> = None;
    let mut last_err = String::from("没有可用镜像");
    for base in &bases {
        let url = format!("{base}{asset}");
        match client.get(&url).timeout(Duration::from_secs(180)).send().await {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(bytes) if bytes.len() >= MIN_ARCHIVE_BYTES => {
                    payload = Some(bytes.to_vec());
                    break;
                }
                Ok(bytes) => last_err = format!("{url} 返回内容过小（{} 字节）", bytes.len()),
                Err(e) => last_err = format!("{url} 读取失败：{e}"),
            },
            Ok(resp) => last_err = format!("{url} 返回 HTTP {}", resp.status()),
            Err(e) => last_err = format!("{url} 请求失败：{e}"),
        }
    }

    let payload = payload.ok_or_else(|| {
        format!("下载失败：{last_err}\n\n可尝试：把浏览器能下载的 frp 压缩包手动放到安装目录，或用「设置 → 自定义 frpc 路径」直接指定。")
    })?;

    // 3) 校验
    let actual = sha256_hex(&payload);
    match &expected {
        Some(exp) if exp != &actual => {
            return Err(format!("SHA256 校验不通过：期望 {exp}，实际 {actual}（已中止）"))
        }
        _ => {}
    }

    // 4) 落盘并解压
    let base_dir = crate::commands::data_dir(&app)?.join("frpc");
    std::fs::create_dir_all(&base_dir).map_err(|e| format!("创建目录失败：{e}"))?;

    let archive = base_dir.join(&asset);
    std::fs::write(&archive, &payload).map_err(|e| format!("写入压缩包失败：{e}"))?;

    let extract_dir = base_dir.join("extract");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).map_err(|e| format!("创建解压目录失败：{e}"))?;
    extract_archive(&archive, &extract_dir)?;

    let mut found = None;
    walk_find(&extract_dir, frpc_file_name(), &mut found);
    let found = found.ok_or("压缩包里没找到 frpc，请手动指定")?;

    let target = base_dir.join(frpc_file_name());
    std::fs::copy(&found, &target).map_err(|e| format!("复制 frpc 失败：{e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }

    // 5) 写回设置，让解析逻辑优先用它
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        store.data.settings.frpc_path = Some(target.to_string_lossy().to_string());
        store.save()?;
    }

    let note = if expected.is_none() {
        "（注意：未能获取官方校验和，未做完整性校验）"
    } else {
        "（SHA256 已校验）"
    };
    Ok(format!("已恢复：{}{note}", target.display()))
}

/* ------------------------------------------------- Windows Defender 加白 */

#[tauri::command]
pub fn add_defender_exclusion(path: String) -> Result<String, String> {
    #[cfg(windows)]
    {
        // 防注入：路径里不允许出现引号/换行，否则直接拒绝
        if path.contains('\'') || path.contains('"') || path.contains('\n') || path.contains('\r') {
            return Err("路径包含非法字符".into());
        }
        if !Path::new(&path).exists() {
            return Err(format!("路径不存在：{path}"));
        }

        // 写成临时脚本再提权执行，避免多层引号嵌套
        let script_path = std::env::temp_dir().join("frpc-desktop-add-exclusion.ps1");
        std::fs::write(
            &script_path,
            format!("Add-MpPreference -ExclusionPath '{path}'\nexit $LASTEXITCODE\n"),
        )
        .map_err(|e| format!("写临时脚本失败：{e}"))?;

        let outer = format!(
            "Start-Process -FilePath powershell -Verb RunAs -Wait -ArgumentList @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{}')",
            script_path.display()
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", outer.as_str()])
            .output()
            .map_err(|e| format!("调用 PowerShell 失败：{e}"))?;

        let _ = std::fs::remove_file(&script_path);

        if out.status.success() {
            Ok("已提交 Defender 排除项请求（UAC 弹窗里点了「是」才会生效）".into())
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(format!(
                "加白失败：{err}\n\n若 UAC 弹窗点了「否」，或系统策略禁止修改排除项，请手动在 Windows 安全中心添加。"
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("该功能仅 Windows 可用".into())
    }
}
