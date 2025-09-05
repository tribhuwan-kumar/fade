use tauri::{
    AppHandle,
    menu::{
        Menu,
        MenuItem
    }, 
    tray::{
        MouseButton,
        MouseButtonState,
        TrayIconBuilder,
        TrayIconEvent
    }, Manager, WindowEvent, RunEvent
};
use tracing::{error, info};
use tauri_plugin_opener::OpenerExt;
use std::sync::{Arc,  OnceLock};
use tokio::sync::Mutex;

use crate::{log, utils, events, gamma};

// global app handle
pub static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn app_handle<'a>() -> &'a AppHandle {
    APP_HANDLE.get().unwrap()
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            events::watch_monitors, events::set_brightness
        ])
        .manage(events::AppState {
            monitors: Arc::new(Mutex::new(Vec::new()))
        })
        .setup(|app| {

            let _global_app_handle = APP_HANDLE.set(app.handle().clone())
                .map_err(|e| anyhow::anyhow!("failed to set global `AppHandle`: {:#?}", e));

            let _guard = log::init_logging(app)?;

            let reset_i = MenuItem::with_id(app, "reset", "Reset", true, None::<&str>)?;
            let about_i = MenuItem::with_id(app, "about", "About", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&reset_i, &about_i, &quit_i])?;

            let _ = TrayIconBuilder::new()
                .menu(&menu)
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("fade Dimmer")
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

            info!("initializing dim brightness fade level");
            Ok(())
        })
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "reset" => {
                    info!("`Reset` menu item clicked");
                    // if let Err(e) = gamma::reset_gamma() {
                    //     error!("failed to reset gamma: {}", e);
                    // }
                }
                "about" => {
                    info!("'About' menu item clicked");
                    if let Err(e) = app.opener().open_url("https://github.com/tribhuwan-kumar/fade", None::<&str>) {
                        error!("failed to open `About` url: {}", e);
                    }
                }
                "quit" => {
                    info!("`Quit` menu item clicked, resetting gamma before exit.");
                    // if let Err(e) = gamma::reset_gamma() {
                    //     error!("failed to reset gamma before quitting: {}", e);
                    // }
                    app.exit(0);
                }
                _ => {}
            }
        });

    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let RunEvent::WindowEvent {
                label,
                event: WindowEvent::Focused(false),
                ..
            } = event {
                if label == "main" {
                    if let Some(window) = _app_handle.get_webview_window("main") {
                        if let Err(e) = window.hide() {
                            error!("failed to hide window on focus lose: {}", e);
                        }
                    }
                }
            }
        });
}
