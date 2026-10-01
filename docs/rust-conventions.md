# Rust conventions

Optimize for a maintainer following a behavior from caller to effect, and for the least code that
does that clearly. The [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html),
[Microsoft's Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/), and
[epage's Rust style guide](https://epage.github.io/dev/rust-style/) are review prompts; local rules
win on layout.

## Keep it small

- Write the straightforward version first. Add a type, trait, module, or layer only when it has a
  second real user, enforces an invariant, or removes duplication that exists today.
- No single-use structs that only bundle arguments for one call, and no "owner", "policy", or
  "plan" types that wrap one function. A function with four clear parameters beats a struct that
  hides them.
- When two code paths do the same steps, write one implementation and parameterize the difference.
  Copies drift.
- Validate once, at the boundary where data enters (config load, clap parsing, provider decode,
  SQL constraint). Downstream code trusts typed values instead of re-checking them.
- Model only states the program produces. Do not add enum variants, error variants, or fields for
  features that do not exist yet. Delete `pub` items with no caller outside tests.
- Error enums need one variant per distinct way a caller reacts, not one per call site. Keep stable
  presentation codes separate from the typed cause.
- Use what the libraries already provide: clap for argument validation, `serde` derives for wire
  and stored JSON, reqwest's redirect policy, SQL arithmetic and joins, ratatui's `ListState`.
- Plain `+` is fine for in-memory counters that cannot realistically overflow. Use checked
  conversion at real boundaries, such as `u64` to SQLite `i64`.

## Layout

- A module owns one recognizable concept. Prefer fewer, larger files over many tiny ones: a file
  of 200–500 lines that holds a whole behavior reads better than five 40-line files.
- Keep the tree shallow. Use `mod.rs` for directory roots.
- Import names where they are used; avoid glob imports outside tests.
- Put the central type or operation first, then helpers in caller-before-callee order.
- Keep parsed CLI arguments next to their execution.
- Keep provider DTOs, domain values, SQL rows, and JSON output distinct only where their meaning
  differs. When an output shape is identical to a domain type, serialize the domain type.
- Avoid behavioral boolean parameters; use distinct functions or an enum. A boolean that records a
  domain fact is fine.

## Correctness rules learned the hard way

- Poll lease renewal alongside the active workflow. Awaiting renewal inside a `select!` branch can
  stop the transaction that must return the sole writer connection, which causes a pool timeout.
- Keep a transaction's commit in the function that opened it. Helpers may borrow the connection
  but must not commit partial membership, coverage, or evidence.
- When filtering a keyset page after its SQL read, advance the cursor from the raw selected rows,
  not only the accepted ones.
- Workers sharing an operation's writer capability must be aborted *and drained* before the
  capability is released. Dropping a `JoinSet` requests abort but does not wait for it.
- Background presentation tasks drain before terminal rendering and abort on owner drop. Advisory
  progress must never change acquisition results or block archive writes.
- TUI selection indices are presentation positions. Retain the selected identity across refreshes
  so reordered or empty results never retarget a write.
- Libraries return typed errors. Defer string conversion to the presentation boundary and do not
  branch on error message text or code strings when a typed signal exists.

## Public APIs and dependencies

Crate roots expose concept modules and a small primary API; do not flatten everything into the
root. Typed errors implement `Debug`, `Display`, and `Error`, keeping an underlying source when it
helps diagnosis. The app has no established external users, so remove development-era aliases when
restructuring, but preserve persisted archive semantics and documented CLI behavior deliberately.
Raise dependency minimums only for required APIs or fixes.

## Tests

- Test bodies are setup, one operation, and direct expectations. The test name states the
  contract.
- Unit tests live beside the code (or in a nearby `tests.rs`). Integration tests cover behavior
  that crosses crate or process boundaries.
- No loops or branches that select scenarios; use separate tests or `rstest` cases.
- Compare whole values or error variants, not row counts or display text, when claiming
  preservation or typed causes.
- Exercise process runtime capabilities (signals, subprocesses, network) with the production
  runtime builder, not a test runtime.
- Do not write tests that only assert internal structure, such as a store rejecting a negative
  `COUNT(*)`.

Run `cargo +nightly fmt` for formatting.
