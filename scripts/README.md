# Native binary scripts

These Python scripts check and package an already built Forgesync CLI. CI and the manual release
workflow run them on each target's native runner. They use only the Python standard library and
require Python 3.9 or newer.

## `smoke_binary.py`

Run `python scripts/smoke_binary.py BINARY` from the repository root. `BINARY` must be an executable
for the current host; the script resolves its path before running it. For example:

```sh
cargo build -p forgesync-cli --release --locked
python scripts/smoke_binary.py target/release/forgesync
```

The script removes the configured Forgesync config path, GitHub token variables, and
`OPENAI_API_KEY` from the **child process** environment. It checks the version output, creates an
archive in a temporary directory, verifies JSON startup diagnostics and the integrity, foreign-key,
FTS5, and schema-history doctor checks, then runs keyword search against the empty archive. The
child's automatic config directory is isolated too. A temporary config selects the database with a
relative path, and init, doctor, and search run without archive flags. The config and archive are
removed on exit. The selected CLI commands stay local and require no provider access. A failed
assertion or CLI command exits nonzero; success prints the checked version.

## `package_binary.py`

Run `python scripts/package_binary.py --binary BINARY --target TARGET` after building the binary for
that target on a native runner. The executable is invoked with `--version`, so a binary for another
operating system cannot be packaged from the current host with this script. For example, on x86_64
Linux:

```sh
python scripts/package_binary.py \
  --binary target/release/forgesync \
  --target x86_64-unknown-linux-gnu \
  --tag v0.1.0 \
  --output dist
```

`--tag` is optional locally; when supplied, its version must match the executable's version.
`--output` defaults to `dist`. The script creates the directory, writes one target-qualified
archive, verifies that it contains exactly the native executable, and prints the archive path. The
caller must choose the target matching the binary; the script does not inspect its architecture. It
replaces an existing archive at that path. It does not create checksums or publish a release; the
release workflow owns those steps.

| Target                     | Archive format | Member          |
| -------------------------- | -------------- | --------------- |
| `x86_64-unknown-linux-gnu` | `.tar.gz`      | `forgesync`     |
| `x86_64-apple-darwin`      | `.tar.gz`      | `forgesync`     |
| `aarch64-apple-darwin`     | `.tar.gz`      | `forgesync`     |
| `x86_64-pc-windows-msvc`   | `.zip`         | `forgesync.exe` |

See [releasing](../docs/releasing.md) for the tag, CI, checksum, and publication workflow.
