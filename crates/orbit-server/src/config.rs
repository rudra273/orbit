//! Configuration, from environment variables.

use anyhow::Context;
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
}

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
        })
    }
}
