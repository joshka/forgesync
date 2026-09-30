# Rust conventions

Optimize for a maintainer following a behavior from caller to effect. Apply the
[Rust API Guidelines checklist](https://rust-lang.github.io/api-guidelines/checklist.html),
[Microsoft's Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/), and
[epage's Rust style guide](https://epage.github.io/dev/rust-style/) with judgment. These guides are
review prompts; Forgesync's domain and crate boundaries decide the final shape.

## Source layout and control flow

- Let a module own one recognizable concept. Split by the reason code changes, not a target line
  count. A file near 200–350 lines is easy to scan; review files past 500 lines for mixed ownership.
- Prefer a broad, shallow module tree: usually one level below the crate root, sometimes two when a
  concept or its tests need it, and rarely three. Add depth only when it improves navigation.
- Import names where they are used. Avoid glob imports, especially `use super::*`, because they hide
  the owning module and let child files depend on unrelated parent imports. Name sibling imports
  explicitly and remove parent imports that only served as a child module's implicit prelude.
- Give genuinely shared policy its own owner when callers otherwise import it through an unrelated
  workflow. Clock acquisition and provider-failure classification should not make analysis depend on
  document materialization or child sync depend on repository enumeration. Prefer a focused private
  module with ordinary public helpers over parent re-export chains. Extract only when the policy and
  its contract form a coherent concept; do not create a generic utilities collection.
- Put the central type or operation first, followed by its methods and local helpers in
  caller-before-callee order where possible. Put tests close to the behavior they prove.
- Write doc comments for helpers when a name and signature leave their purpose, context, or caller
  expectations unclear, even if the implementation is short. Preserve the reason for an ordering,
  omission, side effect, or failure rule; do not narrate ordinary statements.
- Keep command `match` arms short. Delegate substantial work to named operations; keep a visible
  branch when the branch itself expresses the domain rule.
- Let dispatch matches identify the next operation. When an arm performs multiple effects, error
  translations, or state updates, move that work into a verb-named method or nearby function. Define
  the target directly below the dispatcher when practical so reading stays top down. Keep short
  value mappings and meaningful domain policy visible in the match.
- Put behavior on the command, request, or state type that owns it. A parsed command can consume
  `self` in `run`; a reusable read can be a noun-named query on its owning archive or service. Use a
  bare function when no single type owns the operation. Avoid names such as `*_command` that merely
  repeat the enclosing module or type.
- Keep parsed arguments near their execution when both change for the same feature. Retain separate
  modules for genuinely shared parsing values, process output, or other cross-cutting concerns; do
  not maintain parallel trees that force readers to locate one feature twice.
- Apply the same ownership review to iterator closures as to ordinary functions. A closure that
  ranks candidates, projects several output types, and consults shared indexes can hide a concept.
  Give repeated rules named operations on their actual shared facts, so the iterator shows traversal
  and each rule can be understood independently. Keep short field projections inline.
- Break long functions into named phases only when the name lets a reader forget earlier details.
  Keep a linear story together when extraction would add navigation without reducing context.
- Derive repeated facts from the value that owns them instead of accepting another copy as an
  argument. For example, persist acquisition coordinates from the typed coverage state so indexed
  columns and serialized state cannot disagree. A narrower signature should remove a consistency
  obligation, not merely move the same loose fields into a bag.
- Aim for functions and methods that fit on one screen, usually around 25 lines. Inspect functions
  beyond 30–50 lines for separate responsibilities, live state, repeated decisions, and hidden
  transitions. A longer, genuinely linear sequence of meaningful steps can stay together. The
  exception does not cover nested loops, large branches, or repeated error/finalization protocols.
- Inspect methods with three or more parameters beyond `self`, and similarly broad free functions.
  Ask which values describe one identity, request, observation, or phase, and whether that concept
  should own the operation. Parameters that always travel together or must agree are stronger
  evidence than parameter count alone. Keep independent inputs explicit; a bag of unrelated values
  or an all-access context only hides the original problem.
- Look for a state-owning concept when a function keeps several mutable counters, cursors, flags, or
  failure values alive across phases. Give that type operations that enforce its lifecycle. Use
  consuming transitions or an enum when they prevent invalid states, and keep provider-specific
  traversal separate from shared persistence policy. Extract a module when the concept needs its own
  explanation and nearby tests, rather than scattering helpers across unrelated files.
- Review the result by following the main path and one failure path. Extraction should reduce the
  facts a caller carries and the places a policy is repeated. Shorter functions alone are not
  evidence of a better design.
- Poll lease renewal alongside the active workflow even while renewal waits for a connection.
  Awaiting renewal inside a select branch can stop the transaction that must return the sole writer
  connection, causing a pool timeout.
- Scope background listeners and helper tasks with the operation that needs them. Prefer a small
  lifetime owner over repeating manual teardown at every return; cancellation must still let the
  workflow finish its own durable cleanup.
- Give repeated eligibility or ordering policy one owner. Selection and later materialization must
  use the same predicate when they claim the same invariant; repeated inline predicates can drift.
- Keep transaction commits with the operation that opened the transaction. Phase helpers may borrow
  the connection, but must not independently commit partial membership, coverage, or evidence.
- When filtering a keyset page after its SQL read, advance from the raw selected candidates. Using
  only accepted rows as the cursor can revisit invalid data or hide later valid results.
- Name important intermediate values, especially around I/O, parsing, mutation, and errors. Make
  each fallible step and its error context clear. Use explaining variables when they also improve
  line wrapping.
- When a long SQL literal makes formatting compress its binding chain, name the query first and bind
  one value per line. Keep serialization/conversion, database execution, and missing-row validation
  as separate fallible steps so readers can trace values and failure boundaries.
- Avoid behavioral boolean parameters. Use distinct operations, a meaningful enum, or named options.
  A boolean recording a domain fact can remain a boolean.
- In workflow tests, construct the real request next to the real operation and assertions. Shared
  fixtures can build clients or payloads, but should not run acquisition behind convenience names or
  positional flags. Named request fields make selected families, cancellation, and durable effects
  visible without tracing a helper chain.
- Use newtypes when they distinguish identities or preserve a repeated invariant. Put behavior on
  the concept that owns it. Avoid one-use wrappers, parameter bags, generic frameworks, and traits
  without a real variation point.

Use `mod.rs` for directory-root modules, as in Girt and epage's guide. Keep these roots short: they
introduce the concept and point to its children. Use named leaf files for the actual behavior.

## Public APIs and dependencies

Crate roots teach the primary path and expose concept modules. Re-export only a small primary API;
do not flatten every module into the root. Do not forward another crate's DTO collection from the
root as a substitute for its owning modules; workflow signatures can name those original types
without creating a second apparent owner. Prefer `pub` for items meant to be used outside their
module, and restrict the enclosing module when those items are implementation details. Treat
repeated `pub(crate)` or `pub(super)` as a prompt to examine ownership and data flow, not as the
default way to draw a seam. Keep restricted visibility when changing it would expose internals of a
public type or an intentional public module. Keep domain values separate from provider DTOs, SQL
rows, and CLI output. Use standard conversion traits when they communicate the relationship, and
document errors, side effects, cancellation, and lifecycle where callers look. Typed errors should
implement `Debug`, `Display`, and `std::error::Error`, retaining an underlying error as a source
when it helps diagnosis. Defer diagnostic string conversion until the presentation boundary;
wrappers should retain a typed source so cleanup and failure policy do not depend on human text.
Keep stable presentation codes separate from that typed cause.

The app has no established external users, so remove development-era aliases when restructuring.
Preserve persisted archive semantics and documented CLI behavior deliberately. Refresh compatible
dependencies in the lockfile; raise manifest minimums only for required APIs, fixes, or features.
Review changes to parsing, traits, MSRV, and feature resolution separately from routine updates.

## Tests

- Exercise process-owned runtime capabilities with the production runtime builder. Async test
  runtimes enable drivers automatically and cannot prove that the executable enables signal,
  subprocess, or network I/O support.
- Write test bodies as setup, one operation, and direct expectations. Name the behavior so the
  failure identifies the contract.
- Keep focused unit tests beside the implementation. Move a large local test module into a nearby
  `tests.rs`. Reserve integration tests for behavior crossing crate or process boundaries.
- Avoid loops and branches in test bodies that select scenarios or expected outcomes. Use separate
  tests or named `rstest` cases. Fixture helpers may build incidental setup but should not hide the
  behavior under test. Fixed resource cleanup loops may remain when their purpose is documented;
  expanding identical cleanup steps adds noise without clarifying a scenario. Name fixture policies
  such as active versus historical selection instead of passing behavior booleans.
- Coordinate asynchronous scenarios with an observable fixture boundary when possible. A responder
  notification can identify request arrival directly; polling request history adds a loop, timeout,
  and matching policy that readers must understand before they can follow the tested transition.
  Keep the actual workflow and cancellation visible in the scenario.
- Compare values or structured error variants so failures explain what changed. Use `insta` for
  stable structured output or rendering when the snapshot is easier to review than many asserts.
  Review each changed snapshot as a behavior change.
- When a regression claims retained membership or content, compare identities and relevant payloads
  against an explicit baseline. A row count alone can pass after one member is substituted for
  another. Compare whole values when stable; isolate acquisition timestamps or other intentionally
  changing metadata instead of weakening the preservation assertion.
- When a test claims typed cause preservation, downcast `Error::source` and check the concrete
  variant. Matching display text only establishes wording and can pass after the typed boundary has
  been replaced by a string. Keep presentation wording checks separate from source contracts.

Run `cargo +nightly fmt` for formatting. Use focused tests before the applicable workspace gates in
`AGENTS.md`.

## Review evidence and intentional boundaries

Describe completion at the scope actually reviewed: a selected workflow does not prove every
function in its crate is maintainable. Record remaining work explicitly and distinguish local
validation from hosted platform results. API documentation must be checked against effects: a
semantic query may read archived vectors and still contact a service for the query vector.

Restricted visibility remains appropriate for SQL connection helpers inside public store modules,
private fields of the public archive handle, and internal validation hooks on public engine types.
Making these public would expose invariants rather than simplify a private implementation boundary.
When such a helper is moved into a private module, use ordinary `pub` there and import its owning
module directly. Do not replace restricted visibility mechanically or publish SQL state to satisfy a
style preference.

Background presentation tasks need an owner just as archive connections do. Keep their channel and
join handle together, state who closes delivery, drain before terminal rendering, and abort on
unexpected owner drop. Advisory progress must not change acquisition results or block archive
writes. A task extracted only to shorten a function is insufficient if its lifetime remains hidden.

When workers share an operation's writer capability, abort and drain them on every normal exit
before releasing that capability, including fatal worker and persistence errors. Dropping a
`JoinSet` requests abort but does not wait for cleanup. Test the lifetime boundary directly when
cleanup order is part of correctness; a success-path result test does not cover outstanding worker
resources.

Selection indices are presentation positions, not durable operation targets. Keep the highlighted
row separate from the applied domain selection, and retain the selected identity or value across
refreshes. Reordered or empty results must not silently retarget a write or broaden its scope. When
multiple fields describe one lifecycle, use a state type that keeps their valid combinations
explicit, such as idle versus running work with a required label and optional progress.

A coordinator owns relationships between components, not every component's internal protocol. Keep
generation checks, cache retention, and cursor bounds with the panel or request that owns them.
Retain explicit coordination when one transition invalidates another view. Cached actionable data
must still belong to the selected identity: a different selection clears old targets immediately,
while a refresh of the same identity may preserve its cache. Test both cases directly.

For command intent, use distinct variants for distinct transitions rather than a boolean that
selects the operation. Keep booleans for actual observed domain facts. Named requests make the
effect visible where a caller constructs it and keep dispatch exhaustive over meaningful actions.
