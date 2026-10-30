//! CareerTwin Interview, in Rust.

// A release build on Windows is a window, not a console program.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;
use std::sync::Arc;

use careertwin_overlay::control;
use careertwin_overlay::paths::Paths;
use careertwin_overlay::settings::load_all;
use careertwin_overlay::ui::{self, App, Event, theme};

const HELP: &str = "CareerTwin Interview: the window that writes what to say

Usage: careertwin-overlay [--version] [--data-dir <folder>]
       careertwin-overlay ctl <screenshot|ask|ping>

  --version         print the version
  --data-dir        where data/interview/settings.json lives (default: CAREERTWIN_DATA_DIR, a `data` folder
                    here or above, or the platform's data folder)

The window hears the call, answers typed questions and answers the screen. On Windows the screenshot key works in
any program. On Linux bind a key in your compositor to `careertwin-interview-ctl screenshot` (that is
`careertwin-overlay ctl screenshot`); the window must be running.";

struct Shell {
    app: App,
    _runtime: tokio::runtime::Runtime,
    control: Option<control::Server>,
    #[cfg(windows)]
    _hotkey: Option<careertwin_overlay::hotkey::Hotkey>,
}

impl eframe::App for Shell {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.control = None; // takes the socket file away, so the next window can listen
        self.app.shutdown();
    }
}

/// A window program has no console of its own: `--help` and `--version` write to the one that started it.
#[cfg(windows)]
fn attach_console() {
    // SAFETY: attaches this process to the console of its parent, if it has one.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS)
    };
}

fn main() -> ExitCode {
    #[cfg(windows)]
    attach_console();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("careertwin-overlay {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.first().map(String::as_str) == Some("ctl") {
        return match args.get(1).map(|command| control::send(command)) {
            Some(Ok(reply)) => {
                println!("{reply}");
                ExitCode::SUCCESS
            }
            Some(Err(error)) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
            None => {
                eprintln!("Usage: careertwin-overlay ctl <{}>", control::COMMANDS.join("|"));
                ExitCode::FAILURE
            }
        };
    }
    let paths = match args.iter().position(|arg| arg == "--data-dir").and_then(|index| args.get(index + 1)) {
        Some(folder) => Paths::under(folder),
        None => Paths::discover(),
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("could not start: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (settings, app_settings) = load_all(&paths);
    let handle = runtime.handle().clone();
    let result = eframe::run_native(
        "CareerTwin · Interview",
        ui::window_options(&settings, &app_settings),
        Box::new(move |cc| {
            theme::set_palette(theme::from_settings(&app_settings));
            theme::install(&cc.egui_ctx, settings.font_scale as f32 / 100.0);
            let context = cc.egui_ctx.clone();
            let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(move || context.request_repaint());
            let mut app = App::new(paths, handle, repaint.clone());
            app.start_hearing();
            let sender = app.tx.clone();
            let wake = repaint.clone();
            let server = control::Server::start(move |command| {
                let _ = sender.send(Event::Remote(command.to_owned()));
                wake();
            })
            .map_err(|error| eprintln!("the screenshot key works only while this window has the focus: {error}"))
            .ok();
            #[cfg(windows)]
            let hotkey = {
                let (sender, wake, log) = (app.tx.clone(), repaint.clone(), app.paths.log_file());
                let note_log = log.clone();
                match careertwin_overlay::hotkey::start(
                    &app.app.screenshot_key,
                    move || {
                        let _ = sender.send(Event::Remote("screenshot".to_owned()));
                        wake();
                    },
                    move |text| careertwin_overlay::log::note(&note_log, text),
                ) {
                    Ok(hotkey) => {
                        app.screenshot_key = Some(hotkey.name.clone());
                        Some(hotkey)
                    }
                    Err(error) => {
                        careertwin_overlay::log::note(&log, &error);
                        app.say(&error, true);
                        None
                    }
                }
            };
            Ok(Box::new(Shell {
                app,
                _runtime: runtime,
                control: server,
                #[cfg(windows)]
                _hotkey: hotkey,
            }))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the window could not open: {error}");
            ExitCode::FAILURE
        }
    }
}
