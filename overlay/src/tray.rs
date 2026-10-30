//! The icon in the notification area (Windows), drawn like the arrow Windows shows while a program uses your location:
//! an outline when the window waits, solid while it listens or writes an answer. A click brings the window to the front;
//! a right click opens a menu with the screenshot, listening on or off, and quit. The icon follows the window's frames:
//! a window that is minimized is not drawn, so its icon waits until it is.
//!
//! The arrow is drawn here, from a polygon, so the icon needs no file and takes the colour it is given.

/// The corners of the navigation arrow, in a unit square: the tip, the left wing, the notch and the tail.
const ARROW: [(f32, f32); 4] = [(0.88, 0.12), (0.12, 0.50), (0.46, 0.54), (0.50, 0.88)];
/// The blue Windows uses for the location arrow.
pub const BLUE: [u8; 3] = [0x4c, 0xa0, 0xff];

/// What the icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// An outline: nothing is happening.
    Idle,
    /// Solid: the window is listening or writing an answer.
    Active,
}

/// What the menu asks of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Show,
    Screenshot,
    ToggleListening,
    Quit,
}

fn inside(point: (f32, f32)) -> bool {
    let mut crossings = false;
    let mut previous = ARROW[ARROW.len() - 1];
    for corner in ARROW {
        let ((x1, y1), (x2, y2)) = (previous, corner);
        if (y1 > point.1) != (y2 > point.1) && point.0 < (x2 - x1) * (point.1 - y1) / (y2 - y1) + x1 {
            crossings = !crossings;
        }
        previous = corner;
    }
    crossings
}

fn distance_to_edges(point: (f32, f32)) -> f32 {
    let mut best = f32::MAX;
    let mut previous = ARROW[ARROW.len() - 1];
    for corner in ARROW {
        let (dx, dy) = (corner.0 - previous.0, corner.1 - previous.1);
        let length = dx * dx + dy * dy;
        let t = (((point.0 - previous.0) * dx + (point.1 - previous.1) * dy) / length).clamp(0.0, 1.0);
        let (nx, ny) = (previous.0 + t * dx, previous.1 + t * dy);
        best = best.min(((point.0 - nx).powi(2) + (point.1 - ny).powi(2)).sqrt());
        previous = corner;
    }
    best
}

/// The arrow as `size` by `size` pixels of straight RGBA: an outline or solid, with smooth edges.
pub fn arrow_rgba(size: u32, color: [u8; 3], look: Look) -> Vec<u8> {
    const SAMPLES: u32 = 4;
    let half_stroke = 0.065;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut covered = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let point = (
                        (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32,
                        (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32,
                    );
                    let on = match look {
                        Look::Active => inside(point) || distance_to_edges(point) <= half_stroke,
                        Look::Idle => distance_to_edges(point) <= half_stroke,
                    };
                    covered += u32::from(on);
                }
            }
            let alpha = (covered * 255 / (SAMPLES * SAMPLES)) as u8;
            pixels.extend_from_slice(&[color[0], color[1], color[2], alpha]);
        }
    }
    pixels
}

/// The words of the menu, in the window's language.
#[derive(Clone, Debug, Default)]
pub struct Labels {
    pub show: String,
    pub screenshot: String,
    pub listening: String,
    pub quit: String,
}

#[cfg(windows)]
pub use native::{Tray, bring_to_front, reveal};

#[cfg(windows)]
mod native {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowTextW, IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    use super::{BLUE, Command, Labels, Look, arrow_rgba};

    /// The window of this program that has the interview on it.
    fn main_window() -> Option<HWND> {
        crate::capture_exclude::native::windows_of_this_process().into_iter().map(|window| window as HWND).find(
            |window| {
                let mut title = [0u16; 128];
                // SAFETY: a window of this process and a buffer of the length given.
                let length = unsafe { GetWindowTextW(*window, title.as_mut_ptr(), title.len() as i32) };
                length > 0 && String::from_utf16_lossy(&title[..length as usize]).starts_with("CareerTwin")
            },
        )
    }

    /// Bring the window to the front, restoring it when it was minimized.
    pub fn bring_to_front() {
        let Some(window) = main_window() else { return };
        // SAFETY: a window of this process.
        unsafe {
            ShowWindow(window, if IsIconic(window) != 0 { SW_RESTORE } else { SW_SHOW });
            SetForegroundWindow(window);
        }
    }

    /// Restore the window when it is minimized, without taking the focus: what needs a frame of the window (a
    /// screenshot, a quit) is done when the window is drawn, and a minimized window is not drawn.
    pub fn reveal() {
        let Some(window) = main_window() else { return };
        // SAFETY: a window of this process.
        unsafe {
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
        }
    }

