use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EmbedError {
    #[error("embedding provider failed: {0}")]
    Provider(String),
    #[error("embeddings mode `{mode}` is unavailable: {reason}")]
    Unavailable { mode: String, reason: String },
}

pub const EMBEDDINGS_MODE_ENV: &str = "AGENTLOOP_EMBEDDINGS";
const EMBEDDINGS_URL_ENV: &str = "AGENTLOOP_EMBEDDINGS_URL";
const EMBEDDINGS_API_KEY_ENV: &str = "AGENTLOOP_EMBEDDINGS_API_KEY";
const EMBEDDINGS_MODEL_ENV: &str = "AGENTLOOP_EMBEDDINGS_MODEL";
const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";

const DEFAULT_REMOTE_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_REMOTE_MODEL: &str = "text-embedding-3-small";
const DEFAULT_REMOTE_DIM: usize = 1536;

pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError>;

    fn dim(&self) -> usize;

    fn id(&self) -> &str;
}

pub fn resolve_embedder(
    index_dir: &Path,
) -> Result<Option<Arc<dyn EmbeddingProvider>>, EmbedError> {
    let mode = std::env::var(EMBEDDINGS_MODE_ENV)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match mode.as_str() {
        "" | "off" | "none" | "bm25" => Ok(None),
        "remote" => Ok(Some(Arc::new(RemoteEmbedder::from_env()?))),
        "local" => resolve_local_embedder(index_dir),
        other => Err(EmbedError::Unavailable {
            mode: other.to_owned(),
            reason: "expected off|local|remote".to_owned(),
        }),
    }
}

