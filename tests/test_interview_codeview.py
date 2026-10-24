"""Code in an answer shows as code: coloured, indented, in lists and inside sentences."""

from __future__ import annotations

import os
import re

import pytest

from careertwin.interview.codeview import COLOURS, highlight, language_of
from careertwin.interview.prompts import SYSTEM, sections, split_partial

PYTHON = "@cache\ndef two_sum(nums, target):  # two pointers\n    return \"a < b\" if 3 // 2 else None"


def text_of(html_text: str) -> str:
    """What the eye reads: the HTML without its tags."""
    return re.sub(r"<[^>]+>", "", html_text).replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", '"')


def test_a_fence_tag_or_the_code_itself_tells_the_language():
    assert language_of("python", "") == "python" and language_of("TS", "") == "clike" and language_of("sql", "") == "sql"
    assert language_of("", "def f(x):\n    return x") == "python"
    assert language_of("", "SELECT id FROM books;") == "sql"
    assert language_of("", "public int size() {\n    return n;\n}") == "clike"
    assert language_of("", "$ ls -la") == "shell"
    assert language_of("", "just words") == "plain" and language_of("json", "{}") == "plain"


def test_python_gets_keywords_strings_comments_decorators_and_numbers_coloured():
    out = highlight(PYTHON, "python")
    for kind, word in (("decorator", "@cache"), ("keyword", "def"), ("comment", "# two pointers"),
                       ("string", '&quot;a &lt; b&quot;'), ("number", "3"), ("keyword", "None"), ("keyword", "return")):
        assert f'color:{COLOURS[kind]}' in out and f">{word}</span>" in out, (kind, word)
    assert "font-style:italic" in out
    assert "two_sum(nums, target)" in out  # names and punctuation stay plain


def test_floor_division_and_hash_inside_a_string_are_not_comments_in_python():
    out = highlight('x = 7 // 2\ny = "a # b"', "python")
    assert out.count(f'color:{COLOURS["comment"]}') == 0
    assert ">&quot;a # b&quot;</span>" in out


def test_c_like_languages_colour_slash_comments_and_template_strings():
    out = highlight("const url = `/api/${id}`; // fetch it\n/* block */ function go() { return null; }", "js")
    assert ">// fetch it</span>" in out and ">/* block */</span>" in out
    assert ">`/api/${id}`</span>" in out
    assert f'color:{COLOURS["keyword"]}">const</span>' in out and f'color:{COLOURS["keyword"]}">null</span>' in out


def test_sql_keywords_are_recognised_in_any_case():
    out = highlight("select name from books where price > 10 -- cheap", "sql")
    for word in ("select", "from", "where"):
        assert f'color:{COLOURS["keyword"]}">{word}</span>' in out
    assert ">-- cheap</span>" in out


def test_unknown_text_is_only_escaped_and_cannot_break_out_of_its_block():
    out = highlight("</pre><script>alert(1)</script>", "json")
    assert out == "&lt;/pre&gt;&lt;script&gt;alert(1)&lt;/script&gt;" and "<span" not in out
    assert "<script>" not in highlight("</td><script>x</script>", "python")


def test_an_unfinished_string_or_comment_does_not_hide_the_rest_of_the_code():
    out = highlight('print("unfinished\nx = 1', "python")
    assert "x = " in out and 'color:' in out
    assert highlight("/* open", "js").count("<span") == 1  # a half-streamed block comment is a comment


# ---- in the window ------------------------------------------------------------


def window_functions():
    pytest.importorskip("PySide6")
    os.environ["QT_QPA_PLATFORM"] = "offscreen"
    from careertwin.interview import overlay

    return overlay


def test_code_between_backticks_inside_a_sentence_is_monospace():
    overlay = window_functions()
    out = overlay.render_bold("Call `two_sum(nums, 9)` and **check** the `<result>`")
    assert out.count(overlay.INLINE_CODE) == 2 and "two_sum(nums, 9)</span>" in out
    assert "&lt;result&gt;</span>" in out and '<b style="color:#ffffff">check</b>' in out
    assert overlay.render_bold("Use `open_quote").endswith("open_quote</span>")  # still streaming
    assert overlay.render_bold("a **b** <c> **d") == 'a <b style="color:#ffffff">b</b> &lt;c&gt; d'


def test_a_fenced_block_in_a_list_comes_out_as_a_code_block_between_lists():
    overlay = window_functions()
    items = ["use two pointers", "```python\ndef f(x):\n    return x\n```", "mind the `empty` array"]
    out = overlay.render_items(items, "color:red", 14)
    assert out.count("<ul") == 2 and out.count("<pre") == 1
    assert out.index("use two pointers") < out.index("<pre") < out.index("mind the")
    assert "    return x" in text_of(out)


def test_the_code_block_keeps_indentation_wraps_long_lines_and_is_coloured():
    overlay = window_functions()
    block = overlay.code_block("def f(x):\n    return x  # " + "long " * 40, 14, "python")
    assert "white-space:pre-wrap" in block and "monospace" in block and "    return x" in text_of(block)
    assert f'color:{COLOURS["keyword"]}">def</span>' in block


def test_the_parser_keeps_a_fenced_block_in_one_piece_even_inside_points():
    text = "POINTS:\n- idea one\n```python\ndef f(x):\n    return x\n```\n- idea two\nANSWER:\nDone."
    parts = sections(text)
    assert parts.points[0] == "idea one" and parts.points[2] == "idea two"
    assert parts.points[1] == "```python\ndef f(x):\n    return x\n```"
    assert parts.answer == "Done."
    points, answer = split_partial("POINTS:\n- idea\n```python\ndef f(x):\n    ret")
    assert points == ["idea"] and answer.startswith("```python") and answer.endswith("    ret")  # no ANSWER yet


def test_the_rules_ask_for_fenced_code_with_a_language_and_backticked_names():
    assert "fenced block with its language" in SYSTEM and "single backticks" in SYSTEM