    fn icon(look: Look) -> Result<Icon, String> {
        Icon::from_rgba(arrow_rgba(32, BLUE, look), 32, 32).map_err(|error| error.to_string())
    }

    /// The icon, kept alive as long as this is.
    pub struct Tray {
        tray: TrayIcon,
        listening: CheckMenuItem,
        look: Look,
        tooltip: String,
    }

    impl Tray {
        /// Create it on the thread that runs the window's events (Windows needs that). `on_command` is called from
        /// that thread when a menu item is chosen.
        pub fn new(
            labels: &Labels,
            tooltip: &str,
            on_command: impl Fn(Command) + Send + Sync + 'static,
        ) -> Result<Self, String> {
            let show = MenuItem::new(&labels.show, true, None);
            let screenshot = MenuItem::new(&labels.screenshot, true, None);
            let listening = CheckMenuItem::new(&labels.listening, true, true, None);
            let quit = MenuItem::new(&labels.quit, true, None);
            let menu = Menu::new();
            menu.append_items(&[&show, &screenshot, &listening, &PredefinedMenuItem::separator(), &quit])
                .map_err(|error| error.to_string())?;
            let show_id = show.id().clone();
            let (screenshot_id, listening_id, quit_id) =
                (screenshot.id().clone(), listening.id().clone(), quit.id().clone());
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let command = if event.id == show_id {
                    bring_to_front(); // nothing for the window to do: it is only brought forward
                    return;
                } else if event.id == screenshot_id {
                    Command::Screenshot
                } else if event.id == listening_id {
                    Command::ToggleListening
                } else if event.id == quit_id {
                    Command::Quit
                } else {
                    return;
                };
                on_command(command);
            }));
            TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } =
                    event
                {
                    bring_to_front();
                }
            }));
            let tray = TrayIconBuilder::new()
                .with_icon(icon(Look::Idle)?)
                .with_tooltip(tooltip)
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .map_err(|error| error.to_string())?;
            Ok(Self { tray, listening, look: Look::Idle, tooltip: tooltip.to_owned() })
        }

        /// Change what the icon shows; nothing is done when nothing changed.
        pub fn update(&mut self, look: Look, tooltip: &str, listening: bool) {
            if look != self.look
                && let Ok(icon) = icon(look)
            {
                let _ = self.tray.set_icon(Some(icon));
                self.look = look;
            }
            if tooltip != self.tooltip {
                let _ = self.tray.set_tooltip(Some(tooltip));
                self.tooltip = tooltip.to_owned();
            }
            if self.listening.is_checked() != listening {
                self.listening.set_checked(listening);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(pixels: &[u8], size: u32, x: f32, y: f32) -> u8 {
        let (px, py) = ((x * size as f32) as u32, (y * size as f32) as u32);
        pixels[((py * size + px) * 4 + 3) as usize]
    }

    #[test]
    fn the_arrow_is_a_picture_of_the_size_asked_in_the_colour_given() {
        let pixels = arrow_rgba(32, BLUE, Look::Active);
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert!(pixels.chunks(4).all(|pixel| pixel[..3] == BLUE));
        assert_eq!(alpha(&pixels, 32, 0.02, 0.98), 0, "the corner is empty");
    }

    #[test]
    fn the_idle_arrow_is_an_outline_and_the_active_one_is_solid() {
        let centre = (0.5, 0.4);
        assert!(inside(centre));
        let (idle, active) = (arrow_rgba(48, BLUE, Look::Idle), arrow_rgba(48, BLUE, Look::Active));
        assert!(alpha(&idle, 48, centre.0, centre.1) < 30, "an outline is empty inside");
        assert!(alpha(&active, 48, centre.0, centre.1) > 240, "a solid arrow is full inside");
        let covered = |pixels: &[u8]| pixels.chunks(4).map(|pixel| u32::from(pixel[3])).sum::<u32>();
        assert!(covered(&active) > covered(&idle));
        assert!(covered(&idle) > 0);
    }

    #[test]
    fn the_arrow_points_up_and_to_the_right_like_the_location_icon() {
        let pixels = arrow_rgba(48, BLUE, Look::Active);
        // the tip is in the upper right, the tail below, the wing on the left: the lower right and the upper left are empty
        assert!(alpha(&pixels, 48, 0.84, 0.16) > 100);
        assert_eq!(alpha(&pixels, 48, 0.9, 0.9), 0);
        assert_eq!(alpha(&pixels, 48, 0.1, 0.1), 0);
    }
}
