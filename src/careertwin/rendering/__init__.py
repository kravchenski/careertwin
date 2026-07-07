"""PDF rendering pipeline — renders CareerGraph to PDF via HTML templates."""

from __future__ import annotations

import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from jinja2 import Template

from careertwin.graph.schema import CareerGraph
from careertwin.templates import get_template


@dataclass
class RenderConfig:
    template: str = "modern"
    output_path: str = "resume.pdf"
    page_size: str = "A4"
    margin_top: str = "20mm"
    margin_bottom: str = "20mm"
    margin_left: str = "18mm"
    margin_right: str = "18mm"
    enable_page_breaks: bool = True
    font_paths: list[str] = field(default_factory=list)


@dataclass
class RenderResult:
    pdf_path: str
    html_path: str
    template: str
    page_count: int = 0
    warnings: list[str] = field(default_factory=list)


class PdfRenderer:
    """Renders CareerGraph to PDF using HTML templates and weasyprint."""

    def render(self, graph: CareerGraph, config: RenderConfig | None = None) -> RenderResult:
        config = config or RenderConfig()
        html = self._render_html(graph, config.template)
        html = self._inject_page_break_css(html, config)
        html = self._inject_font_faces(html, config)
        html_path = self._write_temp_html(html)
        pdf_path = config.output_path

        self._html_to_pdf(html_path, pdf_path, config)

        return RenderResult(
            pdf_path=pdf_path,
            html_path=html_path,
            template=config.template,
            page_count=self._count_pages(pdf_path),
        )

    def render_to_html(self, graph: CareerGraph, template_name: str = "modern") -> str:
        return self._render_html(graph, template_name)

    def render_to_image(
        self, graph: CareerGraph, config: RenderConfig | None = None, page: int = 1
    ) -> bytes | None:
        """Render PDF then convert specified page to PNG image bytes."""
        result = self.render(graph, config)
        try:
            from pdf2image import convert_from_path
            images = convert_from_path(result.pdf_path, first_page=page, last_page=page, dpi=150)
            if images:
                import io
                buf = io.BytesIO()
                images[0].save(buf, format="PNG")
                return buf.getvalue()
        except ImportError:
            pass
        return None

    def _render_html(self, graph: CareerGraph, template_name: str) -> str:
        template_str = get_template(template_name)
        template = Template(template_str)

        context = {
            "full_name": graph.profile.full_name or "Your Name",
            "headline": graph.profile.headline,
            "email": graph.profile.email,
            "phone": graph.profile.phone,
            "location": graph.profile.location,
            "website": graph.profile.website,
            "linkedin": graph.profile.linkedin,
            "github": graph.profile.github,
            "summary": graph.profile.summary,
            "experience": [
                {
                    "position": exp.position,
                    "company": exp.company,
                    "location": exp.location,
                    "description": exp.description,
                    "start_date": exp.start_date.strftime("%b %Y") if exp.start_date else None,
                    "end_date": exp.end_date.strftime("%b %Y") if exp.end_date else None,
                }
                for exp in graph.experience
            ],
            "education": [
                {
                    "institution": edu.institution,
                    "degree": edu.degree,
                    "field": edu.field,
                    "start_date": edu.start_date,
                    "end_date": edu.end_date,
                }
                for edu in graph.education
            ],
            "skills": [{"name": s.name, "category": s.category.value} for s in graph.skills],
            "projects": [
                {
                    "name": proj.name,
                    "description": proj.description,
                }
                for proj in graph.projects
            ],
        }

        return template.render(**context)

    def _inject_page_break_css(self, html: str, config: RenderConfig) -> str:
        """Inject CSS to prevent orphaned sections across page breaks."""
        if not config.enable_page_breaks:
            return html
        page_break_css = """
<style>
  .section { page-break-inside: avoid; }
  .exp-item { page-break-inside: avoid; }
  h2 { page-break-after: avoid; }
</style>"""
        return html.replace("</head>", f"{page_break_css}\n</head>")

    def _inject_font_faces(self, html: str, config: RenderConfig) -> str:
        """Inject @font-face rules for custom fonts."""
        if not config.font_paths:
            return html
        font_faces = []
        for font_path in config.font_paths:
            path = Path(font_path)
            if path.exists():
                font_faces.append(
                    f'@font-face {{ font-family: "{path.stem}"; '
                    f'src: url("file://{path.resolve()}"); }}'
                )
        if font_faces:
            font_css = "<style>\n" + "\n".join(font_faces) + "\n</style>"
            return html.replace("</head>", f"{font_css}\n</head>")
        return html

    def _write_temp_html(self, html: str) -> str:
        tmp = tempfile.NamedTemporaryFile(suffix=".html", delete=False, mode="w", encoding="utf-8")
        tmp.write(html)
        tmp.close()
        return tmp.name

    def _html_to_pdf(self, html_path: str, pdf_path: str, config: RenderConfig) -> None:
        try:
            from weasyprint import HTML
            HTML(filename=html_path).write_pdf(pdf_path)
        except ImportError:
            self._fallback_html_to_pdf(html_path, pdf_path)

    def _fallback_html_to_pdf(self, html_path: str, pdf_path: str) -> None:
        import subprocess

        try:
            subprocess.run(
                ["wkhtmltopdf", "--quiet", html_path, pdf_path],
                check=True,
                capture_output=True,
            )
        except (FileNotFoundError, subprocess.CalledProcessError):
            Path(pdf_path).write_text(
                f"PDF rendering requires weasyprint or wkhtmltopdf.\n"
                f"HTML preview saved to: {html_path}",
            )

    def _count_pages(self, pdf_path: str) -> int:
        try:
            from pypdf import PdfReader
            reader = PdfReader(pdf_path)
            return len(reader.pages)
        except Exception:
            return 0
