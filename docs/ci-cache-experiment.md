# CI cache experiment

## Decision and scope

Keep routine CI and publication policy unchanged until the proposed gate is reviewed. A pinned
Swatinem/rust-cache dependency cache makes Linux compilation, Clippy, documentation, and binary
smoke individually inexpensive on warm runners. Full workspace tests exceed the 60-second warm
budget. Native platform and release-asset builds remain manual.

This investigation addresses [issue 1](https://github.com/joshka/forgesync/issues/1), using only the
`ci-cache` jj workspace and personal bookmark `joshka/ci-cache-experiment`. The manual Platform
checks workflow gains an opt-in experiment and lane selector; its default native matrix is intact.
There are no changes to routine CI, publication workflows, Rust code, or dependencies.

## Measurement method

Measurements were made on 2026-09-30 using GitHub-hosted `ubuntu-24.04`, Rust 1.98.1, locked
dependencies, and `CARGO_INCREMENTAL=0`. Debug builds use their existing profile. Each lane has its
own cache key; the first run populated fresh keys and the second restored exact matches. Workspace
artifacts are deliberately excluded from the cache, so warm runs still compile and link product
code. No binaries are reused as proof that changed source works.

The initial source revision is `4f397f17a134aae1a9731558f83e94f5f2a9fa30`. Both initial runs use
that revision. Durations below come from the Actions jobs API, at one-second resolution. A `0` smoke
step means less than one clock tick, not zero work. Compilation steps include Cargo dependency
resolution/download, build scripts, compilation, and linking where applicable; they do not isolate
linker CPU time. Toolchain selection includes rustup installation when needed. Job totals include
setup, checkout, cleanup, and cache upload. Gaps between steps explain non-additive totals. Workflow
elapsed means run creation to final update, including dispatch/queue overhead.

- [Uncached routine compilation](https://github.com/joshka/forgesync/actions/runs/36765389395):
  65 seconds workflow, 61 seconds job, 56 seconds Cargo.
- [Cold experiment](https://github.com/joshka/forgesync/actions/runs/36765720395):
  143 seconds complete workflow; all six lanes passed.
- [Warm experiment](https://github.com/joshka/forgesync/actions/runs/36766050619):
  74 seconds complete workflow; all six lanes passed. The full test lane dominates this matrix.

## Initial measurements

All values are seconds. Setup is toolchain selection only; cache post is cleanup/save.

| Lane            | Cold setup | Cold restore | Cold Cargo | Cold execute | Cold cache post | Cold job | Warm setup | Warm restore | Warm Cargo | Warm execute | Warm job |
| --------------- | ---------: | -----------: | ---------: | -----------: | --------------: | -------: | ---------: | -----------: | ---------: | -----------: | -------: |
| Check           |          9 |            1 |         72 |            0 |               7 |       95 |          8 |            7 |          8 |            0 |       30 |
| Default smoke   |          9 |            1 |         86 |            0 |               9 |      110 |          9 |            5 |         11 |            0 |       30 |
| Workspace tests |          8 |            2 |        103 |           13 |               7 |      137 |          9 |            8 |         31 |           13 |       68 |
| Clippy          |         10 |            3 |         76 |            0 |               7 |      102 |          8 |            4 |          8 |            0 |       27 |
| Docs            |          8 |            1 |         65 |            0 |               6 |       84 |          8 |            4 |          8 |            0 |       23 |
| CLI-only smoke  |          7 |            0 |         58 |            0 |               8 |       79 |          8 |            7 |         10 |            0 |       31 |

Warm cache post steps all took less than one second and did not upload replacement caches. Restored
compressed caches were approximately 269 MB for check/Clippy, 329 MB for default smoke, 357 MB for
tests, 247 MB for docs, and 294 MB for CLI-only smoke. Keeping every experiment lane permanently
would duplicate dependency storage; this matrix is for comparison rather than a proposed PR matrix.
The colder cached check is slower than the uncached baseline in this sample: cache saving and
explicit setup have costs, and runner variation prevents attributing the whole difference to cache.

## Combined and focused measurements

The [expanded run](https://github.com/joshka/forgesync/actions/runs/36766378829) adds fresh keys for
two practical alternatives. Other lanes in that run were warm, so its complete elapsed time is not a
cold single-lane workflow measurement. Source revision: `b9b1666687eb` (full revision retained in
the evidence JSON).

| Lane                   | Cold setup | Cold restore | Cold Cargo | Cold execute | Cold save | Cold job | Warm setup | Warm restore | Warm Cargo | Warm execute | Warm job | Warm workflow |
| ---------------------- | ---------: | -----------: | ---------: | -----------: | --------: | -------: | ---------: | -----------: | ---------: | -----------: | -------: | ------------: |
| Clippy + default smoke |          9 |            1 |        116 |            0 |         8 |      140 |          9 |            9 |         17 |            0 |       41 |            47 |
| CLI + offline tests    |          8 |            1 |         61 |            3 |         7 |       85 |          7 |            5 |         11 |            6 |       32 |            37 |

The [47-second combined workflow](https://github.com/joshka/forgesync/actions/runs/36766750569) and
[37-second focused workflow](https://github.com/joshka/forgesync/actions/runs/36766754118) each
dispatch exactly one lane and pass with exact cache hits. Combined and focused caches restore
approximately 467 MB and 349 MB, respectively. Both use source revision `c017c51137ab`, which adds
documentation and the selector without changing Rust source or cache inputs. The focused lane passes
40 cases in the product's `cli_contract` and `offline_queries` suites. The checked-in
[timing evidence](ci-cache-timings.json) preserves run IDs, full source revisions, workflow elapsed
time, job totals, and step durations, independent of hosted log retention.

## Recommendation

Propose the combined Linux Clippy plus default-binary smoke lane as the next routine PR gate: its
complete warm workflow is 47 seconds, with a 140-second cold job. Clippy covers the existing
all-target/all-feature compilation scope and adds lints; building and executing the default binary
adds actual linking, startup, config, archive, and SQLite checks. Avoid running a duplicate
`cargo check` in the same job. Cache warming would need to occur on a branch accessible to PRs.
Repeat measurements on representative source and lockfile edits before calling 47 seconds an SLA.

The focused CLI/offline lane is a stronger behavior-focused alternative at 37 seconds complete, with
an 85-second cold job. It does not replace workspace compilation coverage. If combining it with
Clippy, measure that combined workflow first; separate successful lane times do not prove that a
combined or queued multi-job workflow meets the budget. Full workspace tests should remain an opt-in
broader check until their 74-second warm workflow is reduced or the budget changes.

Public docs and CLI-only smoke each fit individually. Add them for changes affecting documentation
or feature compatibility after measuring the proposed complete PR matrix. The docs measurement has
no strict flags and cannot justify a strict private-Rustdoc gate. Formatting is unmeasured: the
project uses nightly rustfmt, so include pinned-nightly installation and execution in a future
measurement rather than assuming it is free. Native builds and packaging stay manual. Do not add any
second publication gate based on this experiment.

## Coverage and remaining gaps

The check lane compiles all workspace targets and features without execution. Clippy adds warning
and lint checks over the same target scope. Docs builds public documentation, without strict Rustdoc
flags or private documentation. Tests compile then execute the existing workspace suite: 539 tests
passed, with no failures or ignored cases. Execution includes doctests and their cost. CLI-only
smoke establishes that the product builds without the TUI feature.

Both binary smoke lanes exercise startup/version, argument handling, automatic isolated config,
relative archive path resolution, SQLite initialization, JSON startup diagnostics, integrity,
foreign keys, FTS5/schema probes, and an expected empty keyword result. They remove provider
credential environment variables and require no network access. They do not establish positive
search membership, provider acquisition, real terminal interaction, or Tokio signal/subprocess
integration. The existing empty-scope sync test returns before provider work and is insufficient
proof of those drivers. The subsequent
[runtime-driver fix](https://github.com/joshka/forgesync/pull/2) adds signal registration,
subprocess, and fake-credential CLI regressions. Those changes were absent from the measured
revisions; these timings do not certify their additional execution cost.

## Cache contract and miss costs

The experiment pins [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache) to commit
`6323deb102c322ba6fcbdcafc7e3dddab59af2b6` (v2 at measurement time). The prefix is
`ci-experiment-v1`; the lane is an additional key and the action retains its job partition. Compiler
identity, Rust environment, manifests, lockfile, toolchain file, and Cargo configuration contribute
to the generated key. Changing dependencies or the toolchain changes the exact key; a compatible
fallback may restore some dependencies, so an exact miss does not always mean cold. Changing the
prefix forces a fresh namespace. Feature/profile differences use separate lane keys.

The action caches Cargo registry/git data and dependency target artifacts, removes workspace and
incremental artifacts, and skips Cargo binary caching here. Exact hits are immutable: changed
workspace source is rebuilt without extending that cache. Branch-scoped cache availability and
GitHub eviction can still force misses; caches on this personal bookmark do not warm main. A
production experiment must populate its own main-accessible namespace before expecting PR hits. Keep
save policy explicit when sharing caches across jobs or feature combinations.

Expect cold jobs of roughly 79–137 seconds for these measured commands, not a hard upper bound. Keep
locked dependencies and correctness checks on misses rather than silently skipping work to meet the
warm target. Runner demand, network, dependency changes, and larger source changes can exceed these
samples. Two runs do not establish a latency percentile or reliability guarantee.

## Reproduce

Dispatch the existing workflow from the personal experimental bookmark:

```sh
gh workflow run platform-checks.yml --repo joshka/forgesync \
  --ref joshka/ci-cache-experiment -f experiment=true -f gate=all
```

Use `gate=fast` for combined Clippy and default binary smoke, or `gate=focused` for the CLI contract
and offline-query integration suites. Other choices are `check`, `smoke`, `tests`, `clippy`, `docs`,
and `minimal`. Run twice without changing the key to observe cold then warm behavior; increment the
prefix only for a deliberate fresh-cache experiment. Inspect cache-hit messages rather than assuming
a second dispatch is warm. Obtain complete and step timing evidence with:

```sh
gh api repos/joshka/forgesync/actions/runs/RUN_ID
gh api repos/joshka/forgesync/actions/runs/RUN_ID/jobs
gh run view RUN_ID --repo joshka/forgesync --log
```
