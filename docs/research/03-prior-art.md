# 03 · Prior art: what exists, what it gets right, where it stops

> **What this document decides:** for each relevant system — adopt, extend, combine, or build something else. Short answer: **build the core, borrow the ideas.** No existing framework satisfies the invariants in [01](01-problem-definition.md), but almost every good idea Orbit needs has been demonstrated somewhere.

Sources are listed in [references](../references.md). Benchmark figures quoted from vendors are **self-reported** unless stated otherwise; §3 explains why that matters.

## 1. Memory systems

### 1.1 Comparison matrix

| System | Memory unit | Write path | Update semantics | Retrieval | Time | Provenance | Exact strings |
|---|---|---|---|---|---|---|---|
| **Mem0** (2025 paper) | Extracted fact; optional entity graph (Mem0ᵍ) | LLM extracts facts from the latest exchange, then an LLM picks ADD / UPDATE / DELETE / NOOP against similar memories | Destructive: update or delete | Vector similarity (+ graph) | Timestamps | Weak | Only if the extractor copies them |
| **Mem0** (2026 rewrite) | Fact + entity links | Single LLM call, **add-only** | Append; resolve at read time | "Multi-signal" fusion | Improved | Partial | As above |
| **Letta / MemGPT** | Editable memory *blocks* in context; recall + archival stores; now git-backed files ("MemFS") | The agent edits its own memory with tools; sleep-time agents rewrite it offline | Whatever the agent writes | Agent-driven search over history / files | By content | Git history | Yes when stored raw |
| **Zep / Graphiti** | Episodes → entities and *fact edges* → communities | 6–10 LLM calls per episode: extract, resolve, dedupe, date, invalidate | **Invalidate, don't delete**; bi-temporal edges | Cosine + BM25 + graph BFS, then RRF / MMR / cross-encoder; no LLM at read time | **Bi-temporal** (event time, ingestion time) | Edge → episode | Episodes are kept verbatim |
| **Cognee** | Chunks + knowledge-graph triplets | add → cognify (LLM structure extraction) → memify (enrichment) | Graph enrichment | Many modes over vector + graph | Limited | Chunk-level | In chunks |
| **Microsoft GraphRAG** | Entities, relationships, community summaries | Whole-corpus LLM extraction + Leiden clustering | Re-index | Local (entity) / global (map-reduce over summaries) | None | Chunk-level | In chunks |
| **HippoRAG 2** | Passages + phrase nodes + triples | OpenIE triples by LLM; passages added as graph nodes | Append | Personalised PageRank seeded by triples and passages; LLM filter on triples | None | Passage | In passages |
| **A-MEM** | Zettelkasten-style notes with generated tags and links | LLM writes the note, links it, and *rewrites older notes* | Evolution by rewrite | Embedding + links | Weak | Weak | No guarantee |
| **Hindsight** | Four networks: world facts, experiences, entity summaries, **beliefs** | retain / recall / reflect | Reflect updates beliefs traceably | Temporal, entity-aware recall | Yes | Yes | Not a stated goal |
| **Agent Zero Memory** | Three stores: event timeline, entity–event graph, curated documentary memory | Provenanced items (origin, time, evidence pointer) | Timeline records change | Intent gate → router → three concurrent agentic searches → **citation-locked** answer | First-class | **First-class** | Via citation lock |
| **Mastra Observational Memory** | Dense, dated observation log kept *in context* | Observer agent compresses the stream; Reflector condenses past a threshold | Reflection drops what is stale | **None** — the log is the context | Dates in the log | None | Only if observed verbatim |
| **SYNAPSE** | Dynamic episodic–semantic graph | Graph construction | — | Spreading activation with lateral inhibition and temporal decay, fused with embeddings | Decay | — | — |
| **LangGraph / LangMem** | JSON documents in namespaces | Hot-path tools or background manager | Overwrite / patch | Key + optional vector | None | None | As stored |
| **LlamaIndex memory** | FIFO chat buffer + pluggable long-term blocks | Flush on overflow; fact extraction | Overwrite | Block-specific | None | None | As stored |
| **Native assistant memory** | Saved facts + chat-history reference | Vendor pipeline | Opaque | Opaque | Opaque | None exposed | No |
| **MCP memory servers** (reference KG server, OpenMemory, Supermemory, Basic Memory…) | Mostly facts, triples or Markdown notes | Model calls a write tool | Mostly overwrite / append | Vector or keyword | Rare | Rare | Varies |

