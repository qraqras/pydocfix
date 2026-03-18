"""Rule D405 - Docstring has parameter not in function signature."""

from __future__ import annotations

import ast

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, delete_range


def _bare_name(name: str) -> str:
    """Strip leading ``*`` or ``**`` from a parameter name."""
    return name.lstrip("*")


class D405(BaseRule):
    """Docstring documents a parameter that does not exist in the function signature."""

    code = "D405"
    message = "Docstring has parameter not in function signature."
    target_kinds = {
        SyntaxKind.GOOGLE_ARG,
        SyntaxKind.NUMPY_PARAMETER,
    }

    @staticmethod
    def _get_signature_names(func: ast.FunctionDef | ast.AsyncFunctionDef) -> set[str]:
        """Return bare parameter names from the function signature."""
        names: set[str] = set()
        for arg in (*func.args.posonlyargs, *func.args.args, *func.args.kwonlyargs):
            names.add(arg.arg)
        if func.args.vararg:
            names.add(func.args.vararg.arg)
        if func.args.kwarg:
            names.add(func.args.kwarg.arg)
        return names

    @staticmethod
    def _find_child_token(node: Node, kind: SyntaxKind) -> Token | None:
        for child in node.children:
            if isinstance(child, Token) and child.kind == kind:
                return child
        return None

    @staticmethod
    def _build_delete_fix(ds_text: str, param_node: Node) -> Fix:
        """Build a fix that deletes the parameter entry line(s)."""
        ds_bytes = ds_text.encode("utf-8")
        # Delete from the preceding newline to the trailing newline (inclusive)
        nl_before = ds_bytes.rfind(b"\n", 0, param_node.range.start)
        start = nl_before if nl_before != -1 else param_node.range.start
        nl_after = ds_bytes.find(b"\n", param_node.range.end)
        end = nl_after + 1 if nl_after != -1 else param_node.range.end
        return Fix(
            edits=[delete_range(start, end)],
            applicability=Applicability.UNSAFE,
        )

    def diagnose(self, ctx: DiagnoseContext) -> Diagnostic | None:
        cst_node = ctx.target_cst
        if not isinstance(cst_node, Node):
            return None
        if not isinstance(ctx.parent_ast, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return None

        name_token = self._find_child_token(cst_node, SyntaxKind.NAME)
        if name_token is None:
            return None

        sig_names = self._get_signature_names(ctx.parent_ast)
        bare = _bare_name(name_token.text)
        if bare in sig_names:
            return None

        fix = self._build_delete_fix(ctx.docstring_text, cst_node)
        message = f"Parameter '{name_token.text}' not in function signature."
        return self._make_diagnostic(ctx, message, fix=fix, target=name_token)
