mod server;

use std::sync::Arc;
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

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            server: Arc::new(server::ServerManager::new()),
        })
        .invoke_handler(tauri::generate_handler![server_status, restart_server])
        .setup(|app| {
            let mgr = app.state::<AppState>().server.clone();
            std::thread::spawn(move || mgr.start());
            let handle = app.handle().clone();
            if let Some(win) = app.get_webview_window("main") {
                win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { .. } = event {
                        handle.exit(0);
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build tauri application")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                app_handle.state::<AppState>().server.stop();
            }
        });
}
