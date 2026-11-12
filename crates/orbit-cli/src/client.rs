//! HTTP client for the Orbit server's ingest endpoint.

use orbit_ledger::{IncomingEvent, SourceSpec};
use serde::{Deserialize, Serialize};

/// Requests are kept well under the server's body limit.
const MAX_BATCH_BYTES: usize = 8 * 1024 * 1024;

pub struct Client {
    http: reqwest::Client,
    url: String,
    token: String,
}

#[derive(Serialize)]
struct Batch<'a> {
    source: &'a SourceSpec,
    events: &'a [IncomingEvent],
}

#[derive(Deserialize)]
struct Appended {
    duplicate: bool,
}

/// How many events were new, and how many the server already had.
#[derive(Default, Clone, Copy)]
pub struct Sent {
    pub new: usize,
    pub duplicate: usize,
}

impl Client {
    pub fn new(url: String, token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: url.trim_end_matches('/').to_string(),
            token,
        }
    }

    /// Sends one source's events, split into batches by size.
    pub async fn send(&self, source: &SourceSpec, events: &[IncomingEvent]) -> anyhow::Result<Sent> {
        let mut sent = Sent::default();
        let mut start = 0;
        while start < events.len() {
            let mut end = start;
            let mut bytes = 0;
            while end < events.len() && (end == start || bytes + events[end].content.len() < MAX_BATCH_BYTES) {
                bytes += events[end].content.len();
                end += 1;
            }
            let response = self
                .http
                .post(format!("{}/v1/ingest/events", self.url))
                .bearer_auth(&self.token)
                .json(&Batch {
                    source,
                    events: &events[start..end],
                })
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!(
                    "server answered {status}: {}",
                    response.text().await.unwrap_or_default()
                );
            }
            for a in response.json::<Vec<Appended>>().await? {
                if a.duplicate {
                    sent.duplicate += 1
                } else {
                    sent.new += 1
                }
            }
            start = end;
        }
        Ok(sent)
    }
}
