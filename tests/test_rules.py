"""Tests for linting rules and rule infrastructure."""

from __future__ import annotations

import ast
from pathlib import Path

from pydocstring import Node, SyntaxKind, Token, parse_google, parse_numpy

from pydocfix.rules import (
    D200,
    D401,
    D402,
    Applicability,
    DiagnoseContext,
    Diagnostic,
    DocstringLocation,
    Edit,
    Fix,
    Offset,
    Range,
    apply_edits,
    build_registry,
)


def _dummy_stmt(lineno: int = 1, col_offset: int = 0) -> ast.stmt:
    """Create a minimal ast.stmt with the given position."""
    node = ast.Expr(value=ast.Constant(value=""))
    node.lineno = lineno
    node.col_offset = col_offset
    node.end_lineno = lineno
    node.end_col_offset = col_offset
    return node


def _make_diagnose_ctx(raw: str) -> DiagnoseContext:
    """Create a DiagnoseContext with the SUMMARY token as cst_node."""
    parsed = parse_google(raw)
    # Find the SUMMARY token in the CST
    summary_token = parsed.summary
    if summary_token is None:
        # Create a dummy token for empty docstrings
        summary_token = Token(kind="SUMMARY", text="", start=0, end=0)
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=raw,
        docstring_cst=parsed,
        target_cst=summary_token,
        parent_ast=ast.parse("pass").body[0],
        docstring_stmt=_dummy_stmt(1, 0),
        docstring_location=DocstringLocation(Offset(1, 0), 0, len(raw) + 6, '"""', '"""'),
    )


class TestEdit:
    def test_apply_single_insert(self):
        result = apply_edits("abcdef", [Edit(start=3, end=3, new_text="X")])
        assert result == "abcXdef"

    def test_apply_single_replace(self):
        result = apply_edits("abcdef", [Edit(start=1, end=4, new_text="XY")])
        assert result == "aXYef"

    def test_apply_multiple_non_overlapping(self):
        edits = [
            Edit(start=0, end=1, new_text="X"),
            Edit(start=3, end=4, new_text="Y"),
        ]
        result = apply_edits("abcdef", edits)
        assert result == "XbcYef"

    def test_apply_overlapping_raises(self):
        edits = [
            Edit(start=1, end=4, new_text="X"),
            Edit(start=3, end=5, new_text="Y"),
        ]
        try:
            apply_edits("abcdef", edits)
            assert False, "Should have raised ValueError"
        except ValueError:
            pass


class TestDiagnostic:
    def test_fixable_when_fix_present(self):
        d = Diagnostic(
            rule="D200",
            message="test",
            filepath="test.py",
            range=Range(Offset(1, 0), Offset(1, 10)),
            fix=Fix(edits=[], applicability=Applicability.SAFE),
        )
        assert d.fixable is True

    def test_not_fixable_when_no_fix(self):
        d = Diagnostic(
            rule="D200",
            message="test",
            filepath="test.py",
            range=Range(Offset(1, 0), Offset(1, 10)),
        )
        assert d.fixable is False

    def test_line_and_col_properties(self):
        d = Diagnostic(
            rule="D200",
            message="test",
            filepath="test.py",
            range=Range(Offset(5, 4), Offset(7, 0)),
        )
        assert d.lineno == 5
        assert d.col == 4


class TestD200:
    def test_missing_period(self):
        ctx = _make_diagnose_ctx("Do something")
        diag = D200().diagnose(ctx)
        assert diag is not None
        assert diag.rule == "D200"
        assert diag.fixable is True
        assert diag.fix is not None

    def test_has_period(self):
        ctx = _make_diagnose_ctx("Do something.")
        diag = D200().diagnose(ctx)
        assert diag is None

    def test_empty_summary(self):
        # Empty docstrings have no SUMMARY token in the CST,
        # so D200 is never dispatched. Tested via checker integration.
        pass

    def test_fix_returns_edit(self):
        raw = "Do something"
        ctx = _make_diagnose_ctx(raw)
        diag = D200().diagnose(ctx)
        assert diag is not None
        assert diag.fix is not None
        result = apply_edits(raw, diag.fix.edits)
        assert result == "Do something."

    def test_fix_no_change_when_period_exists(self):
        raw = "Do something."
        ctx = _make_diagnose_ctx(raw)
        diag = D200().diagnose(ctx)
        assert diag is None

    def test_fix_preserves_surrounding_text(self):
        raw = "Do something\n\n    Args:\n        x: val.\n"
        ctx = _make_diagnose_ctx(raw)
        diag = D200().diagnose(ctx)
        assert diag is not None
        assert diag.fix is not None
        result = apply_edits(raw, diag.fix.edits)
        assert "Do something." in result
        assert "    Args:\n        x: val.\n" in result


