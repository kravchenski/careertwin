# CareerTwin Prep

A separate program, written in Rust, for preparing for interviews **out loud**, as a conversation with a tutor who talks
like a person. It is not part of the interview overlay: the overlay helps during a real call, Prep is where you train
for it.

Every part looks the same: a glowing sphere in the middle that moves with the voice, what the tutor says written under
it word by word as it is spoken, and two buttons at the bottom: the microphone and a cross to end the call. Start typing and a text box appears. In a call there is nothing else on the screen: no timer, no panel at the bottom.
A pill of three tabs sits at the top; each tab has one big button, because **the tutor leads** and opens whatever it
needs. The menu at the top left lists only what you did last.

| Tab | What it is for |
|---|---|
| **English** | **Study something together**: you bring a text, your notes, a list of new words or just a topic (paste it, or drop a text file on the window) and the tutor makes a lesson of it. It goes through everything first in your own language, then again in English with new questions, and fixes your grammar there. *Just talk* is a relaxed spoken conversation at your level, where the partner repeats your mistakes back correctly inside its own sentences. |
| **Technical** | **The interview**, about an hour and a half, led by the tutor: it takes the topics of your plan one after another (what you know least first), for each the terms in your own language and then again in English, and after the topics a coding task that it watches. Pictures can be asked for at any moment: any topic told as a short animated story. |
| **Interview** | **Your introduction** ("tell me about yourself") coached until it is short, true and yours. |

Under every tab, **Write to the tutor** (in the menu at the top left) opens a quiet chat with the same tutor: nothing is spoken, so it works in a lecture. A
good answer becomes a note.

Every call ends with a **review**: scores, what to work on, each answer with what went well and what to fix, a better
answer you can listen to and say yourself, language notes and phrases to keep, and **the 20% to keep**: the few lines
that matter, saved to your notes.

<p align="center"><img src="media/prep-home.png" alt="The Technical tab: the sphere, today's topic and the button to start it, and what you did last" width="760"></p>

## The plan: 80/20 for your vacancy

Paste the vacancy and the program works out what you are likely to be asked. It ranks the topics (object-oriented
programming, REST, SQL and so on) by how likely each is and by how much work it takes, keeps the few that hold **80% of
the likely questions**, and writes every term of those in plain words, in your language, with an everyday comparison and
a good spoken answer in English. Everything on the plan says how much of the likely questions you already cover.

It is the model's knowledge of what interviews for such a role usually ask, shaped by the text of the vacancy and your
`ABOUT.md`; it does not search the web, and it knows nothing about the company beyond what the vacancy says. A vacancy
that has changed since the plan was made is noticed.

## The terms lesson

