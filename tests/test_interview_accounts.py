"""Several API keys for one address: the list, the saved file, and a key that is tried after a limited one."""

from __future__ import annotations

import json
import threading

import httpx
import pytest

from careertwin.interview import accounts
from careertwin.interview import settings as settings_module
from careertwin.interview.brief import InterviewBrief
from careertwin.interview.coach import CoachError, StreamingCoach
from careertwin.interview.settings import InterviewSettings

GROQ = "https://api.groq.com/openai"


# ---- the list --------------------------------------------------------------


def test_names_follow_default_default_2_default_3_for_each_address():
    items: list[dict[str, str]] = []
    for key in ("k1", "k2", "k3"):
        items = accounts.add(items, GROQ, key)
    items = accounts.add(items, "https://openrouter.ai/api", "o1")
    assert [(a["name"], a["key"]) for a in items] == [("default", "k1"), ("default-2", "k2"), ("default-3", "k3"), ("default", "o1")]
    items = accounts.remove(items, GROQ, "default-2")
    assert accounts.next_name(items, GROQ) == "default-2"  # a freed name is used again


def test_the_same_key_twice_for_one_address_is_refused_but_fine_for_another():
    items = accounts.add([], GROQ, "same")
    with pytest.raises(accounts.AlreadySaved):
        accounts.add(items, GROQ + "/v1/", "same")  # the form people paste is the same address
    assert len(accounts.add(items, "https://api.openai.com", "same")) == 2
    with pytest.raises(ValueError, match="Paste"):
        accounts.add(items, GROQ, "   ")


def test_accounts_are_found_by_address_however_it_was_written():
    items = accounts.add([], GROQ + "/v1", "k1")
    assert [a.key for a in accounts.for_url(items, "HTTPS://API.GROQ.COM/openai/")] == ["k1"]
    assert accounts.for_url(items, "https://api.openai.com") == []


def test_the_end_of_a_key_is_shown_and_never_the_whole_key():
    long = accounts.Account("default", GROQ, "gsk_abcdefghijklmnopqrstuv")
    assert long.tail() == "…qrstuv" and "gsk_" not in long.tail()
    assert accounts.Account("default", GROQ, "secret").tail() == "…cret"


def test_a_saved_list_is_checked_and_tidied():
    tidy = accounts.clean([
        {"name": " work ", "url": GROQ + "/", "key": " k1 "},
        {"name": "work", "url": GROQ, "key": "k2"},  # the name is taken for this address
        {"name": "again", "url": GROQ, "key": "k1"},  # the same key: kept once
    ])
    assert tidy == [{"name": "work", "url": GROQ, "key": "k1"}, {"name": "default", "url": GROQ, "key": "k2"}]
    for bad in ("not a list", [{"name": "x", "url": GROQ}], [{"name": "x", "url": "ftp://h", "key": "k"}], [{"name": "", "url": GROQ, "key": "k"}], [1]):
        with pytest.raises(ValueError):
            accounts.clean(bad)
    with pytest.raises(ValueError, match="at most"):
        accounts.clean([{"name": f"a{n}", "url": GROQ, "key": f"k{n}"} for n in range(accounts.MAX_ACCOUNTS + 1)])


def test_the_spare_keys_are_the_others_of_the_same_address_in_order():
    items = accounts.add(accounts.add(accounts.add([], GROQ, "k1"), GROQ, "k2"), "https://api.openai.com", "o1")
    assert accounts.spares(items, GROQ, "k1") == ["k2"]
    assert accounts.spares(items, GROQ, "k2") == ["k1"]
    assert accounts.spares(items, GROQ, "") == ["k1", "k2"]


# ---- the settings file -----------------------------------------------------


@pytest.fixture
def data(tmp_path, monkeypatch):
    monkeypatch.setattr(settings_module, "DATA_DIR", tmp_path / "data")
    return tmp_path / "data" / "interview" / "settings.json"


def test_accounts_are_saved_in_the_private_settings_file_and_read_back(data):
    settings = InterviewSettings()
    settings.set("accounts", accounts.add([], GROQ, "gsk_secret_one"))
    assert oct(data.stat().st_mode & 0o777) == "0o600"  # the keys are in it
    assert InterviewSettings.load().accounts == [{"name": "default", "url": GROQ, "key": "gsk_secret_one"}]


def test_a_bad_value_is_refused_and_a_bad_file_is_ignored(data):
    settings = InterviewSettings()
    with pytest.raises(ValueError):
        settings.set("accounts", "gsk_secret")
    data.parent.mkdir(parents=True)
    data.write_text(json.dumps({"accounts": [{"name": "x"}], "model": "m"}))
    loaded = InterviewSettings.load()
    assert loaded.accounts == [] and loaded.model == "m"


def test_a_key_saved_before_accounts_existed_becomes_the_first_account_of_its_address(data):
    data.parent.mkdir(parents=True)
    data.write_text(json.dumps({"gateway_url": GROQ + "/v1", "gateway_key": "old-key"}))
    loaded = InterviewSettings.load()
    assert loaded.gateway_key == "old-key"
    assert loaded.accounts == [{"name": "default", "url": GROQ, "key": "old-key"}]
    assert InterviewSettings.load().accounts == loaded.accounts  # reading changes nothing on disk, and is stable


