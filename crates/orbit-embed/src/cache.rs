//! Remembering recent query vectors, so a repeated question costs no provider call.

use crate::{EmbedFuture, Embedder, Role, Space};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

type Key = (String, String);

/// Vectors by (space, query), and the order they arrived in, oldest first.
#[derive(Default)]
struct Entries {
    vectors: HashMap<Key, Vec<f32>>,
    order: VecDeque<Key>,
}

/// Wraps an embedder and keeps the vectors of the last `capacity` distinct queries, in memory
/// only. Documents always go to the inner embedder.
pub struct QueryCache {
    inner: Arc<dyn Embedder>,
    capacity: usize,
    entries: Mutex<Entries>,
}

impl QueryCache {
    pub fn new(inner: Arc<dyn Embedder>, capacity: usize) -> Self {
        Self {
            inner,
            capacity,
            entries: Mutex::default(),
        }
    }

    fn get(&self, key: &Key) -> Option<Vec<f32>> {
        self.entries.lock().expect("cache lock").vectors.get(key).cloned()
    }

    fn put(&self, key: Key, vector: Vec<f32>) {
        let mut entries = self.entries.lock().expect("cache lock");
        if entries.vectors.insert(key.clone(), vector).is_none() {
            entries.order.push_back(key);
            while entries.order.len() > self.capacity {
                if let Some(old) = entries.order.pop_front() {
                    entries.vectors.remove(&old);
                }
            }
        }
    }
}

impl Embedder for QueryCache {
    fn embed<'a>(&'a self, space: &'a Space, role: Role, texts: &'a [String]) -> EmbedFuture<'a> {
        Box::pin(async move {
            if role != Role::Query || texts.len() != 1 {
                return self.inner.embed(space, role, texts).await;
            }
            let key = (space.id.clone(), texts[0].clone());
            if let Some(vector) = self.get(&key) {
                return Ok(vec![vector]);
            }
            let vectors = self.inner.embed(space, role, texts).await?;
            if let [vector] = vectors.as_slice() {
                self.put(key, vector.clone());
            }
            Ok(vectors)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::WordHash;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(AtomicUsize);

    impl Embedder for Counting {
        fn embed<'a>(&'a self, space: &'a Space, role: Role, texts: &'a [String]) -> EmbedFuture<'a> {
            self.0.fetch_add(1, Ordering::SeqCst);
            WordHash.embed(space, role, texts)
        }
    }

    #[tokio::test]
    async fn repeated_queries_skip_the_provider_and_old_ones_are_forgotten() {
        let inner = Arc::new(Counting(AtomicUsize::new(0)));
        let cache = QueryCache::new(inner.clone(), 2);
        let space = Space::hosted("test", 16);
        let q = |s: &str| vec![s.to_string()];

        for text in ["a", "a", "b", "a", "c", "a"] {
            cache.embed(&space, Role::Query, &q(text)).await.unwrap();
        }
        // a (miss), a (hit), b (miss), a (hit), c (miss, forgets a), a (miss).
        assert_eq!(inner.0.load(Ordering::SeqCst), 4);

        cache.embed(&space, Role::Document, &q("a")).await.unwrap();
        assert_eq!(inner.0.load(Ordering::SeqCst), 5, "documents are never cached");
    }
}