The tutor goes through every term of a topic twice, in two stages. It speaks your own language until you ask to
practise English, never before: say "давай попрактикуемся на английском" (or "let's practise English", "now in
English") at any point, or answer yes when, after the last term, it offers to go through them in English.

1. **In your own language**, all the terms one after another. It asks what the term means. If you are right, it says so
   and moves on. If you are partly right, it fixes the rest in plain words with the comparison. If you say you do not
   know, it does not hand over the answer: it asks a small question that leads one step, or gives the comparison, and
   lets you say it.
2. **In English**, the same terms again from the first one, with new questions, the way an interviewer asks them. After each answer it
   fixes the grammar: it says the corrected sentence once, names the rule in your language and has you say it again,
   at most two mistakes a turn.

Each half is scored 0 to 3 by the tutor; what you know is kept, and the plan's coverage moves. A conversation is a focus
block: starting one starts the timer (a quiet chat does not). The main button on the Technical tab starts the whole
interview, topic after topic while there is time (about 65 minutes), then a coding task. The tutor ends each part by
itself and the next begins; the cross ends everything. The review and the notes of every part are made in the
background.

The language you practise (English by default) is a setting; German, Spanish, French, Italian or Polish work in the same
way. If you speak your own language to it while it is in English (you are lost, you did not understand), it answers
in your language, briefly, and goes back. Its silence nudges follow the stage: Russian in the Russian part.

## Pictures

Ask the tutor to show or draw something (or choose *Explain with pictures* and type a topic) and it is told as a short
animated story for a complete beginner: floating glowing panels around the sphere, which narrates in the corner while
the words light up. There are eight kinds of picture (boxes with arrows, a row of cells, a little code with the line that
matters lit, two things side by side, numbered steps, "this is like that", layers, and plain text). Items come in as
they are said, and a panel that continues from one scene to the next grows instead of appearing again.

You can pause, skip, go back, replay, close it, or ask for a simpler version. Speaking over it pauses it; when it ends,
the tutor asks one question to see whether it landed.

<p align="center"><img src="media/prep-picture.png" alt="A visual explanation of a queue: a row of people, a journey and a few lines of code, with the narrator under them" width="760"></p>

## Live coding

You get a task for the vacancy, in the language you choose, with the solution idea, the mistakes to look for and
three nudges kept **for the tutor only**. You write in an editor while the tutor, at the side, lets you think.

- When the code has been still for a few seconds, the tutor looks at it. **On the right track, it says nothing.**
- If something is wrong it never names the line or the fix. It asks you to check: *"Take [2, 7, 11] with the target 9 and
  follow the loop; what does it return?"* If you stay stuck it points at the area, never at the answer.
- If you are stuck, you get a nudge as a question, from the vague ones to the specific.
- If you say you do not know, you work it out **together**, one small step at a time, and you type every change.
- When you say you are done, it asks you to try a tricky input and for the complexity.

It also watches for what you would not be allowed to do in a real interview: a block that arrives all at once instead of
being typed, or the window left for a long time followed by a finished solution. It does not accuse; it asks you to walk
it through that part in your own words. **It sees only this program's window**: it cannot see other windows or the
screen, and it does not try to. The review is told what it saw.

<p align="center"><img src="media/prep-coding.png" alt="Live coding: the tutor with the task at the left, the editor at the right, a question on the screen" width="760"></p>

## Notes, the focus timer and your numbers

- **Notes** are written by the tutor, not by you: after every call it keeps the few lines worth remembering and the
  slips to avoid, one idea each, and in a chat an answer worth keeping is saved on its own. You find them under the
  review of the call (**Open notes**); there you can star the ones that matter, delete one, and copy everything as
  Markdown.
- **Focus timer** (top right): a Pomodoro of 25 minutes of work and 5 of rest, a long rest after four blocks, with a soft
  chime. It starts by itself when the program opens and again after each break; a click pauses it, and Settings can turn
  the automatic start off. The lengths are in Settings. When a block ends during a call, the tutor is told and suggests a break at the next
  natural pause. `Ctrl+Shift+T` starts or pauses it.
- **The panel at the bottom** shows your name and address, and a ring that fills as today's goal is met. The ring
  opens **Your activity**: today against the goal, a calendar of the days you practised (the lighter the square, the
  more you did, as on GitHub), the streak, the week, the total and how much of the plan you have covered. The menu of
  the panel has Settings (`Ctrl+Shift+,`), Your activity, Language, Help and Quit.

<p align="center"><img src="media/prep-menu.png" alt="The menu that opens from the panel at the bottom left: Settings, Your activity, Language, Help, Quit" width="520"></p>

**A display font.** Titles, big numbers and the panel at the bottom can be set in a free display font. It is not shipped,
because its licence does not say that the file may be passed on: put **Ralev001** (or **SOUTHGHETTO**) as a `.ttf` or
`.otf` file into `~/.local/share/fonts/` or `~/.local/share/careertwin-prep/fonts/` and restart the program. Without it
the program uses its serif and sans fonts.

## Run it

```bash
./scripts/install-prep.sh          # builds with offline hearing and voice (needs Rust and the ALSA headers)
LIGHT=1 ./scripts/install-prep.sh  # smaller build that uses cloud or server engines only
careertwin-prep --doctor           # checks the model, recognition, voice, microphone and speakers
careertwin-prep
```

Settings are in the menu at the bottom left of the window. Nothing is needed in a file, but a key can also come from its usual
environment variable (`GROQ_API_KEY`, `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`, `SWITCHYARD_API_KEY`), so a key never has
to be typed anywhere.

<p align="center"><img src="media/prep-review.png" alt="The review after a call: the 20% to keep at the top, then scores and what to work on" width="760"></p>

## How it talks like a person

The call is one loop (`prep/src/conversation.rs`) and each behaviour has a test that runs a whole call on a paused clock.

- **It starts speaking before it has finished writing.** The model streams its reply, the reply is cut into sentences,
  and each sentence is voiced as soon as it is complete, while the next is still being written.
- **It waits for the end of a thought.** If what you said ends in "and", "so", "um" or a comma, it waits a moment for
  the rest and joins the two parts into one answer.
- **It lets a long answer run.** The longer you have been talking, the longer a pause has to be before it takes your
  turn as finished: a short answer is answered straight away, and after about fifteen seconds it waits up to two and a
  half seconds more, so a minute-long story with pauses to think is one turn, not four.
- **Its first words come quickly.** The first piece of a reply is cut at its first comma (or after a few words), so the
  voice starts on a short piece while the rest is still being made; the voice loads while the model is thinking; and
  the voice, the ear and the sphere run at the ordinary priority, so the rest of the machine is left alone. The real
  wait, from your last word to its first, is measured by `prep/tests/latency.rs` on the real models. Most of what is
  left is the model: a fast one (Settings, *Fast model*, for example one served by Groq through your gateway) answers in
  a third of a second where a gateway's `auto` can take several seconds.
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
| Voice | OpenAI (`gpt-4o-mini-tts` takes a tone instruction), ElevenLabs | any server with `/v1/audio/speech`, such as Kokoro, Chatterbox or Qwen3-TTS wrapped in one | **Kokoro-82M** in pure Rust for English and **Piper** (`ru_RU-irina-medium`) for Russian |

Cloud engines receive what they speak or hear. Pointing an engine at `http://127.0.0.1:...` keeps it on this computer,
and an address that would send a key in the clear to another machine is refused.

### Offline

Build with the default `local` feature of the installer, then in Settings choose *In this program (offline)* for
recognition and for the voice. The program downloads the models on first use. Every file has its size and SHA-256
written in the program (`prep/src/models.rs`) and is checked before it is used, and the Kokoro files are pinned to one
commit of their repository.

A download shows how far it is in the *Background* list of the left drawer and in Settings, and has a cross that stops it
and removes the half-written file. Choosing another model in Settings stops the one that was not wanted after all.

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
| Voice, Kokoro, four seconds of speech | about 3 s on the processor (0.77 times real time); 0.27 s on an NVIDIA card | on the processor the first piece of a reply comes after about 2.5 s; on the card after half a second |
| The whole wait, your last word to its first, on the card with a fast model | 2.3 s | 1.1 s of it is hearing you (0.7 s of silence ends a turn), 0.6 s the model, 0.5 s the voice |
| Round trip: the voice says a sentence, the ear hears it | 100% of the words | |

The program makes Whisper read only as much audio as the clip needs, which made short answers about four times faster
than reading a full 30-second window. On a CPU with fewer cores than that the voice can fall behind real time; then use
a cloud or server voice and keep hearing offline. Hearing runs on the processor. The voice runs on an NVIDIA card when the installer finds one: it looks for `nvidia-smi`
and the CUDA toolkit (`nvcc`), builds with the `gpu` feature for the card's compute capability (using an older gcc
if yours is newer than nvcc accepts) and falls back to the processor build if that does not work; `NO_GPU=1`
skips it. The first sentence after the program starts waits a few seconds for the card to be set up, which is
why the voice is loaded as soon as the program opens. The build needs the CUDA libraries at run time, so on a computer
without them use `NO_GPU=1`.

