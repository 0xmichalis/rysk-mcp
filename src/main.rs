//! Entry point: build the Rysk client and serve the MCP server over stdio.
//!
//! Optional environment variables (also loaded from a `.env` file in the working
//! directory):
//!   RYSK_ADDRESS — wallet that `rysk_positions` reads when the caller names none.

use rmcp::ServiceExt;
use rmcp::transport::stdio;
use rysk_mcp::mcp::RyskServer;
use rysk_mcp::rysk::RyskClient;
use rysk_mcp::rysk::transport::NetTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load a .env file if present; real environment variables take precedence.
    dotenvy::dotenv().ok();

    let default_address = std::env::var("RYSK_ADDRESS")
        .ok()
        .filter(|address| !address.trim().is_empty());
    let server = RyskServer::new(RyskClient::new(NetTransport::new()?), default_address);

    // stdout carries the MCP protocol; keep it clean (no logging to stdout).
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
