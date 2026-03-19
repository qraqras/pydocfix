"""Shared infrastructure for linting rules."""

from __future__ import annotations

import ast
import enum
from collections import defaultdict
from collections.abc import Iterable
from dataclasses import dataclass, field
from itertools import pairwise
from typing import TYPE_CHECKING, Final, NamedTuple

from pydocstring import (
    GoogleDocstring,
    Node,
    NumPyDocstring,
    SyntaxKind,
    Token,
)

if TYPE_CHECKING:
    from pathlib import Path

    from pydocfix.config import Config


class Severity(enum.Enum):
    """Severity level for diagnostics."""

    ERROR = "error"
    WARNING = "warning"
    HINT = "hint"


class Applicability(enum.Enum):
    """Whether a fix can be applied safely."""

    SAFE = "safe"
    UNSAFE = "unsafe"
    DISPLAY_ONLY = "display-only"


@dataclass(frozen=True)
class Offset:
    """A source position (1-based line, 0-based column)."""

    lineno: int
    col: int


@dataclass(frozen=True)
class Range:
    """Source location range (1-based lines, 0-based columns)."""

    start: Offset
    end: Offset


@dataclass(frozen=True)
class Edit:
    """A single text replacement within a docstring.

    Offsets are byte positions relative to the start of the docstring
    (including the opening triple-quotes).
    """

    start: int
    end: int
    new_text: str


@dataclass(frozen=True)
class Fix:
    """A set of edits that fix a diagnostic."""

    edits: list[Edit]
    applicability: Applicability


@dataclass(frozen=True)
class Diagnostic:
    """A single issue found in a docstring, optionally bundled with a fix."""

    rule: str
    message: str
    filepath: str
    range: Range
    docstring_line: int = 0
    severity: Severity = Severity.WARNING
    fix: Fix | None = None

    @property
    def fixable(self) -> bool:
        return self.fix is not None

    @property
    def lineno(self) -> int:
        return self.range.start.lineno

    @property
    def col(self) -> int:
        return self.range.start.col


class DocstringLocation(NamedTuple):
    """Pre-computed positional info for a docstring expression."""

    content_offset: Offset  # where content begins (after opening quote)
    byte_start: int  # byte offset of expression start in source
    byte_end: int  # byte offset of expression end in source
    opening: str  # opening quote string (including prefix like r, u)
    closing: str  # closing quote string


@dataclass
class DiagnoseContext:
    """Information passed to a rule's diagnose method."""

    filepath: Path
    docstring_text: str
    docstring_cst: GoogleDocstring | NumPyDocstring
    target_cst: Node | Token
    parent_ast: ast.AST
    docstring_stmt: ast.stmt
    docstring_location: DocstringLocation
    config: Config | None = None
    config: Config | None = None

    def cst_node_range(self, node: Node | Token | None = None) -> Range:
        """Convert a CST node/token byte range to a file-level Range."""
        if node is None:
            node = self.target_cst
        ds_offset = self.docstring_location.content_offset
        start_lc = self.docstring_cst.line_col(node.range.start)
        end_lc = self.docstring_cst.line_col(node.range.end)
        return Range(
            start=Offset(
                ds_offset.lineno + start_lc.lineno - 1,
                ds_offset.col + start_lc.col if start_lc.lineno == 1 else start_lc.col,
            ),
            end=Offset(
                ds_offset.lineno + end_lc.lineno - 1,
                ds_offset.col + end_lc.col if end_lc.lineno == 1 else end_lc.col,
            ),
        )


def replace_token(token: Token, new_text: str) -> Edit:
    """Replace a token's entire text."""
    return Edit(start=token.range.start, end=token.range.end, new_text=new_text)


def insert_at(offset: int, text: str) -> Edit:
    """Insert text at a byte offset (no deletion)."""
    return Edit(start=offset, end=offset, new_text=text)


def delete_range(start: int, end: int) -> Edit:
    """Delete a byte range."""
    return Edit(start=start, end=end, new_text="")


def apply_edits(source: str, edits: Iterable[Edit]) -> str:
    """Apply Edits to a docstring, in reverse-offset order.

    Edit offsets are UTF-8 byte positions (as returned by pydocstring-rs).
    """
    sorted_edits: Final = sorted(edits, key=lambda e: e.start, reverse=True)
    # Validate no overlaps
    for prev, curr in pairwise(sorted_edits):
        if curr.end > prev.start:
            msg = f"Overlapping edits: [{curr.start}:{curr.end}] and [{prev.start}:{prev.end}]"
            raise ValueError(msg)
    buf = source.encode("utf-8")
    for edit in sorted_edits:
        buf = buf[: edit.start] + edit.new_text.encode("utf-8") + buf[edit.end :]
    return buf.decode("utf-8")


def is_applicable(diag: Diagnostic, unsafe_fixes: bool) -> bool:
    """Return True if the diagnostic's fix should be applied."""
    if diag.fix is None:
        return False
    if diag.fix.applicability == Applicability.SAFE:
        return True
    return diag.fix.applicability == Applicability.UNSAFE and unsafe_fixes


class BaseRule:
    """Base class for all linting rules."""

    code: str = ""
    message: str = ""
    target_kinds: set[SyntaxKind] = set()

    def __init__(self, config: Config | None = None) -> None:
        self.config = config

    def diagnose(self, ctx: DiagnoseContext) -> Diagnostic | list[Diagnostic] | None:
        raise NotImplementedError

    # Helper -----------------------------------------------------------

    def _make_diagnostic(
        self,
        ctx: DiagnoseContext,
        message: str,
        *,
        fix: Fix | None = None,
        target: Node | Token | None = None,
    ) -> Diagnostic:
        return Diagnostic(
            rule=self.code,
            message=message,
            filepath=str(ctx.filepath),
            range=ctx.cst_node_range(target),
            docstring_line=ctx.docstring_stmt.lineno,
            fix=fix,
        )


@dataclass
class RuleRegistry:
    """Manages available rules."""

    _rules: dict[str, BaseRule] = field(default_factory=dict)
    _by_kind: dict[SyntaxKind, list[BaseRule]] = field(default_factory=lambda: defaultdict(list))

    def register(self, rule: BaseRule) -> None:
        self._rules[rule.code] = rule
        for kind in rule.target_kinds:
            self._by_kind[kind].append(rule)

    def get(self, code: str) -> BaseRule | None:
        return self._rules.get(code)

    def rules_for_kind(self, kind: SyntaxKind) -> list[BaseRule]:
        return self._by_kind.get(kind, [])

    def all_rules(self) -> list[BaseRule]:
        return list(self._rules.values())

    @property
    def kind_map(self) -> dict[SyntaxKind, list[BaseRule]]:
        return dict(self._by_kind)
