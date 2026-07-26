"""Built-in resume templates."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

TEMPLATES_DIR = Path(__file__).parent / "html"


@dataclass
class TemplateInfo:
    name: str
    description: str
    style: str
    best_for: list[str] = field(default_factory=list)
    colors: list[str] = field(default_factory=list)
    fonts: list[str] = field(default_factory=list)


TEMPLATE_REGISTRY: dict[str, TemplateInfo] = {
    "modern": TemplateInfo(
        name="modern",
        description="Clean professional design with blue accents",
        style="modern",
        best_for=["tech", "business", "general"],
        colors=["#0f3460", "#16213e", "#1a1a2e"],
        fonts=["Segoe UI", "Inter", "Helvetica Neue"],
    ),
    "classic": TemplateInfo(
        name="classic",
        description="Traditional serif-based design for formal industries",
        style="classic",
        best_for=["finance", "law", "academia", "government"],
        colors=["#222222", "#555555", "#888888"],
        fonts=["Georgia", "Times New Roman"],
    ),
    "minimal": TemplateInfo(
        name="minimal",
        description="Ultra-clean design with generous whitespace",
        style="minimal",
        best_for=["design", "consulting", "startup"],
        colors=["#333333", "#666666", "#999999"],
        fonts=["Helvetica Neue", "Arial"],
    ),
    "creative": TemplateInfo(
        name="creative",
        description="Bold gradient design with rounded elements",
        style="creative",
        best_for=["design", "marketing", "media", "creative"],
        colors=["#667eea", "#764ba2"],
        fonts=["Poppins", "Nunito"],
    ),
    "executive": TemplateInfo(
        name="executive",
        description="Premium serif design for senior leadership",
        style="classic",
        best_for=["executive", "director", "vp", "c-suite"],
        colors=["#1a202c", "#4a5568", "#718096"],
        fonts=["Georgia", "Times New Roman"],
    ),
    "technical": TemplateInfo(
        name="technical",
        description="Monospace design for developers and engineers",
        style="modern",
        best_for=["software", "engineering", "devops", "data"],
        colors=["#0d9488", "#1a202c", "#64748b"],
        fonts=["JetBrains Mono", "Fira Code", "Consolas"],
    ),
    "sidebar": TemplateInfo(
        name="sidebar",
        description="Two-column layout with sidebar for contact info",
        style="modern",
        best_for=["tech", "business", "general"],
        colors=["#2563eb", "#1e40af", "#1e3a5f"],
        fonts=["Inter", "Segoe UI", "sans-serif"],
    ),
    "ats": TemplateInfo(
        name="ats",
        description="ATS-optimized plain text friendly design",
        style="minimal",
        best_for=["ats", "applicant-tracking", "recruiting"],
        colors=["#000000", "#333333", "#666666"],
        fonts=["Arial", "Helvetica", "sans-serif"],
    ),
    "senior": TemplateInfo(
        name="senior",
        description="Restrained, editorial design for senior/mid-level IC roles — plain-text skills line, "
        "left-aligned header, single muted accent instead of generic AI blue",
        style="modern",
        best_for=["senior", "mid-level", "engineering", "tech", "general"],
        colors=["#1a1a1a", "#3f6659", "#595959"],
        fonts=["Helvetica Neue", "Arial"],
    ),
}


def get_template(name: str) -> str:
    """Get HTML template by name."""
    path = TEMPLATES_DIR / f"{name}.html"
    if not path.exists():
        available = [p.stem for p in TEMPLATES_DIR.glob("*.html")]
        raise ValueError(f"Template '{name}' not found. Available: {available}")
    return path.read_text(encoding="utf-8")


def list_templates() -> list[str]:
    """List available template names."""
    return sorted(p.stem for p in TEMPLATES_DIR.glob("*.html"))


def get_template_info(name: str) -> TemplateInfo:
    """Get metadata about a template."""
    if name not in TEMPLATE_REGISTRY:
        raise ValueError(f"Template '{name}' not found in registry")
    return TEMPLATE_REGISTRY[name]


def get_templates_for_role(role: str) -> list[str]:
    """Get recommended templates for a job role or industry."""
    role_lower = role.lower()
    recommended: list[str] = []

    for name, info in TEMPLATE_REGISTRY.items():
        for best_for in info.best_for:
            if best_for in role_lower or role_lower in best_for:
                recommended.append(name)
                break

    if not recommended:
        recommended = ["modern", "minimal"]

    return recommended


def get_template_colors(template_name: str) -> list[str]:
    """Get the color palette for a template."""
    if template_name in TEMPLATE_REGISTRY:
        return TEMPLATE_REGISTRY[template_name].colors
    return []
