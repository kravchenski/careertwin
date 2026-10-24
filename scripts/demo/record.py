#!/usr/bin/env python3
"""Record docs/media/demo.mp4 and demo.gif from the real code paths.

Real: the API server, the MCP tool catalogue, the agent loop and its tool calls,
voice-activity detection, local Whisper transcription, question detection, the
streaming coach and the Qt overlay. Scripted: the language model, played by
scripts/demo/stub_gateway.py, so every recording is identical. The questions are
synthesized speech (flite) instead of a live call.

Needs ffmpeg and flite. Run from the repository root:

    uv run --extra interview python scripts/demo/record.py
"""

from __future__ import annotations

import os
import pty
import queue
import re
import select
import shutil
import subprocess
import sys
import tempfile
import textwrap
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = Path(tempfile.mkdtemp(prefix="careertwin-demo-"))
API_PORT, GATEWAY_PORT = 8000, 3299
ENV = {
    **os.environ,
    "CAREERTWIN_DATA_DIR": str(WORK / "data"),
    "CAREERTWIN_ABOUT_PATH": str(ROOT / "scripts/demo/ABOUT.demo.md"),
    "CAREERTWIN_API_URL": f"http://127.0.0.1:{API_PORT}",
    "SWITCHYARD_URL": f"http://127.0.0.1:{GATEWAY_PORT}",
    "SWITCHYARD_API_KEY": "",
    "SWITCHYARD_AGENT_MODEL": "demo",
    "SWITCHYARD_MODEL": "demo",
    "INTERVIEW_MODEL": "demo",
    "CAREERTWIN_AGENT_ALLOW_SUBMIT": "",
    "CAREERTWIN_NO_DOTENV": "1",
    "PYTHONUNBUFFERED": "1",
    "QT_QPA_PLATFORM": "offscreen",
    "QT_SCALE_FACTOR": "2",
}
os.environ.update(ENV)

import httpx  # noqa: E402
import numpy as np  # noqa: E402
from PIL import Image, ImageDraw, ImageFont  # noqa: E402

WIDTH, HEIGHT = 1280, 720
BG, FG, DIM = (15, 17, 21), (214, 218, 224), (122, 129, 140)
GREEN, CYAN, RED, YELLOW = (126, 200, 140), (110, 190, 222), (234, 106, 106), (228, 192, 102)
PAD, FONT_SIZE, LINE = 36, 20, 29
MONO = "/usr/share/fonts/TTF/DejaVuSansMono.ttf"
SANS_BOLD = "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf"
SANS = "/usr/share/fonts/TTF/DejaVuSans.ttf"

Segments = list[tuple[str, tuple[int, int, int]]]


class Film:
    """Frames with durations; identical neighbours are merged."""

    def __init__(self) -> None:
        self.frames: list[tuple[Image.Image, float]] = []

    def add(self, image: Image.Image, seconds: float) -> None:
        if self.frames and self.frames[-1][0].tobytes() == image.tobytes():
            self.frames[-1] = (self.frames[-1][0], self.frames[-1][1] + seconds)
        else:
            self.frames.append((image, seconds))

    def write(self, directory: Path) -> Path:
        directory.mkdir(parents=True, exist_ok=True)
        lines = []
        for index, (image, seconds) in enumerate(self.frames):
            path = directory / f"{index:05d}.png"
            image.save(path)
            lines += [f"file '{path}'", f"duration {max(seconds, 0.04):.3f}"]
        lines.append(f"file '{directory / f'{len(self.frames) - 1:05d}.png'}'")
        listing = directory / "frames.txt"
        listing.write_text("\n".join(lines))
        return listing


