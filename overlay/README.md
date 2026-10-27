# CareerTwin Interview, in Rust

The interview window of CareerTwin rewritten in Rust (egui). It reads and writes the files of the Python window
(`data/interview/settings.json`, `context.json`, the saved interviews), so the two can be swapped while the port
is finished. **The Python window is still the one that hears a call;** this one answers typed questions and the
screen.

```bash
cd overlay
cargo run --release                       # the window
cargo run --release --example export_questions -- ../data/interviews /tmp/out
cargo test                                # 120+ tests; UI_SHOTS=/some/dir writes pictures of every screen
```

## What is in it

| Part | Status |
| --- | --- |
| Question kinds, languages, prompts, reply parser (generated from the Python tables) | done, tested |
| Answers: streaming, several API keys per address (next key on a limit), cooldowns, the fallback chain, images | done, tested |
| Settings: four tabs (Interview, Practice, AI, General), three models (answers, screenshots, audio), accounts, base URL | done |
| Practice: a checklist for the technical interview, a bank of likely questions, a CV review | done |
| Export of the questions of saved interviews (CSV, Markdown, JSON) | done |
| New version window, with a SHA256 checked installer download (never started by the program) | done |
| App icon: colour variants or your own image | done |
| Control socket (`careertwin-interview-ctl screenshot` from a compositor key) | done (Linux, macOS) |
| Screenshot capture | Linux and macOS; Windows next |
| Hearing the call (system audio, VAD, speech recognition, live captions, early answers) | next |
| Defending a project (dossier, code search) | next |
| Windows: global key, screen capture, hiding from screen sharing | next |
| Languages of the window | English and Russian; Polish, Ukrainian, German, French and Spanish next |
