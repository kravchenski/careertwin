//! Pasting needs eframe's `clipboard` feature. The program builds eframe with
//! `default-features = false`, which leaves it off, and then every paste is silently dropped: the
//! vacancy could not be pasted into its fields. This keeps the feature from being lost again.

#[test]
fn the_window_library_is_built_with_the_clipboard() {
    let manifest = include_str!("../Cargo.toml");
    let line = manifest.lines().find(|line| line.starts_with("eframe")).expect("the manifest names eframe");
    assert!(line.contains("\"clipboard\""), "pasting and copying need eframe's clipboard feature: {line}");
}
