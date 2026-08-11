//! HTTP server stub.

pub struct HttpServer;

impl HttpServer {
    pub fn new(_port: u16) -> Self { Self }

    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M11: implement HTTP server")
    }
}