Kokoro speaks English only, so the Russian half of a lesson is said by Piper, the speech engine of the Rhasspy project:
its own Linux x86-64 release (26 MB) and one Russian voice (63 MB), fetched on first use and checked against the sizes
and SHA-256 written in the program like every other model; the archive is unpacked once, and each sentence is made by
running it (about a third of a second for a sentence, 0.04 times real time). Each sentence is voiced in the language it
is written in, so a reply that switches in the middle is spoken by both voices. On another system, or if the voice is not
downloaded, the Russian sentences are shown but not spoken; `careertwin-prep --doctor` says which. A cloud voice or a
voice server speaks both languages.

Pieces of speech are made one at a time, in the order asked for: made at once, they share the cores and the first one,
the one you are waiting for, comes last.

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
cargo test                          # 450+ tests; none needs a sound card or the network
cargo test --features local         # also builds whisper.cpp and the candle voices (needs cmake)
UI_SHOTS=/tmp/shots cargo test --test ui_shots   # writes a PNG of every screen
cargo test --test hardware -- --ignored --nocapture      # the real microphone and speakers
cargo test --test live_llm -- --ignored --nocapture      # the interviewer, the tutor, the pictures and the coding tutor on a real model, with the demo profile
cargo test --release --features local --test local_models -- --ignored --nocapture   # the offline voice and ear
```

The window is egui. The sphere and the floating panels are painted by the program itself (the sphere pixel by pixel on
the CPU), so they look the same everywhere and are checked in tests. Fonts are PT Serif, Manrope and JetBrains Mono (SIL Open Font License, in `prep/assets/fonts`).
