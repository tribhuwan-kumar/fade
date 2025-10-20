/*
 * Copyright 2025 @tribhuwan-kumar within the commons conservancy
 * SPDX-License-Identifier: AGPL-3.0
 * api for handling multiple monitors
*/
use anyhow::anyhow;
use serde::{
    Serialize,
    Deserialize
};
use tokio::sync::mpsc::Sender;
use std::{
    sync::Arc,
    fmt, ptr, iter,
    ffi::{OsString, OsStr},
    os::windows::ffi::{OsStringExt, OsStrExt},
};
use windows::{
    core::{BOOL, PCWSTR},
    Win32::{
        Foundation::{
            ERROR_SUCCESS, HANDLE, CloseHandle, ERROR_ACCESS_DENIED,  LPARAM, RECT,
        },
        Graphics::Gdi::{
            DISPLAY_DEVICE_ACTIVE, DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors,
            GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
        },
        Devices::Display::{
            QueryDisplayConfig, DestroyPhysicalMonitor,
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, 
            GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
            DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME,
            QDC_ONLY_ACTIVE_PATHS, DISPLAYCONFIG_MODE_INFO, PHYSICAL_MONITOR,
            DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS, DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
            DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
        },
        UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME,
        Storage::FileSystem::{
            CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        },
    }
};
use crate::{brightness, overlay::Overlay};

#[inline]
fn flag_set<T: std::ops::BitAnd<Output = T> + std::cmp::PartialEq + Copy>(t: T, flag: T) -> bool {
    t & flag == flag
}

/// for dropping `CloseHandle`
#[derive(PartialEq, Eq)]
pub struct SafeDisplayHandle(pub HANDLE);

// why does rust doesn't have implemention the
// same trait on multiple struct, without using macro?
impl fmt::Debug for SafeDisplayHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Drop for SafeDisplayHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_invalid() {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// for dropping `DestroyPhysicalMonitor`
#[derive(PartialEq, Eq, Clone)]
pub struct SafePhysicalMonitor(pub HANDLE);

impl fmt::Debug for SafePhysicalMonitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Drop for SafePhysicalMonitor {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_invalid() {
                let _ = DestroyPhysicalMonitor(self.0);
            }
        }
    }
}

/// send + sync
unsafe impl Send for SafePhysicalMonitor {}
unsafe impl Sync for SafePhysicalMonitor {}


#[derive(Debug, PartialEq, Eq)]
pub struct MonitorDeviceImpl {
    /// `monitorDevicePath` as unique identifier
    pub id: String,
    /// win32 `DeviceName`
    pub device_name: String,
    /// actual monitors name (as shown in settings)
    pub friendly_name: String,
    /// Internal Display Handler
    pub display_handle: Arc<SafeDisplayHandle>,
    /// Monitor handler
    pub physical_monitor: Arc<SafePhysicalMonitor>,
    /// output display technology for determining internal display
    pub output_technology: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
}

/// send + sync
unsafe impl Send for MonitorDeviceImpl {}
unsafe impl Sync for MonitorDeviceImpl {}


/// custom clone impl for `avoiding invalid handler error`
impl Clone for MonitorDeviceImpl {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            device_name: self.device_name.clone(),
            friendly_name: self.friendly_name.clone(),
            display_handle: Arc::clone(&self.display_handle),
            physical_monitor: Arc::clone(&self.physical_monitor),
            output_technology: self.output_technology,
        }
    }
}

/// especially for passing to the frontend
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MonitorInfo {
    /// win32 `DeviceName`
    pub device_name: String,           
    /// actual monitors name (as shown in settings)
    pub name: String,         
    // current brightness percentage
    pub brightness: u32,
}

// send + sync
unsafe impl Sync for MonitorInfo {}
unsafe impl Send for MonitorInfo {}


fn wchar_to_string(s: &[u16]) -> String {
    let end = s.iter().position(|&x| x == 0).unwrap_or(s.len());
    let truncated = &s[0..end];
    OsString::from_wide(truncated).to_string_lossy().into()
}

/// gets the handler by consuming the `monitorDevicePath` from `DISPLAYCONFIG_TARGET_DEVICE_NAME`
/// passing the `monitorDevicePath` as string cause to relate with frontend in easier way
fn get_handler_from_device_path(
    device_path: &str,
) -> anyhow::Result<Option<SafeDisplayHandle>> {
    unsafe {
        let wide: Vec<u16> = OsStr::new(device_path)
            .encode_wide()
            .chain(iter::once(0))
            .collect();

        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        );

        match handle {
            Ok(h) if !h.is_invalid() => Ok(Some(SafeDisplayHandle(h))),
            Ok(_) => Ok(None), // invalid
            Err(e) => {
                if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
                    Ok(None) // not a real monitor [eg. rdp session]
                } else {
                    Err(anyhow!(
                        "failed to open monitor handle (CreateFileW) for device: {}, err={:?}",
                        device_path,
                        e
                    ))
                }
            }
        }
    }
}

