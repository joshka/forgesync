# Forgesync

Forgesync is a local Rust archive for GitHub issues and pull requests. It syncs discussion and
selected review evidence into SQLite, then provides offline search and local maintainer triage.
Keyword search and the TUI do not need model credentials. GitHub access is needed for sync, provider
refresh, and explicit retry actions. Semantic and hybrid queries send query text to the configured
embedding service; keyword queries stay offline.

## Install and initialize

Build and install the CLI from this checkout with Rust 1.98 or newer:

```sh
cargo install --path crates/forgesync-cli --locked
```

Create your normal archive explicitly, then sync a repository:

```sh
forgesync archive init
gh auth login
forgesync sync rust-lang/rust --with comments,reviews,review-threads
```

Commands use one database in your user data directory. Set `[archive] path` in your user config to
choose another location; `--archive PATH` overrides it for one invocation. See
[archive configuration](docs/configuration.md#archive-and-config-locations).

Forgesync reads `GITHUB_TOKEN` when set, or asks the GitHub CLI for a token. No credentials are
needed for archive creation, local reads, or keyword search.

## Search and inspect offline

```sh
forgesync search "release notes"
forgesync thread list --repo rust-lang/rust
forgesync archive status
forgesync tui
```

Read the [user manual](docs/user-manual.md) for the complete acquisition, search, triage, and
recovery workflows. See [installation and operations](docs/installation.md) for setup, coverage,
recovery, and diagnostics; [configuration](docs/configuration.md) for optional model-backed
retrieval; and [TUI controls](docs/tui.md) for the interactive browser. A direct Rust example uses
the same engine search and sync APIs as the frontends:
[engine examples](crates/forgesync-engine/examples/).

## Scope

Forgesync uses its own archive format. It does not import, migrate, or modify Gitcrawl archives;
Gitcrawl remains usable separately. Cloud and portable distribution, source indexing, summaries,
metrics and analytics, owner erasure, full revision history, deep pull-request details, legacy
import, and GitHub write-back are deliberately outside this selected v2 scope. See the
[compatibility ledger](docs/compatibility.md) for feature dispositions.
