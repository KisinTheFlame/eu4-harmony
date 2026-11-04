//! The screen and a mouse and keyboard, for `perf-run`, which measures
//! vanilla too, where no framework runs to take screenshots or input:
//! regions of the screen read, compared and saved, and clicks and keys sent
//! to the window in front, as the player's hand would.
use crate::Result;
use std::path::Path;
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
        Graphics::Imaging::*,
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED},
    },
};

/// A region of the screen in physical pixels: x, y, width, height.
pub type Area = (i32, i32, i32, i32);

/// Takes the screen in physical pixels, whatever the display's scale, as
/// the game draws it.
pub fn physical_pixels() {
    use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// The region's pixels, blue, green, red and an unused byte each, row after
/// row from the top.
fn grab((x, y, width, height): Area) -> Result<Vec<u8>> {
    use windows_sys::Win32::Graphics::Gdi::*;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(memory, bitmap);
        let copied = BitBlt(memory, 0, 0, width, height, screen, x, y, SRCCOPY) != 0;
        SelectObject(memory, old);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        // Negative: the rows from the top down.
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let rows = GetDIBits(memory, bitmap, 0, height as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);
        if !copied || rows != height {
            return Err("cannot read the screen".into());
        }
    }
    Ok(pixels)
}

/// Every fourth pixel across and down, as red, green and blue bytes, from
/// pixels `bytes` bytes each, blue first.
fn sample(pixels: &[u8], width: i32, height: i32, bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(((width / 4) * (height / 4) * 3) as usize);
    for y in (0..).step_by(4).take_while(|y| y + 4 <= height) {
        for x in (0..).step_by(4).take_while(|x| x + 4 <= width) {
            let at = (y * width + x) as usize * bytes;
            out.extend([pixels[at + 2], pixels[at + 1], pixels[at]]);
        }
    }
    out
}

/// The region's sample, as `reference.bin` keeps it.
pub fn sample_screen(area: Area) -> Result<Vec<u8>> {
    Ok(sample(&grab(area)?, area.2, area.3, 4))
}

/// The mean difference per color value of two samples, 0 to 255.
pub fn difference(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 255.0;
    }
    a.iter().zip(b).map(|(x, y)| (*x as i64 - *y as i64).abs()).sum::<i64>() as f64 / a.len() as f64
}

fn factory() -> Result<IWICImagingFactory> {
    unsafe {
        // COM may be initialized on the thread already, in either model.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| format!("Windows' imaging: {e}"))
    }
}

/// Saves the region as a PNG.
pub fn save(area: Area, path: &Path) -> Result<()> {
    let pixels = grab(area)?;
    let (width, height) = (area.2 as u32, area.3 as u32);
    let saved = || -> windows::core::Result<()> {
        let factory = factory().map_err(|_| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
        unsafe {
            let bitmap = factory.CreateBitmapFromMemory(width, height, &GUID_WICPixelFormat32bppBGR, width * 4, &pixels)?;
            let rgb = factory.CreateFormatConverter()?;
            rgb.Initialize(&bitmap, &GUID_WICPixelFormat24bppBGR, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
            let stream = factory.CreateStream()?;
            stream.InitializeFromFilename(&HSTRING::from(path.as_os_str()), GENERIC_WRITE.0)?;
            let encoder = factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let (mut frame, mut options) = (None, None);
            encoder.CreateNewFrame(&mut frame, &mut options)?;
            let frame = frame.ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_POINTER))?;
            frame.Initialize(options.as_ref())?;
            frame.SetSize(width, height)?;
            let mut format = GUID_WICPixelFormat24bppBGR;
            frame.SetPixelFormat(&mut format)?;
            frame.WriteSource(&rgb, std::ptr::null())?;
            frame.Commit()?;
            encoder.Commit()
        }
    };
    saved().map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The sample of a PNG `save` wrote.
pub fn sample_file(path: &Path) -> Result<Vec<u8>> {
    let read = || -> windows::core::Result<Vec<u8>> {
        let factory = factory().map_err(|_| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
        unsafe {
            let decoder = factory.CreateDecoderFromFilename(&HSTRING::from(path.as_os_str()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)?;
            let frame = decoder.GetFrame(0)?;
            let (mut width, mut height) = (0, 0);
            frame.GetSize(&mut width, &mut height)?;
            let rgb = factory.CreateFormatConverter()?;
            rgb.Initialize(&frame, &GUID_WICPixelFormat24bppBGR, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
            let mut pixels = vec![0u8; (width * height * 3) as usize];
            rgb.CopyPixels(std::ptr::null(), width * 3, &mut pixels)?;
            Ok(sample(&pixels, width as i32, height as i32, 3))
        }
    };
    read().map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// A left click at a place of the screen, as a mouse gives it.
pub fn click(x: i32, y: i32) {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP},
        WindowsAndMessaging::SetCursorPos,
    };
    unsafe {
        SetCursorPos(x, y);
        pause(150);
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        pause(60);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
    pause(300);
}

/// The space bar pressed and let go, as a keyboard gives it.
pub fn space() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_SPACE};
    unsafe {
        keybd_event(VK_SPACE as u8, 0x39, 0, 0);
        pause(30);
        keybd_event(VK_SPACE as u8, 0x39, KEYEVENTF_KEYUP, 0);
    }
    pause(300);
}

pub fn pause(milliseconds: u64) {
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_takes_every_fourth_pixel_as_red_green_blue() {
        // 8 by 4 pixels, blue, green, red and a spare byte, each pixel's
        // blue its place in the row.
        let pixels: Vec<u8> = (0..4).flat_map(|y| (0..8).flat_map(move |x| [x as u8, y as u8, 9, 0])).collect();
        assert_eq!(sample(&pixels, 8, 4, 4), [9, 0, 0, 9, 0, 4]);
        assert_eq!(difference(&[0, 10], &[4, 0]), 7.0);
        assert_eq!(difference(&[0], &[0, 0]), 255.0);
    }
}
