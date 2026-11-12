//! Configuration, from environment variables.

use anyhow::Context;
use orbit_embed::Space;
use std::env;
use uuid::Uuid;

pub struct Config {
    pub database_url: String,
    /// The one brain this deployment holds.
    pub tenant_id: Uuid,
    /// Bearer token for every client (Phase 0; OAuth replaces it).
    pub token: String,
    pub bind: String,
    /// Host names MCP requests may be addressed to (DNS-rebinding protection).
    pub allowed_hosts: Vec<String>,
    /// The dense channel; `None` without an embedding provider key.
    pub embedding: Option<Embedding>,
}

/// Which embedding space to build and search, and the key for its provider.
pub struct Embedding {
    pub openrouter_key: String,
    pub space: Space,
}

const DEFAULT_EMBED_MODEL: &str = "qwen/qwen3-embedding-8b";
const DEFAULT_EMBED_DIMS: usize = 4096;

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let token = env::var("ORBIT_TOKEN").context("ORBIT_TOKEN is not set")?;
        anyhow::ensure!(token.len() >= 32, "ORBIT_TOKEN must be at least 32 characters");
        Ok(Self {
            database_url: env::var("DATABASE_URL").context("DATABASE_URL is not set")?,
            tenant_id: env::var("ORBIT_TENANT_ID")
                .context("ORBIT_TENANT_ID is not set")?
                .parse()
                .context("ORBIT_TENANT_ID is not a UUID")?,
            token,
            bind: env::var("ORBIT_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_string()),
            allowed_hosts: env::var("ORBIT_ALLOWED_HOSTS")
                .unwrap_or_else(|_| "localhost,127.0.0.1,::1".to_string())
                .split(',')
                .map(|h| h.trim().to_string())
                .filter(|h| !h.is_empty())
                .collect(),
            embedding: embedding()?,
        })
    }
}

/// `OPENROUTER_API_KEY` turns the dense channel on. `ORBIT_EMBED_MODEL` and `ORBIT_EMBED_DIMS` pick
/// the space; changing either builds a new space in the background (see orbit-index `dense`).
fn embedding() -> anyhow::Result<Option<Embedding>> {
    let Some(key) = env::var("OPENROUTER_API_KEY").ok().filter(|k| !k.trim().is_empty()) else {
        return Ok(None);
    };
    let model = env::var("ORBIT_EMBED_MODEL").unwrap_or_else(|_| DEFAULT_EMBED_MODEL.to_string());
    let dims = match env::var("ORBIT_EMBED_DIMS") {
        Ok(d) => d.parse().context("ORBIT_EMBED_DIMS is not a number")?,
        Err(_) => DEFAULT_EMBED_DIMS,
    };
    anyhow::ensure!(
        (1..=16_000).contains(&dims),
        "ORBIT_EMBED_DIMS must be between 1 and 16000"
    );
    Ok(Some(Embedding {
        openrouter_key: key.trim().to_string(),
        space: Space::hosted(&model, dims),
    }))
}
