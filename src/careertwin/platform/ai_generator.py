"""AI Resume & Cover Letter generator — uses Ollama for text generation."""

from __future__ import annotations

import json
from dataclasses import dataclass, field


@dataclass
class UserProfile:
    full_name: str = ""
    email: str = ""
    phone: str = ""
    location: str = ""
    linkedin: str = ""
    github: str = ""
    website: str = ""
    summary: str = ""
    experience: list[dict] = field(default_factory=list)
    education: list[dict] = field(default_factory=list)
    skills: list[str] = field(default_factory=list)


@dataclass
class GeneratedResume:
    summary: str = ""
    experience: list[dict] = field(default_factory=list)
    skills: list[str] = field(default_factory=list)
    ats_score: int = 0


@dataclass
class GeneratedCoverLetter:
    greeting: str = ""
    opening: str = ""
    body: str = ""
    closing: str = ""
    full_text: str = ""


class AIResumeGenerator:
    """Generates tailored resumes and cover letters using Ollama."""

    def __init__(self, model: str = "qwen2.5:3b", base_url: str = "http://localhost:11434"):
        self.model = model
        self.base_url = base_url

    def generate_resume(
        self,
        profile: UserProfile,
        job_description: str,
        job_title: str = "",
        company: str = "",
    ) -> GeneratedResume:
        """Generate a tailored resume for a specific job."""
        prompt = self._build_resume_prompt(profile, job_description, job_title, company)
        response = self._call_ollama(prompt)
        return self._parse_resume_response(response, profile)

    def generate_cover_letter(
        self,
        profile: UserProfile,
        job_description: str,
        job_title: str = "",
        company: str = "",
    ) -> GeneratedCoverLetter:
        """Generate a tailored cover letter."""
        prompt = self._build_cover_letter_prompt(profile, job_description, job_title, company)
        response = self._call_ollama(prompt)
        return self._parse_cover_letter_response(response, profile, company)

    def _call_ollama(self, prompt: str) -> str:
        """Call Ollama API for text generation."""
        import httpx

        try:
            response = httpx.post(
                f"{self.base_url}/api/generate",
                json={
                    "model": self.model,
                    "prompt": prompt,
                    "stream": False,
                    "options": {
                        "temperature": 0.7,
                        "top_p": 0.9,
                        "num_predict": 2000,
                    },
                },
                timeout=120.0,
            )
            response.raise_for_status()
            return response.json().get("response", "")
        except Exception as e:
            return f"Error: {e}"

    def _build_resume_prompt(
        self,
        profile: UserProfile,
        job_description: str,
        job_title: str,
        company: str,
    ) -> str:
        return f"""You are a professional resume writer. Generate a tailored resume for this job application.

CANDIDATE PROFILE:
Name: {profile.full_name}
Email: {profile.email}
Phone: {profile.phone}
Location: {profile.location}
LinkedIn: {profile.linkedin}
GitHub: {profile.github}

Summary: {profile.summary}

Experience:
{json.dumps(profile.experience, indent=2)}

Skills: {', '.join(profile.skills)}

TARGET JOB:
Company: {company}
Title: {job_title}
Description: {job_description[:2000]}

Generate a JSON response with:
1. "summary": A 2-3 sentence professional summary tailored to this job
2. "experience": Updated experience entries with bullet points relevant to this job
3. "skills": Top 10 most relevant skills for this job
4. "ats_score": Estimated ATS compatibility score (0-100)

Return ONLY valid JSON, no markdown."""

    def _build_cover_letter_prompt(
        self,
        profile: UserProfile,
        job_description: str,
        job_title: str,
        company: str,
    ) -> str:
        return f"""You are a professional cover letter writer. Generate a cover letter for this job application.

CANDIDATE: {profile.full_name}
EMAIL: {profile.email}

EXPERIENCE SUMMARY:
{profile.summary}

RELEVANT SKILLS: {', '.join(profile.skills[:5])}

TARGET JOB:
Company: {company}
Title: {job_title}
Description: {job_description[:1500]}

Generate a JSON response with:
1. "greeting": Professional greeting (e.g., "Dear Hiring Manager,")
2. "opening": Opening paragraph (2-3 sentences showing enthusiasm and fit)
3. "body": Body paragraph (3-4 sentences highlighting relevant experience)
4. "closing": Closing paragraph (2 sentences with call to action)

The tone should be professional but personable. Focus on specific skills that match the job.
Return ONLY valid JSON, no markdown."""

    def _parse_resume_response(self, response: str, profile: UserProfile) -> GeneratedResume:
        """Parse AI response into GeneratedResume."""
        try:
            start = response.find("{")
            end = response.rfind("}") + 1
            if start >= 0 and end > start:
                data = json.loads(response[start:end])
                return GeneratedResume(
                    summary=data.get("summary", profile.summary),
                    experience=data.get("experience", profile.experience),
                    skills=data.get("skills", profile.skills),
                    ats_score=data.get("ats_score", 75),
                )
        except (json.JSONDecodeError, KeyError):
            pass

        return GeneratedResume(
            summary=profile.summary,
            experience=profile.experience,
            skills=profile.skills,
            ats_score=70,
        )

    def _parse_cover_letter_response(
        self, response: str, profile: UserProfile, company: str
    ) -> GeneratedCoverLetter:
        """Parse AI response into GeneratedCoverLetter."""
        try:
            start = response.find("{")
            end = response.rfind("}") + 1
            if start >= 0 and end > start:
                data = json.loads(response[start:end])
                greeting = data.get("greeting", f"Dear Hiring Manager at {company},")
                opening = data.get("opening", "")
                body = data.get("body", "")
                closing = data.get("closing", "")

                full_text = f"{greeting}\n\n{opening}\n\n{body}\n\n{closing}\n\nSincerely,\n{profile.full_name}"

                return GeneratedCoverLetter(
                    greeting=greeting,
                    opening=opening,
                    body=body,
                    closing=closing,
                    full_text=full_text,
                )
        except (json.JSONDecodeError, KeyError):
            pass

        return GeneratedCoverLetter(
            greeting=f"Dear Hiring Manager at {company},",
            opening=f"I am excited to apply for this position at {company}.",
            body="My experience and skills align well with the requirements of this role.",
            closing="I look forward to discussing how I can contribute to your team.",
            full_text=f"Dear Hiring Manager at {company},\n\nI am excited to apply for this position at {company}. My experience and skills align well with the requirements of this role.\n\nI look forward to discussing how I can contribute to your team.\n\nSincerely,\n{profile.full_name}",
        )
