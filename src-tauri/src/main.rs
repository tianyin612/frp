// GUI 程序在 Windows 上不要附带控制台窗口（debug 构建保留，方便看 eprintln）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod admin;
mod autostart;
mod commands;
mod config;
mod model;
mod store;
mod supervisor;
mod tray;

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::MacosLauncher;

use store::Store;
use supervisor::Supervisor;

pub struct AppState {
    pub store: Mutex<Store>,
    pub sup: Supervisor,
    pub http: reqwest::Client,
}

/// 统一退出：先带走所有 frpc，再退出进程。
pub fn quit(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.sup.stop_all();
    }
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        // 开机自启时追加 --hidden，配合下面的"静默启动"分支
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;

            let store = Store::load(dir.join("store.json"));

            let sup = Supervisor::default();
            sup.set_max_lines(store.data.settings.log_max_lines);
            sup.set_pid_file(dir.join("running.json"));

            let http = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent(concat!("frpc-desktop/", env!("CARGO_PKG_VERSION")))
                .build()?;

            let launch_at_login = store.data.settings.launch_at_login;
            let minimize_to_tray = store.data.settings.minimize_to_tray;

            app.manage(AppState {
                store: Mutex::new(store),
                sup,
                http,
            });

            // 上次异常退出（崩溃/强杀）可能留下还在跑的 frpc，先清掉
            supervisor::cleanup_orphans(&dir.join("running.json"));

            // 托盘初始化失败不应该让应用起不来
            if let Err(e) = tray::setup(app.handle()) {
                eprintln!("[tray] 初始化失败：{e}");
            }

            // 把配置里的开机自启同步到系统（用户可能在别处直接改过）
            autostart::apply(app.handle(), launch_at_login);

            // 由开机自启拉起时静默进托盘，不弹窗打断用户
            let silent = std::env::args().any(|a| a == "--hidden");
            if silent && minimize_to_tray {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }

            // 等窗口出来再拉起 autoStart 的配置
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(900));
                commands::autostart(&handle);
                tray::refresh_status(&handle);
            });

            Ok(())
        })
        // 关窗 = 隐藏到托盘（仅当设置开启；关掉这条设置就是正常退出）
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let minimize = window
                    .app_handle()
                    .try_state::<AppState>()
                    .and_then(|state| {
                        state
                            .store
                            .lock()
                            .ok()
                            .map(|store| store.data.settings.minimize_to_tray)
                    })
                    .unwrap_or(false);

                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::save_settings,
            commands::save_server,
            commands::delete_server,
            commands::save_profile,
            commands::delete_profile,
            commands::duplicate_profile,
            commands::preview_config,
            commands::verify_profile,
            commands::start_profile,
            commands::stop_profile,
            commands::restart_profile,
            commands::get_runtime,
            commands::get_logs,
            commands::clear_logs,
            commands::admin_status,
            commands::admin_reload,
            commands::frps_dashboard,
            commands::detect_frpc,
            commands::export_bundle,
            commands::import_bundle,
            commands::fetch_subscription,
        ])
        .build(tauri::generate_context!())
        .expect("构建 frpc-desktop 失败")
        .run(|app_handle, event| match event {
            // 退出时务必带走所有 frpc 子进程，否则会变成后台孤儿
            tauri::RunEvent::ExitRequested { .. } => {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.sup.stop_all();
                }
            }
            // 注意：Tauri 2 的 RunEvent 里**没有** Reopen 变体（那是 v1 的 API，
            // 2.12 的变体只有 Exit/ExitRequested/WindowEvent/WebviewEvent/Ready/
            // Resumed/MainEventsCleared/MenuEvent/TrayIconEvent）。
            // 所以 macOS 上关窗隐藏后，靠点托盘图标把窗口叫回来，Dock 图标点了不会恢复。
            _ => {}
        });
}
