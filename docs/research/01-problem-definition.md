# 01 · Problem definition

> **What this document decides:** what "universal second brain" has to mean in engineering terms, which requirements are measurable, and where the real difficulty is. Everything later is judged against this page.

## 1. The vision in one paragraph

Orbit is a memory layer that belongs to one person and is shared by every AI they use. Claude, Codex, ChatGPT, a local model and Orbit's own apps all read from and write to the same brain, so switching models never costs the user their identity, knowledge, project history or context. The model is replaceable; the memory is not. Later the same brain backs three roles — a model of how the owner thinks, an assistant that knows everything relevant, and an agent that acts for them — but all three are only as good as the memory underneath, so memory is built first and autonomy second.

## 2. What this is not

Five neighbouring categories look similar and fail for different reasons. The last column is the failure that matters for Orbit.

| | Unit of memory | Who decides what is kept | Time | Truth over time | Provenance | Shared across models | Characteristic failure |
|---|---|---|---|---|---|---|---|
| **RAG over documents** | Chunk | Nobody (everything, flat) | Ignored | None: old and new chunks coexist | Chunk → file | Yes, read-only | Retrieves text that was true once; cannot say "I don't know" |
| **Chatbot memory** (ChatGPT, Claude, Gemini native) | Short fact / summary | Vendor's hidden pipeline | Weak | Overwrite | Usually none | **No** — one silo per vendor | Not portable, not inspectable, not exact |
| **PKM tools** (notes, wikis) | Note / page | The user, by hand | Manual | Manual | Manual links | Via export | Depends on the user doing the remembering |
| **Agent memory frameworks** (Mem0, Letta, Zep, Cognee…) | Extracted fact / block / graph edge | An LLM at write time | Varies; best in Zep | Varies: overwrite, invalidate, or append | Partial | Per-app, per-agent | Lossy extraction; inference indistinguishable from fact; no trust model |
| **Personal intelligence layer** (Orbit) | Evidence → claim → belief → view | Raw kept always; promotion by measured salience | First-class, two timelines | Versioned, justified, reversible | Byte-exact, to the source span | **Yes**, with per-client permissions | (to be prevented by design — see §5) |

The decisive differences are three. Orbit must hold **one person's whole history** (not a corpus, not one app's sessions), it must stay **correct while that person changes their mind**, and it must be **safe to expose to software it does not control**.

## 3. Requirements, stated so they can be tested

| # | Requirement | Measurable form (suite in [15-evaluation](../validation/15-evaluation.md)) |
|---|---|---|
| R1 | **Lossless capture** | Any ingested span is retrievable byte-for-byte for as long as the user keeps it |
| R2 | **Exact recall** | Commands, identifiers, versions, URLs, numbers, quotes: exact-match on recall; literal corruption rate = 0 |
| R3 | **Seven memory types** | Identity, episodic, semantic, project, procedural, relationship, working — each has a passing suite |
| R4 | **Correct over time** | "As of" queries return the version valid then; change ≠ correction ≠ contextual variation |
| R5 | **Epistemic honesty** | Stated / observed / reported / inferred / assumed never conflated; an inference never becomes a fact without user-origin evidence |
| R6 | **Associative recall** | Finds related memories across conversations, projects and vocabularies (multi-hop, cross-client suites) |
| R7 | **Knows what it doesn't know** | Calibrated abstention; false-memory rate bounded at a stated answer coverage |
| R8 | **Context-aware, budgeted retrieval** | Accuracy per 1k context tokens; no unrelated personal data in a coding context |
| R9 | **Model-independent** | Every derived artefact rebuildable with a different extractor/embedding model; no retrieval path requires one vendor |
| R10 | **One brain, many clients** | A fact written through client A is recallable from client B within seconds, with provenance naming A |
| R11 | **User ownership** | Inspect, correct, export, delete — including everything derived from a deleted item |
| R12 | **Safe to connect** | Stored prompt injections and write-path poisoning do not become instructions or facts; per-client grants enforced |
| R13 | **Future-proof for autonomy** | Goals, intentions and outcomes are representable now, without a redesign later |

## 4. Design invariants

