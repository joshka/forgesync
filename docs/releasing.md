# Releasing

Forgesync publishes seven crates at coordinated versions. The `forgesync` package is the installable
product, with CLI and TUI enabled by default. The supporting libraries retain their own module and
API ownership. The first release uses local `cargo publish --workspace --locked`; subsequent
releases are prepared and published by release-plz.

## Automated crate releases

`.github/workflows/release-plz.yml` runs on `main`. It publishes a merged release PR, then prepares
the next release PR. Compilation checks run on source pushes and release PRs, without gating the
publisher a second time. `release-plz.toml` sets `release_always = false`, so ordinary source pushes
do not publish development versions. Review and merge the generated PR to approve its versions and
changelogs. Supporting crates use package-qualified tags; the product uses `vMAJOR.MINOR.PATCH` and
owns the GitHub release.

Publishing uses crates.io trusted publishing. Every crate trusts the GitHub repository
`joshka/forgesync`, workflow filename `release-plz.yml`, and environment `crates-io`. Only `main`
can deploy to that environment. The publisher job grants `id-token: write`; release-plz exchanges
its GitHub OIDC identity for a short-lived registry token. No `CARGO_REGISTRY_TOKEN` secret is
required. See the
[release-plz trusted publishing instructions](https://release-plz.dev/docs/github/quickstart) and
[crates.io trusted publishing reference](https://crates.io/docs/trusted-publishing).

The release PR job uses the built-in `GITHUB_TOKEN`. GitHub does not trigger PR workflows from that
token, so the workflow explicitly calls the same reusable CI on the generated PR branch. This avoids
storing a personal GitHub token merely to trigger checks. Repository Actions settings must allow
GitHub Actions to create pull requests; the initial setup enables this setting.

For future version corrections, use `release-plz set-version` rather than independently editing
manifests, lockfiles, and changelogs. Keep the existing shared version and version group coherent.
New crates need one manual first publication and a trusted publisher entry before CI can publish
later versions. Changing the repository, workflow filename, or environment requires updating the
trusted publisher entries on crates.io too.

When bootstrapping several new crates, crates.io may pause publication at its new-crate rate limit.
Honor the server-provided retry time, then publish only the remaining packages with
`cargo publish -p PACKAGE --locked`. Already published versions are immutable; do not repeat the
full workspace upload after a partial publication. Verify registry checksums against the local
`.crate` archives before declaring the batch complete.

## Native binary assets

The manual `.github/workflows/release.yml` workflow builds and smoke-tests native binaries on Linux,
Apple Silicon macOS, and Windows. It packages each binary with a target-qualified asset name and
generates SHA-256 checksums. Builds use locked Rust dependencies and bundled SQLite, then run
credential-free archive and FTS5 smoke checks. These opt-in builds do not gate crate publication.

1. Merge the release-plz PR and wait for the crate publisher to finish.
1. Dispatch the **Release** workflow for the product's existing `vMAJOR.MINOR.PATCH` tag.
1. Review the native build and smoke jobs, then confirm three platform archives and `checksums.txt`.

The workflow uploads assets to the product release created by release-plz, replacing same-name
assets when rerun. It can also create a release for an existing bootstrap tag. The workflow uses
`--verify-tag` when creating a release and never creates a missing tag. Native asset publication
remains an explicit dispatch: releases created with `GITHUB_TOKEN` do not trigger another workflow.
It does not sign or notarize macOS binaries.

## Local package checks

Build and check the current host's binary without publishing:

```sh
cargo build -p forgesync --release --locked
python scripts/smoke_binary.py target/release/forgesync
python scripts/package_binary.py \
  --binary target/release/forgesync \
  --target x86_64-unknown-linux-gnu \
  --output target/package-smoke
```

The example packages x86_64 Linux; use the target matching the native binary on other hosts. On
Windows, use `target/release/forgesync.exe` and `x86_64-pc-windows-msvc`. `smoke_binary.py` removes
provider credential variables from the child environment, creates a temporary archive, checks all
doctor probes including FTS5, and runs an offline keyword query. It makes no network request. See
the [script reference](../scripts/README.md) for inputs, outputs, and failure behavior.

## Temporary fast checks

Routine CI runs `cargo check --workspace --all-targets --all-features --locked` on one Linux runner.
This checks compilation, including test and example targets, without linking or running them. It is
our temporary inexpensive gate while release latency takes priority. It cannot establish runtime
correctness, platform compatibility, package installability, or that tests pass. Formatting, lint,
test, and documentation gates are temporarily deferred; retain the local validation commands in
AGENTS.md when making changes that need them.

The manually dispatched **Platform checks** workflow retains native build, smoke, and packaging
checks for Linux, Apple Silicon macOS, and Windows. Intel macOS is excluded. Run this workflow when
platform evidence is needed; it is outside the automatic release path. Cargo's normal publication
verification remains enabled.

### What a useful smoke test establishes

A runtime smoke test exercises an already built executable with a small, predictable workload. It
should verify startup and exit status, argument parsing, temporary archive creation, SQLite/FTS5
availability, and an offline query with an expected result. Isolate configuration and data, remove
credentials, and avoid real provider calls. Check behavior rather than merely accepting any output.
Runtime driver and subprocess integration need their own credential-free regression coverage:
compilation and offline database commands alone cannot catch missing Tokio I/O or signal drivers.

Measure build and link time separately from executable smoke time. A quick script attached to a slow
platform build still produces a slow workflow. Before restoring automatic platform checks or adding
release gates, measure the complete workflow with cold and warm caches, including setup and cache
restoration. Aim for a warm workflow below 60 seconds and record cold-cache costs explicitly. Cache
experiments belong in an isolated jj workspace until their benefit and reliability are clear.
