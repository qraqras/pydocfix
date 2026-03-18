"""Rule D404 - Missing parameter in docstring."""

from __future__ import annotations

import ast

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, insert_at


def _bare_name(name: str) -> str:
    """Strip leading ``*`` or ``**`` from a parameter name."""
    return name.lstrip("*")


class D404(BaseRule):
    """Docstring has Args/Parameters section but is missing documented parameters."""

    code = "D404"
    message = "Missing parameter in docstring."
    target_kinds = {
        SyntaxKind.GOOGLE_SECTION,
        SyntaxKind.NUMPY_SECTION,
    }

    # -- helpers -------------------------------------------------------

    @staticmethod
    def _is_param_section(section: Node) -> bool:
        """Return True if *section* contains parameter entries."""
        return any(
            isinstance(c, Node) and c.kind in (SyntaxKind.GOOGLE_ARG, SyntaxKind.NUMPY_PARAMETER)
            for c in section.children
        )

    @staticmethod
    def _get_documented_params(section: Node) -> set[str]:
        """Extract bare parameter names documented in the section."""
        names: set[str] = set()
        for child in section.children:
            if not isinstance(child, Node):
                continue
            if child.kind not in (SyntaxKind.GOOGLE_ARG, SyntaxKind.NUMPY_PARAMETER):
                continue
            for token in child.children:
                if isinstance(token, Token) and token.kind == SyntaxKind.NAME:
                    names.add(_bare_name(token.text))
                    break
        return names

    @staticmethod
    def _get_signature_params(
        func: ast.FunctionDef | ast.AsyncFunctionDef,
    ) -> list[tuple[str, str | None]]:
        """Return ``(display_name, annotation_or_None)`` for each parameter, excluding ``self``/``cls``."""
        result: list[tuple[str, str | None]] = []
        all_positional = [*func.args.posonlyargs, *func.args.args]
        skip_first = bool(all_positional) and all_positional[0].arg in ("self", "cls")
        for i, arg in enumerate(all_positional):
            if i == 0 and skip_first:
                continue
            ann = ast.unparse(arg.annotation) if arg.annotation else None
            result.append((arg.arg, ann))
        for arg in func.args.kwonlyargs:
            ann = ast.unparse(arg.annotation) if arg.annotation else None
            result.append((arg.arg, ann))
        if func.args.vararg:
            ann = ast.unparse(func.args.vararg.annotation) if func.args.vararg.annotation else None
            result.append((f"*{func.args.vararg.arg}", ann))
        if func.args.kwarg:
            ann = ast.unparse(func.args.kwarg.annotation) if func.args.kwarg.annotation else None
            result.append((f"**{func.args.kwarg.arg}", ann))
        return result

    @staticmethod
    def _get_indent(ds_text: str, first_param: Node) -> str:
        """Determine the indentation used for the first parameter entry."""
        ds_bytes = ds_text.encode("utf-8")
        nl_pos = ds_bytes.rfind(b"\n", 0, first_param.range.start)
        if nl_pos == -1:
            return ""
        return ds_bytes[nl_pos + 1 : first_param.range.start].decode("utf-8")

    @staticmethod
    def _build_stub(name: str, ann: str | None, *, is_numpy: bool, indent: str) -> str:
        """Build a stub entry string for a missing parameter."""
        if is_numpy:
            header = f"{indent}{name} : {ann}" if ann else f"{indent}{name}"
            return f"\n{header}"
        if ann:
            return f"\n{indent}{name} ({ann}):"
        return f"\n{indent}{name}:"

    # -- entry point ---------------------------------------------------

    def diagnose(self, ctx: DiagnoseContext) -> list[Diagnostic]:
        section = ctx.target_cst
        if not isinstance(section, Node):
            return []
        if not isinstance(ctx.parent_ast, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return []
        if not self._is_param_section(section):
            return []

        documented = self._get_documented_params(section)
        sig_params = self._get_signature_params(ctx.parent_ast)

        # Determine indentation from existing entries
        param_nodes = [
            c
            for c in section.children
            if isinstance(c, Node) and c.kind in (SyntaxKind.GOOGLE_ARG, SyntaxKind.NUMPY_PARAMETER)
        ]
        if not param_nodes:
            return []

        indent = self._get_indent(ctx.docstring_text, param_nodes[0])
        is_numpy = section.kind == SyntaxKind.NUMPY_SECTION
        insert_offset = section.range.end

        diagnostics: list[Diagnostic] = []
        for display_name, ann in sig_params:
            if _bare_name(display_name) in documented:
                continue
            stub = self._build_stub(display_name, ann, is_numpy=is_numpy, indent=indent)
            fix = Fix(
                edits=[insert_at(insert_offset, stub)],
                applicability=Applicability.UNSAFE,
            )
            message = f"Missing parameter '{display_name}' in docstring."
            diagnostics.append(self._make_diagnostic(ctx, message, fix=fix, target=section))
        return diagnostics
