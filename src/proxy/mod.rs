pub mod router;
pub mod upstream;

pub struct ProxyServer;

impl ProxyServer {
    pub fn new() -> Self {
        // TODO: Configure Hyper server and routing pipeline.
        todo!("implement proxy server initialization")
    }
}
