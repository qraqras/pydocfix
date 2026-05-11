#!/usr/bin/env python3
"""Benchmark the Rust pydocsync CLI against pydoclint.

The default targets and scan-root selection mirror the historical pydocsync
benchmark used for the README: shallow-clone OSS projects, then benchmark the
main Python package directory rather than the whole repository.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from statistics import median

OSS_REPOS = {
    "numpy": "https://github.com/numpy/numpy.git",
    "scikit-learn": "https://github.com/scikit-learn/scikit-learn.git",
    "httpx": "https://github.com/encode/httpx.git",
    "requests": "https://github.com/psf/requests.git",
    "flask": "https://github.com/pallets/flask.git",
    "rich": "https://github.com/Textualize/rich.git",
    "scipy": "https://github.com/scipy/scipy.git",
    "pandas": "https://github.com/pandas-dev/pandas.git",
}

DEFAULT_TARGETS = "numpy,scikit-learn"
WARMUP_RUNS = 1
DEFAULT_RUNS = 5
PYDOCLINT_ALIGN_OPTS = [
    "--arg-type-hints-in-signature=False",
    "--arg-type-hints-in-docstring=False",
]


@dataclass(frozen=True)
class BenchmarkResult:
    target: str
    url: str | None
    scan_path: Path
    files: int
    lines: int
    pydocsync_median: float
    pydocsync_stddev: float
    pydocsync_single_median: float
    pydocsync_single_stddev: float
    pydoclint_median: float
    pydoclint_stddev: float
    pydocsync_violations: int
    pydoclint_violations: int


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default=DEFAULT_TARGETS, help="Comma-separated OSS names or local paths")
    parser.add_argument("--runs", type=int, default=DEFAULT_RUNS)
    parser.add_argument("--docstyle", choices=["google", "numpy"], default="numpy")
    parser.add_argument(
        "--pydocsync",
        default=str(Path(__file__).resolve().parents[1] / "target" / "release" / "pydocsync"),
        help="Path to the Rust pydocsync executable",
    )
    parser.add_argument("--pydoclint", default="pydoclint", help="Path to pydoclint")
    args = parser.parse_args()

    if not shutil.which("hyperfine"):
        sys.exit("hyperfine is required")

    tmp_dir = Path(tempfile.mkdtemp(prefix="pydocsync_bench_"))
    try:
        targets = [target.strip() for target in args.target.split(",") if target.strip()]
        results = [benchmark_target(target, args, tmp_dir) for target in targets]
        print_readme_tables(results, args.runs, args.docstyle)
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)


def benchmark_target(target: str, args: argparse.Namespace, tmp_dir: Path) -> BenchmarkResult:
    repo_root, url = resolve_target(target, tmp_dir)
    scan_path = find_python_src(repo_root)
    files, lines = count_python_files(scan_path)

    pydocsync_cmd = [args.pydocsync, "check", "--output-format", "concise", str(scan_path)]
    pydocsync_single_cmd = [args.pydocsync, "check", "--output-format", "concise", "--jobs", "1", str(scan_path)]
    pydoclint_cmd = [
        args.pydoclint,
        "--quiet",
        f"--style={args.docstyle}",
        *PYDOCLINT_ALIGN_OPTS,
        str(scan_path),
    ]

    pydocsync_times, pydocsync_stddev = run_hyperfine(pydocsync_cmd, args.runs)
    pydocsync_single_times, pydocsync_single_stddev = run_hyperfine(pydocsync_single_cmd, args.runs)
    pydoclint_times, pydoclint_stddev = run_hyperfine(pydoclint_cmd, args.runs)
    pydocsync_output = subprocess.run(pydocsync_cmd, capture_output=True, text=True, check=False).stdout
    pydoclint_output = subprocess.run(pydoclint_cmd, capture_output=True, text=True, check=False)

    return BenchmarkResult(
        target=target,
        url=url,
        scan_path=scan_path,
        files=files,
        lines=lines,
        pydocsync_median=median(pydocsync_times),
        pydocsync_stddev=pydocsync_stddev,
        pydocsync_single_median=median(pydocsync_single_times),
        pydocsync_single_stddev=pydocsync_single_stddev,
        pydoclint_median=median(pydoclint_times),
        pydoclint_stddev=pydoclint_stddev,
        pydocsync_violations=count_pydocsync_violations(pydocsync_output),
        pydoclint_violations=count_pydoclint_violations(pydoclint_output.stdout + pydoclint_output.stderr),
    )


def resolve_target(target: str, tmp_dir: Path) -> tuple[Path, str | None]:
    path = Path(target)
    if path.is_dir():
        return path, None
    if target not in OSS_REPOS:
        sys.exit(f"unknown target {target!r}; use one of {', '.join(OSS_REPOS)} or a local path")
    dest = tmp_dir / target
    subprocess.run(["git", "clone", "--depth=1", "--quiet", OSS_REPOS[target], str(dest)], check=True)
    return dest, OSS_REPOS[target].removesuffix(".git")


def find_python_src(target: Path) -> Path:
    src = target / "src"
    if src.is_dir():
        return src
    best: Path | None = None
    best_count = -1
    for candidate in target.iterdir():
        if not candidate.is_dir() or not (candidate / "__init__.py").exists():
            continue
        count = sum(1 for _ in candidate.rglob("*.py"))
        if count > best_count:
            best = candidate
            best_count = count
    return best if best is not None else target


def count_python_files(target: Path) -> tuple[int, int]:
    files = 0
    lines = 0
    skip = {".git", "__pycache__", ".tox", ".venv", "venv", "node_modules"}
    for path in target.rglob("*.py"):
        if any(part in skip for part in path.parts):
            continue
        files += 1
        lines += len(path.read_text(errors="replace").splitlines())
    return files, lines


def run_hyperfine(cmd: list[str], runs: int) -> tuple[list[float], float]:
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as output:
        json_path = Path(output.name)
    try:
        subprocess.run(
            [
                "hyperfine",
                "--ignore-failure",
                "--warmup",
                str(WARMUP_RUNS),
                "--runs",
                str(runs),
                "--style",
                "none",
                "--export-json",
                str(json_path),
                " ".join(cmd),
            ],
            check=True,
        )
        result = json.loads(json_path.read_text())["results"][0]
        return result["times"], result["stddev"]
    finally:
        json_path.unlink(missing_ok=True)


def count_pydocsync_violations(output: str) -> int:
    return sum(1 for line in output.splitlines() if line.partition(":")[0])


def count_pydoclint_violations(output: str) -> int:
    count = 0
    for line in output.splitlines():
        stripped = line.strip()
        if not stripped or ": DOC" not in stripped:
            continue
        code = stripped.split(":", 1)[1].strip().split(":", 1)[0]
        if code.startswith("DOC") and code[3:].isdigit():
            count += 1
    return count


def fmt_seconds(seconds: float) -> str:
    return f"{seconds:.3f} sec" if seconds < 1 else f"{seconds:.2f} sec"


def fmt_stddev(seconds: float) -> str:
    return f"{seconds:.3f} sec" if seconds >= 0.01 else f"{seconds * 1000:.0f} ms"


def print_readme_tables(results: list[BenchmarkResult], runs: int, docstyle: str) -> None:
    print("## Benchmark")
    print()
    print("### Parallel (default worker pool)")
    print()
    print("| Project | Files | Lines | pydocsync | pydoclint | Speedup |")
    print("|---------|------:|------:|---------:|----------:|--------:|")
    for result in results:
        print(speed_row(result, result.pydocsync_median))
    print()
    print("### Single-threaded (`--jobs 1`)")
    print()
    print("| Project | Files | Lines | pydocsync | pydoclint | Speedup |")
    print("|---------|------:|------:|---------:|----------:|--------:|")
    for result in results:
        print(speed_row(result, result.pydocsync_single_median))
    print()
    print("| Project | pydocsync | pydoclint |")
    print("|---------|------:|------:|")
    for result in results:
        name = f"[{result.target}]({result.url})" if result.url else result.target
        print(f"| {name} | {result.pydocsync_violations:,} | {result.pydoclint_violations:,} |")
    print()
    print(f"> Median of {runs} runs (+ {WARMUP_RUNS} warmup) via hyperfine; docstring style: `{docstyle}`.")
    print(
        "> pydocsync uses the release Rust binary with `--output-format concise`; "
        "pydoclint uses `--arg-type-hints-in-signature=False --arg-type-hints-in-docstring=False`."
    )


def speed_row(result: BenchmarkResult, pydocsync_seconds: float) -> str:
    name = f"[{result.target}]({result.url})" if result.url else result.target
    lines = f"{round(result.lines / 1000)}K"
    speedup = result.pydoclint_median / pydocsync_seconds
    return (
        f"| {name} | {result.files} | {lines} | "
        f"{fmt_seconds(pydocsync_seconds)} | {fmt_seconds(result.pydoclint_median)} | **{speedup:.1f}x** |"
    )


if __name__ == "__main__":
    main()
