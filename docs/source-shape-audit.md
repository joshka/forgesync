# Source shape audit

This inventory records the command ownership, function documentation, and dispatch review across all
six crates. It distinguishes a large match that hides behavior from a compact match that states
policy. Keep this as maintenance evidence; [Rust conventions](rust-conventions.md) holds the
reusable rules.

| Crate  | Finding                                                                                                                                                                                            | Decision                                                                                                                                                                   |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CLI    | Parallel `args/` and `commands/` trees split every command type from its execution. Archive, thread, and run dispatchers nested archive I/O and rendering inside their arms.                       | Consolidated into `command/`. Parsed types own `run`; dispatch points to those methods. Shared provider setup, retry, output, and configuration retain their own owners.   |
| TUI    | `App::apply` mixed generation checks, result handling, and state mutation in one message match. Browser key handling mixed navigation and query construction in several arms.                      | Extracted one result method per message. Browser and triage keys now dispatch to named actions; selection and query effects have local owners.                             |
| Engine | `retrieve_threads` held keyword, semantic, hybrid, and fallback workflows inside one match. Sync and refresh coordinate archive, clients, requests, and cancellation without one natural receiver. | Extracted search paths and fallback. Keep sync and refresh as workflow functions; review large phase helpers on their own merits.                                          |
| Store  | Archive lifecycle and storage operations already live on `Archive`. Observation application is a long, correctness-sensitive transaction; ordering matches express the replacement policy.         | Retain the `Archive` receiver and ordering matches. Documented helper contracts; review observation phases separately while preserving transaction and completeness rules. |
| GitHub | Transport behavior is on `GitHubClient`. Resource fetch and normalization functions are grouped by provider family; pagination and retry matches are mostly local policy.                          | Keep resource families visible instead of flattening them into one client implementation. Documented scope, pagination, and failure helper contracts.                      |
| Core   | Checked identities and values already own their validation methods. Remaining matches are small value or domain-state mappings.                                                                    | Keep current ownership and document private identity and coverage helpers.                                                                                                 |

## Follow-up audit

The initial ownership pass addressed selected workflows in every crate; it did not establish that
all functions, APIs, and test bodies had received the same review. Completion claims must name the
reviewed surface and its evidence, rather than extrapolating from a crate or area label.

Implemented follow-ups:

- Browser keys dispatch to named actions. `Movement` owns bounded position calculations; each pane
  owns selection, invalidation, and scrolling effects.
- Embedding reuse and replacement use `EmbeddingPolicy`. Refresh materialization has a stage owner
  with repository traversal, document outcomes, and vector batch accounting.
- Search input and cluster setup tests spell out their small scenarios without loops.
- Semantic search uses `SemanticSource` for candidate scope and `SemanticRanking` for bounded
  scores. Candidate availability precedes query embedding; every page uses the same filter scope.
- Changed browser, refresh embedding, and semantic modules import dependencies at their owners.
- Archive diagnostics separate schema validation, lease observation, and durable work queries.
- Reservation uses `ReservedGeneration` for ordering and persistence; staging uses `PageWrite` for
  generation validation, replay comparison, insertion, and count accounting. Commit remains with the
  archive operation.
- Family reuse uses `MembershipExpectation` and `FamilyFreshness` for source clock, head, coverage,
  and membership evidence. No behavioral bool selects its validation policy.
- Global, search, failure, cluster, and member input dispatch names the action instead of performing
  multi-step state changes inside key-match arms.
- Reservation APIs describe ordering, rejected generations, fencing, errors, and canonical effects.
- Semantic-search docs distinguish archived document vectors from the network-generated query
  vector.

Search preparation now separates request conversion, fallback validation, query-client setup, and
read-only execution. The execution owner keeps the request, recipe, and optional client coherent; it
closes the archive before rendering and retains cancellation for engine retrieval. Module docs
correct the stale offline claim for service-backed semantic queries.

