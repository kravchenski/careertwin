# CareerTwin Prep

A separate program, written in Rust, for practising interviews **out loud** with an interviewer who talks like a
person. It is not part of the interview overlay: the overlay helps during a real call, Prep is where you train for it.

Three tabs sit in a pill at the top:

| Tab | What it is for |
|---|---|
| **English** | A relaxed spoken conversation at your level (A2 to C1). The partner repeats your mistakes back correctly inside its own sentences, as a person would, and does not stop you to correct. *Say it in English*: say a thought in Russian, hear it in natural English in three registers, say it back and see which words went missing. Phrases from your reviews come back just before you forget them. |
| **Technical** | A spoken technical interview: it asks about the real projects in your `ABOUT.md`, then about the stack, follows up on vague answers, raises the difficulty when you do well, and never gives hints. *Concept checks*: a question, a few answers and **how sure you are**; sure and wrong comes back soonest. |
| **Interview** | Recruiter or hiring-manager interview: introduction, motivation, behavioural questions, availability and salary (only as your profile allows). *Prepare for a vacancy*: your CV against the vacancy, the gaps and how to answer them, and the questions it invites, each of which you can practise. |

Every call ends with a **review**: scores, what to work on, each answer with what went well and what to fix, a better
answer you can listen to and say yourself, language notes and phrases to keep.

<p align="center"><img src="media/prep-home.png" alt="The Technical tab: the pill of three tabs and the three ways to practise" width="760"></p>

## Run it

```bash
./scripts/install-prep.sh          # builds with offline hearing and voice (needs Rust and the ALSA headers)
LIGHT=1 ./scripts/install-prep.sh  # smaller build that uses cloud or server engines only
careertwin-prep --doctor           # checks the model, recognition, voice, microphone and speakers
careertwin-prep
```

Settings are in the window (the gear). Nothing is needed in a file, but a key can also come from its usual
environment variable (`GROQ_API_KEY`, `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`, `SWITCHYARD_API_KEY`), so a key never has
to be typed anywhere.

<p align="center"><img src="media/prep-review.png" alt="The review after a call: scores, what to work on, and a better answer to listen to" width="760"></p>

## How it talks like a person

The call is one loop (`prep/src/conversation.rs`) and each behaviour has a test that runs a whole call on a paused clock.

- **It starts speaking before it has finished writing.** The model streams its reply, the reply is cut into sentences,
  and each sentence is voiced as soon as it is complete, while the next is still being written.
- **It waits for the end of a thought.** If what you said ends in "and", "so", "um" or a comma, it waits a moment for
  the rest and joins the two parts into one answer.
- **It acknowledges you when it needs a moment.** If the model is slow, a short "Mm-hm" is played first, never after
  the real answer has started.
- **It nudges you when you go quiet.** "Take your time" after 14 seconds, an offer to repeat the question after 40,
  and after a minute a simpler question.
- **You can talk over it.** With headphones on, speech over the interviewer stops it at once, and only what was
  really said goes into the history. On speakers the microphone is shut while it talks, so it never answers its own voice.
- **It watches the clock.** Near the end it asks whether you have questions; when time is up the answer is the goodbye.
- **It does not mistake noise for an answer.** "Thank you." from a cough, or a click, is dropped.

