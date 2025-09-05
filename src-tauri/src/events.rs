/// later switch to wmi events
/// currently the event handling is in polling, 
use tauri::{Emitter, State};
use tracing::{error, debug};
use std::{sync::{Arc}};
use tokio::sync::Mutex;
use tokio::{time::{sleep, Duration}};
use crate::{app, monitors,
    monitors::{MonitorDeviceImpl, MonitorInfo}
};

/// pass the devices non-bloncking
pub struct AppState {
    pub monitors: Arc<Mutex<Vec<MonitorDeviceImpl>>>,
}

#[tauri::command]
pub async fn watch_monitors(state: State<'_, AppState>) -> anyhow::Result<(), String> {
    let app = app::app_handle();
    loop {
        let new_devices = match monitors::get_monitors() {
            Ok(list) => list,
            Err(e) => {
                error!("failed to query monitors: {:?}", e);
                sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        let mut changed = false;
        let mut monitors = state.monitors.lock().await;

        // update or add
        for new_dev in &new_devices {
            if let Some(existing) = monitors.iter_mut().find(|d| d.id == new_dev.id) {
                // update metadata only (keep handle alive)
                if existing.friendly_name != new_dev.friendly_name
                    || existing.device_name != new_dev.device_name
                    || existing.output_technology != new_dev.output_technology
                {
                    existing.friendly_name = new_dev.friendly_name.clone();
                    existing.device_name = new_dev.device_name.clone();
                    existing.output_technology = new_dev.output_technology;
                    changed = true;
                }
                existing.handle = new_dev.handle.clone();
                existing.physical_monitor = new_dev.physical_monitor.clone();
            } else {
                // new monitor, push it in
                monitors.push(new_dev.clone());
                changed = true;
            }
        }

        // emit change if needed
        if changed {
            debug!("monitor info changed: {:?}", *monitors);

            let infos: Vec<MonitorInfo> = monitors
                .iter()
                .filter_map(|d| match d.info() {
                    Ok(info) => Some(info),
                    Err(e) => {
                        error!(
                            "failed to build `MonitorInfo` for device: {}, err: {:?}",
                            d.friendly_name, e
                        );
                        None
                    }
                })
                .collect();

            if let Err(e) = app.emit("monitors-changed", &infos) {
                error!("failed to emit monitors-changed: {:?}", e);
            }
        }

        sleep(Duration::from_secs(2)).await;
    }
}

#[tauri::command]
pub async fn set_brightness(
    id: String,
    value: i32,
    state: State<'_, AppState>,
) -> anyhow::Result<(), String> {
    debug!("invoked `set_brightness`");
    let devices = state.monitors.lock().await;
    if let Some(device) = devices.iter().find(|d| d.id == id) {
        let d = device.clone();
        // run non-blocking
        tokio::spawn(async move {
            if let Err(e) = d.slider(value) {
                error!(
                    "failed to set brightness for device: {}, err: {:?}",
                    d.friendly_name, e
                );
            }
        });
        Ok(())
    } else {
        Err(format!("monitor {} not found", id))
    }
}
