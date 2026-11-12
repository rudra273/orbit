//! EmbeddingGemma 2 (text only) on ONNX Runtime, from the onnx-community export.
//!
//! The graph takes image, video and audio features as well as text, so empty feature
//! tensors are passed. Its `sentence_embedding` output is already mean-pooled and
//! projected; vectors are L2-normalised here, as Google's reference code does.

use anyhow::{Context, anyhow};
use ndarray::Array2;
use ort::{
    ep,
    session::{
        Session,
        builder::{AutoDevicePolicy, GraphOptimizationLevel},
    },
    value::Tensor,
};
use std::{path::Path, sync::OnceLock};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

/// Retrieval prompts from the model card.
pub const QUERY_PREFIX: &str = "task: search result | query: ";
pub const DOC_PREFIX: &str = "title: none | text: ";
const FEATURE_WIDTH: usize = 512;

/// ONNX Runtime allows one environment per process; every session shares it.
fn environment() -> anyhow::Result<&'static ort::environment::Environment> {
    static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();
    if let Some(env) = ENV.get() {
        return Ok(env);
    }
    let env = ort::init().build().map_err(|e| anyhow!("onnx runtime: {e}"))?;
    Ok(ENV.get_or_init(|| env))
}

pub struct Embedder {
    session: Session,
    tokenizer: Tokenizer,
}

impl Embedder {
    /// `file` is `model.onnx` (float32) or `model_quantized.onnx` (8-bit).
    pub fn load(dir: &Path, file: &str, max_tokens: usize) -> anyhow::Result<Self> {
        let environment = environment()?;
        let threads = std::thread::available_parallelism()?.get();
        // CPU only: on macOS the default policy adds CoreML, which rejects the empty feature tensors.
        let session = Session::builder(environment)?
            .with_auto_device(AutoDevicePolicy::PreferCPU)
            .map_err(|e| anyhow!("{e}"))?
            .with_execution_providers([ep::CPU::default().build()])
            .map_err(|e| anyhow!("{e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("{e}"))?
            .with_intra_threads(threads)
            .map_err(|e| anyhow!("{e}"))?
            .commit_from_file(dir.join("onnx").join(file))
            .with_context(|| format!("loading {file}"))?;
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| anyhow!("tokenizer: {e}"))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: max_tokens,
                ..Default::default()
            }))
            .map_err(|e| anyhow!("{e}"))?;
        tokenizer.with_padding(Some(PaddingParams {
            pad_id: 0,
            pad_token: "<pad>".into(),
            ..Default::default()
        }));
        Ok(Self { session, tokenizer })
    }

    /// Embeds texts in batches of similar length (less padding), returning vectors in input order.
    pub fn embed(
        &mut self,
        texts: &[String],
        batch: usize,
        mut progress: impl FnMut(usize),
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut order: Vec<usize> = (0..texts.len()).collect();
        order.sort_by_key(|&i| texts[i].len());
        let mut out = vec![Vec::new(); texts.len()];
        let mut done = 0;
        for chunk in order.chunks(batch.max(1)) {
            let inputs: Vec<&str> = chunk.iter().map(|&i| texts[i].as_str()).collect();
            for (slot, v) in chunk.iter().zip(self.run(&inputs)?) {
                out[*slot] = v;
            }
            done += chunk.len();
            progress(done);
        }
        Ok(out)
    }

    fn run(&mut self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| anyhow!("{e}"))?;
        let (rows, cols) = (encodings.len(), encodings[0].len());
        let ids = Array2::from_shape_fn((rows, cols), |(r, c)| i64::from(encodings[r].get_ids()[c]));
        let mask = Array2::from_shape_fn((rows, cols), |(r, c)| i64::from(encodings[r].get_attention_mask()[c]));
        let empty = || Tensor::from_array(Array2::<f32>::zeros((0, FEATURE_WIDTH)));
        let outputs = self.session.run(ort::inputs![
            "input_ids" => Tensor::from_array(ids)?,
            "attention_mask" => Tensor::from_array(mask)?,
            "image_features" => empty()?,
            "video_features" => empty()?,
            "audio_features" => empty()?,
        ])?;
        let pooled = outputs["sentence_embedding"].try_extract_array::<f32>()?;
        Ok(pooled
            .outer_iter()
            .map(|row| normalise(row.iter().copied().collect()))
            .collect())
    }
}

pub fn normalise(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
