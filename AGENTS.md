# Forgesync agent notes

Forgesync is a focused local-first Rust v2 for GitHub discussions. Follow the selected scope in the
local implementation plan. Do not add deferred cloud/portable distribution, code indexing,
summaries, metrics, analytics, legacy import, full revision history, old CLI compatibility, or
GitHub write-back as dependencies of the selected workflows.

## Boundaries

- Core owns domain identities, normalized content, observations, coverage, and outcomes.
- Store owns SQLite lifecycle, SQL, ordering/application, recovery, and local decisions.
- GitHub owns typed provider DTOs, transport, pagination, and normalization; it does not write to the
  archive or load application config.
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
- Libraries return typed errors and never install a tracing subscriber or read process environment.
  The CLI owns config resolution, subscriber setup, exit codes, and JSON rendering.
- Do not log credentials, headers, prompts, discussion bodies, or raw provider payloads.

## Change and validation procedure

- Use jj for version control. Start a described new change for each separable task and run jj
  operations sequentially. Do not create Git worktree threads.
- Read the plan, this file, implementation status, and the selected reference tests before each task.
  Implement the earliest unblocked task and update docs/implementation-status.md with evidence and
  the next task.
- Add dependencies only when the current selected feature needs them. Keep deferred crates and
  features out of the workspace and normal build.
- Run focused tests, then applicable workspace gates: cargo fmt --all -- --check,
  cargo clippy --workspace --all-targets --all-features -- -D warnings,
  cargo test --workspace --all-features --locked, cargo build -p forgesync-cli
  --no-default-features --locked, and cargo doc --workspace --no-deps --all-features.
- Lint changed Markdown with markdownlint-cli2 and /Users/joshka/.markdownlint-cli2.yaml.
