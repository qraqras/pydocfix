"""Rule PRM008 - Docstring parameter has empty description."""

from __future__ import annotations

import ast
from collections.abc import Iterator

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import BaseRule, DiagnoseContext, Diagnostic


class PRM008(BaseRule):
    """Docstring parameter has no description."""

    code = "PDX-PRM008"
    message = "Docstring parameter has empty description."
    target_kinds = {
        SyntaxKind.GOOGLE_ARG,
        SyntaxKind.NUMPY_PARAMETER,
    }

    @staticmethod
    def _find_child_token(node: Node, kind: SyntaxKind) -> Token | None:
        for child in node.children:
            if isinstance(child, Token) and child.kind == kind:
                return child
        return None

    def diagnose(self, ctx: DiagnoseContext) -> Iterator[Diagnostic]:
        cst_node = ctx.target_cst
        if not isinstance(cst_node, Node):
            return
        if not isinstance(ctx.parent_ast, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return

        name_token = self._find_child_token(cst_node, SyntaxKind.NAME)
        if name_token is None:
            return

        desc_token = self._find_child_token(cst_node, SyntaxKind.DESCRIPTION)
        if desc_token is not None and desc_token.text.strip():
            return

        message = f"Parameter '{name_token.text}' has no description."
        yield self._make_diagnostic(ctx, message, target=name_token)
