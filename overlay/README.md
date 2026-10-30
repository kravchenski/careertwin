# CareerTwin Interview, in Rust

The interview window of CareerTwin (egui): it hears the call, writes what to say, answers the screen and defends a
project from its code. It reads and writes the files of the older Python window (`data/interview/settings.json`,
`context.json`, the saved interviews, the stored project dossiers), so a folder of data works with either.

```bash
cd overlay
cargo run --release                       # the window
cargo run --release --example export_questions -- ../data/interviews /tmp/out
cargo test                                # 200+ tests; UI_SHOTS=/some/dir writes pictures (add -- --test-threads=1)
cargo build --release --no-default-features --features gui   # without Whisper on the computer (no cmake needed)
```

Building with Whisper (the default) needs cmake, a C++ compiler and libclang (whisper.cpp). Release packages:
`scripts/package_overlay.sh linux|macos`, and the release workflow for Windows.

## What is in it

| Part | Notes |
| --- | --- |
| Question kinds, languages, prompts, reply parser | generated from the Python tables, tested against them |
| Answers: streaming, several API keys per address (next key on a limit), cooldowns, the fallback chain, images | tested with a fake server |
| Hearing the call: system audio (PulseAudio/PipeWire on Linux, WASAPI loopback on Windows), the microphone, silence-based utterance detection | tested with fake sources; **never run on a real Windows or Mac sound card** |
| Speech recognition: Whisper on the computer (ggml models downloaded once, SHA256 checked) or Deepgram | the real Whisper build compiles; accuracy not measured here |
| Live captions, early answers while the interviewer is still speaking, SKIP for words that are not a question | tested with a scripted recogniser |
| Defending a project: reads a folder (secrets and binaries left out, tokens blanked), a dossier, a keyword search over the code; a project question sends a lean request (about a third of a normal one) so it fits a free provider's limit | tested |
| Settings: four tabs (Interview, Practice, AI, General), three models (answers, screenshots, audio), accounts, base URL | drawn headless in tests |
| Practice: a checklist for the technical interview, a bank of likely questions, a CV review | done |
| Export of the questions of saved interviews (CSV, Markdown, JSON) | done |
| New version window, with a SHA256 checked installer download (never started by the program) | done |
| Themes (seven and your own), the dot equalizer, app icon: colour variants or your own image | done |
| Windows: a screenshot key that works in any program (`RegisterHotKey` on its own thread, fallbacks and a log), screen capture (GDI), the window kept out of screen sharing (`WDA_EXCLUDEFROMCAPTURE`) | compiled for Windows in CI; **not run on a real Windows machine** |
| Linux and macOS screenshots (grim, spectacle, gnome-screenshot, maim, scrot; screencapture) and the control socket (`careertwin-overlay ctl screenshot`) | Linux tried |
| Window languages: English, Russian, Polish, Ukrainian, German, French, Spanish | every key is in every language (a test); drawn headless in each |
| Screen readers: every input has a name (the check of the test harness is on) | the accessibility tree is built; no screen reader has been tried |

## Not in it

- Call audio on macOS (it needs a virtual audio device). The microphone works.
- The graphics card for Whisper: it runs on the processor.
- A hands-on try on Windows or macOS.
