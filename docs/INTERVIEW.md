# Interview assistant

A compact window you keep next to Zoom, Meet or Teams. It listens to the call, and when the
interviewer says something it writes **what you should say**: short first-person lines you can
read out, built only from **your own** background (your `ABOUT.md`, your CV, the vacancy), in
the language of the interview.

<p align="center"><img src="media/interview-overlay.png" alt="The overlay: status dots, Ask and Screenshot buttons, the question and the lines to say" width="760"></p>

For "hi" it gives the greeting itself, who you are and an invitation to start. For "What is
Python?" it explains: a one-sentence definition, the key properties, typical uses and the main
trade-off, with at most a closing line about your own experience. For "Do you have experience
with Kubernetes?" the first line says honestly that you do not, and the rest bridges to what you
really did. Nothing in it is advice such as "keep it short", and an earlier greeting is never
repeated. Typos and speech-recognition slips in the question ("pytohn") are read through.

## Use it fairly

Many employers forbid AI help during interviews, and recording a call can need the other
side's consent. Check the rules for each interview and tell the interviewer when you are
expected to. The tool never hides itself from screen sharing, and it only suggests: you
decide what to say.

## Run it

```bash
./scripts/install.sh               # once: commands in ~/.local/bin and a launcher entry
switchyard                         # the language model gateway
careertwin-interview               # or open "CareerTwin Interview" from your launcher
```

The install is editable (it runs this checkout; re-run the script when dependencies change) and
`./scripts/uninstall.sh` removes it. Without installing, use `uv sync --extra interview` and
`uv run careertwin-interview`; the commands then live in the project's virtualenv, not on your
`PATH` (`source .venv/bin/activate`, or `activate.fish` in fish).

**Windows.** `make windows` (or `./scripts/build_windows.sh`) builds a portable 64-bit folder and zip,
`dist/windows/CareerTwin-Interview-windows-x64.zip`, on Linux without Wine: uv fetches the Windows
wheels and the program runs on the python.org embeddable Python. Unzip it anywhere writable and start
*Start CareerTwin Interview.bat*; your data lives in its `data` folder. In this version the window,
settings, documents, typed questions, listening and screenshots work. The call is heard by recording
what the default output plays (WASAPI loopback through the `soundcard` package; pin another output in
*Settings → Audio*), and a screenshot copies the primary screen or all screens (GDI, no extra package).
Speech recognition runs on the CPU there. The build is checked under Wine only: the doctor runs, the
window renders, devices open and frames arrive at the right pace and the screen is read with the right
colours, but Wine plays no real sound into the recorder, so capture of an actual call has not been
tried on a real Windows machine: run *Check setup* there first.

**Everything is set up in the window.** Open ☰ → *Settings…* (or click the dim *Context* line
at the bottom). Changes apply at once and are saved, so the next start is already configured.
There is nothing to edit in files and no flag you need.

## One interview per vacancy

Each interview keeps its **own vacancy, the CV you sent for it and your notes**; `ABOUT.md` is
shared. Set one up in a minute:

1. ☰ → *Interview* → *New interview…* (or *New* at the top of Settings).
2. **Vacancy** tab: paste the vacancy text or a link, or pick it from the application pipeline
   (the CV prepared for it is attached automatically).
3. **Documents** tab: add the CV you sent for this vacancy (drop the file on the window or *Add
   files…*). Add anything else about you as *About me*.

The interview is named after the vacancy. Switch between interviews from ☰ → *Interview* or the
picker at the top of Settings; the vacancy, CV and notes change together, and the last one used
opens next time. The dim line at the bottom of the window shows which interview is active and what
it reads.

## Settings

<p align="center"><img src="media/interview-settings.png" alt="The Documents tab of the settings window" width="640"></p>

