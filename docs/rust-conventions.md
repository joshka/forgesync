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
- Break long functions into named phases only when the name lets a reader forget earlier details.
  Keep a linear story together when extraction would add navigation without reducing context.
- Name important intermediate values, especially around I/O, parsing, mutation, and errors. Make
  each fallible step and its error context clear. Use explaining variables when they also improve
  line wrapping.
- Avoid behavioral boolean parameters. Use distinct operations, a meaningful enum, or named options.
  A boolean recording a domain fact can remain a boolean.
- Use newtypes when they distinguish identities or preserve a repeated invariant. Put behavior on
  the concept that owns it. Avoid one-use wrappers, parameter bags, generic frameworks, and traits
  without a real variation point.

Use `mod.rs` for directory-root modules, as in Girt and epage's guide. Keep these roots short: they
introduce the concept and point to its children. Use named leaf files for the actual behavior.

## Public APIs and dependencies

Crate roots teach the primary path and expose concept modules. Re-export only a small primary API;
do not flatten every module into the root. Prefer `pub` for items meant to be used outside their
module, and restrict the enclosing module when those items are implementation details. Treat
repeated `pub(crate)` or `pub(super)` as a prompt to examine ownership and data flow, not as the
default way to draw a seam. Keep restricted visibility when changing it would expose internals of a
public type or an intentional public module. Keep domain values separate from provider DTOs, SQL
rows, and CLI output. Use standard conversion traits when they communicate the relationship, and
document errors, side effects, cancellation, and lifecycle where callers look.

The app has no established external users, so remove development-era aliases when restructuring.
Preserve persisted archive semantics and documented CLI behavior deliberately. Refresh compatible
dependencies in the lockfile; raise manifest minimums only for required APIs, fixes, or features.
Review changes to parsing, traits, MSRV, and feature resolution separately from routine updates.

## Tests

- Write test bodies as setup, one operation, and direct expectations. Name the behavior so the
  failure identifies the contract.
- Keep focused unit tests beside the implementation. Move a large local test module into a nearby
  `tests.rs`. Reserve integration tests for behavior crossing crate or process boundaries.
- Avoid loops and branches in test bodies that select scenarios or expected outcomes. Use separate
  tests or named `rstest` cases. Fixture helpers may build incidental setup but should not hide the
  behavior under test.
- Compare values or structured error variants so failures explain what changed. Use `insta` for
  stable structured output or rendering when the snapshot is easier to review than many asserts.
  Review each changed snapshot as a behavior change.

Run `cargo +nightly fmt` for formatting. Use focused tests before the applicable workspace gates in
`AGENTS.md`.
