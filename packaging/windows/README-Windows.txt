CareerTwin Interview for Windows (portable, 64-bit)
===================================================

Unzip the folder anywhere you can write to (not "Program Files"), then double-click
"Start CareerTwin Interview.bat". "Create shortcut.bat" puts an icon on the desktop and
"Check setup.bat" prints what works and what does not.

Everything you add (your CV, ABOUT, settings, transcripts) is stored in the "data" folder next
to the program. If the window does not open, read data\last-run.log.

What it does
  - The window, all settings, your CV and ABOUT documents, per-interview contexts.
  - Listens to the call: it records what your default output plays (WASAPI loopback), so Zoom,
    Teams and Meet are heard without a virtual cable, and turns it into text on your computer.
    If the call plays on another output, pick that output in Settings -> Audio -> call audio.
    The first start downloads the speech model (about 480 MB for "small").
  - Speech recognition on your processor (Whisper), or in the cloud with Deepgram
    (Settings -> Audio & speech -> Speech recognition): quicker on a laptop without a graphics card,
    but the voices of the call are sent to Deepgram.
  - Questions you type or paste, answered by the provider you pick in Settings -> AI model
    (Switchyard, Groq, OpenAI, OpenRouter, Ollama, LM Studio or your own address).
  - The Screenshot button copies the primary screen (or all screens, see Settings -> Screenshot)
    and asks about what is on it. The window hides itself for the moment of the capture.

Not on Windows yet
  - The graphics card for speech recognition (it runs on the processor).

This build was checked under Wine only, not on a real Windows machine. Run "Check setup.bat"
first: it opens the call audio for a moment and prints the loudest level it heard, so play
something and run it again if it says 0.

The program only suggests answers and does not hide itself from screen sharing. Check whether
the interviewer allows AI help, and say so when you are expected to.
