use crate::config;

pub struct AppContext {
    pub server: Option<String>,
    pub json: bool,
}

impl AppContext {
    pub fn new(server: Option<String>, json: bool) -> Self {
        Self { server, json }
    }

    pub fn base_url(&self) -> String {
        config::resolve_server_url(self.server.as_deref())
    }
}

pub fn log_filter(verbose: u8) -> tracing_subscriber::EnvFilter {
    let filter = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };

    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| filter.into())
}