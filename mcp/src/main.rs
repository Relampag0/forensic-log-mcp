mod engine;
mod parsers;
mod tools;

use rmcp::ServiceExt;
use tokio::io::{stdin, stdout};
use tracing_subscriber::{self, EnvFilter};

use tools::LogForensicsServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to stderr (so it doesn't interfere with MCP stdio)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting Forensic Log MCP Server");

    // Create the server instance
    let server = LogForensicsServer::new();

    // Serve over stdio
    let service = server.serve((stdin(), stdout())).await?;

    // Wait for the service to complete
    service.waiting().await?;

    Ok(())
}