Sync preparation and acquisition now belong to its parsed command, with one archive-close point and
typed selection/client/engine failures. Nearby request cases and existing CLI sync contracts cover
selection and output behavior.

Retry now has a command-local request owner and typed boundary failures. Its outer method closes the
archive once before rendering. Sync and retry share a progress owner that drains before result
output and aborts on unexpected drop. Engine retry planning names failure selection, repository
resolution, scope merging, and deterministic ordering; recorded inclusion facts form one concept
instead of behavioral boolean parameters.

The thread SQL projection and update input live with their column mappings in a private module,
using ordinary public items within that implementation boundary. Their docs explain independent
content/evidence positions and the optional evidence advance. Observation children import external
dependencies directly. The source clock columns retain deliberate crate visibility within the public
observation module, with their SQL shape invariant documented.

Terminal repository state now has a `RepositoryPicker` owner. Highlight and applied scope are
separate, and the applied repository is retained across reordered or empty refreshes. This fixes an
index-based targeting risk while keeping stale replies and failed refreshes isolated. A documented
idle/running writer display replaces independent busy, label, and progress fields; it cannot retain
active progress after completion. Discussion list/detail, coverage, failed-run, and cluster
list/detail now have documented owners with direct transition tests. Query tasks own cancellation
and shutdown draining. Local reads share a documented scheduling context, and discussion requests
own filter preparation and retrieval mode. Progress forwarding has a producer/task owner with normal
draining and unexpected-drop cleanup. Query requests document each intent; local cluster transitions
use separate named variants rather than boolean-selected commands. Renderer dependencies name their
defining modules directly. Failure summary selection and ledger projection have dedicated owners and
tests.

Discussion replies now bind generation, offset, and page together. The detail pane uses explicit
empty/loading/ready/failed states and invalidates old selections before beginning another read.
Coverage and failed-run owners document their retained-cache rules. Cluster detail clears another
cluster's members before a pending read, preventing keyboard decisions from targeting old data;
same-cluster refresh can retain its cache. Typed query messages document that asynchronous boundary
in their own module. App state is reduced to panel coordination and shared status rather than each
panel's internal mutation protocol.

Remaining review surfaces:

- Broader function and state review beyond the selected traversal and diagnostic slices.
- Review additional store operations beyond the reservation, staging, and freshness slices.
- Restricted visibility and imports routed through remaining parent module aliases. Engine-root
  passthrough exports are removed; child-family freshness now imports from the actual owners.
- Deeper module and item documentation contracts across all crates. All six crate introductions now
  have expanded entry guidance and have been reviewed in rendered Rustdoc.
- Hosted behavior of the refreshed CI actions. Local workflow syntax validation passes.
- Hosted Linux, Intel macOS, and Windows validation; local checks cannot establish those results.

Retain simple domain mappings and linear SQL binding maps when splitting them increases navigation.
Review exceptions on their actual contracts rather than using line counts as proof of completion.

## Completion checklist

These requirements preserve the full maintainer request. A passing compiler or a selected slice is
not evidence for every row. Keep this checklist open until its scope has actually been reviewed.

| Requirement                                              | Current evidence                                                                  | Remaining work                                                                                   |
| -------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Meaningful small modules, broad shallow navigation       | Six crate module maps and selected vertical slices                                | Inspect current long functions and multi-effect match arms throughout the workspace              |
| Command/state ownership and top-down reading             | Unified CLI command tree; workflow and transaction owners                         | Review remaining CLI and engine orchestration, TUI result application, and presentation branches |
| Domain types for related inputs; no behavioral bools     | Embedding policy and family membership expectations                               | Review every remaining bool parameter and broad signature for an intentional contract            |
| Explicit local imports and deliberate visibility         | No `use super::*`; changed workflow imports name owners                           | Remove remaining parent import preludes and document public-boundary exceptions                  |
| Every application function documented                    | All handwritten production methods have comments, including local trait contracts | Assess comment depth across all items; presence alone does not establish a useful contract       |
| All modules and items teach their role and relationships | Expanded roots, workflow modules, examples, and API contracts                     | Review remaining private type/constant docs, field contracts, module maps, and rendered pages    |
| Linear nearby tests with clear scenarios                 | Split suites and direct keyboard/cluster setup                                    | Inspect remaining scenario branches and fixture burden; preserve meaningful data-driven cases    |
| Current dependencies and tools                           | Full direct/transitive aggressive audit reports no outdated dependencies          | CI actions refreshed against upstream; native tools checked and nightly refreshed                |
| Reusable guidance recorded                               | Linked documentation and Rust conventions guides                                  | Record any additional recurring findings at the owning guide                                     |
| Formatting and local gates                               | Previous follow-up passed every local gate                                        | Rerun focused and workspace gates for each subsequent implementation batch                       |
| Hosted platform evidence                                 | Native smoke/package matrix is configured                                         | Obtain current Linux, Intel macOS, and Windows execution results                                 |

