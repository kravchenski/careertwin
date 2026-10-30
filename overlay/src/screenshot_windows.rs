//! The screen through GDI: no helper program, and it works whatever has the focus.

use std::mem::size_of;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW, HGDIOBJ, MONITOR_DEFAULTTOPRIMARY,
    MONITORINFO, MonitorFromWindow, ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use super::{ScreenshotError, Shot, bgra_to_rgb, encode_jpeg};

fn fail(why: &str) -> ScreenshotError {
    ScreenshotError::Windows(why.to_owned())
}

/// What to capture: the screen that holds the window in front, or every screen.
fn area(all: bool) -> Result<RECT, ScreenshotError> {
    if all {
        // SAFETY: plain system metrics.
        let (left, top, width, height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        return Ok(RECT { left, top, right: left + width, bottom: top + height });
    }
    // SAFETY: the handle of the window in front is only read; the primary screen answers when there is none.
    let monitor = unsafe { MonitorFromWindow(GetForegroundWindow() as HWND, MONITOR_DEFAULTTOPRIMARY) };
    // SAFETY: `info` is ours and its size is set, as the call needs.
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return Err(fail("the screen could not be measured"));
    }
    Ok(info.rcMonitor)
}

pub fn capture(all: bool) -> Result<Shot, ScreenshotError> {
    let rect = area(all)?;
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        return Err(fail("the screen has no size"));
    }
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: every handle made here is released below, in the reverse order, whether or not the copy worked.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return Err(fail("the screen could not be opened"));
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let mut copied = false;
        if !memory.is_null() && !bitmap.is_null() {
            let previous = SelectObject(memory, bitmap as HGDIOBJ);
            // CAPTUREBLT includes windows that are layered, such as the one that shows a shared slide
            copied = BitBlt(memory, 0, 0, width, height, screen, rect.left, rect.top, SRCCOPY | CAPTUREBLT) != 0;
            if copied {
                let mut header: BITMAPINFO = std::mem::zeroed();
                header.bmiHeader = BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                };
                copied = GetDIBits(
                    memory,
                    bitmap,
                    0,
                    height as u32,
                    pixels.as_mut_ptr().cast(),
                    &mut header,
                    DIB_RGB_COLORS,
                ) != 0;
            }
            SelectObject(memory, previous);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap as HGDIOBJ);
        }
        if !memory.is_null() {
            DeleteDC(memory);
        }
        ReleaseDC(std::ptr::null_mut(), screen);
        if !copied {
            return Err(fail("the picture could not be copied (a protected window or a locked screen)"));
        }
    }
    encode_jpeg(bgra_to_rgb(&pixels), width as u32, height as u32).map_err(|why| fail(&why))
}