class TestRegistry:
    def test_build_registry_contains_d200(self):
        registry = build_registry()
        assert registry.get("D200") is not None

    def test_build_registry_contains_d401(self):
        registry = build_registry()
        assert registry.get("D401") is not None

    def test_build_registry_contains_d402(self):
        registry = build_registry()
        assert registry.get("D402") is not None

    def test_all_rules(self):
        registry = build_registry()
        rules = registry.all_rules()
        codes = [r.code for r in rules]
        assert "D200" in codes
        assert "D401" in codes
        assert "D402" in codes

    def test_rules_for_kind(self):
        registry = build_registry()
        summary_rules = registry.rules_for_kind(SyntaxKind.SUMMARY)
        assert any(r.code == "D200" for r in summary_rules)
        assert registry.rules_for_kind(SyntaxKind.COLON) == []
        google_arg_rules = registry.rules_for_kind(SyntaxKind.GOOGLE_ARG)
        assert any(r.code == "D401" for r in google_arg_rules)
        google_return_rules = registry.rules_for_kind(SyntaxKind.GOOGLE_RETURNS)
        assert any(r.code == "D402" for r in google_return_rules)


# ── Helpers for D401 ─────────────────────────────────────────────────


def _make_d401_ctx_google(
    ds_text: str,
    func_src: str,
    cst_node: Node | Token,
) -> DiagnoseContext:
    """Build a DiagnoseContext for D401 tests (Google style)."""
    parsed = parse_google(ds_text)
    tree = ast.parse(func_src)
    func_node = tree.body[0]
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=cst_node,
        parent_ast=func_node,
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


def _find_cst_nodes(parsed, kind: SyntaxKind) -> list[Node]:
    """Find all CST nodes of a given kind."""
    results: list[Node] = []

    def _walk(n):
        if hasattr(n, "children"):
            if n.kind == kind:
                results.append(n)
            for c in n.children:
                _walk(c)
        elif isinstance(n, Token) and n.kind == kind:
            results.append(n)

    _walk(parsed.node)
    return results


# ── D401 Tests ───────────────────────────────────────────────────────


class TestD401GoogleParam:
    """D401: parameter type mismatch in Google-style docstrings."""

    def test_mismatch_detected(self):
        ds = "Summary.\n\nArgs:\n    x (str): The x value.\n"
        func = "def foo(x: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        assert len(args) == 1
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D401().diagnose(ctx)
        assert diag is not None
        assert diag.rule == "D401"
        assert "'str'" in diag.message
        assert "'int'" in diag.message
        assert "'x'" in diag.message

    def test_matching_types_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x value.\n"
        func = "def foo(x: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D401().diagnose(ctx)
        assert diag is None

    def test_no_annotation_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (str): The x value.\n"
        func = "def foo(x):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D401().diagnose(ctx)
        assert diag is None

    def test_no_doc_type_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x: The x value.\n"
        func = "def foo(x: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D401().diagnose(ctx)
        assert diag is None

    def test_fix_replaces_type(self):
        ds = "Summary.\n\nArgs:\n    x (str): The x value.\n"
        func = "def foo(x: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D401().diagnose(ctx)
        assert diag is not None
        assert diag.fix is not None
        result = apply_edits(ds, diag.fix.edits)
        assert "(int)" in result

    def test_complex_type(self):
        ds = "Summary.\n\nArgs:\n    items (list): The items.\n"
        func = "def foo(items: list[int]):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D401().diagnose(ctx)
        assert diag is not None
        result = apply_edits(ds, diag.fix.edits)
        assert "(list[int])" in result

    def test_multiple_params(self):
        ds = "Summary.\n\nArgs:\n    x (str): X.\n    y (int): Y.\n"
        func = "def foo(x: int, y: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        assert len(args) == 2
        # x: str vs int → mismatch
        ctx0 = _make_d401_ctx_google(ds, func, args[0])
        diag0 = D401().diagnose(ctx0)
        assert diag0 is not None
        assert "'x'" in diag0.message
        # y: int vs int → match
        ctx1 = _make_d401_ctx_google(ds, func, args[1])
        diag1 = D401().diagnose(ctx1)
        assert diag1 is None

    def test_class_method_skipped(self):
        """Non-function AST nodes (e.g. class) should not crash."""
        ds = "Summary.\n\nArgs:\n    x (int): X.\n"
        func = "class Foo:\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        tree = ast.parse(func)
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=args[0],
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(1, 0),
            docstring_location=DocstringLocation(Offset(1, 0), 0, 0, '"""', '"""'),
        )
        diag = D401().diagnose(ctx)
        assert diag is None


