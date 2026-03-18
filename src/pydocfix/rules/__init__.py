"""Linting rules for docstrings."""

from __future__ import annotations

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

__all__ = [
    "Applicability",
    "BaseRule",
    "D200",
    "D401",
    "D402",
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
]


def build_registry() -> RuleRegistry:
    """Create a registry populated with built-in rules."""
    registry = RuleRegistry()
    for cls in _BUILTIN_RULES:
        registry.register(cls())
    return registry
