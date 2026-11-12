# embed-bench

Compares embedding models on the owner's own history, with ground truth taken from the data
itself, so no model writes the answers:

- **title → session:** a session's title as the query; any turn of that session counts.
- **question → answer:** a question the user asked; the assistant's next reply is the answer.

Each score is hit@5 (the answer is in the first five results) with a 95% bootstrap interval.
It reads a database the history was imported into and never writes to it. API vectors are cached
in `target/embed-bench/`; they are derived from the history, so treat them like it.

```bash
cargo run --release -p embed-bench -- run --api-models qwen/qwen3-embedding-8b --dims 4096,1024 --fusion-weights 0.1
```

## Results (2026-11-12, 6,000 turns, 136 title and 300 question queries)

| Method | Title → session | Question → answer |
|---|---|---|
| PostgreSQL full-text (Orbit before the dense channel) | 0.35 [0.27–0.43] | 0.13 [0.09–0.16] |
| BM25 | 0.60 [0.52–0.68] | 0.40 [0.35–0.46] |
| EmbeddingGemma 2, local, 768d | 0.75 [0.68–0.82] | 0.52 [0.47–0.58] |
| **Qwen3-Embedding-8B, 4096d** | **0.77 [0.70–0.84]** | **0.66 [0.61–0.71]** |
| Qwen3-Embedding-8B, 2048d | 0.77 | 0.65 |
| Qwen3-Embedding-8B, 1024d | 0.77 | 0.64 |
| Qwen3-Embedding-8B, 512d | 0.78 | 0.63 |
| Qwen3-Embedding-8B, 4096d, 1-bit scan + re-rank of 200 | 0.77 | 0.66 |
| Qwen3 4096d + full-text, fused at weight 0.1 | 0.79 | 0.67 |
| Qwen3 4096d + full-text, fused at weight 0.5 | 0.72 | 0.59 |
| Qwen3 4096d + BM25, fused at equal weight | 0.70 | 0.56 |

Decisions taken from it: Qwen3-Embedding-8B at the full 4096 dimensions; a 1-bit scan with
re-ranking, which lost nothing; full-text fused at weight 0.1, since heavier weights hurt.

Not yet measured: queries for code and exact identifiers (covered by `orbit_exact` meanwhile),
and a fair scoring that credits any turn that answers a question, not only the next reply.
