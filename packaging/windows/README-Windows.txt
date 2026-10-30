CareerTwin Interview for Windows (64-bit)
=========================================

Setup program: run the "-setup.exe". It installs for you only (no administrator rights), adds a Start
menu entry and keeps your settings when you update. Portable zip: unzip the folder anywhere you can
write to (not "Program Files") and double-click careertwin-overlay.exe.

Everything you add (your CV, ABOUT, settings, transcripts) is stored in the "data" folder next to the
program. What the window did about the sound and the screenshot key is written to
data\interview\interview.log: read it if something does not work.

What it does
  - Listens to the call: it records what your default output plays (WASAPI loopback), so Zoom, Teams
    and Meet are heard without a virtual cable, and turns it into text on your computer (Whisper) or
    in the cloud (Deepgram). If the call plays on another output, pick it in Settings -> General ->
    Listening. The first start downloads the speech model once.
  - Answers questions it hears, questions you type, and what is on the screen. Pick the model for each
    in Settings -> AI.
  - A blue arrow in the notification area, like the one Windows shows while a program uses your location:
    an outline while the window waits, solid while it listens or writes an answer. Click it to bring the
    window forward; right-click for the screenshot, listening on or off, and quit. Settings -> General
    turns it off. The arrow follows the window's drawing, so it waits while the window is minimized.
  - Several accounts (Settings -> AI): when one is slow, limited or down the next answers; the next one is
    also started when the first is slow (or all at once, if you choose that), and the first to answer wins.
  - Answers are shown with their Markdown: headings, lists, tables, quotes, code, links.
  - The screenshot key, Ctrl+Shift+Enter, works in any program (the window says which key it got when
    another program already owns that one).
  - Reads a project folder so the answers can defend it (Settings -> Interview).
  - Looks for a new version once a day and downloads the installer for you to run; it never starts it.

Hiding from screen sharing
  The window keeps itself out of screen captures (Zoom, Teams, Meet, OBS and screenshots do not include
  it); you still see it. Turn it off in Settings -> General. Windows older than 10 version 2004 shows a
  black box instead. A physical camera or a photo of the screen still sees the window. Many employers
  forbid AI help in an interview: check the rules, and say so when you are expected to. The window only
  suggests; you decide what to say.

This build was made by GitHub Actions and its parts that do not need a screen were tested there; its
author has not tried it by hand on a Windows computer yet.
