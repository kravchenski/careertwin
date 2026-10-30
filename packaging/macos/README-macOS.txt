CareerTwin Interview for macOS
==============================

Install: drag "CareerTwin Interview" onto the Applications folder.

First start: this app is not signed by Apple, so macOS refuses a double click. Right-click the app,
choose Open, then Open again. (If macOS still says the app is damaged, run once in Terminal:
xattr -dr com.apple.quarantine "/Applications/CareerTwin Interview.app")

What works on a Mac
  - The window, the settings, your documents, the vacancy and the interview library.
  - Questions you type or paste, answered by the provider you pick in Settings -> AI
    (Switchyard, Groq, OpenAI, OpenRouter, Ollama, LM Studio or your own address).
  - Your microphone, heard and turned into text (Settings -> General -> Listening). macOS asks once
    for permission to use the microphone.
  - Defending a project: Settings -> Interview reads a folder and answers questions about it.
  - The Screenshot button (macOS asks once for permission to record the screen; allow it for the
    app in System Settings -> Privacy & Security -> Screen Recording).
  - Looks for a new version once a day and downloads the disk image for you to open.

Not on a Mac yet
  - Listening to the call: macOS has no way to record what the speakers play without an extra
    audio driver, and this version does not use one. Type the question, or use the microphone.
  - Hiding the window from screen sharing: that is a Windows feature.

Your settings and documents are kept in
  ~/Library/Application Support/CareerTwin Interview
Delete that folder to remove them.

This build was made by GitHub Actions and has not been run by its author on a Mac.
