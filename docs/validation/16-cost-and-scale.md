# 16 · Cost and scalability

> **What this document decides:** a cost model with its assumptions exposed, so that it can be corrected with measurements. Every figure here is an **estimate from stated assumptions**, not a measurement. The structure of the model matters more than the totals: it shows which levers exist and which assumption, if wrong, changes the answer.

## 1. Inputs

**Usage profiles** (tokens of conversation and documents ingested per month, `T`):

| Profile | Per day | `T` per month | Deep recalls per day |
|---|---|---|---|
| Light | ~3k | 0.1M | 0.5 |
| Typical | ~15k | 0.5M | 3 |
| Heavy | ~100k | 3M | 20 |

**Example prices.** The design is provider-neutral; to make the arithmetic concrete it uses Anthropic's published list prices as of 2026-11-03 for the two tiers in [14 §6](../system-design/14-tech-stack.md#6-models). Substitute current prices for any provider.

| Tier | Example model | Input / 1M tokens | Output / 1M tokens |
|---|---|---|---|
| `bulk` | Claude Haiku 5.5 (prompts up to 100k tokens) | $0.10 | $0.50 |
| `careful`, batched (50% of list) | Claude Opus 5.5 at $4 / $20 list | $2.00 | $10.00 |

## 2. Model-token flow

How many model tokens each stage consumes per ingested token. These multipliers are the assumptions most worth measuring in Phase 0.

| Stage | Tier | Assumption | Input × `T` | Output × `T` |
|---|---|---|---|---|
| Light extraction (summary, mentions) | bulk | 30% of tokens reach tier 1; 1.2× prompt overhead | 0.36 | 0.02 |
| Full extraction (claims) | bulk | 20% of tokens reach tier 2; 1.6× overhead | 0.32 | 0.07 |
| Verification | bulk | ~4 claims per 1k tier-2 tokens; ~350 tokens each | 0.28 | 0.008 |
| Relation judgements | bulk | 30% of claims touch an existing slot; 2 comparisons | 0.19 | 0.014 |
| **Bulk subtotal** | | | **1.15** | **0.11** |
| Consolidation (views, association, induction) | careful, batched | Offline, prioritised | 0.15 | 0.02 |
| Deep recall | bulk | ~3k in, ~0.3k out per deep recall | per query | per query |

Half of all ingested tokens receive no model processing at all; they are stored and indexed raw, and remain findable.

## 3. Model cost per user-month

| | Light | Typical | Heavy |
|---|---|---|---|
| Bulk input (incl. deep recall) | 0.16M → $0.02 | 0.85M → $0.08 | 5.25M → $0.53 |
| Bulk output | 0.016M → $0.01 | 0.08M → $0.04 | 0.51M → $0.26 |
| Careful input | 0.015M → $0.03 | 0.075M → $0.15 | 0.45M → $0.90 |
| Careful output | 0.002M → $0.02 | 0.01M → $0.10 | 0.06M → $0.60 |
| **Models** | **≈ $0.07** | **≈ $0.38** | **≈ $2.3** |

### Sensitivity

| If… | Heavy user becomes | Lesson |
|---|---|---|
| The bulk tier cost 10× more (the price of a previous-generation small model) | ≈ $9 | Cheap, verifiable extraction is the economic foundation |
| Consolidation ran on the bulk tier | ≈ $0.9 | The careful tier is the largest single line for heavy users and is the first place to tune |
| Everything were fully extracted (no salience gating) | Bulk cost roughly triples | Lazy extraction earns its complexity |
| Deep recall were the default | + $0.5–1.5 | Keep the fast path model-free |

## 4. Embeddings

About 1.2 × `T` tokens are embedded (chunks plus derived statements): 3.6M tokens a month for a heavy user. Hosted embedding endpoints are priced on the order of cents per million tokens, and a self-hosted open-weight model costs mainly the GPU kept warm. Budget **$0.05–0.40 per heavy user-month**, far less for others. Check current prices.

## 5. Storage

Per **heavy user, per year** (≈ 36M tokens):

| Data | Size | Where |
|---|---|---|
| Evidence text | ~150 MB raw, ~40–50 MB compressed | PostgreSQL / object storage |
| Claims (~30k), belief versions (~10k), mentions, edges (~300k), logs | ~150–300 MB with indexes | PostgreSQL |
| Vectors (~165k): search copy, binary-quantised | ~20–40 MB | Image, in memory when hot |
| Vectors: rescoring copy | ~170–330 MB | Image, on disk |
| Lexical index | ~50–75 MB | Image |
| Graph | ~7–10 MB | Image, in memory when hot |
| **Image total** | **~300–500 MB**, of which ~60–120 MB hot | Object storage + cache |

