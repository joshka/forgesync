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
ordered evidence; `observations/thread_rows` owns the payload binding map. `families/application`
checks reserved generations and applies complete membership or incomplete coverage inside the
transaction opened by `families/finish`. These owners borrow the transaction and never commit it.

Refresh `coordinator` binds services and validated repository scope to `RefreshExecution`; its
stage methods preserve independent reports. TUI `query/operations` owns task and message lifetimes,
while `query/action` owns the selected action request and terminal status.

Clustering `candidates` validates stable input, `evidence` selects sparse eligible edges,
`references` interprets title/body mentions, and `components` applies bounded grouping and
representative policy. Evidence selection shares one score predicate across both selection phases.

The crates expose named concept modules rather than blanket root exports. The main sync, search,
cluster, storage, GitHub transport, CLI command, and TUI rendering paths are grouped by behavior.
The [maintainability plan](maintainability-plan.md) tracks the remaining large and mixed-purpose
modules. Use the module path as the first navigation clue, then read the adjacent tests.
