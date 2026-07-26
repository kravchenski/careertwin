"""Design trend scraper — fetches resume design trends from Dribbble and Behance."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class DesignTrend:
    name: str
    source: str
    colors: list[str] = field(default_factory=list)
    fonts: list[str] = field(default_factory=list)
    style_tags: list[str] = field(default_factory=list)
    popularity: float = 0.0
    url: str = ""
    image_url: str = ""


class DesignTrendScraper:
    """Scrapes and caches design trends for resume styling."""

    BUILT_IN_TRENDS = [
        DesignTrend(
            name="Modern Professional",
            source="built-in",
            colors=["#0f3460", "#16213e", "#1a1a2e"],
            fonts=["Segoe UI", "Inter", "Helvetica Neue"],
            style_tags=["clean", "professional", "single-column"],
            popularity=0.9,
        ),
        DesignTrend(
            name="Minimalist Mono",
            source="built-in",
            colors=["#333333", "#666666", "#999999"],
            fonts=["Helvetica Neue", "Arial", "system-ui"],
            style_tags=["minimal", "monochrome", "spacious"],
            popularity=0.85,
        ),
        DesignTrend(
            name="Classic Executive",
            source="built-in",
            colors=["#222222", "#555555", "#888888"],
            fonts=["Georgia", "Times New Roman", "serif"],
            style_tags=["classic", "formal", "traditional"],
            popularity=0.7,
        ),
        DesignTrend(
            name="Tech Startup",
            source="built-in",
            colors=["#6366f1", "#8b5cf6", "#a855f7"],
            fonts=["Inter", "JetBrains Mono", "Fira Code"],
            style_tags=["modern", "colorful", "tech"],
            popularity=0.8,
        ),
        DesignTrend(
            name="Creative Accent",
            source="built-in",
            colors=["#0ea5e9", "#06b6d4", "#14b8a6"],
            fonts=["Poppins", "Nunito", "Montserrat"],
            style_tags=["creative", "accent-color", "friendly"],
            popularity=0.75,
        ),
    ]

    def __init__(self, cache_path: str = ".design_trends.json"):
        self.cache_path = Path(cache_path)
        self._trends: list[DesignTrend] | None = None

    def get_trends(self) -> list[DesignTrend]:
        """Get all available design trends."""
        if self._trends is None:
            self._trends = self._load_trends()
        return self._trends

    def get_trending_colors(self) -> list[str]:
        """Get the most popular colors across trends."""
        color_counts: dict[str, float] = {}
        for trend in self.get_trends():
            for color in trend.colors:
                color_counts[color] = color_counts.get(color, 0) + trend.popularity
        return sorted(color_counts, key=color_counts.get, reverse=True)[:10]

    def get_trending_fonts(self) -> list[str]:
        """Get the most popular fonts across trends."""
        font_counts: dict[str, float] = {}
        for trend in self.get_trends():
            for font in trend.fonts:
                font_counts[font] = font_counts.get(font, 0) + trend.popularity
        return sorted(font_counts, key=font_counts.get, reverse=True)[:8]

    def get_trends_by_style(self, style: str) -> list[DesignTrend]:
        """Filter trends by style tag."""
        return [
            t for t in self.get_trends()
            if style.lower() in [s.lower() for s in t.style_tags]
        ]

    def scrape_dribbble(self, query: str = "resume design", page: int = 1) -> list[DesignTrend]:
        """Scrape resume design trends from Dribbble."""
        import httpx

        trends: list[DesignTrend] = []
        try:
            response = httpx.get(
                "https://api.dribbble.com/v2/shots",
                params={
                    "query": query,
                    "page": page,
                    "per_page": 10,
                    "sort": "popular",
                },
                timeout=15.0,
            )
            response.raise_for_status()
            data = response.json()

            for shot in data:
                colors = self._extract_colors_from_image(shot.get("images", {}).get("normal", ""))
                trend = DesignTrend(
                    name=shot.get("title", "Untitled"),
                    source="dribbble",
                    colors=colors[:5],
                    style_tags=self._infer_style_tags(shot),
                    popularity=min(1.0, shot.get("likes_count", 0) / 1000),
                    url=shot.get("html_url", ""),
                    image_url=shot.get("images", {}).get("normal", ""),
                )
                trends.append(trend)
        except Exception:
            pass

        return trends

    def scrape_behance(self, query: str = "resume design", page: int = 1) -> list[DesignTrend]:
        """Scrape resume design trends from Behance."""
        import httpx

        trends: list[DesignTrend] = []
        try:
            response = httpx.get(
                "https://www.behance.net/search/projects",
                params={
                    "search": query,
                    "page": page,
                },
                headers={"User-Agent": "Mozilla/5.0"},
                timeout=15.0,
            )
            response.raise_for_status()
            html = response.text

            project_pattern = r'data-id="(\d+)"'
            title_pattern = r'class="ProjectCoverResults-title[^"]*"[^>]*>([^<]+)</a>'

            ids = re.findall(project_pattern, html)
            titles = re.findall(title_pattern, html)

            for i, (proj_id, title) in enumerate(zip(ids[:10], titles[:10])):
                trend = DesignTrend(
                    name=title.strip(),
                    source="behance",
                    colors=[],
                    style_tags=["behance", "design"],
                    popularity=max(0.5, 1.0 - i * 0.05),
                    url=f"https://www.behance.net/gallery/{proj_id}",
                )
                trends.append(trend)
        except Exception:
            pass

        return trends

    def scrape_all(self, query: str = "resume design") -> list[DesignTrend]:
        """Scrape from all available sources and merge with built-in trends."""
        all_trends = list(self.BUILT_IN_TRENDS)

        dribbble_trends = self.scrape_dribbble(query)
        all_trends.extend(dribbble_trends)

        behance_trends = self.scrape_behance(query)
        all_trends.extend(behance_trends)

        self._trends = all_trends
        self.save_trends(all_trends)

        return all_trends

    def _extract_colors_from_image(self, image_url: str) -> list[str]:
        """Extract dominant colors from an image URL (placeholder implementation)."""
        default_colors = ["#333333", "#666666", "#999999", "#0f3460", "#1a1a2e"]
        return default_colors

    def _infer_style_tags(self, shot: dict) -> list[str]:
        """Infer style tags from Dribbble shot metadata."""
        tags = ["resume"]
        title = shot.get("title", "").lower()

        if any(w in title for w in ["minimal", "clean", "simple"]):
            tags.append("minimal")
        if any(w in title for w in ["creative", "colorful", "bold"]):
            tags.append("creative")
        if any(w in title for w in ["professional", "corporate", "executive"]):
            tags.append("professional")
        if any(w in title for w in ["modern", " contemporary"]):
            tags.append("modern")

        return tags

    def _load_trends(self) -> list[DesignTrend]:
        trends = list(self.BUILT_IN_TRENDS)
        if self.cache_path.exists():
            try:
                data = json.loads(self.cache_path.read_text())
                for item in data:
                    trends.append(DesignTrend(**item))
            except (json.JSONDecodeError, KeyError):
                pass
        return trends

    def save_trends(self, trends: list[DesignTrend]) -> None:
        """Save trends to cache file."""
        data = [
            {
                "name": t.name,
                "source": t.source,
                "colors": t.colors,
                "fonts": t.fonts,
                "style_tags": t.style_tags,
                "popularity": t.popularity,
                "url": t.url,
                "image_url": t.image_url,
            }
            for t in trends
        ]
        self.cache_path.write_text(json.dumps(data, indent=2))
