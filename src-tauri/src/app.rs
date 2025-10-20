use tokio::sync::Mutex;
use tracing::{error, info};
use tauri_plugin_opener::OpenerExt;
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc::{Sender, channel};
use tauri::{
    Manager, WindowEvent, RunEvent,
    AppHandle, menu::{Menu, MenuItem}, 
    tray::{MouseButton, MouseButtonState,
        TrayIconBuilder, TrayIconEvent
    }
};
use tracing_appender::non_blocking::WorkerGuard;

use crate::{
    log, utils, events, overlay,
    overlay::Overlay,
    monitors::MonitorDeviceImpl
};

/// keep it non blocking
#[derive(Clone)]
pub struct AppState {
    pub log_guard: Arc<WorkerGuard>, 
    pub monitor_device: Arc<Mutex<Vec<MonitorDeviceImpl>>>,
    pub overlay_tx: Arc<Mutex<Option<Sender<Overlay>>>>,
}

/// global app handle
pub static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn app_handle<'a>() -> &'a AppHandle {
    APP_HANDLE.get().expect("app handle could not initialized")
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            events::set_brightness,
        ])
        .setup(|app| {
            APP_HANDLE.set(app.handle().clone())
                .map_err(|e| anyhow::anyhow!("failed to set global `AppHandle`: {:#?}", e))?;

            let log_guard = log::init_logging(app)?;
            let state = AppState {
                log_guard: Arc::new(log_guard),
                monitor_device: Arc::new(Mutex::new(Vec::new())),
                overlay_tx: Arc::new(Mutex::new(None)),
            };
            app.manage(state.clone());

            tauri::async_runtime::spawn({
                let state = state.clone();
                async move {
                    if let Err(e) = events::start_ws_server(state).await {
                        error!("WebSocket server failed: {:?}", e);
                    }
                }
            });

            tauri::async_runtime::spawn_blocking({
                let state = state.clone();
                move || {
                    tauri::async_runtime::block_on(async move {
                        let (tx, rx) = channel::<Overlay>(32);
                        *state.overlay_tx.lock().await = Some(tx.clone());
                        if let Err(e) = overlay::init_overlay(rx).await {
                            error!("overlay thread crashed: {:?}", e);
                        }
                    });
                }
            });

            let reset_i = MenuItem::with_id(app, "reset", "Reset", true, None::<&str>)?;
            let about_i = MenuItem::with_id(app, "about", "About", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&reset_i, &about_i, &quit_i])?;

            let _ = TrayIconBuilder::new()
                .menu(&menu)
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("fade & brightness")
                .on_tray_icon_event(|tray, event|  {
                    if let TrayIconEvent::Click {
                        position,
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let is_visible = window.is_visible().unwrap_or(false);
                            if is_visible {
                                if let Err(e) = window.hide() {
                                    error!("failed to hide window: {}", e);
                                }
                            } else {
                                utils::show_tray_window(&window, &position);
                            }
                        }
                    }
                })
                .show_menu_on_left_click(false)
                .build(app)?;

            info!("initializing fade & brightness");
            Ok(())
        })
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "reset" => {
                    info!("`Reset` menu item clicked");
                }
                "about" => {
                    info!("`About` menu item clicked");
                    if let Err(e) = app.opener().open_url("https://github.com/tribhuwan-kumar/fade", None::<&str>) {
                        error!("failed to open `About` url: {}", e);
                    }
                }
                "quit" => {
                    info!("`Quit` menu item clicked, exiting");
                    app.exit(0);
                }
                _ => {}
            }
        });

    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::WindowEvent {
                label,
                event: WindowEvent::Focused(false),
                ..
            } = event {
                if label == "main" {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        if let Err(e) = window.hide() {
                            error!("failed to hide window on focus lose: {}", e);
                        }
                    }
                }
            }
        });
}
