# Installation and operations

## Requirements

- Rust 1.98 or newer to build from source.
- GitHub CLI (`gh`) or `GITHUB_TOKEN` for commands that contact GitHub.
- An OpenAI-compatible embedding service and its API key for embedding generation and semantic or
  hybrid query vectors. Keyword search and local cluster inspection do not need that service.

Forgesync is a native CLI for Linux, macOS, and Windows. The source build uses the bundled SQLite
library; `archive doctor` checks the SQLite build, FTS5, and foreign-key enforcement at runtime.

## Build from source

From a checkout, install the default CLI with the interactive TUI enabled:

```sh
cargo install --path crates/forgesync-cli --locked
```

For a smaller build without the TUI:

```sh
cargo install --path crates/forgesync-cli --locked --no-default-features
```

## Create an archive and sync

Archive creation is explicit. Forgesync never creates or migrates an archive while opening it. The
normal database lives in your user data directory. Configure `[archive] path` to select an existing
Forgesync database or a different new location. `--archive PATH` remains a one-invocation override.
A Gitcrawl database is not a compatible archive. See
[archive and config locations](configuration.md#archive-and-config-locations).

```sh
forgesync archive init
```

Authenticate with `gh auth login`, or set `GITHUB_TOKEN` in the process environment. Then acquire
the selected discussion and review evidence:

```sh
forgesync sync rust-lang/rust \
  --with comments,reviews,review-threads
```

The default sync covers open discussions and the durable closed-thread sweep. Use `--state all` for
a full open-and-closed enumeration. Comments, reviews, and review threads are explicit families;
they are not prerequisites for the local keyword search workflow.

## Query the archive offline

Keyword search is local and does not read GitHub or embedding credentials:

```sh
forgesync search "release notes" --repo rust-lang/rust
forgesync thread list --repo rust-lang/rust --state open
forgesync thread show rust-lang/rust#1
```

Search output includes each matching thread's current evidence coverage. `archive status` shows
archive-wide counts by evidence family, including complete, incomplete, missing, failed, and
unavailable data. `archive doctor` checks database integrity, schema history, foreign keys, and
FTS5. A complete empty collection is distinct from a family that has never been acquired.

## Recover interrupted or partial work

Every sync records its run, jobs, checkpoints, and unresolved failures. Successfully acquired pages
remain committed if a later page or evidence family fails. Restarting a sync safely reacquires the
current requested scope; use the run ledger to inspect or explicitly retry recorded failures:

```sh
forgesync archive status
forgesync archive doctor
forgesync run list
forgesync run show 12
forgesync run retry 12 --family comments,reviews
```

Use the run ID reported by your archive. Retry requires GitHub credentials and contacts GitHub.
`archive migrate` applies pending schema changes explicitly; run it only when a Forgesync version
reports that migration is required. Do not point Forgesync at a Gitcrawl database.

## Diagnostics and tracing

Diagnostic logs are separate from command results. They go to stderr, so `--json` results remain
valid JSON on stdout. Logging is quiet by default; increase verbosity for more detail:

```sh
forgesync -v archive status
forgesync -vv --log-format json sync rust-lang/rust
```

`-v` enables informational diagnostics, `-vv` enables debug diagnostics, and `-vvv` enables trace
diagnostics. JSON logs are newline-delimited objects. The CLI startup event includes its version.
GitHub transport diagnostics record request origin, method, attempt, response status, and redirect
count. They do not log credentials, authorization headers, request or response bodies, or discussion
content.

## Optional model configuration

Keyword search, sync, and the TUI work without an embedding model. To use semantic retrieval or
clustering, configure an OpenAI-compatible endpoint and model in TOML, then provide the API key in
the named environment variable. The key itself is never stored in the file. See
[configuration](configuration.md) for recipes, limits, and exact model identity behavior.

```toml
[documents]
recipe = "discussion_enriched"

[embeddings]
endpoint = "https://api.openai.com/v1"
model = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"
```

Save these preferences in the automatically loaded user config, or select a file with
`--config PATH` or `FORGESYNC_CONFIG`. Embedding settings do not make model credentials a
prerequisite for keyword workflows.

## Terminal browser

Launch the TUI against an existing archive:

```sh
forgesync tui
```

Browsing, local search, coverage, failures, and cluster inspection stay local. Sync, refresh, retry,
and local cluster decisions use the shared engine operations; remote actions require GitHub access.
See [TUI controls and behavior](tui.md) for keys, progress, cancellation, and writer ownership.

## Use the engine APIs directly

The examples in `crates/forgesync-engine/examples` call the same read and sync operations used by
the CLI and TUI. Offline search only needs an existing archive:

```sh
cargo run -p forgesync-engine --example offline_search -- \
  ./forgesync.sqlite "release notes"
```

The sync example accepts `OWNER/REPO` on GitHub.com and reads `GITHUB_TOKEN` from its environment:

```sh
GITHUB_TOKEN="$(gh auth token --hostname github.com)" \
  cargo run -p forgesync-engine --example sync_repository -- \
  ./forgesync.sqlite rust-lang/rust
```

The engine itself accepts an already opened archive, typed clients, a request, and cancellation. It
does not discover credentials or read process configuration.

## Release binaries

Release archives use one native build per supported target:

- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Archives contain the `forgesync` executable (or `forgesync.exe` on Windows). The release workflow
also publishes `checksums.txt`. Choose the target that matches the operating system and CPU
architecture, extract the executable, and place it on `PATH`. Releases are prepared through the
manual workflow described in [releasing](releasing.md).
