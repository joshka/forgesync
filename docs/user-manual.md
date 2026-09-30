# Forgesync user manual

Forgesync keeps a local archive of GitHub issues and pull requests. Acquire the repositories you
care about, search and inspect their archived discussions, and organize related discussions into
local triage groups. Your archive is an SQLite database on your machine. Forgesync never closes,
edits, or comments on GitHub issues or pull requests.

## Contents

- [First session](#first-session)
- [Acquire and update discussions](#acquire-and-update-discussions)
- [Search and inspect](#search-and-inspect)
- [Understand coverage](#understand-coverage)
- [Use the terminal browser](#use-the-terminal-browser)
- [Enable semantic search](#enable-semantic-search)
- [Organize related discussions](#organize-related-discussions)
- [Recover and maintain an archive](#recover-and-maintain-an-archive)
- [Use JSON and diagnostics](#use-json-and-diagnostics)
- [Command reference](#command-reference)

## First session

### Install

From a checkout, install with Rust 1.98 or newer:

```sh
cargo install --path crates/forgesync --locked
forgesync --version
```

The default build includes the terminal browser. Add `--no-default-features` to the installation
command to build without it. Native release packaging is described in
[installation and operations](installation.md).

### Create an archive

Initialize your normal database once:

```sh
forgesync archive init
```

Commands now select the same normal database automatically. Its default location is
`~/.local/share/forgesync/archive.sqlite` on Linux/macOS, or `%APPDATA%\forgesync\archive.sqlite` on
Windows. Unix `XDG_DATA_HOME` can change the base directory. `archive init` creates missing parent
directories but refuses to overwrite an existing database. Other commands require an existing
archive and never create or migrate one automatically.

To select a different normal database, set `[archive] path` in the user config. Forgesync loads
`~/.config/forgesync/config.toml` on Linux/macOS (respecting `XDG_CONFIG_HOME`) or
`%APPDATA%\forgesync\config.toml` on Windows. For example:

```toml
[archive]
path = "/absolute/path/to/forgesync.sqlite"
```

Relative TOML paths are relative to the config file; `~` and environment variables are not expanded.
Use `--archive PATH` to override the normal database for one command. Global options may appear
before or after the command. `archive status` reports the selected database path. If you already
have a Forgesync database, configure its path and open it normally instead of initializing it again.

Use a separate database from any Gitcrawl installation. Forgesync cannot open or import a Gitcrawl
archive.

### Acquire your first repository

Authenticate with the GitHub CLI:

```sh
gh auth login
forgesync sync ratatui/ratatui
```

Alternatively, supply a GitHub token through `GITHUB_TOKEN`. The token must have access to the
repositories you select. Without a usable token, Forgesync can attempt anonymous access; private
repositories and evidence requiring authentication will remain inaccessible, and provider rate
limits still apply. Acquisition registers repositories in this archive; there is no separate
repository-add command.

The command prints a summary of acquired work. A partial or deferred outcome means some selected
work remains, even if discussion data was stored successfully. Inspect the archive afterward:

```sh
forgesync archive status
forgesync thread list --repo ratatui/ratatui
forgesync search "terminal resize"
```

These reads use the archived data and need no GitHub credentials or network connection.

## Acquire and update discussions

### Choose repositories and evidence

Sync one or several repositories, or update all repositories already registered in the archive:

```sh
forgesync sync ratatui/ratatui rust-lang/rust
forgesync sync --all
```

Replace the example repositories with your own. `--all` selects the local registry; it does not
discover every repository on GitHub or in your account. It cannot be combined with repository
arguments.

Basic sync acquires issue and pull-request discussion records. Request additional evidence
explicitly:

```sh
forgesync sync ratatui/ratatui \
  --with comments,reviews,review-threads
```

| Family           | Acquired evidence                                              |
| ---------------- | -------------------------------------------------------------- |
| `comments`       | Issue and pull-request discussion comments                     |
| `reviews`        | Submitted pull-request reviews and reviewer identities         |
| `review-threads` | Pull-request review threads, nested comments, and resolution   |

Review acquisition also obtains the pull-request metadata needed to interpret its head context.
Issues do not have pull-request review families. Additional families are optional; acquiring them
does not make keyword search dependent on model configuration.

### Choose source state

The default sync fetches open discussions and performs a closed-discussion sweep using the stored
successful sweep position. Choose an explicit scope when needed:

```sh
forgesync sync ratatui/ratatui --state open
forgesync sync ratatui/ratatui --state closed
forgesync sync ratatui/ratatui --state all
```

`open` fetches only open discussions. `closed` uses the successful closed-sweep watermark; it is not
a request to enumerate all historical closed discussions every time. `all` performs a full
open-and-closed enumeration. Local query filters only select stored rows and never acquire missing
history.

### Refresh an established archive

Run `sync` again to update the selected scope. Successfully stored work survives failures in later
pages or independent evidence families. Repeating acquisition does not duplicate canonical rows.

`refresh` combines acquisition with optional analysis. Without analysis flags it performs sync:

```sh
forgesync refresh ratatui/ratatui \
  --with comments,reviews,review-threads
```

See [semantic search](#enable-semantic-search) for analysis settings. Refresh reports its stages
independently, so successful acquisition remains useful if a later analysis stage fails.

## Search and inspect

### Find text

Default keyword search matches archived discussion titles and bodies. Quote a query containing
spaces so the shell passes it as one argument:

```sh
forgesync search "terminal resize"
forgesync search "terminal resize" \
  --repo ratatui/ratatui --kind issue --state open
```

Ordinary keyword mode treats punctuation as separators rather than interpreting query operators. Use
explicit advanced FTS mode for phrases, boolean operators, or grouping:

```sh
forgesync search '"terminal resize" OR viewport' \
  --mode advanced-fts
```

Malformed FTS expressions produce an error. Neither keyword nor advanced FTS mode contacts GitHub or
an embedding service. They do not search comments or reviews as separate text records.

### Filter, sort, and page

Search and `thread list` share these filters:

| Option     | Values and behavior                                          |
| ---------- | ------------------------------------------------------------ |
| `--repo`   | A registered repository; repeat the option to select several |
| `--kind`   | `issue` or `pr`; omit to include both                        |
| `--state`  | `all` (default), `open`, or `closed`                         |
| `--sort`   | `relevance`, `updated`, or `created`                         |
| `--limit`  | Results per page, 1–1000; default 20                         |
| `--offset` | Matching results to skip; default 0                          |

Thread listing defaults to newest source update first. Keyword search defaults to relevance. Use
time ordering when reviewing recent activity:

```sh
forgesync thread list --repo ratatui/ratatui \
  --kind pr --sort updated --limit 20 --offset 0
forgesync thread list --repo ratatui/ratatui \
  --kind pr --sort updated --limit 20 --offset 20
```

For JSON pagination, use the returned `next_offset` rather than guessing whether another page
exists. A concurrent sync can change the available rows between requests.

### Open a discussion

Use a repository and positive discussion number, or a GitHub issue/pull-request URL:

```sh
forgesync thread show 'ratatui/ratatui#1'
forgesync thread show https://github.com/ratatui/ratatui/issues/1
```

Replace `1` with a number returned by your archive. Showing a URL resolves it locally; it does not
download the discussion. Detail includes retained comments, review evidence, coverage, and a
chronological view of current source content. It is not a full revision history.

`OWNER/REPO` refers to GitHub.com. An HTTPS repository URL identifies another GitHub host
explicitly; use the corresponding host in discussion URLs too. Acquisition needs credentials for
that host.

## Understand coverage

An archive contains what you have acquired, rather than a live view of GitHub. Check both the
discussion content and the coverage of its evidence families:

```sh
forgesync archive status
forgesync thread show 'ratatui/ratatui#1'
```

| Coverage meaning | How to interpret it                                                 |
| ---------------- | ------------------------------------------------------------------- |
| Complete         | The selected collection was acquired completely; it may be empty    |
| Incomplete       | Acquisition did not establish the entire collection                 |
| Missing          | No accepted collection is available for that family                 |
| Failed           | Acquisition recorded a failure; retained content may still exist    |
| Deferred         | The family was intentionally not attempted; inspect its reason      |
| Unavailable      | The family is not available in the applicable source context        |
| Stale            | Retained evidence no longer matches the current source/head context |

Staleness is a freshness indication, not another collection size. A complete empty collection means
the source had no members in the acquired snapshot. Missing evidence means you cannot infer that.

An incomplete refresh preserves the last complete membership rather than treating unreceived members
as deleted. Old reviews can remain visible after the pull-request head changes, with stale coverage
identifying their earlier context. To improve coverage, acquire the needed families again or inspect
recorded failures as described below.

## Use the terminal browser

Start from an interactive terminal:

```sh
forgesync tui
```

Use `Tab` to move between repositories, discussions, and detail. Select a repository with `Enter`,
then select a discussion and press `Enter` to load its detail. Use arrow keys or `j`/`k` to move or
scroll, `n`/`p` to change discussion pages, `/` to enter a keyword query, and `r` to reload.

| Key             | Task                                                |
| --------------- | --------------------------------------------------- |
| `c`             | Inspect coverage, health, and writer ownership      |
| `f`             | Inspect failed or unfinished runs                   |
| `t` in failures | Retry unresolved work from the selected run         |
| `s`             | Sync the selected repository or all registered ones |
| `R`             | Refresh with comments, reviews, and review threads  |
| `g`             | Inspect generated clusters                          |
| `Esc`           | Return to browsing or leave cluster detail          |
| `q` / `Ctrl+C`  | Exit, or request cancellation of an active action   |

The TUI resolves credentials for registered hosts at startup. Browser queries remain local; sync,
refresh, and retry contact GitHub. The TUI does not perform embedding analysis. It requires a
writable existing archive and does not support `--json`.

During an action, `q` or `Ctrl+C` requests cancellation. The browser waits for the interrupted
result and stays open to display it; press `q` again to exit. Only one maintainer action runs at a
time. [TUI controls](tui.md) includes cluster decision keys and detailed background-work behavior.

## Enable semantic search

### Configure a service

Semantic search ranks discussions by stored embedding vectors instead of literal text matches.
Hybrid search combines keyword and semantic ranks. Both need compatible stored vectors and a service
capable of generating an embedding for your query.

Add these settings to your automatically loaded user config:

```toml
[documents]
recipe = "discussion_enriched"

[embeddings]
endpoint = "https://api.openai.com/v1"
model = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"
```

This is an example OpenAI-compatible configuration. Supply your service key in the named environment
variable, not in TOML. The endpoint, model, and document recipe identify compatible stored vectors.
See [configuration](configuration.md) for alternate services, dimensions, request limits, and retry
settings.

An explicit `--config PATH` takes precedence over `FORGESYNC_CONFIG`, which takes precedence over
the user config. Only an absent automatic file uses built-in defaults. Selected missing files,
unknown fields, and invalid values are errors. See [configuration](configuration.md) for path
details.

### Generate vectors, then query

```sh
forgesync embed ratatui/ratatui
forgesync \
  search "layout changes when the terminal gets smaller" --mode semantic
forgesync \
  search "terminal resize" --mode hybrid --repo ratatui/ratatui
```

Embedding generation sends archived document text to your configured service. Semantic and hybrid
queries send query text to that service; they still retrieve discussions from the local archive and
do not refresh GitHub data. Service use can incur charges according to your provider.

The default `discussion_enriched` recipe includes acquired comments and selected review evidence as
well as the title, body, and labels. `original_body` uses the title, body, and labels only.
Switching recipes or service identity can make existing vectors ineligible for the new query
settings.

Repeat `embed` after acquisition updates the archive. It reuses matching stored chunks and requests
missing ones; successful batches survive later batch failures. `--force` requests vectors again even
when compatible vectors already exist.

To acquire evidence, generate vectors, and build groups together:

```sh
forgesync refresh ratatui/ratatui \
  --with comments,reviews,review-threads --analyze embeddings,clusters
```

Use `--no-sync --analyze embeddings,clusters` to analyze the current archive without contacting
GitHub. Embedding analysis still contacts the embedding service. A local-only refresh must select at
least one analysis stage.

### Choose fallback deliberately

Semantic and hybrid queries normally report an error when retrieval cannot use the required vectors
or service. `--keyword-fallback` permits keyword results for eligible retrieval failures:

```sh
forgesync \
  search "terminal resize" --mode hybrid --keyword-fallback
```

Fallback does not bypass invalid configuration, missing credentials during client setup, or
cancellation. It is valid only with semantic or hybrid mode. JSON identifies the requested and
effective modes and a fallback reason; check these before treating results as semantic matches.
Semantic/hybrid pagination retains at most 10,000 ranked results.

## Organize related discussions

### Build and inspect groups

Clusters group current open discussions using compatible vectors already stored in the archive:

```sh
forgesync cluster build ratatui/ratatui
forgesync cluster list --repo ratatui/ratatui
forgesync cluster show 12
```

Replace `12` with an archive-local cluster ID from the list. Building clusters uses the configured
recipe, endpoint, and model identity, but does not read the service key or make embedding requests.
Generate vectors first if none are compatible.

Defaults are similarity threshold `0.80`, issue-to-pull-request threshold `0.93`, fanout `16`,
maximum group size `40`, and minimum group size `1`. `cluster build --help` lists their override
flags. Groups are proposals for review; similarity does not establish that discussions are
duplicates.

Build reports distinguish eligible discussions from those with compatible vectors. Partial vector
coverage preserves unseen groups and memberships; a complete generation can retire groups that no
longer qualify. Use `cluster list --include-retired` to inspect retired groups.

### Record local decisions

Use IDs and members from your own cluster detail:

```sh
forgesync cluster dismiss 12 --reason "Reviewed together"
forgesync cluster restore 12
forgesync cluster exclude 12 'ratatui/ratatui#1' \
  --reason "Different underlying issue"
forgesync cluster include 12 'ratatui/ratatui#1'
forgesync cluster canonical 12 'ratatui/ratatui#2'
```

Dismissal records a local triage choice. Exclusion marks a current member as excluded; it does not
erase its archived content. Canonical selection chooses the representative discussion. Decisions
persist across matched regenerated clusters and never change GitHub state. Include and canonical
selection operate on current members, rather than adding arbitrary discussions to a group.

## Recover and maintain an archive

### Inspect partial, failed, or interrupted acquisition

```sh
forgesync run list
forgesync run show 12
forgesync run retry 12 --family comments,reviews
```

Use a run ID from `run list` or the operation report. `run show` displays jobs and the failure
ledger. Retry contacts GitHub for unresolved failures, optionally restricted to `threads`,
`comments`, `pull-request-metadata`, `reviews`, or `review-threads`. It does not mean rerunning
every job in the original scope. For a general freshness update or interrupted scope without a
matching unresolved failure, run `sync` again.

On a rate-limit deferral, allow the provider budget to recover before retrying. On authentication or
permission failures, fix the credentials or access first. Completed pages and independent successful
families remain committed.

### Resolve writer contention

Mutating workflows use an archive writer lease. If another process owns it, inspect `archive status`
and let that process finish or cancel it through its own CLI/TUI. A read of lease status is only an
observation; a later write checks ownership again. Do not delete database files or edit lease rows
to bypass an active writer.

### Queries, retries, and timeouts during sync

Forgesync uses SQLite WAL mode. Queries can normally read committed archive data while a sync
process writes. GitHub requests happen outside SQL transactions; the database is not kept in a write
transaction while waiting for the network. Multi-part detail and diagnostics can observe commits
between their separate reads; they are not a frozen snapshot of the whole sync.

There is still one archive-wide application writer lease. Another sync, retry, embedding operation,
cluster build, or local cluster decision cannot take over that lease while its owner is active, even
for a different repository. A held lease is an ownership conflict, not a request to wait until the
whole active workflow finishes. Inspect its owner with `archive status`, then retry after the active
action finishes. Plain queries do not acquire this workflow lease.

Timeouts have different purposes:

| Boundary          | Current behavior                                                                                   |
| ----------------- | -------------------------------------------------------------------------------------------------- |
| SQLite lock wait  | Connections wait up to five seconds for SQLite lock contention; this is not a whole-query deadline |
| GitHub request    | Default timeout is 30 seconds per request attempt; retries have a 120-second budget                |
| Credential helper | GitHub CLI token discovery has a five-second subprocess timeout                                    |
| Sync writer lease | Expires after 60 seconds without renewal; active sync renews every 20 seconds                      |

Neither the SQLite lock wait nor the provider retry budget is a deadline for an entire sync across
many pages or repositories. A slow or failed provider request can produce partial acquisition while
earlier pages remain stored. It does not make local reads wait for the provider timeout. If a writer
crashes, its unrenewed lease can expire, allowing a later workflow to acquire a new fence. Writes
using an obsolete fence are rejected. Cancellation lets sync record its interrupted result and
attempt lease release; lease expiry is recovery for an abandoned owner, not permission to interrupt
an active one manually.

Long read transactions can delay WAL checkpointing, and SQLite can still report busy errors. Retry a
failed local read after contention subsides. Keep the active archive on a local filesystem; SQLite
WAL is not designed for concurrent access through a network filesystem. This change keeps one
database and the existing lease design; it does not introduce concurrent independent writers.

### Check health and migrate

```sh
forgesync archive doctor
forgesync archive migrate
```

Doctor checks archive integrity, schema history, foreign keys, and SQLite capabilities including
FTS5. Healthy storage does not imply fresh GitHub data or complete evidence coverage. It must be
able to open the archive before it can produce a diagnostic report.

Run migration explicitly when the installed version reports that an existing Forgesync archive
requires it. Earlier successful migrations can remain applied if a later step fails; inspect the
reported error rather than assuming the file is unchanged. Back up the archive before upgrading.

For an offline file backup, stop every process using the archive first. Preserve the database and
any adjacent `-wal` and `-shm` files together; do not copy only the database while writers are
active or discard a remaining WAL file. Restore into a separate location and inspect it with the
matching Forgesync version before replacing your working archive. There is no built-in backup/export
command.

### Troubleshooting

| Symptom                      | Next action                                                     |
| ---------------------------- | --------------------------------------------------------------- |
| Archive path is missing      | Check `archive.path`/override; initialize only a new archive    |
| Repository/thread not found  | Check the host/reference and acquire it with `sync`             |
| Search returns no hits       | Check acquired scope, filters, and title/body text              |
| Comments or reviews missing  | Sync with the corresponding `--with` families; inspect coverage |
| Reviews marked stale         | Reacquire review evidence against the current pull-request head |
| Partial/deferred acquisition | Inspect the run ledger, resolve the cause, and retry or sync    |
| No compatible vectors        | Check recipe/model/endpoint identity and run `embed`            |
| Model input rejected         | Reduce documented byte/batch limits for your service            |
| Writer lease held            | Let the owning action finish or cancel it                       |
| TUI command unavailable      | Install a build with the default `tui` feature                  |

## Use JSON and diagnostics

### Machine-readable results

```sh
forgesync --json archive status
forgesync --json search "terminal resize" > results.json
```

Application results use a JSON envelope with `schema_version` (currently `1`), `command`, and
`warnings`. Results include `data`; application failures include `error.code` and `error.message`
instead. A partial report can still be in `data`, so check its outcome and the process exit status.
Argument parsing and missing required global options can fail before the application envelope is
created; do not assume every nonzero invocation emits JSON.

| Exit status | Meaning                                                             |
| ----------- | ------------------------------------------------------------------- |
| `0`         | Command completed successfully; an empty query page is also success |
| `1`         | Operation, storage, provider, or diagnostic failure                 |
| `2`         | Argument/configuration or command usage failure                     |
| `3`         | Structured partial success or deferred work                         |
| `130`       | Cancellation/interruption                                           |

### Diagnostic output

Results go to stdout; diagnostics go to stderr. Increase verbosity or change log encoding:

```sh
forgesync -v archive status
forgesync -vv --log-format json sync ratatui/ratatui \
  > result.txt 2> diagnostics.jsonl
```

`-v` enables informational logs, `-vv` debug logs, and `-vvv` trace logs. `--log-format json`
changes diagnostics only; add `--json` for JSON command results. Credentials, authorization headers,
and raw discussion payloads are not logged by Forgesync's diagnostics.

## Command reference

Commands use the configured or default archive unless `--archive PATH` overrides it. Use
`forgesync COMMAND --help` or `forgesync COMMAND SUBCOMMAND --help` for exact flags in your
installed build.

| Command                       | Purpose                                        | External service use                  |
| ----------------------------- | ---------------------------------------------- | ------------------------------------- |
| `archive init`                | Create a new archive                           | None                                  |
| `archive status`              | Inspect counts, coverage, and durable work     | None                                  |
| `archive doctor`              | Check archive health and SQLite capabilities   | None                                  |
| `archive migrate`             | Apply explicit schema migrations               | None                                  |
| `sync REPO...` / `--all`      | Acquire selected discussions and families      | GitHub                                |
| `refresh REPO...`             | Sync and optionally analyze                    | Depends on selected stages            |
| `thread list` / `show`        | Inspect archived discussions                   | None                                  |
| `search QUERY`                | Find archived discussions                      | Embedding service for semantic/hybrid |
| `embed REPO...`               | Generate compatible discussion vectors         | Embedding service                     |
| `run list` / `show`           | Inspect acquisition history                    | None                                  |
| `run retry ID`                | Retry selected unresolved acquisition          | GitHub                                |
| `cluster build REPO`          | Generate related-discussion groups             | None; stored vectors required         |
| `cluster list` / `show`       | Inspect generated groups                       | None                                  |
| `cluster dismiss` / `restore` | Record or clear a local dismissal              | None                                  |
| `cluster exclude` / `include` | Change a current member's local exclusion      | None                                  |
| `cluster canonical`           | Choose a current representative member         | None                                  |
| `tui`                         | Browse and perform explicit maintainer actions | GitHub for acquisition actions        |

Global options are `--archive`, `--config`, `--json`, `--color auto|always|never`,
`--log-format text|json`, and repeatable `-v`/`--verbose`. Help and version do not require an
archive.

Forgesync's selected scope excludes GitHub write-back, Gitcrawl import, full revision history,
source-code indexing, generated summaries, cloud/portable distribution, and deep pull-request file,
commit, or check acquisition. The [compatibility ledger](compatibility.md) records those boundaries.
