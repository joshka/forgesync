# Terminal browser and maintainer actions

The terminal interface reads local archive data through the same engine queries as the CLI. It opens
an existing archive for writing so that local cluster decisions and explicit acquisition actions can
use the same fenced engine APIs as their CLI commands. It never creates or migrates an archive.

Start it with an initialized archive from an interactive terminal:

```sh
forgesync tui
```

The TUI uses the configured/default database; `--archive PATH` can override it. The CLI loads
archive configuration after terminal/output validation, then resolves GitHub credentials for
registered repository hosts at startup. Browser queries, search, coverage, failures, and cluster
inspection remain local. Sync, refresh, and retry actions contact GitHub. The TUI does not construct
embedding clients or perform embedding analysis.

The normal CLI build enables the `tui` feature. Builds made with `--no-default-features` omit the
command. Interactive output is not compatible with `--json`.

## Keyboard controls

| Key                   | Action                                                                         |
| --------------------- | ------------------------------------------------------------------------------ |
| `Tab` / `Shift+Tab`   | Move focus between repositories, discussions, and detail                       |
| `↑` / `k`, `↓` / `j`  | Move or scroll in the browser and cluster list                                 |
| `Enter`               | Apply a repository, open discussion detail, or inspect cluster members         |
| `/`                   | Enter a local keyword search; `Enter` submits it and `Esc` cancels it          |
| `n` / `p`             | Move to the next or previous page of discussions                               |
| `r`                   | Reload the current discussion page                                             |
| `g`                   | Show generated clusters, including retired clusters                            |
| `d`                   | Dismiss or restore the selected cluster locally                                |
| `e` / `i`             | Exclude or include the selected cluster member locally                         |
| `k` in cluster detail | Set the selected member as the local canonical discussion                      |
| `s`                   | Sync the selected repository or all registered repositories                    |
| `R`                   | Refresh selected repositories, including comments, reviews, and review threads |
| `c`                   | Show archive coverage, writer ownership, and local health                      |
| `f`                   | Show recent failed and unfinished runs                                         |
| `t` in failures       | Retry unresolved work from the selected run                                    |
| `Esc`                 | Return to browsing or leave cluster detail                                     |
| `q` / `Ctrl+C`        | Exit, or request cancellation while an action is active                        |

## Background work

Queries and maintainer actions run in Tokio tasks, so SQL and provider work do not block keyboard
input or redraws. Query generations discard stale results. Progress snapshots use bounded
non-blocking delivery; a slow terminal can lose intermediate snapshots without slowing archive work.

Only one maintainer action runs at a time. Pressing `q` or `Ctrl+C` during an action requests
cancellation. The TUI waits for the engine to record and report its interrupted result, then stays
open so that the result remains visible. Press `q` again to exit. Failed actions, partial reports,
and cancellation are shown with their actual outcome, never as successful completion.

When sync, refresh, or retry cannot take the archive writer lease because another process holds it,
the TUI reports the lease owner and expiry. Local cluster decisions use the same archive fencing and
show lease contention as an action failure.

The layout stacks its panes on narrow terminals and uses side-by-side panes on wider terminals.
Terminal state is restored after normal exit, I/O errors, and panics.

For query/write overlap, lock waits, provider budgets, and abandoned writer recovery, see the
[user manual](user-manual.md#queries-retries-and-timeouts-during-sync).
