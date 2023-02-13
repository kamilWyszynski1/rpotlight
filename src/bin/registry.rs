use rpot::cli;
use rpot::communication;
use rpot::communication::discoverer_client;
use rpot::registry;
use rpot::twoway;
use std::path::Path;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::{mpsc, Mutex};
use tonic::transport::Server;
use tracing::error;

fn create_or_load_db<P: AsRef<Path>>(p: P) -> anyhow::Result<registry::DB> {
    let tree = sled::open(p)?;
    Ok(Arc::new(Mutex::new(tree)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let db = create_or_load_db("db")?;

    let cli_addr = "[::1]:50053".parse()?;

    let (tx, rx) = mpsc::channel(64);
    let (cli_tx, cli_rx) = twoway::channel(100);
    let mut registry = registry::Registry::new(rx, cli_rx, db.clone()).await?;

    let file_provider = registry::FileInfoProvider::new(db.clone(), tx).await?;

    // spawn task that will read files
    tokio::spawn(async move {
        if let Err(err) = file_provider
            .read_and_send_files("/home/kamil/programming/rust/rpotlight")
            .await
        {
            error!(err = err.to_string(), "read_and-send_files failed");
        }
    });

    // spawn task that will receive cli rpc calls
    tokio::spawn(async move {
        if let Err(err) = Server::builder()
            .add_service(communication::cli_server::CliServer::new(
                cli::CliServer::new(cli_tx),
            ))
            .serve(cli_addr)
            .await
        {
            error!(err = err.to_string(), "cli server failed");
        }
    });

    let discoverer_client =
        communication::discoverer_client::DiscovererClient::connect("http://[::1]:50059").await?;

    registry.fetch_parsers(discoverer_client).await;
    registry.start_receiving().await?;

    tokio::select! {
        _ = signal::ctrl_c() => {},
    }

    Ok(())
}