The interviewer only says what the context allows: facts about you come from your `ABOUT.md` and CV and nowhere else,
facts about the company only from the vacancy text, and where the profile has an honest limit ("no production
Kubernetes") it asks about the gap instead of letting you claim it.

## Engines: cloud, your own server, or offline

Three things are chosen separately: the **language model** (the interviewer's mind), **recognition** (hearing you) and
the **voice**.

| | Cloud | Your own server | Inside this program (offline) |
|---|---|---|---|
| Language model | Groq, OpenAI, OpenRouter | Switchyard, Ollama, LM Studio, a llama.cpp server | runs in Ollama or LM Studio; this program talks to it |
| Recognition | Groq (`whisper-large-v3-turbo`), OpenAI | any server with `/v1/audio/transcriptions` | **Whisper** through whisper.cpp |
| Voice | OpenAI (`gpt-4o-mini-tts` takes a tone instruction), ElevenLabs | any server with `/v1/audio/speech`, such as Kokoro, Chatterbox or Qwen3-TTS wrapped in one | **Kokoro-82M** in pure Rust, English |

Cloud engines receive what they speak or hear. Pointing an engine at `http://127.0.0.1:...` keeps it on this computer,
and an address that would send a key in the clear to another machine is refused.

### Offline

Build with the default `local` feature of the installer, then in Settings choose *In this program (offline)* for
recognition and for the voice. The program downloads the models on first use. Every file has its size and SHA-256
written in the program (`prep/src/models.rs`) and is checked before it is used, and the Kokoro files are pinned to one
commit of their repository.

What fits where (the program detects the memory and the cores and suggests):

| Memory | Hearing | Interviewer (in Ollama) |
|---|---|---|
| 2-3 GB | Whisper tiny (32 MB) | too small to hold an interview |
| 4-7 GB | Whisper base (60 MB) | `gemma3:1b` is the most that fits and is weak: use a cloud model for the interviewer |
| 8-15 GB | Whisper small (190 MB) | `gemma3:4b` |
| 16 GB and a GPU or 8+ cores | Whisper large-v3-turbo (574 MB) | `gemma3:12b` |
| 28 GB and up | the same | `gemma3:27b` |

A model that follows a spoken conversation and asks real follow-ups needs about four billion parameters at least.

Measured on a 12-core laptop CPU, release build, GPU not used (`cargo test --release --features local --test local_models`):

| | Time | What it means |
|---|---|---|
| Hearing, Whisper small, a four-second phrase | 0.65 to 0.7 s | a short answer is understood before you notice; a 20-second one takes a few seconds |
| Voice, Kokoro, four seconds of speech | about 3 s (0.77 times real time) | the first sentence of a reply comes after about three seconds, and the next ones keep up |
| Round trip: the voice says a sentence, the ear hears it | 100% of the words | |

The program makes Whisper read only as much audio as the clip needs, which made short answers about four times faster
than reading a full 30-second window. On a CPU with fewer cores than that the voice can fall behind real time; then use
a cloud or server voice and keep hearing offline. With an NVIDIA GPU and CUDA the installer builds both engines with
CUDA and they are much faster.

The offline voice speaks English. For Russian prompts and explanations use a voice server or a cloud voice, or turn
the Russian voice off: the interviewer then writes those parts.

## Privacy

- The settings file is written readable only by you (mode `0600`), in a folder that only you can enter.
- A key is never printed, logged or shown: it appears as `***` in any text the program makes, including error messages
  and `--doctor`.
- Sessions, the review deck and cached voice clips are private files in `~/.local/share/careertwin-prep` and
  `~/.cache/careertwin-prep`. Delete a session from the list and its file is gone.
- Your `ABOUT.md` and CV are read from where you point to and are sent only to the language model you chose.

## Checking and troubleshooting

`careertwin-prep --doctor` tells you which piece is wrong and what to do:

```
ok   profile                ABOUT.md · 1.5k
ok   language model         http://127.0.0.1:3260 answers, 319 models, using auto
FAIL recognition            Speech recognition returned 401: ... (check the recognition key in Settings)
ok   microphone             Default Audio Device, level 424
```

`--no-devices` leaves the microphone and speakers alone; `--no-probes` does not ask the recogniser and the voice for a
sample.

## Development

```bash
cd prep
cargo test                          # 290+ tests; none needs a sound card or the network
cargo test --features local         # also builds whisper.cpp and the candle voices (needs cmake)
UI_SHOTS=/tmp/shots cargo test --test ui_shots   # writes a PNG of every screen
cargo test --test hardware -- --ignored --nocapture      # the real microphone and speakers
cargo test --test live_llm -- --ignored --nocapture      # the interviewer on a real model, with the demo profile
cargo test --release --features local --test local_models -- --ignored --nocapture   # the offline voice and ear
```

The window is egui. The sphere is painted pixel by pixel on the CPU, so it looks the same everywhere and is checked in
tests. Fonts are PT Serif, Manrope and JetBrains Mono (SIL Open Font License, in `prep/assets/fonts`).
