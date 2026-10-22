# Interview assistant

A compact window you keep next to Zoom, Meet or Teams. It listens to the call, and when the
interviewer asks something it shows a short answer built from **your own** background:
your `ABOUT.md`, the CV the employer received, and the vacancy. Hints come in the language
of the interview.

<p align="center"><img src="media/interview-overlay.png" alt="The overlay: status dots, Ask and Screenshot buttons, the question and four bullet points" width="760"></p>

## Use it fairly

Many employers forbid AI help during interviews, and recording a call can need the other
side's consent. Check the rules for each interview and tell the interviewer when you are
expected to. The tool never hides itself from screen sharing, and it only suggests: you
decide what to say.

## Run it

```bash
uv sync --extra interview          # PySide6 and faster-whisper
switchyard                         # the language model gateway
uv run careertwin-interview --doctor
uv run careertwin-interview --job <vacancy id | description.txt | https://…> --resume cv.pdf
```

The commands live in the project's virtualenv, not on your `PATH`: run them with `uv run`, or
`source .venv/bin/activate` (`activate.fish` in fish) first. `--doctor` checks Qt, Whisper,
PipeWire, Switchyard, your `ABOUT.md`, the CV and the screenshot tool.

## What it knows about you

Every hint is generated from these sources, shown in the dim line at the bottom of the window:

| Source | How it gets there |
|---|---|
| **`ABOUT.md`** | Always. Read whole, including *Interview stories*, *Honest limits* and *Never claim*. If it is too long, the least useful sections (*Applications log*, *CV files*, *Contact*) go first, never the tail. |
| **Your CV** | `--resume cv.pdf` (repeat for several), `CAREERTWIN_RESUME=/path/a.pdf:/path/b.pdf` in `.env`, **drag a PDF onto the window**, or ☰ → *Add CV…*. PDF, DOCX and text work; a scanned or blank PDF is refused with a message. |
| **The CV that was sent** | With `--job <vacancy id>` the CV prepared for that vacancy is attached automatically, so answers match what the interviewer read. |
| **The vacancy** | `--job` as an id, a text file or a URL. |
| **Notes** | `--notes "extra facts for this interview"`. |

If the CV and `ABOUT.md` disagree, `ABOUT.md` wins and the first bullet says the CV differs.
A CV you keep per role (`Name_CV_{Role}_{EN|PL}.pdf`) works well: pass the one you sent.

## Language

Answers are written in the **interview language**, whatever language the question, `ABOUT.md`
or the CV is in. Company names, technologies and job titles are never translated.

- `--language auto` (default): Whisper detects the language of each utterance and the window
  settles on the majority of the last few, so one stray sentence does not flip it.
- `--language ru` (or `en`, `pl`, `de`, `uk`, …): fixes the language for speech recognition
  and answers.
- ☰ → *Interview language* changes it during the call.

The window itself is translated into English, Russian and Polish and follows the interview
language.

## The window

| Control | Does |
|---|---|
| ⏸ | Pause or resume listening |
| Dots | Live level of the call audio, so you can see it is being heard; turn into text while loading, thinking or on errors |
| **Ask** `Ctrl ↵` | Answer the last thing the interviewer said, even if it did not sound like a question |
| **Screenshot** `Ctrl ⇧ ↵` | Send the screen to a vision model: a coding task, a slide, a question on screen. The window becomes transparent for the capture. Needs `grim` (Wayland) or `spectacle`; `maim`/`scrot` on X11 |
| Input + **Get answer** | Type or paste a question |
| `Ctrl ←` `Ctrl →` | Page through earlier answers (`2/2`) |
| ☰ | Transcript, full spoken answer, pin on top, opacity, add CV, language, quit |

Answers are short bullets with the key terms in bold. Turn on ☰ → *Full answer* to also see a
30-second spoken version. `Ctrl P` pauses.

Questions are recognised automatically (debounced, split questions merged). The first bullet
says "No direct experience on record" when your context has nothing to draw on, and the answer
is an honest bridge instead of a bluff.

### Options

| Option | Meaning |
|---|---|
| `--job` | Vacancy id from the pipeline, a text file, or a URL |
| `--resume FILE` | CV to read; repeatable |
| `--notes "…"` | Extra facts for this interview |
| `--language auto` | Interview language |
| `--whisper small` | `base` is fastest, `small` a good default, `medium` and `large-v3-turbo` the most accurate |
| `--mic` | Also transcribe your microphone so the transcript shows both sides; use headphones |
| `--source NAME` | Another Pulse source for the call audio; `--list-sources` shows them |
| `--text-only` | No audio: type or paste questions |
| `--framed` | A normal window with a title bar instead of the frameless strip |
| `--no-pin` | Do not keep the window on top |
| `--xcb` | Run through XWayland so *pin on top* works on compositors that ignore it |
| `--model` | Gateway model id; env `INTERVIEW_MODEL`, then `SWITCHYARD_MODEL`, then `auto` |

## How it works

```
call audio (sink monitor) ─► 20 ms frames ─► utterance segmenter ─► local Whisper (+ language)
                                                                          │ text
                             is it a question? ─► debounce ─► streaming coach ─► Switchyard (SSE)
                                                                          │ tokens
          ABOUT.md · CV · vacancy · last turns · interview language ──────┘
```

- **Audio stays local.** `parec` reads the sink monitor; faster-whisper transcribes on your
  machine (CPU, or GPU when its CUDA libraries load). Speech recognition is primed with your
  skills and the vacancy so names like Kubernetes are heard correctly.
- **Only text leaves the machine:** the question, recent turns, your context and CV, and
  screenshots you send go to the model through Switchyard.
- **Answers stick to your facts.** The prompt forbids inventing employers, dates, numbers or
  skills.
- **Salary follows your policy** from the Studio. In `auto` mode it never states a number you
  did not choose.
- **Transcripts** are saved to `data/interviews/` when you close the window. Audio is never stored.

## Window managers

The window is frameless and translucent; drag it by the empty parts of the header and resize
from the corner grip. Qt sets the window's app id to `careertwin-interview`, so you can
float it. In niri (25.01 or later):

```kdl
window-rule {
    match app-id="careertwin-interview"
    open-floating true
    default-column-width { fixed 920; }
    default-window-height { fixed 400; }
}
```

Without such a rule a tiling compositor gives it a full tile, which the layout handles but
looks sparse. Under Wayland, *pin on top* is the compositor's call; `--xcb` runs the window
through XWayland where the hint is honoured.

## Practice before the call

The same coach is available to agents and over HTTP:

```bash
curl -s localhost:8000/v1/interview/practice -H 'content-type: application/json' \
  -d '{"job_id": "<vacancy id>", "count": 8, "language": "ru"}'
curl -s localhost:8000/v1/interview/suggest -H 'content-type: application/json' \
  -d '{"question": "Tell me about a time you debugged production.", "language": "en"}'
```

In an agent session use `prepare_interview_questions` and `suggest_interview_answer` (see
[AGENTS.md](AGENTS.md)).

## Troubleshooting

- **No transcript:** watch the dots; if they stay dark, run `--list-sources` and pick the
  monitor of your output device with `--source`. Bluetooth headsets that switch profile during
  a call change the sink; restart the app after joining.
- **CV refused:** the PDF has no text layer. Export a text PDF or attach DOCX.
- **First start is slow:** Whisper downloads its model once and loads it before listening
  starts; the status pill says so.
- **`libcublas` errors:** the app already falls back to CPU. For a GPU, install the CUDA 12
  runtime libraries.
