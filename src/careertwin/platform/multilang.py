"""Multi-language CV support — translates and localizes resumes."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum


class Language(str, Enum):
    EN = "en"
    RU = "ru"
    DE = "de"
    FR = "fr"
    ES = "es"
    PT = "pt"
    ZH = "zh"
    JA = "ja"
    KO = "ko"
    UK = "uk"


@dataclass
class LocalizedText:
    original: str
    translated: str = ""
    language: Language = Language.EN


SECTION_TRANSLATIONS: dict[str, dict[str, str]] = {
    "Summary": {
        "ru": "Резюме",
        "de": "Zusammenfassung",
        "fr": "Résumé",
        "es": "Resumen",
        "pt": "Resumo",
        "zh": "摘要",
        "ja": "概要",
        "ko": "요약",
        "uk": "Резюме",
    },
    "Experience": {
        "ru": "Опыт работы",
        "de": "Berufserfahrung",
        "fr": "Expérience",
        "es": "Experiencia",
        "pt": "Experiência",
        "zh": "工作经验",
        "ja": "職歴",
        "ko": "경력",
        "uk": "Досвід роботи",
    },
    "Education": {
        "ru": "Образование",
        "de": "Bildung",
        "fr": "Éducation",
        "es": "Educación",
        "pt": "Educação",
        "zh": "教育背景",
        "ja": "学歴",
        "ko": "학력",
        "uk": "Освіта",
    },
    "Skills": {
        "ru": "Навыки",
        "de": "Fähigkeiten",
        "fr": "Compétences",
        "es": "Habilidades",
        "pt": "Habilidades",
        "zh": "技能",
        "ja": "スキル",
        "ko": "기술",
        "uk": "Навички",
    },
    "Projects": {
        "ru": "Проекты",
        "de": "Projekte",
        "fr": "Projets",
        "es": "Proyectos",
        "pt": "Projetos",
        "zh": "项目",
        "ja": "プロジェクト",
        "ko": "프로젝트",
        "uk": "Проєкти",
    },
    "Contact": {
        "ru": "Контакты",
        "de": "Kontakt",
        "fr": "Contact",
        "es": "Contacto",
        "pt": "Contato",
        "zh": "联系方式",
        "ja": "連絡先",
        "ko": "연락처",
        "uk": "Контакти",
    },
}

DATE_FORMATS: dict[str, str] = {
    "en": "%b %Y",
    "ru": "%b %Y",
    "de": "%m.%Y",
    "fr": "%m/%Y",
    "es": "%m/%Y",
    "pt": "%m/%Y",
    "zh": "%Y年%m月",
    "ja": "%Y年%m月",
    "ko": "%Y년 %m월",
    "uk": "%b %Y",
}


class MultiLanguageCV:
    """Handles multi-language resume generation."""

    def __init__(self, target_language: Language = Language.EN):
        self.target_language = target_language

    def translate_section(self, section_name: str) -> str:
        translations = SECTION_TRANSLATIONS.get(section_name, {})
        return translations.get(self.target_language.value, section_name)

    def get_date_format(self) -> str:
        return DATE_FORMATS.get(self.target_language.value, "%b %Y")

    def format_date(self, date_obj) -> str:
        if date_obj is None:
            return ""
        fmt = self.get_date_format()
        try:
            return date_obj.strftime(fmt)
        except (AttributeError, ValueError):
            return str(date_obj)

    def get_all_sections(self) -> list[str]:
        return list(SECTION_TRANSLATIONS.keys())

    def get_supported_languages(self) -> list[Language]:
        return list(Language)

    def get_language_name(self, lang: Language) -> str:
        names = {
            Language.EN: "English",
            Language.RU: "Русский",
            Language.DE: "Deutsch",
            Language.FR: "Français",
            Language.ES: "Español",
            Language.PT: "Português",
            Language.ZH: "中文",
            Language.JA: "日本語",
            Language.KO: "한국어",
            Language.UK: "Українська",
        }
        return names.get(lang, lang.value)
