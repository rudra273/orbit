# Orbit

A persistent memory that belongs to one person and is shared by every AI they use.
One brain per deployment, running in its owner's cloud.

The design is in [docs/](docs/README.md). This repository is at **Phase 0**: the base only.

## What exists

| Path | What it is |
|---|---|
| `crates/orbit-core` | Shared types: typed ids, the labels every memory carries, byte anchors, the ledger hash chain |
| `crates/orbit-ledger` | Admission (validation, secret redaction, labels) and the append-only evidence ledger |
| `crates/orbit-recall` | The retrieval interface. No engine yet |
| `crates/orbit-server` | The engine: `GET /healthz` and `POST /v1/ingest/events`, guarded by one bearer token |
| `migrations/` | PostgreSQL schema, applied at start-up. Currently the ledger and the change feed |
| `workers/` | Python workers. Currently one stage: grammar-based literal detection |
| `evals/` | Placeholder for the evaluation harness |

Not built yet: extraction, claims and beliefs, retrieval, MCP, OAuth, any UI.

## Run it

Needs Rust, Docker and Python 3.9+.

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

Store a piece of evidence:

```bash
curl -s localhost:8787/v1/ingest/events \
  -H "Authorization: Bearer $ORBIT_TOKEN" -H "Content-Type: application/json" \
  -d '{"source": {"kind": "conversation", "external_ref": "demo-1", "app": "curl"},
       "events": [{"seq": 0, "author_role": "user", "occurred_at": "2026-11-12T10:00:00Z",
                   "content": "I prefer Rust for the core Orbit engine", "idempotency_key": "demo-1:0",
                   "integrity": "user_direct"}]}'
```

Sending the same `idempotency_key` again returns the same id with `"duplicate": true`.

## Test

```bash
make test
```
