use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{
            WIN32_ERROR, GetLastError, LocalFree, HLOCAL
        },
        System::Diagnostics::Debug::{
            FormatMessageW,
            FORMAT_MESSAGE_FROM_SYSTEM,
            FORMAT_MESSAGE_IGNORE_INSERTS,
            FORMAT_MESSAGE_ALLOCATE_BUFFER,
        },
    }
};
use tracing::error;
use tauri::{
    PhysicalPosition,
    WebviewWindow
};

// TODO: remove the window shadow
pub fn show_tray_window(window: &WebviewWindow, position: &PhysicalPosition<f64>) {
    // need monitor size for positioning the cursor!!
    if let Ok(Some(monitor)) = window.current_monitor() {
        let monitor_size = monitor.size();  
        let window_size = match window.outer_size() {
            Ok(size) => size,
            Err(e) => {
                error!("Failed to get window outer size: {}", e);
                return;
            }
        };

        let x_center: f64 = 2.0;
        let y_margin: f64 = 40.0;
        // center the window horizontally on the cursor's `x` position
        let pos_x = position.x - (window_size.width as f64 / x_center);
        // position the window directly under the cursor, with some margin `y`
        let pos_y = position.y - window_size.height as f64 + y_margin;
        
        let final_x = pos_x.max(0.0).min(monitor_size.width as f64 - window_size.width as f64);
        let final_y = pos_y.max(0.0).min(monitor_size.height as f64 - window_size.height as f64);
        
        let new_pos = tauri::PhysicalPosition::new(final_x, final_y);

        if let Err(e) = window.set_position(new_pos) {
            error!("failed to set window position: {}", e);
        }
    }

    // avoid unwrapping
    if let Err(e) = window.unminimize() { error!("failed to unminimize window: {}", e); }
    if let Err(e) = window.show() { error!("failed to show window: {}", e); }
    if let Err(e) = window.set_focus() { error!("failed to focus window: {}", e); }
}


/// returns string by formatting win32 error
pub fn format_win_err(err: WIN32_ERROR) -> String {
    let mut msg_buf = PWSTR::null();
    let len = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            None,
            err.0,
            0,                                              // dwlanguageid
            PWSTR(&mut msg_buf.0 as *mut _ as *mut u16),    // lpbuffer
            0,                                              // nsize
            None,                                           // args
        )
    };

    if len == 0 {
        let last_error = unsafe { GetLastError() };
        return format!(
            "unknown error code {} (FormatMessageW failed with code {})",
            err.0, last_error.0
        );
    }

    let msg = unsafe {
        let slice = std::slice::from_raw_parts(msg_buf.0, len as usize);
        String::from_utf16_lossy(slice).trim().to_string()
    };

    unsafe {
        LocalFree(Some(HLOCAL(msg_buf.0 as *mut _)));
    }

    msg
}

