//! Tauri 命令层：前端能调的都在这里。
//!
//! 约定：所有错误统一 `Result<T, String>`，前端直接把文案弹出来。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::admin;
use crate::config;
use crate::model::{new_id, AppSettings, Profile, Server, StoreData};
use crate::store::atomic_write;
use crate::supervisor::{LogLine, RunningInfo};
use crate::AppState;

/* --------------------------------------------------------------- 工具函数 */

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败：{e}"))
}

fn config_path(app: &AppHandle, profile_id: &str) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("profiles").join(format!("{profile_id}.toml")))
}

#[cfg(unix)]
fn ensure_exec(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
}

#[cfg(not(unix))]
fn ensure_exec(_p: &Path) {}

/// 解析 frpc 可执行文件：自定义路径 → 打包资源目录 → 可执行文件同目录 → 开发期 binaries/
fn resolve_frpc(app: &AppHandle, settings: &AppSettings) -> Result<(PathBuf, bool), String> {
    let exe_name = if cfg!(windows) { "frpc.exe" } else { "frpc" };

    if let Some(raw) = settings.frpc_path.as_ref().filter(|p| !p.trim().is_empty()) {
        let pb = PathBuf::from(raw.trim());
        if pb.is_file() {
            ensure_exec(&pb);
            return Ok((pb, false));
        }
        return Err(format!("自定义 frpc 路径不存在：{}", pb.display()));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = app.path().resource_dir() {
        candidates.push(rd.join(exe_name));
        candidates.push(rd.join("binaries").join(exe_name));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(exe_name));
        }
    }
    let triple = env!("TARGET_TRIPLE");
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    candidates.push(PathBuf::from(format!("binaries/frpc-{triple}{suffix}")));
    candidates.push(PathBuf::from(format!(
        "../src-tauri/binaries/frpc-{triple}{suffix}"
    )));

    for c in &candidates {
        if c.is_file() {
            ensure_exec(c);
            return Ok((c.clone(), true));
        }
    }

    let tried = candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n  ");
    Err(format!(
        "找不到 frpc 内核 —— 内置的 frpc.exe 很可能已被杀毒软件隔离或删除（frp 是内网穿透工具，容易被按类别误报）。\n\n\
         已尝试以下位置：\n  {tried}\n\n\
         处理办法（任选其一）：\n\
         1) 把杀毒软件的「信任区/白名单」加上安装目录（或整个 frpc-desktop 文件夹），然后重新运行安装包修复一次；\n\
         2) 或自行下载 frpc.exe 放到任意位置，在「设置 → 自定义 frpc 路径」里指定它；\n\
         3) 详见项目内 docs/AV-FALSE-POSITIVE.md。"
    ))
}

fn run_frpc(frpc: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(frpc)
        .args(args)
        .output()
        .map_err(|e| format!("执行 frpc 失败：{e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else if stderr.is_empty() {
        Err(stdout)
    } else {
        Err(stderr)
    }
}

/// 生成配置并落盘，返回 (path, toml 文本)
fn write_profile_config(
    app: &AppHandle,
    state: &AppState,
    profile: &Profile,
) -> Result<(PathBuf, String), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let server = store
        .data
        .server(&profile.server_id)
        .ok_or("该配置还没有绑定服务器")?;
    let text = config::build(profile, server, &store.data.settings)?;
    drop(store);

    let path = config_path(app, &profile.id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    atomic_write(&path, text.as_bytes())?;
    Ok((path, text))
}

fn start_inner(app: &AppHandle, state: &AppState, profile_id: &str) -> Result<(), String> {
    let profile = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .data
            .profile(profile_id)
            .cloned()
            .ok_or("配置不存在")?
    };
    let settings = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.data.settings.clone()
    };

    let (cfg_path, _text) = write_profile_config(app, state, &profile)?;
    let (frpc, _builtin) = resolve_frpc(app, &settings)?;

    // 先校验：配置写错时在 GUI 里报出来，而不是让进程一闪而过
    let cfg_str = cfg_path.to_string_lossy().to_string();
    run_frpc(&frpc, &["verify", "-c", cfg_str.as_str()])
        .map_err(|e| format!("配置校验失败：\n{e}"))?;

    if state.sup.is_running(profile_id) {
        state.sup.stop(profile_id)?;
    }

    let mut cmd = Command::new(&frpc);
    cmd.arg("-c").arg(&cfg_path);
    state.sup.spawn(profile_id, cmd, cfg_path)?;
    crate::tray::refresh_status(app);
    Ok(())
}

