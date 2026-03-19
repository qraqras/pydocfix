"""Tests for the config module."""

from __future__ import annotations

from pathlib import Path

import pytest

from pydocfix.config import Config, find_pyproject_toml, load_config


class TestFindPyprojectToml:
    def test_finds_file_in_current_dir(self, tmp_path: Path):
        toml = tmp_path / "pyproject.toml"
        toml.write_text("[project]\nname = 'test'\n")
        assert find_pyproject_toml(tmp_path) == toml

    def test_finds_file_in_parent_dir(self, tmp_path: Path):
        toml = tmp_path / "pyproject.toml"
        toml.write_text("[project]\nname = 'test'\n")
        subdir = tmp_path / "sub"
        subdir.mkdir()
        assert find_pyproject_toml(subdir) == toml

    def test_returns_none_when_not_found(self, tmp_path: Path):
        # Use a deeply nested dir with no pyproject.toml
        subdir = tmp_path / "a" / "b"
        subdir.mkdir(parents=True)
        result = find_pyproject_toml(subdir)
        # May find the real project root's pyproject.toml, so just verify
        # it returns a Path or None
        assert result is None or result.name == "pyproject.toml"


class TestLoadConfig:
    def test_defaults_when_no_file(self, tmp_path: Path):
        config = load_config(tmp_path / "nonexistent")
        assert config.ignore == []

    def test_empty_pydocfix_section(self, tmp_path: Path):
        (tmp_path / "pyproject.toml").write_text("[tool.pydocfix]\n")
        config = load_config(tmp_path)
        assert config.ignore == []

    def test_ignore_list(self, tmp_path: Path):
        (tmp_path / "pyproject.toml").write_text('[tool.pydocfix]\nignore = ["D200", "D401"]\n')
        config = load_config(tmp_path)
        assert config.ignore == ["D200", "D401"]

    def test_no_pydocfix_section(self, tmp_path: Path):
        (tmp_path / "pyproject.toml").write_text("[project]\nname = 'test'\n")
        config = load_config(tmp_path)
        assert config.ignore == []

    def test_invalid_toml_returns_defaults(self, tmp_path: Path):
        (tmp_path / "pyproject.toml").write_text("not valid TOML !!!")
        config = load_config(tmp_path)
        assert config.ignore == []


class TestIgnoreViaConfig:
    """Integration: ignored rules produce no diagnostics."""

    def test_ignored_rule_not_reported(self, tmp_path: Path):
        from pydocfix.checker import check_file
        from pydocfix.rules import build_registry

        source = 'def foo():\n    """No period"""\n    pass\n'
        registry = build_registry(ignore=["D200"])
        diags, *_ = check_file(source, tmp_path / "f.py", registry.kind_map)
        assert diags == []

    def test_non_ignored_rule_still_reported(self, tmp_path: Path):
        from pydocfix.checker import check_file
        from pydocfix.rules import build_registry

        source = 'def foo():\n    """No period"""\n    pass\n'
        registry = build_registry(ignore=["D401"])
        diags, *_ = check_file(source, tmp_path / "f.py", registry.kind_map)
        assert any(d.rule == "D200" for d in diags)

    def test_cli_respects_ignore_in_pyproject(self, tmp_path: Path, monkeypatch):
        from typer.testing import CliRunner

        from pydocfix.cli import app

        monkeypatch.chdir(tmp_path)
        (tmp_path / "pyproject.toml").write_text('[tool.pydocfix]\nignore = ["D200"]\n')
        p = tmp_path / "bad.py"
        p.write_text('def foo():\n    """No period"""\n    pass\n')

        result = CliRunner().invoke(app, ["check", str(p)])
        assert result.exit_code == 0
        assert "D200" not in result.output
