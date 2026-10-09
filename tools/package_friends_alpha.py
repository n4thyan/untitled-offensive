#!/usr/bin/env python
"""Build and verify a Windows friends-alpha directory without retail assets."""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys

MARKER = ".friends-alpha-package"
MANIFEST = "PACKAGE-MANIFEST.txt"
DEFAULT_OUTPUT = Path(r"G:\untitled-builds\friends-alpha")

# Game archives and loose/render-ready retail assets must never enter the package.
PROHIBITED_EXTENSIONS = {
    ".ff", ".iwd", ".iwi", ".bsp", ".pk3",
    ".dds", ".tga", ".jpg", ".jpeg", ".png",
    ".wav", ".mp3", ".flac", ".ogg",
    ".xmodel_bin", ".xmodel_export", ".xanim_bin", ".xanim_export",
}
PROHIBITED_NAMES = {"codmp.exe", "codsp.exe", "iw4mp.exe", "iw4sp.exe"}

STATIC_FILES = (
    "FRIENDS-ALPHA.md",
    "README.md",
    "LICENSE",
    "NOTICE",
    "run-dev-match.bat",
    "stop-dev-match.bat",
)
BINARIES = ("vcod.exe", "vcod-server.exe")


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def relative_files(root: Path) -> list[Path]:
    return sorted(
        (p.relative_to(root) for p in root.rglob("*") if p.is_file()),
        key=lambda p: p.as_posix().lower(),
    )


def prohibited(path: Path) -> bool:
    name = path.name.lower()
    return name in PROHIBITED_NAMES or any(
        name.endswith(ext) for ext in PROHIBITED_EXTENSIONS
    )


def validate_no_retail_assets(root: Path) -> None:
    bad = [p for p in relative_files(root) if prohibited(p)]
    if bad:
        listing = "\n  ".join(p.as_posix() for p in bad)
        raise RuntimeError(f"prohibited retail-style files found:\n  {listing}")


def expected_sources(source: Path) -> list[tuple[Path, Path]]:
    pairs = [(source / name, Path(name)) for name in STATIC_FILES]
    pairs.extend(
        (source / "target" / "release" / name, Path(name)) for name in BINARIES
    )
    classes = sorted((source / "assets" / "classes").glob("*.txt"))
    pairs.extend((path, Path("assets/classes") / path.name) for path in classes)
    return pairs


def ensure_sources(pairs: list[tuple[Path, Path]]) -> None:
    missing = [str(src) for src, _ in pairs if not src.is_file()]
    if missing:
        raise RuntimeError("required package inputs are missing:\n  " + "\n  ".join(missing))
    unsafe = [str(src) for src, _ in pairs if src.is_symlink() or prohibited(src)]
    if unsafe:
        raise RuntimeError("refusing unsafe package inputs:\n  " + "\n  ".join(unsafe))


def write_manifest(output: Path) -> None:
    entries = [p for p in relative_files(output) if p.name not in {MARKER, MANIFEST}]
    lines = ["# SHA-256  relative path"]
    lines.extend(f"{digest(output / p)}  {p.as_posix()}" for p in entries)
    (output / MANIFEST).write_text("\n".join(lines) + "\n", encoding="utf-8")


def read_manifest(output: Path) -> dict[str, str]:
    path = output / MANIFEST
    if not path.is_file():
        raise RuntimeError(f"missing {MANIFEST}")
    result: dict[str, str] = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        try:
            sha, rel = line.split("  ", 1)
        except ValueError as error:
            raise RuntimeError(f"bad manifest line {number}") from error
        if len(sha) != 64 or rel in result:
            raise RuntimeError(f"bad manifest line {number}")
        result[rel] = sha
    return result


def check_package(output: Path) -> None:
    if not output.is_dir() or not (output / MARKER).is_file():
        raise RuntimeError(f"{output} is not a marked friends-alpha package")
    validate_no_retail_assets(output)
    manifest = read_manifest(output)
    actual = {
        p.as_posix()
        for p in relative_files(output)
        if p.name not in {MARKER, MANIFEST}
    }
    expected = set(manifest)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        details = [*(f"missing: {p}" for p in missing), *(f"extra: {p}" for p in extra)]
        raise RuntimeError("package contents differ from manifest:\n  " + "\n  ".join(details))
    changed = [rel for rel, sha in manifest.items() if digest(output / rel) != sha]
    if changed:
        raise RuntimeError("package files failed SHA-256 verification:\n  " + "\n  ".join(changed))


def require_g_drive(output: Path, allow_non_g_output: bool) -> None:
    if allow_non_g_output:
        return
    drive, _ = os.path.splitdrive(str(output.resolve()))
    if drive.lower() != "g:":
        raise RuntimeError("output must be on G: (tests may pass --allow-non-g-output)")


def clean_output(output: Path) -> None:
    if not output.exists():
        output.mkdir(parents=True)
        return
    if not output.is_dir() or not (output / MARKER).is_file():
        raise RuntimeError(
            f"refusing to clean unmarked output {output}; remove it manually or choose another path"
        )
    shutil.rmtree(output)
    output.mkdir(parents=True)


def mark_output(output: Path) -> None:
    # Mark before copying so an interrupted run remains safely replaceable.
    (output / MARKER).write_text(
        "Created by tools/package_friends_alpha.py; safe for that script to replace.\n",
        encoding="utf-8",
    )


def create_package(source: Path, output: Path, build: bool) -> None:
    if build:
        subprocess.run(
            ["cargo", "build", "--release", "-p", "vcod", "-p", "vcod-server"],
            cwd=source,
            check=True,
        )
    pairs = expected_sources(source)
    ensure_sources(pairs)
    clean_output(output)
    mark_output(output)
    for src, rel in pairs:
        dst = output / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
    validate_no_retail_assets(output)
    write_manifest(output)
    check_package(output)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--source-dir", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--check", action="store_true", help="verify an existing output only")
    parser.add_argument("--skip-build", action="store_true", help="use existing release binaries")
    parser.add_argument("--allow-non-g-output", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    source = args.source_dir.resolve()
    output = args.output.resolve()
    try:
        require_g_drive(output, args.allow_non_g_output)
        if args.check:
            check_package(output)
            print(f"friends-alpha package OK: {output}")
        else:
            create_package(source, output, not args.skip_build)
            print(f"friends-alpha package created and verified: {output}")
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