class Terminal:
    def __init__(
        self, width: int, height: int, font: ImageFont.FreeTypeFont, pad: int = PAD
    ) -> None:
        self.width, self.height, self.font, self.pad = width, height, font, pad
        self.char = font.getlength("M")
        self.cols = int((width - 2 * pad) / self.char)
        self.rows = int((height - 2 * pad) / LINE)
        self.lines: list[Segments] = []

    def write(self, segments: Segments) -> None:
        """Append a line, wrapping at the terminal width."""
        text = "".join(part for part, _ in segments)
        if len(text) <= self.cols:
            self.lines.append(segments)
            return
        colour = segments[-1][1]
        indent = len(text) - len(text.lstrip())
        for chunk in textwrap.wrap(text, self.cols, subsequent_indent=" " * indent):
            self.lines.append([(chunk, colour)])

    def clear(self) -> None:
        self.lines = []

    def render(self, cursor: bool = False) -> Image.Image:
        image = Image.new("RGB", (self.width, self.height), BG)
        draw = ImageDraw.Draw(image)
        visible = self.lines[-self.rows :]
        for row, segments in enumerate(visible):
            x, y = self.pad, self.pad + row * LINE
            for text, colour in segments:
                draw.text((x, y), text, font=self.font, fill=colour)
                x += self.char * len(text)
            if cursor and row == len(visible) - 1:
                draw.rectangle((x, y + 3, x + self.char * 0.6, y + LINE - 5), fill=FG)
        return image


class Scene:
    def __init__(
        self, film: Film, caption: str, terminal: Terminal, overlay=None, overlay_at=(0, 0)
    ) -> None:
        self.film, self.caption, self.term = film, caption, terminal
        self.overlay, self.overlay_at = overlay, overlay_at

    def frame(self, cursor: bool = False) -> Image.Image:
        canvas = Image.new("RGB", (WIDTH, HEIGHT), BG)
        canvas.paste(self.term.render(cursor), (0, 0))
        if self.overlay is not None:
            canvas.paste(self.overlay, self.overlay_at)
        draw = ImageDraw.Draw(canvas)
        draw.text((PAD, HEIGHT - 34), self.caption, font=small_font(), fill=DIM)
        return canvas

    def hold(self, seconds: float) -> None:
        self.film.add(self.frame(), seconds)

    def command(self, command: str) -> None:
        prompt: Segments = [("$ ", GREEN)]
        for end in range(0, len(command) + 1, 2):
            self.term.lines.append([*prompt, (command[:end], FG)])
            self.film.add(self.frame(cursor=True), 0.045)
            self.term.lines.pop()
        self.term.write([*prompt, (command, FG)])
        self.hold(0.5)

    def print(self, lines: list[Segments], per_line: float = 0.06) -> None:
        for segments in lines:
            self.term.write(segments)
            self.film.add(self.frame(), per_line)


_small: ImageFont.FreeTypeFont | None = None


def small_font() -> ImageFont.FreeTypeFont:
    global _small
    _small = _small or ImageFont.truetype(SANS, 16)
    return _small


def colourise(line: str) -> Segments:
    stripped = line.lstrip()
    indent = line[: len(line) - len(stripped)]
    if stripped.startswith("→"):
        return [(indent, FG), (stripped, CYAN)]
    if stripped.startswith("✓"):
        return [(indent, FG), (stripped, GREEN)]
    if stripped.startswith(("✗", "error")):
        return [(indent, FG), (stripped, RED)]
    if line.startswith("INFO"):
        return [("INFO", GREEN), (line[4:], FG)]
    return [(line, FG)]


def title_card(
    film: Film, heading: str, lines: list[str], seconds: float, bright: int = 1
) -> None:
    image = Image.new("RGB", (WIDTH, HEIGHT), BG)
    draw = ImageDraw.Draw(image)
    big, body = ImageFont.truetype(SANS_BOLD, 66), ImageFont.truetype(SANS, 28)
    draw.text((WIDTH / 2, 250), heading, font=big, fill=FG, anchor="mm")
    for index, text in enumerate(lines):
        colour = FG if index < bright else DIM
        draw.text((WIDTH / 2, 340 + index * 44), text, font=body, fill=colour, anchor="mm")
    film.add(image, seconds)


def wait_for(url: str, timeout: float = 30) -> None:
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        try:
            if httpx.get(url, timeout=2).status_code < 500:
                return
        except httpx.HTTPError:
            time.sleep(0.3)
    raise SystemExit(f"{url} did not come up")


