use tauri::{Emitter, AppHandle, State};
use tracing::{error, debug, info};
use tokio::{task, time::{sleep, Duration}};
use crate::{app, monitors, app::AppState,
    monitors::MonitorInfo,
};

/// poll every 2 seconds for brightness changes
async fn brightness_changes(app: AppHandle, state: AppState) {
    let mut last_infos: Vec<MonitorInfo> = Vec::new();

    loop {
        let mut current_infos = Vec::new();
        let monitors_lock = state.monitor_device.lock().await;

        for device in monitors_lock.iter() {
            if let Ok(info) = device.info() {
                current_infos.push(info);
            }
        }
        drop(monitors_lock);

        if last_infos != current_infos {
            if !current_infos.is_empty() {
                 app.emit("monitors-changed", &current_infos).ok();
                 last_infos = current_infos;
            }
        }

        sleep(Duration::from_secs(2)).await;
    }
}

/// poll every 10 sec of new monitors device!
async fn device_changes(state: AppState) {
    loop {
        sleep(Duration::from_secs(10)).await;

        let new_devices = match monitors::get_monitors() {
            Ok(list) => list,
            Err(_) => continue,
        };

        let mut monitors = state.monitor_device.lock().await;

        if monitors.len() != new_devices.len() 
            || !monitors.iter().all(|d| new_devices.iter().any(|nd| nd.id == d.id)) {
             *monitors = new_devices;
        } else {
            for existing in monitors.iter_mut() {
                if let Some(new_dev) = new_devices.iter().find(|nd| nd.id == existing.id) {
                    existing.display_handle = new_dev.display_handle.clone();
                    existing.physical_monitor = new_dev.physical_monitor.clone();
                }
            }
        }
        drop(monitors)
    }
}

#[tauri::command]
pub async fn watch_monitors(
    state: State<'_, AppState>,
) -> Result<Vec<MonitorInfo>, String> {
    let app = app::app_handle();

    let mut initial_infos: Vec<MonitorInfo> = Vec::new();
    let initial_devices = monitors::get_monitors()
        .map_err(|e| e.to_string())?;

    info!("initial devices, {:?}", initial_devices);

    for device in &initial_devices {
        if let Ok(info) = device.info() {
            initial_infos.push(info);
        }
    }
    {
        let mut monitors_lock = state.monitor_device.lock().await;
        *monitors_lock = initial_devices;
    }

    tokio::spawn(brightness_changes(app.clone(), state.inner().clone()));
    tokio::spawn(device_changes(state.inner().clone()));

    Ok(initial_infos)
}

#[tauri::command]
pub async fn set_brightness(
    value: i32,
    device_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    debug!("invoked `set_brightness` for device id: {}", device_name);

    let device_clone = {
        let devices = state.monitor_device.lock().await;
        if let Some(device) = devices.iter().find(|d| d.device_name == device_name) {
            device.clone()
        } else {
            let err_msg = format!("failed to find device name: '{:?}'", device_name);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };

    task::spawn_blocking(move || {
        if let Err(e) = device_clone.slider(value) {
            error!(
                "failed to set brightness for device: {}, err: {:?}",
                device_clone.friendly_name, e
            );
        }
    });

    Ok(())
}
