# Orbit

A persistent memory that belongs to one person and is shared by every AI they use.
One brain per deployment, running in its owner's cloud.

The design is in [docs/](docs/README.md).

## Status

**Phase 0 (floor and yardstick), in progress.** Your agent history can be imported, stored as exact evidence, searched, and recalled over REST and MCP. Still to do in Phase 0: the meaning-based (vector) search channel and the evaluation harness.

The pure core of Phase 1 (formation checks and belief revision) is also built and tested, but not wired in.

## Layout

| Path | What it is |
|---|---|
| `crates/orbit-core` | Shared types: typed ids, labels (integrity, basis, speech act…), scope, byte anchors, the ledger hash chain |
| `crates/orbit-ledger` | Admission (validation, secret redaction, labels) and the append-only evidence ledger; owns the migrations |
| `crates/orbit-import` | Parsers for agent transcripts on disk: Claude Code and Codex |
| `crates/orbit-index` | Raw indexing: full-text vectors and literals (commands, paths, URLs, versions, code) for new evidence |
| `crates/orbit-recall` | Recall over raw evidence: ranked full-text search and byte-exact literal lookup |
| `crates/orbit-server` | The engine: ingest, recall and exact over REST; `orbit_recall` and `orbit_exact` over MCP; background indexing |
| `crates/orbit-cli` | The `orbit` command: `orbit import claude-code`, `orbit import codex` |
| `crates/orbit-formation` | Phase 1: deterministic checks on model output (literals, handle templates, anchor-verified claims) |
| `crates/orbit-revision` | Phase 1: belief revision (Scope-Before-Supersede, slot replay, assessment) |
| `migrations/` | PostgreSQL schema, applied at start-up |
| `evals/` | Placeholder for the evaluation harness |

## Run it

Needs Rust and Docker.

```bash
make db
```

```bash
cp .env.example .env
```

Fill in `ORBIT_TENANT_ID` (`uuidgen`) and `ORBIT_TOKEN` (`openssl rand -hex 32`) in `.env`, then:

```bash
make run
```

## Import your history

With the server running, in another terminal:

```bash
make import
```

This reads `~/.claude/projects` and `~/.codex/sessions`. Re-running it only adds what is new. What is left out is reported, never dropped silently:

- **subagent transcripts:** assistant-to-assistant work, not your conversation;
- **tool output over 16 KiB:** skipped rather than cut, because evidence is never truncated;
- **non-conversation records:** settings, usage, reasoning.

Who said what is kept:

- **Your typed words:** marked `user_direct`.
- **Assistant text and tool calls:** marked `agent`.
- **Client-injected text:** stored as `system`, not as yours. In Codex, only the first block of a user message counts as typed; the rest is client context.

## Connect an AI client

The MCP endpoint is `http://127.0.0.1:8787/mcp`, with the token as a bearer header. For Claude Code:

```bash
claude mcp add --transport http orbit http://127.0.0.1:8787/mcp --header "Authorization: Bearer $ORBIT_TOKEN"
```

Tools:

- **`orbit_recall`:** searches what was said, by you and assistants, with when and where.
- **`orbit_exact`:** returns a command, path, URL, version or code block byte for byte, or `not_found`.

Both read only. REST equivalents: `POST /v1/recall`, `POST /v1/recall/exact`.

## Test

Unit tests need nothing:

```bash
make test
```

Database tests need `make db` and a `.env`:

```bash
make test-db
```

`make check` runs formatting, lints and unit tests, as CI does.
