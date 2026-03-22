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

# --- Parameter rules ---
from pydocfix.rules.prm.prm001 import PRM001
from pydocfix.rules.prm.prm004 import PRM004
from pydocfix.rules.prm.prm005 import PRM005
from pydocfix.rules.prm.prm006 import PRM006
from pydocfix.rules.prm.prm007 import PRM007
from pydocfix.rules.prm.prm008 import PRM008
from pydocfix.rules.prm.prm009 import PRM009
from pydocfix.rules.prm.prm101 import PRM101

# --- Return rules ---
from pydocfix.rules.rtn.rtn101 import RTN101

# --- Summary rules ---
from pydocfix.rules.sum.sum002 import SUM002

__all__ = [
    "Applicability",
    "BaseRule",
    # **** RULES ****
    # prm
    "PRM001",
    "PRM004",
    "PRM005",
    "PRM006",
    "PRM007",
    "PRM008",
    "PRM009",
    "PRM101",
    # rtn
    "RTN101",
    # sum
    "SUM002",
    # **** FRAMEWORK ****
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
    SUM002,
    PRM001,
    PRM004,
    PRM005,
    PRM006,
    PRM007,
    PRM008,
    PRM009,
    PRM101,
    RTN101,
]


def build_registry(
    ignore: list[str] | None = None,
    select: list[str] | None = None,
    config: Config | None = None,
) -> RuleRegistry:
    """Create a registry populated with built-in rules.

    Args:
        ignore: Rule codes to exclude (e.g. ``["PDX-SUM002", "PDX-PRM101"]``).
        select: Rule codes to explicitly enable. ``["ALL"]`` enables every rule
            including those with ``enabled_by_default = False``.  When empty,
            only rules whose ``enabled_by_default`` is ``True`` are active.
        config: Resolved configuration passed to each rule instance.
    """
    ignored: frozenset[str] = frozenset(ignore or [])
    selected: frozenset[str] = frozenset(select or [])
    select_all: bool = "ALL" in selected
    has_select: bool = bool(selected)
    registry = RuleRegistry()
    for cls in _BUILTIN_RULES:
        instance = cls(config)
        if instance.code in ignored:
            continue
        if select_all:
            registry.register(instance)
        elif has_select:
            if instance.code in selected:
                registry.register(instance)
        elif instance.enabled_by_default:
            registry.register(instance)
    return registry
