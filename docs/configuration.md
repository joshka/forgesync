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
the content hash. Embedding service configuration is optional and does not affect keyword-only
workflows.

## Embedding service

The optional `[embeddings]` section configures one OpenAI-compatible service. The API key is read
from the named environment variable and is never saved in TOML:

```toml
[embeddings]
endpoint = "https://api.openai.com/v1"
model = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"
max_input_bytes = 7000
max_batch_input_bytes = 250000
batch_size = 64
concurrency = 4
```

The default endpoint is the OpenAI `/v1` base; Forgesync appends `/embeddings`. A local HTTP URL is
allowed for loopback fixtures. Other endpoints must use HTTPS. Requests use `model`, `input`,
optional `dimensions`, and `encoding_format = "float"`, and map response vectors back by their
returned input index. See the
[Create embeddings API reference](https://developers.openai.com/api/reference/resources/embeddings/methods/create).

`max_input_bytes` limits each deterministic UTF-8 chunk. `max_batch_input_bytes` limits the combined
bytes in one request. These are byte budgets, not token estimates; lower them for services with a
smaller model input limit. Forgesync does not infer a token limit from error text. `batch_size` and
`concurrency` bound request count and parallel requests. `request_timeout_seconds`,
`retry_budget_seconds`, and `max_attempts` configure bounded transient retries. `dimensions` can set
the expected output length; when omitted, vectors in each response must still agree in length.

The `embed` command accepts `--endpoint`, `--model`, `--api-key-env`, `--dimensions`,
`--max-input-bytes`, `--max-batch-input-bytes`, `--batch-size`, and `--concurrency` overrides.
Completed vector batches are stored independently; a later run reuses matching chunks and requests
only missing ones. Provider keys are never included in archive identity or output.

## Refresh

`refresh OWNER/REPO` runs the selected GitHub sync without requiring an embedding service. Analysis
stages are opt-in with `--analyze embeddings,clusters`; `embeddings` materializes current discussion
documents and requests missing vectors, while `clusters` uses compatible vectors already in the
archive. Clustering does not read the API key or contact the embedding provider. Use `--no-sync` to
run selected analysis against only the local archive.

Refresh output reports each selected stage independently. A later stage failure keeps earlier
results and lists the stages that may need another run. The embedding key is read only when
`embeddings` is selected.

## Related-discussion clusters

`cluster build OWNER/REPO` uses the configured embedding endpoint and model identity together with
the configured document recipe. It reads current open discussions and their compatible stored
vectors. The command does not read the API key or contact the embedding provider; use `embed` to
create missing vectors first. Command-line `--endpoint` and `--model` select which stored vector
identity to use.

Clustering defaults to a cosine threshold of `0.80`, a cross-kind issue/pull-request threshold of
`0.93`, fanout `16`, maximum component size `40`, and minimum component size `1`. Override these
with `--threshold`, `--cross-kind-threshold`, `--fanout`, `--max-cluster-size`, and
`--min-cluster-size`. The graph is deterministic for the same archive content, vectors, and options.

The command compares the number of current open discussions with the number that have complete
compatible vectors. A run with partial vector coverage records groups from available vectors and
preserves unseen groups and memberships. A complete run can retire groups that no longer qualify. If
eligible discussions exist but none has a compatible vector, the command reports that vectors are
unavailable and writes no generation. JSON and human output report eligible and vector counts so the
coverage policy is visible.

`cluster list`, `cluster show`, `cluster dismiss`, `cluster restore`, `cluster exclude`,
`cluster include`, and `cluster canonical` inspect or change archive-local triage decisions. These
commands do not change GitHub state. Dismissal, exclusion, and canonical choices persist across
matched regenerated clusters.

## Retrieval modes

`search QUERY` defaults to local keyword search and does not use an embedding service. Use
`--mode semantic` for exact cosine ranking or `--mode hybrid` to combine keyword and semantic ranks
with reciprocal rank fusion (constant 60). Semantic and hybrid search use the configured document
recipe, endpoint, and model, and only consider complete vectors whose document matches the current
archived thread and evidence coverage. A document's score is the best cosine similarity among its
deterministic chunks.

Semantic and hybrid modes return an explicit error when compatible vectors or the embedding service
are unavailable. Add `--keyword-fallback` to opt into keyword-only results for those cases; JSON
reports both requested and effective modes and the fallback reason. `--sort updated` and
`--sort created` apply those stable source-time orders after scoring or fusion. Semantic/hybrid
pagination retains at most 10,000 results before applying the requested offset and limit.

Unknown fields and recipe values are errors. Configuration is loaded after command-line parsing;
configuration errors use the normal application error output, including the JSON envelope when
`--json` is selected.
