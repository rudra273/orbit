//! Ranking metrics, bootstrap intervals and reciprocal-rank fusion.

use crate::data::{Bench, Task};
use std::collections::HashMap;
use uuid::Uuid;

/// Ranked document ids per query, best first.
pub type Ranked = Vec<Vec<Uuid>>;

const RESAMPLES: usize = 2000;

/// Per query: the rank of the first relevant document within the top 50, if any.
fn first_relevant(task: &Task, ranked: &Ranked) -> Vec<Option<usize>> {
    task.queries
        .iter()
        .zip(ranked)
        .map(|(q, list)| {
            list.iter()
                .filter(|id| Some(**id) != q.exclude)
                .take(50)
                .position(|id| q.relevant.contains(id))
        })
        .collect()
}

fn hit(rank: Option<usize>, k: usize) -> f64 {
    f64::from(rank.is_some_and(|r| r < k))
}

fn rr(rank: Option<usize>) -> f64 {
    rank.map_or(0.0, |r| 1.0 / (r as f64 + 1.0))
}

/// Mean, with a 95% percentile-bootstrap interval over queries (fixed seed).
fn mean_ci(values: &[f64]) -> (f64, f64, f64) {
    let n = values.len().max(1);
    let mean = values.iter().sum::<f64>() / n as f64;
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize % n
    };
    let mut means: Vec<f64> = (0..RESAMPLES)
        .map(|_| (0..n).map(|_| values[next()]).sum::<f64>() / n as f64)
        .collect();
    means.sort_by(f64::total_cmp);
    (mean, means[RESAMPLES * 25 / 1000], means[RESAMPLES * 975 / 1000])
}

pub fn header(bench: &Bench) {
    let [a, b] = [bench.tasks[0].name, bench.tasks[1].name];
    println!("| method | {a} hit@5 [95% CI] | {a} MRR | {b} hit@5 [95% CI] | {b} hit@10 | {b} MRR | docs/s |");
    println!("|---|---|---:|---|---:|---:|---:|");
}

pub fn row(bench: &Bench, name: &str, ranked: &[Ranked], docs_per_sec: Option<f64>) {
    let cell = |task: &Task, ranked: &Ranked| {
        let ranks = first_relevant(task, ranked);
        let h5 = mean_ci(&ranks.iter().map(|r| hit(*r, 5)).collect::<Vec<_>>());
        let h10 = ranks.iter().map(|r| hit(*r, 10)).sum::<f64>() / ranks.len().max(1) as f64;
        let mrr = ranks.iter().map(|r| rr(*r)).sum::<f64>() / ranks.len().max(1) as f64;
        (h5, h10, mrr)
    };
    let (a5, _, amrr) = cell(&bench.tasks[0], &ranked[0]);
    let (b5, b10, bmrr) = cell(&bench.tasks[1], &ranked[1]);
    let speed = docs_per_sec.map_or("–".to_string(), |s| format!("{s:.1}"));
    println!(
        "| {name} | {:.3} [{:.3}–{:.3}] | {amrr:.3} | {:.3} [{:.3}–{:.3}] | {b10:.3} | {bmrr:.3} | {speed} |",
        a5.0, a5.1, a5.2, b5.0, b5.1, b5.2
    );
}

/// Reciprocal-rank fusion (k = 60) of two rankings per task, equal weights.
pub fn fuse(a: &[Ranked], b: &[Ranked]) -> Vec<Ranked> {
    fuse_weighted(a, 1.0, b, 1.0)
}

/// Reciprocal-rank fusion (k = 60) with a weight per ranking.
pub fn fuse_weighted(a: &[Ranked], wa: f64, b: &[Ranked], wb: f64) -> Vec<Ranked> {
    a.iter()
        .zip(b)
        .map(|(ta, tb)| {
            ta.iter()
                .zip(tb)
                .map(|(la, lb)| {
                    let mut score: HashMap<Uuid, f64> = HashMap::new();
                    for (list, w) in [(la, wa), (lb, wb)] {
                        for (rank, id) in list.iter().enumerate() {
                            *score.entry(*id).or_default() += w / (60.0 + rank as f64 + 1.0);
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
