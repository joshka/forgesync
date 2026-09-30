# Forgesync agent notes

Forgesync is a focused local-first Rust v2 for GitHub discussions. Follow the selected scope in the
local implementation plan. Do not add deferred cloud/portable distribution, code indexing,
summaries, metrics, analytics, legacy import, full revision history, old CLI compatibility, or
GitHub write-back as dependencies of the selected workflows.

Read [documentation guidance](docs/documentation.md), [Rust conventions](docs/rust-conventions.md),
and the [module map](docs/architecture.md) when changing their respective surfaces. Apply the
[Rust API Guidelines checklist](https://rust-lang.github.io/api-guidelines/checklist.html),
[Microsoft's Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/), and
[epage's Rust style guide](https://epage.github.io/dev/rust-style/) where they improve this app's
reader locality, correctness, and API clarity. Local rules resolve conflicting layout preferences.

## Maintaining project guidance

- When maintainer feedback or a recurring review finding establishes a reusable rule, record it in
  this file or the relevant linked guide during the change. Do not rely on conversation history.
- State the general rule and why it helps readers or maintainers. Keep one-off implementation
  decisions with the affected code or change instead of accumulating them here.
- Update existing guidance when it fits. Keep this file as the entry point to the detailed guides.

## Boundaries

- The `forgesync` product package owns the executable and module facade. CLI and TUI are default
  product features; library-only consumers opt out. Keep workflow behavior in its existing owner,
  and keep process integration tests with the executable package.
- Core owns domain identities, normalized content, observations, coverage, and outcomes.
- Store owns SQLite lifecycle, SQL, ordering/application, recovery, and local decisions.
- GitHub owns typed provider DTOs, transport, pagination, and normalization; it does not write to
  the archive or load application config.
- Engine owns workflow and analysis policy. CLI and TUI build requests and present results.
- TUI depends on engine/core, never CLI. Add GitHub, engine, and TUI crates only when their first
  implementation task begins.
- Keep provider values, domain values, SQL rows, and public JSON DTOs distinct where their meaning
  differs. Avoid generic repository, provider, or scheduler frameworks.

## Storage rules

- Opening an archive never creates, migrates, downloads, or refreshes it. Create, read-only open,
  writable open, and migration are explicit operations.
- Use SQLx with SQLite only, bound parameters, and explicit archive methods. Keep migrations ordered
  and immutable. Convert rows at the store boundary; do not expose SQL rows as domain or CLI types.
- Enable foreign keys per connection. Use on-disk temporary databases for WAL, pool, and concurrency
  behavior. Never hold a transaction across provider I/O.
- Keep source timestamps, acquisition sequence, completeness, and resource family distinct. An
  incomplete collection cannot replace canonical complete membership.

## Workflow and diagnostics

- Engine operations accept an already opened archive and explicit cancellation. Reads stay local and
  side-effect-free. Partial success remains a structured report.
- Preserve the selected observation, checkpoint, partial collection, and failure-isolation
  invariants from docs/compatibility.md. Do not use last-write-wins without fixture evidence.
- The CLI selects one configured/default archive, with `--archive` as an invocation override.
  Libraries still receive an explicit opened archive. Default path selection never implies creation
  or migration; only explicit `archive init` creates its parent directories.
- Libraries return typed errors and never install a tracing subscriber or read process environment.
  The CLI owns config resolution, subscriber setup, exit codes, and JSON rendering.
- Do not log credentials, headers, prompts, discussion bodies, or raw provider payloads.

## Change and validation procedure

- Read [release guidance](docs/releasing.md) before registry publication or release automation
  changes. New crates require an initial Cargo publication before configuring their trusted
  publisher; CI uses the matching workflow filename and environment without a registry secret.
- Use jj for version control. Start a described new change for each separable task and run jj
  operations sequentially. Do not create Git worktree threads.
- Read the plan, this file, implementation status, and the selected reference tests before each
  task. Implement the earliest unblocked task and update docs/implementation-status.md with evidence
  and the next task.
- Add dependencies only when the current selected feature needs them. Keep deferred crates and
  features out of the workspace and normal build.
- Run focused tests, then applicable workspace gates: cargo +nightly fmt --all -- --check, cargo
  clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace
  --all-features --locked, cargo build -p forgesync --no-default-features --features cli --locked,
  and cargo doc --workspace --no-deps --all-features.
- Lint changed Markdown with markdownlint-cli2 and /Users/joshka/.markdownlint-cli2.yaml.

## Release latency

Routine hosted CI temporarily uses a single Linux compilation check. Native platform checks are
manual and do not gate crate publication. Before adding automatic release gates, measure complete
cold and warm workflow durations and explain the coverage gained. See
[release guidance](docs/releasing.md) for the temporary policy, smoke-test expectations, and
follow-up issue.
