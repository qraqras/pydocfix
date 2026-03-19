from __future__ import annotations

from typing import TYPE_CHECKING, Final

from pydocstring import SyntaxKind, Token

from pydocfix.rules._base import Applicability, BaseRule, DiagnoseContext, Diagnostic, Fix, insert_at

if TYPE_CHECKING:
    from pydocfix.config import Config

_DEFAULT_PERIOD: Final[str] = "."
_PERIOD_SET: Final[frozenset[str]] = frozenset([_DEFAULT_PERIOD, "!", "?"])


class D200(BaseRule):
    """Summary should end with a period."""

    code = "D200"
    message = "Summary should end with a period."
    target_kinds = {SyntaxKind.SUMMARY}

    def __init__(self, config: Config | None = None) -> None:
        super().__init__(config)
        self._conf_period: Final[str | None] = config.period.strip() if config and config.period else None
        self._valid_endings: frozenset[str] = _PERIOD_SET | {self._conf_period} if self._conf_period else _PERIOD_SET

    def diagnose(self, ctx: DiagnoseContext) -> Diagnostic | None:
        token = ctx.target_cst
        assert isinstance(token, Token)

        summary: Final[str] = token.text.strip()
        last_char: Final[str | None] = summary[-1] if summary else None

        if last_char not in self._valid_endings:
            fix = Fix(
                edits=[insert_at(token.range.end, self._conf_period or _DEFAULT_PERIOD)],
                applicability=Applicability.SAFE,
            )
            return self._make_diagnostic(ctx, self.message, fix=fix)
        return None
