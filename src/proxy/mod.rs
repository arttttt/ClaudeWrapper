pub mod health;
pub mod router;
pub mod upstream;

use std::net::SocketAddr;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Request;
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use tokio::net::TcpListener;

use crate::proxy::router::RouterEngine;

pub struct ProxyServer {
    pub addr: SocketAddr,
    router: RouterEngine,
}

impl ProxyServer {
    pub fn new() -> Self {
        let addr = "127.0.0.1:8080".parse().expect("Invalid bind address");
        Self {
            addr,
            router: RouterEngine::new(),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.addr).await?;
        println!("Proxy server listening on {}", self.addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let router = self.router.clone();

            tokio::task::spawn(async move {
                let service = service_fn(move |req: Request<Incoming>| {
                    let router = router.clone();
                    async move {
                        router.route(req).await
                            .map(|resp| {
                                resp.map(|body| {
                                    Full::new(body)
                                })
                            })
                    }
                });

                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    }
}

impl Default for ProxyServer {
    fn default() -> Self {
        Self::new()
    }
}