A typical user is about a sixth of this; a light user about a thirtieth.

At generic public-cloud rates (object storage around $0.02 per GB-month; managed PostgreSQL storage around $0.10–0.25 per GB-month — verify), a heavy user after three years costs roughly **$0.10–0.25 a month to store**. Storage is not the constraint.

## 6. Serving

- **Recall nodes.** The hot part of a heavy brain is a few hundred megabytes once cold episodes are represented by gist. A node with ~48 GB available for images holds roughly 100–150 hot heavy brains or 1,000–2,000 typical ones. If about 5% of users are active in any quarter-hour, one node serves a few thousand heavy users or tens of thousands of typical ones: **≈ $0.01–0.15 per user-month**.
- **Fast-path CPU.** Tens of milliseconds per recall; negligible.
- **Actors and workers.** I/O-bound on model calls; negligible beside the calls themselves.

## 7. Per-user summary

| Profile | Models | Embeddings | Storage | Serving + overhead | **Total per month** |
|---|---|---|---|---|---|
| Light | $0.07 | < $0.02 | < $0.02 | $0.05–0.10 | **≈ $0.15–0.25** |
| Typical | $0.38 | $0.02–0.07 | $0.02–0.05 | $0.05–0.20 | **≈ $0.50–0.75** |
| Heavy | $2.3 | $0.05–0.40 | $0.10–0.25 | $0.20–0.60 | **≈ $3–4** |

Model calls are 60–75% of the cost at every level. Everything else is small.

**For a personal deployment** these per-user figures are the marginal costs, and a fixed cost is added: a server that is always on, a PostgreSQL instance and backups. The fixed part does not depend on usage and is likely to be the larger share for anyone but a heavy user. It should be measured on the chosen cloud in Phase 0; a brain this size does not need a large machine. Model usage is billed to the owner's own provider keys.

## 8. At a million users — hosted service only, not the current plan

Kept for reference. With one brain per deployment, none of this infrastructure is built.

Assume 60% light, 35% typical, 5% heavy.

| Quantity | Estimate |
|---|---|
| Tokens ingested | ~385 billion a month, ~150k tokens per second on average |
| Model spend | ~$290k a month ≈ $0.29 per user |
| PostgreSQL growth | ~27 TB a year across the fleet |
| Cells (10–20k users each) | 50–100, each growing ~0.3–0.5 TB a year |
| Recall nodes | Tens |

Heavy users are 5% of the population and roughly 40% of the model bill, which argues for usage-aware pricing.

## 9. What scales, and why

| Property | Reason |
|---|---|
| Users do not interact | No cross-tenant queries, joins or global indexes; growth is adding cells |
| Each brain is small | A decade of heavy use is single-digit gigabytes; one node holds it comfortably |
| Writes are appends | Ingest is a sequential write plus an outbox row |
| Understanding is asynchronous and prioritised | Load spikes turn into derivation lag, not into lost data or slow writes |
| Retrieval is local to one image | No network hops between indexes |
| Idle brains cost storage only | Images are evicted and reloaded on demand |

## 10. What strains first

| Pressure | Symptom | Response |
|---|---|---|
| Bulk imports — years of history at once | Derivation backlog; model-spend spike | Per-user budget; import at tier 0/1 first, promote lazily; batch pricing |
| Many simultaneously active heavy users | Recall-node memory | Tiering to gist; more nodes; pin by cell |
| PostgreSQL write volume in a cell | Replication lag | Partitioning; move cold ledger segments to columnar files in object storage |
| Consolidation backlog | Stale views | Gain × need priority; cap careful-tier spend per user |
| Very large individual brains | Slow cold start | Dedicated cell; segment-level lazy loading |

## 11. Levers, in order of effect

1. Salience thresholds and lazy extraction.
2. Which tier does consolidation, and how often.
3. Batch processing and prompt caching for everything offline.
4. Skipping assistant boilerplate and long code blocks in extraction (they are still stored and searchable).
5. Vector quantisation and dimension.
6. Tiering policy for cold episodes.

## 12. What to measure in Phase 0 to replace these estimates

Tokens in and out per stage per 1M ingested · share of tokens at each salience tier on real history · claims per 1k tokens · rejection rates · bytes per user-month in each store · recall latency by path.
