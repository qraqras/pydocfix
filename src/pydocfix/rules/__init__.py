"""Linting rules for docstrings."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydocfix.config import Config

from pydocfix.rules._base import (
    Applicability,
    BaseRule,
    DiagnoseContext,
    Diagnostic,
    DocstringLocation,
    Edit,
    Fix,
    Offset,
    Range,
    RuleRegistry,
    Severity,
    apply_edits,
    delete_range,
    insert_at,
    is_applicable,
    replace_token,
)
from pydocfix.rules.d200 import D200
from pydocfix.rules.d401 import D401
from pydocfix.rules.d402 import D402
from pydocfix.rules.d403 import D403
from pydocfix.rules.d404 import D404
from pydocfix.rules.d405 import D405
from pydocfix.rules.d406 import D406
from pydocfix.rules.d407 import D407
from pydocfix.rules.d408 import D408
from pydocfix.rules.d409 import D409

__all__ = [
    "Applicability",
    "BaseRule",
    "D200",
    "D401",
    "D402",
    "D403",
    "D404",
    "D405",
    "D406",
    "D407",
    "D408",
    "D409",
    "DiagnoseContext",
    "Diagnostic",
    "DocstringLocation",
    "Edit",
    "Fix",
    "Offset",
    "Range",
    "RuleRegistry",
    "Severity",
    "apply_edits",
    "build_registry",
    "delete_range",
    "insert_at",
    "is_applicable",
    "replace_token",
]

_BUILTIN_RULES: list[type[BaseRule]] = [
    D200,
    D401,
    D402,
    D403,
    D404,
    D405,
    D406,
    D407,
    D408,
    D409,
]


def build_registry(ignore: list[str] | None = None, config: Config | None = None) -> RuleRegistry:
    """Create a registry populated with built-in rules.

    Args:
        ignore: Rule codes to exclude (e.g. ``["D200", "D401"]``).
        config: Resolved configuration passed to each rule instance.
    """
    ignored: frozenset[str] = frozenset(ignore or [])
    registry = RuleRegistry()
    for cls in _BUILTIN_RULES:
        instance = cls(config)
        if instance.code not in ignored:
            registry.register(instance)
    return registry
