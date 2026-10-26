//! CareerTwin Prep: a window for practising English, technical and general interviews out loud.

use std::process::ExitCode;
use std::sync::Arc;

use careertwin_prep::config::{Paths, Settings};
use careertwin_prep::ui::{self, PrepApp};
use careertwin_prep::{doctor, services};

const HELP: &str = "CareerTwin Prep: practise interviews out loud

Usage: careertwin-prep [--doctor [--no-devices] [--no-probes]] [--version]

  --doctor       check the model, recognition, voice, microphone and speakers, then exit
  --no-devices   with --doctor, leave the microphone and speakers alone
  --no-probes    with --doctor, do not ask the recogniser and the voice for a sample
  --version      print the version

Settings: CAREERTWIN_PREP_HOME moves every folder this program uses under one place.";

struct Shell {
    app: PrepApp,
    /// Ends the window after this many frames; for checking that it starts.
    smoke: Option<u32>,
    frames: u32,
    _runtime: tokio::runtime::Runtime,
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
        self.frames += 1;
        if self.smoke.is_some_and(|limit| self.frames >= limit) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version") {
        println!("careertwin-prep {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let paths = match Paths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error:#}");
            return ExitCode::FAILURE;
        }
    };
    let settings = Settings::load(&paths);
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("could not start: {error}");
            return ExitCode::FAILURE;
        }
    };
    if args.iter().any(|a| a == "--doctor") {
        let options = doctor::Options {
            devices: !args.iter().any(|a| a == "--no-devices"),
            probes: !args.iter().any(|a| a == "--no-probes"),
        };
        let checks = runtime.block_on(doctor::run(&settings, &paths, &options));
        for check in &checks {
            println!("{}", check.line());
        }
        return if checks.iter().all(|c| c.ok) { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    let smoke = args.iter().position(|a| a == "--smoke").and_then(|i| args.get(i + 1)).and_then(|n| n.parse().ok());
    let (services, problems) = services::build(&settings, &paths);
    let handle = runtime.handle().clone();
    let result = eframe::run_native(
        "CareerTwin Prep",
        ui::window_options(),
        Box::new(move |cc| {
            ui::theme::install(&cc.egui_ctx);
            let context = cc.egui_ctx.clone();
            let app =
                PrepApp::new(paths, settings, services, problems, handle, Arc::new(move || context.request_repaint()));
            Ok(Box::new(Shell { app, smoke, frames: 0, _runtime: runtime }))
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
