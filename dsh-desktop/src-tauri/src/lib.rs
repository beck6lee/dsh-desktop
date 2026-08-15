mod server;

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent, WindowEvent};

pub struct AppState {
    pub server: Arc<server::ServerManager>,
}

#[tauri::command]
fn server_status(state: tauri::State<'_, AppState>) -> server::StatusSnapshot {
    state.server.status()
}

#[tauri::command]
fn restart_server(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mgr = state.server.clone();
    std::thread::spawn(move || mgr.restart());
    Ok("restarting".to_string())
}

/// 显示并聚焦主窗口（Dock 点击 / 托盘菜单共用）。
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            server: Arc::new(server::ServerManager::new()),
        })
        .invoke_handler(tauri::generate_handler![server_status, restart_server])
        .setup(|app| {
            let mgr = app.state::<AppState>().server.clone();
            std::thread::spawn(move || mgr.start());

            // 窗口关闭改为隐藏到托盘（不再退出）
            let main_win = app
                .get_webview_window("main")
                .expect("main window should exist");
            let close_win = main_win.clone();
            main_win.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = close_win.hide();
                }
            });

            // 托盘菜单：dsh 版本显示（禁用）、检查更新、显示/隐藏窗口、退出
            let toggle = MenuItem::with_id(app, "toggle", "显示/隐藏窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 DeepSeek Harness", true, None::<&str>)?;
            let version_item = MenuItem::with_id(app, "dshver", "dsh 版本：检查中…", false, None::<&str>)?;
            let update_item = MenuItem::with_id(app, "update", "检查更新", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&version_item, &update_item, &toggle, &quit])?;
            // 启动自检用的版本行克隆：on_menu_event 的 move 闭包将持有 version_item 本体，
            // 因此克隆必须先于 tray builder 构造。
            let startup_item = version_item.clone();
            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "toggle" => {
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    }
                    "update" => {
                        let app_handle = app.clone();
                        let item = version_item.clone();
                        let _ = item.set_text("dsh 版本：检查中…");
                        std::thread::spawn(move || {
                            let info = server::check_update();
                            let text = server::format_update_text(&info);
                            let _ = app_handle.run_on_main_thread(move || {
                                let _ = item.set_text(text);
                            });
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            // build() 内部将托盘强引用存入 app resources table，句柄无需持有；
            // 直接丢弃返回值不会移除系统托盘（tauri app.rs resources_table）。
            tray_builder.build(app)?;

            // 启动时自动检查一次 dsh 版本（后台线程，结果经主线程更新菜单文本）
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let info = server::check_update();
                let text = server::format_update_text(&info);
                let _ = app_handle.run_on_main_thread(move || {
                    let _ = startup_item.set_text(text);
                });
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build tauri application")
        .run(|app_handle, event| {
            match event {
                // macOS Cmd+Q 只发 RunEvent::Exit（tao LoopDestroyed），不发 ExitRequested；
                // 必须同时处理两者，否则退出清理不执行（Task 9 验收发现的真实 bug）。
                // stop() 幂等，双触发无害。
                RunEvent::ExitRequested { .. } | RunEvent::Exit => {
                    app_handle.state::<AppState>().server.stop();
                }
                // Dock 图标点击（Reopen）：恢复窗口
                RunEvent::Reopen { .. } => {
                    show_main_window(app_handle);
                }
                _ => {}
            }
        });
}
