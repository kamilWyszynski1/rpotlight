use rpot::communication;
use rpot::discoverer::LocalDiscoverer;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let addr = "[::1]:50059".parse()?;

    let ld = LocalDiscoverer::default();
    ld.keep_parsers_in_sync().await;

    let server_thread = tokio::spawn(async move {
        Server::builder()
            .add_service(communication::discoverer_server::DiscovererServer::new(ld))
            .serve(addr)
            .await
            .unwrap();
    });

    server_thread.await?;
    Ok(())
}