/* --------------------------------------------------------------- 全局状态 */

#[tauri::command]
pub fn get_app_state(state: State<'_, AppState>) -> Result<StoreData, String> {
    Ok(state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .data
        .clone())
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<StoreData, String> {
    let (data, launch_at_login) = {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        store.data.settings = settings;
        store.save()?;
        state.sup.set_max_lines(store.data.settings.log_max_lines);
        (store.data.clone(), store.data.settings.launch_at_login)
    };
    // 同步到系统（LaunchAgent / Run 键），失败只打日志
    crate::autostart::apply(&app, launch_at_login);
    Ok(data)
}

/* ----------------------------------------------------------------- 服务器 */

#[tauri::command]
pub fn save_server(state: State<'_, AppState>, server: Server) -> Result<StoreData, String> {
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    store.data.upsert_server(server);
    store.save()?;
    Ok(store.data.clone())
}

#[tauri::command]
pub fn delete_server(state: State<'_, AppState>, server_id: String) -> Result<StoreData, String> {
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    store.data.remove_server(&server_id);
    store.save()?;
    Ok(store.data.clone())
}

/* ------------------------------------------------------------------- 配置 */

#[tauri::command]
pub fn save_profile(state: State<'_, AppState>, profile: Profile) -> Result<StoreData, String> {
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    store.data.upsert_profile(profile);
    store.save()?;
    Ok(store.data.clone())
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, profile_id: String) -> Result<StoreData, String> {
    state.sup.stop(&profile_id)?;
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    store.data.remove_profile(&profile_id);
    store.save()?;
    Ok(store.data.clone())
}

#[tauri::command]
pub fn duplicate_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<StoreData, String> {
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    store
        .data
        .duplicate_profile(&profile_id)
        .ok_or("要复制的配置不存在")?;
    store.save()?;
    Ok(store.data.clone())
}

/* --------------------------------------------------------- 生成 / 校验配置 */

#[tauri::command]
pub fn preview_config(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .data
            .profile(&profile_id)
            .cloned()
            .ok_or("配置不存在")?
    };
    let (_, text) = write_profile_config(&app, state.inner(), &profile)?;
    Ok(text)
}

#[tauri::command]
pub fn verify_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store
            .data
            .profile(&profile_id)
            .cloned()
            .ok_or("配置不存在")?
    };
    let settings = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.data.settings.clone()
    };
    let (cfg_path, _) = write_profile_config(&app, state.inner(), &profile)?;
    let (frpc, _) = resolve_frpc(&app, &settings)?;
    let cfg = cfg_path.to_string_lossy().to_string();
    run_frpc(&frpc, &["verify", "-c", cfg.as_str()])
}

/* --------------------------------------------------------------- 生命周期 */

#[tauri::command]
pub fn start_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    start_inner(&app, state.inner(), &profile_id)
}

#[tauri::command]
pub fn stop_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    state.sup.stop(&profile_id)?;
    crate::tray::refresh_status(&app);
    Ok(())
}

#[tauri::command]
pub fn restart_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    state.sup.stop(&profile_id)?;
    start_inner(&app, state.inner(), &profile_id)
}

#[tauri::command]
pub fn get_runtime(state: State<'_, AppState>) -> Result<Vec<RunningInfo>, String> {
    Ok(state.sup.running())
}

/// 启动时自动拉起 auto_start 的配置（失败不阻塞 UI）
pub fn autostart(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let ids: Vec<String> = {
        let Ok(store) = state.store.lock() else { return };
        store
            .data
            .profiles
            .iter()
            .filter(|p| p.auto_start && !p.server_id.is_empty())
            .map(|p| p.id.clone())
            .collect()
    };
    for id in ids {
        if let Err(e) = start_inner(app, state.inner(), &id) {
            eprintln!("[autostart] 配置 {id} 启动失败：{e}");
        }
    }
    crate::tray::refresh_status(app);
}

