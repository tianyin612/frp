//! 系统托盘：显示主窗口 / 全部启停 / 退出。
//!
//! 设计取舍：
//! - 托盘只做"操作入口"，不复制业务逻辑，启动/停止都复用 commands 里的同一套函数，
//!   避免出现"托盘点了能跑、界面点了报错"的两套代码。
//! - 左键点图标 = 显示窗口；菜单用右键（`show_menu_on_left_click(false)`）。
//! - 运行数量体现在**窗口标题**上（`refresh_status`），不用托盘 tooltip ——
//!   `AppHandle::tray_by_id` 的参数类型跨小版本不一致，避开它更稳。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "frpc-tray";

/// 显示并聚焦主窗口（隐藏 -> 同一次点两次也不会卡住）
pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let start_all = MenuItem::with_id(app, "start_all", "启动全部配置", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, "stop_all", "停止全部配置", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 frpc-desktop", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &start_all, &stop_all, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("frpc-desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "start_all" => crate::commands::start_all(app),
            "stop_all" => crate::commands::stop_all_profiles(app),
            "quit" => crate::quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });

    // 没有图标资源时不要崩，只打印一条日志（图标由 `pnpm tauri icon` 生成）
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    } else {
        eprintln!("[tray] 没有默认窗口图标，托盘会显示为空白方块");
    }

    let tray = builder.build(app)?;
    // 不同 Tauri 小版本对"句柄 drop 后是否移除图标"的处理不一致，
    // 这里主动留一份，保证托盘常驻；进程退出时由系统统一回收。
    std::mem::forget(tray);
    refresh_status(app);
    Ok(())
}

/// 更新窗口标题，让任务栏 / Dock 上也能看到运行数量。
///
/// 注意：这里**没有**用 `AppHandle::tray_by_id` 去改托盘 tooltip，
/// 因为该方法在不同小版本里参数类型不一致（&str vs &TrayIconId），
/// 换成确定存在的 `set_title` 更稳。
pub fn refresh_status(app: &AppHandle) {
    let count = app
        .try_state::<crate::AppState>()
        .map(|state| state.sup.running().len())
        .unwrap_or(0);

    if let Some(win) = app.get_webview_window("main") {
        let title = if count == 0 {
            "frpc-desktop".to_string()
        } else {
            format!("frpc-desktop · {count} 个运行中")
        };
        let _ = win.set_title(&title);
    }
}
