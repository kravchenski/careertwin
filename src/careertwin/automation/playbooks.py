"""What the form filler knows about each application system, learned from real applications.

The model drives the browser from what it reads on the page; these notes tell it how the systems behave where a
page alone does not (a drop-down that ignores typing, a resume import that overwrites the fields, the one button
that sends). They are guidance for the model, never permission: the executor's limits apply on every site.
"""

from __future__ import annotations

from urllib.parse import urlsplit

WORKDAY = """Workday application (myworkdayjobs.com). Steps: My Information, My Experience, Application Questions,
Voluntary Disclosures, Review. "Save and Continue" only moves on; only the Submit button on Review sends the application.
- A dialog may offer "Autofill with Resume", "Apply Manually" and "Use My Last Application". Take "Use My Last
  Application" when the candidate is signed in (it keeps the answers of the last application), otherwise "Apply
  Manually". Never take "Autofill with Resume": it overwrites the fields from a parsed CV.
- Values carried over from the last application can be stale: read every page and correct what differs from the facts
  and the saved answers. Replace the attached resume with the one the executor supplies: click the trash button of
  the attached file, then upload_file on the file input. Duplicate work entries carried over are left alone.
- A drop-down is a button: click it, click the option by its visible text, then read the page to see that the button
  shows the value. Typing into it and pressing Enter does not select.
- A hierarchical list (citizenship, phone code, skills) opens a country or item first, then a second list with the
  choices, for example "Citizen (X)" and "Permanent Resident (X)". Tick only what the facts state, and never "Citizen"
  of a country the facts do not name as the candidate's citizenship.
- Work authorization: when the facts give a residence permit, pick the option that names a work authorization or visa,
  never "by virtue of citizenship" unless the facts say the candidate is a citizen of that country; the permit name and
  dates go in the follow-up field exactly as the facts give them.
- Voluntary disclosures (gender, ethnicity, veteran, disability): choose "Decline" or "Prefer not to disclose" when the
  facts do not state the answer.
- The required terms or privacy box follows standing_consent; marketing, job alert and talent community boxes stay
  unticked.
- On Review compare every value with the facts and saved answers. Click Submit only when submission_allowed is true and
  everything matches; otherwise stop with done=true."""

DAYFORCE = """Dayforce candidate portal (dayforcehcm.com). The sign-in page offers "Apply without an Account"; a Privacy
Statement dialog needs its box ticked (standing_consent) and Save before the form opens. Steps: Candidate Info,
Questionnaire, Submit (a candidate acknowledgement box and the Submit button).
- Upload the resume first. The portal reads it and fills the fields, sometimes in capitals or wrongly: read the whole
  page after the upload and correct every field (names, the e-mail twice, country, phone, employment entries).
- Employment entries need an employer name and full dates. The CV usually gives only years: a saved answer about
  partial dates says what to use; without one, ask.
- Each entry has its own Next button that saves it and may also move to the next step: read the saved list afterwards
  and go back with Previous when an entry is missing or wrong.
- The questionnaire answers come from the facts and saved answers only; ask when they do not hold one."""

PLAYBOOKS = (
    (("myworkdayjobs.com", "workday.com"), WORKDAY),
    (("dayforcehcm.com",), DAYFORCE),
)


def playbook(url: str) -> str:
    """The notes for the application system at `url`, or an empty string when there are none."""
    try:
        host = (urlsplit(url).hostname or "").lower().rstrip(".")
    except ValueError:
        return ""
    for suffixes, notes in PLAYBOOKS:
        if any(host == suffix or host.endswith("." + suffix) for suffix in suffixes):
            return notes
    return ""
