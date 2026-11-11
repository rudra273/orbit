# Orbit — the brain

Research, architecture and system design for Orbit's memory: a persistent, human-inspired memory layer that belongs to one person and is shared by every AI they use.

> **Status: design, not yet built.** Nothing here has been implemented or measured. Mechanisms marked *Hypothesis* are original proposals with a test attached; figures in the evaluation and cost documents are targets and estimates. Research pass dated 2026-11-11.

> **Deployment decision (2026-11-11).** Orbit is built first for its owner alone: **one brain per deployment, running in the owner's own cloud**. When it works, others get the same codebase to deploy in their cloud. A hosted service holding many people's brains is not planned for now. No memory feature is reduced for this; only fleet infrastructure is deferred. See [12 §8](system-design/12-system-design.md#8-deployment-model-and-upgrade-path).

## The design in one page

**The problem.** Today each AI keeps its own shallow, lossy, unverifiable memory. Existing memory frameworks improve on that but share three weaknesses: they *extract and discard* (so extraction errors are permanent), they *cannot tell what was said from what they concluded*, and they have *no model of trust* for a memory that many programs read and write.

**The answer: an Evidential Memory Architecture.** Four layers, each derived from the one below and rebuildable from it.

| Layer | Holds | Rule |
|---|---|---|
| **L0 Evidence** | The exact bytes of what happened | Append-only. The only source of truth |
| **L1 Claims** | Who asserted what, with what force, anchored to byte spans | Every claim cites its source |
| **L2 Beliefs** | What Orbit currently holds, per subject · predicate · scope, on two timelines | Never edited: change makes a new version |
| **L3 Views** | Identity card, project briefs, profiles, summaries | Disposable; every sentence cited |

**Ten decisions that define it.**

1. **Store raw first, understand later.** A write is durable and searchable before any model sees it. Understanding happens offline, repeatedly, and can be redone with a better model.
2. **Separate claims from beliefs.** "The user said X" and "X is true now" are different records. This is what makes corrections, ratified suggestions and context-dependent preferences representable.
3. **Exact strings are copied, never generated.** Models point at literals through handles; a validator rejects anything they type themselves. A summary of a summary still contains the original bytes.
4. **Two timelines.** *When it was true* and *when Orbit believed it* are tracked separately, so change, correction and late-arriving history each do the right thing.
5. **Scope before supersede.** A conflicting statement is tested first as a narrower context, then as a change over time, then by authority — and is reported as a dispute if none applies.
6. **Basis and integrity are types.** `stated · observed · reported · inferred · assumed` and the trust level of the channel are computed, enforced by the storage layer, and never raised by a model's opinion or by Orbit hearing its own output.
7. **Recall by resonance.** Seven retrieval channels in parallel, activation spreading over a typed memory graph, then a grounding gate that verifies against evidence and returns one of four verdicts — including *not found*.
8. **Forgetting is ranking.** Decay changes what surfaces first; nothing is destroyed except by the owner.
9. **One brain, many clients, each with a grant.** MCP is the primary interface (written against the stateless 2026-07-28 specification), backed by a REST API, capture connectors and an optional LLM gateway. Every client sees only what its grant allows, and every disclosure is logged.
10. **PostgreSQL is the record; everything else is a projection.** One writer per brain; one in-memory index per brain; one brain per deployment, in the owner's cloud.

**What is new, and what is borrowed.**

| Original to this design (*Hypothesis*) | Adopted from proven work |
|---|---|
| Claim / belief separation with justification-driven retraction | Complementary learning systems: fast store + slow store + replay |
| Handle-locked rendering for exactness | Bi-temporal, invalidate-don't-delete facts (Graphiti) |
| Scope-Before-Supersede revision with order-independent slot replay | Personalised PageRank over a graph that keeps passages (HippoRAG 2) |
| Echo Guard against self-corroboration | Spreading activation with inhibition and decay (SYNAPSE; ACT-R) |
| Intent-typed, validity- and policy-masked spreading activation | Citation-locked answers and abstention (Agent Zero Memory) |
| Integrity-typed memory and non-interfering retrieval | Stable compressed context prefix (observational memory) |
| Calibrated verdicts from retrieval features | Hybrid lexical + dense retrieval with rank fusion |
| Per-user brain images | Statement + qualifiers + references (Wikidata); truth maintenance (Doyle) |

## Documents

Read in order for the full argument, or jump by question.

### Research

| | Document | Answers |
|---|---|---|
| 01 | [Problem definition](research/01-problem-definition.md) | What exactly is being built? What must be true of it? |
| 02 | [Cognitive science](research/02-cognitive-science.md) | What does human memory teach — and what must not be copied? |
| 03 | [Prior art](research/03-prior-art.md) | What exists? Adopt, extend, combine or build? |
| 04 | [Architecture options](research/04-architecture-options.md) | Four candidate architectures and why one wins |

### Architecture

| | Document | Answers |
|---|---|---|
| 05 | [Memory model](architecture/05-memory-model.md) | Canonical representation, taxonomy, type rules, knowledge graph |
| 06 | [Memory formation](architecture/06-memory-formation.md) | Capture, extraction, salience, **exact-memory strategy** |
| 07 | [Consolidation and evolution](architecture/07-consolidation.md) | Revision, confidence, sleep cycles, forgetting, identity |
| 08 | [Retrieval](architecture/08-retrieval.md) | Hybrid associative recall, algorithms and pseudocode |
| 09 | [Project memory](architecture/09-project-memory.md) | The universal, living `AGENTS.md` |
| 10 | [Interoperability](architecture/10-interop-mcp-apis.md) | MCP server, tool specifications, REST API, gateway, permissions |
| 11 | [Security and privacy](architecture/11-security-privacy.md) | Threat model, memory firewall, encryption, ownership |

### System design

| | Document | Answers |
|---|---|---|
| 12 | [System design](system-design/12-system-design.md) | Services, data flows, concurrency, sync, scale |
| 13 | [Data model](system-design/13-data-model.md) | Schemas |
| 14 | [Tech stack](system-design/14-tech-stack.md) | What it is built with, and what is left out |

### Validation

| | Document | Answers |
|---|---|---|
| 15 | [Evaluation](validation/15-evaluation.md) | How to know it works |
| 16 | [Cost and scale](validation/16-cost-and-scale.md) | What it costs; what breaks first |
| 17 | [Roadmap and risks](validation/17-roadmap-risks.md) | Order of work, risks, open questions |
| — | [References](references.md) | Sources |

## Where each requested deliverable lives

| # | Deliverable | Document |
|---|---|---|
| 1 | Executive summary of the vision | This page; [01 §1](research/01-problem-definition.md) |
| 2 | Definition of the universal second-brain problem | [01](research/01-problem-definition.md) |
| 3 | Cognitive science and neuroscience research | [02](research/02-cognitive-science.md) |
| 4 | Existing memory architecture comparison | [03](research/03-prior-art.md), [04](research/04-architecture-options.md) |
| 5 | Proposed memory taxonomy | [05 §6](architecture/05-memory-model.md) |
| 6 | Canonical memory representation | [05 §1–5](architecture/05-memory-model.md) |
| 7 | Memory formation and ingestion pipeline | [06](architecture/06-memory-formation.md) |
| 8 | Consolidation and evolution architecture | [07](architecture/07-consolidation.md) |
| 9 | Exact-memory preservation strategy | [06 §5](architecture/06-memory-formation.md) |
| 10 | Hybrid associative retrieval architecture | [08](architecture/08-retrieval.md) |
| 11 | Knowledge graph design | [05 §7–8](architecture/05-memory-model.md) |
| 12 | Database and storage recommendations | [03 §2](research/03-prior-art.md), [12 §5](system-design/12-system-design.md), [14](system-design/14-tech-stack.md) |
| 13 | MCP server architecture | [10 §2](architecture/10-interop-mcp-apis.md) |
| 14 | Universal AI interoperability design | [10](architecture/10-interop-mcp-apis.md) |
| 15 | Cloud infrastructure architecture | [12](system-design/12-system-design.md) |
| 16 | Security and privacy architecture | [11](architecture/11-security-privacy.md) |
| 17 | Project memory architecture | [09](architecture/09-project-memory.md) |
| 18 | API and MCP tool specifications | [10 §3, §7](architecture/10-interop-mcp-apis.md) |
| 19 | Data models and schemas | [13](system-design/13-data-model.md) |
| 20 | System architecture diagrams | [12 §1](system-design/12-system-design.md), [04](research/04-architecture-options.md) |
| 21 | End-to-end data flow diagrams | [12 §3–4](system-design/12-system-design.md), [06 §2](architecture/06-memory-formation.md) |
| 22 | Retrieval algorithms and pseudocode | [08](architecture/08-retrieval.md), [07 §1–2](architecture/07-consolidation.md) |
| 23 | Evaluation and benchmarking strategy | [15](validation/15-evaluation.md) |
| 24 | Cost and scalability analysis | [16](validation/16-cost-and-scale.md) |
| 25 | Risks, limitations, unresolved research questions | [17 §3–5](validation/17-roadmap-risks.md) |
| 26 | Phased implementation roadmap | [17 §1–2](validation/17-roadmap-risks.md) |

## Conventions used throughout

- **Proven / Adapted / Hypothesis** mark how much evidence stands behind a mechanism.
- **INV-n** refers to the ten design invariants in [01 §4](research/01-problem-definition.md).
- Every major decision ends in a table: alternatives, why this one, what it costs, how to test it.
- Diagrams are Mermaid and render on GitHub.
