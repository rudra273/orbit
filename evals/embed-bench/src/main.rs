//! Embedding-model benchmark on the owner's own history.
//!
//! Two tasks with ground truth taken from the data itself, so no model writes the answers:
//! - **title → session**: a session's title as the query; any turn from that session counts.
//! - **question → answer**: a question the user asked; the assistant's next reply is the answer.
//!
//! Compared: PostgreSQL full-text (today's Orbit), BM25, each embedding model, and BM25 + model
//! fused by reciprocal rank. Scores carry 95% bootstrap intervals. Read-only against the database.

mod api;
mod data;
mod dense;
mod gemma2;
mod lexical;
mod metrics;
mod validate;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check the ONNX embedder against reference vectors from Google's implementation.
    Validate {
        #[arg(long)]
        gemma_dir: PathBuf,
        #[arg(long)]
        reference: PathBuf,
    },
    /// Run the retrieval benchmark.
    Run {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
        #[arg(long, env = "ORBIT_TENANT_ID")]
        tenant_id: uuid::Uuid,
        #[arg(long)]
        gemma_dir: Option<PathBuf>,
        /// EmbeddingGemma 2 ONNX files to test, comma-separated (e.g. model_quantized.onnx).
        #[arg(long, value_delimiter = ',')]
        files: Vec<String>,
        /// OpenRouter embedding models to test, comma-separated (needs OPENROUTER_API_KEY).
        #[arg(long, value_delimiter = ',')]
        api_models: Vec<String>,
        /// Vector sizes to score API models at (Matryoshka truncation), comma-separated; default: full size.
        #[arg(long, value_delimiter = ',')]
        dims: Vec<usize>,
        /// Lexical weights to fuse with each API run (dense weight 1), comma-separated.
        #[arg(long, value_delimiter = ',')]
        fusion_weights: Vec<f64>,
        #[arg(long, default_value_t = 6000)]
        docs: usize,
        #[arg(long, default_value_t = 300)]
        questions: usize,
        #[arg(long, default_value_t = 512)]
        max_tokens: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Args::parse().command {
        Command::Validate { gemma_dir, reference } => validate::run(&gemma_dir, &reference),
        Command::Run {
            database_url,
            tenant_id,
            gemma_dir,
            files,
            api_models,
            dims,
            fusion_weights,
            docs,
            questions,
            max_tokens,
        } => {
            let pool = sqlx::PgPool::connect(&database_url).await?;
            let bench = data::load(&pool, tenant_id, docs, questions).await?;
            println!(
                "corpus {} docs · {} title queries · {} question queries\n",
                bench.docs.len(),
                bench.tasks[0].queries.len(),
                bench.tasks[1].queries.len()
            );
            metrics::header(&bench);
            let fts = lexical::postgres(&pool, tenant_id, &bench).await?;
            metrics::row(&bench, "postgres-fts (current Orbit)", &fts, None);
            let bm25 = lexical::bm25(&bench);
            metrics::row(&bench, "bm25", &bm25, None);
            for file in files.iter().filter(|f| !f.is_empty()) {
                let dir = gemma_dir
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("--gemma-dir is needed with --files"))?;
                let run = dense::gemma2(dir, file, max_tokens, &bench)?;
                let name = format!("gemma2 {file} ({}d)", run.dims);
                metrics::row(&bench, &name, &run.lists, Some(run.docs_per_sec));
                metrics::row(
                    &bench,
                    &format!("bm25 + gemma2 {file}"),
                    &metrics::fuse(&bm25, &run.lists),
                    None,
                );
            }
            for model in api_models.iter().filter(|m| !m.is_empty()) {
                for (name, run) in dense::api(model, &dims, &bench).await? {
                    metrics::row(&bench, &name, &run.lists, Some(run.docs_per_sec));
                    for w in fusion_weights.iter().copied().filter(|_| !name.contains("1-bit")) {
                        let fused = metrics::fuse_weighted(&run.lists, 1.0, &bm25, w);
                        metrics::row(&bench, &format!("  + bm25 ×{w}"), &fused, None);
                        let fused = metrics::fuse_weighted(&run.lists, 1.0, &fts, w);
                        metrics::row(&bench, &format!("  + postgres-fts ×{w}"), &fused, None);
                    }
                }
            }
            Ok(())
        }
    }
}
