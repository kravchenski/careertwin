//! `check-links`: check every company's careers page and website in `companies.json`.
//!
//! ```text
//! check-links [--limit N] [--concurrency N] [--field careers|domain|both] [--out report.json] [--root DIR]
//! ```
//!
//! Exit code 1 when a broken link was found (useful for CI, or for an agent to branch on).

use std::path::PathBuf;
use std::process::ExitCode;

use careertwin_tools::links::{self, Fields};
use serde_json::Value;

const USAGE: &str = "usage: check-links [--limit N] [--concurrency N] [--field careers|domain|both] [--out report.json] [--root DIR]";
const COMPANIES: &str = "src/careertwin/jobs/data/companies.json";

struct Options {
    limit: Option<usize>,
    concurrency: usize,
    fields: Fields,
    out: Option<PathBuf>,
    root: PathBuf,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        limit: None,
        concurrency: 15,
        fields: Fields {
            careers: true,
            domain: true,
        },
        out: None,
        root: PathBuf::from("."),
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--limit" => {
                options.limit = Some(
                    value("--limit")?
                        .parse()
                        .map_err(|_| format!("--limit needs a number\n{USAGE}"))?,
                )
            }
            "--concurrency" => {
                options.concurrency = value("--concurrency")?
                    .parse()
                    .map_err(|_| format!("--concurrency needs a number\n{USAGE}"))?
            }
            "--field" => {
                options.fields = Fields::parse(&value("--field")?)
                    .ok_or_else(|| format!("--field is careers, domain or both\n{USAGE}"))?
            }
            "--out" => options.out = Some(value("--out")?.into()),
            "--root" => options.root = value("--root")?.into(),
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    Ok(options)
}

#[tokio::main]
async fn main() -> ExitCode {
    match go().await {
        Ok(found_broken) => ExitCode::from(u8::from(found_broken)),
        Err(message) => {
            eprintln!("check-links: {message}");
            ExitCode::from(2)
        }
    }
}

async fn go() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return Ok(false);
    }
    let options = options(&args)?;
    let file = options.root.join(COMPANIES);
    let content = std::fs::read_to_string(&file)
        .map_err(|error| format!("could not read {}: {error}", file.display()))?;
    let Value::Array(mut companies) =
        serde_json::from_str(&content).map_err(|error| format!("{}: {error}", file.display()))?
    else {
        return Err(format!("{} is not a list of companies", file.display()));
    };
    if let Some(limit) = options.limit.filter(|limit| *limit > 0) {
        companies.truncate(limit);
    }
    let results = links::run(
        &companies,
        options.fields,
        options.concurrency,
        links::TIMEOUT,
    )
    .await?;
    print!("{}", links::report(&results));
    if let Some(out) = &options.out {
        let json = Value::Array(results.iter().map(links::Entry::to_json).collect());
        let text = serde_json::to_string_pretty(&json).map_err(|error| error.to_string())?;
        std::fs::write(out, text)
            .map_err(|error| format!("could not write {}: {error}", out.display()))?;
        println!("\nFull results written to {}", out.display());
    }
    Ok(!links::broken(&results).is_empty())
}