/* ------------------------------------------------------------- 托盘批量操作 */

/// 托盘菜单「启动全部配置」：跳过没绑定服务器的，单个失败不影响其他
pub fn start_all(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let ids: Vec<String> = {
        let Ok(store) = state.store.lock() else { return };
        store
            .data
            .profiles
            .iter()
            .filter(|p| !p.server_id.is_empty())
            .map(|p| p.id.clone())
            .collect()
    };
    for id in ids {
        if let Err(e) = start_inner(app, state.inner(), &id) {
            eprintln!("[start_all] 配置 {id} 启动失败：{e}");
        }
    }
    crate::tray::refresh_status(app);
}

/// 托盘菜单「停止全部配置」
pub fn stop_all_profiles(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.sup.stop_all();
    }
    crate::tray::refresh_status(app);
}

/* ------------------------------------------------------------------- 日志 */

#[tauri::command]
pub fn get_logs(
    state: State<'_, AppState>,
    profile_id: String,
    since_seq: u64,
) -> Result<Vec<LogLine>, String> {
    Ok(state.sup.logs(&profile_id, since_seq))
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>, profile_id: String) -> Result<(), String> {
    state.sup.clear_logs(&profile_id);
    Ok(())
}

/* --------------------------------------------------------------- 管理 API */

/// frpc 侧：拿到各代理运行状态（前端可画实时状态表）
#[tauri::command]
pub async fn admin_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Value, String> {
    let (base, user, pass) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let profile = store.data.profile(&profile_id).ok_or("配置不存在")?.clone();
        let server = store
            .data
            .server(&profile.server_id)
            .ok_or("该配置未绑定服务器")?
            .clone();
        let (user, pass) = config::admin_credentials(&profile.id, &server.token);
        (
            admin::base("127.0.0.1", config::admin_port(&profile, &store.data.settings)),
            user,
            pass,
        )
    };
    let client = state.http.clone();
    admin::get(&client, &format!("{base}/api/status"), &user, &pass).await
}

/// frpc 侧：热加载配置。失败时前端应该退回"重启进程"。
#[tauri::command]
pub async fn admin_reload(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<String, String> {
    let (profile, base, user, pass) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let profile = store.data.profile(&profile_id).ok_or("配置不存在")?.clone();
        let server = store
            .data
            .server(&profile.server_id)
            .ok_or("该配置未绑定服务器")?
            .clone();
        let (user, pass) = config::admin_credentials(&profile.id, &server.token);
        (
            profile.clone(),
            admin::base("127.0.0.1", config::admin_port(&profile, &store.data.settings)),
            user,
            pass,
        )
    };

    let (_, text) = write_profile_config(&app, state.inner(), &profile)?;
    let client = state.http.clone();
    let body = serde_json::json!({ "configContent": text });
    admin::post_json(&client, &format!("{base}/api/reload"), &user, &pass, &body).await?;
    Ok("已发送热加载请求".into())
}

/// frps 侧看板：服务端信息 + 各类代理列表
#[tauri::command]
pub async fn frps_dashboard(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Value, String> {
    let (base, user, pass) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let server = store
            .data
            .server(&server_id)
            .ok_or("服务器不存在")?
            .clone();
        (
            admin::base(&server.addr, server.frps_admin_port),
            server.frps_admin_user.clone(),
            server.frps_admin_password.clone(),
        )
    };

    let client = state.http.clone();
    let mut out = serde_json::Map::new();
    out.insert(
        "serverinfo".into(),
        admin::get(&client, &format!("{base}/api/serverinfo"), &user, &pass).await?,
    );

    let mut proxies = serde_json::Map::new();
    for kind in ["tcp", "udp", "http", "https", "stcp", "sudp", "xtcp", "tcpmux"] {
        if let Ok(v) = admin::get(&client, &format!("{base}/api/proxy/{kind}"), &user, &pass).await
        {
            proxies.insert(kind.to_string(), v);
        }
    }
    out.insert("proxies".into(), Value::Object(proxies));
    Ok(Value::Object(out))
}

/* ------------------------------------------------------------------- 内核 */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrpcInfo {
    pub path: String,
    pub version: String,
    pub builtin: bool,
}

