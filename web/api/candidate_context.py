"""Shared candidate context. Studio publishes facts; every agent reads this file."""

from __future__ import annotations

import errno
import json
import os
import re
import tempfile
import threading
from pathlib import Path
from typing import Any

from config import REPO_ROOT

LOCK = threading.RLock()
MARKER = "<!-- CAREERTWIN_STUDIO_STATE -->"


def context_path(username: str = "default") -> Path:
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", username):
        raise ValueError("Invalid username")
    base = Path(os.getenv("CAREERTWIN_ABOUT_PATH", str(REPO_ROOT / "ABOUT.md")))
    return (
        base if username == "default" else base.parent / "data" / "profiles" / username / "ABOUT.md"
    )


def read_context(username: str = "default") -> str:
    with LOCK:
        path = context_path(username)
        return path.read_text(encoding="utf-8") if path.is_file() else ""


def write_context(text: str, username: str = "default") -> None:
    """Replace atomically; bind-mounted single files require an in-place write."""
    path = context_path(username)
    path.parent.mkdir(parents=True, exist_ok=True)
    with LOCK:
        fd, name = tempfile.mkstemp(prefix=".ABOUT-", dir=path.parent)
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                stream.write(text)
                stream.flush()
                os.fsync(stream.fileno())
            try:
                os.replace(name, path)
            except OSError as error:
                if error.errno != errno.EBUSY:
                    raise
                with path.open("w", encoding="utf-8") as stream:
                    stream.write(text)
                    stream.flush()
                    os.fsync(stream.fileno())
        finally:
            Path(name).unlink(missing_ok=True)


def studio_snapshot(context: str) -> dict[str, Any]:
    match = re.search(re.escape(MARKER) + r"\s*```json\s*([\s\S]*?)\n```", context)
    if not match:
        return {}
    try:
        value = json.loads(match[1])
        return value if isinstance(value, dict) else {}
    except ValueError, TypeError:
        return {}


def salary_policy(context: str) -> dict[str, Any]:
    salary = studio_snapshot(context).get("preferences", {}).get("salary", {})
    return salary if salary.get("mode") == "fixed" else {"mode": "auto"}


def text(value: Any) -> str:
    if value is None or value == "" or value == []:
        return "Not provided"
    if isinstance(value, bool):
        return "Yes" if value else "No"
    if isinstance(value, list):
        return ", ".join(text(item) for item in value)
    if isinstance(value, dict):
        return "; ".join(f"{key}: {text(item)}" for key, item in value.items())
    return str(value).replace("\n", " ").replace("|", "\\|").replace("`", "'")


LABELS = {
    "employmentStatus": "Employment status",
    "jobTitles": "Target roles",
    "experienceLevel": "Experience level",
    "industries": "Industries",
    "workTypes": "Employment types",
    "startDate": "Available from",
    "willingToRelocate": "Willing to relocate",
    "workMode": "Work modes",
    "remoteLocations": "Remote countries",
    "onsiteCities": "On-site / hybrid cities",
    "driversLicense": "Driving licence",
    "gender": "Gender",
    "disability": "Disability",
    "veteran": "Veteran status",
    "contactEmail": "Contact email",
    "contactConsentNewsletter": "Newsletter consent",
}