| Tab | What you set |
|---|---|
| **Interview picker** (above the tabs) | Switch, create, rename or delete interviews. |
| **Prepare** | Before the call. *Question bank* lists the questions this interviewer is likely to ask for the active interview, each with first-person lines to say: definitions are explained, and where you lack experience the first line says so plainly and the rest bridges to what you did. *Review my CV for this vacancy* compares the CV with the vacancy: a summary, the real strengths to lead with, gaps with an honest way to address each, the questions those gaps invite, and concrete fixes to the CV text. Results are kept per interview while the window is open. |
| **Documents** | Add your CV, a LinkedIn export, a cover letter or any file about you (PDF, DOCX, text) with *Add files…* or by dropping them on the window. Mark each as *CV (the employer has it)* or *About me (extra facts)*, switch it off without deleting it, or remove it. Files are copied into `data/interview/documents/`, so moving the original does no harm. A blank or scanned PDF is refused with a reason. |
| **Vacancy** | Pick a vacancy from the application pipeline (the CV prepared for it is attached too), or paste its text or a link. Notes for this interview go here as well. |
| **About me** | Your `ABOUT.md`, editable in place and saved to the file. This is the main source of facts; write the *Interview stories (STAR)*, *Honest limits* and *Never claim* sections for the best answers. |
| **Language** | Interview language (auto-detect or a fixed one), window language (follow the interview, or English, Russian, Polish), answer style (concise, standard, detailed), answer layout (main answer only, or lines plus the detailed answer) and whether to show the detailed answer. |
| **Audio & speech** | Listen to the call on or off, the call audio source (*Automatic* follows the output that plays the call, or pin one), your microphone, the Whisper model (`tiny` to `large-v3-turbo`) and device. |
| **AI model** | The *Provider* (Switchyard, Groq, OpenAI, OpenRouter, Ollama, LM Studio, or your own address), its API key and the model; *Load models* lists what the provider offers and *Test connection* checks it. Choosing another provider clears the key and the model, so a key is never sent to the wrong server. Cloud providers receive your CV, ABOUT text and each question. Leave everything empty to use `.env` (`SWITCHYARD_*`) and auto routing on a local Switchyard. |
| **Screenshot** | Turn the button on or off, choose the capture tool (grim, spectacle, maim, scrot), the delay, and *Test capture*. |
| **Window** | Keep on top, frameless or framed, opacity, text size, saving transcripts, and *Reset all settings*. |

Settings are stored in `data/interview/settings.json` (readable only by you; an API key you type
here is kept in that file) and your context in `data/interview/context.json`. Command-line flags
(`--language`, `--whisper`, `--model`, `--source`, `--mic`, `--text-only`, `--resume`, `--job`,
`--notes`, `--framed`, `--no-pin`) override the saved values for that run only; see
`careertwin-interview --help`.

## What it knows about you

Every hint is generated from the sources shown in the dim line at the bottom of the window:

- **`ABOUT.md`**, read whole, including the interview stories and honest limits. If it is too long,
  the least useful sections (*Applications log*, *CV files*, *Contact*) go first, never the tail.
- **Your documents** of the active interview, from the Documents tab. A document marked *CV* is treated as what the
  interviewer has read, so answers stay consistent with it; *About me* documents are extra facts.
  If a document and `ABOUT.md` disagree, `ABOUT.md` wins and the first line says so honestly.
- **The vacancy and your notes** of the active interview, from the Vacancy tab. With `--job <vacancy id>` the CV that was
  prepared for that vacancy is attached automatically.

With no context at all the window says so instead of guessing.

## Language

Answers are written in the **interview language**, whatever language the question, `ABOUT.md` or
your documents are in. Company names, technologies and job titles are never translated.

- *Auto-detect* (default): Whisper detects the language of each utterance and the window settles on
  the majority of the last few, so one stray sentence does not flip it.
- A fixed language (Settings → Language, or ☰ → *Interview language*) is used for speech
  recognition and answers, and can be changed during the call.

The window itself is translated into English, Russian and Polish.

## The window

| Control | Does |
|---|---|
| ⏸ | Pause or resume listening |
| Dots | Live level of the call audio, so you can see it is heard; text while loading, thinking or on errors |
| **Ask** `Ctrl ↵` | Answer the last thing the interviewer said, even if it did not sound like a question |
| **Screenshot** `Ctrl ⇧ ↵` | Send the screen to a vision model: a coding task, a slide, a question on screen. The window turns transparent for the capture |
| Input + **Get answer** | Type or paste a question |
| `Ctrl ←` `Ctrl →` | Page through earlier answers (`2/2`) |
| **Details** `Ctrl D` | Show or hide the detailed answer under the lines (lines layout) |
| `Ctrl M` | Switch between *main answer only* and *lines + details* |
| ☰ | Settings, Context, transcript, detailed answer, pin on top, interview language, quit |

**Main answer only** (Settings → Language → Answer layout, or `Ctrl M`) skips everything else: the model
writes just one paragraph to read aloud and it starts at once, so it is also the fastest layout.

In the default *lines* layout the first thing you see is the lines to say, with key terms in bold; they stream in within a few
seconds. Under them the **detailed answer** follows as the rest arrives, in three parts: the *main
answer* (a paragraph), *key points* (a term in bold and one sentence each) and *practical context*
(where it is used, or how it applies to this vacancy and your real work). It is on by default; the
**Details** button or `Ctrl D` hides it, and the *Concise* answer style skips it altogether for the
fastest replies. The window scrolls when the answer is long.
`Ctrl P` pauses. Questions are recognised automatically (debounced, split questions merged).

## How it works

```
call audio (sink monitor) ─► 20 ms frames ─► utterance segmenter ─► local Whisper (+ language)
                                                                          │ text
                             is it a question? ─► debounce ─► streaming coach ─► Switchyard (SSE)
                                                                          │ tokens
          ABOUT.md · documents · vacancy · last turns · interview language ┘
```