#[tauri::command]
pub fn detect_frpc(app: AppHandle, state: State<'_, AppState>) -> Result<FrpcInfo, String> {
    let settings = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.data.settings.clone()
    };
    let (path, builtin) = resolve_frpc(&app, &settings)?;
    let version = run_frpc(&path, &["-v"]).unwrap_or_else(|_| "unknown".into());
    Ok(FrpcInfo {
        path: path.to_string_lossy().to_string(),
        version,
        builtin,
    })
}

/* --------------------------------------------------- 导入 / 导出 / 订阅 */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleOut {
    version: u32,
    exported_at: i64,
    servers: Vec<Server>,
    profiles: Vec<Profile>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct BundleIn {
    servers: Vec<Server>,
    profiles: Vec<Profile>,
}

#[tauri::command]
pub fn export_bundle(state: State<'_, AppState>) -> Result<String, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let bundle = BundleOut {
        version: 1,
        exported_at: crate::supervisor::now_secs(),
        servers: store.data.servers.clone(),
        profiles: store.data.profiles.clone(),
    };
    serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_bundle(state: State<'_, AppState>, json: String) -> Result<String, String> {
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let msg = import_text(&mut store.data, &json)?;
    store.save()?;
    Ok(msg)
}

#[tauri::command]
pub async fn fetch_subscription(state: State<'_, AppState>, url: String) -> Result<String, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("订阅地址必须是 http(s) 链接".into());
    }
    let client = state.http.clone();
    let text = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("拉取订阅失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("拉取订阅失败：{e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let msg = import_text(&mut store.data, &text)?;
    store.save()?;
    Ok(msg)
}

/// 统一导入入口：JSON 包 / base64 包裹的 JSON / 单个 frpc.toml
fn import_text(store: &mut StoreData, raw: &str) -> Result<String, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("导入内容为空".into());
    }

    if text.starts_with('{') {
        if let Ok(bundle) = serde_json::from_str::<BundleIn>(text) {
            if !bundle.servers.is_empty() || !bundle.profiles.is_empty() {
                return merge_bundle(store, bundle);
            }
        }
    }

    // base64 包裹（部分面板的导出格式）
    if let Ok(bytes) = base64_decode(text) {
        if let Ok(inner) = String::from_utf8(bytes) {
            let inner_trim = inner.trim();
            if inner_trim.starts_with('{') {
                if let Ok(bundle) = serde_json::from_str::<BundleIn>(inner_trim) {
                    if !bundle.servers.is_empty() || !bundle.profiles.is_empty() {
                        return merge_bundle(store, bundle);
                    }
                }
            }
            if let Ok((server, profile)) = config::parse_frpc_toml(inner_trim) {
                return push_pair(store, server, profile);
            }
        }
    }

    let (server, profile) = config::parse_frpc_toml(text)?;
    push_pair(store, server, profile)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| e.to_string())
}

fn push_pair(store: &mut StoreData, mut server: Server, mut profile: Profile) -> Result<String, String> {
    server.id = new_id();
    profile.id = new_id();
    profile.server_id = server.id.clone();
    for p in profile.proxies.iter_mut() {
        if p.id.is_empty() {
            p.id = new_id();
        }
    }
    let name = server.name.clone();
    store.servers.push(server);
    store.profiles.push(profile);
    Ok(format!("已导入 1 台服务器（{name}）和 1 个配置"))
}

fn merge_bundle(store: &mut StoreData, bundle: BundleIn) -> Result<String, String> {
    let mut id_map: HashMap<String, String> = HashMap::new();
    let server_count = bundle.servers.len();
    for mut s in bundle.servers {
        let old = s.id.clone();
        s.id = new_id();
        id_map.insert(old, s.id.clone());
        store.servers.push(s);
    }

    let profile_count = bundle.profiles.len();
    for mut p in bundle.profiles {
        p.id = new_id();
        if let Some(new_sid) = id_map.get(&p.server_id) {
            p.server_id = new_sid.clone();
        }
        for x in p.proxies.iter_mut() {
            if x.id.is_empty() {
                x.id = new_id();
            }
        }
        store.profiles.push(p);
    }

    Ok(format!("已导入 {server_count} 台服务器、{profile_count} 个配置"))
}
