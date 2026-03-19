"""Source file checker — orchestrates parsing and diagnosis."""

from __future__ import annotations

import ast
import logging
import re
from collections.abc import Iterable, Iterator, Sequence
from pathlib import Path
from typing import Final, NamedTuple

import pydocstring
from pydocstring import Style, SyntaxKind

from pydocfix.config import Config
from pydocfix.rules import (
    BaseRule,
    DiagnoseContext,
    Diagnostic,
    DocstringLocation,
    Edit,
    Fix,
    Offset,
    apply_edits,
    is_applicable,
)

logger = logging.getLogger(__name__)


class _DocstringInfo(NamedTuple):
    """Extracted info about a docstring from source."""

    content: str
    parent_node: ast.AST
    stmt: ast.stmt


def _extract_docstrings(source: str, filepath: Path) -> Iterator[_DocstringInfo]:
    """Yield :class:`_DocstringInfo` for every docstring in *source*."""
    try:
        tree: Final = ast.parse(source, filename=str(filepath))
    except SyntaxError:
        logger.warning(f"{filepath}: could not parse (syntax error), skipping")
        return

    for node in ast.walk(tree):
        try:
            docstr: Final = ast.get_docstring(node, clean=False)  # type: ignore
        except TypeError:
            continue  # node cannot have a docstring

        if docstr is None:
            continue  # node has no docstring

        yield _DocstringInfo(
            docstr,
            node,
            node.body[0],  # type: ignore
        )


_REGEX_OPENING_QUOTES: Final = re.compile(r'([rRuUfFbB]{0,2})("""|\'\'\'|"|\')')


def _locate_docstring(
    ds_stmt: ast.stmt,
    lines: Sequence[str],
    line_offsets: Sequence[int],
    source_bytes: bytes,
) -> DocstringLocation | None:
    """Compute all positional info for a docstring expression at once.

    Returns None if the opening quote cannot be located.
    """
    start_line: Final = lines[ds_stmt.lineno - 1]
    matched: Final = _REGEX_OPENING_QUOTES.match(start_line, pos=ds_stmt.col_offset)
    if matched is None:
        logger.warning("could not locate opening quote at line %d", ds_stmt.lineno)
        return None

    quote_chars: Final = matched.group(2)

    # Byte range of the entire expression in source
    byte_start = line_offsets[ds_stmt.lineno - 1] + ds_stmt.col_offset
    byte_end = line_offsets[ds_stmt.end_lineno - 1] + ds_stmt.end_col_offset

    # Extract closing quote directly from pre-encoded source bytes
    closing: Final = source_bytes[byte_end - len(quote_chars) : byte_end].decode("ascii")

    return DocstringLocation(
        content_offset=Offset(ds_stmt.lineno, matched.end()),
        byte_start=byte_start,
        byte_end=byte_end,
        opening=matched.group(0),
        closing=closing,
    )


def build_rules_map(rules: Iterable[BaseRule]) -> dict[SyntaxKind, list[BaseRule]]:
    """Build cst->rules dispatch map."""
    kind_map: dict[SyntaxKind, list[BaseRule]] = {}
    for rule in rules:
        for kind in rule.target_kinds:
            kind_map.setdefault(kind, []).append(rule)
    return kind_map


def _has_overlap(accepted: Iterable[Edit], candidate: Fix) -> bool:
    """Return True if any edit in *candidate* overlaps with *accepted* edits."""
    for new in candidate.edits:
        for existing in accepted:
            if new.start < existing.end and existing.start < new.end:
                return True
    return False


def check_file(
    source: str,
    filepath: Path,
    kind_map: dict[SyntaxKind, list[BaseRule]],
    *,
    fix: bool = False,
    unsafe_fixes: bool = False,
    config: Config | None = None,
) -> tuple[list[Diagnostic], str | None, frozenset[int]]:
    """Diagnose and optionally fix all docstrings in one pass.

    Returns (all_diagnostics, fixed_source_or_none, indices_of_fixed_diagnostics).
    """
    lines: Final = source.splitlines(keepends=True)
    source_bytes: Final = source.encode("utf-8")
    # Build line-start byte-offset table by scanning the byte string once
    line_offsets: Final[list[int]] = [0]
    pos = -1
    while (pos := source_bytes.find(b"\n", pos + 1)) != -1:
        line_offsets.append(pos + 1)
    all_diagnostics: list[Diagnostic] = []
    fixed_indices: set[int] = set()
    file_edits: list[tuple[int, int, bytes]] = []

    for ds_content, parent_ast, ds_stmt in _extract_docstrings(source, filepath):
        style = pydocstring.detect_style(ds_content)
        parsed = pydocstring.parse_numpy(ds_content) if style == Style.NUMPY else pydocstring.parse_google(ds_content)

        # Determine where the docstring content starts (after opening triple-quote).
        ds_loc = _locate_docstring(ds_stmt, lines, line_offsets, source_bytes)
        if ds_loc is None:
            continue

        # Walk the CST and dispatch to matching rules
        ds_diagnostics: list[Diagnostic] = []
        for cst in pydocstring.walk(parsed.node):
            matching_rules = kind_map.get(cst.kind, [])
            if not matching_rules:
                continue
            ctx = DiagnoseContext(
                filepath=filepath,
                docstring_text=ds_content,
                docstring_cst=parsed,
                target_cst=cst,
                parent_ast=parent_ast,
                docstring_stmt=ds_stmt,
                docstring_location=ds_loc,
                config=config,
            )
            for rule in matching_rules:
                result = rule.diagnose(ctx)
                if result is not None:
                    if isinstance(result, list):
                        ds_diagnostics.extend(result)
                    else:
                        ds_diagnostics.append(result)

        base_idx = len(all_diagnostics)
        all_diagnostics.extend(ds_diagnostics)

        # Fix phase (per-docstring)
        if fix and ds_diagnostics:
            accepted_edits: list[Edit] = []
            for i, d in enumerate(ds_diagnostics):
                if not is_applicable(d, unsafe_fixes):
                    continue
                assert d.fix is not None
                if _has_overlap(accepted_edits, d.fix):
                    logger.warning(
                        "skipping fix from rule %s (overlapping edits)",
                        d.rule,
                    )
                    continue
                accepted_edits.extend(d.fix.edits)
                fixed_indices.add(base_idx + i)

            if accepted_edits:
                new_raw = apply_edits(ds_content, accepted_edits)
                file_edits.append(
                    (
                        ds_loc.byte_start,
                        ds_loc.byte_end,
                        (ds_loc.opening + new_raw + ds_loc.closing).encode("utf-8"),
                    )
                )

    fixed_source: str | None = None
    if file_edits:
        file_edits.sort(key=lambda e: e[0], reverse=True)
        buf = source_bytes
        for start, end, replacement in file_edits:
            buf = buf[:start] + replacement + buf[end:]
        fixed_source = buf.decode("utf-8")

    return all_diagnostics, fixed_source, frozenset(fixed_indices)