The syntax inventory distinguishes production functions from tests and trait implementations. It
measures actual function bodies, excluding braces in strings. The initial continuation found 38
production functions over 50 body lines and 113 match arms spanning at least five lines. These are
inspection candidates, not defects by themselves: simple error-code tables, DTO construction, and
linear SQL binding maps may remain. The item pass also found undocumented private representations
and policy constants; trait-associated aliases inherit their trait contract. Documentation presence
and line counts do not establish documentation quality.

### Current tool evidence

Checked on 2026-09-29: rumdl 0.2.77 matches the
[upstream release](https://github.com/rvben/rumdl/releases/tag/v0.2.77); actionlint 1.7.12 matches
[its upstream release](https://github.com/rhysd/actionlint/releases/tag/v1.7.12).
The npm registry reports markdownlint-cli2 0.23.3, matching the installed command. Nightly is
refreshed to 2026-09-29, with rustc 1.101.0-nightly and its matching rustfmt. The repository's Rust
1.98.1 toolchain remains its deliberate build and validation baseline.

CI and release workflows now use checkout v7, setup-python v7, upload-artifact v7, and
download-artifact v8. The upstream usage and input contracts preserve this repository's checkout,
Python setup, and zipped artifact transfer:
[checkout](https://github.com/actions/checkout/blob/v7/README.md),
[setup-python](https://github.com/actions/setup-python/blob/v7/README.md),
[upload-artifact](https://github.com/actions/upload-artifact/blob/v7/README.md), and
[download-artifact](https://github.com/actions/download-artifact/blob/v8/README.md).
Actionlint passes both workflow files. Hosted execution remains separate evidence.

## Bounded remaining implementation work

The remaining cleanup is eight batches followed by one acceptance pass. This replaces the earlier
open-ended follow-up targets; the requirements in the completion checklist remain the acceptance
criteria. Completed CLI embedding/refresh preparation, deterministic chunk construction, and
embedding document selection are evidence for this inventory, not new future tasks.

### 1. Embedding execution — implemented

Selection and deterministic chunks have local owners. `batches` owns request grouping and response
order, `scheduling::BatchScheduler` owns bounded workers and outcome dispatch, and
`execution::EmbeddingWriter` owns service identity, fenced persistence, and lease release. Fatal
exits now abort and drain outstanding requests before fence release, with direct cleanup cases and
existing partial-success/retry integration evidence. Broad signature and documentation acceptance
still belong to batch 7; this phase does not claim to complete the workspace cleanup.

### 2. Cluster construction — implemented

Implemented: `ClusterBuildLease` owns renewal, cooperative interruption, and release ordering. Three
nearby lease cases and generation integration establish error preservation, release, child cleanup,
and caller-token isolation. CLI build preparation now belongs to the parsed arguments, with
canonical identity/policy conversion cases and archive-close-before-presentation ordering. Refresh
traversal now has a stage owner and direct outcome-policy cases. Engine evidence preparation now has
a repository-scoped snapshot and named generation projection. Store input preparation and durable
identity matching now have named modules and local row contracts. Overlap ranking now uses named
membership evidence with six direct assignment cases. Transactional writes now have an application
owner, with a visible single archive commit and linear SQL bind maps retained at their private
owner.

Review CLI `command/cluster/build`, engine `clustering/build` and `refresh/clusters`, and store
`clusters/generation`. Finish when preparation, analysis, and generation persistence have coherent
owners and the writer lease/transaction boundaries remain explicit. Preserve deterministic proposal
ordering, decision application, and existing generation fencing.

### 3. Search — implemented

Implemented: hybrid fusion has named identity-union and per-discussion evidence owners, coupled
semantic evidence, explicit projection/order operations, and local formula documentation. Ranked
retrieval now has its own coordinator and a validated window owner. Named semantic and hybrid
projections keep scoring/fusion separate from acquisition and fallback. Keyword and semantic helpers
import actual dependency owners; scoring limits live beside their scoring implementation.

The coordinator retains a linear acquisition sequence: validate window, load optional keyword
candidates, obtain semantic evidence, classify fallback, read coverage, and project. These explicit
I/O inputs do not require an application-context wrapper. Final current-tree workspace validation is
running. Broader documentation and test review remain in their respective bounded batches.

### 4. Acquisition

Review engine `enumeration/scan`, sync metadata completion, store enumeration finalization, and
nearby replay scenarios. Finish when scan completion and metadata outcome transitions expose their
ordering, cancellation, and failure contracts, with linear replay cases. Preserve completeness and
checkpoint invariants rather than simplifying them into last-write-wins behavior.

### 5. Store operations

Review remaining detail/timeline, coverage, diagnostics, cluster-query, document-write, and member
decision operations. Finish when each combines a coherent read projection or explicit transaction
story, and its errors and partial effects are documented. Straightforward SQL binding and column
maps can stay together with a recorded reason; they do not require helper-per-column extraction.

### 6. CLI and TUI presentation

Review CLI detail/archive summaries and TUI coverage, cluster detail, search editing, and event-loop
policy. Finish when presentation decisions and state-changing dispatch have named local owners,
selection cannot target stale data, and meaningful rendering behavior is covered. Preserve the
configured output contracts and terminal cancellation behavior.

### 7. Workspace conventions and documentation

Make one complete pass over the existing six crates' modules, items, function signatures, imports,
and visibility. Review documentation depth, remaining parent import preludes, behavioral boolean
parameters, broad signatures, and representation placement. Finish with each finding fixed or an
explicit justified exception, usable module introductions, item contracts at their owning level, and
an accurate module map. Record recurring rules in the linked guides. Documentation presence alone
does not satisfy this pass.

### 8. Tests

Review the existing suites for scenario loops, branches, opaque behavior helpers, distance from the
code, and weak assertions. Finish with straightforward named scenarios, construction fixtures whose
setup is clear, appropriate nearby/separate suites, and focused rendering snapshots where they
establish observable behavior. Retain genuine complete-catalog/property checks with their purpose
explained. Do not rewrite every assertion simply to introduce rstest or insta.

### Acceptance pass and stopping rules

- Reconcile every explicit maintainer requirement against current source and recorded evidence.
- Give every inspection candidate one disposition: fixed or retained with a concrete reason. Line
  counts trigger inspection, not mandatory extraction or repeated work on newly named helpers.
- Newly noticed aesthetic opportunities do not extend these batches unless they violate an agreed
  requirement or are consequences of the changes being made.
- Update the module map, guidance, and completion checklist to describe the actual implementation;
  remove completed work from the remaining inventory.
- Run focused checks and all applicable local workspace formatting, lint, test, build, and strict
  public/private documentation gates on the final tree. Confirm dependency/tool evidence remains
  current without turning the cleanup into another dependency redesign.
- Keep new features, generic frameworks, and deferred distribution out of this scope.

Hosted Linux, Intel macOS, and Windows results remain separate validation evidence. Workflow syntax
and local gates do not establish those results. No remote publication or hosted execution has been
performed as part of this cleanup.
