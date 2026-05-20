use ort::{inputs, session::builder::GraphOptimizationLevel, session::Session, value::TensorRef};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum PredictError {
    #[error("Failed to load vocab: {0}")]
    VocabLoad(String),
    #[error("Failed to load model: {0}")]
    ModelLoad(String),
    #[error("ORT error: {0}")]
    Ort(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

impl From<ort::Error> for PredictError {
    fn from(e: ort::Error) -> Self {
        PredictError::Ort(e.to_string())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Vocab {
    tokens: HashMap<String, i64>,
    ids: HashMap<i64, String>,
    bos_id: i64,
    eos_id: i64,
    unk_id: i64,
    pad_id: i64,
}

impl Vocab {
    pub fn load(path: &PathBuf) -> Result<Self, PredictError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| PredictError::VocabLoad(e.to_string()))?;

        let raw: HashMap<String, i64> =
            serde_json::from_str(&content).map_err(|e| PredictError::VocabLoad(e.to_string()))?;

        let ids: HashMap<i64, String> = raw.iter().map(|(k, v)| (*v, k.clone())).collect();

        let bos_id = raw.get("[BOS]").copied().unwrap_or(1);
        let eos_id = raw.get("[EOS]").copied().unwrap_or(2);
        let unk_id = raw.get("[UNK]").copied().unwrap_or(3);
        let pad_id = raw.get("[PAD]").copied().unwrap_or(0);

        Ok(Self {
            tokens: raw,
            ids,
            bos_id,
            eos_id,
            unk_id,
            pad_id,
        })
    }

    pub fn encode(&self, text: &str) -> Vec<i64> {
        let mut ids = vec![self.bos_id];
        for ch in text.chars() {
            ids.push(
                self.tokens
                    .get(&ch.to_string())
                    .copied()
                    .unwrap_or(self.unk_id),
            );
        }
        ids
    }

    pub fn decode(&self, id: i64) -> Option<&str> {
        self.ids.get(&id).map(|s| s.as_str())
    }

    pub fn is_special(&self, id: i64) -> bool {
        id == self.pad_id || id == self.bos_id || id == self.eos_id || id == self.unk_id
    }

    pub fn vocab_size(&self) -> usize {
        self.tokens.len()
    }
}

pub struct Predictor {
    vocab: Vocab,
    session: Session,
    model_dir: PathBuf,
    model_name: String,
}

impl Predictor {
    pub fn new(model_name: Option<&str>) -> Result<Self, PredictError> {
        let name = model_name.unwrap_or("predictive-text-small");
        let model_dir = get_model_dir(Some(name));

        let vocab_path = model_dir.join("vocab.json");
        let model_path = model_dir.join("model.onnx");

        if !model_path.exists() {
            return Err(PredictError::ModelNotFound(
                model_path.display().to_string(),
            ));
        }

        let vocab = Vocab::load(&vocab_path)?;

        info!("Loading ONNX model from {}", model_path.display());

        let session = Session::builder()
            .map_err(|e| PredictError::ModelLoad(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| PredictError::ModelLoad(e.to_string()))?
            .with_intra_threads(1)
            .map_err(|e| PredictError::ModelLoad(e.to_string()))?
            .commit_from_file(model_path)?;

        info!("Model loaded successfully");

        Ok(Self {
            vocab,
            session,
            model_dir,
            model_name: name.to_string(),
        })
    }

    pub fn predict(
        &mut self,
        prefix: &str,
        top_k: usize,
    ) -> Result<Vec<(String, f32)>, PredictError> {
        let tokens = self.vocab.encode(prefix);

        // Create input tensor following demo pattern
        let input =
            TensorRef::from_array_view((vec![1i64, 1, tokens.len() as i64], tokens.as_slice()))?;

        // Run inference
        let outputs = self.session.run(inputs![input])?;

        // Extract logits - shape [B, _, S, V]
        let (dim, probabilities) = outputs[0].try_extract_tensor::<f32>()?;

        // Get vocab size and sequence length
        let seq_len = dim[2] as usize;
        let vocab_size = dim[3] as usize;

        // Get probabilities for last token position
        let last_token_probs = &probabilities[(seq_len - 1) * vocab_size..];

        // Sort by probability
        let mut candidates: Vec<(usize, f32)> =
            last_token_probs.iter().copied().enumerate().collect();

        candidates
            .sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Less));

        // Filter out special tokens and return top_k
        Ok(candidates
            .iter()
            .take(top_k)
            .filter_map(|(id, score)| {
                if self.vocab.is_special(*id as i64) {
                    return None;
                }
                self.vocab
                    .decode(*id as i64)
                    .map(|token| (token.to_string(), *score))
            })
            .collect())
    }

    pub fn model_dir(&self) -> &PathBuf {
        &self.model_dir
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab.vocab_size()
    }
}

pub fn get_model_dir(model_name: Option<&str>) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let base = PathBuf::from(home).join(".config/xime/models");
    match model_name {
        Some(name) => base.join(name),
        None => base.join("predictive-text-small"),
    }
}

pub fn check_model_exists(model_name: Option<&str>) -> bool {
    let model_dir = get_model_dir(model_name);
    model_dir.join("vocab.json").exists()
        && model_dir.join("model.onnx").exists()
        && model_dir.join("model.onnx.data").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vocab_load() {
        let vocab_path = PathBuf::from("test_vocab.json");
        let test_vocab = r#"{"[PAD]":0,"[BOS]":1,"[EOS]":2,"[UNK]":3,"你":4,"好":5,"世":6,"界":7}"#;
        std::fs::write(&vocab_path, test_vocab).unwrap();

        let vocab = Vocab::load(&vocab_path).unwrap();
        assert_eq!(vocab.encode("你好"), vec![1, 4, 5]);
        assert_eq!(vocab.decode(4), Some("你"));
        assert!(vocab.is_special(0));
        assert!(!vocab.is_special(4));

        std::fs::remove_file(&vocab_path).ok();
    }
}
