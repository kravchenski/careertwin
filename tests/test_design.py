"""Tests for Design DNA engine and trends."""

from careertwin.rendering.design_dna import DesignDNA, DesignDNAEngine
from careertwin.rendering.trends import (
    DesignTrend,
    DesignTrendScraper,
)


class TestDesignDNAEngine:
    def test_analyze_html(self):
        html = """
        <style>
        body { font-family: 'Segoe UI', sans-serif; color: #333; }
        h1 { color: #0f3460; }
        .tag { background: #e8f0fe; border-radius: 4px; }
        </style>
        """
        engine = DesignDNAEngine()
        dna = engine.analyze_html(html)

        assert isinstance(dna, DesignDNA)
        assert dna.style == "modern"
        assert len(dna.colors) > 0
        assert len(dna.fonts) > 0

    def test_analyze_classic(self):
        html = """
        <style>
        body { font-family: 'Georgia', serif; color: #222; }
        </style>
        """
        engine = DesignDNAEngine()
        dna = engine.analyze_html(html)

        assert dna.style == "classic"

    def test_analyze_minimal(self):
        html = """
        <style>
        body { color: #333; }
        </style>
        """
        engine = DesignDNAEngine()
        dna = engine.analyze_html(html)

        assert dna.style == "minimal"

    def test_extract_colors(self):
        html = "color: #0f3460; background: #ffffff;"
        engine = DesignDNAEngine()
        colors = engine._extract_colors(html)

        assert "#0f3460" in colors
        assert "#ffffff" in colors

    def test_extract_fonts(self):
        html = "font-family: 'Inter', 'Helvetica Neue', sans-serif;"
        engine = DesignDNAEngine()
        fonts = engine._extract_fonts(html)

        assert "Inter" in fonts
        assert "Helvetica Neue" in fonts

    def test_analyze_spacing_compact(self):
        html = "margin: 4px; padding: 2px;"
        engine = DesignDNAEngine()
        assert engine._analyze_spacing(html) == "compact"

    def test_analyze_spacing_spacious(self):
        html = "margin: 20px; padding: 16px;"
        engine = DesignDNAEngine()
        assert engine._analyze_spacing(html) == "spacious"

    def test_detect_layout_single(self):
        html = "<div>content</div>"
        engine = DesignDNAEngine()
        assert engine._detect_layout(html) == "single"

    def test_detect_layout_two_column(self):
        html = "display: grid; grid-template-columns: 1fr 2fr;"
        engine = DesignDNAEngine()
        assert engine._detect_layout(html) == "two-column"

    def test_compute_scores(self):
        engine = DesignDNAEngine()
        scores = engine._compute_scores(["#333"], ["Inter"], "normal", "single", "modern")

        assert "overall" in scores
        assert 0 <= scores["overall"] <= 100

    def test_color_harmony_score(self):
        engine = DesignDNAEngine()
        score = engine._compute_color_harmony(["#0f3460", "#16213e", "#1a1a2e"])
        assert 0 <= score <= 100

    def test_readability_score(self):
        engine = DesignDNAEngine()
        html = "font-size: 10pt; line-height: 1.5;"
        score = engine._compute_readability_score(html)
        assert 0 <= score <= 100

    def test_ats_score(self):
        engine = DesignDNAEngine()
        html = "<h2>Experience</h2><h2>Education</h2><h2>Skills</h2>"
        score = engine._compute_ats_score(html)
        assert 0 <= score <= 100


class TestDesignTrendScraper:
    def test_get_trends(self):
        scraper = DesignTrendScraper()
        trends = scraper.get_trends()

        assert len(trends) >= 5

    def test_get_trending_colors(self):
        scraper = DesignTrendScraper()
        colors = scraper.get_trending_colors()

        assert len(colors) > 0
        assert all(c.startswith("#") for c in colors)

    def test_get_trending_fonts(self):
        scraper = DesignTrendScraper()
        fonts = scraper.get_trending_fonts()

        assert len(fonts) > 0

    def test_get_trends_by_style(self):
        scraper = DesignTrendScraper()
        trends = scraper.get_trends_by_style("minimal")

        assert all("minimal" in t.style_tags for t in trends)

    def test_built_in_trends_have_required_fields(self):
        scraper = DesignTrendScraper()
        trends = scraper.get_trends()

        for trend in trends:
            assert trend.name
            assert trend.source
            assert len(trend.colors) > 0
            assert len(trend.fonts) > 0
            assert 0 <= trend.popularity <= 1

    def test_save_and_load_trends(self, tmp_path):
        cache_path = tmp_path / "trends.json"
        scraper = DesignTrendScraper(cache_path=str(cache_path))
        trends = scraper.get_trends()
        scraper.save_trends(trends)

        scraper2 = DesignTrendScraper(cache_path=str(cache_path))
        loaded = scraper2.get_trends()
        assert len(loaded) >= len(trends)

    def test_design_trend_dataclass(self):
        trend = DesignTrend(
            name="Test",
            source="dribbble",
            colors=["#fff"],
            style_tags=["test"],
            popularity=0.5,
        )
        assert trend.name == "Test"
        assert trend.source == "dribbble"
