# Documentation guidance

Forgesync documentation should let a reader understand a workflow, find the code that implements
it, and recover from failure. Describe current behavior. Version control holds the history.

## Give each page a job

- The README covers purpose, a first local workflow, and links to the deeper guides.
- Task guides give steps, prerequisites, observable results, and recovery.
- Reference pages give exact options, defaults, formats, and error contracts.
- Explanation pages cover relationships, invariants, and decisions that code alone does not show.
- `docs/implementation-status.md` holds current open work, the next action, and the validation
  scope of recent changes. Update it in place; do not append a log entry for every change.

Name the reader's task in the heading and lead with the useful point. Wrap Markdown at 100 columns
and lint it with markdownlint-cli2.

## Rust documentation

Comments cost reading time and go stale, so each one must tell the reader something the code does
not.

- Public items: state the contract callers need. That includes invariants, side effects,
  cancellation, partial state on failure, and lifecycle (archives, network clients, terminal
  state). Skip it when the name and signature already say everything.
- Private items: comment only a non-obvious reason. Examples are an ordering requirement, a
  transaction or completeness rule, a provider quirk, or a workaround.
- Module docs: a crate root explains the crate's boundary and its main path. Other modules get a
  short `//!` only when the file's purpose is not obvious from its path. Do not list sibling
  modules or narrate the code below.
- Do not restate a field's name, repeat a signature, or list things a function does *not* do. Do
  not copy the same explanation into every leaf; put it once where the concept lives.
- Tests: a descriptive test name usually suffices. Add a comment only when the scenario hides an
  important expectation.
- Examples belong at crate and public API boundaries. Opening an archive and doing an offline read
  teaches more than constructing a type in isolation. Check them with `cargo test --doc`.

When the reason for a constraint is unclear, inspect the implementation, tests, and history before
writing one down. Do not present an inference as a guarantee.
