# Translating the window

The interview window and its Settings speak English, Russian, Polish, Ukrainian, German, French and
Spanish. A language is one JSON file, so adding another needs no change to the code.

## Add a language

1. Copy `src/careertwin/interview/locales/en.json` to `<code>.json`, for example `it.json`. The code is the
   two-letter ISO 639-1 code that Whisper uses.
2. Translate the **values**. Leave the keys, the `{placeholders}`, `ABOUT.md`, `.env`, `data/interviews`,
   `DEEPGRAM_API_KEY`, product names (Whisper, Deepgram, Switchyard, Groq) and key names (`Ctrl`) as they are.
   Keep the order of the keys. `{n}`, `{name}` and the like are filled in by the program: they must appear
   in your string exactly as in English, though they may move.
3. Add the language's own name to `NATIVE` in `src/careertwin/interview/languages.py` if it is not there
   yet (it is shown in the language pickers), and its code to `ORDER` in `i18n.py` if you want it to appear
   before the alphabetical rest.
4. Run the check and look at the result:

   ```bash
   uv run pytest tests/test_interview_i18n.py
   uv run --extra interview python scripts/render_overlay_preview.py /tmp/overlay.png it
   uv run --extra interview python scripts/render_settings_preview.py /tmp/settings.png interview it
   ```

The test fails with the exact key when a translation is missing, has a key too many, is empty, or loses a
placeholder. A key you have not translated yet is not an error in the program: it shows the English text, never
a blank label, so a half-finished translation is safe to ship and to improve later.

## Questions in your language (optional)

The window shows your labels as soon as the file exists. For the program to *hear* a spoken question without
a question mark, add an entry to `LEXICON` in `src/careertwin/interview/questions.py`: the words a question
starts with, the short replies that need no answer, and a few phrases for each kind of question (intro,
salary, motivation, definition, logistics...). `tests/test_interview_languages.py` shows what a good entry
catches. Without an entry the program still answers sentences of five words or more.

## What is not translated

Error messages from the coach and the speech engine (for example "No AI gateway answers at …"), the
documentation, and the prompts that go to the model. The model is told the interview language and writes the
answers in it whatever the language of the window.