def build_context(profile: dict, preferences: dict, previous: str = "") -> str:
    """Deterministic rendering, without an LLM inventing candidate attributes.

    Preserve verified background sections missing from a partial Studio profile,
    but only when the names match. Studio preferences supersede old job-specific answers.
    """
    old_sections = dict(re.findall(r"^## ([^\n]+)\n([\s\S]*?)(?=^## |\Z)", previous, re.M))
    old_name = re.search(r"^\| Full name \| ([^|]+)\|", previous, re.M)
    name = str(profile.get("fullName") or "Candidate")
    same_person = bool(
        old_name and old_name[1].strip().split(" (")[0].casefold() == name.casefold()
    )
    retained = old_sections.copy() if same_person else {}
    retained.pop("Studio answers", None)
    retained.pop("Salary policy", None)
    contact = {}
    if same_person:
        for key, value in re.findall(
            r"^\| ([^|]+) \| ([^|]+)\|", old_sections.get("Contact", ""), re.M
        ):
            if key.strip() not in {"Field", "---"}:
                contact[key.strip()] = value.strip()
    for label, key in {
        "Full name": "fullName",
        "Title": "title",
        "Email": "email",
        "Phone": "phone",
        "Location": "location",
        "LinkedIn": "linkedin",
        "GitHub": "github",
        "Website": "website",
    }.items():
        if profile.get(key):
            contact[label] = text(profile[key])
    if preferences.get("gender"):
        contact.pop("Prefix / gender", None)
        contact.pop("Gender", None)
    sections = {
        "Contact": "| Field | Value |\n| --- | --- |\n"
        + "\n".join(f"| {key} | {value} |" for key, value in contact.items())
    }
    if profile.get("summary"):
        sections["Summary"] = str(profile["summary"])
    for label, key in [("Skills", "skills"), ("Languages", "languages")]:
        if profile.get(key):
            sections[label] = "\n".join(f"- {text(item)}" for item in profile[key])
    for label, key, title_keys in [
        ("Experience", "experience", ("role", "company")),
        ("Education", "education", ("degree", "field", "school")),
        ("Certifications", "certificates", ("name", "issuer")),
        ("Projects", "projects", ("name",)),
    ]:
        if not profile.get(key):
            continue
        blocks = []
        for item in profile[key]:
            if not isinstance(item, dict):
                blocks.append(f"- {text(item)}")
                continue
            title = ", ".join(text(item[k]) for k in title_keys if item.get(k))
            dates = " to ".join(
                str(v)
                for v in [item.get("start"), "present" if item.get("current") else item.get("end")]
                if v
            )
            heading = f"### {title}" + (f" | {dates}" if dates else "")
            details = [
                f"- {line.lstrip('- ').strip()}"
                for line in str(item.get("description", "")).splitlines()
                if line.strip()
            ]
            details += [
                f"- {text(item[k])}" for k in ("year", "url", "technologies") if item.get(k)
            ]
            blocks.append(heading + "\n\n" + ("\n".join(details) or f"- {title}"))
        sections[label] = "\n\n".join(blocks)
    choice_labels = {
        "unemployed_need": "Unemployed, urgently seeking work",
        "unemployed_caln": "Unemployed, seeking work without urgency",
        "badly_employed": "Employed, seeking a change",
        "employed_open": "Employed, open to opportunities",
        "entry": "Entry-level",
        "first_role": "First full-time role",
        "junior": "Junior / Associate",
        "mid": "Mid-level",
        "senior": "Senior / Lead",
        "full_time": "Full-time",
        "part_time": "Part-time",
        "contract": "Contract / Freelance",
        "prefer_not": "Prefer not to disclose",
        "yes": "Yes",
        "no": "No",
        "onsite": "On-site / hybrid",
        "remote": "Remote",
        "it": "Information Technology",
    }

    def choice(value):
        if isinstance(value, list):
            return ", ".join(choice(item) for item in value) or "Not provided"
        return choice_labels.get(str(value), text(value))

    preference_lines = []
    free_text = {"contactEmail", "startDate", "jobTitles", "remoteLocations", "onsiteCities"}
    for key, label in LABELS.items():
        value = preferences.get(key)
        rendered_value = text(value) if key in free_text else choice(value)
        preference_lines.append(f"- {label}: {rendered_value}")
    sections["Availability and work preferences"] = "\n".join(preference_lines)
    if preferences.get("workAuth"):
        sections["Work authorization"] = "\n".join(
            f"- {text(item)}" for item in preferences["workAuth"]
        )
        if same_person:
            # The authorization dropdown does not collect citizenship or permit dates.
            identity_details = [
                line
                for line in old_sections.get("Work authorization", "").splitlines()
                if line.startswith(("- Citizenship:", "- Residence:"))
            ]
            sections["Work authorization"] += "\n" + "\n".join(identity_details)
    policy = preferences.get("salary", {})
    sections["Salary policy"] = (
        "Mode: AUTO. Choose expectations separately for each vacancy "
        "from its published salary band, "
        "contract, currency and pay period. Never reuse a salary from another application. "
        "Do not convert gross to net or annual to monthly without explicit compatible units. "
        "If no usable budget is published, use negotiable wording where allowed; "
        "ask for an amount if the form requires a number. Salary is not a strict discovery filter."
        if policy.get("mode") != "fixed"
        else (
            f"Mode: FIXED. Expected salary: {text(policy.get('amount'))} "
            f"{text(policy.get('currency'))} / {text(policy.get('period'))}; "
            f"{text(policy.get('basis'))}. Do not silently convert units."
        )
    )
    sections.update({key: value for key, value in retained.items() if key not in sections})
    header = f"# {text(name)}: candidate profile\n\n"
    header += "Shared source of truth for every CareerTwin agent. Unknown answers remain unknown. "
    header += "Use Studio preferences for current requirements; "
    header += "retained application history is historical. "
    header += "Current Studio answers override old form answers. "
    header += "Prefer not to disclose is a disclosure choice, never a No answer. "
    header += "Do not include salary, demographics or consent preferences in a CV.\n"
    rendered = header + "".join(
        f"\n## {key}\n\n{value.strip()}\n" for key, value in sections.items()
    )
    snapshot = {"profile": profile, "preferences": preferences}
    # Escape backticks in JSON to prevent user text closing the fenced snapshot.
    encoded = json.dumps(snapshot, ensure_ascii=False, indent=2).replace("`", "\\u0060")
    return rendered + f"\n## Studio answers\n\n{MARKER}\n```json\n{encoded}\n```\n"
