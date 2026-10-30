//! `next-release`: decide the next release from the commits since the last tag (see `careertwin_tools::release`).

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match careertwin_tools::release::run(&args, &mut std::io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("next-release: {message}");
            ExitCode::FAILURE
        }
    }
}
