"""Rule D401 - Docstring parameter type does not match type hint."""

from __future__ import annotations

import ast

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, replace_token


class D401(BaseRule):
    """Docstring parameter type does not match type hint."""

    code = "D401"
    message = "Docstring parameter type does not match type hint."
    target_kinds = {
        SyntaxKind.GOOGLE_ARG,
        SyntaxKind.NUMPY_PARAMETER,
    }

    def __init__(self):
        self._ann_cache: tuple[int, dict[str, str]] = (0, {})

    def _get_annotation_map(self, ast_node: ast.AST) -> dict[str, str]:
        """Build a mapping of parameter name -> unparsed type annotation."""
        node_id = id(ast_node)
        if self._ann_cache[0] == node_id:
            return self._ann_cache[1]
        if not isinstance(ast_node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return {}
        result: dict[str, str] = {}
        for arg in (
            *ast_node.args.args,
            *ast_node.args.posonlyargs,
            *ast_node.args.kwonlyargs,
        ):
            if arg.annotation is not None:
                result[arg.arg] = ast.unparse(arg.annotation)
        if ast_node.args.vararg and ast_node.args.vararg.annotation is not None:
            ann = ast.unparse(ast_node.args.vararg.annotation)
            name = ast_node.args.vararg.arg
            result[name] = ann
            result[f"*{name}"] = ann
        if ast_node.args.kwarg and ast_node.args.kwarg.annotation is not None:
            ann = ast.unparse(ast_node.args.kwarg.annotation)
            name = ast_node.args.kwarg.arg
            result[name] = ann
            result[f"**{name}"] = ann
        self._ann_cache = (node_id, result)
        return result

    def _find_child_token(self, node: Node, kind: SyntaxKind) -> Token | None:
        """Find the first child token with the given kind."""
        for child in node.children:
            if isinstance(child, Token) and child.kind == kind:
                return child
        return None

    def diagnose(self, ctx: DiagnoseContext) -> Diagnostic | None:
        cst_node = ctx.target_cst
        if not isinstance(cst_node, Node):
            return None

        name_token = self._find_child_token(cst_node, SyntaxKind.NAME)
        type_token = self._find_child_token(cst_node, SyntaxKind.TYPE)
        if name_token is None or type_token is None:
            return None

        ann_map = self._get_annotation_map(ctx.parent_ast)
        param_name = name_token.text
        hint_type = ann_map.get(param_name)
        if hint_type is None:
            return None

        doc_type = type_token.text
        if doc_type == hint_type:
            return None

        fix = Fix(
            edits=[replace_token(type_token, hint_type)],
            applicability=Applicability.UNSAFE,
        )
        message = f"Docstring type '{doc_type}' does not match type hint '{hint_type}' for parameter '{param_name}'."
        return self._make_diagnostic(ctx, message, fix=fix, target=type_token)
