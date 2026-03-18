"""Rule D403 - Docstring parameter name missing '*' or '**' prefix."""

from __future__ import annotations

import ast

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, replace_token


class D403(BaseRule):
    """Docstring parameter name missing '*' or '**' prefix."""

    code = "D403"
    message = "Docstring parameter name missing '*' or '**' prefix."
    target_kinds = {
        SyntaxKind.GOOGLE_ARG,
        SyntaxKind.NUMPY_PARAMETER,
    }

    def _get_vararg_kwarg_names(self, ast_node: ast.AST) -> dict[str, str]:
        """Return mapping of bare name -> prefixed name for *args/**kwargs."""
        if not isinstance(ast_node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return {}
        result: dict[str, str] = {}
        if ast_node.args.vararg:
            result[ast_node.args.vararg.arg] = f"*{ast_node.args.vararg.arg}"
        if ast_node.args.kwarg:
            result[ast_node.args.kwarg.arg] = f"**{ast_node.args.kwarg.arg}"
        return result

    def _find_child_token(self, node: Node, kind: SyntaxKind) -> Token | None:
        for child in node.children:
            if isinstance(child, Token) and child.kind == kind:
                return child
        return None

    def diagnose(self, ctx: DiagnoseContext) -> Diagnostic | None:
        cst_node = ctx.target_cst
        if not isinstance(cst_node, Node):
            return None

        name_token = self._find_child_token(cst_node, SyntaxKind.NAME)
        if name_token is None:
            return None

        param_name = name_token.text
        # Already has prefix — nothing to do
        if param_name.startswith("*"):
            return None

        prefixed_names = self._get_vararg_kwarg_names(ctx.parent_ast)
        expected = prefixed_names.get(param_name)
        if expected is None:
            return None

        fix = Fix(
            edits=[replace_token(name_token, expected)],
            applicability=Applicability.SAFE,
        )
        message = f"Docstring parameter '{param_name}' should be '{expected}'."
        return self._make_diagnostic(ctx, message, fix=fix, target=name_token)
