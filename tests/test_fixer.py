"""Tests for the check_file fix integration."""

from __future__ import annotations

from pathlib import Path

from pydocfix.checker import build_rules_map, check_file
from pydocfix.rules import (
    Applicability,
    SUM002,
    Edit,
    Fix,
)


class TestCheckFileFix:
    def test_fixes_missing_period(self, tmp_path: Path):
        f = tmp_path / "example.py"
        src = 'def foo():\n    """Do something"""\n    pass\n'
        f.write_text(src)
        _, result, _ = check_file(src, f, build_rules_map([SUM002()]), fix=True)
        assert result is not None
        assert '"""Do something."""' in result

    def test_no_fix_needed(self, tmp_path: Path):
        f = tmp_path / "example.py"
        src = 'def foo():\n    """Do something."""\n    pass\n'
        f.write_text(src)
        _, result, _ = check_file(src, f, build_rules_map([SUM002()]), fix=True)
        assert result is None

    def test_preserves_surrounding_code(self, tmp_path: Path):
        f = tmp_path / "example.py"
        src = 'x = 1\n\ndef foo():\n    """Do something"""\n    return x\n'
        f.write_text(src)
        _, result, _ = check_file(src, f, build_rules_map([SUM002()]), fix=True)
        assert result is not None
        assert "x = 1" in result
        assert "return x" in result
        assert '"""Do something."""' in result

    def test_no_fix_without_flag(self, tmp_path: Path):
        f = tmp_path / "example.py"
        src = 'def foo():\n    """Do something"""\n    pass\n'
        f.write_text(src)
        diags, result, _ = check_file(src, f, build_rules_map([SUM002()]))
        assert len(diags) == 1
        assert result is None  # fix=False by default

    def test_diagnostics_returned_with_fix(self, tmp_path: Path):
        f = tmp_path / "example.py"
        src = 'def foo():\n    """Do something"""\n    pass\n'
        f.write_text(src)
        diags, result, _ = check_file(src, f, build_rules_map([SUM002()]), fix=True)
        assert len(diags) == 1
        assert diags[0].rule == "PDX-SUM002"
        assert result is not None
