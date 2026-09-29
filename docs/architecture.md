# Module map

Forgesync follows a local discussion from GitHub acquisition into an SQLite archive, then answers
offline reads and presents maintainer decisions. This map names the owner of each transition.

| Crate              | Owns                                                             | Main path                                   |
| ------------------ | ---------------------------------------------------------------- | ------------------------------------------- |
| `forgesync-core`   | Identities, normalized content, observations, coverage, outcomes | Provider values become domain values        |
| `forgesync-github` | Typed responses, transport, pagination, normalization            | GitHub response becomes a typed observation |
| `forgesync-store`  | Archive lifecycle, SQL, ordering, recovery, local decisions      | Observation becomes durable state           |
| `forgesync-engine` | Sync, refresh, search, clustering policy                         | Request becomes a workflow report           |
| `forgesync-cli`    | Arguments, configuration, process output and exit codes          | User command becomes an engine request      |
| `forgesync-tui`    | Local navigation, actions, rendering                             | Archive state becomes an interactive view   |

For a sync change, start at the CLI command, follow the engine sync operation through the GitHub
resource method and store application method, then inspect the matching fixture and regression test.
CLI parsing and execution share `crates/forgesync-cli/src/command/`; the parsed command type owns
its process `run` method. Shared configuration and result rendering remain separate modules. For an
offline read, start at the engine request, inspect the store query, then the CLI or TUI
presentation. The engine accepts an opened archive; the store alone decides transaction and
observation ordering. GitHub code does not open the archive.

Within engine sync, `review_collection` owns the durable lifecycle shared by reviews and review
threads. `ReviewSync` selects the family and prepares against a metadata result; preparation either
finishes immediately or yields a reserved, head-aware `ReviewCollection`. The collection owns
staging counts and consuming completion/failure operations. `reviews` follows REST page links, while
`review_threads` owns GraphQL cursor traversal and cycle detection. Provider traversal and archive
finalization can therefore be read independently without repeating their shared policy.

Repository sync uses `jobs` for lookup and scope traversal, `thread_job` for durable parent scans,
and `repository_work` for immutable services and selected scope. `comment_job` owns repository-wide
comment accounting; `comments` owns a reserved per-discussion collection. `pull_requests` owns
selected metadata/review jobs, and `family_job` holds their IDs, accumulated results, and terminal
ledger writes. `metadata` reserves and applies the head observation before review acquisition.

Within the store, `observations/apply` selects a canonical parent and applies its independently
ordered evidence; `observations/thread_rows` owns the payload binding map and its private read/write
representations. `StoredThreadObservation` keeps canonical content and complete-evidence positions
independent. `ThreadPayloadUpdate` makes an optional evidence advance explicit.
`families/application` checks reserved generations and applies complete membership or incomplete
coverage inside the transaction opened by `families/finish`. These owners borrow the transaction and
never commit it.

Refresh `coordinator` binds services and validated repository scope to `RefreshExecution`; its stage
methods preserve independent reports. TUI `query/operations` owns task and message lifetimes, while
`query/action` owns the selected action request and terminal status.

Clustering `candidates` validates stable input, `evidence` selects sparse eligible edges,
`references` interprets title/body mentions, and `components` applies bounded grouping and
representative policy. Evidence selection shares one score predicate across both selection phases.

Store thread reads let `ThreadQuery` build bound filters and sorting before page coverage assembly.
Embedding `read` keeps raw candidate order, hydrates current evidence, and accepts only complete
valid chunk groups. Raw candidates determine pagination even when every vector is rejected.

GitHub transport `request` owns budgeted attempts and trusted redirect traversal; `client` owns
construction and endpoint entry points. CLI `command/embedding_service` prepares configured clients
without making provider requests, preserving configuration versus initialization failures.
`command/interruption` scopes the Ctrl-C listener and lends its token to command workflows; engine
operations retain ownership of interrupted reports and durable cleanup. CLI `reports/detail` and TUI
`view/detail` build named presentation sections from loaded projections.

CLI `command/progress` owns the bounded advisory channel and stderr task shared by sync and retry.
It closes local delivery before draining and aborts the task when its command owner is dropped.
`command/retry` owns durable failure selection and closes its archive before rendering any outcome.
Engine `runs/planning` interprets recorded scope, resolves failure repositories, and merges and
orders minimal retry requests without provider I/O; `runs` executes those requests through sync.

The crates expose named concept modules rather than blanket root exports. The main sync, search,
cluster, storage, GitHub transport, CLI command, and TUI rendering paths are grouped by behavior.
The [maintainability plan](maintainability-plan.md) tracks the remaining large and mixed-purpose
modules. Use the module path as the first navigation clue, then read the adjacent tests.

## Child-family transaction phases

Reservation, staging, and finalization share one ordering contract but own different effects.
`ReservedGeneration` compares a proposed source clock and sequence, then writes a reservation and
recoverable generation. `PageWrite` validates that generation, recognizes identical replay, and
stores provisional pages with received counts. `FamilyApplication` promotes complete membership or
records partial coverage. Each archive operation opens and commits its own transaction; these owners
only borrow its connection.

Reuse is a separate read. `MembershipExpectation` names the independent evidence available from the
parent, and `FamilyFreshness` verifies source clock, review head when required, complete coverage,
and canonical membership count. None of these reads promote staged pages or change coverage.

## Terminal picker and writer display state

`app/repositories` owns repository rows, highlight, applied scope, and the pending read generation.
The synthetic all-repositories row is a cursor position; the applied filter retains a repository
value independently of refreshed row order. The owner rejects stale replies, clamps cursor bounds,
and retains loaded rows when a refresh fails. App coordination resets thread and detail state when
the user applies a new scope.

`app/operation` owns the displayed writer generation and an idle/running enum. A running state
contains its required label and optional progress snapshot. Input reads whether cancellation is
needed before quitting; the view reads presentation facts. Query tasks still own execution and
cancellation. The display accepts current-generation progress only while running, and completion
clears transient state before returning the status to the app.
