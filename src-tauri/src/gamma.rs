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

pub fn set_gamma(
    level: i32,             // -100 dimmer, 0 normal
    device_name: &str,
) -> anyhow::Result<()> {
    let v = level.clamp(-100, 0);
    // normalize: -100 → 0.0, 0 → 1.0
    let t = (v + 100) as f32 / 100.0;

    let wide: Vec<u16> = OsStr::new(device_name)
        .encode_wide()
        .chain(iter::once(0))
        .collect();

    let min_gamma: f32 = 0.25; // very dim but not black
    let gamma = min_gamma.powf(1.0 - t);

    unsafe {
        // create a dc for corresponding monitor
        let hdc = CreateDCW(PCWSTR(wide.as_ptr()), PCWSTR(wide.as_ptr()), PCWSTR::null(), None);
        if hdc.0 == ptr::null_mut() {
            anyhow::bail!("failed to open dc for {:?}", device_name);
        }

        let mut ramp: [[u16; 256]; 3] = [[0; 256]; 3];

        for i in 0..256 {
            let value = ((i as f32 / 255.0).powf(1.0 / gamma) * 65535.0).min(65535.0);
            let v = value as u16;
            ramp[0][i] = v;
            ramp[1][i] = v;
            ramp[2][i] = v;
        }

        if SetDeviceGammaRamp(hdc, &ramp as *const _ as *const _) == BOOL(0) {
            let _ = DeleteDC(hdc);
            anyhow::bail!("failed to set gamma ramp");
        }

        let _ = DeleteDC(hdc);
    }

    Ok(())
}

pub fn reset_gamma(device_name: &str) -> anyhow::Result<()> {
    set_gamma(0, device_name)
}
