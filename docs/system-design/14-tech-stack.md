# 14 · Tech stack

> **What this document decides:** what Orbit is built with, phase by phase, and what is deliberately left out. Rule of thumb: **boring where it is infrastructure, custom where it is the product.** The product is the ledger, the revision engine and the recall engine; everything else should be as standard as possible.

## 1. Principles

1. **Few moving parts.** Each stateful system is a thing to secure, back up, isolate per tenant and delete from. Start with two: PostgreSQL and object storage.
2. **Replaceable intelligence.** No model, embedding or provider is named in the data model. All are reached through one gateway, by tier.
3. **One engine, two homes.** The core runs in the cloud and, compiled for desktop and Android, on the device — and alone, as a local brain.
4. **Nothing ships without its evaluation.** The evaluation harness is part of the stack, not an afterthought.

## 2. At a glance

| Layer | Choice | From phase | Alternatives considered | Why |
|---|---|---|---|---|
| Engine language | **Rust** | 1 | Go, TypeScript, Python | Predictable latency on the recall path; memory safety in a multi-tenant security boundary; first-class search and index libraries; one core for server, desktop and Android |
| Worker language | **Python** | 0 | Rust, TypeScript | Fastest iteration on model pipelines and evaluation; richest NLP tooling |
| System of record | **PostgreSQL 18** | 0 | Graph DB, document store, event-store DB, XTDB | Transactions, constraints, range types and temporal exclusion, row-level security, partitioning, logical replication, operational maturity |
| Blobs, images, exports | **S3-compatible object storage** | 0 | — | Cheap, durable, versioned |
| Dense search (early) | **pgvector** | 0 | Dedicated vector DB | One less system while single-tenant scale is small |
| Lexical search | **Tantivy** inside the recall engine (PostgreSQL full-text as fallback) | 1 | BM25 extensions in PostgreSQL; OpenSearch | Per-user indexes, code-aware tokenisers, no extra service |
| Literal lookup | PostgreSQL trigram + exact indexes → finite-state dictionary in the image | 0 → 2 | — | Exact and typo-tolerant |
| Graph | **Edge tables in PostgreSQL, projected to in-memory adjacency** | 2 | Neo4j, FalkorDB, Kuzu, Apache AGE | The graph is small per user and traversal wants memory locality; one source of truth |
| Retrieval index | In-process index in the engine: Tantivy + quantised vector file + in-memory adjacency, built from PostgreSQL. Per-user bundles on object storage only if a hosted service is ever built | 2 | Qdrant with tenant partitioning; shared search cluster | Many-small-tenants regime; isolation; cost |
| Queue and workflow | PostgreSQL job table with per-user advisory locks → partitioned log + durable workflow engine | 0 → 4 | Kafka from day one | Fewer systems until volume requires them |
| Working-memory cache | In-process, durable copy in PostgreSQL → Valkey/Redis | 1 → 4 | — | Small, hot, disposable |
| Model access | **Internal model gateway** with tiers | 0 | Direct SDK calls scattered in code | Independence, budgets, logging, routing by sensitivity |
| Embeddings | **Open-weight, self-hostable** model as the baseline space | 0 | Hosted-only | No provider is *required*; treat vectors as sensitive |
| Auth | Standards-compliant OAuth 2.1 / OIDC server (adopted, not written) + Orbit's own grant layer | 1 | Hand-rolled auth | Protocol correctness is not the product |
| Device storage | **SQLite** (encrypted) | 3 | — | Embedded, universal |
| Desktop client | **Tauri** + shared Rust core | 3 | Electron | Reuses the engine; small footprint |
| Android client | **Kotlin** + Rust core through generated bindings | 3 | — | Native UI, shared logic |
| Web client | **TypeScript / React** | 3 | — | — |
| Observability | **OpenTelemetry** → any backend | 0 | — | MCP now carries trace context; Logging is deprecated in MCP |
| Infrastructure | **One deployment per brain in the owner's cloud**: containers on a single server (or a small managed stack), infrastructure-as-code, cloud KMS; cloud-agnostic | 0 | Shared multi-user fleet | Nobody else's data to hold; packaged later for others to deploy |

## 3. Languages, pragmatically

The engine is Rust on the merits. The brief's own example sentence prefers Rust for the core engine for the same reasons; that is treated here as an inclination to confirm, not as a recorded decision.

But the riskiest part of this project is **algorithmic quality**, and that is learned fastest in Python against a real evaluation harness. So:

