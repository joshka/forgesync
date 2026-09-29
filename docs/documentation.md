# Documentation guidance

Forgesync documentation should let a reader understand a workflow, locate its owning code, and
recover from failure without reconstructing the entire implementation. Describe current behavior;
keep plans and historical evidence visibly separate.

## Give each page a job

- Use the README for purpose, a first local workflow, and links to the deeper guides.
- Use task guides for steps, prerequisites, observable results, and recovery.
- Use reference pages for exact options, defaults, formats, and error contracts.
- Use explanation pages for relationships, invariants, and decisions that code alone does not show.
- Keep dated implementation evidence in `docs/implementation-status.md`, outside the newcomer path.

Name the reader's task in the heading and lead with the useful point. Favor connected prose for
relationships and short lists for independent steps. Explain the local contract before linking to
external background. Avoid repeating identifiers, signatures, or obvious control flow in Rustdoc.

## Document ownership and effects

The [module map](architecture.md) explains the six crate boundaries. Public module Rustdoc should
state the concept it owns, its primary workflow, and related modules. Public types should explain
their invariant and who constructs them. Fallible operations should explain relevant side effects,
partial state, cancellation, retry, and recovery. State the lifecycle of archives, network clients,
and terminal state at their owning APIs.

Keep provider DTOs, domain values, database rows, and CLI JSON shapes distinct in prose as well as
code. Document the difference between a complete collection and an incomplete observation where the
distinction controls stored membership. Keep examples practical: opening an existing archive and
performing an offline read explains more than constructing a type without using it.

## Review documentation as a contract

Check changed commands, paths, options, links, and examples against the current checkout. Check
Rustdoc examples with `cargo doc` and tests where applicable. When the reason for a constraint is
unclear, inspect the implementation, relevant tests, and introducing history before writing one.
Keep uncertainty explicit rather than presenting an inference as a guarantee.

Review a page from two entry points: the intended reading path and a direct landing from search or
Rustdoc. Remove duplicated setup and stale promises. Preserve explanations that prevent a mistake,
even when they take more words. Wrap Markdown prose at 100 columns and run the repository's rumdl
and markdownlint checks after editing.

This guidance distills the local Practice documentation workflow and Girt documentation standard for
Forgesync's archive, sync, search, and triage workflows.