### 1.2 What each one teaches

**Mem0.** The 2025 design is the canonical *lossy write-time extraction* system, and its own results show the cost: on LoCoMo its best variant scored about 68% against a plain full-context baseline at about 73% in the same table. The 2026 rewrite quietly concedes the point — extraction is now add-only, with resolution deferred to read time — and reports 92.5 on LoCoMo and 94.4 on LongMemEval. *Lesson:* destructive updates at write time are the wrong default. *Verdict:* borrow the add-only stance; do not build on it — no epistemic status, no trust model, no byte-level provenance.

**Letta / MemGPT.** The original insight — let the agent page memory in and out of a limited context — still stands, and the 2025 result that a Letta agent with nothing but file tools reached 74.0% on LoCoMo (above Mem0's then-reported 68.5%) is the most important negative result in the field: *raw text plus competent search is a strong baseline.* Letta's 2026 move to git-backed "context repositories" doubles down on that. *Verdict:* adopt the principle that verbatim history must be first-class and searchable, and that memory should be inspectable like files. Agent-authored memory is not the source of truth for Orbit: it puts one model's judgement in charge of the record.

**Zep / Graphiti.** The best-engineered treatment of time: every fact edge carries when it became true and when it stopped, plus when the system learned and unlearned it, and contradicted edges are invalidated rather than deleted. Retrieval needs no LLM. Costs: 6–10 LLM calls per ingested episode, a graph database to operate, and facts compressed into edges. *Verdict:* adopt bi-temporality and invalidate-don't-delete wholesale; extend it with the distinctions Graphiti lacks (assertion vs belief, correction vs change, scope); do not adopt the graph database as system of record.

**Cognee, GraphRAG.** Good at turning a static corpus into something navigable; GraphRAG's community summaries answer "what are the themes here?", which no fact store can. Neither models change, provenance below chunk level, or permissions; GraphRAG's indexing cost led Microsoft itself to LazyGraphRAG, which defers the LLM work to query time. *Verdict:* borrow hierarchical summaries for the *views* layer and the *lazy* idea for extraction.

**HippoRAG 2.** Demonstrates that Personalised PageRank over a graph that *includes the passages themselves* improves associative, multi-hop retrieval without the usual loss on simple factual questions. Its abstract names the trap precisely: structure-augmented methods that replace passages do worse than plain RAG on basic factual memory. *Verdict:* adopt PPR-style spreading and keep evidence in the graph.

**Hindsight.** Separates world facts, experiences, entity summaries and evolving beliefs — the first system here to treat *evidence versus inference* as structural. *Verdict:* same instinct as Orbit's epistemic ladder; Orbit goes further by making the separation a type rule that the storage layer enforces.

**Agent Zero Memory (Aug 2026).** Closest prior art to this design: provenanced items with evidence pointers, an intent gate, and answers that may cite only evidence the reader actually opened, so the system abstains instead of guessing. Reports 95.60% on LongMemEval. *Verdict:* confirms that provenance + citation lock + abstention is the winning direction. It does not describe belief revision, trust labels or exactness guarantees in derived text, which is where Orbit adds.

**Mastra Observational Memory.** With no retrieval at all — just a compressed, dated log held in a stable context prefix of about 30k tokens — it reports 94.87% on LongMemEval with a small reader model. *Lesson:* at the 10⁵-token scale, a good compressed log beats elaborate retrieval and keeps prompt caches warm. *Verdict:* adopt exactly this for working memory and briefs (stable, cacheable prefix); it cannot be the whole answer at 10⁸ tokens.

**SYNAPSE.** Prior art for spreading activation in agent memory, with lateral inhibition and temporal decay. *Verdict:* Orbit's Typed Spreading Activation builds on this and HippoRAG; what is new is listed honestly in [08](../architecture/08-retrieval.md#13-what-is-new-here).

**LangGraph / LangMem, LlamaIndex.** Application-level conveniences: a place to put JSON and some patterns for when to write. Useful for building an agent; not a memory architecture. *Verdict:* Orbit should be easy to plug into them, not built from them.

**Native assistant memory and vendor import/export.** Each vendor's memory is a silo. During 2026 the major assistants added import or export of saved memories (reported as one-way and lossy), and an open bundle format has been proposed (MacPaw's *Portable Memory*). *Verdict:* treat these as **import sources** and as an **export target**; they are evidence that the portability problem is recognised, not solutions to it.

