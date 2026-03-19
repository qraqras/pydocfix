"""Rule D409 - Docstring parameters are in a different order than the signature."""

from __future__ import annotations

import ast

from pydocstring import Node, SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Edit, Fix


def _bare_name(name: str) -> str:
    """Strip leading ``*`` or ``**`` from a parameter name."""
    return name.lstrip("*")


class D409(BaseRule):
    """Docstring parameters are listed in a different order than the function signature."""

    code = "D409"
    message = "Docstring parameters are not in the same order as the function signature."
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
    def _get_documented_param_nodes(section: Node) -> list[tuple[str, Node]]:
        """Return ``(bare_name, node)`` pairs for each documented parameter, in order."""
        result: list[tuple[str, Node]] = []
        for child in section.children:
            if not isinstance(child, Node):
                continue
            if child.kind not in (SyntaxKind.GOOGLE_ARG, SyntaxKind.NUMPY_PARAMETER):
                continue
            for token in child.children:
                if isinstance(token, Token) and token.kind == SyntaxKind.NAME:
                    result.append((_bare_name(token.text), child))
                    break
        return result

    @staticmethod
    def _get_signature_order(func: ast.FunctionDef | ast.AsyncFunctionDef) -> list[str]:
        """Return bare parameter names in signature order, excluding ``self``/``cls``."""
        result: list[str] = []
        all_positional = [*func.args.posonlyargs, *func.args.args]
        skip_first = bool(all_positional) and all_positional[0].arg in ("self", "cls")
        for i, arg in enumerate(all_positional):
            if i == 0 and skip_first:
                continue
            result.append(arg.arg)
        for arg in func.args.kwonlyargs:
            result.append(arg.arg)
        if func.args.vararg:
            result.append(func.args.vararg.arg)
        if func.args.kwarg:
            result.append(func.args.kwarg.arg)
        return result

    @staticmethod
    def _find_child_token(node: Node, kind: SyntaxKind) -> Token | None:
        for child in node.children:
            if isinstance(child, Token) and child.kind == kind:
                return child
        return None

    @staticmethod
    def _entry_span(ds_bytes: bytes, param_node: Node) -> tuple[int, int]:
        """Return ``(start, end)`` byte positions for a full parameter entry (including trailing newline)."""
        nl_before = ds_bytes.rfind(b"\n", 0, param_node.range.start)
        start = nl_before + 1 if nl_before != -1 else param_node.range.start
        nl_after = ds_bytes.find(b"\n", param_node.range.end)
        end = nl_after + 1 if nl_after != -1 else param_node.range.end
        return start, end

    def _build_reorder_fix(
        self,
        ds_text: str,
        doc_params: list[tuple[str, Node]],
        sig_order: list[str],
    ) -> Fix:
        ds_bytes = ds_text.encode("utf-8")

        # Collect (name, start, end) for each entry
        entries = [(name, *self._entry_span(ds_bytes, node)) for name, node in doc_params]

        # Desired order: signature order first, then unknown params in their original relative order
        sig_index = {name: i for i, name in enumerate(sig_order)}
        sig_set = set(sig_order)
        in_sig = sorted(
            [(name, s, e) for name, s, e in entries if name in sig_set],
            key=lambda x: sig_index[x[0]],
        )
        not_in_sig = [(name, s, e) for name, s, e in entries if name not in sig_set]
        desired = in_sig + not_in_sig

        new_text = "".join(ds_bytes[s:e].decode("utf-8") for _, s, e in desired)
        block_start = entries[0][1]
        block_end = entries[-1][2]

        return Fix(
            edits=[Edit(start=block_start, end=block_end, new_text=new_text)],
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

        doc_params = self._get_documented_param_nodes(section)
        sig_order = self._get_signature_order(ctx.parent_ast)

        # Only consider params that appear in both signature and docstring
        sig_set = set(sig_order)
        doc_names = [name for name, _ in doc_params if name in sig_set]
        # The expected order is sig_order filtered to those present in doc_names
        doc_name_set = set(doc_names)
        expected = [name for name in sig_order if name in doc_name_set]

        if doc_names == expected:
            return None

        fix = self._build_reorder_fix(ctx.docstring_text, doc_params, sig_order)

        # Report diagnostics for each parameter that is out of position;
        # attach the (single) reorder fix only to the first violation.
        diagnostics: list[Diagnostic] = []
        for (doc_name, param_node), exp_name in zip(doc_params, expected):
            if doc_name == exp_name or doc_name not in sig_set:
                continue
            name_token = self._find_child_token(param_node, SyntaxKind.NAME)
            message = f"Parameter '{doc_name}' is in the wrong order (expected '{exp_name}' at this position)."
            diagnostics.append(
                self._make_diagnostic(
                    ctx,
                    message,
                    fix=fix if not diagnostics else None,
                    target=name_token or param_node,
                )
            )

        return diagnostics or None
