use crate::config::Config;
use redis::aio::ConnectionManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone)]
pub struct AppState(pub Arc<Inner>);

pub struct Inner {
    pub config: Config,
    pub http: reqwest::Client,
    pub redis: ConnectionManager,
    /// Debounce map for `/containers/{name}/touch` keep-alive pings (browser sessions).
    pub touch_debounce: Mutex<HashMap<String, Instant>>,
}

impl std::ops::Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let redis_client = redis::Client::open(config.redis_url.clone())?;
        let redis = ConnectionManager::new(redis_client).await?;
        Ok(Self(Arc::new(Inner {
            config,
            http,
            redis,
            touch_debounce: Mutex::new(HashMap::new()),
        })))
    }
}
