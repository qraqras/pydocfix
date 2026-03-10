"""Command-line interface for pydocfix."""

from __future__ import annotations

import argparse
import sys


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="pydocfix",
        description="A Python docstring linter with auto-fix support.",
    )
    parser.add_argument(
        "paths",
        nargs="*",
        default=["."],
        help="Files or directories to check.",
    )
    parser.add_argument(
        "--fix",
        action="store_true",
        help="Automatically fix docstring issues.",
    )
    parser.add_argument(
        "--version",
        action="version",
        version=f"%(prog)s {_get_version()}",
    )
    return parser.parse_args(argv)


def _get_version() -> str:
    from pydocfix import __version__

    return __version__


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    # TODO: implement linting logic
    print(f"pydocfix: checking {args.paths}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
