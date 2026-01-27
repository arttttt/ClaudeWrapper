use hyper::{Request, Response, Method};
use hyper::body::Incoming;
use hyper::body::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use std::sync::Arc;
use anyhow::Result;

use crate::config::ConfigStore;
use crate::proxy::health::HealthHandler;
use crate::proxy::upstream::UpstreamClient;

#[derive(Clone)]
pub struct RouterEngine {
    health: Arc<HealthHandler>,
    upstream: Arc<UpstreamClient>,
    config: ConfigStore,
}

impl RouterEngine {
    pub fn new(config: ConfigStore) -> Self {
        Self {
            health: Arc::new(HealthHandler::new()),
            upstream: Arc::new(UpstreamClient::new()),
            config,
        }
    }

    pub async fn route(&self, req: Request<Incoming>) -> Result<Response<UnsyncBoxBody<Bytes, hyper::Error>>> {
        tracing::debug!(method = %req.method(), path = %req.uri().path(), "Incoming request");
        let path = req.uri().path();

        match (req.method(), path) {
            (&Method::GET, "/health") => self.health.handle().await,
            _ => self.upstream.forward(req, self.config.get()).await,
        }
    }
}
