"""Tests for linting rules and rule infrastructure."""

from __future__ import annotations

import ast
from pathlib import Path

from pydocstring import Node, SyntaxKind, Token, parse_google, parse_numpy

from pydocfix.rules import (
    D200,
    D401,
    D402,
    D403,
    D404,
    D405,
    D406,
    D407,
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


# ── D403 Tests ───────────────────────────────────────────────────────


class TestD403GoogleParam:
    """D403: parameter name missing prefix in Google-style docstrings."""

    def test_kwargs_missing_prefix(self):
        ds = "Summary.\n\nArgs:\n    kwargs (int): desc.\n"
        func = "def foo(**kwargs: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        assert len(args) == 1
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D403().diagnose(ctx)
        assert diag is not None
        assert diag.rule == "D403"
        assert "'kwargs'" in diag.message
        assert "'**kwargs'" in diag.message

    def test_args_missing_prefix(self):
        ds = "Summary.\n\nArgs:\n    args (int): desc.\n"
        func = "def foo(*args: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D403().diagnose(ctx)
        assert diag is not None
        assert "'args'" in diag.message
        assert "'*args'" in diag.message

    def test_kwargs_with_prefix_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    **kwargs (int): desc.\n"
        func = "def foo(**kwargs: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D403().diagnose(ctx)
        assert diag is None

    def test_args_with_prefix_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    *args (int): desc.\n"
        func = "def foo(*args: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D403().diagnose(ctx)
        assert diag is None

    def test_regular_param_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): desc.\n"
        func = "def foo(x: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D403().diagnose(ctx)
        assert diag is None

    def test_fix_adds_prefix(self):
        ds = "Summary.\n\nArgs:\n    kwargs (int): desc.\n"
        func = "def foo(**kwargs: int):\n    pass\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        ctx = _make_d401_ctx_google(ds, func, args[0])
        diag = D403().diagnose(ctx)
        assert diag is not None
        assert diag.fix.applicability == Applicability.SAFE
        result = apply_edits(ds, diag.fix.edits)
        assert "**kwargs" in result

    def test_non_function_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    kwargs (int): desc.\n"
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
        diag = D403().diagnose(ctx)
        assert diag is None


class TestD403NumpyParam:
    """D403: parameter name missing prefix in NumPy-style docstrings."""

    def test_kwargs_missing_prefix(self):
        ds = "Summary.\n\nParameters\n----------\nkwargs : int\n    desc.\n"
        func = "def foo(**kwargs: int):\n    pass\n"
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
        diag = D403().diagnose(ctx)
        assert diag is not None
        assert "'kwargs'" in diag.message
        assert "'**kwargs'" in diag.message

    def test_kwargs_with_prefix_no_diagnostic(self):
        ds = "Summary.\n\nParameters\n----------\n**kwargs : int\n    desc.\n"
        func = "def foo(**kwargs: int):\n    pass\n"
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
        diag = D403().diagnose(ctx)
        assert diag is None


# ── Helpers for D404 ─────────────────────────────────────────────────


def _make_d404_ctx_google(ds_text: str, func_src: str) -> DiagnoseContext:
    """Build a DiagnoseContext targeting the GOOGLE_SECTION node."""
    parsed = parse_google(ds_text)
    sections = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_SECTION)
    # Pick the first section that contains GOOGLE_ARG children
    section = None
    for s in sections:
        if any(isinstance(c, Node) and c.kind == SyntaxKind.GOOGLE_ARG for c in s.children):
            section = s
            break
    assert section is not None, "No Args section found"
    tree = ast.parse(func_src)
    func_node = tree.body[0]
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=section,
        parent_ast=func_node,
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


def _make_d404_ctx_numpy(ds_text: str, func_src: str) -> DiagnoseContext:
    """Build a DiagnoseContext targeting the NUMPY_SECTION node."""
    parsed = parse_numpy(ds_text)
    sections = _find_cst_nodes(parsed, SyntaxKind.NUMPY_SECTION)
    section = None
    for s in sections:
        if any(isinstance(c, Node) and c.kind == SyntaxKind.NUMPY_PARAMETER for c in s.children):
            section = s
            break
    assert section is not None, "No Parameters section found"
    tree = ast.parse(func_src)
    func_node = tree.body[0]
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=section,
        parent_ast=func_node,
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


