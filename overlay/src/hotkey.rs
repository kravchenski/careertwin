//! A key that takes the screenshot from any program, while the window is hidden behind a call.
//!
//! Windows only. A key bound with `RegisterHotKey` to a thread of its own is delivered whatever has the focus, so
//! it does not matter where the pointer is or which program is in front. Anywhere else the compositor binds the
//! key (`careertwin-interview-ctl screenshot`).
//!
//! Another program may already own the combination. The registration then falls back to the next one and says so,
//! in the log and in the window, with Windows' own error number.

/// `MOD_*` flags of `RegisterHotKey`.
pub const ALT: u32 = 0x1;
pub const CONTROL: u32 = 0x2;
pub const SHIFT: u32 = 0x4;
pub const NO_REPEAT: u32 = 0x4000;

/// A key and the modifiers held with it, as Windows names them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Combo {
    pub modifiers: u32,
    pub key: u32,
    pub name: String,
}

/// What is tried when nothing is chosen, best first.
pub const DEFAULT: &str = "Ctrl+Shift+Enter";
const FALLBACKS: [&str; 3] = ["Ctrl+Alt+Enter", "Ctrl+Shift+F9", "Ctrl+Alt+F9"];

/// The virtual-key code of a key name: a letter, a digit, F1-F24, or a named key.
fn key_code(name: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    if lower.chars().count() == 1 {
        let c = lower.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(u32::from(c.to_ascii_uppercase()));
        }
        if c.is_ascii_digit() {
            return Some(u32::from(c));
        }
    }
    if let Some(number) = lower.strip_prefix('f').and_then(|digits| digits.parse::<u32>().ok())
        && (1..=24).contains(&number)
    {
        return Some(0x6F + number); // VK_F1 is 0x70
    }
    match lower.as_str() {
        "enter" | "return" => Some(0x0D),
        "space" => Some(0x20),
        "tab" => Some(0x09),
        "esc" | "escape" => Some(0x1B),
        "backspace" => Some(0x08),
        "insert" => Some(0x2D),
        "delete" | "del" => Some(0x2E),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" => Some(0x21),
        "pagedown" => Some(0x22),
        _ => None,
    }
}

/// `Ctrl+Shift+Enter` and the like. At least one modifier is needed: a bare key would be taken from every other
/// program.
pub fn parse(text: &str) -> Option<Combo> {
    let parts: Vec<&str> = text.split('+').map(str::trim).filter(|part| !part.is_empty()).collect();
    let (key, held) = parts.split_last()?;
    let mut modifiers = 0;
    let mut names: Vec<&str> = Vec::new();
    for part in held {
        let (flag, name) = match part.to_lowercase().as_str() {
            "ctrl" | "control" => (CONTROL, "Ctrl"),
            "alt" => (ALT, "Alt"),
            "shift" => (SHIFT, "Shift"),
            _ => return None,
        };
        if modifiers & flag == 0 {
            modifiers |= flag;
            names.push(name);
        }
    }
    if modifiers == 0 {
        return None;
    }
    let code = key_code(key)?;
    let key_name = if key.chars().count() == 1 { key.to_uppercase() } else { capitalise(key) };
    names.push(&key_name);
    Some(Combo { modifiers, key: code, name: names.join("+") })
}

