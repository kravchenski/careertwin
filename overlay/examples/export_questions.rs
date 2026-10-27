//! Export the questions of the saved interviews: `cargo run --example export_questions -- <transcripts> <into>`.

use std::path::PathBuf;

use careertwin_overlay::export::{Format, export_questions, rows};
use careertwin_overlay::transcript::read_all;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(from), Some(into)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: export_questions <transcripts folder> <folder to write into>");
    };
    let all = rows(&read_all(&PathBuf::from(from)));
    println!("{} questions in {} kinds", all.len(), all.iter().map(|row| row.kind.as_str()).collect::<std::collections::BTreeSet<_>>().len());
    let stamp = chrono::Local::now().format("%Y-%m-%d").to_string();
    for path in export_questions(&PathBuf::from(from), &PathBuf::from(into), &Format::ALL, &stamp)? {
        println!("{}", path.display());
    }
    Ok(())
}