class TestD402GoogleReturn:
    """D402: return type mismatch in Google-style docstrings."""

    def test_mismatch_detected(self):
        ds = "Summary.\n\nReturns:\n    str: The result.\n"
        func = "def foo() -> int:\n    pass\n"
        parsed = parse_google(ds)
        rets = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_RETURNS)
        assert len(rets) == 1
        ctx = _make_d401_ctx_google(ds, func, rets[0])
        diag = D402().diagnose(ctx)
        assert diag is not None
        assert diag.rule == "D402"
        assert "'str'" in diag.message
        assert "'int'" in diag.message

    def test_matching_return_no_diagnostic(self):
        ds = "Summary.\n\nReturns:\n    int: The result.\n"
        func = "def foo() -> int:\n    pass\n"
        parsed = parse_google(ds)
        rets = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_RETURNS)
        ctx = _make_d401_ctx_google(ds, func, rets[0])
        diag = D402().diagnose(ctx)
        assert diag is None

    def test_no_return_annotation_no_diagnostic(self):
        ds = "Summary.\n\nReturns:\n    int: The result.\n"
        func = "def foo():\n    pass\n"
        parsed = parse_google(ds)
        rets = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_RETURNS)
        ctx = _make_d401_ctx_google(ds, func, rets[0])
        diag = D402().diagnose(ctx)
        assert diag is None

    def test_fix_replaces_return_type(self):
        ds = "Summary.\n\nReturns:\n    str: The result.\n"
        func = "def foo() -> bool:\n    pass\n"
        parsed = parse_google(ds)
        rets = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_RETURNS)
        ctx = _make_d401_ctx_google(ds, func, rets[0])
        diag = D402().diagnose(ctx)
        assert diag is not None
        result = apply_edits(ds, diag.fix.edits)
        assert "bool:" in result


class TestD401Numpy:
    """D401: type mismatch in NumPy-style docstrings."""

    def test_param_mismatch(self):
        ds = "Summary.\n\nParameters\n----------\nx : str\n    The x.\n"
        func = "def foo(x: int):\n    pass\n"
        parsed = parse_numpy(ds)
        params = _find_cst_nodes(parsed, SyntaxKind.NUMPY_PARAMETER)
        assert len(params) == 1
        tree = ast.parse(func)
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=params[0],
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(2, 4),
            docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
        )
        diag = D401().diagnose(ctx)
        assert diag is not None
        assert "'str'" in diag.message
        assert "'int'" in diag.message

    def test_return_mismatch(self):
        ds = "Summary.\n\nReturns\n-------\nstr\n    The result.\n"
        func = "def foo() -> int:\n    pass\n"
        parsed = parse_numpy(ds)
        rets = _find_cst_nodes(parsed, SyntaxKind.NUMPY_RETURNS)
        assert len(rets) == 1
        tree = ast.parse(func)
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=rets[0],
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(2, 4),
            docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
        )
        diag = D402().diagnose(ctx)
        assert diag is not None
        assert "'str'" in diag.message
        assert "'int'" in diag.message

    def test_param_match_no_diagnostic(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\n"
        func = "def foo(x: int):\n    pass\n"
        parsed = parse_numpy(ds)
        params = _find_cst_nodes(parsed, SyntaxKind.NUMPY_PARAMETER)
        tree = ast.parse(func)
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=params[0],
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(2, 4),
            docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
        )
        diag = D401().diagnose(ctx)
        assert diag is None
