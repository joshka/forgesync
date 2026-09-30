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

For checked constructors, state what is validated and what remains a caller or downstream boundary
obligation. Valid spelling, positivity, or encoding does not prove record existence, archive
allocation, provider parentage, model compatibility, or truthful counts. Public record construction
and derived deserialization often preserve supplied facts without validating their relationships.
Document normalization and rejected-input behavior where they occur; explain equality/hash scope
when readers might mistake a local number or display name for durable identity.

Distinguish producer obligations from enforced guarantees. Calling a message safe or a record
validated does not make public-field construction or deserialization redact credentials or verify
relationships. Name the boundary that supplies safe text, truthful completeness, or compatibility
metadata. Explain typed categories separately from retry policy; a failure label alone does not
establish what recovery is allowed or whether earlier writes survived.

For counts and summaries, name the unit and population: jobs, runs, ledger entries, source members,
and committed work are different quantities. State whether counts can be added or reconciled and
whether unknown categories contribute to totals. Do not describe every in-progress record as
abandoned or every unresolved entry as retryable without evidence from the owning workflow.

For assembled read projections, document whether related values share one database snapshot or come
from separate reads that concurrent writers may advance between. Explain ordering and fallback
selection where those determine visible identities. A diagnostic observation is not mutation
authority: lease state, current membership, or a displayed canonical choice must not imply that a
later write can skip its own fence and identity validation. Keep these contracts beside the owning
operation so callers need not reconstruct them from SQL.

Describe side effects precisely: an operation can preserve durable application data while creating
connection-local temporary probe objects. Explain cleanup and error reporting at that operation;
calling it read-only should not conceal temporary writes. Likewise, state which checks an aggregate
health flag summarizes rather than implying that pending work or evidence freshness is included.

Every Rust module file, including private leaves and focused test modules, needs an opening `//!`
that orients a reader arriving directly from search. Explain what the file contains, when its main
types or operations are used, who calls them, and how they relate to neighboring modules. A small
leaf may need roughly ten lines; a crate root or coordinating module may need several sections. Use
the complexity of the mental model rather than a line quota to decide the depth. A one-line label
seldom explains a module with multiple types, state transitions, or failure boundaries.

Let documentation settle at the narrowest level that covers its readers. Crate roots explain
boundaries and the route through the package. Coordinating modules explain workflows and how their
children divide work. Leaf modules explain their own types and invariants. Item and function docs
explain the contract, preconditions, effects, and failure meaning specific to that item. Avoid
copying a shared explanation into every leaf, but do not force a reader to climb to a distant guide
to learn why a local operation exists. Top-of-file docs should serve as a map for the code below.

Capture durable reasoning while implementing: why a boundary, ordering rule, completeness check, or
state transition matters to the current design. Include the facts a future maintainer would
otherwise have to reconstruct by tracing callers, tests, and history. Favor present-tense
explanations of what is here. Mention a former design or bug only when that history explains a
current constraint or regression case; do not turn module docs into a change log. An example is
useful when it clarifies a contract or typical call sequence, especially at crate and public API
boundaries.

Document application functions and methods in private modules too. A brief name does not tell a
reader who calls an operation, which state it changes, or why it is separate from neighboring
operations. Put the useful contract at the function rather than relying on a distant module guide.
Document handwritten trait methods when the implementation adds a local contract: serialized
representation, validation, default policy, ordering, redaction, or cleanup effects. The trait name
explains the mechanism, but rarely explains these application choices. Avoid comments that only
repeat the signature; descriptive tests likewise need context only when their scenario leaves an
important expectation implicit.

Keep provider DTOs, domain values, database rows, and CLI JSON shapes distinct in prose as well as
code. Document the difference between a complete collection and an incomplete observation where the
distinction controls stored membership. Keep examples practical: opening an existing archive and
performing an offline read explains more than constructing a type without using it.

Executable examples need their own reader contract. Explain why the example exists, which API
boundary or design choice it demonstrates, required inputs and setup, how to run it, observable
effects, and how to interpret its output. State the limits of its coverage where readers might
otherwise generalize from it. For performance probes, identify the timed region, excluded setup,
workload, and measurement limits so a sample timing does not imply a full workflow benchmark.

## Review documentation as a contract

Check changed commands, paths, options, links, and examples against the current checkout. Check
Rustdoc examples with `cargo doc` and tests where applicable. When the reason for a constraint is
unclear, inspect the implementation, relevant tests, and introducing history before writing one.
Keep uncertainty explicit rather than presenting an inference as a guarantee.

Review a page from two entry points: the intended reading path and a direct landing from search or
Rustdoc. For source docs, open individual module files as if their parent module were unknown; check
that the first screen establishes purpose, relationships, and the important invariant. Remove
duplicated setup and stale promises. Preserve explanations that prevent a mistake, even when they
take more words. Wrap Markdown prose at 100 columns and run the repository's rumdl and markdownlint
checks after editing.

This guidance distills the local Practice documentation workflow and Girt documentation standard for
Forgesync's archive, sync, search, and triage workflows.
