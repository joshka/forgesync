#!/usr/bin/env python3
"""Package one native Forgesync release binary as a thin platform archive."""

import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile


# Asset names are target-qualified, while archive members use the native executable name.
TARGETS = {
    "x86_64-unknown-linux-gnu": "forgesync",
    "x86_64-apple-darwin": "forgesync",
    "aarch64-apple-darwin": "forgesync",
    "x86_64-pc-windows-msvc": "forgesync.exe",
}
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?\Z")


def verify_archive(archive_path: Path, suffix: str, executable_name: str) -> None:
    """Reject a package containing anything other than the selected executable."""
    if suffix == "zip":
        with zipfile.ZipFile(archive_path) as archive:
            names = archive.namelist()
    else:
        with tarfile.open(archive_path, mode="r:gz") as archive:
            names = archive.getnames()
    if names != [executable_name]:
        raise SystemExit(f"unexpected package contents in {archive_path}: {names!r}")


def binary_version(binary: Path) -> str:
    """Read the executable's own version so package names cannot drift from the build."""
    result = subprocess.run(
        [str(binary), "--version"],
        check=True,
        capture_output=True,
        text=True,
    )
    prefix, separator, version = result.stdout.strip().partition(" ")
    if prefix != "forgesync" or not separator or not VERSION.fullmatch(version):
        raise SystemExit(f"unexpected binary version output: {result.stdout.strip()!r}")
    return version


def main() -> int:
    """Package one native binary under the caller-selected target label."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--tag", help="release tag to match, such as v0.1.0")
    parser.add_argument("--output", type=Path, default=Path("dist"))
    arguments = parser.parse_args()

    binary = arguments.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"binary does not exist: {binary}")
    version = binary_version(binary)
    if arguments.tag and arguments.tag.removeprefix("v") != version:
        raise SystemExit(
            f"release tag {arguments.tag!r} does not match binary version {version!r}"
        )

    arguments.output.mkdir(parents=True, exist_ok=True)
    suffix = "zip" if arguments.target == "x86_64-pc-windows-msvc" else "tar.gz"
    archive_path = arguments.output / f"forgesync_{version}_{arguments.target}.{suffix}"
    executable_name = TARGETS[arguments.target]

    with tempfile.TemporaryDirectory(prefix="forgesync-package-") as temporary_directory:
        staged_binary = Path(temporary_directory) / executable_name
        shutil.copy2(binary, staged_binary)
        if suffix == "zip":
            with zipfile.ZipFile(
                archive_path,
                mode="w",
                compression=zipfile.ZIP_DEFLATED,
            ) as archive:
                archive.write(staged_binary, arcname=executable_name)
        else:
            with tarfile.open(archive_path, mode="w:gz") as archive:
                archive.add(staged_binary, arcname=executable_name, recursive=False)
    verify_archive(archive_path, suffix, executable_name)

    print(archive_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
