use anyhow::{anyhow, bail};
use std::collections::HashMap;
use tracing::{warn, debug, info, error};
use tokio::{
    sync::mpsc::Receiver,
    time::{sleep, Duration}
};
use windows::{
    core::{w, BOOL},
    Win32::{
        Foundation::{
            HWND, LPARAM, LRESULT, POINT, RECT, WPARAM, COLORREF, HINSTANCE, GetLastError, ERROR_CLASS_ALREADY_EXISTS,
        },
        Graphics::Gdi::{
            HDC, HMONITOR, BeginPaint, EndPaint, EnumDisplayMonitors, FillRect, GetMonitorInfoW, GetStockObject, 
            MonitorFromPoint, BLACK_BRUSH, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, PAINTSTRUCT, HBRUSH, MONITORINFOEXW
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
            SetLayeredWindowAttributes, ShowWindow, TranslateMessage, LWA_ALPHA, MSG, SW_SHOW,
            WNDCLASSW, WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, PeekMessageW,
            RegisterClassExW, GetClassInfoExW, WM_QUIT, WS_POPUP, PM_REMOVE, WS_VISIBLE, PostQuitMessage,
            WS_EX_TRANSPARENT, WNDCLASSEXW, WM_PAINT, 
        },
        System::LibraryLoader::GetModuleHandleW
    }
};
use crate::{utils::format_win_err, monitors::{enum_display_monitors, get_monitors}};


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub level: u8,
    pub device_name: String,
}

/// message overlay thread will listen for.
/// it's an alpha value: 0 is transparent, 255 is fully opaque.
pub async fn init_overlay(mut rx: Receiver<Overlay>) -> anyhow::Result<()> {
    unsafe {
        let class_name = w!("FadeOverlay");
        let instance = GetModuleHandleW(None)?;

        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };

        // make sure to register the class
        if RegisterClassExW(&wc) == 0 {
            let last_error = GetLastError();
            if last_error != ERROR_CLASS_ALREADY_EXISTS {
                warn!("failed to register window class, err: {:?}", last_error);
            } else {
                warn!("class already exists, err: {:?}", last_error);
            }
        }

        // create an overlay window for each monitor
        // let mut windows: Vec<HWND> = Vec::new();
        let mut windows: HashMap<String, HWND> = HashMap::new();

        let monitor_handles = enum_display_monitors()?;
        debug!("Found {} monitors for UI overlay", monitor_handles.len());

        for monitor in monitor_handles {
            let mut info_ex = MONITORINFOEXW::default();
            info_ex.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

            // let mut info = MONITORINFO { 
            //     cbSize: size_of::<MONITORINFO>() as u32,
            //     ..Default::default()
            // };

            if GetMonitorInfoW(monitor, &mut info_ex.monitorInfo as *mut _ as *mut MONITORINFO).as_bool() {
                let device_name = String::from_utf16_lossy(&info_ex.szDevice)
                    .trim_end_matches('\0')
                    .to_string();
                let info = info_ex.monitorInfo;
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class_name,
                    w!(""),                             // keep window name empty
                    WS_POPUP,
                    info.rcMonitor.left,
                    info.rcMonitor.top,
                    info.rcMonitor.right - info.rcMonitor.left,
                    info.rcMonitor.bottom - info.rcMonitor.top,
                    None,
                    None,
                    Some(instance.into()),
                    None
                )?;

                windows.insert(device_name.clone(), hwnd);
                debug!("created dim overlay for device: {}", device_name);
            } else {
                let error = { GetLastError() };
                error!("`GetMonitorInfoW` failed for device win32 error: {:?}", format_win_err(error));
            }
        }

        debug!("overlay windows created: {:?}, {:?}", windows.keys(), windows);

        for &hwnd in windows.values() {
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA)?;
            ShowWindow(hwnd, SW_SHOW);
        }
        
        // for &hwnd in &windows {
        //     SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA)?;
        //     ShowWindow(hwnd, SW_SHOW);
        // }

        let mut msg = MSG::default();
        loop {
            if let Ok(overlay) = rx.try_recv() {
                // debug!("alpha value received: {:#?}", overlay);
                info!("alpha value received for device '{}': {}", &overlay.device_name, overlay.level);
                if let Some(&hwnd) = windows.get(&overlay.device_name) {
                    SetLayeredWindowAttributes(hwnd, COLORREF(0), overlay.level, LWA_ALPHA)?;
                } else {
                    warn!("Received overlay update for unknown device: {}", &overlay.device_name);
                }
                // for &hwnd in &windows {
                //     SetLayeredWindowAttributes(hwnd, COLORREF(0), overlay.level, LWA_ALPHA)?;
                // }
            }

            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            sleep(Duration::from_millis(16)).await;
        }
    }
}

/// window procedure for our overlay windows. it just paints itself black.
extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                FillRect(hdc, &ps.rcPaint, HBRUSH(GetStockObject(BLACK_BRUSH).0));
                let _end_paint = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            // fuck it, just drop the thread
            // WM_DESTROY => {
            //     PostQuitMessage(0);
            //     LRESULT(0)
            // }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