def test_a_key_without_an_address_belongs_to_the_default_gateway(data, monkeypatch):
    monkeypatch.delenv("SWITCHYARD_URL", raising=False)
    data.parent.mkdir(parents=True)
    data.write_text(json.dumps({"gateway_key": "gateway-key"}))
    assert InterviewSettings.load().accounts == [{"name": "default", "url": "http://127.0.0.1:3260", "key": "gateway-key"}]


# ---- a limited key is followed by the next one ------------------------------


def sse(*parts):
    lines = "".join(f"data: {json.dumps({'choices': [{'delta': {'content': part}}]})}\n\n" for part in parts)
    return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=lines + "data: [DONE]\n\n")


def coach(handler, **options):
    brief = InterviewBrief(candidate="Alex, a data engineer")
    return StreamingCoach(brief, transport=httpx.MockTransport(handler), url=GROQ, **options)


def ask(stream_coach):
    return "".join(stream_coach.stream("What is a left join?", [], threading.Event(), "en"))


def test_a_rate_limited_key_is_followed_by_the_next_key_on_the_same_model():
    log = []

    def handler(request):
        model, key = json.loads(request.content)["model"], request.headers["authorization"].split()[-1]
        log.append((model, key))
        return httpx.Response(429, text="slow down") if key == "k1" else sse("from ", key)

    stream_coach = coach(handler, model="groq/big", api_key="k1", spare_keys=["k2"], fallback_models=["backup"])
    assert ask(stream_coach) == "from k2"
    assert log == [("groq/big", "k1"), ("groq/big", "k2")]  # the chosen model, not the fallback, answered
    assert stream_coach.fell_back is None


def test_a_rejected_key_rests_and_is_not_asked_again_while_another_works():
    log = []

    def handler(request):
        key = request.headers["authorization"].split()[-1]
        log.append(key)
        return httpx.Response(401, text="invalid api key") if key == "bad" else sse("ok")

    stream_coach = coach(handler, model="m", api_key="bad", spare_keys=["good"])
    assert ask(stream_coach) == "ok" and ask(stream_coach) == "ok"
    assert log == ["bad", "good", "good"]  # the second question went straight to the key that works


def test_a_model_problem_goes_to_the_next_model_without_trying_the_other_keys():
    log = []

    def handler(request):
        model, key = json.loads(request.content)["model"], request.headers["authorization"].split()[-1]
        log.append((model, key))
        return httpx.Response(404, text="no such model") if model == "gone" else sse("ok")

    stream_coach = coach(handler, model="gone", api_key="k1", spare_keys=["k2"], fallback_models=["backup"])
    assert ask(stream_coach) == "ok"
    assert log == [("gone", "k1"), ("backup", "k1")]


def test_every_key_limited_goes_on_to_the_next_model_and_reports_the_first_error_when_all_fail():
    log = []

    def handler(request):
        model, key = json.loads(request.content)["model"], request.headers["authorization"].split()[-1]
        log.append((model, key))
        return httpx.Response(429, text="limited") if model == "a" else sse("from b")

    assert ask(coach(handler, model="a", api_key="k1", spare_keys=["k2"], fallback_models=["b"])) == "from b"
    assert log == [("a", "k1"), ("a", "k2"), ("b", "k1")]

    def never(request):
        return httpx.Response(429, text="limited")

    with pytest.raises(CoachError, match="429") as error:
        ask(coach(never, model="a", api_key="k1", spare_keys=["k2"]))
    assert error.value.status == 429


def test_a_cooldown_belongs_to_one_key_so_the_other_key_is_still_asked():
    log = []

    def handler(request):
        key = request.headers["authorization"].split()[-1]
        log.append(key)
        return httpx.Response(429, text="next one in 30s") if key == "k1" else sse("ok")

    stream_coach = coach(handler, model="m", api_key="k1", spare_keys=["k2"])
    ask(stream_coach)
    ask(stream_coach)
    assert log == ["k1", "k2", "k2"]  # k1 was told to wait; k2 was not


def test_one_key_behaves_as_before():
    log = []

    def handler(request):
        log.append(request.headers["authorization"])
        return httpx.Response(401, text="invalid")

    with pytest.raises(CoachError, match="401"):
        ask(coach(handler, model="m", api_key="only"))
    assert log == ["Bearer only"]


def test_a_blocking_completion_tries_the_next_key_too():
    log = []

    def handler(request):
        key = request.headers["authorization"].split()[-1]
        log.append(key)
        if key == "k1":
            return httpx.Response(429, text="limited")
        return httpx.Response(200, json={"choices": [{"message": {"content": "an answer"}}]})

    stream_coach = coach(handler, model="m", api_key="k1", spare_keys=["k2"])
    assert stream_coach.complete("system", "prompt") == "an answer"
    assert log == ["k1", "k2"]
    log.clear()
    gone = coach(lambda request: httpx.Response(404, text="no such model"), model="m", api_key="k1", spare_keys=["k2"])
    with pytest.raises(CoachError, match="404"):
        gone.complete("s", "p")
