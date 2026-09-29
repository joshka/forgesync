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
For an offline read, start at the engine request, inspect the store query, then the CLI or TUI
presentation. The engine accepts an opened archive; the store alone decides transaction and
observation ordering. GitHub code does not open the archive.

The crates expose named concept modules rather than blanket root exports. The main sync, search,
cluster, storage, GitHub transport, CLI command, and TUI rendering paths are grouped by behavior.
The [maintainability plan](maintainability-plan.md) tracks the remaining large and mixed-purpose
modules. Use the module path as the first navigation clue, then read the adjacent tests.
