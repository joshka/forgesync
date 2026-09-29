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

## Next passes

1. Review `command/search.rs`, `command/retry.rs`, and `command/cluster.rs` for phase boundaries and
   repeated cancellation setup. Extract only a shared lifecycle operation that actually reduces
   parameter traffic and navigation.
1. Review TUI browser input branches for multi-effect actions. Preserve key-to-action dispatch as a
   visible map; move state changes into nearby `App` methods when an arm becomes a workflow.
1. Review engine sync and store observation application as separate correctness-sensitive changes.
   Keep provider I/O outside store transactions and preserve partial collection rules.
1. Review the new private-function comments alongside their implementations when changing a
   workflow. Rustdoc's `missing_docs` lint covers the public surface only; comments that merely
   repeat a name should be improved as the surrounding code becomes clearer.
