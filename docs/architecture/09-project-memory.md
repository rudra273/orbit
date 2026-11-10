# 09 · Project memory: a living, universal AGENTS.md

> **What this document decides:** how a project is represented, how the active project is identified, and how Orbit compiles the right context for whichever agent is working on it — instead of every agent re-reading a hand-maintained file.

## 1. What a project is

A project is not a document. It is:

- an **entity** (type `project`) with components, repositories and people attached;
- a **scope**: beliefs can hold *for this project*, or for one of its components;
- a set of **typed beliefs** (below), each with basis, validity and provenance like any other;
- **views** compiled from those beliefs: the card, the brief, per-section pages;
- **bindings** that let a client say "I'm in this project" without knowing Orbit's ids.

Because it is made of ordinary beliefs, project memory inherits everything in [05](05-memory-model.md)–[08](08-retrieval.md): history, corrections, exact strings, permissions. `AGENTS.md` and `CLAUDE.md` become an import source and an export format; Orbit is the canonical record.

## 2. Typed project knowledge

| Kind | Holds | Example |
|---|---|---|
| `project.purpose` | Why it exists; vision | "A memory layer shared by every AI the owner uses" |
| `project.goal` / `non_goal` | What it is and is not trying to do, with horizon | "No autonomy until memory passes its gates" |
| `project.constraint` | Hard limits: legal, cost, platform, compatibility | "Must not depend on one LLM provider" |
| `project.architecture` | Components, responsibilities, data flow | "Recall service loads a per-user index image" |
| `project.stack` | Technology choices with role and version | "System of record: PostgreSQL ⟦18⟧" |
| `project.decision` | A decision record — see §3 | "No graph database as system of record" |
| `project.rejected_approach` | A considered option and why it lost | "Neo4j as the store — per-tenant isolation, dual writes" |
| `project.open_question` | Undecided, with what would settle it | "Lance or a custom format for index images?" |
| `project.issue` | Known problems, status | |
| `project.task`, `project.milestone` | Work, state, order | |
| `project.convention` | How things are done here (scoped procedural memory) | "Every derived table carries `derivation_id`" |
| `project.glossary` | The project's own terms — exact strings | "*brain image*: the per-user index bundle" |
| `project.artifact` | Repos, key files, docs, URLs, with pointers | |
| `project.command` | Build, test, run, deploy — as literal handles with roles | |
| `project.dependency`, `project.stakeholder` | Other projects and services; people and roles | |
| `project.change` | Notable changes, linked to what they changed | |

Each is a belief, so each can be `stated`, `observed` (read from the repository), `inferred` or `disputed`, can be superseded, and cites its evidence.

## 3. Decision records

Decisions are the part of project history agents most often get wrong, so they have structure — essentially an architecture decision record, kept current automatically.

| Field | Source |
|---|---|
| Decision | A `decide` claim, or a ratified proposal |
| Status | `proposed` · `accepted` · `superseded` · `rejected` · `deprecated` |
| Context | The problem being solved, from the surrounding episode |
| Alternatives | `alternative_to` links to rejected approaches |
| Rationale | `rationale_for` links, each anchored to a quote |
| Consequences | Later beliefs and issues that cite it |
| Lineage | What it supersedes; what superseded it |

"Why did I reject that earlier architecture?" is a walk over these links, ending in the user's own words.

A preference is not a decision, and a decision that was only discussed is `proposed`. The distinction is preserved because agents act differently on each.

## 4. Identifying the active project

Earlier MCP revisions let a server ask the client for its workspace roots. The 2026-07-28 revision deprecates that; context now travels as ordinary tool arguments. Orbit therefore accepts **hints** and resolves them.

