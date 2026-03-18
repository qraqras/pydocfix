"""Rule D200 - Summary should end with a period."""

from __future__ import annotations

from pydocstring import SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, insert_at


class D200(BaseRule):
    """Summary should end with a period."""

    code = "D200"
    message = "Summary should end with a period."
    target_kinds = {SyntaxKind.SUMMARY}

    def diagnose(self, ctx: DiagnoseContext) -> Diagnostic | None:
        token = ctx.target_cst
        assert isinstance(token, Token)
        text = token.text.strip()
        if text and not text.endswith("."):
            fix = Fix(
                edits=[insert_at(token.range.end, ".")],
                applicability=Applicability.SAFE,
            )
            return self._make_diagnostic(ctx, self.message, fix=fix)
        return None
