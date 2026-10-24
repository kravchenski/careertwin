CareerTwin Interview for Windows (portable, 64-bit)
===================================================

Unzip the folder anywhere you can write to (not "Program Files"), then double-click
"Start CareerTwin Interview.bat". "Create shortcut.bat" puts an icon on the desktop and
"Check setup.bat" prints what works and what does not.

Everything you add (your CV, ABOUT, settings, transcripts) is stored in the "data" folder next
to the program. If the window does not open, read data\last-run.log.

What works in this version
  - The window, all settings, your CV and ABOUT documents, per-interview contexts.
  - Questions you type or paste: the answer streams from your Switchyard gateway
    (Settings -> AI model: URL, key, model).

What does not work on Windows yet
  - Listening to the call (speech recognition): call audio is captured through PulseAudio/PipeWire
    on Linux, so "Listen to the call" is off here and the window says so if you turn it on.
  - The screenshot button.

The program only suggests answers and does not hide itself from screen sharing. Check whether
the interviewer allows AI help, and say so when you are expected to.
