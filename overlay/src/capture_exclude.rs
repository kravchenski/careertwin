//! Keep the window out of screen sharing and recordings while it stays on your own monitor (Windows).
//!
//! Windows 10 (version 2004 and later) and 11 can leave a window out of every capture surface they serve: Zoom,
//! Teams, Meet, OBS, screenshots. The window is still drawn on the physical screen. Older builds fall back to
//! painting the window black in captures. Windows may reset the flag on some window events, so it is set again
//! every couple of seconds. Other systems have no such thing; the window then does nothing.
//!
//! Many employers forbid AI help during interviews: check the rules of each one.

use std::time::Duration;

/// `WDA_EXCLUDEFROMCAPTURE` of `SetWindowDisplayAffinity`.
pub const EXCLUDE_FROM_CAPTURE: u32 = 0x11;
/// `WDA_NONE`: the window is captured like any other.
pub const NONE: u32 = 0;
/// How often the flag is set again.
pub const REASSERT_EVERY: Duration = Duration::from_secs(2);

/// Whether this system can leave a window out of captures.
pub const fn supported() -> bool {
    cfg!(windows)
}

/// Set the affinity of every top-level window of this process; `on` leaves them out of captures, `off` puts them
/// back. Returns how many windows accepted it.
pub fn set(on: bool) -> usize {
    #[cfg(windows)]
    {
        native::set_all(if on { EXCLUDE_FROM_CAPTURE } else { NONE })
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        0
    }
}

/// Apply `affinity` to each window with `apply`, counting the ones that took it. Split out so the counting is
/// tested where there is no Windows.
pub fn apply_all(windows: &[isize], affinity: u32, apply: impl Fn(isize, u32) -> bool) -> usize {
    windows.iter().filter(|window| apply(**window, affinity)).count()
}

#[cfg(windows)]
mod native {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, SetWindowDisplayAffinity};

    unsafe extern "system" fn collect(window: HWND, found: LPARAM) -> i32 {
        let mut process = 0u32;
        // SAFETY: `window` is a handle Windows just gave this callback, and `process` outlives the call.
        unsafe { GetWindowThreadProcessId(window, &mut process) };
        // SAFETY: `found` is the address of the Vec that `windows_of_this_process` passed to `EnumWindows`.
        if process == unsafe { GetCurrentProcessId() } {
            let list = unsafe { &mut *(found as *mut Vec<isize>) };
            list.push(window as isize);
        }
        1
    }

    fn windows_of_this_process() -> Vec<isize> {
        let mut found: Vec<isize> = Vec::new();
        // SAFETY: the callback only reads handles and pushes to `found`, which lives until `EnumWindows` returns.
        unsafe { EnumWindows(Some(collect), std::ptr::addr_of_mut!(found) as *mut c_void as LPARAM) };
        found
    }

    pub fn set_all(affinity: u32) -> usize {
        super::apply_all(&windows_of_this_process(), affinity, |window, affinity| {
            // SAFETY: a window handle of this process and a documented affinity value.
            unsafe { SetWindowDisplayAffinity(window as HWND, affinity) != 0 }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exclusion_is_the_documented_value_and_only_windows_claims_it() {
        assert_eq!(EXCLUDE_FROM_CAPTURE, 0x11);
        assert_eq!(supported(), cfg!(windows));
        if !supported() {
            assert_eq!(set(true), 0);
        }
    }

    #[test]
    fn the_windows_that_accept_the_flag_are_counted() {
        let seen = std::sync::Mutex::new(Vec::new());
        let accepted = apply_all(&[1, 2, 3], EXCLUDE_FROM_CAPTURE, |window, affinity| {
            seen.lock().unwrap().push((window, affinity));
            window != 2
        });
        assert_eq!(accepted, 2);
        assert_eq!(*seen.lock().unwrap(), [(1, 0x11), (2, 0x11), (3, 0x11)]);
        assert_eq!(apply_all(&[], NONE, |_, _| true), 0);
    }
}