| Hint | Strength |
|---|---|
| Explicit project id or slug | Exact |
| Git remote URL | Strong |
| Root commit hash (the repository's first commit — stable across clones and renames) | Strong |
| Package name from a manifest | Medium |
| Workspace folder name | Weak |
| The conversation itself (entities mentioned) | Weak; used only to suggest |

A `project_binding` table maps fingerprints to projects. An unknown repository yields either a suggestion ("looks like *Orbit* — bind it?") or a provisional project; confirmation goes through an elicitation when the client supports it, or the Orbit UI.

Bindings are per user and may be many-to-one (several repositories, one project).

## 5. The living brief

| Tier | Size | Contents | Changes |
|---|---|---|---|
| **T0 Card** | 120–200 tokens | Name, one-line purpose, stage, headline stack, top priorities, the few rules that must never be broken | Rarely — cacheable |
| **T1 Brief** | 800–1,500 tokens | Purpose and vision; architecture overview; stack with versions; accepted decisions, one line each with ids; conventions; current state and roadmap; open questions; known issues; glossary highlights; key commands | When cited beliefs change |
| **T2 Task pack** | Budgeted per request | Task-conditioned retrieval inside the project scope, plus the user-level preferences and procedures that apply here | Every request |
| **T3 On demand** | — | Expansion by id: full decision records, episodes, quotes | — |

**How it is built.** Sections are assembled from typed beliefs by template — the structure is already in the data. A model is used only to compress prose, under handle locking, and every line carries the belief ids it rests on. When a belief changes, only the sections that cite it are regenerated.

**It is versioned.** Each regeneration is a new version with a diff, which enables the most useful thing a shared project memory can do:

### Delta briefing

An agent that worked on the project last week does not need the brief again; it needs **what changed**. `orbit_context(…, since=<handle or version>)` returns new decisions, superseded ones, new issues, closed tasks and the latest handoff note — typically a few hundred tokens. Across agents this is how Codex learns what Claude decided yesterday without anyone copying anything.

### Illustrative card

What Orbit's own card would look like if this design were adopted:

```text
ORBIT — memory layer shared by every AI its owner uses.  Stage: design.
Stack: Rust core · PostgreSQL 18 (system of record) · per-user index images.
Now: Phase 0 — evidence ledger, baseline retrieval, evaluation harness.
Never: edit evidence · generate an exact string · serve an inference as a fact.
```

## 6. Task packs: context for the job at hand

When Codex is about to work on a Rust module, the request names the task and, optionally, the files. Orbit returns:

| Section | Source |
|---|---|
| Card + brief (or delta) | Views |
| Decisions touching this component, with rationale and rejected alternatives | Slot channel, scope = component; spreading along decision links |
| Conventions and the user's coding preferences that apply | `project.convention`; `procedural.*` and `identity.preference` whose scope matches (most specific wins) |
| Recent history of this area | Episodes mentioning the component or files, newest first, as short cards |
| Exact commands and identifiers | Literal handles with role |
| Open questions, known issues, pending intentions here | Typed beliefs; triggered intentions |

It does **not** return personal conversations, other projects, or anything above the client's sensitivity ceiling — both because the scope excludes them from retrieval and because the grant masks them ([11](11-security-privacy.md)). When Claude is helping with a business decision instead, the same machinery runs with a different intent, scope and grant, and returns a different context.

## 7. Import and export

**Import.** `AGENTS.md`, `CLAUDE.md`, READMEs and decision records are ingested as `user_artifact` evidence — anchored to file, line range and commit. Stated conventions and commands become claims like any others. A file from a repository the user does not control is `external` and cannot define conventions.

**Export.** The T1 brief, conventions and commands are rendered into `AGENTS.md` (or any similar format) with a generated-file header and belief ids in comments. A CLI command or a repository hook refreshes it.

**Round trip.** If someone edits the exported file by hand, the diff is re-imported as evidence. The file is a view and an input; it is never the source of truth.

## 8. Knowledge about code

Orbit does not index the codebase — the coding agent has the files. Orbit holds what the files cannot say: why a module is shaped the way it is, which approach was tried and abandoned, the gotcha discovered at 2 a.m. Such beliefs carry **pointers** (path, symbol, commit). At recall the pointer's commit is compared with what the client reports; a mismatch is returned as *may be stale* and queued for re-verification.

## 9. Compared with a static file

| | `AGENTS.md` | Orbit project memory |
|---|---|---|
| Maintained by | Hand | Derived from every conversation and commit, reviewed by the user |
| Size sent to the agent | The whole file, every time | A card, a delta, or a task-sized pack |
| History | Git log of the file | Every decision's lineage, with reasons |
| Rejected approaches | Usually absent | First-class |
| Shared across tools | If each tool reads the same file | By construction |
| Exact commands | Whatever was typed into the file | Anchored to where they were actually used |
| Distinguishes stated from guessed | No | Yes |
| Scope | One repository | Project, component, and the user's cross-project preferences |

## 10. Decisions

| Decision | Alternatives | Why this | Cost | Test |
|---|---|---|---|---|
| Project memory as typed beliefs | A Markdown document per project, edited by agents | History, corrections, permissions and exactness come for free; no agent owns the record | Needs a kind taxonomy | Brief coverage / precision against ground-truth project state |
| Template-first brief generation | Free-form LLM summary | Less to hallucinate; every line cited | Less fluent prose | Citation validity; human rating |
| Hints + bindings for project identity | Client roots | Roots is deprecated; hints work in every client | Ambiguity on first contact | Binding accuracy on scripted sessions |
| Delta briefing | Always send the full brief | Tokens; attention on what changed | Version tracking per handle | Tokens to reach equal task success |
| Files as import/export | Files as the source of truth | One record, many renderings | Round-trip logic | Round-trip consistency |