def run_in_pty(args: list[str], answers: dict[str, str]) -> list[str]:
    """Run a command on a pseudo-terminal and answer its prompts like a person would."""
    pid, fd = pty.fork()
    if pid == 0:
        os.execvpe(args[0], args, ENV)
    output, answered = b"", set()
    while True:
        ready, _, _ = select.select([fd], [], [], 60)
        if not ready:
            break
        try:
            data = os.read(fd, 4096)
        except OSError:
            break
        if not data:
            break
        output += data
        for prompt, reply in answers.items():
            if prompt.encode() in output and prompt not in answered:
                answered.add(prompt)
                time.sleep(0.2)
                os.write(fd, reply.encode())
    os.waitpid(pid, 0)
    text = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", output.decode(errors="replace"))
    return [line.rstrip() for line in text.replace("\r", "").split("\n") if line.strip()]


def synthesize(text: str) -> np.ndarray:
    wav = WORK / f"{abs(hash(text))}.wav"
    pcm = WORK / f"{abs(hash(text))}.pcm"
    subprocess.run(["flite", "-voice", "slt", "-t", text, "-o", str(wav)], check=True)
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-y", "-i", str(wav), "-ar", "16000", "-ac", "1",
         "-f", "s16le", str(pcm)],
        check=True,
    )
    return np.fromfile(pcm, dtype=np.int16)


class QueueSource:
    """Feeds recorded audio in 20 ms frames, standing in for the call's sink monitor."""

    def __init__(self) -> None:
        self.queue: queue.Queue[np.ndarray | None] = queue.Queue()

    def feed(self, audio: np.ndarray) -> None:
        padded = np.concatenate([np.zeros(16000, np.int16), audio, np.zeros(24000, np.int16)])
        for start in range(0, len(padded) - 319, 320):
            self.queue.put(padded[start : start + 320])

    def frames(self):
        while (frame := self.queue.get()) is not None:
            yield frame

    def close(self) -> None:
        self.queue.put(None)


def record_interview(questions: list[str]) -> tuple[list[dict], list[list[Image.Image]]]:
    """Run the real pipeline for each question; return measurements and overlay frames."""
    from careertwin.interview.brief import InterviewBrief
    from careertwin.interview.coach import StreamingCoach
    from careertwin.interview.overlay import Bridge, OverlayWindow, create_app
    from careertwin.interview.questions import classify
    from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
    from careertwin.interview.stt import WhisperTranscriber
    from careertwin.interview.transcript import TranscriptLog

    app = create_app(sys.argv[:1])
    bridge = Bridge()
    brief = InterviewBrief.load()
    window = OverlayWindow(
        bridge,
        on_ask=lambda _: None,
        on_hint=lambda: None,
        on_pause=lambda _: None,
        on_screenshot=lambda: None,
        on_add_document=lambda _path: brief.sources(),
        sources=brief.sources(),
        ui_language="en",
    )
    window.resize(920, 400)
    window.show()

    events: list[tuple[str, str, float]] = []
    clock = time.monotonic
    callbacks = Callbacks(
        on_turn=lambda s, t: events.append(("turn", t, clock())),
        on_suggestion_start=lambda q: events.append(("start", q, clock())),
        on_suggestion_chunk=lambda c: events.append(("chunk", c, clock())),
        on_suggestion_done=lambda t: events.append(("done", t, clock())),
        on_status=lambda s: events.append(("status", s, clock())),
        on_error=lambda e: events.append(("error", e, clock())),
    )
    session = InterviewSession(
        brief, WhisperTranscriber("small", language="en"), StreamingCoach(brief, model="demo"),
        callbacks, debounce=0.3, log=TranscriptLog(WORK / "interviews"),
    )
    source = QueueSource()
    session.start({INTERVIEWER: source})

    def wait(kind: str, count: int, timeout: float = 240) -> None:
        end = clock() + timeout
        while sum(1 for e in events if e[0] == kind) < count:
            if any(e[0] == "error" for e in events):
                raise SystemExit(f"interview pipeline error: {events[-1][1]}")
            if clock() > end:
                raise SystemExit(f"timed out waiting for {kind}")
            time.sleep(0.05)

    def snapshot(frames: list[Image.Image]) -> None:
        app.processEvents()
        path = WORK / "overlay.png"
        window.grab().save(str(path))
        image = Image.open(path).convert("RGB")
        width = WIDTH - 120
        scaled = image.resize((width, int(image.height * width / image.width)), Image.LANCZOS)
        frames.append(scaled)

    wait("status", 2)
    stats: list[dict] = []
    frames_by_question: list[list[Image.Image]] = []
    bridge.state.emit("listening")
    for number, question in enumerate(questions, start=1):
        mark = len(events)
        source.feed(synthesize(question))
        wait("done", number)
        mine = events[mark:]
        heard = next(e for e in mine if e[0] == "turn")
        first = next(e for e in mine if e[0] == "chunk")
        stats.append(
            {
                "heard": heard[1],
                "kind": classify(heard[1]).value,
                "seconds": first[2] - heard[2],
            }
        )
        frames: list[Image.Image] = []
        bridge.level.emit(INTERVIEWER, 0.7)
        snapshot(frames)
        bridge.turn.emit(INTERVIEWER, heard[1])
        snapshot(frames)
        chunks = 0
        for kind, payload, _ in mine:
            if kind == "start":
                bridge.state.emit("thinking")
                bridge.started.emit(payload)
            elif kind == "chunk":
                bridge.chunk.emit(payload)
                chunks += 1
                if chunks % 3 == 0:
                    snapshot(frames)
            elif kind == "done":
                bridge.state.emit("listening")
                bridge.level.emit(INTERVIEWER, 0.4)
                bridge.done.emit(payload)
                snapshot(frames)
        frames_by_question.append(frames)
    session.stop()
    return stats, frames_by_question


