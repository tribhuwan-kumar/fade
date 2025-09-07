use std::{
    ptr,
    iter,
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
};
use windows::{
    core::{BOOL, PCWSTR},
    Win32::{
        UI::ColorSystem::SetDeviceGammaRamp,
        Graphics::Gdi::{CreateDCW, DeleteDC},
    }
};

pub fn dim_brightness(
    level: i32,
    device_name: &str, 
) -> anyhow::Result<()> {
    let clamped_level = level.clamp(-100, 0);
    let multiplier = (clamped_level as f32 + 100.0) / 100.0;
    let wide: Vec<u16> = OsStr::new(device_name)
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    unsafe {
        let hdc = CreateDCW(PCWSTR(wide.as_ptr()), PCWSTR(wide.as_ptr()), PCWSTR::null(), None);
        if hdc.is_invalid() {
            anyhow::bail!("failed to open dc for {:?}", device_name);
        }

        let mut gamma_ramp: [u16; 3 * 256] = [0; 3 * 256];

        for i in 0..256usize {
            let value = (i as f32 * multiplier).round() as u16;
            let v = value * 257;
            gamma_ramp[i] = v;          // Red
            gamma_ramp[i + 256] = v;    // Green
            gamma_ramp[i + 512] = v;    // Blue
        }

        if SetDeviceGammaRamp(hdc, gamma_ramp.as_ptr() as *const _) == false {
            let _ = DeleteDC(hdc);
            anyhow::bail!("failed to set gamma ramp for {}", device_name);
        }

        let _ = DeleteDC(hdc);
    }
    Ok(())
}

pub fn reset_gamma(device_name: &str) -> anyhow::Result<()> {
    dim_brightness(0, device_name)
}
