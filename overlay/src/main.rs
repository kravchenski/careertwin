//! CareerTwin Interview, in Rust.

use std::process::ExitCode;
use std::sync::Arc;

use careertwin_overlay::control;
use careertwin_overlay::paths::Paths;
use careertwin_overlay::settings::load_all;
use careertwin_overlay::ui::{self, App, Event, theme};

const HELP: &str = "CareerTwin Interview (Rust): the window that writes what to say

Usage: careertwin-overlay [--version] [--data-dir <folder>]

  --version         print the version
  --data-dir        where data/interview/settings.json lives (default: CAREERTWIN_DATA_DIR, a `data` folder
                    here or above, or the platform's data folder)

The window answers typed questions and the screen. To answer the screen from another program, bind a key in
your compositor to `careertwin-interview-ctl screenshot`.";

struct Shell {
    app: App,
    _runtime: tokio::runtime::Runtime,
    _control: Option<control::Server>,
}

impl eframe::App for Shell {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("careertwin-overlay {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
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
            theme::install(&cc.egui_ctx, settings.font_scale as f32 / 100.0);
            let context = cc.egui_ctx.clone();
            let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(move || context.request_repaint());
            let app = App::new(paths, handle, repaint.clone());
            let sender = app.tx.clone();
            let wake = repaint.clone();
            let server = control::Server::start(move |command| {
                let _ = sender.send(Event::Remote(command.to_owned()));
                wake();
            })
            .map_err(|error| eprintln!("the screenshot key works only while this window has the focus: {error}"))
            .ok();
            Ok(Box::new(Shell { app, _runtime: runtime, _control: server }))
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
