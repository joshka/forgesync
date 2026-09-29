# Maintainability work

This is the migration plan for the existing implementation. Completion means a maintainer can locate
one behavior, follow its main path top-down, and find direct tests without loading unrelated
workflows into memory. File length is a signal to inspect ownership, not a pass/fail target.

The crate-root, CLI command, engine sync/search/clustering/refresh, GitHub resource/transport, store
operation, TUI view/state, and TUI read/operation splits are complete. Core identities and embedding
client concerns now have smaller owning modules. Review-thread acquisition and normalization have
separate owners. Store and CLI integration suites are grouped by scenario, CLI arguments and
dispatch by command, and TUI keys by screen. Each further split should follow a coherent behavior
and preserve the nearby test path.

The file-level splits do not establish that functions inside those files have coherent ownership.
Review function shape as well: a long function can hide a state machine even inside a well-named
module. Use length and parameter count to find candidates, then follow the data and transitions
before choosing an extraction.

## Function and state ownership pass

Aim for a main path that fits on a screen, with meaningful operations directly below it. Around 25
lines is a useful reading target; functions beyond 30–50 lines and methods with at least three
parameters deserve inspection. These signals identify work to understand, not mechanical limits. The
detailed rules are in [Rust conventions](rust-conventions.md).

The first acquisition slice replaces the roughly 200-line review and 240-line review-thread
functions with a shared reserved collection lifecycle. Preparation consumes the metadata result as
one value, and either finishes immediately or yields a head-aware collection. The collection owns
staged counts and terminal archive writes. REST links and GraphQL cursor-cycle checks remain with
their provider collectors. Completion and failure consume the collection, exposing terminal state in
the type's operations rather than relying on callers to coordinate counters and flags.

The survey found these next candidates across the workspace. Approximate lengths describe the
starting implementation and include signatures; inspect current code before beginning a slice.

| Area                         | Signal                                                               | Concept to investigate                                                     | Contract to preserve                                                     |
| ---------------------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Engine sync jobs             | `run_jobs` around 260 lines; comment and PR jobs around 180          | A durable family job with accumulated progress and a terminal outcome      | Independent failures, cancellation, lease fencing, retry ledger          |
| Engine comments and metadata | Comment acquisition around 180 lines; metadata around 125            | Reserved family attempts with distinct source requirements                 | Empty versus incomplete membership, freshness, head metadata             |
| Engine enumeration           | Page enumeration around 160 lines                                    | Scan progress and checkpoint advancement                                   | A checkpoint advances only after complete durable coverage               |
| Engine refresh               | Several clients, request, cancellation, and progress passed together | A refresh execution with selected stages and explicit service dependencies | Stage-specific partial results; completed sync survives analysis failure |
| Engine clustering            | Candidate construction around 110 lines with graph mutations         | Evidence selection versus component policy                                 | Deterministic grouping, fanout, cross-kind and size safeguards           |
| Store observations           | Parent application around 215 lines; family finalization around 220  | Transaction-local canonical selection and membership application           | Ordering and completeness remain atomic with persisted evidence          |
| Store query building         | Embedding reads around 130 lines; thread reads around 100            | Typed filters and row-to-projection assembly                               | Bound SQL, stable ordering, model compatibility, pagination              |
| GitHub transport             | Retry loop around 90 lines and redirect traversal around 70          | Budgeted attempts and trusted-origin redirect state                        | Credential containment, cancellation, rate-limit deferral, retry bounds  |
| CLI execution                | Retry and embed around 140 lines; refresh around 105                 | Command-owned setup, execution, and report phases                          | Config precedence, credentials, exit codes, partial JSON reports         |
| CLI presentation             | Thread detail around 125 lines                                       | Sections of a prepared display model                                       | Evidence coverage and stable human meaning                               |
| TUI operations               | Action execution around 140 lines with large match arms              | Action methods over an operation's services and cancellation               | Generation tracking, progress, writer ownership, error presentation      |
| TUI drawing                  | Several 50–70-line line builders                                     | Screen sections and a prepared presentation model                          | Selection, scroll bounds, resize behavior                                |

Core currently has no comparable production function over 50 lines in this survey. Its checked value
types are a useful reference for moving invariants into an owner. Store transaction functions may
remain longer when their steps are linear and share one transaction; do not move SQL to remote
helpers merely to shorten the caller. For transport and workflow loops, distinguish immutable
request data from mutable attempt state rather than introducing a general service container.

Continue with durable sync-job ownership, then store canonical application and TUI action dispatch.
Each slice should preserve its focused regression cases and update the architecture explanation when
a new concept gains an owner.

## Order of work

1. Establish the guidance in `docs/documentation.md`, `docs/rust-conventions.md`, and
   `docs/architecture.md`. Add the nightly rustfmt configuration and align CI.
1. Separate CLI startup, command handlers, and presentation. Each command family should own its
   request construction, engine call, rendering, and command-specific tests. Keep shared process
   output and configuration at the CLI boundary.
1. Split engine sync by evidence family and separate orchestration from resource acquisition and
   reporting. Split search by keyword, semantic, and hybrid policy; split clustering by build,
   scoring, and decisions. Move focused tests with each owner.
1. Group GitHub endpoints and normalization by resource. Group store SQL by archive lifecycle,
   discussion observations, runs/checkpoints, search reads, and clusters. Keep SQL rows private.
1. Organize TUI code by screen and action, with shared navigation and task orchestration. Keep
   rendering a translation of prepared state.
1. Replace blanket crate-root re-exports with public concept modules and deliberate primary exports.
   Update imports and Rustdoc together. Remove obsolete aliases where no external contract exists.
1. Audit dependency and tool versions. Refresh compatible lockfile resolutions, then assess
   behavior-affecting updates in separate changes. Review snapshots and run the repository gates.

## Review each slice

Trace one command or action from input to effect. Check that names expose policy and side effects,
large `match` arms delegate substantial work, and errors retain useful context. Read the tests
without mentally executing helper logic. Confirm that archive ordering, completeness, cancellation,
CLI JSON, and recovery contracts still hold. Update the module map and public docs when ownership
moves.

The existing implementation status is historical evidence of feature completion. Record evidence for
each migration slice there; do not mark this plan complete merely because formatting passes.