- **It does not wait for the end of the question.** While the interviewer speaks, a caption line (🎧)
  shows the words heard so far and refreshes about every 1.5 s. The question counts as finished after
  half a second of silence, and the request to the model leaves 0.4 s later (so a question split by a
  short pause is still sent as one). A long monologue is cut into pieces of at most 12 s instead of
  one 25 s block, and a caption that already covers the speech is used as the final text.
- **Audio stays local.** `parec` reads the sink monitor; faster-whisper transcribes on your
  machine (CPU, or GPU when its CUDA libraries load). Recognition is primed with your skills and
  the vacancy so names like Kubernetes are heard correctly.
- **Only text leaves the machine:** the question, recent turns, your context and documents, and
  screenshots you send go to the model through Switchyard.
- **Answers stick to your facts.** The prompt forbids inventing employers, dates, numbers or skills.
- **Salary follows your policy** from the Studio. In `auto` mode it never states a number you did
  not choose.
- **Transcripts** are saved to `data/interviews/` when you close the window (switch off in
  Settings → Window). Audio is never stored.

## Window managers

The window is frameless and translucent; drag it by the empty parts of the header and resize from
the corner grip (or turn *Frameless window* off). Qt sets the app id to `careertwin-interview`, so
you can float it. In niri (25.01 or later):

```kdl
window-rule {
    match app-id="careertwin-interview"
    open-floating true
    default-column-width { fixed 920; }
    default-window-height { fixed 400; }
}
```

Without such a rule a tiling compositor gives it a full tile, which the layout handles. Under
Wayland, *keep on top* is the compositor's call; `careertwin-interview --xcb` runs the window
through XWayland where the hint is honoured.

## Practice before the call

The Prepare tab does this in the window. The same coach is available to agents and over HTTP:

```bash
curl -s localhost:8000/v1/interview/cv-review -H 'content-type: application/json' \
  -d '{"job_id": "<vacancy id>", "language": "en"}'   # needs the vacancy's prepared CV
curl -s localhost:8000/v1/interview/practice -H 'content-type: application/json' \
  -d '{"job_id": "<vacancy id>", "count": 8, "language": "ru"}'
curl -s localhost:8000/v1/interview/suggest -H 'content-type: application/json' \
  -d '{"question": "Tell me about a time you debugged production.", "language": "en"}'
```

In an agent session use `prepare_interview_questions` and `suggest_interview_answer` (see
[AGENTS.md](AGENTS.md)).

## Troubleshooting

- **No transcript:** watch the dots; if they stay dark, hover the dots to see which output the app
  listens to. *Automatic* (the default) follows the output that is playing, so plugging in a headset
  or starting the call on another output is picked up within a few seconds. If it still hears
  nothing, pin the right output in Settings → Audio & speech (*Refresh* after joining a call).
- **Screenshot is greyed out:** install `grim` (Wayland) or `spectacle`, or `maim`/`scrot` on X11,
  and check Settings → Screenshot.
- **A document is refused:** the PDF has no text layer. Export a text PDF or attach DOCX.
- **Speech turns into text slowly:** on a CPU one Whisper pass takes about 0.8 s whatever the length of the
  question, so the delay you feel is the silence wait plus the model. With an NVIDIA GPU the installer adds
  the CUDA 12 libraries (about 1.3 GB; `CAREERTWIN_NO_GPU=1 ./scripts/install.sh` skips them) and Whisper
  runs on the GPU in half precision: on an RTX 4050 a pass dropped from 0.8 s to 0.35 s, and the text of a
  9-second question was ready about 1 s after the speech ended (1.9 s on the CPU). Settings → Audio & speech → *Device*
  shows `auto`, `cpu` or `cuda`; if the GPU fails to start the app falls back to the CPU by itself.
- **Answers are slow:** the model matters most. Settings → AI model → *Load models*; in a test with a
  realistic prompt `groq/openai/gpt-oss-120b` finished in about 1–4 s and `kilo/poolside/laguna-s-2.1:free` showed
  its first words in about 2 s, while `auto` varied from 1 s to 40 s. If the chosen model fails before saying
  anything (rate limit, outage), the request is repeated once on `auto`. Answers also get faster with the
  *Concise* style and a smaller `ABOUT.md`; sections that never help answer (the applications log, CV file
  names, job-ad matching rules, the project catalogue) are already left out of every request.
- **First start is slow:** Whisper downloads its model once and loads it before listening starts;
  the status pill says so.
- **`libcublas` errors:** the app already falls back to CPU. For a GPU, install the CUDA 12 runtime
  libraries.
- **The launcher shows a generic gear icon:** the launcher cached its icon folders before the install
  created `~/.local/share/icons`. The desktop entry uses an absolute icon path to avoid this; if an old
  entry is still shown, restart the launcher or shell (for Noctalia, restart `qs`).
- **Check the setup without the window:** `uv run careertwin-interview --doctor`.