// returns list of `PHYSICAL_MONITOR` handle, 
// it'd also return a valid handle for [ddc/ci, non ddc/ci, rdp, ...] monitors.
fn get_physical_monitors_from_hmonitor(
    hmonitor: HMONITOR,
) -> anyhow::Result<Vec<SafePhysicalMonitor>> {
    let mut physical_number: u32 = 0;
    unsafe {
        GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut physical_number) 
        .map_err(|e| 
            anyhow!("the length of GetPhysicalMonitorsFromHMONITOR() and EnumDisplayDevicesW() results did not \n
                    match, this could be because monitors were connected/disconnected while loading devices, err: {:#?}", e
            ))?;

        let mut raw_physical_monitors = vec![PHYSICAL_MONITOR::default(); physical_number as usize];
        // allocate first so that pushing the wrapped handles always succeeds.
        let mut physical_monitors = Vec::with_capacity(raw_physical_monitors.len());

        GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut raw_physical_monitors)
            .map_err(|e| 
                anyhow!("the length of GetPhysicalMonitorsFromHMONITOR() and EnumDisplayDevicesW() results did not \n
                        match, this could be because monitors were connected/disconnected while loading devices, err: {:#?}", e 
                ))?;
        // transform immediately into WrappedPhysicalMonitor so the handles don't leak
        raw_physical_monitors
            .into_iter()
            .for_each(|pm| physical_monitors.push(SafePhysicalMonitor(pm.hPhysicalMonitor)));
        Ok(physical_monitors)
    }
}

/// returns list of display devices that belong to a `HMONITOR`
/// connected but inactive displays will filtered out
fn get_display_devices_from_hmonitor(
    hmonitor: HMONITOR,
) -> anyhow::Result<Vec<DISPLAY_DEVICEW>> {
    unsafe {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

        let info_ptr = &mut info as *mut _ as *mut MONITORINFO;
            GetMonitorInfoW(hmonitor, info_ptr)
            .ok()
            .map_err(|e| anyhow!("failed to get monitor info: {:#?}", e))?;

        Ok((0..)
            .map_while(|device_number| {
                let mut device = DISPLAY_DEVICEW {
                    cb: size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                EnumDisplayDevicesW(
                    PCWSTR(info.szDevice.as_ptr()),
                    device_number,
                    &mut device,
                    EDD_GET_DEVICE_INTERFACE_NAME,
                )
                .as_bool()
                .then_some(device)
            })
            .filter(|device| flag_set(device.StateFlags, DISPLAY_DEVICE_ACTIVE))
            .collect())
    }
}

/// returns a list of `HMONITOR` handles,
/// it's a logical construct that might correspond to multiple physical monitors
/// e.g. when in "Duplicate" mode two physical monitors will belong to the same `HMONITOR`
pub fn enum_display_monitors() -> anyhow::Result<Vec<HMONITOR>> {
    unsafe{
        extern "system" fn enum_monitors(
            handle: HMONITOR,
            _: HDC,
            _: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let monitors = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
            monitors.push(handle);
            true.into()
        }

        let mut hmonitors = Vec::<HMONITOR>::new();

        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitors),
            LPARAM(&mut hmonitors as *mut _ as isize),
        )
        .ok()
        .map_err(|e| anyhow!("failed to enumerate device monitors, err: {:#?}", e))?;

        Ok(hmonitors)
    }
}

impl MonitorDeviceImpl {
    pub fn new(
        id: String,
        device_name: String,
        friendly_name: String,
        display_handle: Arc<SafeDisplayHandle>,
        physical_monitor: Arc<SafePhysicalMonitor>,
        output_technology: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
    ) -> Self {
        Self {
            id,
            device_name,
            friendly_name,
            display_handle,
            physical_monitor,
            output_technology,
        }
    }

    pub fn info(&self) -> anyhow::Result<MonitorInfo> {
        Ok(
            MonitorInfo {
                device_name: self.device_name.clone(),
                name: self.friendly_name.clone(),
                brightness: self.get()?,
            }
        )
    }

    /// check if its an internal display
    pub fn is_internal(&self) -> bool {
        match self.output_technology {
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL |
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS |
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED => {
                true
            }
            _ => false,
        }
    }

    /// returns the corresponding monitor's brightness value
    pub fn get(&self) -> anyhow::Result<u32> {
        Ok(if self.is_internal() {
            brightness::ioctl_query_display_brightness(self)?
        } else {
            brightness::ddcci_get_monitor_brightness(self)?.get_current_percentage()
        })
    }

    /// set brightness percentage
    pub fn set(&self, percentage: u32) -> anyhow::Result<()> {
        if self.is_internal() {
            let supported = brightness::ioctl_query_supported_brightness(self)?;
            let new_value = supported.get_nearest(percentage);
            brightness::ioctl_set_display_brightness(self, new_value)?;
        } else {
            let current = brightness::ddcci_get_monitor_brightness(self)?;
            tracing::debug!("current ddcci monitor brightness: {:?}", current);
            let new_value = current.percentage_to_current(percentage);
            brightness::ddcci_set_monitor_brightness(self, new_value)?;
        }
        Ok(())
    }

