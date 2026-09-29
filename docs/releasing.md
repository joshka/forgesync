# Releasing

The manual `.github/workflows/release.yml` workflow builds and smoke-tests native binaries on Linux,
Intel and Apple Silicon macOS, and Windows. It packages each binary with a target-qualified asset
name, generates SHA-256 checksums, and publishes the artifacts to an existing version tag. Builds
use the repository's locked Rust dependencies and bundled SQLite, then run the same credential-free
archive and FTS5 smoke checks used by CI.

## Prepare a release

1. Merge the intended source change and ensure the platform CI and workspace gates pass.
1. Choose the version in `Cargo.toml` and keep the release tag equal to the CLI version.
1. Create and push the matching `vMAJOR.MINOR.PATCH` tag.
1. Dispatch the **Release** workflow for that existing tag.
1. Review all native build and smoke jobs, then confirm the release includes four platform archives
   and `checksums.txt`.

The workflow uses `gh release create --verify-tag`, so it cannot create a missing version tag. It
does not sign or notarize macOS binaries. Users who need notarization must use their own platform
distribution process.

## Local package checks

Build and check the current host's binary without publishing:

```sh
cargo build -p forgesync-cli --release --locked
python scripts/smoke_binary.py target/release/forgesync
python scripts/package_binary.py \
  --binary target/release/forgesync \
  --target x86_64-unknown-linux-gnu \
  --output target/package-smoke
```

On Windows, use `target/release/forgesync.exe` and `x86_64-pc-windows-msvc`. `smoke_binary.py`
removes provider credential variables from the child environment, creates a temporary archive,
checks all doctor probes including FTS5, and runs an offline keyword query. It makes no network
request.
