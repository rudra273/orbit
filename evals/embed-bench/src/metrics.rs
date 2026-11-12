//! Ranking metrics and reciprocal-rank fusion.

use crate::data::Task;
use std::collections::HashMap;
use uuid::Uuid;

/// Ranked document ids per query, best first.
pub type Ranked = Vec<Vec<Uuid>>;

pub struct Scores {
    pub hit5: f64,
    pub hit10: f64,
    /// Mean reciprocal rank of the first relevant document, over the top 50.
    pub mrr: f64,
}

pub fn score(task: &Task, ranked: &Ranked) -> Scores {
    let (mut hit5, mut hit10, mut mrr) = (0.0, 0.0, 0.0);
    for (q, list) in task.queries.iter().zip(ranked) {
        let first = list.iter().filter(|id| Some(**id) != q.exclude).take(50).position(|id| q.relevant.contains(id));
        if let Some(rank) = first {
            hit5 += f64::from(rank < 5);
            hit10 += f64::from(rank < 10);
            mrr += 1.0 / (rank as f64 + 1.0);
        }
    }
    let n = task.queries.len().max(1) as f64;
    Scores { hit5: hit5 / n, hit10: hit10 / n, mrr: mrr / n }
}

/// Reciprocal-rank fusion (k = 60) of two rankings per task.
pub fn fuse(a: &[Ranked], b: &[Ranked]) -> Vec<Ranked> {
    a.iter()
        .zip(b)
        .map(|(ta, tb)| {
            ta.iter()
                .zip(tb)
                .map(|(la, lb)| {
                    let mut score: HashMap<Uuid, f64> = HashMap::new();
                    for list in [la, lb] {
                        for (rank, id) in list.iter().enumerate() {
                            *score.entry(*id).or_default() += 1.0 / (60.0 + rank as f64 + 1.0);
                        }
                    }
                    let mut fused: Vec<(Uuid, f64)> = score.into_iter().collect();
                    fused.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(&y.0)));
                    fused.into_iter().take(50).map(|(id, _)| id).collect()
                })
                .collect()
        })
        .collect()
}