# ── D404 Tests ───────────────────────────────────────────────────────


class TestD404GoogleParam:
    """D404: missing parameter in Google-style docstrings."""

    def test_missing_param_detected(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        assert diags[0].rule == "D404"
        assert "'y'" in diags[0].message

    def test_all_documented_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n    y (str): The y.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []

    def test_multiple_missing(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int, y: str, z: float):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 2
        names = {d.message for d in diags}
        assert any("'y'" in m for m in names)
        assert any("'z'" in m for m in names)

    def test_self_excluded(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(self, x: int):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []

    def test_cls_excluded(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(cls, x: int):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []

    def test_varargs_missing(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int, *args: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        assert "'*args'" in diags[0].message

    def test_kwargs_missing(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int, **kwargs: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        assert "'**kwargs'" in diags[0].message

    def test_varargs_documented_with_prefix(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n    *args (str): Extra.\n"
        func = "def foo(x: int, *args: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []

    def test_varargs_documented_without_prefix(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n    args (str): Extra.\n"
        func = "def foo(x: int, *args: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []

    def test_kwonly_missing(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int, *, key: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        assert "'key'" in diags[0].message

    def test_no_annotation(self):
        ds = "Summary.\n\nArgs:\n    x: The x.\n"
        func = "def foo(x, y):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        assert "'y'" in diags[0].message

    def test_fix_inserts_stub_with_type(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        assert diags[0].fix is not None
        assert diags[0].fix.applicability == Applicability.UNSAFE
        result = apply_edits(ds, diags[0].fix.edits)
        assert "y (str):" in result
        assert "y (str): " not in result or result.endswith("y (str):")
        # Original content preserved
        assert "x (int): The x." in result

    def test_fix_inserts_stub_without_type(self):
        ds = "Summary.\n\nArgs:\n    x: The x.\n"
        func = "def foo(x, y):\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        result = apply_edits(ds, diags[0].fix.edits)
        assert "\n    y:" in result

    def test_non_function_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        parsed = parse_google(ds)
        sections = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_SECTION)
        section = sections[0]
        tree = ast.parse("class Foo:\n    pass\n")
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=section,
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(1, 0),
            docstring_location=DocstringLocation(Offset(1, 0), 0, 0, '"""', '"""'),
        )
        diags = D404().diagnose(ctx)
        assert diags == []

    def test_returns_section_ignored(self):
        """D404 should not flag Returns sections."""
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n\nReturns:\n    int: The result.\n"
        func = "def foo(x: int) -> int:\n    pass\n"
        ctx = _make_d404_ctx_google(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []


class TestD404NumpyParam:
    """D404: missing parameter in NumPy-style docstrings."""

    def test_missing_param_detected(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d404_ctx_numpy(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        assert "'y'" in diags[0].message

    def test_all_documented_no_diagnostic(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\ny : str\n    The y.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d404_ctx_numpy(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []

    def test_fix_inserts_numpy_stub_with_type(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d404_ctx_numpy(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        result = apply_edits(ds, diags[0].fix.edits)
        assert "y : str" in result
        assert "Description." not in result

    def test_fix_inserts_numpy_stub_without_type(self):
        ds = "Summary.\n\nParameters\n----------\nx\n    The x.\n"
        func = "def foo(x, y):\n    pass\n"
        ctx = _make_d404_ctx_numpy(ds, func)
        diags = D404().diagnose(ctx)
        assert len(diags) == 1
        result = apply_edits(ds, diags[0].fix.edits)
        assert "\ny" in result

    def test_self_excluded(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\n"
        func = "def foo(self, x: int):\n    pass\n"
        ctx = _make_d404_ctx_numpy(ds, func)
        diags = D404().diagnose(ctx)
        assert diags == []


class TestD404Registry:
    """D404 is registered correctly."""

    def test_registry_contains_d404(self):
        registry = build_registry()
        assert registry.get("D404") is not None

    def test_rules_for_google_section(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.GOOGLE_SECTION)
        assert any(r.code == "D404" for r in rules)

    def test_rules_for_numpy_section(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.NUMPY_SECTION)
        assert any(r.code == "D404" for r in rules)


# ── Helpers for D405 ─────────────────────────────────────────────────


def _make_d405_ctx_google(ds_text: str, func_src: str, arg_index: int = 0) -> DiagnoseContext:
    """Build a DiagnoseContext targeting a GOOGLE_ARG node."""
    parsed = parse_google(ds_text)
    args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
    assert len(args) > arg_index
    tree = ast.parse(func_src)
    func_node = tree.body[0]
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=args[arg_index],
        parent_ast=func_node,
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


def _make_d405_ctx_numpy(ds_text: str, func_src: str, param_index: int = 0) -> DiagnoseContext:
    """Build a DiagnoseContext targeting a NUMPY_PARAMETER node."""
    parsed = parse_numpy(ds_text)
    params = _find_cst_nodes(parsed, SyntaxKind.NUMPY_PARAMETER)
    assert len(params) > param_index
    tree = ast.parse(func_src)
    func_node = tree.body[0]
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=params[param_index],
        parent_ast=func_node,
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


# ── D405 Tests ───────────────────────────────────────────────────────


class TestD405GoogleParam:
    """D405: extra parameter in Google-style docstrings."""

    def test_extra_param_detected(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n    y (str): The y.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d405_ctx_google(ds, func, arg_index=1)
        diag = D405().diagnose(ctx)
        assert diag is not None
        assert diag.rule == "D405"
        assert "'y'" in diag.message

    def test_valid_param_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d405_ctx_google(ds, func, arg_index=0)
        diag = D405().diagnose(ctx)
        assert diag is None

    def test_self_not_in_signature_check(self):
        """self is in func.args but not user-visible; doc param named 'self' should be flagged."""
        ds = "Summary.\n\nArgs:\n    self (Foo): This.\n"
        func = "def foo(self):\n    pass\n"
        ctx = _make_d405_ctx_google(ds, func, arg_index=0)
        # 'self' IS in func.args, so not flagged as extra
        diag = D405().diagnose(ctx)
        assert diag is None

    def test_star_args_with_prefix_valid(self):
        ds = "Summary.\n\nArgs:\n    *args: Extra.\n"
        func = "def foo(*args):\n    pass\n"
        ctx = _make_d405_ctx_google(ds, func, arg_index=0)
        diag = D405().diagnose(ctx)
        assert diag is None

    def test_star_args_without_prefix_valid(self):
        ds = "Summary.\n\nArgs:\n    args: Extra.\n"
        func = "def foo(*args):\n    pass\n"
        ctx = _make_d405_ctx_google(ds, func, arg_index=0)
        diag = D405().diagnose(ctx)
        assert diag is None

    def test_fix_deletes_entry(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n    y (str): The y.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d405_ctx_google(ds, func, arg_index=1)
        diag = D405().diagnose(ctx)
        assert diag is not None
        assert diag.fix is not None
        assert diag.fix.applicability == Applicability.UNSAFE
        result = apply_edits(ds, diag.fix.edits)
        assert "x (int): The x." in result
        assert "y (str)" not in result

    def test_non_function_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        tree = ast.parse("class Foo:\n    pass\n")
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=args[0],
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(1, 0),
            docstring_location=DocstringLocation(Offset(1, 0), 0, 0, '"""', '"""'),
        )
        diag = D405().diagnose(ctx)
        assert diag is None


class TestD405NumpyParam:
    """D405: extra parameter in NumPy-style docstrings."""

    def test_extra_param_detected(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\ny : str\n    The y.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d405_ctx_numpy(ds, func, param_index=1)
        diag = D405().diagnose(ctx)
        assert diag is not None
        assert "'y'" in diag.message

    def test_valid_param_no_diagnostic(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d405_ctx_numpy(ds, func, param_index=0)
        diag = D405().diagnose(ctx)
        assert diag is None

    def test_fix_deletes_entry(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\ny : str\n    The y.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d405_ctx_numpy(ds, func, param_index=1)
        diag = D405().diagnose(ctx)
        assert diag is not None
        result = apply_edits(ds, diag.fix.edits)
        assert "x : int" in result
        assert "y : str" not in result


# ── Helpers for D406 ─────────────────────────────────────────────────


def _make_d406_ctx_google(ds_text: str, func_src: str) -> DiagnoseContext:
    """Build a DiagnoseContext targeting the GOOGLE_DOCSTRING root node."""
    parsed = parse_google(ds_text)
    tree = ast.parse(func_src)
    func_node = tree.body[0]
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=parsed.node,
        parent_ast=func_node,
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


def _make_d406_ctx_numpy(ds_text: str, func_src: str) -> DiagnoseContext:
    """Build a DiagnoseContext targeting the NUMPY_DOCSTRING root node."""
    parsed = parse_numpy(ds_text)
    tree = ast.parse(func_src)
    func_node = tree.body[0]
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=parsed.node,
        parent_ast=func_node,
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


# ── D406 Tests ───────────────────────────────────────────────────────


class TestD406Google:
    """D406: missing Args section in Google-style docstrings."""

    def test_missing_section_detected(self):
        ds = "Summary."
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None
        assert diag.rule == "D406"

    def test_no_params_no_diagnostic(self):
        ds = "Summary."
        func = "def foo():\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is None

    def test_only_self_no_diagnostic(self):
        ds = "Summary."
        func = "def foo(self):\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is None

    def test_has_args_section_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is None

    def test_returns_section_only_still_flagged(self):
        ds = "Summary.\n\nReturns:\n    int: The result.\n"
        func = "def foo(x: int) -> int:\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None

    def test_fix_inserts_section(self):
        ds = "Summary."
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None
        assert diag.fix is not None
        assert diag.fix.applicability == Applicability.UNSAFE
        result = apply_edits(ds, diag.fix.edits)
        assert "Args:" in result
        assert "x (int):" in result
        assert "y (str):" in result
        assert "Description." not in result
        assert "Summary." in result

    def test_fix_without_annotations(self):
        ds = "Summary."
        func = "def foo(x, y):\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None
        result = apply_edits(ds, diag.fix.edits)
        assert "\n    x:" in result
        assert "\n    y:" in result
        assert "Description." not in result

    def test_fix_includes_varargs(self):
        ds = "Summary."
        func = "def foo(x: int, *args: str, **kwargs: bool):\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None
        result = apply_edits(ds, diag.fix.edits)
        assert "*args (str):" in result
        assert "**kwargs (bool):" in result
        assert "Description." not in result

    def test_non_function_no_diagnostic(self):
        ds = "Summary."
        parsed = parse_google(ds)
        tree = ast.parse("class Foo:\n    pass\n")
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=parsed.node,
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(1, 0),
            docstring_location=DocstringLocation(Offset(1, 0), 0, 0, '"""', '"""'),
        )
        diag = D406().diagnose(ctx)
        assert diag is None

    def test_async_function(self):
        ds = "Summary."
        func = "async def foo(x: int):\n    pass\n"
        ctx = _make_d406_ctx_google(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None


class TestD406Numpy:
    """D406: missing Parameters section in NumPy-style docstrings."""

    def test_missing_section_detected(self):
        ds = "Summary."
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d406_ctx_numpy(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None

    def test_has_parameters_section_no_diagnostic(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d406_ctx_numpy(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is None

    def test_fix_inserts_numpy_section(self):
        ds = "Summary."
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d406_ctx_numpy(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is not None
        result = apply_edits(ds, diag.fix.edits)
        assert "Parameters" in result
        assert "----------" in result
        assert "x : int" in result
        assert "y : str" in result

    def test_no_params_no_diagnostic(self):
        ds = "Summary."
        func = "def foo():\n    pass\n"
        ctx = _make_d406_ctx_numpy(ds, func)
        diag = D406().diagnose(ctx)
        assert diag is None


class TestD405D406Registry:
    """D405 and D406 are registered correctly."""

    def test_registry_contains_d405(self):
        registry = build_registry()
        assert registry.get("D405") is not None

    def test_registry_contains_d406(self):
        registry = build_registry()
        assert registry.get("D406") is not None

    def test_d405_rules_for_google_arg(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.GOOGLE_ARG)
        assert any(r.code == "D405" for r in rules)

    def test_d405_rules_for_numpy_parameter(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.NUMPY_PARAMETER)
        assert any(r.code == "D405" for r in rules)

    def test_d406_rules_for_google_docstring(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.GOOGLE_DOCSTRING)
        assert any(r.code == "D406" for r in rules)

    def test_d406_rules_for_numpy_docstring(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.NUMPY_DOCSTRING)
        assert any(r.code == "D406" for r in rules)


# ── Helpers for D407 ─────────────────────────────────────────────────


def _make_d407_ctx_google(ds_text: str, func_src: str, arg_index: int = 0) -> DiagnoseContext:
    parsed = parse_google(ds_text)
    args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
    assert len(args) > arg_index
    tree = ast.parse(func_src)
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=args[arg_index],
        parent_ast=tree.body[0],
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


def _make_d407_ctx_numpy(ds_text: str, func_src: str, param_index: int = 0) -> DiagnoseContext:
    parsed = parse_numpy(ds_text)
    params = _find_cst_nodes(parsed, SyntaxKind.NUMPY_PARAMETER)
    assert len(params) > param_index
    tree = ast.parse(func_src)
    return DiagnoseContext(
        filepath=Path("test.py"),
        docstring_text=ds_text,
        docstring_cst=parsed,
        target_cst=params[param_index],
        parent_ast=tree.body[0],
        docstring_stmt=_dummy_stmt(2, 4),
        docstring_location=DocstringLocation(Offset(2, 7), 0, 0, '"""', '"""'),
    )


# ── D407 Tests ───────────────────────────────────────────────────────


class TestD407GoogleParam:
    """D407: empty description in Google-style docstrings."""

    def test_empty_description_detected(self):
        ds = "Summary.\n\nArgs:\n    x (int):\n    y (str): The y.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d407_ctx_google(ds, func, arg_index=0)
        diag = D407().diagnose(ctx)
        assert diag is not None
        assert diag.rule == "D407"
        assert "'x'" in diag.message

    def test_has_description_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d407_ctx_google(ds, func, arg_index=0)
        diag = D407().diagnose(ctx)
        assert diag is None

    def test_second_arg_empty(self):
        ds = "Summary.\n\nArgs:\n    x (int): The x.\n    y (str):\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d407_ctx_google(ds, func, arg_index=1)
        diag = D407().diagnose(ctx)
        assert diag is not None
        assert "'y'" in diag.message

    def test_no_fix(self):
        ds = "Summary.\n\nArgs:\n    x (int):\n    y (str): The y.\n"
        func = "def foo(x: int, y: str):\n    pass\n"
        ctx = _make_d407_ctx_google(ds, func, arg_index=0)
        diag = D407().diagnose(ctx)
        assert diag is not None
        assert diag.fix is None

    def test_non_function_no_diagnostic(self):
        ds = "Summary.\n\nArgs:\n    x (int):\n"
        parsed = parse_google(ds)
        args = _find_cst_nodes(parsed, SyntaxKind.GOOGLE_ARG)
        tree = ast.parse("class Foo:\n    pass\n")
        ctx = DiagnoseContext(
            filepath=Path("test.py"),
            docstring_text=ds,
            docstring_cst=parsed,
            target_cst=args[0],
            parent_ast=tree.body[0],
            docstring_stmt=_dummy_stmt(1, 0),
            docstring_location=DocstringLocation(Offset(1, 0), 0, 0, '"""', '"""'),
        )
        diag = D407().diagnose(ctx)
        assert diag is None


class TestD407NumpyParam:
    """D407: empty description in NumPy-style docstrings."""

    def test_empty_description_detected(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d407_ctx_numpy(ds, func, param_index=0)
        diag = D407().diagnose(ctx)
        assert diag is not None
        assert "'x'" in diag.message

    def test_has_description_no_diagnostic(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n    The x.\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d407_ctx_numpy(ds, func, param_index=0)
        diag = D407().diagnose(ctx)
        assert diag is None

    def test_no_fix(self):
        ds = "Summary.\n\nParameters\n----------\nx : int\n"
        func = "def foo(x: int):\n    pass\n"
        ctx = _make_d407_ctx_numpy(ds, func, param_index=0)
        diag = D407().diagnose(ctx)
        assert diag is not None
        assert diag.fix is None


class TestD407Registry:
    """D407 is registered correctly."""

    def test_registry_contains_d407(self):
        registry = build_registry()
        assert registry.get("D407") is not None

    def test_rules_for_google_arg(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.GOOGLE_ARG)
        assert any(r.code == "D407" for r in rules)

    def test_rules_for_numpy_parameter(self):
        registry = build_registry()
        rules = registry.rules_for_kind(SyntaxKind.NUMPY_PARAMETER)
        assert any(r.code == "D407" for r in rules)