**MCP memory servers.** A crowded field that proves the demand for "one memory across Claude, ChatGPT and Cursor". Nearly all are a fact store or note store behind a handful of tools. *Verdict:* this is the product category Orbit competes in; the differentiation is everything below the tool surface.

## 2. Storage and retrieval technology

| Technology | What it is good at | Limits for Orbit | Verdict |
|---|---|---|---|
| **PostgreSQL** (18; 19 in release candidate) | Transactions, constraints, range types and temporal keys, row-level security, JSONB, logical replication, mature operations | Not a search engine or a graph engine by itself | **System of record** |
| **pgvector** | Dense vectors beside the data; HNSW; half-precision and binary types; filtered scans | Shared indexes with a tenant filter degrade at many small tenants | Use in early phases; later only as fallback |
| **pg_textsearch / ParadeDB pg_search** | Real BM25 inside Postgres | Extension availability on managed Postgres; young (pg_textsearch 1.0 shipped March 2026) | Optional; lexical search lives in the recall engine |
| **SQL/PGQ in Postgres 19** | Graph pattern syntax over relational tables | Rewrites to joins; no variable-depth traversal in 19 | Convenience for fixed patterns only |
| **Neo4j** | Mature property graph, Cypher, graph algorithms | A second source of truth; clustering is an enterprise feature; multi-tenant isolation is awkward at millions of small graphs | Do not use as system of record |
| **FalkorDB / Kuzu** | Fast or embedded graph engines; Graphiti backends | Smaller ecosystems; check upstream maintenance before depending on either | Not needed (see below) |
| **Qdrant** | Rust; strong filtering; sparse + dense + multi-vector; per-tenant partitioning | Another stateful service; graph and lexical still elsewhere | **Best off-the-shelf vector engine** if brain images are postponed |
| **Weaviate / Milvus** | Hybrid search / very large shared collections | Sized for few-large rather than many-small indexes | Not a fit |
| **Redis / Valkey** | Sub-millisecond state | Not durable truth | Working-memory cache only |
| **SQLite** (+ FTS5, vector extensions) | Zero-ops, embedded, one file per brain | Vector search is basic | **Local replica and local-brain mode** |
| **OpenSearch / Elasticsearch** | Mature lexical + vector, aggregations | Heavy; per-tenant index sprawl | Not a fit |
| **Lance / LanceDB** | Versioned columnar tables on object storage with vector and BM25 indexes | Younger; verify index features in a spike | **Leading candidate for the brain-image format** |
| **XTDB** | Bitemporal by construction | Smaller ecosystem; no vector/RLS story comparable to Postgres | Model its semantics in Postgres instead |

**Why no graph database.** A personal graph is small — a decade of heavy use is on the order of 10⁶–10⁷ edges, well under a gigabyte in compressed adjacency form. Traversal-heavy retrieval wants that in process memory beside the lexical and vector indexes, not behind a network hop. Durable truth wants transactions, constraints and row-level security. So edges live in Postgres tables and are *projected* into an in-memory graph per user. This removes a whole class of dual-write and isolation problems.

**The many-small-tenants regime.** Orbit's shape — millions of users, each with a small index — is the opposite of what most vector databases optimise for. Systems built for it keep one physically separate index per tenant on object storage and load it on demand. That is the reasoning behind *brain images* in [12](../system-design/12-system-design.md).

## 3. Benchmarks: what they measure and how far to trust them