    /// especially for the frontend
    pub async fn slider(
        &self, value: i32,
        overlay_tx: &Sender<Overlay>
    ) -> anyhow::Result<()> { // handle to manage [-100..100]
        if value >= 0 {
            self.set(value as u32)?
        } else {
            let alpha = ((-value) as f32 * 2.55) as u8;
            overlay_tx.send(Overlay {
                level: alpha,
                device_name: self.device_name.clone(),
            }).await?;
        }
        Ok(())
    }
}


/// it consumes `monitorDevicePath` for both ddc/ci and ioctl devices
pub fn get_monitors() -> anyhow::Result<Vec<MonitorDeviceImpl>> {
    unsafe {
        let mut path_count: u32 = 0;
        let mut mode_count: u32 = 0;

        // errors are in win error code, todo: format error
        let err = GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count);
        if err != ERROR_SUCCESS {
            return Err(anyhow!("`GetDisplayConfigBufferSizes` failed: {:?}", err));
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];

        let err = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        );

        if err != ERROR_SUCCESS {
            return Err(anyhow!("`QueryDisplayConfig` failed: {:?}", err));
        }

        let mut monitors = Vec::new();
        let mut device_name = String::new();

        for mode in &modes {
            if mode.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
                let mut target: DISPLAYCONFIG_TARGET_DEVICE_NAME = std::mem::zeroed();
                target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: mode.adapterId,
                    id: mode.id,
                };

                let err = DisplayConfigGetDeviceInfo(&mut target as *mut _ as *mut _);
                if err == ERROR_SUCCESS.0 as i32 {
                    let friendly = String::from_utf16_lossy(
                        &target.monitorFriendlyDeviceName
                            .iter()
                            .take_while(|&&c| c != 0)
                            .cloned()
                            .collect::<Vec<u16>>(),
                    );

                    // sometimes the name is blank when the display is internal or embebed!!
                    let name = if friendly.trim().is_empty() {
                        match target.outputTechnology {
                            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL |                  // default internal display
                            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS |                      // lvds connector display
                            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED => {   // embedded display port
                                "Internal Display".to_string()
                            }
                            _ => "Unknown Display".to_string(),
                        }
                    } else {
                        friendly
                    };

                    let device_path = String::from_utf16_lossy(
                        &target.monitorDevicePath
                            .iter()
                            .take_while(|&&c| c != 0)
                            .cloned()
                            .collect::<Vec<u16>>(),
                    );

                    // for internal ioctl displays
                    let internal_display = if matches!(
                        target.outputTechnology,
                        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS
                        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                    ) {
                        let mut adapter = DISPLAY_DEVICEW {
                            cb: size_of::<DISPLAY_DEVICEW>() as u32,
                            ..Default::default()
                        };
                        if EnumDisplayDevicesW(PCWSTR::null(), 0, &mut adapter, 0).as_bool() {
                            device_name = wchar_to_string(&adapter.DeviceName);
                        }
                        get_handler_from_device_path(&device_path)?
                            .unwrap_or(SafeDisplayHandle(HANDLE(ptr::null_mut())))
                    } else {
                        SafeDisplayHandle(HANDLE(ptr::null_mut()))
                    };

                    // for external ddc/ci monitors
                    let physical_monitor = if internal_display.0.is_invalid() {
                        let mut found: Option<SafePhysicalMonitor> = None;
                        for hm in enum_display_monitors()? {
                            let devices = get_display_devices_from_hmonitor(hm)?;
                            let pms = get_physical_monitors_from_hmonitor(hm)?;
                            if devices.len() != pms.len() {
                                // there doesn't seem to be any way to directly associate a physical monitor
                                // handle with the equivalent display device, other than by array indexing
                                // https://stackoverflow.com/questions/63095216/how-to-associate-physical-monitor-with-monitor-deviceid
                                return Err(
                                    anyhow!(
                                    "the length of `get_display_devices_from_hmonitor()` and `get_physical_monitors_from_hmonitor()` results did not \n
                                    match, this could be because monitors were connected/disconnected while loading devices"
                                ));
                            }
                            for (dev, pm) in devices.into_iter().zip(pms.into_iter()) {
                                let path = wchar_to_string(&dev.DeviceID);
                                if path == device_path {
                                    device_name = wchar_to_string(&dev.DeviceName);
                                    found = Some(pm);
                                    break;
                                }
                            }
                            if found.is_some() {
                                break;
                            }
                        }
                        found.unwrap_or(SafePhysicalMonitor(HANDLE(ptr::null_mut())))
                    } else {
                        SafePhysicalMonitor(HANDLE(ptr::null_mut()))
                    };

                    monitors.push(MonitorDeviceImpl::new(
                        device_path.clone(),
                        device_name.clone(),
                        name.clone(),
                        Arc::new(internal_display),
                        Arc::new(physical_monitor),
                        target.outputTechnology,
                    ));
                }
            }
        }

        Ok(monitors)
    }
}
