# Source shape audit

This inventory records the command ownership, function documentation, and dispatch review across all
six crates. It distinguishes a large match that hides behavior from a compact match that states
policy. Keep this as maintenance evidence; [Rust conventions](rust-conventions.md) holds the
reusable rules.

| Crate  | Finding                                                                                                                                                                                            | Decision                                                                                                                                                                   |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CLI    | Parallel `args/` and `commands/` trees split every command type from its execution. Archive, thread, and run dispatchers nested archive I/O and rendering inside their arms.                       | Consolidated into `command/`. Parsed types own `run`; dispatch points to those methods. Shared provider setup, retry, output, and configuration retain their own owners.   |
| TUI    | `App::apply` mixed generation checks, result handling, and state mutation in one message match. Browser key handling still performs navigation and query construction in several arms.             | Extracted one result method per message. Review browser input next; preserve concise key mappings where they already name a local action.                                  |
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
- Reservation APIs describe ordering, rejected generations, fencing, errors, and canonical effects.
- Semantic-search docs distinguish archived document vectors from the network-generated query
  vector.

Remaining review surfaces:

- Broader function and state review beyond the selected traversal and diagnostic slices.
- Store reservation and staging transaction phases and meaningful request concepts.
- Restricted visibility and imports routed through parent module aliases.
- Public API examples and deeper documentation contracts across all crates, including rendered docs.
- Tool currency. Workspace dependency audits, including aggressive updates, report no outdated
  dependencies in this checkout.
- Hosted Linux, Intel macOS, and Windows validation; local checks cannot establish those results.

Retain simple domain mappings and linear SQL binding maps when splitting them increases navigation.
Review exceptions on their actual contracts rather than using line counts as proof of completion.
