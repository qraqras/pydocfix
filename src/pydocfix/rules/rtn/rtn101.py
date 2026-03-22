"""Rule RTN101 - Docstring return type does not match type hint."""

from __future__ import annotations

import ast
from collections.abc import Iterator

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, replace_token


class RTN101(BaseRule):
    """Docstring return type does not match type hint."""

    code = "PDX-RTN101"
    message = "Docstring return type does not match type hint."
    target_kinds = {
        SyntaxKind.GOOGLE_RETURNS,
        SyntaxKind.NUMPY_RETURNS,
    }

    def _get_return_annotation(self, ast_node: ast.AST) -> str | None:
        """Return unparsed return type annotation, or None."""
        if not isinstance(ast_node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return None
        if ast_node.returns is None:
            return None
        return ast.unparse(ast_node.returns)

    def _find_child_token(self, node: Node, kind: SyntaxKind) -> Token | None:
        """Find the first child token with the given kind."""
        for child in node.children:
            if isinstance(child, Token) and child.kind == kind:
                return child
        return None

    def diagnose(self, ctx: DiagnoseContext) -> Iterator[Diagnostic]:
        cst_node = ctx.target_cst
        if not isinstance(cst_node, Node):
            return

        ret_type_token = self._find_child_token(cst_node, SyntaxKind.RETURN_TYPE)
        if ret_type_token is None:
            return

        hint_type = self._get_return_annotation(ctx.parent_ast)
        if hint_type is None:
            return

        doc_type = ret_type_token.text
        if doc_type == hint_type:
            return

        fix = Fix(
            edits=[replace_token(ret_type_token, hint_type)],
            applicability=Applicability.UNSAFE,
        )
        message = f"Docstring return type '{doc_type}' does not match type hint '{hint_type}'."
        yield self._make_diagnostic(ctx, message, fix=fix, target=ret_type_token)
