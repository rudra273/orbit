//! Checks the Rust embedder against vectors from Google's reference implementation.

use crate::gemma2::{Embedder, cosine};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Reference {
    texts: Vec<String>,
    reference: Vec<Vec<f32>>,
}

/// Embeds the reference texts one at a time and all together (padding must not change a vector).
pub fn run(dir: &Path, reference: &Path) -> anyhow::Result<()> {
    let r: Reference = serde_json::from_reader(std::fs::File::open(reference)?)?;
    for file in ["model.onnx", "model_quantized.onnx"] {
        let mut model = Embedder::load(dir, file, 2048)?;
        let batched = model.embed(&r.texts, r.texts.len(), |_| {})?;
        let single: Vec<Vec<f32>> = r
            .texts
            .iter()
            .map(|t| model.embed(std::slice::from_ref(t), 1, |_| {}).map(|mut v| v.remove(0)))
            .collect::<anyhow::Result<_>>()?;
        let min = |vs: &[Vec<f32>]| {
            vs.iter()
                .zip(&r.reference)
                .map(|(a, b)| cosine(a, b))
                .fold(f32::MAX, f32::min)
        };
        let pad = batched
            .iter()
            .zip(&single)
            .map(|(a, b)| cosine(a, b))
            .fold(f32::MAX, f32::min);
        println!(
            "{file:22} vs reference: one-by-one min cosine {:.6}, batched min cosine {:.6}; batched vs one-by-one {:.6}",
            min(&single),
            min(&batched),
            pad
        );
    }
    Ok(())
}