fn resolve_local_embedder(
    index_dir: &Path,
) -> Result<Option<Arc<dyn EmbeddingProvider>>, EmbedError> {
    #[cfg(feature = "local-embeddings")]
    {
        let cache = index_dir.join("models");
        let provider = FastembedProvider::open(&cache)?;
        Ok(Some(Arc::new(provider)))
    }
    #[cfg(not(feature = "local-embeddings"))]
    {
        let _ = index_dir;
        tracing::warn!(
            "AGENTLOOP_EMBEDDINGS=local set, but the `local-embeddings` cargo \
             feature is not compiled in; degrading to BM25-only retrieval"
        );
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct MockEmbedder {
    dim: usize,
    id: String,
    overrides: Vec<(String, Vec<f32>)>,
}

impl MockEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            id: "mock".to_owned(),
            overrides: Vec::new(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_override(mut self, needle: impl Into<String>, vector: Vec<f32>) -> Self {
        debug_assert_eq!(vector.len(), self.dim, "override vector must match dim");
        self.overrides.push((needle.into(), vector));
        self
    }

    fn embed_one(&self, text: &str) -> Vec<f32> {
        for (needle, vector) in &self.overrides {
            if text.contains(needle.as_str()) {
                return vector.clone();
            }
        }
        hash_vector(text, self.dim)
    }
}

fn hash_vector(text: &str, dim: usize) -> Vec<f32> {
    let digest = blake3::hash(text.as_bytes());
    let bytes = digest.as_bytes();
    let mut state = u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    let mut out = Vec::with_capacity(dim);
    for _ in 0..dim {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let scaled = ((state >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0;
        out.push(scaled);
    }
    normalize(&mut out);
    out
}

fn normalize(vector: &mut [f32]) {
    let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for value in vector.iter_mut() {
            *value /= norm;
        }
    }
}

impl EmbeddingProvider for MockEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbedderKey {
    pub id: String,
    pub dim: usize,
}

impl EmbedderKey {
    pub fn of(provider: &dyn EmbeddingProvider) -> Self {
        Self {
            id: provider.id().to_owned(),
            dim: provider.dim(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoteEmbedder {
    base_url: String,
    api_key: String,
    model: String,
    id: String,
    dim: usize,
}

impl RemoteEmbedder {
    pub fn from_env() -> Result<Self, EmbedError> {
        let api_key = std::env::var(EMBEDDINGS_API_KEY_ENV)
            .or_else(|_| std::env::var(OPENAI_API_KEY_ENV))
            .map_err(|_| EmbedError::Unavailable {
                mode: "remote".to_owned(),
                reason: format!(
                    "set {EMBEDDINGS_API_KEY_ENV} or {OPENAI_API_KEY_ENV} for remote embeddings"
                ),
            })?;
        if api_key.trim().is_empty() {
            return Err(EmbedError::Unavailable {
                mode: "remote".to_owned(),
                reason: "API key is empty".to_owned(),
            });
        }
        let base_url = std::env::var(EMBEDDINGS_URL_ENV)
            .unwrap_or_else(|_| DEFAULT_REMOTE_BASE_URL.to_owned())
            .trim_end_matches('/')
            .to_owned();
        let model =
            std::env::var(EMBEDDINGS_MODEL_ENV).unwrap_or_else(|_| DEFAULT_REMOTE_MODEL.to_owned());
        Ok(Self::new(base_url, api_key, model, DEFAULT_REMOTE_DIM))
    }

    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        dim: usize,
    ) -> Self {
        let model = model.into();
        let base_url = base_url.into();
        let id = format!("remote/{model}");
        Self {
            base_url,
            api_key: api_key.into(),
            model,
            id,
            dim,
        }
    }
}

#[derive(Serialize)]
struct RemoteEmbedRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
}

#[derive(Deserialize)]
struct RemoteEmbedResponse {
    data: Vec<RemoteEmbedDatum>,
}

#[derive(Deserialize)]
struct RemoteEmbedDatum {
    embedding: Vec<f32>,
    index: usize,
}

impl EmbeddingProvider for RemoteEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/embeddings", self.base_url);
        let body = RemoteEmbedRequest {
            model: &self.model,
            input: texts,
        };
        let response = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|err| EmbedError::Provider(format!("remote embed HTTP failed: {err}")))?;
        let parsed: RemoteEmbedResponse = response
            .into_json()
            .map_err(|err| EmbedError::Provider(format!("remote embed decode failed: {err}")))?;
        if parsed.data.len() != texts.len() {
            return Err(EmbedError::Provider(format!(
                "remote embed returned {} vectors for {} inputs",
                parsed.data.len(),
                texts.len()
            )));
        }
        let mut ordered = vec![Vec::new(); texts.len()];
        for datum in parsed.data {
            if datum.index >= ordered.len() {
                return Err(EmbedError::Provider(format!(
                    "remote embed index {} out of range for {} inputs",
                    datum.index,
                    texts.len()
                )));
            }
            if datum.embedding.len() != self.dim {
                return Err(EmbedError::Provider(format!(
                    "remote embed dim mismatch: got {}, expected {}",
                    datum.embedding.len(),
                    self.dim
                )));
            }
            ordered[datum.index] = datum.embedding;
        }
        if ordered.iter().any(Vec::is_empty) {
            return Err(EmbedError::Provider(
                "remote embed response missing one or more indices".to_owned(),
            ));
        }
        Ok(ordered)
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(feature = "local-embeddings")]
pub mod fastembed_provider {
    use std::path::Path;
    use std::sync::Mutex;

    use super::{EmbedError, EmbeddingProvider};

    const BGE_SMALL_DIM: usize = 384;
    const MODEL_ID: &str = "fastembed/bge-small-en-v1.5";

    pub struct FastembedProvider {
        model: Mutex<fastembed::TextEmbedding>,
    }

    impl FastembedProvider {
        pub fn open(cache_dir: &Path) -> Result<Self, EmbedError> {
            let options = fastembed::TextInitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
                .with_cache_dir(cache_dir.to_path_buf())
                .with_show_download_progress(false);
            let model = fastembed::TextEmbedding::try_new(options)
                .map_err(|err| EmbedError::Provider(format!("fastembed init failed: {err}")))?;
            Ok(Self {
                model: Mutex::new(model),
            })
        }
    }

    impl EmbeddingProvider for FastembedProvider {
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
            let mut model = self
                .model
                .lock()
                .map_err(|_| EmbedError::Provider("fastembed model mutex poisoned".to_owned()))?;
            let embeddings = model
                .embed(texts, None)
                .map_err(|err| EmbedError::Provider(format!("fastembed embed failed: {err}")))?;
            Ok(embeddings)
        }

        fn dim(&self) -> usize {
            BGE_SMALL_DIM
        }

        fn id(&self) -> &str {
            MODEL_ID
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        #[ignore = "downloads/loads a real ONNX model; run explicitly, not in CI"]
        fn embeds_real_text_with_correct_dimension() {
            let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
            let provider =
                FastembedProvider::open(dir.path()).unwrap_or_else(|e| panic!("open: {e}"));
            let vectors = provider
                .embed(&["hello world", "user login check"])
                .unwrap_or_else(|e| panic!("embed: {e}"));
            assert_eq!(vectors.len(), 2);
            for vector in &vectors {
                assert_eq!(vector.len(), BGE_SMALL_DIM);
            }
        }
    }
}

#[cfg(feature = "local-embeddings")]
pub use fastembed_provider::FastembedProvider;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_embedder_is_deterministic() {
        let embedder = MockEmbedder::new(16);
        let a = embedder
            .embed(&["hello world"])
            .unwrap_or_else(|e| panic!("{e}"));
        let b = embedder
            .embed(&["hello world"])
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a, b);
        assert_eq!(a[0].len(), 16);
    }

    #[test]
    fn mock_embedder_differs_across_distinct_inputs() {
        let embedder = MockEmbedder::new(16);
        let vectors = embedder
            .embed(&["hello world", "totally different text"])
            .unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(vectors[0], vectors[1]);
    }

    #[test]
    fn mock_embedder_override_wins_over_hash() {
        let engineered = vec![1.0, 0.0, 0.0, 0.0];
        let embedder = MockEmbedder::new(4).with_override("user login check", engineered.clone());
        let vectors = embedder
            .embed(&["user login check"])
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(vectors[0], engineered);
    }

    #[test]
    fn embedder_key_captures_id_and_dim() {
        let embedder = MockEmbedder::new(8).with_id("custom-mock");
        let key = EmbedderKey::of(&embedder);
        assert_eq!(key.id, "custom-mock");
        assert_eq!(key.dim, 8);
    }

    #[test]
    fn resolve_embedder_defaults_to_none() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let mode = std::env::var(EMBEDDINGS_MODE_ENV).unwrap_or_default();
        if mode.is_empty() || mode.eq_ignore_ascii_case("off") {
            let resolved = resolve_embedder(dir.path()).unwrap_or_else(|e| panic!("resolve: {e}"));
            assert!(resolved.is_none());
        }
    }

    #[test]
    fn remote_embedder_id_includes_model() {
        let embedder = RemoteEmbedder::new("http://example.invalid/v1", "sk-test", "m", 8);
        assert_eq!(embedder.dim(), 8);
        assert_eq!(embedder.id(), "remote/m");
    }
}
