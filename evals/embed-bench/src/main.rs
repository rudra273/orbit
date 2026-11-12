//! Embedding-model benchmark on the owner's own history.
//!
//! Two tasks with ground truth taken from the data itself, so no model writes the answers:
//! - **title → session**: a session's title as the query; any turn from that session counts.
//! - **question → answer**: a question the user asked; the assistant's next reply is the answer.
//!
//! Compared: PostgreSQL full-text (today's Orbit), BM25, each embedding model, and BM25 + model
//! fused by reciprocal rank. Read-only against the database.

mod data;
mod dense;
mod lexical;
mod metrics;

use clap::Parser;
use std::time::Instant;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    #[arg(long, env = "ORBIT_TENANT_ID")]
    tenant_id: uuid::Uuid,
    /// Documents in the corpus: every answer document, plus sampled distractors.
    #[arg(long, default_value_t = 6000)]
    docs: usize,
    /// Question → answer queries to sample.
    #[arg(long, default_value_t = 300)]
    questions: usize,
    /// Models to run, comma-separated (see `dense::MODELS`).
    #[arg(long, value_delimiter = ',', default_value = "minilm,nomic-v1.5,gemma-300m,bge-m3,qwen3-0.6b")]
    models: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let pool = sqlx::PgPool::connect(&args.database_url).await?;
    let bench = data::load(&pool, args.tenant_id, args.docs, args.questions).await?;
    println!(
        "corpus {} docs · {} title queries · {} question queries",
        bench.docs.len(),
        bench.tasks[0].queries.len(),
        bench.tasks[1].queries.len()
    );

    let mut rankings: Vec<(String, Vec<metrics::Ranked>, Option<f64>)> = Vec::new();
    rankings.push(("postgres-fts (current)".into(), lexical::postgres(&pool, args.tenant_id, &bench).await?, None));
    let bm25 = lexical::bm25(&bench);
    rankings.push(("bm25".into(), bm25.clone(), None));

    for name in args.models.iter().filter(|m| !m.is_empty()) {
        let spec = dense::spec(name)?;
        let start = Instant::now();
        let ranked = dense::rank(&spec, &bench)?;
        let docs_per_sec = bench.docs.len() as f64 / start.elapsed().as_secs_f64();
        rankings.push((format!("{name} ({}d)", ranked.dims), ranked.lists.clone(), Some(docs_per_sec)));
        rankings.push((format!("bm25 + {name}"), metrics::fuse(&bm25, &ranked.lists), None));
    }

    let [a_name, b_name] = [bench.tasks[0].name, bench.tasks[1].name];
    println!("\n| method | {a_name} hit@5 | MRR | {b_name} hit@5 | hit@10 | MRR | docs/s |");
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for (name, ranked, speed) in &rankings {
        let a = metrics::score(&bench.tasks[0], &ranked[0]);
        let b = metrics::score(&bench.tasks[1], &ranked[1]);
        let speed = speed.map_or("–".to_string(), |s| format!("{s:.0}"));
        println!(
            "| {name} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {speed} |",
            a.hit5, a.mrr, b.hit5, b.hit10, b.mrr
        );
    }
    Ok(())
}
