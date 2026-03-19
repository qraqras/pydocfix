"""Rule D408 - Duplicate parameter in docstring."""

from __future__ import annotations

import ast

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import (
    Applicability,
    BaseRule,
    DiagnoseContext,
    Diagnostic,
    Fix,
    delete_range,
)


class D408(BaseRule):
    """Docstring documents a parameter more than once."""

    code = "D408"
    message = "Duplicate parameter in docstring."
    target_kinds = {
        SyntaxKind.GOOGLE_SECTION,
        SyntaxKind.NUMPY_SECTION,
    }

    @staticmethod
    def _is_param_section(section: Node) -> bool:
        return any(
            isinstance(c, Node) and c.kind in (SyntaxKind.GOOGLE_ARG, SyntaxKind.NUMPY_PARAMETER)
            for c in section.children
        )

    @staticmethod
    def _find_child_token(node: Node, kind: SyntaxKind) -> Token | None:
        for child in node.children:
            if isinstance(child, Token) and child.kind == kind:
                return child
        return None

    @staticmethod
    def _build_delete_fix(ds_text: str, param_node: Node) -> Fix:
        ds_bytes = ds_text.encode("utf-8")
        nl_before = ds_bytes.rfind(b"\n", 0, param_node.range.start)
        start = nl_before + 1 if nl_before != -1 else param_node.range.start
        nl_after = ds_bytes.find(b"\n", param_node.range.end)
        end = nl_after + 1 if nl_after != -1 else param_node.range.end
        return Fix(
            edits=[delete_range(start, end)],
            applicability=Applicability.UNSAFE,
        )

    def diagnose(self, ctx: DiagnoseContext) -> list[Diagnostic] | None:
        section = ctx.target_cst
        if not isinstance(section, Node):
            return None
        if not isinstance(ctx.parent_ast, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return None
        if not self._is_param_section(section):
            return None

        seen: dict[str, Node] = {}
        diagnostics: list[Diagnostic] = []

        for child in section.children:
            if not isinstance(child, Node):
                continue
            if child.kind not in (SyntaxKind.GOOGLE_ARG, SyntaxKind.NUMPY_PARAMETER):
                continue

            name_token = self._find_child_token(child, SyntaxKind.NAME)
            if name_token is None:
                continue

            name = name_token.text
            if name in seen:
                fix = self._build_delete_fix(ctx.docstring_text, child)
                message = f"Parameter '{name}' is documented more than once."
                diagnostics.append(self._make_diagnostic(ctx, message, fix=fix, target=name_token))
            else:
                seen[name] = child

        return diagnostics or None