def make_demo_cv() -> Path:
    """A one-page CV for the fictional persona, so the overlay shows an attached CV."""
    from weasyprint import HTML

    persona = (ROOT / "scripts/demo/ABOUT.demo.md").read_text()
    body = re.sub(r"^#+ ", "", persona, flags=re.M).replace("\n", "<br>")
    path = WORK / "Alex_Demo_CV.pdf"
    HTML(string=f"<body style='font-family:sans-serif;font-size:11pt'>{body}</body>").write_pdf(path)
    return path


def main() -> None:
    for tool in ("ffmpeg", "flite"):
        if not shutil.which(tool):
            raise SystemExit(f"{tool} is required")
    for port in (API_PORT, GATEWAY_PORT):
        with __import__("socket").socket() as probe:
            if probe.connect_ex(("127.0.0.1", port)) == 0:
                raise SystemExit(f"port {port} is busy; stop what runs there and retry")

    os.environ["CAREERTWIN_RESUME"] = str(make_demo_cv())
    python = sys.executable
    gateway = subprocess.Popen(
        [python, str(ROOT / "scripts/demo/stub_gateway.py"), "--port", str(GATEWAY_PORT)], env=ENV
    )
    log_path = WORK / "api.log"
    api = subprocess.Popen(
        [python, "-m", "careertwin.cli", "serve", "--port", str(API_PORT)],
        env=ENV, stdout=open(log_path, "w"), stderr=subprocess.STDOUT, cwd=ROOT,
    )
    try:
        wait_for(f"http://127.0.0.1:{GATEWAY_PORT}/v1/models")
        wait_for(f"http://127.0.0.1:{API_PORT}/v1/health")
        film = build_film(log_path)
    finally:
        api.terminate()
        gateway.terminate()
        api.wait(timeout=10)
        gateway.wait(timeout=10)
    export(film)


