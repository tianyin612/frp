//! 开机自启。
//!
//! 走 Tauri 官方 autostart 插件（macOS: LaunchAgent；Windows: HKCU 的 Run 键；
//! Linux: ~/.config/autostart/*.desktop）。
//!
//! 刻意不做任何"隐藏"手段：
//! - Windows 的 Run 键会出现在任务管理器"启动"页，用户可自行关闭；
//! - macOS 的 LaunchAgent 是 ~/Library/LaunchAgents 下的明文 plist。
//! 隐藏自启正是杀软判定后门的典型特征，这里反其道而行。

use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// 把设置同步到系统。失败只记日志，不阻断保存（用户可能没权限）。
pub fn apply<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);

    if enabled == current {
        return;
    }

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    match result {
        Ok(()) => eprintln!(
            "[autostart] 已{}开机自启",
            if enabled { "开启" } else { "关闭" }
        ),
        Err(e) => eprintln!(
            "[autostart] {}开机自启失败: {e}",
            if enabled { "开启" } else { "关闭" }
        ),
    }
}
