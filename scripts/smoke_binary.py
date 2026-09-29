#!/usr/bin/env python3
"""Check a built Forgesync binary without provider credentials or network access."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main() -> int:
    """Exercise a native binary's local archive path in an isolated temporary directory."""
    if len(sys.argv) != 2:
        raise SystemExit("usage: smoke_binary.py BINARY")

    binary = Path(sys.argv[1]).resolve()
    if not binary.is_file():
        raise SystemExit(f"binary does not exist: {binary}")

    environment = os.environ.copy()
    for name in (
        "FORGESYNC_CONFIG",
        "FORGESYNC_GITHUB_TOKEN",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "OPENAI_API_KEY",
    ):
        environment.pop(name, None)

    def run(*arguments: str) -> subprocess.CompletedProcess[str]:
        """Run one expected-success CLI command with provider credentials removed."""
        result = subprocess.run(
            [str(binary), *arguments],
            check=False,
            capture_output=True,
            env=environment,
            text=True,
        )
        if result.returncode != 0:
            sys.stderr.write(result.stderr)
            raise SystemExit(
                f"command failed with exit code {result.returncode}: {arguments!r}"
            )
        return result

    version = run("--version")
    if not version.stdout.startswith("forgesync "):
        raise SystemExit(f"unexpected version output: {version.stdout.strip()!r}")

    with tempfile.TemporaryDirectory(prefix="forgesync-smoke-") as temporary_directory:
        archive = Path(temporary_directory) / "smoke.sqlite"
        run("--archive", str(archive), "--json", "archive", "init")
        doctor = run(
            "--archive",
            str(archive),
            "--json",
            "--log-format",
            "json",
            "-v",
            "archive",
            "doctor",
        )
        diagnostics = [
            json.loads(line)
            for line in doctor.stderr.splitlines()
            if line.strip()
        ]
        expected_version = version.stdout.strip().split()[-1]
        startup_event = next(
            (
                event.get("fields", event)
                for event in diagnostics
                if event.get("message") == "Forgesync command started"
                or event.get("fields", {}).get("message") == "Forgesync command started"
            ),
            None,
        )
        if startup_event is None or startup_event.get("version") != expected_version:
            raise SystemExit("JSON diagnostics did not contain the safe startup event")

        doctor_data = json.loads(doctor.stdout)["data"]
        checks = {check["name"]: check["healthy"] for check in doctor_data["checks"]}
        for name in ("integrity", "foreign_keys", "fts5", "schema_history"):
            if checks.get(name) is not True:
                raise SystemExit(f"archive doctor check failed or was missing: {name}")

        search = run("--archive", str(archive), "--json", "search", "smoke")
        search_data = json.loads(search.stdout)["data"]
        if search_data["items"]:
            raise SystemExit("an empty archive unexpectedly returned search results")

    print(f"credential-free binary smoke passed: {version.stdout.strip()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
