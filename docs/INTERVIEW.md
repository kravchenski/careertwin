# Interview assistant

A small window you keep next to Zoom, Meet or Teams. It listens to the call,
shows what the interviewer said and streams talking points and a short spoken
answer built from **your own** background in `ABOUT.md`.

<p align="center"><img src="media/interview-overlay.png" alt="Overlay with a transcript and a suggested answer" width="360"></p>

## Use it fairly

Many employers forbid AI help during interviews, and recording a call can need
the other side's consent. Check the rules for each interview and tell the
interviewer when you are expected to. The tool never hides itself from screen
sharing, and it only suggests: you decide what to say.

## Run it

```bash
uv sync --extra interview          # PySide6 and faster-whisper
switchyard                         # the language model gateway
uv run careertwin-interview --doctor
uv run careertwin-interview --job <vacancy id | description.txt | https://…>
```

The commands live in the project's virtualenv, not on your `PATH`: run them with `uv run`, or
`source .venv/bin/activate` (`activate.fish` in fish) first. `--doctor` checks Qt, Whisper,
PipeWire, Switchyard and your `ABOUT.md`. Start
the interview app before the call, join the call, and speak English, Russian,
Polish or anything else Whisper knows.

| Option | Meaning |
|---|---|
| `--job` | Vacancy id from the pipeline, a text file, or a URL; adds the requirements to the prompt |
| `--notes "…"` | Extra facts for this interview |
| `--whisper small` | `base` is fastest, `small` is a good default, `medium` and `large-v3-turbo` are the most accurate |
| `--language auto` | Force `en`, `ru`, `pl`, … if auto-detection slips |
| `--mic` | Also transcribe your microphone so the transcript shows both sides; use headphones |
| `--source NAME` | Another Pulse source for the call audio; `--list-sources` shows them |
| `--text-only` | No audio: type or paste questions |
| `--xcb` | Run through XWayland so **Pin** (always on top) works under Wayland |
| `--model` | Gateway model id; env `INTERVIEW_MODEL`, then `SWITCHYARD_MODEL`, then `auto` |

Shortcuts: `Ctrl+H` hint now, `Ctrl+P` pause. **Hint now** answers the last
thing the interviewer said even if it did not sound like a question.

## How it works

```
call audio (sink monitor) ──► 20 ms frames ──► utterance segmenter
                                                     │
                       local Whisper ◄───────────────┘
                            │ text
                  is it a question? ── no ──► transcript only
                            │ yes (debounced, split questions merged)
        question + ABOUT.md + vacancy + last turns ──► Switchyard ──► streamed hint
```

- **Audio stays local.** `parec` reads the sink monitor; faster-whisper
  transcribes on your machine (CPU, or GPU when its CUDA libraries load).
- **Only text leaves the machine:** the question, recent turns, and your
  context go to the model through Switchyard.
- **Answers stick to your facts.** The prompt forbids inventing employers,
  dates, numbers or skills. If your context has nothing on a topic, the first
  bullet says "No direct experience on record" and the answer is an honest bridge.
- **Salary follows your policy** from the Studio. In `auto` mode it never states
  a number you did not choose.
- **Transcripts** are saved to `data/interviews/` when you close the window.
  Audio is never stored.

## Practice before the call

The same coach is available to agents and over HTTP:

```bash
curl -s localhost:8000/v1/interview/practice -H 'content-type: application/json' \
  -d '{"job_id": "<vacancy id>", "count": 8}'
curl -s localhost:8000/v1/interview/suggest -H 'content-type: application/json' \
  -d '{"question": "Tell me about a time you debugged production."}'
```

In an agent session use `prepare_interview_questions` and
`suggest_interview_answer` (see [AGENTS.md](AGENTS.md)).

## Troubleshooting

- **No transcript:** run `--list-sources`, pick the monitor of your output
  device with `--source`. Bluetooth headsets that switch profile during a call
  change the sink; restart the app after joining.
- **Pin does nothing:** Wayland ignores always-on-top for normal windows. Use
  `--xcb`, or set a "keep above" rule for the window in your compositor.
- **First start is slow:** Whisper downloads its model once and loads it before
  listening starts; the status line says so.
- **`libcublas` errors:** the app already falls back to CPU. For a GPU, install
  the CUDA 12 runtime libraries.
