use async_trait::async_trait;
use langchain_rust::embedding::{
    embedder_trait::Embedder, ollama::OllamaEmbedder as LcOllamaEmbedder, EmbedderError,
};
use ollama_rs::Ollama;
use std::sync::Arc;
use url::Url;

/// Ollama-backed embedder used by the hivemind service.
#[derive(Clone)]
pub struct HivemindEmbedder {
    inner: Arc<LcOllamaEmbedder>,
}

impl HivemindEmbedder {
    /// Create an Ollama-based embedder.
    pub fn new_ollama(base_url: &str, model: &str) -> Self {
        let url = Url::parse(base_url).unwrap_or_else(|_| {
            Url::parse(&format!("http://{}", base_url)).expect("invalid Ollama URL")
        });
        let host = url.host_str().unwrap_or("localhost");
        let port = url.port().unwrap_or(11434);
        let full_url = format!("http://{}:{}", host, port);
        let client = Ollama::try_new(full_url).expect("failed to create Ollama client");
        Self {
            inner: Arc::new(LcOllamaEmbedder::new(Arc::new(client), model, None)),
        }
    }
}

#[async_trait]
impl Embedder for HivemindEmbedder {
    async fn embed_documents(&self, documents: &[String]) -> Result<Vec<Vec<f64>>, EmbedderError> {
        self.inner.embed_documents(documents).await
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f64>, EmbedderError> {
        self.inner.embed_query(text).await
    }
}
