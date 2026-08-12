//! HTTP server stub.

pub struct HttpServer;

impl HttpServer {
    #[must_use]
    pub fn new(_port: u16) -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M11: implement HTTP server")
    }
}
