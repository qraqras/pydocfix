"""Configuration loader for pydocfix."""

from __future__ import annotations

import logging
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

logger = logging.getLogger(__name__)


@dataclass
class Config:
    """Resolved pydocfix configuration."""

    ignore: list[str] = field(default_factory=list)
    # Period character used for D200 auto-fix. None means fall back to "."
    period: str | None = None


def find_pyproject_toml(start: Path | None = None) -> Path | None:
    """Walk up from *start* (defaults to CWD) to locate pyproject.toml."""
    current = (start or Path.cwd()).resolve()
    for directory in [current, *current.parents]:
        candidate = directory / "pyproject.toml"
        if candidate.is_file():
            return candidate
    return None


def load_config(start: Path | None = None) -> Config:
    """Load ``[tool.pydocfix]`` from the nearest pyproject.toml.

    Falls back to defaults when no file is found or the section is absent.
    """
    toml_path = find_pyproject_toml(start)
    if toml_path is None:
        return Config()

    try:
        with toml_path.open("rb") as fh:
            data = tomllib.load(fh)
    except Exception:
        logger.warning("could not read %s, using defaults", toml_path)
        return Config()

    section: dict = data.get("tool", {}).get("pydocfix", {})

    ignore: list[str] = [str(code) for code in section.get("ignore", [])]
    period: str | None = section.get("period") or None
    return Config(ignore=ignore, period=period)
