use rpot::cli;
use rpot::communication;
use rpot::db;
use rpot::registry;
use rpot::registry::Parsers;
use rpot::twoway;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::mpsc;
use tonic::transport::Server;
use tracing::error;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let config = db::Config {
        app_name: Some("discoverer"),
        database: "discoverer",
        host: "localhost",
        password: "rpotlight",
        username: "rpotlight",
        port: 27017,
    };
    let conn = db::conn(config).await?;

    let cli_addr = "[::1]:50053".parse()?;

    let (tx, rx) = mpsc::channel(64);
    let (cli_tx, cli_rx) = twoway::channel(100);

    let parsers: Parsers = Arc::default();
    let mut registry =
        registry::Registry::new(rx, parsers.clone(), cli_rx, conn.collection("content")).await?;

    let file_provider = registry::FileInfoProvider::new(conn.collection("parsed"), tx).await?;

    // spawn task that will read files
    tokio::spawn(async move {
        if let Err(err) = file_provider
            .read_and_send_files(parsers.clone(), "/home/kamil/programming/rust/rpotlight")
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
