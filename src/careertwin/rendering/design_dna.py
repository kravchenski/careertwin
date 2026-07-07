"""Design DNA engine — analyzes and extracts design patterns from resume templates."""

from __future__ import annotations

import colorsys
import re
from dataclasses import dataclass, field


@dataclass
class DesignDNA:
    """Extracted design characteristics of a resume."""
    colors: list[str] = field(default_factory=list)
    fonts: list[str] = field(default_factory=list)
    spacing: str = "normal"  # compact, normal, spacious
    layout: str = "single"  # single, two-column, sidebar
    style: str = "modern"  # modern, classic, minimal, creative
    section_style: str = "line"  # line, box, bold, underline
    tag_style: str = "filled"  # filled, outlined, plain
    scores: dict[str, float] = field(default_factory=dict)
    readability_score: float = 0.0
    ats_score: float = 0.0
    visual_hierarchy: str = "good"  # good, poor, excellent


class DesignDNAEngine:
    """Analyzes HTML/CSS to extract design DNA."""

    COLOR_PATTERNS = [
        r"color:\s*(#[0-9a-fA-F]{3,8})",
        r"background(?:-color)?:\s*(#[0-9a-fA-F]{3,8})",
        r"(?:rgb|hsl)a?\([^)]+\)",
    ]

    FONT_SIZES = {
        "xs": (0, 8),
        "sm": (8, 10),
        "base": (10, 12),
        "lg": (12, 14),
        "xl": (14, 18),
        "2xl": (18, 24),
        "3xl": (24, 32),
    }

    def analyze_html(self, html: str) -> DesignDNA:
        """Extract design DNA from HTML/CSS content."""
        colors = self._extract_colors(html)
        fonts = self._extract_fonts(html)
        spacing = self._analyze_spacing(html)
        layout = self._detect_layout(html)
        style = self._detect_style(html, colors, fonts)
        section_style = self._detect_section_style(html)
        tag_style = self._detect_tag_style(html)

        scores = self._compute_scores(colors, fonts, spacing, layout, style)
        readability = self._compute_readability_score(html)
        ats = self._compute_ats_score(html)
        hierarchy = self._analyze_visual_hierarchy(html)

        return DesignDNA(
            colors=colors,
            fonts=fonts,
            spacing=spacing,
            layout=layout,
            style=style,
            section_style=section_style,
            tag_style=tag_style,
            scores=scores,
            readability_score=readability,
            ats_score=ats,
            visual_hierarchy=hierarchy,
        )

    def _extract_colors(self, html: str) -> list[str]:
        colors: list[str] = []
        for pattern in self.COLOR_PATTERNS:
            for match in re.finditer(pattern, html, re.IGNORECASE):
                color = match.group(0) if "(" in match.group(0) else match.group(1)
                if color not in colors:
                    colors.append(color)
        return colors[:10]

    def _extract_fonts(self, html: str) -> list[str]:
        fonts: list[str] = []
        for match in re.finditer(r"font-family:\s*([^;]+)", html, re.IGNORECASE):
            font_list = match.group(1).strip()
            for font in font_list.split(","):
                font = font.strip().strip("'\"")
                if font and font not in fonts and font not in (
                    "system-ui", "-apple-system", "sans-serif", "serif", "monospace",
                ):
                    fonts.append(font)
        return fonts[:5]

    def _analyze_spacing(self, html: str) -> str:
        margins = re.findall(r"margin(?:-(?:top|bottom|left|right))?:\s*(\d+)(?:px|mm|pt)", html)
        if not margins:
            return "normal"
        avg = sum(int(m) for m in margins) / len(margins)
        if avg < 6:
            return "compact"
        elif avg > 14:
            return "spacious"
        return "normal"

    def _detect_layout(self, html: str) -> str:
        html_lower = html.lower()
        if "grid" in html_lower and "column" in html_lower:
            return "two-column"
        if "sidebar" in html_lower or r"float:\s*left" in html_lower:
            return "sidebar"
        return "single"

    def _detect_style(self, html: str, colors: list[str], fonts: list[str]) -> str:
        html_lower = html.lower()
        if "georgia" in html_lower or "times" in html_lower:
            return "classic"
        if len(colors) <= 2 and not fonts:
            return "minimal"
        if "gradient" in html_lower or "shadow" in html_lower:
            return "creative"
        return "modern"

    def _detect_section_style(self, html: str) -> str:
        if "border-bottom" in html.lower():
            return "line"
        if "background" in html.lower() and "padding" in html.lower():
            return "box"
        if r"text-transform:\s*uppercase" in html.lower():
            return "bold"
        return "underline"

    def _detect_tag_style(self, html: str) -> str:
        if "border-radius" in html.lower():
            if "background" in html.lower():
                return "filled"
            return "outlined"
        return "plain"

    def _compute_scores(
        self,
        colors: list[str],
        fonts: list[str],
        spacing: str,
        layout: str,
        style: str,
    ) -> dict[str, float]:
        scores: dict[str, float] = {}
        scores["color_harmony"] = self._compute_color_harmony(colors)
        scores["typography"] = self._compute_typography_score(fonts)
        scores["spacing"] = {"compact": 60, "normal": 85, "spacious": 70}[spacing]
        scores["layout"] = {"single": 75, "two-column": 90, "sidebar": 85}[layout]
        scores["overall"] = sum(scores.values()) / len(scores)
        return scores

    def _compute_color_harmony(self, colors: list[str]) -> float:
        """Compute color harmony score based on color relationships."""
        if len(colors) < 2:
            return 70.0

        hex_colors = [c for c in colors if c.startswith("#") and len(c) in (4, 7)]
        if len(hex_colors) < 2:
            return 70.0

        hue_diffs: list[float] = []
        for i in range(len(hex_colors) - 1):
            h1 = self._hex_to_hue(hex_colors[i])
            h2 = self._hex_to_hue(hex_colors[i + 1])
            diff = abs(h1 - h2)
            hue_diffs.append(min(diff, 360 - diff))

        avg_diff = sum(hue_diffs) / len(hue_diffs)

        if 15 <= avg_diff <= 45:
            return 95.0
        elif 45 < avg_diff <= 90:
            return 85.0
        elif avg_diff < 15:
            return 75.0
        else:
            return 65.0

    def _hex_to_hue(self, hex_color: str) -> float:
        """Convert hex color to hue (0-360)."""
        hex_color = hex_color.lstrip("#")
        if len(hex_color) == 3:
            hex_color = "".join(c * 2 for c in hex_color)

        r = int(hex_color[0:2], 16) / 255
        g = int(hex_color[2:4], 16) / 255
        b = int(hex_color[4:6], 16) / 255

        h, _, _ = colorsys.rgb_to_hsv(r, g, b)
        return h * 360

    def _compute_typography_score(self, fonts: list[str]) -> float:
        """Compute typography score based on font choices."""
        if not fonts:
            return 50.0

        score = 70.0

        professional_fonts = {
            "inter", "helvetica neue", "helvetica", "arial", "segoe ui",
            "roboto", "open sans", "lato", "montserrat", "source sans pro",
        }

        for font in fonts:
            if font.lower() in professional_fonts:
                score += 10
                break

        if len(fonts) <= 2:
            score += 10

        return min(100.0, score)

    def _compute_readability_score(self, html: str) -> float:
        """Compute readability score based on text properties."""
        score = 80.0

        font_sizes = re.findall(r"font-size:\s*(\d+\.?\d*)(?:pt|px)", html)
        if font_sizes:
            sizes = [float(s) for s in font_sizes]
            min_size = min(sizes)
            max_size = max(sizes)

            if min_size < 8:
                score -= 10
            if max_size > 24:
                score -= 5

            if len(set(sizes)) >= 3:
                score += 5

        line_heights = re.findall(r"line-height:\s*(\d+\.?\d*)", html)
        if line_heights:
            heights = [float(h) for h in line_heights]
            avg_height = sum(heights) / len(heights)
            if 1.3 <= avg_height <= 1.6:
                score += 10

        return min(100.0, max(0.0, score))

    def _compute_ats_score(self, html: str) -> float:
        """Compute ATS compatibility score."""
        score = 80.0
        html_lower = html.lower()

        if "<table" in html_lower:
            score -= 15

        if "position: absolute" in html_lower or "position: fixed" in html_lower:
            score -= 10

        if "display: grid" in html_lower or "display: flex" in html_lower:
            score += 5

        headers = ["summary", "experience", "education", "skills"]
        for header in headers:
            if header in html_lower:
                score += 3

        return min(100.0, max(0.0, score))

    def _analyze_visual_hierarchy(self, html: str) -> str:
        """Analyze visual hierarchy quality."""
        font_sizes = re.findall(r"font-size:\s*(\d+\.?\d*)(?:pt|px)", html)

        if not font_sizes:
            return "good"

        sizes = sorted(set(float(s) for s in font_sizes), reverse=True)

        if len(sizes) >= 3:
            if sizes[0] > sizes[1] * 1.3:
                return "excellent"
            elif sizes[0] > sizes[1]:
                return "good"
            else:
                return "poor"

        return "good"
