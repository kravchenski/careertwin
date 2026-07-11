"""Vision QA agent — checks resume layout and visual quality using vision models."""

from __future__ import annotations

import base64
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class QAIssue:
    category: str
    severity: str
    message: str
    suggestion: str = ""


@dataclass
class QAReport:
    score: float
    issues: list[QAIssue] = field(default_factory=list)
    summary: str = ""
    provider: str = ""

    def to_text(self) -> str:
        lines = [f"Visual QA Score: {self.score}/100"]
        if self.provider:
            lines.append(f"Provider: {self.provider}")
        if self.summary:
            lines.append(self.summary)
        for issue in self.issues:
            icon = {"critical": "!!!", "warning": "!", "info": "."}.get(issue.severity, "?")
            lines.append(f"  [{icon}] [{issue.category}] {issue.message}")
            if issue.suggestion:
                lines.append(f"      -> {issue.suggestion}")
        return "\n".join(lines)


class VisionQAAgent:
    """Checks resume visual quality using vision models (GPT-4V, Qwen-VL, or LLaVA)."""

    QA_PROMPT = """Analyze this resume image and provide a quality assessment.

Check for these issues and return a JSON array of issues found:
- layout: alignment, overflow, cut-off text, overlapping elements
- typography: font consistency, readability, size hierarchy
- spacing: consistent margins, paragraph spacing, section spacing
- content: completeness, grammar, placeholder text
- ats: machine-readability, standard section names

For each issue, provide:
- category: layout|typography|spacing|content|ats
- severity: critical|warning|info
- message: description of the issue
- suggestion: how to fix it

Also provide an overall score (0-100) and a brief summary.

Return JSON: {"score": number, "summary": "...", "issues": [...]}
"""

    def __init__(self, provider: str = "openai", model: str = "gpt-4o"):
        self.provider = provider
        self.model = model

    def check_pdf(self, pdf_path: str) -> QAReport:
        """Check a PDF file by converting first page to image and analyzing."""
        image_bytes = self._pdf_to_image(pdf_path)
        if image_bytes:
            return self._analyze_image(image_bytes)
        return self._rule_based_check(pdf_path)

    def check_image(self, image_path: str) -> QAReport:
        """Check an image file directly."""
        image_bytes = Path(image_path).read_bytes()
        return self._analyze_image(image_bytes)

    def check_html(self, html_path: str) -> QAReport:
        """Check HTML content for common issues."""
        html = Path(html_path).read_text()
        return self._check_html_content(html)

    def _analyze_image(self, image_bytes: bytes) -> QAReport:
        """Send image to vision model for analysis with fallback chain."""
        providers = [
            ("openai", self._call_openai_vision),
            ("qwen", self._call_qwen_vision),
            ("llava", self._call_llava_vision),
        ]

        for provider_name, call_fn in providers:
            if self.provider in ("auto", provider_name):
                try:
                    report = call_fn(image_bytes)
                    report.provider = provider_name
                    return report
                except Exception:
                    continue

        return QAReport(
            score=50.0,
            summary="No vision model available, using rule-based analysis",
            provider="rule-based",
        )

    def _call_openai_vision(self, image_bytes: bytes) -> QAReport:
        """Call OpenAI GPT-4V or compatible vision API."""
        import httpx

        api_key = self._get_api_key("OPENAI_API_KEY")
        if not api_key:
            raise ValueError("No OpenAI API key configured")

        b64_image = base64.b64encode(image_bytes).decode()

        response = httpx.post(
            "https://api.openai.com/v1/chat/completions",
            headers={"Authorization": f"Bearer {api_key}"},
            json={
                "model": self.model,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {"type": "text", "text": self.QA_PROMPT},
                            {
                                "type": "image_url",
                                "image_url": {
                                    "url": f"data:image/png;base64,{b64_image}",
                                    "detail": "high",
                                },
                            },
                        ],
                    }
                ],
                "max_tokens": 1000,
            },
            timeout=30.0,
        )
        response.raise_for_status()
        data = response.json()
        content = data["choices"][0]["message"]["content"]
        return self._parse_qa_response(content)

    def _call_qwen_vision(self, image_bytes: bytes) -> QAReport:
        """Call Qwen-VL API (Alibaba Cloud DashScope or local)."""
        import httpx

        api_key = self._get_api_key("DASHSCOPE_API_KEY")
        if not api_key:
            raise ValueError("No DashScope API key configured")

        b64_image = base64.b64encode(image_bytes).decode()

        response = httpx.post(
            "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
            },
            json={
                "model": "qwen-vl-max",
                "input": {
                    "messages": [
                        {
                            "role": "user",
                            "content": [
                                {"image": f"data:image/png;base64,{b64_image}"},
                                {"text": self.QA_PROMPT},
                            ],
                        }
                    ]
                },
                "parameters": {"max_tokens": 1000},
            },
            timeout=30.0,
        )
        response.raise_for_status()
        data = response.json()
        content = data["output"]["choices"][0]["message"]["content"][0]["text"]
        return self._parse_qa_response(content)

    def _call_llava_vision(self, image_bytes: bytes) -> QAReport:
        """Call LLaVA model (local Ollama or compatible endpoint)."""
        import httpx

        base_url = self._get_env("LLAVA_BASE_URL", "http://localhost:11434")
        b64_image = base64.b64encode(image_bytes).decode()

        response = httpx.post(
            f"{base_url}/api/generate",
            json={
                "model": "llava",
                "prompt": self.QA_PROMPT,
                "images": [b64_image],
                "stream": False,
            },
            timeout=60.0,
        )
        response.raise_for_status()
        data = response.json()
        content = data.get("response", "")
        return self._parse_qa_response(content)

    def _get_api_key(self, env_var: str) -> str | None:
        import os
        return os.environ.get(env_var)

    def _get_env(self, env_var: str, default: str) -> str:
        import os
        return os.environ.get(env_var, default)

    def _parse_qa_response(self, content: str) -> QAReport:
        """Parse JSON response from vision model."""
        import json

        try:
            start = content.find("{")
            end = content.rfind("}") + 1
            if start >= 0 and end > start:
                data = json.loads(content[start:end])
                issues = [
                    QAIssue(
                        category=i.get("category", "unknown"),
                        severity=i.get("severity", "info"),
                        message=i.get("message", ""),
                        suggestion=i.get("suggestion", ""),
                    )
                    for i in data.get("issues", [])
                ]
                return QAReport(
                    score=data.get("score", 50.0),
                    issues=issues,
                    summary=data.get("summary", ""),
                )
        except (json.JSONDecodeError, KeyError):
            pass
        return QAReport(score=50.0, summary="Could not parse vision model response")

    def _pdf_to_image(self, pdf_path: str) -> bytes | None:
        """Convert first page of PDF to PNG image."""
        try:
            from pdf2image import convert_from_path
            images = convert_from_path(pdf_path, first_page=1, last_page=1, dpi=150)
            if images:
                import io
                buf = io.BytesIO()
                images[0].save(buf, format="PNG")
                return buf.getvalue()
        except Exception:
            pass
        return None

    def _rule_based_check(self, pdf_path: str) -> QAReport:
        """Fallback rule-based QA when no vision model is available."""
        issues: list[QAIssue] = []

        try:
            from pypdf import PdfReader
            reader = PdfReader(pdf_path)
            page = reader.pages[0]
            text = page.extract_text() or ""

            if len(text) < 100:
                issues.append(QAIssue(
                    category="content",
                    severity="warning",
                    message="Very little text extracted — may have rendering issues",
                ))

            if len(text) > 3000:
                issues.append(QAIssue(
                    category="layout",
                    severity="info",
                    message="Dense content — consider reducing text for better readability",
                ))

            words = text.split()
            if words and len(words[0]) > 20:
                issues.append(QAIssue(
                    category="layout",
                    severity="warning",
                    message="Possible text merging — check for spacing issues",
                ))

        except Exception:
            issues.append(QAIssue(
                category="content",
                severity="info",
                message="Could not analyze PDF content",
            ))

        score = max(0, 100 - len(issues) * 15)
        return QAReport(
            score=score,
            issues=issues,
            summary=f"Rule-based analysis: {len(issues)} issues found",
            provider="rule-based",
        )

    def _check_html_content(self, html: str) -> QAReport:
        """Check HTML content for common resume issues."""
        issues: list[QAIssue] = []

        html_lower = html.lower()
        if "your name" in html_lower or "{{ full_name }}" in html_lower:
            issues.append(QAIssue(
                category="content",
                severity="critical",
                message="Placeholder name detected",
                suggestion="Replace with actual name",
            ))

        if "email" not in html_lower and "@" not in html:
            issues.append(QAIssue(
                category="content",
                severity="warning",
                message="No email address found",
                suggestion="Add email to contact section",
            ))

        if "<table" in html_lower:
            issues.append(QAIssue(
                category="ats",
                severity="warning",
                message="Table layout detected — may cause ATS parsing issues",
                suggestion="Use CSS-based layout",
            ))

        score = max(0, 100 - len(issues) * 15)
        return QAReport(
            score=score,
            issues=issues,
            summary=f"HTML analysis: {len(issues)} issues found",
            provider="html-analysis",
        )
