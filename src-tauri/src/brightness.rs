//!
//! this code is heavily modified version of:
//! `https://github.com/stephaneyfx/brightness/blob/master/src/blocking/windows.rs`
//! its under 0BSD license & belongs to respective owners!!
//! `https://spdx.org/licenses/0BSD.html`
//! credit: @jacob-pro, @stephaneyfx
//!
use std::{
    ffi::c_void,
    mem::size_of,
};
use anyhow::anyhow;
use windows::{
    core::BOOL,
    Win32::{
        System::IO::DeviceIoControl,
        Devices::Display::{
            DISPLAY_BRIGHTNESS, DISPLAYPOLICY_AC, DISPLAYPOLICY_DC,
            GetMonitorBrightness, SetMonitorBrightness, IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS,
            IOCTL_VIDEO_QUERY_SUPPORTED_BRIGHTNESS, IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS,
        },
    },
};

use crate::monitors::MonitorDeviceImpl;


#[derive(Debug)]
pub struct IoctlSupportedBrightnessLevels(Vec<u8>);

#[derive(Debug, Default)]
pub struct DdcciBrightnessValues {
    pub min: u32,
    pub max: u32,
    pub current: u32,
}

impl IoctlSupportedBrightnessLevels {
    pub fn get_nearest(&self, percentage: u32) -> u8 {
        self.0
            .iter()
            .copied()
            .min_by_key(|&num| (num as i64 - percentage as i64).abs())
            .unwrap_or(0)
    }
}

impl DdcciBrightnessValues {
    pub fn get_current_percentage(&self) -> u32 {
        let normalised_max = (self.max - self.min) as f64;
        let normalised_current = (self.current - self.min) as f64;
        (normalised_current / normalised_max * 100.0).round() as u32
    }

    pub fn percentage_to_current(&self, percentage: u32) -> u32 {
        let normalised_max = (self.max - self.min) as f64;
        let fraction = percentage as f64 / 100.0;
        let normalised_current = fraction * normalised_max;
        normalised_current.round() as u32 + self.min
    }
}

/// returns the brightness percentage of ddc/ci display
pub fn ddcci_get_monitor_brightness(
    device: &MonitorDeviceImpl,
) -> anyhow::Result<DdcciBrightnessValues> {
    unsafe {
        let mut v = DdcciBrightnessValues::default();
        BOOL(GetMonitorBrightness(
            device.physical_monitor.0,
            &mut v.min,
            &mut v.current,
            &mut v.max,
        ))
        .ok()
        .map(|_| v)
        .map_err(|e| 
            anyhow!(
                "failed to get monitor brightness (ddcci), device: {:#?}, err {:#?}", 
                device.friendly_name.clone(), e
            ))
    }
}

/// set brightness to ddc/ci monitors
pub fn ddcci_set_monitor_brightness(
    device: &MonitorDeviceImpl,
    value: u32
) -> anyhow::Result<()> {
    unsafe {
        BOOL(SetMonitorBrightness(device.physical_monitor.0, value))
            .ok()
            .map_err(|e| 
            anyhow!(
                "failed to set monitor brightness (ddcci), device: {:#?}, err {:#?}", 
                device.friendly_name.clone(), e
            ))
    }
}

/// query ioctl brightness (internal display)
pub fn ioctl_query_supported_brightness(
    device: &MonitorDeviceImpl,
) -> anyhow::Result<IoctlSupportedBrightnessLevels> {
    unsafe {
        let mut bytes_returned = 0;
        let mut out_buffer = Vec::<u8>::with_capacity(256);
        DeviceIoControl(
            device.handle.0,
            IOCTL_VIDEO_QUERY_SUPPORTED_BRIGHTNESS,
            None,
            0,
            Some(out_buffer.as_mut_ptr() as *mut c_void),
            out_buffer.capacity() as u32,
            Some(&mut bytes_returned),
            None,
        )
        .map(|_| {
            out_buffer.set_len(bytes_returned as usize);
            IoctlSupportedBrightnessLevels(out_buffer)
        })
        .map_err(|e| 
            anyhow!(
                "failed to set query supported monitor brightness (ioctl), device: {:#?}, err {:#?}", 
                device.friendly_name.clone(), e
            ))
    }
}

/// returns the brightness percentage of ioctl display
pub fn ioctl_query_display_brightness(
    device: &MonitorDeviceImpl
) -> anyhow::Result<u32> {
    unsafe {
        let mut bytes_returned = 0;
        let mut display_brightness = DISPLAY_BRIGHTNESS::default();
        DeviceIoControl(
            device.handle.0,
            IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS,
            None,
            0,
            Some(&mut display_brightness as *mut DISPLAY_BRIGHTNESS as *mut c_void),
            size_of::<DISPLAY_BRIGHTNESS>() as u32,
            Some(&mut bytes_returned),
            None,
        )
        .map_err(|e|
                anyhow!(
                    "failed to query monitor brightness (ioctl), device: {:#?}, err {:#?}", 
                    device.friendly_name.clone(), e
                ))
        .and_then(|_| match display_brightness.ucDisplayPolicy as u32 {
            DISPLAYPOLICY_AC => {
                // this is a value between 0 and 100.
                Ok(display_brightness.ucACBrightness as u32)
            }
            DISPLAYPOLICY_DC => {
                // this is a value between 0 and 100.
                Ok(display_brightness.ucDCBrightness as u32)
            }
            _ => Err(anyhow!(
                "unexpected response when querying display brightness (ioctl), device: {:#?}",
                device.friendly_name.clone()
            )),
        })
    }
}

/// set brightness for ioctl display
pub fn ioctl_set_display_brightness(
    device: &MonitorDeviceImpl,
    value: u8
) -> anyhow::Result<()> {
    // bit 0: controls ac brightness
    // bit 1: controls dc brightness
    // bit 2: combines both ac & dc
    const DISPLAYPOLICY_BOTH: u8 = 3;
    unsafe {
        let mut display_brightness = DISPLAY_BRIGHTNESS {
            ucACBrightness: value,
            ucDCBrightness: value,
            ucDisplayPolicy: DISPLAYPOLICY_BOTH,
        };
        let mut bytes_returned = 0;
        DeviceIoControl(
            device.handle.0,
            IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS,
            Some(&mut display_brightness as *mut DISPLAY_BRIGHTNESS as *mut c_void),
            size_of::<DISPLAY_BRIGHTNESS>() as u32,
            None,
            0,
            Some(&mut bytes_returned),
            None,
        )
        .map(|_| {
            // there is a bug where if the IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS is
            // called immediately after then it won't show the newly updated values
            // doing a very tiny sleep seems to mitigate this
            std::thread::sleep(std::time::Duration::from_nanos(1));
        })
        .map_err(|e| 
            anyhow!(
                "failed to set monitor brightness (ioctl), device: {:#?}, err: {:#?}", 
                device.friendly_name.clone(), e
            ))
    }
}