def build_film(log_path: Path) -> Film:
    film = Film()
    mono = ImageFont.truetype(MONO, FONT_SIZE)
    full = Terminal(WIDTH, HEIGHT - 30, mono)

    title_card(
        film,
        "CareerTwin",
        [
            "Agents run your job search through one API.",
            "Switchyard supplies the model. No web UI.",
            "Recorded with a scripted model; the API, tools and pipeline are real.",
        ],
        4.0,
    )

    scene = Scene(film, "1 · Start the API", full)
    scene.command("uv run careertwin serve")
    log = [line for line in log_path.read_text().splitlines() if line.startswith("INFO")][:4]
    scene.print([colourise(line) for line in log], 0.25)
    scene.command("curl -s localhost:8000/v1/health")
    health = httpx.get(f"http://127.0.0.1:{API_PORT}/v1/health").text
    scene.print([[(health, FG)]])
    scene.hold(1.8)

    full.clear()
    scene = Scene(film, "2 · Every capability is a typed tool, served over MCP", full)
    scene.command("uv run careertwin-mcp --list")
    listing = subprocess.run(
        [str(ROOT / ".venv/bin/careertwin-mcp"), "--list"], env=ENV, capture_output=True, text=True
    ).stdout.splitlines()
    colours = {"read": DIM, "local": YELLOW, "employer": RED}
    rows: list[Segments] = []
    for line in listing:
        name, kind, summary = line[:28], line[28:38].strip(), line[38:]
        rows.append([(name, FG), (f"{kind:<10}", colours[kind]), (summary, DIM)])
    scene.print(rows, 0.07)
    scene.hold(3.0)

    full.clear()
    scene = Scene(film, "3 · An agent works through the tools on a Switchyard gateway", full)
    task = "Import https://example.com/jobs/python-developer and apply for me"
    scene.command(f'uv run careertwin agent "{task}"')
    transcript = run_in_pty(
        [str(ROOT / ".venv/bin/careertwin"), "agent", task], {"[y/N]": "n\n"}
    )
    for line in transcript:
        scene.term.write(colourise(line))
        film.add(scene.frame(), 0.9 if line.lstrip().startswith("→") else 0.25)
    scene.hold(4.0)

    stats, overlays = record_interview(
        [
            "Tell me about a time you fixed a production problem.",
            "Do you have experience with Kubernetes?",
        ]
    )
    top = Terminal(WIDTH, 200, mono, pad=22)
    scene = Scene(
        film, "4 · Interview assistant: local Whisper, your ABOUT.md and CV, answers in the interview language",
        top, overlay_at=(60, 178),
    )
    scene.command("uv run careertwin-interview --job job.txt --resume Alex_Demo_CV.pdf")
    scene.print([[("# demo input: two recorded questions instead of a live call", DIM)]], 0.1)
    scene.overlay = overlays[0][0]
    scene.hold(1.0)
    for item, frames in zip(stats, overlays):
        scene.overlay = frames[1]
        scene.print(
            [[("heard  ", DIM), (f'"{item["heard"]}"  ', FG), (f'→ {item["kind"]} question', DIM)]], 0.5
        )
        for frame in frames[2:]:
            scene.overlay = frame
            film.add(scene.frame(), 0.14)
        scene.hold(3.5)
        top.clear()
        top.write([("$ ", GREEN), ("uv run careertwin-interview --job job.txt --resume Alex_Demo_CV.pdf", FG)])
    scene.hold(1.0)

    title_card(
        film,
        "Try it",
        [
            "uv run careertwin serve",
            "uv run careertwin agent",
            "uv run --extra interview careertwin-interview --doctor",
            "README.md · docs/AGENTS.md · docs/INTERVIEW.md",
        ],
        4.5,
        bright=3,
    )
    return film


def export(film: Film) -> None:
    out = ROOT / "docs" / "media"
    out.mkdir(parents=True, exist_ok=True)
    listing = film.write(WORK / "frames")
    base = ["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", str(listing)]
    subprocess.run(
        [*base, "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-crf", "21",
         "-movflags", "+faststart", str(out / "demo.mp4")],
        check=True,
    )
    subprocess.run(
        [*base, "-vf",
         "fps=10,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=48:stats_mode=diff[p];"
         "[b][p]paletteuse=dither=none:diff_mode=rectangle",
         str(out / "demo.gif")],
        check=True,
    )
    seconds = sum(duration for _, duration in film.frames)
    print(f"{seconds:.0f} s, {len(film.frames)} frames")
    for name in ("demo.mp4", "demo.gif"):
        print(f"{name}: {(out / name).stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    try:
        main()
    finally:
        shutil.rmtree(WORK, ignore_errors=True)