| Benchmark | Size | Tests | Caveats |
|---|---|---|---|
| **DMR** (from MemGPT) | Short | Single-fact recall | Saturated (~94–95%) |
| **LoCoMo** | 10 conversations, ~1.5k questions | Single-hop, multi-hop, temporal, open-domain, adversarial | An independent audit found **6.4% of the answer key wrong** and a judge that accepted up to 63% of deliberately wrong answers; no knowledge-update questions; fits in a modern context window |
| **LongMemEval** | 500 questions; S ≈ 115k tokens, M ≈ 1.5M | Extraction, multi-session reasoning, temporal reasoning, **knowledge updates, abstention** | The best public design; S is near-saturated in vendor reports |
| **MemoryAgentBench** | Incremental multi-turn | Retrieval, test-time learning, long-range understanding, conflict resolution | Newer; fewer reference results |
| **BEAM** | 100 conversations, 100k → **10M** tokens, 2,000 questions | Ten memory abilities at scale | The only public test near Orbit's regime |

Three facts about the numbers in circulation:

1. **They are not comparable.** Top LongMemEval claims cluster at 94–96%, each with a different reader model, judge and prompt. One "96.6%" turned out to be retrieval recall, not answer accuracy.
2. **They do not survive a controlled harness.** A 2026 study that ran Letta, Mem0 and its own system under one harness on LongMemEval-S, with repaired adapters and a binary judge, measured all three at about 47–48%.
3. **They are disputed even between vendors.** Mem0 and Zep published mutually contradictory re-runs of each other on LoCoMo, with Zep's score landing anywhere from 58% to 75% depending on who ran it.

What LongMemEval did establish, and Orbit adopts: store at a finer grain than the session; index by extracted facts as well as raw text; make the query time-aware; and read with structured notes. Its headline — a ~30% accuracy drop for commercial assistants over sustained interaction — is the problem statement.

## 4. Lessons carried into the design

| | Lesson | Where it lands |
|---|---|---|
| L1 | Lossy write-time extraction makes errors permanent | Evidence ledger is the source of truth (INV-1) |
| L2 | Raw text + good search is the baseline to beat, and the floor to fall back on | Lexical/dense search over evidence always available |
| L3 | Time must be first-class, on two axes | Bitemporal beliefs |
| L4 | Graphs help association but hurt simple recall if they replace passages | Evidence stays in the graph; the graph is an index, not the store |
| L5 | Provenance, citation lock and abstention drive the best results | Grounding gate |
| L6 | A stable compressed prefix beats retrieval at small scale | Identity card, project brief and working memory as cacheable prefix |
| L7 | Scale (10⁷ tokens and beyond) is the open frontier | Evaluate on BEAM-scale and beyond from day one |
| L8 | LLM-heavy graph construction dominates cost | Salience gating and lazy extraction |
| L9 | Public benchmarks are noisy and gameable | Programmatic ground truth in OrbitBench |
| L10 | Security is almost absent from memory frameworks | Integrity labels and grants are core, not add-ons |
| L11 | Evidence vs inference separation is emerging but unenforced | Epistemic type rules |
| L12 | Inspired complexity does not guarantee gains | Ablate every mechanism |

## 5. Build-or-adopt summary

| Layer | Decision |
|---|---|
| System of record | **Adopt** PostgreSQL |
| Temporal model | **Adopt** Graphiti's bi-temporal, invalidate-don't-delete idea; **extend** with claims, scope and correction |
| Associative retrieval | **Combine** HippoRAG-style PPR with SYNAPSE-style activation dynamics; **extend** with typed edges, validity and policy masks |
| Compressed stable context | **Adopt** the observational-log pattern for working memory and briefs |
| Extraction | **Build**, anchored and verified; **borrow** add-only and lazy ideas |
| Belief revision, epistemic and integrity typing, exactness guarantees | **Build** — no adequate prior implementation found |
| Interop | **Adopt** MCP (2026-07-28) as primary; **build** REST, gateway and capture connectors around it |
| Whole frameworks (Mem0, Letta, Zep, Cognee) | **Do not adopt as the core**; support as import sources where useful |
