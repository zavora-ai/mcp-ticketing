use mcp_ticketing::server::TicketingServer;
use mcp_ticketing::store::TicketingStore;
use rmcp::{ServiceExt, transport::stdio};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap()),
        )
        .init();
    let store = Arc::new(TicketingStore::new());
    let server = TicketingServer { store };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