fn capitalise(word: &str) -> String {
    let lower = word.to_lowercase();
    if let Some(number) = lower.strip_prefix('f').filter(|rest| rest.chars().all(|c| c.is_ascii_digit())) {
        return format!("F{number}");
    }
    let mut chars = lower.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// The combinations to try, the chosen one first, then the fallbacks that are not the same.
pub fn candidates(preferred: &str) -> Vec<Combo> {
    let mut list: Vec<Combo> = Vec::new();
    for text in std::iter::once(preferred).chain(std::iter::once(DEFAULT)).chain(FALLBACKS) {
        if let Some(combo) = parse(text)
            && !list.iter().any(|known| known.modifiers == combo.modifiers && known.key == combo.key)
        {
            list.push(combo);
        }
    }
    list
}

#[cfg(windows)]
pub use native::{Hotkey, start};

#[cfg(not(windows))]
pub fn start(_: &str, _: impl Fn() + Send + 'static, _: impl Fn(&str) + Send + 'static) -> Result<(), String> {
    Err("On this system the compositor binds the key: run `careertwin-interview-ctl screenshot` from it.".to_owned())
}

#[cfg(windows)]
mod native {
    use std::sync::mpsc::channel;
    use std::thread::JoinHandle;

    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, PostThreadMessageW, WM_HOTKEY, WM_QUIT};

    use super::{Combo, NO_REPEAT, candidates};

    const ID: i32 = 0x4354; // "CT"
    const ALREADY_REGISTERED: u32 = 1409;

    /// The key, registered on a thread of its own until this is dropped.
    pub struct Hotkey {
        /// The combination that was free, as the user reads it.
        pub name: String,
        thread: u32,
        worker: Option<JoinHandle<()>>,
    }

    impl Drop for Hotkey {
        fn drop(&mut self) {
            // SAFETY: a thread id this program started; the message only asks its loop to end.
            unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn register(combo: &Combo) -> Result<(), u32> {
        // SAFETY: no window (the key belongs to this thread), an id of ours and documented flags.
        if unsafe { RegisterHotKey(std::ptr::null_mut(), ID, combo.modifiers | NO_REPEAT, combo.key) } != 0 {
            Ok(())
        } else {
            // SAFETY: reads this thread's last error, right after the call that set it.
            Err(unsafe { GetLastError() })
        }
    }

    /// Register `preferred` (or a fallback when another program has it) and call `on_press` for every press. `note`
    /// receives what happened, for the log.
    pub fn start(
        preferred: &str,
        on_press: impl Fn() + Send + 'static,
        note: impl Fn(&str) + Send + 'static,
    ) -> Result<Hotkey, String> {
        let list = candidates(preferred);
        let (ready, answer) = channel::<Result<(String, u32), String>>();
        let worker = std::thread::Builder::new()
            .name("hotkey".to_owned())
            .spawn(move || {
                let mut failures = Vec::new();
                for combo in &list {
                    match register(combo) {
                        Ok(()) => {
                            note(&format!("hotkey {} registered", combo.name));
                            // SAFETY: the id of this very thread.
                            let _ = ready.send(Ok((combo.name.clone(), unsafe { GetCurrentThreadId() })));
                            loop {
                                // SAFETY: `message` is ours; a null window reads this thread's queue.
                                let mut message: MSG = unsafe { std::mem::zeroed() };
                                let got = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
                                if got <= 0 {
                                    break;
                                }
                                if message.message == WM_HOTKEY && message.wParam == ID as usize {
                                    note(&format!("hotkey {} pressed", combo.name));
                                    on_press();
                                }
                            }
                            // SAFETY: unregisters what this thread registered.
                            unsafe { UnregisterHotKey(std::ptr::null_mut(), ID) };
                            note("hotkey released");
                            return;
                        }
                        Err(code) => {
                            let why = if code == ALREADY_REGISTERED { "taken by another program" } else { "refused" };
                            note(&format!("hotkey {} {why} (Windows error {code})", combo.name));
                            failures.push(format!("{} ({why}, error {code})", combo.name));
                        }
                    }
                }
                let _ = ready.send(Err(format!("No key was free for the screenshot: {}", failures.join("; "))));
            })
            .map_err(|error| format!("The key thread could not start: {error}"))?;
        match answer.recv() {
            Ok(Ok((name, thread))) => Ok(Hotkey { name, thread, worker: Some(worker) }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => Err("The key thread stopped before it could say".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_combination_is_read_whatever_the_case_and_spacing() {
        let combo = parse("ctrl + SHIFT + enter").unwrap();
        assert_eq!((combo.modifiers, combo.key, combo.name.as_str()), (CONTROL | SHIFT, 0x0D, "Ctrl+Shift+Enter"));
        assert_eq!(parse("Alt+f9").unwrap().key, 0x78);
        assert_eq!(parse("Ctrl+Alt+s").unwrap().name, "Ctrl+Alt+S");
        assert_eq!(parse("Ctrl+Ctrl+1").unwrap().modifiers, CONTROL);
    }

    #[test]
    fn a_key_without_a_modifier_or_with_an_unknown_name_is_refused() {
        for bad in ["Enter", "", "Ctrl+", "Ctrl+Banana", "Win+S", "Hyper+S", "F25", "Ctrl+Shift"] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_chosen_key_comes_first_and_the_fallbacks_are_not_repeated() {
        let names: Vec<String> = candidates("Ctrl+Alt+K").into_iter().map(|combo| combo.name).collect();
        assert_eq!(names, ["Ctrl+Alt+K", "Ctrl+Shift+Enter", "Ctrl+Alt+Enter", "Ctrl+Shift+F9", "Ctrl+Alt+F9"]);
        let default: Vec<String> = candidates(DEFAULT).into_iter().map(|combo| combo.name).collect();
        assert_eq!(default.len(), 4);
        assert_eq!(candidates("nonsense")[0].name, DEFAULT);
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_the_compositor_binds_the_key() {
        assert!(start(DEFAULT, || {}, |_| {}).unwrap_err().contains("careertwin-interview-ctl"));
    }
}
