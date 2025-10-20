use anyhow::anyhow;
use axum::extract::ws::Utf8Bytes;
use tracing::{error, debug, info};
use futures::{StreamExt, SinkExt};
use tokio::{
    sync::broadcast,
    net::TcpListener,
    task, time::{sleep, Duration}
};
use tauri::{Emitter, AppHandle, State};
use crate::{app, monitors, app::AppState,
    monitors::MonitorInfo, /* overlay */
};
use std::{
    thread,
    sync::{
        Mutex,
        mpsc::{
            self,
        },
    }
};
use axum::{
    Router,
    routing,
    response::IntoResponse,
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
};

#[derive(Clone)]
pub struct MonitorBroadcaster {
    pub sender: broadcast::Sender<Vec<MonitorInfo>>,

}

async fn ws_monitors_handler(
    ws: WebSocketUpgrade,
    broadcaster: axum::extract::State<MonitorBroadcaster>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| {
        handle_monitor_socket(
            socket,
            broadcaster.0.clone(),
        )
    })
}

/// 2 sec sleep for brightness updates
async fn brightness_changes(state: AppState, broadcaster: MonitorBroadcaster) {
    let mut last_infos = Vec::new();

    loop {
        let mut current_infos = Vec::new();
        let devices = state.monitor_device.lock().await;

        for dev in devices.iter() {
            if let Ok(info) = dev.info() {
                current_infos.push(info);
            }
        }
        drop(devices);

        if current_infos != last_infos {
            debug!("brightness changed detected, {:?}", current_infos);
            let _ = broadcaster.sender.send(current_infos.clone());
            last_infos = current_infos;
        }

        sleep(Duration::from_secs(2)).await;
    }
}

/// 10 sec sleep for brightness updates
async fn device_changes(state: AppState, broadcaster: MonitorBroadcaster) {
    loop {
        sleep(Duration::from_secs(10)).await;

        let new_devices = match monitors::get_monitors() {
            Ok(list) => list,
            Err(e) => {
                error!("device scan failed: {e}");
                continue;
            }
        };

        let mut devices_lock = state.monitor_device.lock().await;

        // compare device lists by IDs
        let changed = new_devices.len() != devices_lock.len()
            || !devices_lock.iter().all(|d| 
                new_devices.iter().any(|nd| nd.id == d.id)
            );

        if changed {
            *devices_lock = new_devices.clone();
            // map devices → MonitorInfo for frontend broadcast
            let infos: Vec<_> = new_devices
                .iter()
                .filter_map(|d| d.info().ok())
                .collect();

            debug!("monitor device configuration changed: {:?}", infos);
            let _ = broadcaster.sender.send(infos);
        }

        drop(devices_lock);
    }
}


/// Handle each connected websocket client
async fn handle_monitor_socket(
    mut socket: WebSocket,
    broadcaster: MonitorBroadcaster,
) {
    let mut rx = broadcaster.sender.subscribe();

    // send initial monitor list
    if let Ok(monitors) = monitors::get_monitors() {
        let infos: Vec<MonitorInfo> = monitors.iter()
            .filter_map(|d| d.info().ok())
            .collect();
        let _ = socket.send(Message::Text(Utf8Bytes::from(
            serde_json::to_string(&infos).unwrap()))
        ).await;
    }

    // forward all broadcast updates to this websocket client
    while let Ok(monitors) = rx.recv().await {
        let json = serde_json::to_string(&monitors).unwrap();
        let _ = socket.send(Message::Text(Utf8Bytes::from(json))).await;
    }
}


/// A simple websocket for monitors based updates
pub async fn start_ws_server(state: AppState) -> anyhow::Result<()> {
    let (tx, _rx) = broadcast::channel(16);
    let broadcaster = MonitorBroadcaster { sender: tx.clone() };

    // start both watchers
    tokio::spawn(device_changes(state.clone(), broadcaster.clone()));
    tokio::spawn(brightness_changes(state.clone(), broadcaster.clone()));

    let app = Router::new()
        .route("/ws/monitors", routing::get(ws_monitors_handler))
        .with_state(broadcaster.clone());

    // keep it hardcoded :p
    let listener = TcpListener::bind("127.0.0.1:8956").await?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("WebSocket server failed: {}", e);
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn set_brightness(
    value: i32,
    device_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let devices = state.monitor_device.lock().await;
    let overlay_tx = state.overlay_tx.lock().await;

    let tx = match overlay_tx.as_ref() {
        Some(tx) => tx,
        None => return Err("overlay channel not initialized".to_string()),
    };

    if let Some(dev) = devices.iter().find(|d| d.device_name == device_name) {
        let _ = dev.slider(value, tx).await.map_err(|e| error!("slider crashed: {:?}", e.to_string()));
    } else {
        return Err(format!("device not found: {}", device_name));
    }

    Ok(())
}
