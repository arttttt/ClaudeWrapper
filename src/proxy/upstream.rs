use hyper::{Request, Response, StatusCode};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HOST, CONTENT_TYPE};
use http_body_util::{BodyExt, Full, combinators::UnsyncBoxBody};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use anyhow::Result;

use crate::config::build_auth_header;
use crate::config::Config;

pub struct UpstreamClient {
    client: Client<HttpConnector, Full<Bytes>>,
}

impl UpstreamClient {
    pub fn new() -> Self {
        let connector = HttpConnector::new();
        let client = Client::builder(TokioExecutor::new()).build(connector);

        Self {
            client,
        }
    }

    pub async fn forward(&self, req: Request<Incoming>, config: Config) -> Result<Response<UnsyncBoxBody<Bytes, hyper::Error>>> {
        // NOTE: Design decision - Config clone per request
        // We clone full Config on each request instead of using lock-free Arc<Backend> swap.
        // Rationale:
        // - Config is small (~1KB, few backends), clone cost ~100ns
        // - Current load is ~10 RPS, clone overhead is negligible vs API latency (~100-500ms)
        // - Simpler code, fewer race condition risks
        // TODO: If load grows to 1000+ RPS, consider atomic Arc<Backend> swap for lock-free reads

        let method = req.method().clone();
        let uri = req.uri();
        let path_and_query = uri.path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");

        let active_backend_name = &config.defaults.active;
        let backend = config.backends
            .iter()
            .find(|b| &b.name == active_backend_name)
            .ok_or_else(|| anyhow::anyhow!("Active backend '{}' not found in configuration", active_backend_name))?;

        let upstream_uri = format!("{}{}", backend.base_url, path_and_query);

        let mut builder = Request::builder()
            .method(method)
            .uri(upstream_uri);

        for (name, value) in req.headers() {
            if name != HOST {
                builder = builder.header(name, value);
            }
        }

        if let Some((name, value)) = build_auth_header(backend) {
            builder = builder.header(&name, value);
        }

        let body_bytes = req.into_body().collect().await?.to_bytes();

        let upstream_req = builder.body(Full::new(body_bytes))
            .map_err(|e| anyhow::anyhow!("Failed to build upstream request: {}", e))?;

        let upstream_resp = match self.client.request(upstream_req).await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Failed to forward request to backend '{}': {}", backend.name, e);
                let error_body = Full::new(Bytes::from(format!(
                    "Bad Gateway: Failed to reach backend '{}': {}",
                    backend.name, e
                )));
                let mut builder = hyper::Response::builder().status(StatusCode::BAD_GATEWAY);
                builder = builder.header(CONTENT_TYPE, "text/plain");
                return Ok(builder
                    .body(error_body.map_err(|never: std::convert::Infallible| match never {}).boxed_unsync())?);
            }
        };

        let content_type = upstream_resp.headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());

        let is_streaming = content_type.map_or(false, |ct| ct.contains("text/event-stream"));

        let status = upstream_resp.status();
        let mut builder = hyper::Response::builder().status(status);

        for (name, value) in upstream_resp.headers() {
            builder = builder.header(name, value);
        }

        if is_streaming {
            Ok(builder.body(upstream_resp.into_body().boxed_unsync())?)
        } else {
            let body_bytes = upstream_resp.into_body().collect().await?.to_bytes();
            Ok(builder.body(
                Full::new(body_bytes)
                    .map_err(|never: std::convert::Infallible| match never {})
                    .boxed_unsync()
            )?)
        }
    }
}

impl Default for UpstreamClient {
    fn default() -> Self {
        Self::new()
    }
}