The brief's eighteen non-negotiable principles collapse into ten properties that are enforced by schema and tests rather than by good intentions. They are referenced as **INV-n** throughout.

| | Invariant | Enforced by |
|---|---|---|
| INV-1 | Evidence is immutable and is the only source of truth; everything else is derived and rebuildable | Append-only ledger; replay test |
| INV-2 | Every derived statement points to the exact source bytes that justify it | Anchor foreign keys + hash check |
| INV-3 | Exact strings are copied, never generated | Handle-locked rendering; byte-equality test |
| INV-4 | Nothing is edited in place: change creates a new version, and the old one stays queryable | Bitemporal versions; exclusion constraint |
| INV-5 | `basis` rises only on evidence of the right kind — never on a model's opinion, and never on Orbit hearing its own output | Promotion rules in the revision function; Echo Guard |
| INV-6 | Low-integrity content can inform, but cannot instruct and cannot define who the user is | Integrity labels + promotion gates |
| INV-7 | A client can neither receive nor be *influenced by* data outside its grant | Policy mask applied before ranking |
| INV-8 | Deleting evidence retracts everything that depended only on it | Justification graph propagation |
| INV-9 | The belief state is a deterministic function of the evidence *set*, not of arrival order or of which model happened to run | Slot-local replay; cached judgments |
| INV-10 | No number in a claim about quality without an evaluation that produced it | Release gates |

## 5. Where the difficulty actually is

Ranked by how much of the outcome each one controls.

1. **Faithful formation.** Deciding what was actually said, by whom, about what, with what force — without paraphrase drift, without promoting an assistant's suggestion into the user's decision, and without losing the exact string. Every existing system that extracts at write time and discards the source has made its extraction errors permanent.
2. **Truth over time.** A later statement can be a *change* ("I used to prefer Python"), a *correction* ("that was wrong"), a *narrower context* ("Rust for systems code"), or *noise*. These need four different responses; most systems have one (overwrite) or two (invalidate / append).
3. **Retrieval at lifetime scale.** Public benchmarks top out between 10⁵ and 10⁷ tokens of history. A heavy AI user produces roughly 20–50 million tokens of conversation a year, so a single year exceeds the largest benchmark and a decade lands near 10⁹ with documents. Self-reported results already fall sharply with scale: Mem0's own figures on BEAM drop from 64.1 at 1M tokens to 48.6 at 10M ([Mem0, 2026](../references.md#systems)). Near-saturated scores on LongMemEval-S (~115k tokens) say little about this regime.
4. **The trust boundary.** A shared memory is the "private data" leg of every connected agent's lethal trifecta, and a persistent store for anything injected into it. Memory poisoning is now listed separately from prompt injection in OWASP's agentic risk list (ASI06). Very little of the memory-framework literature treats this as a design input.
5. **Capture coverage.** MCP is model-controlled: recall and capture happen only if the model decides to call a tool. Reliable memory needs deterministic capture outside the model's discretion.
6. **Evaluation.** The standard benchmark (LoCoMo) has a measurably wrong answer key and a lenient judge; vendor numbers are not comparable; and none of the public suites test exactness, provenance, epistemic status, permissions or injection.

## 6. Non-goals for the memory phase

- Indexing whole codebases (coding agents already do this; Orbit stores knowledge *about* code, with pointers).
- Storing credentials in memory (they go to a vault and are never recalled — see [11-security-privacy](../architecture/11-security-privacy.md)).
- Fine-tuning any model on user data (unattributable, hard to delete, model-dependent).
- Autonomy. The memory model reserves room for it (R13) and nothing more.

## 7. Assumptions this research rests on

- The owner is one person; multi-user/team brains are a later extension of compartments and grants.
- A cloud service may process plaintext to extract and index. If that is unacceptable, the confidential-computing and local-brain modes in [12-system-design](../system-design/12-system-design.md) apply, at a cost in capability or price.
- Small, cheap LLMs remain adequate for bulk extraction when their output is mechanically verified. If they are not, cost per user rises by roughly an order of magnitude ([16-cost-and-scale](../validation/16-cost-and-scale.md)).
