"""Tests for PDF rendering pipeline."""

from careertwin.graph.schema import CareerGraph, Profile
from careertwin.rendering import PdfRenderer, RenderConfig, RenderResult
from careertwin.templates import get_template, list_templates


class TestPdfRenderer:
    def test_render_to_html(self, sample_graph: CareerGraph):
        renderer = PdfRenderer()
        html = renderer.render_to_html(sample_graph, "modern")

        assert html
        assert "John Doe" in html
        assert "EXPERIENCE" in html.upper() or "Experience" in html

    def test_render_to_html_classic(self, sample_graph: CareerGraph):
        renderer = PdfRenderer()
        html = renderer.render_to_html(sample_graph, "classic")

        assert html
        assert "John Doe" in html

    def test_render_to_html_minimal(self, sample_graph: CareerGraph):
        renderer = PdfRenderer()
        html = renderer.render_to_html(sample_graph, "minimal")

        assert html
        assert "John Doe" in html

    def test_render_to_html_empty_graph(self):
        graph = CareerGraph(profile=Profile(full_name="Test User"))
        renderer = PdfRenderer()
        html = renderer.render_to_html(graph, "modern")

        assert html
        assert "Test User" in html

    def test_render_config_defaults(self):
        config = RenderConfig()
        assert config.template == "modern"
        assert config.output_path == "resume.pdf"
        assert config.enable_page_breaks is True
        assert config.font_paths == []

    def test_render_result_dataclass(self):
        result = RenderResult(pdf_path="test.pdf", html_path="test.html", template="modern")
        assert result.pdf_path == "test.pdf"
        assert result.page_count == 0

    def test_inject_page_break_css(self, sample_graph: CareerGraph):
        renderer = PdfRenderer()
        html = renderer.render_to_html(sample_graph, "modern")
        config = RenderConfig(enable_page_breaks=True)
        result = renderer._inject_page_break_css(html, config)
        assert "page-break-inside: avoid" in result

    def test_inject_page_break_css_disabled(self, sample_graph: CareerGraph):
        renderer = PdfRenderer()
        html = renderer.render_to_html(sample_graph, "modern")
        config = RenderConfig(enable_page_breaks=False)
        result = renderer._inject_page_break_css(html, config)
        assert result == html

    def test_inject_font_faces_empty(self, sample_graph: CareerGraph):
        renderer = PdfRenderer()
        html = renderer.render_to_html(sample_graph, "modern")
        config = RenderConfig(font_paths=[])
        result = renderer._inject_font_faces(html, config)
        assert result == html


class TestTemplates:
    def test_list_templates(self):
        templates = list_templates()
        assert len(templates) >= 6
        assert "modern" in templates
        assert "classic" in templates
        assert "minimal" in templates
        assert "sidebar" in templates
        assert "executive" in templates
        assert "ats" in templates

    def test_get_template_modern(self):
        html = get_template("modern")
        assert html
        assert "{{ full_name }}" in html

    def test_get_template_classic(self):
        html = get_template("classic")
        assert html
        assert "{{ full_name }}" in html

    def test_get_template_minimal(self):
        html = get_template("minimal")
        assert html
        assert "{{ full_name }}" in html

    def test_get_template_sidebar(self):
        html = get_template("sidebar")
        assert html
        assert "{{ full_name }}" in html
        assert "sidebar" in html.lower()

    def test_get_template_executive(self):
        html = get_template("executive")
        assert html
        assert "{{ full_name }}" in html
        assert "Georgia" in html or "Times" in html

    def test_get_template_ats(self):
        html = get_template("ats")
        assert html
        assert "{{ full_name }}" in html
        assert "Arial" in html or "sans-serif" in html

    def test_get_template_invalid(self):
        import pytest
        with pytest.raises(ValueError, match="not found"):
            get_template("nonexistent")
