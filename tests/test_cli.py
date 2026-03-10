"""Tests for the CLI module."""

from __future__ import annotations

from pydocfix.cli import main, parse_args


class TestParseArgs:
    def test_default_paths(self):
        args = parse_args([])
        assert args.paths == ["."]

    def test_custom_paths(self):
        args = parse_args(["src/", "lib/"])
        assert args.paths == ["src/", "lib/"]

    def test_fix_flag(self):
        args = parse_args(["--fix"])
        assert args.fix is True

    def test_no_fix_by_default(self):
        args = parse_args([])
        assert args.fix is False


class TestMain:
    def test_returns_zero(self):
        assert main([]) == 0
