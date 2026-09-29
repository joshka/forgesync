# Maintainability work

This is the migration plan for the existing implementation. Completion means a maintainer can locate
one behavior, follow its main path top-down, and find direct tests without loading unrelated
workflows into memory. File length is a signal to inspect ownership, not a pass/fail target.

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
