# Configuration

Forgesync reads an optional TOML file. Select it with `--config PATH`; when that option is absent,
Forgesync reads the path in `FORGESYNC_CONFIG`. If neither is set, built-in defaults apply.

## Document recipe

P4.1 supports two recipes for deterministic retrieval documents:

```toml
[documents]
recipe = "discussion_enriched"
```

`discussion_enriched` is the default. It includes the source title, body, labels, current comments,
and selected pull-request review evidence. `original_body` includes only the source title, body, and
labels:

```toml
[documents]
recipe = "original_body"
```

The recipe and its version are included in document identity, so changing the recipe invalidates
derived document artifacts. Source retrieval timestamps are recorded separately and do not change
the content hash. Embedding service settings are documented with the embedding workflow.

Unknown fields and recipe values are errors. Configuration is loaded after command-line parsing;
configuration errors use the normal application error output, including the JSON envelope when
`--json` is selected.
