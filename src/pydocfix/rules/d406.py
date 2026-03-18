"""Rule D406 - Function has parameters but docstring has no Args/Parameters section."""

from __future__ import annotations

import ast

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, insert_at


class D406(BaseRule):
    """Function has parameters but docstring has no Args/Parameters section."""

    code = "D406"
    message = "Missing Args/Parameters section in docstring."
    target_kinds = {
        SyntaxKind.GOOGLE_DOCSTRING,
        SyntaxKind.NUMPY_DOCSTRING,
    }

    # -- helpers -------------------------------------------------------

    @staticmethod
    def _get_signature_params(
        func: ast.FunctionDef | ast.AsyncFunctionDef,
    ) -> list[tuple[str, str | None]]:
        """Return ``(display_name, annotation_or_None)`` excluding ``self``/``cls``."""
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
    def _has_param_section(root: Node) -> bool:
        """Return True if the docstring already contains a parameter section."""
        for child in root.children:
            if not isinstance(child, Node):
                continue
            if child.kind not in (SyntaxKind.GOOGLE_SECTION, SyntaxKind.NUMPY_SECTION):
                continue
            for gc in child.children:
                if isinstance(gc, Node) and gc.kind in (SyntaxKind.GOOGLE_ARG, SyntaxKind.NUMPY_PARAMETER):
                    return True
        return False

    @staticmethod
    def _detect_indent(ds_text: str, root: Node) -> str:
        """Guess indentation from existing sections or default to 4 spaces."""
        ds_bytes = ds_text.encode("utf-8")
        for child in root.children:
            if isinstance(child, Node) and child.kind in (SyntaxKind.GOOGLE_SECTION, SyntaxKind.NUMPY_SECTION):
                # Use leading whitespace of first section entry after header
                for gc in child.children:
                    if isinstance(gc, Node) and gc.kind not in (
                        SyntaxKind.GOOGLE_SECTION_HEADER,
                        SyntaxKind.NUMPY_SECTION_HEADER,
                    ):
                        nl = ds_bytes.rfind(b"\n", 0, gc.range.start)
                        if nl != -1:
                            return ds_bytes[nl + 1 : gc.range.start].decode("utf-8")
        return "    "

    @staticmethod
    def _build_section_stub(
        params: list[tuple[str, str | None]],
        *,
        is_numpy: bool,
        indent: str,
    ) -> str:
        """Build a full Args/Parameters section string."""
        lines: list[str] = []
        if is_numpy:
            lines.append("Parameters")
            lines.append("----------")
            for name, ann in params:
                lines.append(f"{name} : {ann}" if ann else name)
        else:
            lines.append("Args:")
            for name, ann in params:
                if ann:
                    lines.append(f"{indent}{name} ({ann}):")
                else:
                    lines.append(f"{indent}{name}:")
        return "\n".join(lines)

    # -- entry point ---------------------------------------------------

    def diagnose(self, ctx: DiagnoseContext) -> Diagnostic | None:
        root = ctx.target_cst
        if not isinstance(root, Node):
            return None
        if not isinstance(ctx.parent_ast, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return None

        params = self._get_signature_params(ctx.parent_ast)
        if not params:
            return None

        if self._has_param_section(root):
            return None

        is_numpy = root.kind == SyntaxKind.NUMPY_DOCSTRING
        indent = self._detect_indent(ctx.docstring_text, root)
        stub = self._build_section_stub(params, is_numpy=is_numpy, indent=indent)
        insert_text = "\n\n" + stub + "\n"
        fix = Fix(
            edits=[insert_at(root.range.end, insert_text)],
            applicability=Applicability.UNSAFE,
        )
        message = "Missing Args/Parameters section in docstring."
        return self._make_diagnostic(ctx, message, fix=fix, target=root)
