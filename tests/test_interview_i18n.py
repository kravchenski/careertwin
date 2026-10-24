"""The window and Settings speak every language in locales/, and a missing word falls back to English."""

from __future__ import annotations

import json
import re

import pytest

from careertwin.interview import i18n, languages

CODES = i18n.available()


def english():
    return json.loads((i18n.LOCALES / "en.json").read_text(encoding="utf-8"))


def holes(text: str) -> list[str]:
    return sorted(re.findall(r"\{(\w+)\}", text))


def test_the_window_speaks_these_languages_in_this_order():
    assert CODES == ("en", "ru", "pl", "uk", "de", "fr", "es")
    assert languages.UI_LANGUAGES == CODES
    assert all(i18n.has(code) for code in CODES) and not i18n.has("xx") and not i18n.has(None) and not i18n.has("")


@pytest.mark.parametrize("code", CODES)
def test_a_translation_has_every_key_and_the_same_placeholders_as_english(code):
    base, other = english(), json.loads((i18n.LOCALES / f"{code}.json").read_text(encoding="utf-8"))
    assert set(other) == {"overlay", "dialog"}
    for section in ("overlay", "dialog"):
        assert list(other[section]) == list(base[section]), f"{code}.{section}: keys differ from English"
        for key, value in other[section].items():
            assert isinstance(value, str) and value.strip(), f"{code}.{section}.{key} is empty"
            assert holes(value) == holes(base[section][key]), f"{code}.{section}.{key}: placeholders differ"


@pytest.mark.parametrize("code", CODES)
def test_every_string_can_be_filled_in_and_has_no_stray_braces(code):
    sample = {name: "X" for name in ("ask", "n", "name", "path", "label", "error", "files", "size", "when", "folder")}
    for section, entries in english().items():
        for key in entries:
            filled = i18n.text(code, section, key, **{name: "X" for name in holes(entries[key])}) if holes(entries[key]) else i18n.text(code, section, key)
            assert "{" not in filled and "}" not in filled, f"{code}.{section}.{key}: {filled!r}"
    assert sample  # the placeholder names the strings use are all among the ones above


def test_a_translated_string_is_not_just_the_english_one():
    for code in CODES[1:]:
        other = json.loads((i18n.LOCALES / f"{code}.json").read_text(encoding="utf-8"))
        same = [key for key, value in other["dialog"].items() if value == english()["dialog"][key]]
        assert len(same) < 15, (code, same)  # a few names (Python, Whisper, "Audio") may match, not a copy


def test_a_missing_key_or_language_falls_back_to_english(monkeypatch):
    assert i18n.text("xx", "overlay", "ask") == "Ask" and i18n.text(None, "overlay", "ask") == "Ask"
    assert i18n.text("uk", "overlay", "ask") == "Запитати"
    partial = {"overlay": {"ask": "Demander"}, "dialog": {"title": ""}}
    monkeypatch.setattr(i18n, "catalog", lambda code: partial if code == "fr" else english())
    assert i18n.text("fr", "overlay", "ask") == "Demander"
    assert i18n.text("fr", "overlay", "screenshot") == "Screenshot"  # not translated yet: English, never blank
    assert i18n.text("fr", "dialog", "title") == "Settings"  # an empty string counts as missing
    assert i18n.text("fr", "overlay", "waiting_hint", ask="Ask") == english()["overlay"]["waiting_hint"].format(ask="Ask")


def test_a_damaged_or_missing_locale_file_is_just_empty(tmp_path, monkeypatch):
    (tmp_path / "bad.json").write_text("{not json")
    (tmp_path / "list.json").write_text("[1, 2]")
    monkeypatch.setattr(i18n, "LOCALES", tmp_path)
    i18n.catalog.cache_clear()
    try:
        assert i18n.catalog("bad") == {} and i18n.catalog("list") == {} and i18n.catalog("nope") == {}
    finally:
        i18n.catalog.cache_clear()


def test_every_ui_language_has_a_native_name_for_the_picker():
    for code in CODES:
        assert code in languages.NATIVE, code


def test_a_new_file_is_a_new_language_with_no_code_change(tmp_path, monkeypatch):
    import shutil

    shutil.copytree(i18n.LOCALES, tmp_path / "locales")
    (tmp_path / "locales" / "xx.json").write_text(json.dumps({"overlay": {"ask": "Zzz"}, "dialog": {}}), encoding="utf-8")
    monkeypatch.setattr(i18n, "LOCALES", tmp_path / "locales")
    i18n.catalog.cache_clear()
    try:
        assert i18n.available() == (*CODES, "xx")
        assert i18n.text("xx", "overlay", "ask") == "Zzz" and i18n.text("xx", "dialog", "title") == "Settings"
    finally:
        i18n.catalog.cache_clear()


# ---- in the window ------------------------------------------------------------------------------------


@pytest.fixture
def gui_for(monkeypatch):
    pytest.importorskip("PySide6")
    monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
    from careertwin.interview.overlay import Bridge, OverlayWindow, create_app

    app = create_app([])
    windows = []

    def make(language):
        bridge = Bridge()
        window = OverlayWindow(bridge, on_ask=lambda _: None, on_hint=lambda: None, on_pause=lambda _: None, ui_language=language)
        windows.append(window)
        return window, bridge, app

    yield make
    for window in windows:
        window.close()


def test_the_window_follows_the_interview_into_ukrainian(gui_for):
    window, bridge, app = gui_for("en")
    assert window.ask.title.text() == "Ask"
    bridge.language.emit("uk")
    app.processEvents()
    assert window.ui == "uk" and window.ask.title.text() == "Запитати" and window.shot.title.text() == "Скріншот"
    assert window.input.placeholderText() == "Введіть запитання вручну…"
    bridge.language.emit("de")
    app.processEvents()
    assert window.ui == "de" and window.ask.title.text() == "Fragen"
    bridge.language.emit("tr")  # no translation: English, as before
    app.processEvents()
    assert window.ui == "en" and window.ask.title.text() == "Ask"


@pytest.mark.parametrize("code", CODES)
def test_the_window_builds_and_every_label_is_translated(gui_for, code):
    window, _, app = gui_for(code)
    assert window.ui == code
    for key in english()["overlay"]:
        assert window.tr_(key, ask="Ask") if "{ask}" in english()["overlay"][key] else window.tr_(key)
    window.show()
    app.processEvents()
    assert window.ask.title.text() == i18n.text(code, "overlay", "ask")
    assert window.menu.actions()[0].text() == i18n.text(code, "overlay", "settings")


def test_the_guide_for_translators_names_the_real_files_and_checks():
    from pathlib import Path

    guide = (Path(__file__).resolve().parents[1] / "docs" / "TRANSLATING.md").read_text(encoding="utf-8")
    for needle in ("locales/en.json", "tests/test_interview_i18n.py", "NATIVE", "ORDER", "LEXICON",
                   "render_overlay_preview.py", "render_settings_preview.py"):
        assert needle in guide
    assert (Path(__file__).resolve().parents[1] / "scripts" / "render_overlay_preview.py").is_file()
