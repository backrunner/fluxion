use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    fluxion_cli::daemon_runtime::run().await
}
