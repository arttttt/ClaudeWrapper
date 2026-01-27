pub struct UpstreamPool;

impl UpstreamPool {
    pub fn new() -> Self {
        // TODO: Build connection pool for upstream backends.
        todo!("implement upstream pool")
    }
}

impl Default for UpstreamPool {
    fn default() -> Self {
        Self::new()
    }
}
