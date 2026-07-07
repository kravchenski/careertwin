"""Tests for Vision QA agent."""

from careertwin.vision import (
    QAIssue,
    QAReport,
    VisionQAAgent,
)


class TestVisionQAAgent:
    def test_rule_based_check(self, tmp_path):
        pdf_path = tmp_path / "test.pdf"
        pdf_path.write_bytes(b"%PDF-1.4 minimal")

        agent = VisionQAAgent()
        report = agent._rule_based_check(str(pdf_path))

        assert isinstance(report, QAReport)
        assert 0 <= report.score <= 100

    def test_check_html_content(self, tmp_path):
        html_path = tmp_path / "test.html"
        html_path.write_text("<html><body><h1>John Doe</h1></body></html>")

        agent = VisionQAAgent()
        report = agent.check_html(str(html_path))

        assert isinstance(report, QAReport)
        assert report.score >= 0

    def test_check_html_placeholder(self, tmp_path):
        html_path = tmp_path / "test.html"
        html_path.write_text("<html><body>{{ full_name }}</body></html>")

        agent = VisionQAAgent()
        report = agent.check_html(str(html_path))

        assert any(i.severity == "critical" for i in report.issues)

    def test_check_html_missing_email(self, tmp_path):
        html_path = tmp_path / "test.html"
        html_path.write_text("<html><body><h1>Test</h1></body></html>")

        agent = VisionQAAgent()
        report = agent.check_html(str(html_path))

        assert any("email" in i.message.lower() for i in report.issues)

    def test_qa_report_to_text(self):
        report = QAReport(
            score=75.0,
            issues=[
                QAIssue(category="layout", severity="warning", message="Test issue"),
            ],
            summary="Test summary",
        )
        text = report.to_text()

        assert "75" in text
        assert "Test issue" in text
        assert "Test summary" in text

    def test_qa_report_empty(self):
        report = QAReport(score=100.0)
        text = report.to_text()
        assert "100" in text

    def test_parse_qa_response(self):
        agent = VisionQAAgent()
        response = (
            '{"score": 85, "summary": "Good", '
            '"issues": [{"category": "layout", "severity": "info", '
            '"message": "Minor issue"}]}'
        )
        report = agent._parse_qa_response(response)

        assert report.score == 85
        assert len(report.issues) == 1
        assert report.issues[0].category == "layout"

    def test_parse_qa_response_invalid(self):
        agent = VisionQAAgent()
        report = agent._parse_qa_response("not json")

        assert report.score == 50.0

    def test_init_default_provider(self):
        agent = VisionQAAgent()
        assert agent.provider == "openai"

    def test_init_custom_provider(self):
        agent = VisionQAAgent(provider="qwen")
        assert agent.provider == "qwen"

    def test_qa_report_with_provider(self):
        report = QAReport(score=80.0, provider="openai")
        text = report.to_text()
        assert "openai" in text.lower()

    def test_analyze_image_fallback(self):
        agent = VisionQAAgent(provider="invalid")
        report = agent._analyze_image(b"fake image bytes")
        assert report.score == 50.0
        assert "rule-based" in report.provider