| Phase | Rust | Python |
|---|---|---|
| 0 — prove the algorithms | Ledger and ingest only, or nothing yet | Extraction, verification, revision and retrieval prototypes; the whole evaluation harness |
| 1 onward | Ledger, API, MCP, revision engine, recall engine, indexer, sync | Derivation workers (model-heavy), consolidation, evaluation |

Revision and every mechanical check (literal detection, handle validation, anchoring) live in Rust, because they must be deterministic, identical on server and device, and run on every write. Python workers only call models and return proposals; the engine verifies and applies them.

## 4. Data

**PostgreSQL 18** is the current stable release (19 is in its release-candidate period at the time of writing). Features this design relies on: `uuidv7()`; range and multirange types; exclusion constraints on partitioned tables; row-level security; hash partitioning by tenant; logical replication for the change feed. PostgreSQL 19's SQL/PGQ graph syntax is a convenience for fixed-length patterns and is not required.

Extensions: `btree_gist` (temporal constraints), `pg_trgm` (literals), `vector` (early dense search). A BM25 extension is optional.

**Object storage** holds large evidence bodies, brain-image segments, exports and backups, encrypted under tenant keys.

**SQLite** is the device replica and the local-brain store.

## 5. Inside the recall engine

| Need | Component |
|---|---|
| Lexical / BM25, phrase and field search | Tantivy, with a tokeniser that keeps `snake_case`, `camelCase`, paths and flags whole *and* split |
| Alias and literal dictionaries | Finite-state transducers (prefix, fuzzy, exact) |
| Dense search | Binary-quantised vectors scanned with SIMD, rescored at higher precision; an HNSW graph only when a brain is large enough to need it |
| Graph | Compressed sparse adjacency with typed edges; the push algorithm from [08](../architecture/08-retrieval.md#52-algorithm) |
| Temporal and slot lookups | Sorted columnar belief snapshot with interval search |
| Fusion, grounding, weaving | Plain Rust |

A flat scan is not a compromise at this scale: a heavy user's brain is on the order of 10⁵–10⁶ vectors, and exhaustive binary search over that many is a few milliseconds and exact.

## 6. Models

Reached only through the gateway, by tier. Names below are current defaults in configuration, not architecture.

| Tier | Used for | Example default | Notes |
|---|---|---|---|
| `decide` | Fixed-choice judgements: speech act, salience tier, sensitivity check, instruction-likeness, relation (same / refines / contradicts / unrelated), ratification strength, state vs trait, query intent | A non-generative decision model — e.g. Gemini Decision 1 (`gemini-decision-1`) or Jev — or a small distilled classifier | Returns a probability per option; several questions about one input in a single call |
| `bulk` | Extraction, verification, summaries, fast planner | A small, fast model — e.g. Claude Haiku 5.5 (`claude-haiku-5-5`) — or an open-weight model served locally | Output is mechanically verified, which is what makes a small model acceptable |
| `careful` | Consolidation (abstraction, association), disputed judgements, deep-path planning | A frontier model — e.g. Claude Opus 5.5 (`claude-opus-5-5`) | Runs offline in batches where possible |
| `local` | Anything labelled `sensitive`; local-brain mode | Open-weight model on the user's or Orbit's own hardware | Quality floor |
| Embedding | All dense vectors | An open-weight multilingual model (candidates: the Qwen3-Embedding family, BGE-M3; EmbeddingGemma on device) | Chosen by retrieval tests on OrbitBench, not by leaderboard rank |
| Reranker | Deep-path relevance on evidence | Open-weight cross-encoder | Self-hosted |
| Classifiers | Admission-time checks that must run before persistence (secret and paste detection, first sensitivity label) | Rules and small models on CPU | No per-call cost; no data leaves the server |

**The `decide` tier.** Many pipeline steps are not generation at all but *pick one of these options*. Decision models answer exactly that: given an input and a set of typed questions (yes/no, choice, ordered score), they return a probability for every option and generate no text. Gemini Decision 1, for example, scores up to 1,000 questions over one input in a single pass and accepts text, images, audio, video and PDFs. For Orbit that means one call per turn answers all of its classification questions together, and screenshots or voice notes can be triaged even though they cannot yet be anchored.

Rules for using it:

- Every choice question includes an explicit `other` / `none` option, and answers below a confidence threshold fall back to the `bulk` tier. Decision models spread probability across the given options even when none fits.
- Only the content being judged is sent, never surrounding history; these APIs are billed on input and may not cache it.
- Probabilities feed Orbit's own calibrated confidence; they are inputs, not the final number.
- `sensitive` content goes to a local classifier instead. Before any personal data is sent to a hosted decision model, its data-use terms are checked: no free tiers whose content may be used for training, and retention must be acceptable.
- Classifier output never grants trust. Poisoning defence stays structural ([11 §3](../architecture/11-security-privacy.md#3-the-memory-firewall)); input can argue for its own classification.
- Which model fills the tier is decided by accuracy on OrbitBench's labelled judgements against the `bulk` tier, at equal or lower cost.

Every call records provider, model id, prompt hash and the ids of what it processed. Swapping a tier's model starts a new derivation epoch that is evaluated before it replaces the old one.

## 7. Interfaces

- **MCP:** an official SDK where it supports the 2026-07-28 revision (the Rust SDK was in beta for it at the time of writing), otherwise a thin hand-written handler. The surface Orbit uses is small: discovery, tools, resources, prompts, subscriptions, multi-round-trip results.
- **REST:** OpenAPI-described; SDKs generated for TypeScript and Python, hand-finished for Rust and Kotlin.
- **Auth:** an existing OAuth 2.1 / OIDC authorization server. Selection criteria: PKCE; resource indicators (RFC 8707); Client ID Metadata Documents; issuer identification (RFC 9207); passkeys; a customisable consent screen for grants. Support for the newer items varies and must be verified per candidate.

## 8. Build or adopt

| Build (it is the product) | Adopt |
|---|---|
| Evidence ledger and admission | PostgreSQL, object storage, KMS |
| Anchor-verified extraction; handle-locked rendering | Model providers; open-weight models |
| Revision engine (Scope-Before-Supersede, slot replay, justification) | OAuth / OIDC server |
| Recall engine (channels, spreading, grounding, weaving) | Tantivy, vector and FST libraries |
| Brain image and indexer | Lance, if the spike favours it |
| Grants, policy mask, disclosure ledger | OpenTelemetry stack |
| OrbitBench and the invariant suite | Public benchmarks, as secondary evidence |
| Capture connectors | Client hook and extension mechanisms |

## 9. Deliberately not in the stack

| Not used | Reason |
|---|---|
| A graph database | Second source of truth; per-tenant isolation; the graph fits in memory |
| A shared vector database | Wrong shape for many small tenants; isolation risk. (Qdrant is the fallback if brain images are postponed) |
| An agent or memory framework in the core | They decide the memory model; Orbit needs its own. Integrations with them are welcome at the edges |
| A streaming platform on day one | A PostgreSQL outbox is enough until it isn't |
| Fine-tuning on user data | Unattributable, hard to delete, model-dependent |
| MCP Sampling, Roots, Logging | Deprecated in the current specification |

## 10. Repository layout

```text
orbit/
  crates/
    orbit-core/      shared types: ids, labels, scope, anchors, hash chain — pure, no I/O
    orbit-formation/ literal detection, handle templates, claim verification — pure
    orbit-revision/  scope-before-supersede, slot replay, assessment — pure
    orbit-ledger/    admission, append, hash chain storage; owns the migrations
    orbit-recall/    channels, fusion, spreading, grounding, weaving
    orbit-index/     indexer and image format
    orbit-server/    REST + MCP handlers
    orbit-sync/      change feed, device protocol
    orbit-cli/       import, export, agents-md, admin
  migrations/        PostgreSQL schema, applied at start-up
  workers/           Python: model calls only (extraction proposals, summaries); mechanical checks live in Rust
  evals/             invariants, OrbitBench generator, benchmark harnesses
  connectors/        agent hooks, transcript importers, browser extension
  clients/           desktop, android, web
  sdk/               typescript, python
  deploy/
  docs/
```

## 11. Open choices, settled by spike

| Question | Decide by |
|---|---|
| Brain-image format: Lance or custom | Cold start, filtered search, compaction, deletion |
| Embedding model and dimension | Retrieval suites at fixed cost |
| Which small model clears the extraction bar | Extraction precision/recall by model |
| Which model fills the `decide` tier, and whether it beats the `bulk` tier | Accuracy and calibration on labelled judgements; cost per turn; data-use terms |
| Authorization server | The criteria in §7 |
| Workflow engine for consolidation | Operational fit once volume justifies one |
